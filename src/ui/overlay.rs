use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use egui::{Align2, Color32, Stroke, TextFormat, Vec2, text::LayoutJob};
use egui_overlay::EguiOverlay;
use egui_render_three_d::ThreeDBackend as DefaultGfxBackend;
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        FLASHW_STOP, FLASHWINFO, FlashWindowEx, GWL_EXSTYLE, GetWindowLongPtrW, SW_HIDE,
        SW_SHOWDEFAULT, SetWindowLongPtrW, ShowWindow, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    },
};

use crate::{
    app_state::{AppState, RecordingStatus},
    assets::get_owl_bytes,
    config::OverlayLocation,
    system::hardware_specs::get_primary_monitor_resolution,
    ui::util,
};

pub struct OverlayApp {
    initialized: bool,
    app_state: Arc<AppState>,

    /// local overlay location
    overlay_location: OverlayLocation,
    /// local opacity tracker
    overlay_opacity: u8,
    /// local recording status
    rec_status: RecordingStatus,

    last_paint_time: Instant,
    stopped_rx: tokio::sync::broadcast::Receiver<()>,
}
impl OverlayApp {
    pub fn new(app_state: Arc<AppState>, stopped_rx: tokio::sync::broadcast::Receiver<()>) -> Self {
        let (overlay_location, overlay_opacity) = {
            let config = app_state.config.read().unwrap();
            (
                config.preferences.overlay_location,
                config.preferences.overlay_opacity,
            )
        };
        let rec_status = app_state.state.read().unwrap().clone();
        Self {
            initialized: false,
            app_state,

            overlay_location,
            overlay_opacity,
            rec_status,

            last_paint_time: Instant::now(),
            stopped_rx,
        }
    }
}
impl OverlayApp {
    fn first_frame_init(
        &mut self,
        egui_context: &egui::Context,
        glfw_backend: &mut egui_window_glfw_passthrough::GlfwBackend,
        curr_location: OverlayLocation,
        curr_opacity: u8,
    ) {
        // install image loaders
        egui_extras::install_image_loaders(egui_context);

        // don't show transparent window outline
        glfw_backend.window.set_decorated(false);
        glfw_backend.set_window_size([600.0, 50.0]);
        update_overlay_position_based_on_location(&mut glfw_backend.window, curr_location);
        // always allow input to passthrough
        glfw_backend.set_passthrough(true);

        // hide glfw overlay icon from taskbar and alt+tab
        let hwnd = glfw_backend.window.get_win32_window() as isize;
        if hwnd != 0 {
            unsafe {
                let hwnd = HWND(hwnd as *mut std::ffi::c_void);

                // https://stackoverflow.com/a/7219089
                // glfw window might bug sometimes, if user is alt tabbed / focusing another window while glfw starts up
                // hiding it from taskbar might break. so we have to do this shit per microsoft:
                // "you must hide the window first (by calling ShowWindow with SW_HIDE), change the window style, and then show the window."
                let flash_info = FLASHWINFO {
                    cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                    hwnd,
                    dwFlags: FLASHW_STOP,
                    uCount: 0,
                    dwTimeout: 0,
                };
                let _ = FlashWindowEx(&flash_info);

                let _ = ShowWindow(hwnd, SW_HIDE); // hide the window

                // set the style
                let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                ex_style |= WS_EX_TOOLWINDOW.0 as isize; // Hide from taskbar
                ex_style |= WS_EX_NOACTIVATE.0 as isize; // Don't steal focus
                ex_style &= !(WS_EX_APPWINDOW.0 as isize); // Remove from Alt+Tab
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);

                let _ = ShowWindow(hwnd, SW_SHOWDEFAULT); // show the window for the new style to come into effect
            }
        }

