use std::{
    ptr,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

use egui::{
    Align2, Color32, Context, FontFamily, FontId, Image, Margin, RichText, Stroke, TextFormat,
    Vec2, WidgetText, Window, containers::Frame, text::LayoutJob,
};
use egui_wgpu::wgpu;
use egui_winit::winit::raw_window_handle;
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
};

use crate::{
    app_state::{AppState, RecordingStatus},
    assets::get_owl_bytes,
    config::OverlayLocation,
    system::hardware_specs::get_primary_monitor_resolution,
    ui::util,
};

const OVERLAY_WIDTH: i32 = 600;
const OVERLAY_HEIGHT: i32 = 50;

pub struct OverlayApp {
    app_state: Arc<AppState>,
    stopped_rx: tokio::sync::broadcast::Receiver<()>,

    // Window state
    hwnd: HWND,
    wgpu_state: Option<WgpuOverlayState>,

    // UI state
    initialized: bool,
    overlay_location: OverlayLocation,
    overlay_opacity: u8,
    rec_status: RecordingStatus,
    last_paint_time: Instant,

    // wgpu instance
    wgpu_instance: wgpu::Instance,
}

struct WgpuOverlayState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    renderer: egui_wgpu::Renderer,
    egui_ctx: Context,
    scale_factor: f32,
}

impl OverlayApp {
    pub fn new(
        app_state: Arc<AppState>,
        stopped_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<Self> {
        let (overlay_location, overlay_opacity) = {
            let config = app_state.config.read().unwrap();
            (
                config.preferences.overlay_location,
                config.preferences.overlay_opacity,
            )
        };
        let rec_status = app_state.state.read().unwrap().clone();

        let wgpu_instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });

