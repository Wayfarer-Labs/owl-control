/// based on https://github.com/kaphula/winit-egui-wgpu-template/blob/master/src/egui_tools.rs
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    rc::Rc,
    sync::Arc,
};

use egui_wgpu::ScreenDescriptor;
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::SurfaceError;
use egui_winit::State as EguiWinitState;
use winit::{
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

/// Per-viewport state containing window-specific resources
pub struct ViewportState {
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub egui_state: EguiWinitState,
}

/// Shared rendering state that can be accessed from the immediate viewport callback
pub struct SharedWgpuState {
    // Shared resources
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub egui_renderer: egui_wgpu::Renderer,
    pub texture_format: wgpu::TextureFormat,
    pub adapter: wgpu::Adapter,
    pub instance: wgpu::Instance,

    // Per-viewport resources
    pub viewports: HashMap<egui::ViewportId, ViewportState>,
    pub window_to_viewport: HashMap<WindowId, egui::ViewportId>,

    // Event loop reference for creating windows in immediate viewports
    pub event_loop: Option<*const ActiveEventLoop>,
}

// Safety: We only access event_loop from the main thread during rendering
unsafe impl Send for SharedWgpuState {}
unsafe impl Sync for SharedWgpuState {}

impl SharedWgpuState {
    fn resize_viewport(&mut self, viewport_id: egui::ViewportId, width: u32, height: u32) {
        if let Some(viewport) = self.viewports.get_mut(&viewport_id)
            && width > 0
            && height > 0
        {
            viewport.surface_config.width = width;
            viewport.surface_config.height = height;
            viewport
                .surface
                .configure(&self.device, &viewport.surface_config);
        }
    }
}

pub struct WgpuState {
    pub shared: Rc<RefCell<SharedWgpuState>>,
    pub egui_ctx: egui::Context,
}

impl WgpuState {
    /// based on https://github.com/kaphula/winit-egui-wgpu-template/blob/master/src/egui_tools.rs
    pub async fn new(
        instance: wgpu::Instance,
        initial_surface: wgpu::Surface<'static>,
        window: Arc<Window>,
        width: u32,
        height: u32,
    ) -> Self {
        tracing::debug!("WgpuState::new() called");
        tracing::debug!("Requesting WGPU adapter");
        let power_pref = wgpu::PowerPreference::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: power_pref,
                force_fallback_adapter: false,
                compatible_surface: Some(&initial_surface),
            })
            .await
            .expect("Failed to find an appropriate adapter");
        tracing::debug!("WGPU adapter acquired");

        tracing::debug!("Requesting WGPU device and queue");
        let features = wgpu::Features::empty();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: features,
                ..Default::default()
            })
            .await
            .expect("Failed to create device");
        tracing::debug!("WGPU device and queue created");

        tracing::debug!("Configuring surface");
        let swapchain_capabilities = initial_surface.get_capabilities(&adapter);
        let selected_format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let texture_format = *swapchain_capabilities
            .formats
            .iter()
            .find(|d| **d == selected_format)
            .expect("failed to select proper surface texture format!");

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: texture_format,
            width,
            height,
            // if u use AutoNoVsync instead it will fix tearing behaviour when resizing, but at cost of significantly higher CPU usage
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![],
        };

        initial_surface.configure(&device, &surface_config);
        tracing::debug!("Surface configured");

        // Create shared egui context
        let egui_ctx = egui::Context::default();

        // Enable native viewports (instead of embedding them in the root window)
        egui_ctx.set_embed_viewports(false);

        // Create egui_winit state for ROOT viewport
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            Some(2048),
        );

        // Create shared egui renderer
        tracing::debug!("Creating egui renderer");
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            texture_format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                ..Default::default()
            },
        );
        tracing::debug!("Egui renderer created");

        // Create ROOT viewport state
        let root_viewport = ViewportState {
            window: window.clone(),
            surface: initial_surface,
            surface_config,
            egui_state,
        };

        let mut viewports = HashMap::new();
        viewports.insert(egui::ViewportId::ROOT, root_viewport);

        let mut window_to_viewport = HashMap::new();
        window_to_viewport.insert(window.id(), egui::ViewportId::ROOT);

        let shared = Rc::new(RefCell::new(SharedWgpuState {
            device,
            queue,
            egui_renderer,
            texture_format,
            adapter,
            instance,
            viewports,
            window_to_viewport,
            event_loop: None,
        }));

        // Set up immediate viewport renderer
        let shared_for_callback = shared.clone();
        let ctx_for_callback = egui_ctx.clone();
        egui::Context::set_immediate_viewport_renderer(move |ctx, immediate_viewport| {
            let mut shared = shared_for_callback.borrow_mut();
            render_immediate_viewport(&mut shared, &ctx_for_callback, ctx, immediate_viewport);
        });

        tracing::debug!("WgpuState::new() complete");
        Self { shared, egui_ctx }
    }

    pub fn context(&self) -> &egui::Context {
        &self.egui_ctx
    }

    /// Get the viewport ID for a given window ID
    pub fn get_viewport_id(&self, window_id: WindowId) -> Option<egui::ViewportId> {
        self.shared
            .borrow()
            .window_to_viewport
            .get(&window_id)
            .copied()
    }

    /// Get the root viewport's window
    pub fn root_window(&self) -> Option<Arc<Window>> {
        self.shared
            .borrow()
            .viewports
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.window.clone())
    }

    /// Resize a viewport's surface
    pub fn resize_viewport(&mut self, viewport_id: egui::ViewportId, width: u32, height: u32) {
        self.shared
            .borrow_mut()
            .resize_viewport(viewport_id, width, height);
    }

    /// Handle input for a specific viewport
    pub fn handle_input(
        &mut self,
        viewport_id: egui::ViewportId,
        event: &winit::event::WindowEvent,
    ) -> egui_winit::EventResponse {
        let mut shared = self.shared.borrow_mut();
        if let Some(viewport) = shared.viewports.get_mut(&viewport_id) {
            viewport.egui_state.on_window_event(&viewport.window, event)
        } else {
            egui_winit::EventResponse {
                consumed: false,
                repaint: false,
            }
        }
    }

    /// Set the event loop reference for window creation
    pub fn set_event_loop(&mut self, event_loop: &ActiveEventLoop) {
        self.shared.borrow_mut().event_loop = Some(event_loop as *const _);
    }

    /// Clear the event loop reference
    pub fn clear_event_loop(&mut self) {
        self.shared.borrow_mut().event_loop = None;
    }

    /// Render the root viewport
    pub fn render(&mut self, ui: impl FnOnce(&egui::Context)) -> Option<egui::FullOutput> {
        self.render_viewport(egui::ViewportId::ROOT, ui)
    }

    /// Render a specific viewport
    pub fn render_viewport(
        &mut self,
        viewport_id: egui::ViewportId,
        ui: impl FnOnce(&egui::Context),
    ) -> Option<egui::FullOutput> {
        // First phase: get input and prepare for rendering
        let (raw_input, screen_descriptor, surface_texture, surface_view, encoder) = {
            let mut shared = self.shared.borrow_mut();

            // Create encoder first before borrowing viewport
            let encoder = shared
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            let viewport = shared.viewports.get_mut(&viewport_id)?;

            let screen_descriptor = ScreenDescriptor {
                size_in_pixels: [
                    viewport.surface_config.width,
                    viewport.surface_config.height,
                ],
                pixels_per_point: viewport.window.scale_factor() as f32,
            };

            let surface_texture = match viewport.surface.get_current_texture() {
                Ok(t) => t,
                Err(SurfaceError::Outdated) => return None,
                Err(e) => {
                    tracing::error!("Failed to get surface texture: {:?}", e);
                    return None;
                }
            };

            let surface_view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            let raw_input = viewport.egui_state.take_egui_input(&viewport.window);

            (
                raw_input,
                screen_descriptor,
                surface_texture,
                surface_view,
                encoder,
            )
        };

        // Second phase: run UI (may trigger immediate viewport callbacks)
        self.egui_ctx.begin_pass(raw_input);
        ui(&self.egui_ctx);
        let full_output = self.egui_ctx.end_pass();

        // Third phase: finish rendering
        render_egui_output(
            &mut self.shared.borrow_mut(),
            &self.egui_ctx,
            viewport_id,
            full_output.clone(),
            encoder,
            &surface_view,
            &screen_descriptor,
        );

        surface_texture.present();

        Some(full_output)
    }

    /// Process viewport output to create/destroy/update viewports
    pub fn process_viewport_output(
        &mut self,
        event_loop: &ActiveEventLoop,
        viewport_output: &BTreeMap<egui::ViewportId, egui::ViewportOutput>,
    ) {
        let mut shared = self.shared.borrow_mut();

        // Collect viewport IDs that should exist
        let active_viewport_ids: std::collections::HashSet<_> =
            viewport_output.keys().copied().collect();

        // Create new viewports
        for (viewport_id, output) in viewport_output.iter() {
            if *viewport_id == egui::ViewportId::ROOT {
                // Process commands for ROOT but don't recreate it
                if let Some(viewport) = shared.viewports.get(&egui::ViewportId::ROOT) {
                    process_viewport_commands(&viewport.window, &output.commands);
                }
                continue;
            }

            if !shared.viewports.contains_key(viewport_id) {
                // Create new viewport
                if let Some(window) = create_window_from_builder(event_loop, &output.builder) {
                    add_viewport(&mut shared, *viewport_id, window, &self.egui_ctx);
                }
            } else if let Some(viewport) = shared.viewports.get(viewport_id) {
                // Process commands for existing viewport
                process_viewport_commands(&viewport.window, &output.commands);
            }
        }

        // Remove viewports that are no longer active (except ROOT)
        let viewports_to_remove: Vec<_> = shared
            .viewports
            .keys()
            .filter(|id| **id != egui::ViewportId::ROOT && !active_viewport_ids.contains(*id))
            .copied()
            .collect();

        for viewport_id in viewports_to_remove {
            if let Some(viewport) = shared.viewports.remove(&viewport_id) {
                shared.window_to_viewport.remove(&viewport.window.id());
                tracing::debug!("Removed viewport {:?}", viewport_id);
            }
        }
    }
}

