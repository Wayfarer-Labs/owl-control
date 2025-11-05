use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use color_eyre::Result;
use constants::encoding::VideoEncoderType;
use egui_commonmark::CommonMarkCache;
use egui_wgpu::wgpu;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

use crate::{
    app_state::{
        AppState, AsyncRequest, GitHubRelease, HotkeyRebindTarget, ListeningForNewHotkey, UiUpdate,
        UiUpdateUnreliable,
    },
    assets,
    config::{Credentials, Preferences},
    system::keycode::virtual_keycode_to_name,
};

mod internal;
mod overlay;
pub mod tray_icon;
mod util;

mod views;

pub mod notification;

/// Optimized to show everything in the layout at 1x scaling.
///
/// Update this whenever you add or remove content. Assume that everything that a normal user
/// might see should be covered by this size (e.g. no temporary notices, but yes "delete invalid" button)
///
/// Try to keep this below ~840px ((1080/1.25 = 864) - 24px taskbar)).
const WINDOW_INNER_SIZE: PhysicalSize<u32> = PhysicalSize::new(600, 825);

pub fn start(
    wgpu_instance: wgpu::Instance,
    app_state: Arc<AppState>,
    ui_update_rx: tokio::sync::mpsc::UnboundedReceiver<UiUpdate>,
    ui_update_unreliable_rx: tokio::sync::broadcast::Receiver<UiUpdateUnreliable>,
    stopped_tx: tokio::sync::broadcast::Sender<()>,
    stopped_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    let tray_icon = tray_icon::TrayIconState::new()?;
    let visible = Arc::new(AtomicBool::new(true));

    // launch overlay on separate thread so non-blocking
    std::thread::spawn({
        let app_state = app_state.clone();
        let stopped_rx = stopped_rx.resubscribe();
        move || {
            egui_overlay::start(overlay::OverlayApp::new(app_state, stopped_rx));
        }
    });

    let event_loop = EventLoop::new().unwrap();
    // setting controlflow::wait is important. This means that once minimized to tray,
    // unlike eframe, it will no longer poll for updates - massively saving CPU.
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);

    let mut app = App::new(
        wgpu_instance,
        app_state,
        visible,
        stopped_rx,
        stopped_tx,
        ui_update_rx,
        ui_update_unreliable_rx,
        tray_icon,
    )?;

    event_loop.run_app(&mut app).unwrap();

    Ok(())
}

struct App {
    instance: wgpu::Instance,
    wgpu_state: Option<internal::WgpuState>,
    window: Option<Arc<Window>>,
    main_app: MainApp,
    last_repaint_requested: Instant,
}

impl App {
    #[allow(clippy::too_many_arguments)]
    fn new(
        wgpu_instance: wgpu::Instance,
        app_state: Arc<AppState>,
        visible: Arc<AtomicBool>,
        stopped_rx: tokio::sync::broadcast::Receiver<()>,
        stopped_tx: tokio::sync::broadcast::Sender<()>,
        ui_update_rx: tokio::sync::mpsc::UnboundedReceiver<UiUpdate>,
        ui_update_unreliable_rx: tokio::sync::broadcast::Receiver<UiUpdateUnreliable>,
        tray_icon: tray_icon::TrayIconState,
    ) -> Result<Self> {
        let main_app = MainApp::new(
            app_state,
            visible,
            stopped_rx,
            stopped_tx,
            ui_update_rx,
            ui_update_unreliable_rx,
            tray_icon,
        )?;

        Ok(Self {
            instance: wgpu_instance,
            wgpu_state: None,
            window: None,
            main_app,
            last_repaint_requested: Instant::now(),
        })
    }

    async fn set_window(&mut self, window: Window, inner_size: PhysicalSize<u32>) {
        let window = Arc::new(window);
        let _ = window.request_inner_size(inner_size);

        let surface = self
            .instance
            .create_surface(window.clone())
            .expect("Failed to create surface!");

        let state = internal::WgpuState::new(
            &self.instance,
            surface,
            &window,
            inner_size.width,
            inner_size.height,
        )
        .await;

        self.window.get_or_insert(window);
        self.wgpu_state.get_or_insert(state);
    }

