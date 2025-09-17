use std::sync::Arc;

use egui::{Align2, Color32, RichText, Stroke, Vec2};
use egui_overlay::EguiOverlay;
use egui_render_three_d::ThreeDBackend as DefaultGfxBackend;
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        FLASHW_STOP, FLASHWINFO, FlashWindowEx, GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW,
        WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    },
};

use crate::{RecordingState, RecordingStatus};

pub struct OverlayApp {
    frame: u64,
    recording_state: Arc<RecordingState>,
    overlay_opacity: u8,         // local opacity tracker
    rec_status: RecordingStatus, // local rec status
}
impl OverlayApp {
    pub fn new(recording_state: Arc<RecordingState>) -> Self {
        let overlay_opacity = *recording_state.opacity.read().unwrap();
        let rec_status = *recording_state.state.read().unwrap();
        Self {
            frame: 0,
            recording_state,
            overlay_opacity,
            rec_status,
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
            glfw_backend.set_window_size([200.0, 50.0]);
            // anchor top left always
            glfw_backend.window.set_pos(0, 0);
            // always allow input to passthrough
            glfw_backend.set_passthrough(true);
            // hide glfw overlay icon from taskbar and alt+tab
            let hwnd = glfw_backend.window.get_win32_window() as isize;
            if hwnd != 0 {
                unsafe {
                    let hwnd = HWND(hwnd as *mut std::ffi::c_void);

                    // TODO: there is a bug with egui overlay start where if the user is alt tabbed at the moment
                    // that the app is started, the overlay will be permanently unminimizable and highlighted in the
                    // task bar. Idk how to fix that, for now I have fixed the highlighting part here.
                    let flash_info = FLASHWINFO {
                        cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                        hwnd,
                        dwFlags: FLASHW_STOP,
                        uCount: 0,
                        dwTimeout: 0,
                    };
                    let _ = FlashWindowEx(&flash_info);

                    let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                    ex_style |= WS_EX_TOOLWINDOW.0 as isize; // Hide from taskbar
                    ex_style |= WS_EX_NOACTIVATE.0 as isize; // Don't steal focus
                    ex_style &= !(WS_EX_APPWINDOW.0 as isize); // Remove from Alt+Tab
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
                }
            }
            egui_context.request_repaint();
        }

        let curr_opacity = *self.recording_state.opacity.read().unwrap();
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

        // only repaint the window when recording state changes, saves more cpu
        let curr_state = *self.recording_state.state.read().unwrap();
        if curr_state != self.rec_status {
            self.rec_status = curr_state;
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
                    ui.label(
                        RichText::new(self.rec_status.display_text())
                            .size(12.0)
                            .color(Color32::from_white_alpha(self.overlay_opacity)),
                    );
                });
            });
    }
}