        // Hide window if opacity is 0
        if curr_opacity == 0 {
            self.set_window_visible(glfw_backend, false);
        }
    }

    fn set_window_visible(
        &self,
        glfw_backend: &mut egui_window_glfw_passthrough::GlfwBackend,
        visible: bool,
    ) {
        let hwnd = glfw_backend.window.get_win32_window() as isize;
        if hwnd != 0 {
            tracing::info!("Setting overlay visible: {visible}");
            unsafe {
                let _ = ShowWindow(
                    HWND(hwnd as *mut std::ffi::c_void),
                    if visible { SW_SHOWDEFAULT } else { SW_HIDE },
                );
            }
        }
    }
}
impl EguiOverlay for OverlayApp {
    fn gui_run(
        &mut self,
        egui_context: &egui::Context,
        _default_gfx_backend: &mut DefaultGfxBackend,
        glfw_backend: &mut egui_window_glfw_passthrough::GlfwBackend,
    ) {
        let (curr_opacity, curr_location) = {
            let config = self.app_state.config.read().unwrap();
            (
                config.preferences.overlay_opacity,
                config.preferences.overlay_location,
            )
        };

        // kind of cringe that we are forced to check first frame setup logic like this, but egui_overlay doesn't expose
        // any setup/init interface
        if !self.initialized {
            self.first_frame_init(egui_context, glfw_backend, curr_location, curr_opacity);
            egui_context.request_repaint();
            self.initialized = true;
        }

        if self.stopped_rx.try_recv().is_ok() {
            tracing::info!("Overlay received stop signal");
            glfw_backend.window.set_should_close(true);
            return;
        }

        if curr_opacity != self.overlay_opacity {
            self.overlay_opacity = curr_opacity;
            egui_context.request_repaint();

            self.set_window_visible(glfw_backend, curr_opacity > 0);
        }
        if curr_location != self.overlay_location {
            self.overlay_location = curr_location;
            update_overlay_position_based_on_location(&mut glfw_backend.window, curr_location);
        }
        let frame = egui::containers::Frame {
            fill: Color32::from_black_alpha(self.overlay_opacity), // Transparent background
            stroke: Stroke::NONE,                                  // No border
            corner_radius: 0.0.into(),                             // No rounded corners
            shadow: Default::default(),                            // Default shadow settings
            inner_margin: egui::Margin::same(8),                   // Inner padding
            outer_margin: egui::Margin::ZERO,                      // No outer margin
        };

        // only repaint the window every 500ms or when the recording state changes
        let curr_state = self.app_state.state.read().unwrap().clone();
        if self.last_paint_time.elapsed() > Duration::from_millis(500)
            || curr_state != self.rec_status
        {
            self.rec_status = curr_state;
            self.last_paint_time = Instant::now();
            egui_context.request_repaint();
        }
        let (align, pos) = match self.overlay_location {
            OverlayLocation::TopLeft => (Align2::LEFT_TOP, Vec2 { x: 10.0, y: 10.0 }),
            OverlayLocation::TopRight => (Align2::RIGHT_TOP, Vec2 { x: -10.0, y: 10.0 }),
            OverlayLocation::BottomLeft => (Align2::LEFT_BOTTOM, Vec2 { x: 10.0, y: -10.0 }),
            OverlayLocation::BottomRight => (Align2::RIGHT_BOTTOM, Vec2 { x: -10.0, y: -10.0 }),
        };
        egui::Window::new("recording overlay")
            .title_bar(false) // No title bar
            .resizable(false) // Non-resizable
            .scroll([false, false]) // Non-scrollable (both x and y)
            .collapsible(false) // Non-collapsible (removes collapse button)
            .anchor(align, pos)
            .auto_sized()
            .frame(frame)
            .show(egui_context, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Image::from_bytes("bytes://", get_owl_bytes())
                            .fit_to_exact_size(Vec2 { x: 24.0, y: 24.0 })
                            .tint(Color32::from_white_alpha(self.overlay_opacity)),
                    );

                    let font_id = egui::FontId::new(12.0, egui::FontFamily::Proportional);
                    let color = Color32::from_white_alpha(self.overlay_opacity);
                    let recording_text: egui::WidgetText = match &self.rec_status {
                        RecordingStatus::Stopped => egui::RichText::new("Stopped")
                            .font(font_id)
                            .color(color)
                            .into(),
                        RecordingStatus::Recording {
                            start_time,
                            game_exe,
                        } => {
                            let mut job = LayoutJob::default();
                            job.append(
                                "Recording ",
                                0.0,
                                TextFormat {
                                    font_id: font_id.clone(),
                                    color,
                                    ..Default::default()
                                },
                            );
                            job.append(
                                game_exe,
                                0.0,
                                TextFormat {
                                    font_id: font_id.clone(),
                                    italics: true,
                                    color,
                                    ..Default::default()
                                },
                            );
                            job.append(
                                &format!(
                                    " ({})",
                                    util::format_seconds(start_time.elapsed().as_secs())
                                ),
                                0.0,
                                TextFormat {
                                    font_id,
                                    color,
                                    ..Default::default()
                                },
                            );
                            job.into()
                        }
                        RecordingStatus::Paused => egui::RichText::new("Paused")
                            .font(font_id)
                            .color(color)
                            .into(),
                    };
                    ui.label(recording_text);
                });
            });
    }
}

fn update_overlay_position_based_on_location(
    window: &mut egui_window_glfw_passthrough::glfw::PWindow,
    location: OverlayLocation,
) {
    let (width, height) = window.get_size();
    let (monitor_width, monitor_height) = match get_primary_monitor_resolution() {
        Some((monitor_width, monitor_height)) => (monitor_width as i32, monitor_height as i32),
        None => {
            tracing::error!("Failed to get primary monitor resolution, using 800x600");
            (800, 600)
        }
    };
    match location {
        OverlayLocation::TopLeft => {
            window.set_pos(0, 0);
        }
        OverlayLocation::TopRight => {
            window.set_pos(monitor_width - width, 0);
        }
        OverlayLocation::BottomLeft => {
            window.set_pos(0, monitor_height - height);
        }
        OverlayLocation::BottomRight => {
            window.set_pos(monitor_width - width, monitor_height - height);
        }
    }
}