    fn handle_resized(&mut self, width: u32, height: u32) {
        if width > 0
            && height > 0
            && let Some(state) = self.wgpu_state.as_mut()
        {
            state.resize_surface(width, height);
        }
    }

    fn handle_redraw(&mut self) {
        // Attempt to handle minimizing window
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.is_minimized().is_some_and(|v| v) {
            return;
        }
        let Some(state) = self.wgpu_state.as_mut() else {
            return;
        };

        state.render(window, |ctx| self.main_app.render(ctx));
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // Load window icon for taskbar
        let (icon_rgb, (icon_width, icon_height)) =
            assets::load_icon_data_from_bytes(assets::get_logo_default_bytes());
        let window_icon = winit::window::Icon::from_rgba(icon_rgb, icon_width, icon_height)
            .expect("Failed to create window icon");

        let inner_size = WINDOW_INNER_SIZE;
        let window_attributes = Window::default_attributes()
            .with_title(format!("OWL Control v{}", env!("CARGO_PKG_VERSION")))
            .with_inner_size(inner_size)
            .with_min_inner_size(PhysicalSize::new(400, 450))
            .with_resizable(true)
            .with_window_icon(Some(window_icon));

        let window = event_loop.create_window(window_attributes).unwrap();

        // Now that we have the scale factor, we can multiply the inner size by it
        // to ensure that the user will see the content at their DPI scaling.
        let scale_factor = window.scale_factor();
        let inner_size = PhysicalSize::new(
            (inner_size.width as f64 * scale_factor) as u32,
            (inner_size.height as f64 * scale_factor) as u32,
        );

        // Block on async initialization
        futures::executor::block_on(self.set_window(window, inner_size));

        // Initialize tray icon and egui context after window is created
        let Some(ctx) = self.wgpu_state.as_ref().map(|state| state.context()) else {
            return;
        };
        let _ = self.main_app.app_state.ui_update_tx.ctx.set(ctx.clone());

        self.main_app.tray_icon.post_initialize(
            ctx.clone(),
            self.window.clone().unwrap(),
            self.main_app.visible.clone(),
            self.main_app.stopped_tx.clone(),
            self.main_app.app_state.ui_update_tx.clone(),
        );

        catppuccin_egui::set_theme(ctx, catppuccin_egui::MACCHIATO);

        ctx.style_mut(|style| {
            let bg_color = egui::Color32::from_rgb(19, 21, 26);
            style.visuals.window_fill = bg_color;
            style.visuals.panel_fill = bg_color;
        });

        if let Some(window) = self.window.clone() {
            ctx.set_request_repaint_callback(move |_info| {
                // We just ignore the delay for now
                window.request_redraw();
            })
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(state) = self.wgpu_state.as_mut() else {
            return;
        };

        // Let egui renderer process the event first
        let response = state
            .renderer()
            .handle_input(self.window.as_ref().unwrap(), &event);

        // We throttle this so we aren't unnecessarily repainting for what is otherwise a relatively
        // simple UI. 16ms ~= 60fps.
        if response.repaint && self.last_repaint_requested.elapsed() > Duration::from_millis(16) {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            self.last_repaint_requested = Instant::now();
        }

        // Handle window events
        self.main_app
            .handle_window_event(event_loop, &event, state.context());

        match event {
            WindowEvent::CloseRequested => {
                if self.main_app.should_close() {
                    tracing::info!("Closing Winit App handler");
                    event_loop.exit();
                } else {
                    // Minimize to tray
                    if let Some(window) = self.window.as_ref() {
                        window.set_visible(false);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.handle_redraw();
            }
            WindowEvent::Resized(new_size) => {
                self.handle_resized(new_size.width, new_size.height);
            }
            _ => (),
        }
    }
}

const HEADING_TEXT_SIZE: f32 = 24.0;
const SUBHEADING_TEXT_SIZE: f32 = 16.0;

pub struct MainApp {
    app_state: Arc<AppState>,
    frame: u64,
    /// Receives commands from various tx in other threads to perform some UI update
    ui_update_rx: tokio::sync::mpsc::UnboundedReceiver<UiUpdate>,
    /// Receives commands from various tx in other threads to perform some UI update
    /// that don't need to be processed immediately.
    ui_update_unreliable_rx: tokio::sync::broadcast::Receiver<UiUpdateUnreliable>,

    /// Available video encoders, updated from tokio thread via mpsc channel
    available_video_encoders: Vec<VideoEncoderType>,

    login_api_key: String,
    is_authenticating_login_api_key: bool,
    authenticated_user_id: Option<Result<String, String>>,
    has_scrolled_to_bottom_of_consent: bool,

    /// Local copy of credentials, used to track UI state before saving to config
    local_credentials: Credentials,
    /// Local copy of preferences, used to track UI state before saving to config
    local_preferences: Preferences,
    /// Time since last requested config edit: we only attempt to save once enough time has passed
    config_last_edit: Option<Instant>,

    /// A newer release is available, updated from tokio thread via mpsc channel
    newer_release_available: Option<GitHubRelease>,

    md_cache: CommonMarkCache,
    visible: Arc<AtomicBool>,
    stopped_rx: tokio::sync::broadcast::Receiver<()>,
    stopped_tx: tokio::sync::broadcast::Sender<()>,
    has_stopped: bool,

    main_view_state: views::main::MainViewState,

    tray_icon: tray_icon::TrayIconState,

    /// Whether the encoder settings window is open
    encoder_settings_window_open: bool,
}
impl MainApp {
    fn new(
        app_state: Arc<AppState>,
        visible: Arc<AtomicBool>,
        stopped_rx: tokio::sync::broadcast::Receiver<()>,
        stopped_tx: tokio::sync::broadcast::Sender<()>,
        ui_update_rx: tokio::sync::mpsc::UnboundedReceiver<UiUpdate>,
        ui_update_unreliable_rx: tokio::sync::broadcast::Receiver<UiUpdateUnreliable>,
        tray_icon: tray_icon::TrayIconState,
    ) -> Result<Self> {
        let (local_credentials, local_preferences) = {
            let configs = app_state.config.read().unwrap();
            (configs.credentials.clone(), configs.preferences.clone())
        };

        // If we're fully authenticated, submit a request to validate our existing API key
        if !local_credentials.api_key.is_empty() && local_credentials.has_consented {
            app_state
                .async_request_tx
                .blocking_send(AsyncRequest::ValidateApiKey {
                    api_key: local_credentials.api_key.clone(),
                })
                .ok();
        }

        Ok(Self {
            app_state,
            frame: 0,
            ui_update_rx,
            ui_update_unreliable_rx,

            login_api_key: local_credentials.api_key.clone(),
            is_authenticating_login_api_key: false,
            authenticated_user_id: None,
            has_scrolled_to_bottom_of_consent: false,

            available_video_encoders: vec![],

            local_credentials,
            local_preferences,
            config_last_edit: None,

            newer_release_available: None,

            md_cache: CommonMarkCache::default(),
            visible,
            stopped_rx,
            stopped_tx,
            has_stopped: false,

            main_view_state: views::main::MainViewState::default(),

            tray_icon,

            encoder_settings_window_open: false,
        })
    }

    fn should_close(&self) -> bool {
        self.has_stopped
    }

    fn handle_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        event: &WindowEvent,
        ctx: &egui::Context,
    ) {
        loop {
            match self.ui_update_rx.try_recv() {
                Ok(UiUpdate::ForceUpdate) => {
                    ctx.request_repaint();
                }
                Ok(UiUpdate::UpdateAvailableVideoEncoders(encoders)) => {
                    self.available_video_encoders = encoders;
                }
                Ok(UiUpdate::UpdateUserId(uid)) => {
                    let was_successful = uid.is_ok();
                    self.authenticated_user_id = Some(uid);
                    self.is_authenticating_login_api_key = false;
                    if was_successful && !self.local_credentials.has_consented {
                        self.go_to_consent();
                    }
                }
                Ok(UiUpdate::UploadFailed(error)) => {
                    self.main_view_state
                        .upload_manager
                        .update_last_upload_error(Some(error));
                }
                Ok(UiUpdate::UpdateTrayIconRecording(recording)) => {
                    self.tray_icon.set_icon_recording(recording);
                }
                Ok(UiUpdate::UpdateNewerReleaseAvailable(release)) => {
                    self.newer_release_available = Some(release);
                }
                Ok(UiUpdate::UpdateUserUploads(uploads)) => {
                    self.main_view_state
                        .upload_manager
                        .update_user_uploads(uploads.uploads);
                }
                Ok(UiUpdate::UpdateLocalRecordings(recordings)) => {
                    self.main_view_state
                        .upload_manager
                        .update_local_recordings(recordings);
                }
                Ok(UiUpdate::FolderPickerResult { old_path, new_path }) => {
                    // Check if there are any recordings in the old location
                    if old_path.exists()
                        && std::fs::read_dir(&old_path).is_ok_and(|dir| {
                            dir.filter_map(Result::ok)
                                .any(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                        })
                        && old_path != new_path
                    {
                        // Show confirmation dialog to ask about moving files
                        self.main_view_state.pending_move_location = Some((old_path, new_path));
                    } else {
                        // No recordings to move, just update the location
                        self.local_preferences.recording_location = new_path;
                    }
                }
                Err(_) => {
                    break;
                }
            };
        }

        loop {
            match self.ui_update_unreliable_rx.try_recv() {
                Ok(UiUpdateUnreliable::UpdateUploadProgress(progress_data)) => {
                    self.main_view_state
                        .upload_manager
                        .update_current_upload_progress(progress_data);
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                    tracing::warn!("UiUpdateUnreliable channel lagged, dropping message");
                }
                Err(
                    tokio::sync::broadcast::error::TryRecvError::Empty
                    | tokio::sync::broadcast::error::TryRecvError::Closed,
                ) => {
                    break;
                }
            }
        }

        if self.stopped_rx.try_recv().is_ok() {
            tracing::info!("MainApp received stop signal");
            self.has_stopped = true;
            event_loop.exit();
            return;
        }

        // if user closes the app instead minimize to tray
        if matches!(event, WindowEvent::CloseRequested) && !self.has_stopped {
            self.visible.store(false, Ordering::Relaxed);
            // we handle visibility in the App level
        }

        // Handle hotkey rebinds
        let listening_for_new_hotkey = *self.app_state.listening_for_new_hotkey.read().unwrap();
        if let ListeningForNewHotkey::Captured { target, key } = listening_for_new_hotkey {
            if let Some(key_name) = virtual_keycode_to_name(key) {
                let rebind_target = match target {
                    HotkeyRebindTarget::Start => &mut self.local_preferences.start_recording_key,
                    HotkeyRebindTarget::Stop => &mut self.local_preferences.stop_recording_key,
                };
                *rebind_target = key_name.to_string();

                *self.app_state.listening_for_new_hotkey.write().unwrap() =
                    ListeningForNewHotkey::NotListening;
            } else {
                // Invalid hotkey? Try again
                *self.app_state.listening_for_new_hotkey.write().unwrap() =
                    ListeningForNewHotkey::Listening { target };
            }
        }
    }

    fn render(&mut self, ctx: &egui::Context) {
        let (has_api_key, has_consented) = (
            !self.local_credentials.api_key.is_empty(),
            self.local_credentials.has_consented,
        );

        match (has_api_key, has_consented) {
            (true, true) => self.main_view(ctx),
            (true, false) => self.consent_view(ctx),
            (false, _) => self.login_view(ctx),
        }

        // Queue up a save if any state has changed
        {
            let mut config = self.app_state.config.write().unwrap();
            let mut requires_save = false;
            if config.credentials != self.local_credentials {
                config.credentials = self.local_credentials.clone();
                requires_save = true;
            }
            if config.preferences != self.local_preferences {
                config.preferences = self.local_preferences.clone();
                requires_save = true;
            }
            if requires_save {
                self.config_last_edit = Some(Instant::now());
            }
        }

        if self
            .config_last_edit
            .is_some_and(|t| t.elapsed() > Duration::from_millis(250))
        {
            let _ = self.app_state.config.read().unwrap().save();
            self.config_last_edit = None;
        }

        self.frame += 1;
    }
}
impl MainApp {
    fn go_to_login(&mut self) {
        self.local_credentials.logout();
        self.authenticated_user_id = None;
        self.is_authenticating_login_api_key = false;
    }

    fn go_to_consent(&mut self) {
        self.local_credentials.api_key = self.login_api_key.clone();
        self.local_credentials.has_consented = false;
        self.has_scrolled_to_bottom_of_consent = false;
    }

    fn go_to_main(&mut self) {
        self.local_credentials.has_consented = true;
    }
}