/// Helper to render egui output to a viewport
fn render_egui_output(
    shared: &mut SharedWgpuState,
    egui_ctx: &egui::Context,
    viewport_id: egui::ViewportId,
    full_output: egui::FullOutput,
    mut encoder: wgpu::CommandEncoder,
    surface_view: &wgpu::TextureView,
    screen_descriptor: &ScreenDescriptor,
) {
    // Handle platform output
    if let Some(viewport) = shared.viewports.get_mut(&viewport_id) {
        viewport
            .egui_state
            .handle_platform_output(&viewport.window, full_output.platform_output.clone());
    }

    // Tessellate and render
    let tris = egui_ctx.tessellate(full_output.shapes.clone(), full_output.pixels_per_point);

    // Update textures - need to get device/queue refs before mutable borrow of renderer
    let device = &shared.device;
    let queue = &shared.queue;

    for (id, image_delta) in &full_output.textures_delta.set {
        shared
            .egui_renderer
            .update_texture(device, queue, *id, image_delta);
    }

    shared
        .egui_renderer
        .update_buffers(device, queue, &mut encoder, &tris, screen_descriptor);

    {
        let rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: surface_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        shared
            .egui_renderer
            .render(&mut rpass.forget_lifetime(), &tris, screen_descriptor);
    }

    for x in &full_output.textures_delta.free {
        shared.egui_renderer.free_texture(x)
    }

    shared.queue.submit(Some(encoder.finish()));

    if let Some(viewport) = shared.viewports.get(&viewport_id) {
        viewport.window.pre_present_notify();
    }
}

