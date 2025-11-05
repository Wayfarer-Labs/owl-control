use std::{
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use color_eyre::Result;
use egui_wgpu::wgpu;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

use crate::{
    app_state::{AppState, UiUpdate, UiUpdateUnreliable},
    assets,
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
/// The UI will bug out below a given size. This is a conservative estimate.
const WINDOW_MIN_INNER_SIZE: PhysicalSize<u32> = PhysicalSize::new(400, 450);

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

    let mut app = WinitApp::new(
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

struct WinitApp {
    instance: wgpu::Instance,
    wgpu_state: Option<internal::WgpuState>,
    window: Option<Arc<Window>>,
    main_app: views::App,
    last_repaint_requested: Instant,
}
impl WinitApp {
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
        let main_app = views::App::new(
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

impl ApplicationHandler for WinitApp {
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
            .with_min_inner_size(WINDOW_MIN_INNER_SIZE)
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

        if let Some(window) = self.window.clone() {
            self.main_app.resumed(ctx, window.clone());
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