        Ok(Self {
            app_state,
            stopped_rx,
            hwnd: HWND(ptr::null_mut()),
            wgpu_state: None,
            initialized: false,
            overlay_location,
            overlay_opacity,
            rec_status,
            last_paint_time: Instant::now(),
            wgpu_instance,
        })
    }

    fn create_window(&mut self) -> Result<()> {
        unsafe {
            let class_name = w!("OwlOverlayClass");
            let window_name = w!("OWL Recording Overlay");

            let hinstance = GetModuleHandleW(None)?;

            // Register window class
            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
                lpfnWndProc: Some(Self::wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(ptr::null_mut()),
                ..Default::default()
            };

            let atom = RegisterClassW(&wc);
            if atom == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(Error::from_win32());
            }

            // Calculate initial position
            let (x, y) = self.calculate_position(OVERLAY_WIDTH, OVERLAY_HEIGHT);

            // Create window
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class_name,
                window_name,
                WS_POPUP,
                x,
                y,
                OVERLAY_WIDTH,
                OVERLAY_HEIGHT,
                None,
                None,
                Some(hinstance.into()),
                None,
            )?;

            self.hwnd = hwnd;

            // Set window transparency
            if self.overlay_opacity > 0 {
                SetLayeredWindowAttributes(hwnd, COLORREF(0), self.overlay_opacity, LWA_ALPHA)?;
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }

            Ok(())
        }
    }

    fn calculate_position(&self, width: i32, height: i32) -> (i32, i32) {
        let (monitor_width, monitor_height) = match get_primary_monitor_resolution() {
            Some((w, h)) => (w as i32, h as i32),
            None => {
                tracing::error!("Failed to get primary monitor resolution");
                (1920, 1080)
            }
        };

        match self.overlay_location {
            OverlayLocation::TopLeft => (0, 0),
            OverlayLocation::TopRight => (monitor_width - width, 0),
            OverlayLocation::BottomLeft => (0, monitor_height - height),
            OverlayLocation::BottomRight => (monitor_width - width, monitor_height - height),
        }
    }

    async fn init_wgpu(&mut self) -> Result<()> {
        // Create surface from HWND
        let window_handle = WindowHandle {
            hwnd: self.hwnd.0 as *mut _,
        };

        let surface_target = unsafe {
            wgpu::SurfaceTargetUnsafe::from_window(&window_handle)
                .map_err(|e| Error::new(E_FAIL, format!("Failed to create surface target: {}", e)))?
        };

        let surface = unsafe {
            self.wgpu_instance.create_surface_unsafe(surface_target)
                .map_err(|e| Error::new(E_FAIL, format!("Failed to create surface: {}", e)))?
        };

        // Request adapter
        let adapter = self
            .wgpu_instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| Error::new(E_FAIL, format!("Failed to find adapter: {}", e)))?;

        // Request device
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::default(),
                required_limits: wgpu::Limits::default(),
                label: Some("overlay_device"),
                ..Default::default()
            })
            .await
            .map_err(|e| Error::new(E_FAIL, format!("Failed to create device: {}", e)))?;

        // Get supported alpha modes
        let surface_caps = surface.get_capabilities(&adapter);
        let alpha_mode = surface_caps
            .alpha_modes
            .iter()
            .find(|mode| matches!(mode, wgpu::CompositeAlphaMode::PreMultiplied | wgpu::CompositeAlphaMode::PostMultiplied))
            .copied()
            .unwrap_or(surface_caps.alpha_modes[0]);

        tracing::info!("Overlay using alpha mode: {:?}", alpha_mode);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: OVERLAY_WIDTH as u32,
            height: OVERLAY_HEIGHT as u32,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &surface_config);

        // Create egui context
        let egui_ctx = Context::default();

        // Create renderer
        let renderer = egui_wgpu::Renderer::new(
            &device,
            surface_config.format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                ..Default::default()
            },
        );

        let scale_factor = unsafe {
            let hdc = GetDC(Some(self.hwnd));
            let dpi = GetDeviceCaps(Some(hdc), LOGPIXELSX);
            let _ = ReleaseDC(Some(self.hwnd), hdc);
            dpi as f32 / 96.0
        };

        self.wgpu_state = Some(WgpuOverlayState {
            surface,
            device,
            queue,
            surface_config,
            renderer,
            egui_ctx,
            scale_factor,
        });

        Ok(())
    }

    fn set_window_visible(&self, visible: bool) {
        unsafe {
            if visible && self.overlay_opacity > 0 {
                let _ = SetLayeredWindowAttributes(
                    self.hwnd,
                    COLORREF(0),
                    self.overlay_opacity,
                    LWA_ALPHA,
                );
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            } else {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }

    fn update_window_position(&self, _location: OverlayLocation) {
        let (x, y) = self.calculate_position(OVERLAY_WIDTH, OVERLAY_HEIGHT);
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                OVERLAY_WIDTH,
                OVERLAY_HEIGHT,
                SWP_NOACTIVATE,
            );
        }
    }

    fn render_ui(&mut self) {
        // Get current config
        let (curr_opacity, curr_location) = {
            let config = self.app_state.config.read().unwrap();
            (
                config.preferences.overlay_opacity,
                config.preferences.overlay_location,
            )
        };

        // Handle opacity changes
        if curr_opacity != self.overlay_opacity {
            self.overlay_opacity = curr_opacity;
            self.set_window_visible(curr_opacity > 0);
        }

        // Handle location changes
        if curr_location != self.overlay_location {
            self.overlay_location = curr_location;
            self.update_window_position(curr_location);
        }

        // Check if we need to repaint
        let curr_state = self.app_state.state.read().unwrap().clone();
        let should_repaint = self.last_paint_time.elapsed() > Duration::from_millis(500)
            || curr_state != self.rec_status;

        if should_repaint {
            self.rec_status = curr_state;
            self.last_paint_time = Instant::now();
        }

        if self.overlay_opacity == 0 {
            return;
        }

        // Now handle rendering
        let Some(wgpu_state) = &mut self.wgpu_state else {
            return;
        };

        // Prepare frame
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(
                    OVERLAY_WIDTH as f32 / wgpu_state.scale_factor,
                    OVERLAY_HEIGHT as f32 / wgpu_state.scale_factor,
                ),
            )),
            ..Default::default()
        };

        // Set pixels_per_point in context before running
        wgpu_state.egui_ctx.set_pixels_per_point(wgpu_state.scale_factor);

        // Collect data needed for UI rendering
        let overlay_opacity = self.overlay_opacity;
        let overlay_location = self.overlay_location;
        let rec_status = self.rec_status.clone();
        let is_out_of_date = self.app_state.is_out_of_date.load(Ordering::Relaxed);

        let full_output = wgpu_state.egui_ctx.run(raw_input, |ctx| {
            Self::render_overlay_ui_static(
                ctx,
                overlay_opacity,
                overlay_location,
                &rec_status,
                is_out_of_date,
            );
        });

        let primitives = wgpu_state
            .egui_ctx
            .tessellate(full_output.shapes, wgpu_state.scale_factor);

        // Render
        let surface_texture = match wgpu_state.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(e) => {
                tracing::error!("Failed to get surface texture: {}", e);
                return;
            }
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = wgpu_state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("overlay_encoder"),
            });

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [wgpu_state.surface_config.width, wgpu_state.surface_config.height],
            pixels_per_point: wgpu_state.scale_factor,
        };

        for (id, image_delta) in &full_output.textures_delta.set {
            wgpu_state
                .renderer
                .update_texture(&wgpu_state.device, &wgpu_state.queue, *id, image_delta);
        }

        wgpu_state.renderer.update_buffers(
            &wgpu_state.device,
            &wgpu_state.queue,
            &mut encoder,
            &primitives,
            &screen_descriptor,
        );

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            wgpu_state
                .renderer
                .render(&mut render_pass.forget_lifetime(), &primitives, &screen_descriptor);
        }

        wgpu_state.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        for id in &full_output.textures_delta.free {
            wgpu_state.renderer.free_texture(id);
        }
    }

    fn render_overlay_ui_static(
        ctx: &Context,
        overlay_opacity: u8,
        overlay_location: OverlayLocation,
        rec_status: &RecordingStatus,
        is_out_of_date: bool,
    ) {
        // Install image loaders on first use
        egui_extras::install_image_loaders(ctx);

        let frame = Frame {
            fill: Color32::from_black_alpha(overlay_opacity),
            stroke: Stroke::NONE,
            corner_radius: 0.0.into(),
            shadow: Default::default(),
            inner_margin: Margin::same(8),
            outer_margin: Margin::ZERO,
        };

        let (align, pos) = match overlay_location {
            OverlayLocation::TopLeft => (Align2::LEFT_TOP, Vec2 { x: 10.0, y: 10.0 }),
            OverlayLocation::TopRight => (Align2::RIGHT_TOP, Vec2 { x: -10.0, y: 10.0 }),
            OverlayLocation::BottomLeft => (Align2::LEFT_BOTTOM, Vec2 { x: 10.0, y: -10.0 }),
            OverlayLocation::BottomRight => (Align2::RIGHT_BOTTOM, Vec2 { x: -10.0, y: -10.0 }),
        };

        Window::new("recording overlay")
            .title_bar(false)
            .resizable(false)
            .scroll([false, false])
            .collapsible(false)
            .anchor(align, pos)
            .auto_sized()
            .frame(frame)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        Image::from_bytes("bytes://", get_owl_bytes())
                            .fit_to_exact_size(Vec2 { x: 24.0, y: 24.0 })
                            .tint(Color32::from_white_alpha(overlay_opacity)),
                    );

                    let font_id = FontId::new(12.0, FontFamily::Proportional);
                    let color = Color32::from_white_alpha(overlay_opacity);
                    let recording_text: WidgetText = if is_out_of_date {
                        RichText::new("Out of date; will not record. Please update!")
                            .font(font_id)
                            .color(color)
                            .into()
                    } else {
                        match rec_status {
                            RecordingStatus::Stopped => {
                                RichText::new("Stopped").font(font_id).color(color).into()
                            }
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
                            RecordingStatus::Paused => {
                                RichText::new("Paused").font(font_id).color(color).into()
                            }
                        }
                    };
                    ui.label(recording_text);
                });
            });
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe {
            match msg {
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
    }

    fn run_message_loop(&mut self) {
        unsafe {
            let mut msg = MSG::default();
            let mut last_render = Instant::now();

            loop {
                // Check for stop signal
                if self.stopped_rx.try_recv().is_ok() {
                    tracing::info!("Overlay received stop signal");
                    break;
                }

                // Process messages without blocking
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        return;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                // Render at ~60fps
                if last_render.elapsed() > Duration::from_millis(16) {
                    self.render_ui();
                    last_render = Instant::now();
                }

                // Sleep to avoid burning CPU
                std::thread::sleep(Duration::from_millis(8));
            }

            // Cleanup
            if !self.hwnd.is_invalid() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

// Helper struct for creating wgpu surface from HWND
struct WindowHandle {
    hwnd: *mut std::ffi::c_void,
}

impl raw_window_handle::HasWindowHandle for WindowHandle {
    fn window_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>
    {
        let handle = raw_window_handle::Win32WindowHandle::new(
            std::num::NonZeroIsize::new(self.hwnd as isize).unwrap(),
        );
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(handle.into()) })
    }
}

impl raw_window_handle::HasDisplayHandle for WindowHandle {
    fn display_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError>
    {
        let handle = raw_window_handle::WindowsDisplayHandle::new();
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(handle.into()) })
    }
}

pub fn start_overlay(app_state: Arc<AppState>, stopped_rx: tokio::sync::broadcast::Receiver<()>) {
    let mut app = match OverlayApp::new(app_state, stopped_rx) {
        Ok(app) => app,
        Err(e) => {
            tracing::error!("Failed to create overlay app: {}", e);
            return;
        }
    };

    if let Err(e) = app.create_window() {
        tracing::error!("Failed to create overlay window: {}", e);
        return;
    }

    // Initialize wgpu
    if let Err(e) = futures::executor::block_on(app.init_wgpu()) {
        tracing::error!("Failed to initialize overlay wgpu: {}", e);
        return;
    }

    app.initialized = true;
    app.run_message_loop();
}
