use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tray_icon::{
    MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWDEFAULT, ShowWindow},
};
use winit::raw_window_handle::Win32WindowHandle;

const LOGO_DEFAULT_BYTES: &[u8] = include_bytes!("../../assets/owl-logo.png");
const LOGO_RECORDING_BYTES: &[u8] = include_bytes!("../../assets/owl-logo-recording.png");

pub fn egui_icon() -> egui::IconData {
    load_egui_icon_from_bytes(LOGO_DEFAULT_BYTES)
}

/// Initial creation of tray icon
pub fn initialize() -> tray_icon::Result<TrayIcon> {
    // tray icon right click menu for quit option
    let quit_item = MenuItem::new("Quit", true, None);
    let quit_item_id = quit_item.id().clone();
    let tray_menu = Menu::new();
    let _ = tray_menu.append(&quit_item);

    // create tray icon
    let (rgba, (width, height)) = load_icon_data_from_bytes(LOGO_DEFAULT_BYTES);
    let tray_icon_data =
        tray_icon::Icon::from_rgba(rgba, width, height).expect("Failed to create tray icon");

    let tray_icon = TrayIconBuilder::new()
        .with_icon(tray_icon_data)
        .with_tooltip("OWL Control")
        .with_menu(Box::new(tray_menu))
        .build()?;

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        match event.id() {
            id if id == &quit_item_id => {
                // Close the application
                // TODO: probably should be a more graceful way to close the app?
                // probably need a atomicbool with close flag so we can close via
                // context_menu.send_viewport_cmd(egui::ViewportCommand::Close);
                // and then also clean up any uploading video process in progress.
                std::process::exit(0);
            }
            _ => {}
        }
    }));

    Ok(tray_icon)
}

pub fn post_initialize(
    context: egui::Context,
    window_handle: Win32WindowHandle,
    visible: Arc<AtomicBool>,
) {
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        let hwnd = HWND(window_handle.hwnd.get() as *mut std::ffi::c_void);

        if let TrayIconEvent::Click {
            button: tray_icon::MouseButton::Left,
            button_state: MouseButtonState::Down,
            ..
        } = event
        {
            if visible.load(Ordering::Relaxed) {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                visible.store(false, Ordering::Relaxed);
            } else {
                // set viewport visible true in case it was minimised to tray via closing the app
                context.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
                }
                visible.store(true, Ordering::Relaxed);
            }
            context.request_repaint();
        }
    }));
}

pub fn set_icon_recording(recording: bool) {
    set_app_icon_windows(&load_egui_icon_from_bytes(if recording {
        LOGO_RECORDING_BYTES
    } else {
        LOGO_DEFAULT_BYTES
    }));
}

fn load_egui_icon_from_bytes(bytes: &[u8]) -> egui::IconData {
    let (rgba, (width, height)) = load_icon_data_from_bytes(bytes);
    egui::IconData {
        rgba,
        width,
        height,
    }
}

fn load_icon_data_from_bytes(bytes: &[u8]) -> (Vec<u8>, (u32, u32)) {
    let image = image::load_from_memory(bytes)
        .expect("Failed to load embedded icon")
        .into_rgba8();
    let dimensions = image.dimensions();
    let rgba = image.into_raw();
    (rgba, dimensions)
}

// shamelessly stolen from https://github.com/emilk/egui/blob/a5973e5cac461a23c853cb174b28c8e9317ecce6/crates/eframe/src/native/app_icon.rs#L78-L99

/// In which state the app icon is (as far as we know).
#[derive(PartialEq, Eq)]
pub enum AppIconStatus {
    /// We did not set it or failed to do it. In any case we won't try again.
    NotSetIgnored,

    /// We haven't set the icon yet, we should try again next frame.
    ///
    /// This can happen repeatedly due to lazy window creation on some platforms.
    NotSetTryAgain,

    /// We successfully set the icon and it should be visible now.
    #[allow(dead_code)] // Not used on Linux
    Set,
}

