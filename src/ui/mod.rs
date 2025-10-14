use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use color_eyre::Result;
use egui_commonmark::{CommonMarkCache, commonmark_str};

use crate::{
    app_state::{AppState, AsyncRequest, UiUpdate},
    assets,
    config::{Credentials, Preferences},
    upload,
};

use egui_tools::EguiRenderer;
use egui_wgpu::{ScreenDescriptor, wgpu};
use wgpu::SurfaceError;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

mod egui_tools;
mod overlay;
pub mod tray_icon;
mod util;

pub mod notification;

#[derive(PartialEq, Clone, Copy)]
enum HotkeyRebindTarget {
    /// Listening for start key
    Start,
    /// Listening for stop key
    Stop,
}

pub fn start(
    app_state: Arc<AppState>,
    ui_update_rx: tokio::sync::mpsc::Receiver<UiUpdate>,
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
        app_state,
        visible,
        stopped_rx,
        stopped_tx,
        ui_update_rx,
        tray_icon,
    )?;

    event_loop.run_app(&mut app).unwrap();

    Ok(())
}

struct WgpuState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface<'static>,
    scale_factor: f32,
    egui_renderer: EguiRenderer,
}

impl WgpuState {
    /// based on https://github.com/kaphula/winit-egui-wgpu-template/blob/master/src/egui_tools.rs
    async fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        window: &Window,
        width: u32,
        height: u32,
    ) -> Self {
        let power_pref = wgpu::PowerPreference::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: power_pref,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("Failed to find an appropriate adapter");

        let features = wgpu::Features::empty();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: features,
                required_limits: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .expect("Failed to create device");

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let selected_format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let swapchain_format = swapchain_capabilities
            .formats
            .iter()
            .find(|d| **d == selected_format)
            .expect("failed to select proper surface texture format!");

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: *swapchain_format,
            width,
            height,
            // if u use AutoNoVsync instead it will fix tearing behaviour when resizing, but at cost of significantly higher CPU usage
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &surface_config);

        let egui_renderer = EguiRenderer::new(&device, surface_config.format, None, 1, window);

        let scale_factor = 1.0;

        Self {
            device,
            queue,
            surface,
            surface_config,
            egui_renderer,
            scale_factor,
        }
    }

    fn resize_surface(&mut self, width: u32, height: u32) {
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }
}

struct App {
    instance: wgpu::Instance,
    wgpu_state: Option<WgpuState>,
    window: Option<Arc<Window>>,
    main_app: MainApp,
}

impl App {
    fn new(
        app_state: Arc<AppState>,
        visible: Arc<AtomicBool>,
        stopped_rx: tokio::sync::broadcast::Receiver<()>,
        stopped_tx: tokio::sync::broadcast::Sender<()>,
        ui_update_rx: tokio::sync::mpsc::Receiver<UiUpdate>,
        tray_icon: tray_icon::TrayIconState,
    ) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let main_app = MainApp::new(
            app_state,
            visible,
            stopped_rx,
            stopped_tx,
            ui_update_rx,
            tray_icon,
        )?;