/// Render an immediate viewport - called from the egui callback
fn render_immediate_viewport(
    shared: &mut SharedWgpuState,
    egui_ctx: &egui::Context,
    ctx: &egui::Context,
    mut immediate_viewport: egui::ImmediateViewport,
) {
    let viewport_id = immediate_viewport.ids.this;

    // Create viewport if it doesn't exist
    if !shared.viewports.contains_key(&viewport_id) {
        // Safety: event_loop is only set during render, and we're being called from render
        let event_loop = match shared.event_loop {
            Some(ptr) => unsafe { &*ptr },
            None => {
                tracing::error!("No event loop available for immediate viewport creation");
                return;
            }
        };

        if let Some(window) = create_window_from_builder(event_loop, &immediate_viewport.builder) {
            add_viewport(shared, viewport_id, window, egui_ctx);
        } else {
            tracing::error!("Failed to create window for viewport {:?}", viewport_id);
            return;
        }
    }

    // Create encoder first before borrowing viewport
    let encoder = shared
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    // Get the data we need from the viewport
    let (screen_descriptor, surface_texture, surface_view, raw_input) = {
        let viewport = match shared.viewports.get_mut(&viewport_id) {
            Some(v) => v,
            None => return,
        };

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [
                viewport.surface_config.width,
                viewport.surface_config.height,
            ],
            pixels_per_point: viewport.window.scale_factor() as f32,
        };

        let surface_texture = match viewport.surface.get_current_texture() {
            Ok(t) => t,
            Err(SurfaceError::Outdated) => return,
            Err(e) => {
                tracing::error!(
                    "Failed to get surface texture for immediate viewport: {:?}",
                    e
                );
                return;
            }
        };

        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let raw_input = viewport.egui_state.take_egui_input(&viewport.window);

        (screen_descriptor, surface_texture, surface_view, raw_input)
    };

    // Run the viewport's UI
    ctx.begin_pass(raw_input);
    (immediate_viewport.viewport_ui_cb)(ctx);
    let full_output = ctx.end_pass();

    // Render using the shared helper
    render_egui_output(
        shared,
        ctx,
        viewport_id,
        full_output,
        encoder,
        &surface_view,
        &screen_descriptor,
    );

    surface_texture.present();
}