/// Set icon for Windows applications.
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn set_app_icon_windows(icon_data: &egui::IconData) -> AppIconStatus {
    use eframe::icon_data::IconDataExt;
    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::{
            Input::KeyboardAndMouse::GetActiveWindow,
            WindowsAndMessaging::{
                CreateIconFromResourceEx, GetSystemMetrics, HICON, ICON_BIG, ICON_SMALL,
                LR_DEFAULTCOLOR, SM_CXICON, SM_CXSMICON, SendMessageW, WM_SETICON,
            },
        },
    };

    // We would get fairly far already with winit's `set_window_icon` (which is exposed to eframe) actually!
    // However, it only sets ICON_SMALL, i.e. doesn't allow us to set a higher resolution icon for the task bar.
    // Also, there is scaling issues, detailed below.

    // TODO(andreas): This does not set the task bar icon for when our application is started from python.
    //      Things tried so far:
    //      * Querying for an owning window and setting icon there (there doesn't seem to be an owning window)
    //      * using undocumented SetConsoleIcon method (successfully queried via GetProcAddress)

    // SAFETY: WinApi function without side-effects.
    let window_handle = unsafe { GetActiveWindow() };
    if window_handle.0.is_null() {
        // The Window isn't available yet. Try again later!
        return AppIconStatus::NotSetTryAgain;
    }

    fn create_hicon_with_scale(unscaled_image: &image::RgbaImage, target_size: i32) -> HICON {
        let image_scaled = image::imageops::resize(
            unscaled_image,
            target_size as _,
            target_size as _,
            image::imageops::Lanczos3,
        );

        // Creating transparent icons with WinApi is a huge mess.
        // We'd need to go through CreateIconIndirect's ICONINFO struct which then
        // takes a mask HBITMAP and a color HBITMAP and creating each of these is pain.
        // Instead we workaround this by creating a png which CreateIconFromResourceEx magically understands.
        // This is a pretty horrible hack as we spend a lot of time encoding, but at least the code is a lot shorter.
        let mut image_scaled_bytes: Vec<u8> = Vec::new();
        if image_scaled
            .write_to(
                &mut std::io::Cursor::new(&mut image_scaled_bytes),
                image::ImageFormat::Png,
            )
            .is_err()
        {
            return HICON(std::ptr::null_mut());
        }

        // SAFETY: Creating an HICON which should be readonly on our data.
        unsafe {
            CreateIconFromResourceEx(
                &image_scaled_bytes,
                true,
                0x00030000,  // Version number of the HICON
                target_size, // Note that this method can scale, but it does so *very* poorly. So let's avoid that!
                target_size,
                LR_DEFAULTCOLOR,
            )
            .unwrap_or(HICON(std::ptr::null_mut()))
        }
    }

    let unscaled_image = match icon_data.to_image() {
        Ok(unscaled_image) => unscaled_image,
        Err(err) => {
            tracing::warn!("Invalid icon: {err}");
            return AppIconStatus::NotSetIgnored;
        }
    };

    // Only setting ICON_BIG with the icon size for big icons (SM_CXICON) works fine
    // but the scaling it does then for the small icon is pretty bad.
    // Instead we set the correct sizes manually and take over the scaling ourselves.
    // For this to work we first need to set the big icon and then the small one.
    //
    // Note that ICON_SMALL may be used even if we don't render a title bar as it may be used in alt+tab!
    {
        // SAFETY: WinAPI getter function with no known side effects.
        let icon_size_big = unsafe { GetSystemMetrics(SM_CXICON) };
        let icon_big = create_hicon_with_scale(&unscaled_image, icon_size_big);
        if icon_big.0.is_null() {
            tracing::warn!("Failed to create HICON (for big icon) from embedded png data.");
            return AppIconStatus::NotSetIgnored; // We could try independently with the small icon but what's the point, it would look bad!
        } else {
            // SAFETY: Unsafe WinApi function, takes objects previously created with WinAPI, all checked for null prior.
            unsafe {
                SendMessageW(
                    window_handle,
                    WM_SETICON,
                    Some(WPARAM(ICON_BIG as usize)),
                    Some(LPARAM(icon_big.0 as isize)),
                );
            }
        }
    }
    {
        // SAFETY: WinAPI getter function with no known side effects.
        let icon_size_small = unsafe { GetSystemMetrics(SM_CXSMICON) };
        let icon_small = create_hicon_with_scale(&unscaled_image, icon_size_small);
        if icon_small.0.is_null() {
            tracing::warn!("Failed to create HICON (for small icon) from embedded png data.");
            return AppIconStatus::NotSetIgnored;
        } else {
            // SAFETY: Unsafe WinApi function, takes objects previously created with WinAPI, all checked for null prior.
            unsafe {
                SendMessageW(
                    window_handle,
                    WM_SETICON,
                    Some(WPARAM(ICON_SMALL as usize)),
                    Some(LPARAM(icon_small.0 as isize)),
                );
            }
        }
    }

    // It _probably_ worked out.
    AppIconStatus::Set
}