        Ok(Self {
            instance,
            wgpu_state: None,
            window: None,
            main_app,
        })
    }

    async fn set_window(&mut self, window: Window) {
        let window = Arc::new(window);
        let initial_width = 800;
        let initial_height = 1060;

        let _ = window.request_inner_size(PhysicalSize::new(initial_width, initial_height));

        let surface = self
            .instance
            .create_surface(window.clone())
            .expect("Failed to create surface!");

        let state = WgpuState::new(
            &self.instance,
            surface,
            &window,
            initial_width,
            initial_height,
        )
        .await;

        self.window.get_or_insert(window);
        self.wgpu_state.get_or_insert(state);
    }

    fn handle_resized(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.wgpu_state
                .as_mut()
                .unwrap()
                .resize_surface(width, height);
        }
    }

    fn handle_redraw(&mut self) {
        // Attempt to handle minimizing window
        if let Some(window) = self.window.as_ref()
            && let Some(min) = window.is_minimized()
            && min
        {
            return;
        }

        let state = self.wgpu_state.as_mut().unwrap();

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [state.surface_config.width, state.surface_config.height],
            pixels_per_point: self.window.as_ref().unwrap().scale_factor() as f32
                * state.scale_factor,
        };

        let surface_texture = state.surface.get_current_texture();

        match surface_texture {
            Err(SurfaceError::Outdated) => {
                // Ignoring outdated to allow resizing and minimization
                return;
            }
            Err(_) => {
                surface_texture.expect("Failed to acquire next swap chain texture");
                return;
            }
            Ok(_) => {}
        };

        let surface_texture = surface_texture.unwrap();

        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let window = self.window.as_ref().unwrap();

        {
            state.egui_renderer.begin_frame(window);

            // Render the main UI
            self.main_app.render(state.egui_renderer.context());

            state.egui_renderer.end_frame_and_draw(
                &state.device,
                &state.queue,
                &mut encoder,
                window,
                &surface_view,
                screen_descriptor,
            );
        }

        state.queue.submit(Some(encoder.finish()));
        surface_texture.present();
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

        let window_attributes = Window::default_attributes()
            .with_title("OWL Control")
            .with_inner_size(PhysicalSize::new(600, 660))
            .with_min_inner_size(PhysicalSize::new(400, 450))
            .with_resizable(true)
            .with_window_icon(Some(window_icon));

        let window = event_loop.create_window(window_attributes).unwrap();

        // Block on async initialization
        futures::executor::block_on(self.set_window(window));

        // Initialize tray icon and egui context after window is created
        let ctx = self.wgpu_state.as_ref().unwrap().egui_renderer.context();
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
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if self.wgpu_state.is_none() {
            return;
        }

        // Let egui renderer process the event first
        let _response = self
            .wgpu_state
            .as_mut()
            .unwrap()
            .egui_renderer
            .handle_input(self.window.as_ref().unwrap(), &event);

        // Handle window events
        self.main_app.handle_window_event(
            event_loop,
            &event,
            self.wgpu_state.as_ref().unwrap().egui_renderer.context(),
        );

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
                self.window.as_ref().unwrap().request_redraw();
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
    ui_update_rx: tokio::sync::mpsc::Receiver<UiUpdate>,

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
    /// Is the UI currently listening for user to select a new hotkey for recording shortcut
    listening_for_hotkey_rebind: Option<HotkeyRebindTarget>,

    /// Current upload progress, updated from upload bridge via mpsc channel
    current_upload_progress: Option<upload::ProgressData>,
    /// Last upload error, updated from upload bridge via mpsc channel
    last_upload_error: Option<String>,

    md_cache: CommonMarkCache,
    visible: Arc<AtomicBool>,
    stopped_rx: tokio::sync::broadcast::Receiver<()>,
    stopped_tx: tokio::sync::broadcast::Sender<()>,
    has_stopped: bool,

    tray_icon: tray_icon::TrayIconState,
}
impl MainApp {
    fn new(
        app_state: Arc<AppState>,
        visible: Arc<AtomicBool>,
        stopped_rx: tokio::sync::broadcast::Receiver<()>,
        stopped_tx: tokio::sync::broadcast::Sender<()>,
        ui_update_rx: tokio::sync::mpsc::Receiver<UiUpdate>,
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

            login_api_key: local_credentials.api_key.clone(),
            is_authenticating_login_api_key: false,
            authenticated_user_id: None,
            has_scrolled_to_bottom_of_consent: false,

            local_credentials,
            local_preferences,
            config_last_edit: None,
            listening_for_hotkey_rebind: None,

            current_upload_progress: None,
            last_upload_error: None,

            md_cache: CommonMarkCache::default(),
            visible,
            stopped_rx,
            stopped_tx,
            has_stopped: false,

            tray_icon,
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
        match self.ui_update_rx.try_recv() {
            Ok(UiUpdate::ForceUpdate) => {
                ctx.request_repaint();
            }
            Ok(UiUpdate::UpdateUploadProgress(progress_data)) => {
                self.current_upload_progress = progress_data;
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
                self.last_upload_error = Some(error);
            }
            Ok(UiUpdate::UpdateTrayIconRecording(recording)) => {
                self.tray_icon.set_icon_recording(recording);
            }
            Err(_) => {}
        };

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
        if let Some(target) = self.listening_for_hotkey_rebind {
            ctx.input(|i| {
                let Some(key) = i.keys_down.iter().next().map(|k| k.name().to_string()) else {
                    return;
                };

                let rebind_target = match target {
                    HotkeyRebindTarget::Start => &mut self.local_preferences.start_recording_key,
                    HotkeyRebindTarget::Stop => &mut self.local_preferences.stop_recording_key,
                };
                *rebind_target = key;
                self.listening_for_hotkey_rebind = None;
            });
        }
        // Very lazy solution (as opposed to tracking state changes), but should be sufficient
        self.app_state.is_currently_rebinding.store(
            self.listening_for_hotkey_rebind.is_some(),
            Ordering::Relaxed,
        );
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

    pub fn login_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Center the content vertically and horizontally
            ui.vertical_centered(|ui| {
                // Extremely ugly bodge. I assume there's a way to do this correctly, but I can't find it at a glance.
                let content_height = 240.0;
                let available_height = ui.available_height();
                ui.add_space((available_height - content_height) / 2.0);

                ui.set_max_width(ui.available_width() * 0.8);
                ui.vertical_centered(|ui| {
                    // Logo/Icon area (placeholder for now)
                    ui.add_space(20.0);

                    // Main heading with better styling
                    ui.heading(
                        egui::RichText::new("Welcome to OWL Control")
                            .size(28.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 220, 220)),
                    );

                    ui.add_space(8.0);

                    // Subtitle
                    ui.label(
                        egui::RichText::new("Please enter your API key to continue")
                            .size(16.0)
                            .color(egui::Color32::from_rgb(180, 180, 180)),
                    );

                    ui.add_space(20.0);

                    // API Key input section
                    ui.vertical_centered(|ui| {
                        // Styled text input
                        let text_edit = egui::TextEdit::singleline(&mut self.login_api_key)
                            .desired_width(ui.available_width())
                            .vertical_align(egui::Align::Center)
                            .hint_text("sk_...");

                        ui.add_sized(egui::vec2(ui.available_width(), 40.0), text_edit);

                        ui.add_space(10.0);

                        // Help text
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                            ui.label(
                                egui::RichText::new("Don't have an API key? Please sign up at ")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(140, 140, 140)),
                            );
                            ui.hyperlink_to(
                                egui::RichText::new("our website.").size(12.0),
                                "https://wayfarerlabs.ai/handler/sign-in",
                            );
                        });
                        ui.add_space(10.0);

                        if let Some(Err(err)) = &self.authenticated_user_id {
                            ui.label(
                                egui::RichText::new(err)
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(255, 0, 0)),
                            );
                            ui.add_space(10.0);
                        }

                        // Submit button
                        ui.add_enabled_ui(!self.is_authenticating_login_api_key, |ui| {
                            let submit_button = ui.add_sized(
                                egui::vec2(120.0, 36.0),
                                egui::Button::new(
                                    egui::RichText::new(if self.is_authenticating_login_api_key {
                                        "Validating..."
                                    } else {
                                        "Continue"
                                    })
                                    .size(16.0)
                                    .strong(),
                                ),
                            );

                            if submit_button.clicked() && !self.is_authenticating_login_api_key {
                                self.is_authenticating_login_api_key = true;
                                self.app_state
                                    .async_request_tx
                                    .blocking_send(AsyncRequest::ValidateApiKey {
                                        api_key: self.login_api_key.clone(),
                                    })
                                    .ok();
                            }
                        });
                    });
                });
            });
        });
    }

    pub fn consent_view(&mut self, ctx: &egui::Context) {
        let padding = 8;
        let button_font_size = 14.0;

        egui::TopBottomPanel::top("consent_panel_top").show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(padding))
                .show(ui, |ui| {
                    ui.heading(
                        egui::RichText::new("Informed Consent & Terms of Service")
                            .size(HEADING_TEXT_SIZE)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new("Please read the following information carefully.")
                            .size(SUBHEADING_TEXT_SIZE),
                    );
                });
        });

        egui::TopBottomPanel::bottom("consent_panel_bottom").show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(padding))
                .show(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(8.0, 2.0);
                            if ui
                                .add_enabled(
                                    self.has_scrolled_to_bottom_of_consent,
                                    egui::Button::new(
                                        egui::RichText::new("Accept")
                                            .size(button_font_size)
                                            .strong(),
                                    ),
                                )
                                .clicked()
                            {
                                self.go_to_main();
                            }
                            if ui
                                .button(
                                    egui::RichText::new("Cancel")
                                        .size(button_font_size)
                                        .strong(),
                                )
                                .clicked()
                            {
                                self.go_to_login();
                            }
                        });
                    });
                });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(padding))
                .show(ui, |ui| {
                    let output = egui::ScrollArea::vertical().show(ui, |ui| {
                        commonmark_str!(ui, &mut self.md_cache, "./src/ui/consent.md");
                    });

                    self.has_scrolled_to_bottom_of_consent |= (output.state.offset.y
                        + output.inner_rect.height())
                        >= output.content_size.y;
                });
        });
    }

    pub fn main_view(&mut self, ctx: &egui::Context) {
        const SETTINGS_TEXT_WIDTH: f32 = 150.0;
        const SETTINGS_TEXT_HEIGHT: f32 = 20.0;

        fn add_settings_text(ui: &mut egui::Ui, widget: impl egui::Widget) -> egui::Response {
            ui.allocate_ui_with_layout(
                egui::vec2(SETTINGS_TEXT_WIDTH, SETTINGS_TEXT_HEIGHT),
                egui::Layout {
                    main_dir: egui::Direction::LeftToRight,
                    main_wrap: false,
                    main_align: egui::Align::RIGHT,
                    main_justify: true,
                    cross_align: egui::Align::Center,
                    cross_justify: true,
                },
                |ui| ui.add(widget),
            )
            .inner
        }

        fn add_settings_ui<R>(
            ui: &mut egui::Ui,
            add_contents: impl FnOnce(&mut egui::Ui) -> R,
        ) -> egui::InnerResponse<R> {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), SETTINGS_TEXT_HEIGHT),
                egui::Layout {
                    main_dir: egui::Direction::LeftToRight,
                    main_wrap: false,
                    main_align: egui::Align::LEFT,
                    main_justify: true,
                    cross_align: egui::Align::Center,
                    cross_justify: true,
                },
                add_contents,
            )
        }

        fn add_settings_widget(ui: &mut egui::Ui, widget: impl egui::Widget) -> egui::Response {
            add_settings_ui(ui, |ui| ui.add(widget)).inner
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(
                egui::RichText::new("Settings")
                    .size(HEADING_TEXT_SIZE)
                    .strong(),
            );
            ui.label(
                egui::RichText::new("Configure your recording preferences")
                    .size(SUBHEADING_TEXT_SIZE),
            );
            ui.add_space(10.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Account Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Account").size(18.0).strong());
                    ui.separator();

                    ui.vertical(|ui| {
                        ui.label("User ID:");
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_sized(
                                            egui::vec2(0.0, SETTINGS_TEXT_HEIGHT),
                                            egui::Button::new("Log out"),
                                        )
                                        .clicked()
                                    {
                                        self.go_to_login();
                                    }

                                    let user_id = self
                                        .authenticated_user_id
                                        .clone()
                                        .unwrap_or_else(|| Ok("Authenticating...".to_string()))
                                        .unwrap_or_else(|e| format!("Error: {e}"));
                                    ui.add_sized(
                                        egui::vec2(ui.available_width(), SETTINGS_TEXT_HEIGHT),
                                        egui::TextEdit::singleline(&mut user_id.as_str()),
                                    );
                                },
                            );
                        });
                    });
                });
                ui.add_space(10.0);

                // Keyboard Shortcuts Section
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("Keyboard Shortcuts")
                            .size(18.0)
                            .strong(),
                    );
                    ui.separator();

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Start Recording:"));
                        let button_text = if self.listening_for_hotkey_rebind
                            == Some(HotkeyRebindTarget::Start)
                        {
                            "Press any key...".to_string()
                        } else {
                            self.local_preferences.start_recording_key.clone()
                        };

                        if add_settings_widget(ui, egui::Button::new(button_text)).clicked() {
                            self.listening_for_hotkey_rebind = Some(HotkeyRebindTarget::Start);
                        }
                    });

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Stop Recording:"));
                        let button_text =
                            if self.listening_for_hotkey_rebind == Some(HotkeyRebindTarget::Stop) {
                                "Press any key...".to_string()
                            } else {
                                self.local_preferences.stop_recording_key.clone()
                            };

                        if add_settings_widget(ui, egui::Button::new(button_text)).clicked() {
                            self.listening_for_hotkey_rebind = Some(HotkeyRebindTarget::Stop);
                        }
                    });
                });
                ui.add_space(10.0);

                // Overlay Settings Section
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("Recorder Customization")
                            .size(18.0)
                            .strong(),
                    );
                    ui.separator();

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Overlay Location:"));
                        add_settings_ui(ui, |ui| {
                            egui::ComboBox::from_id_salt("overlay_location")
                                .selected_text(self.local_preferences.overlay_location.to_string())
                                .show_ui(ui, |ui| {
                                    for location in crate::config::OverlayLocation::ALL {
                                        ui.selectable_value(
                                            &mut self.local_preferences.overlay_location,
                                            location,
                                            location.to_string(),
                                        );
                                    }
                                });
                        });
                    });

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Overlay Opacity:"));
                        let mut stored_opacity = self.local_preferences.overlay_opacity;

                        let mut egui_opacity = stored_opacity as f32 / 255.0 * 100.0;

                        let r = ui
                            .scope(|ui| {
                                // one day egui will make sliders respect their width properly
                                ui.spacing_mut().slider_width = ui.available_width() - 50.0;
                                add_settings_widget(
                                    ui,
                                    egui::Slider::new(&mut egui_opacity, 0.0..=100.0)
                                        .suffix("%")
                                        .integer(),
                                )
                            })
                            .inner;
                        if r.changed() {
                            stored_opacity = (egui_opacity / 100.0 * 255.0) as u8;
                            self.local_preferences.overlay_opacity = stored_opacity;
                        }
                    });

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Recording Audio Cue:"));
                        let honk = self.local_preferences.honk;
                        add_settings_widget(
                            ui,
                            egui::Checkbox::new(
                                &mut self.local_preferences.honk,
                                match honk {
                                    true => "Honk.",
                                    false => "Honk?",
                                },
                            ),
                        );
                    })
                });

                ui.add_space(10.0);

                // Upload Manager Section
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Upload Manager").size(18.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(egui::RichText::new("Open Recordings Folder").size(12.0))
                                .clicked()
                            {
                                self.app_state
                                    .async_request_tx
                                    .blocking_send(AsyncRequest::OpenDataDump)
                                    .ok();
                            }
                        });
                    });
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        let available_width = ui.available_width() - 40.0;
                        let cell_width = available_width / 4.0;

                        let upload_stats = self.app_state.upload_stats.read().unwrap().clone();

                        // Cell 1: Total Uploaded
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                create_upload_cell(
                                    ui,
                                    "📊", // Icon
                                    "Total Uploaded",
                                    &util::format_seconds(
                                        upload_stats.total_duration_uploaded as u64,
                                    ),
                                );
                            },
                        );

                        // Cell 2: Files Uploaded
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                create_upload_cell(
                                    ui,
                                    "📁", // Icon
                                    "Files Uploaded",
                                    &upload_stats.total_files_uploaded.to_string(),
                                );
                            },
                        );

                        // Cell 3: Volume Uploaded
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                create_upload_cell(
                                    ui,
                                    "💾", // Icon
                                    "Volume Uploaded",
                                    &util::format_bytes(upload_stats.total_volume_uploaded),
                                );
                            },
                        );

                        // Cell 4: Last Upload
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                create_upload_cell(
                                    ui,
                                    "🕒", // Icon
                                    "Last Upload",
                                    &upload_stats
                                        .last_upload_date
                                        .as_date()
                                        .map(util::format_datetime)
                                        .unwrap_or_else(|| "Never".to_string()),
                                );
                            },
                        );
                    });

                    // Progress Bar
                    let is_uploading = self.current_upload_progress.is_some();
                    if let Some(progress) = &self.current_upload_progress {
                        ui.add_space(10.0);
                        ui.label(format!(
                            "Current upload: {:.2}% ({}/{})",
                            progress.percent,
                            util::format_bytes(progress.bytes_uploaded),
                            util::format_bytes(progress.total_bytes),
                        ));
                        ui.add(egui::ProgressBar::new(progress.percent as f32 / 100.0));
                        ui.label(format!(
                            "Speed: {:.1} MB/s • ETA: {}",
                            progress.speed_mbps,
                            util::format_seconds(progress.eta_seconds as u64),
                        ));
                    }

                    // Unreliable Connection Setting
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Checkbox::new(
                            &mut self.local_preferences.unreliable_connection,
                            "Optimize for unreliable connections",
                        ));
                    });
                    ui.label(
                        egui::RichText::new(concat!(
                            "Enable this if you have a slow or unstable internet connection. ",
                            "This will use smaller file chunks to improve upload success rates."
                        ))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(128, 128, 128)),
                    );

                    // Upload Button
                    ui.add_space(10.0);
                    ui.add_enabled_ui(!is_uploading, |ui| {
                        if ui
                            .add_sized(
                                egui::vec2(ui.available_width(), 32.0),
                                egui::Button::new(
                                    egui::RichText::new(if is_uploading {
                                        "Upload in Progress..."
                                    } else {
                                        "Upload Recordings"
                                    })
                                    .size(12.0),
                                ),
                            )
                            .clicked()
                        {
                            self.last_upload_error = None;
                            self.app_state
                                .async_request_tx
                                .blocking_send(AsyncRequest::UploadData)
                                .ok();
                        }
                        if let Some(error) = &self.last_upload_error {
                            ui.label(
                                egui::RichText::new(error)
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(255, 0, 0)),
                            );
                        }
                    });
                });

                // Logo
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        if ui.button("FAQ").clicked() {
                            opener::open_browser(
                                "https://github.com/Wayfarer-Labs/owl-control/blob/main/GAMES.md",
                            )
                            .ok();
                        }
                        if ui.button("Logs").clicked() {
                            self.app_state
                                .async_request_tx
                                .blocking_send(AsyncRequest::OpenLog)
                                .ok();
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("Wayfarer Labs")
                                .italics()
                                .color(egui::Color32::LIGHT_BLUE),
                        );
                    });
                });
            });
        });
    }
}

fn create_upload_cell(ui: &mut egui::Ui, icon: &str, title: &str, value: &str) {
    // Icon
    ui.label(egui::RichText::new(icon).size(28.0));
    ui.add_space(8.0);
    // Title
    ui.label(egui::RichText::new(title).size(12.0).strong());
    ui.add_space(4.0);
    // Value
    ui.label(
        egui::RichText::new(value)
            .size(10.0)
            .color(egui::Color32::from_rgb(128, 128, 128)),
    );
}