fn add_viewport(
    shared: &mut SharedWgpuState,
    viewport_id: egui::ViewportId,
    window: Arc<Window>,
    egui_ctx: &egui::Context,
) {
    let surface = shared
        .instance
        .create_surface(window.clone())
        .expect("Failed to create surface for viewport");

    let swapchain_capabilities = surface.get_capabilities(&shared.adapter);
    let size = window.inner_size();

    let surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: shared.texture_format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: wgpu::PresentMode::AutoVsync,
        desired_maximum_frame_latency: 2,
        alpha_mode: swapchain_capabilities.alpha_modes[0],
        view_formats: vec![],
    };

    surface.configure(&shared.device, &surface_config);

    let egui_state = egui_winit::State::new(
        egui_ctx.clone(),
        viewport_id,
        &window,
        Some(window.scale_factor() as f32),
        None,
        Some(2048),
    );

    let viewport_state = ViewportState {
        window: window.clone(),
        surface,
        surface_config,
        egui_state,
    };

    shared.window_to_viewport.insert(window.id(), viewport_id);
    shared.viewports.insert(viewport_id, viewport_state);

    tracing::debug!(
        "Added viewport {:?} for window {:?}",
        viewport_id,
        window.id()
    );
}

fn create_window_from_builder(
    event_loop: &ActiveEventLoop,
    builder: &egui::ViewportBuilder,
) -> Option<Arc<Window>> {
    let mut window_attributes = Window::default_attributes();

    if let Some(title) = &builder.title {
        window_attributes = window_attributes.with_title(title.clone());
    }

    if let Some(inner_size) = builder.inner_size {
        window_attributes = window_attributes
            .with_inner_size(winit::dpi::LogicalSize::new(inner_size.x, inner_size.y));
    }

    if let Some(min_size) = builder.min_inner_size {
        window_attributes = window_attributes
            .with_min_inner_size(winit::dpi::LogicalSize::new(min_size.x, min_size.y));
    }

    if let Some(max_size) = builder.max_inner_size {
        window_attributes = window_attributes
            .with_max_inner_size(winit::dpi::LogicalSize::new(max_size.x, max_size.y));
    }

    if let Some(resizable) = builder.resizable {
        window_attributes = window_attributes.with_resizable(resizable);
    }

    if let Some(decorations) = builder.decorations {
        window_attributes = window_attributes.with_decorations(decorations);
    }

    match event_loop.create_window(window_attributes) {
        Ok(window) => Some(Arc::new(window)),
        Err(e) => {
            tracing::error!("Failed to create window: {:?}", e);
            None
        }
    }
}

fn process_viewport_commands(window: &Window, commands: &[egui::ViewportCommand]) {
    for cmd in commands {
        match cmd {
            egui::ViewportCommand::Title(title) => {
                window.set_title(title);
            }
            egui::ViewportCommand::Visible(visible) => {
                window.set_visible(*visible);
            }
            egui::ViewportCommand::InnerSize(size) => {
                let _ = window.request_inner_size(winit::dpi::LogicalSize::new(size.x, size.y));
            }
            egui::ViewportCommand::MinInnerSize(size) => {
                window.set_min_inner_size(Some(winit::dpi::LogicalSize::new(size.x, size.y)));
            }
            egui::ViewportCommand::MaxInnerSize(size) => {
                window.set_max_inner_size(Some(winit::dpi::LogicalSize::new(size.x, size.y)));
            }
            egui::ViewportCommand::Resizable(resizable) => {
                window.set_resizable(*resizable);
            }
            egui::ViewportCommand::Decorations(decorations) => {
                window.set_decorations(*decorations);
            }
            egui::ViewportCommand::Focus => {
                window.focus_window();
            }
            egui::ViewportCommand::RequestUserAttention(attention) => {
                let winit_attention = match attention {
                    egui::UserAttentionType::Informational => {
                        Some(winit::window::UserAttentionType::Informational)
                    }
                    egui::UserAttentionType::Critical => {
                        Some(winit::window::UserAttentionType::Critical)
                    }
                    egui::UserAttentionType::Reset => None,
                };
                window.request_user_attention(winit_attention);
            }
            egui::ViewportCommand::Minimized(minimized) => {
                window.set_minimized(*minimized);
            }
            egui::ViewportCommand::Maximized(maximized) => {
                window.set_maximized(*maximized);
            }
            // Close is handled by removing the viewport from the output
            _ => {}
        }
    }
}
