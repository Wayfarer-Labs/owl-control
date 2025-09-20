use std::{
    sync::{Arc, atomic::Ordering},
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

use crate::{AppState, RecordingStatus};

pub struct OverlayApp {
    frame: u64,
    app_state: Arc<AppState>,
    overlay_opacity: u8,         // local opacity tracker
    rec_status: RecordingStatus, // local rec status
    last_paint_time: Instant,
}
impl OverlayApp {
    pub fn new(app_state: Arc<AppState>) -> Self {
        let overlay_opacity = app_state.opacity.load(Ordering::Relaxed);
        let rec_status = app_state.state.read().unwrap().clone();
        Self {
            frame: 0,
            app_state,
            overlay_opacity,
            rec_status,
            last_paint_time: Instant::now(),
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
        // kind of cringe that we are forced to check first frame setup logic like this, but egui_overlay doesn't expose
        // any setup/init interface
        if self.frame == 0 {
            // install image loaders
            egui_extras::install_image_loaders(egui_context);

            // don't show transparent window outline
            glfw_backend.window.set_decorated(false);
            glfw_backend.set_window_size([600.0, 50.0]);
            // anchor top left always
            glfw_backend.window.set_pos(0, 0);
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
            egui_context.request_repaint();
        }

        let curr_opacity = self.app_state.opacity.load(Ordering::Relaxed);
        if curr_opacity != self.overlay_opacity {
            self.overlay_opacity = curr_opacity;
            egui_context.request_repaint();
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
        egui::Window::new("recording overlay")
            .title_bar(false) // No title bar
            .resizable(false) // Non-resizable
            .scroll([false, false]) // Non-scrollable (both x and y)
            .collapsible(false) // Non-collapsible (removes collapse button)
            .anchor(Align2::LEFT_TOP, Vec2 { x: 10.0, y: 10.0 }) // Anchored to top-right corner
            .auto_sized()
            .frame(frame)
            .show(egui_context, |ui| {
                self.frame += 1;
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Image::new(egui::include_image!("../assets/owl.png"))
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
                                &game_exe,
                                0.0,
                                TextFormat {
                                    font_id: font_id.clone(),
                                    italics: true,
                                    color,
                                    ..Default::default()
                                },
                            );
                            job.append(
                                &format!(" ({}s)", start_time.elapsed().as_secs()),
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
