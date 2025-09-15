mod config_manager;
mod find_game;
mod hardware_id;
mod hardware_specs;
mod idle;
mod input_recorder;
mod keycode;
mod raw_input_debouncer;
mod recorder;
mod recording;
mod upload_manager;
mod window_recorder;

use std::{
    path::PathBuf,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use color_eyre::{Result, eyre::eyre};

use game_process::does_process_exist;
use raw_input::{PressState, RawInput};
use tokio::{
    sync::{mpsc, oneshot},
    time::MissedTickBehavior,
};

use crate::{
    config_manager::{ConfigManager, Credentials, Preferences},
    idle::IdlenessTracker,
    keycode::lookup_keycode,
    raw_input_debouncer::EventDebouncer,
    recorder::Recorder,
    upload_manager::{is_upload_bridge_running_async, start_upload_bridge_async},
};

use eframe::egui;
use egui::{Align2, Color32, RichText, Rounding, Stroke, Vec2};
use egui_overlay::EguiOverlay;
use egui_render_three_d::ThreeDBackend as DefaultGfxBackend;

use std::sync::{Arc, Mutex, RwLock};
use tray_icon::{MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWDEFAULT, ShowWindow};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    // This is set because I'm lazy to specify cargo run location every time. And also because ObsContext::spawn_updater() breaks otherwise.
    // I suspect that the libobs bootstrapper ObsContext::spawn_updater() which restarts the app after is bugged and doesn't accept args correctly.
    // If run with a specified default value it doesn't call the relative path correctly on the restart, leading to the restarted app crashing immediately.
    // But if you just run as is "cargo run" without params it will restart properly.
    #[arg(long, default_value = "./data_dump/games")]
    recording_location: PathBuf,

    #[arg(long, default_value = "F4")]
    start_key: String,

    #[arg(long, default_value = "F5")]
    stop_key: String,
}

const MAX_IDLE_DURATION: Duration = Duration::from_secs(90);
const MAX_RECORDING_DURATION: Duration = Duration::from_secs(10 * 60);

// lots of repeated code to just load bytes, especially tray_icon needs different type, so use a macro here
macro_rules! load_icon_from_bytes {
    (@internal $rgba:expr, $width:expr, $height:expr, egui_icon) => {
        egui::IconData { rgba: $rgba, width: $width, height: $height }
    };

    (@internal $rgba:expr, $width:expr, $height:expr, tray_icon) => {
        tray_icon::Icon::from_rgba($rgba, $width, $height)
            .expect("Failed to create tray icon")
    };

    ($path:literal, $icon_type:ident) => {{
        const ICON_BYTES: &[u8] = include_bytes!($path);
        let image = image::load_from_memory(ICON_BYTES)
            .expect("Failed to load embedded icon")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        load_icon_from_bytes!(@internal rgba, width, height, $icon_type)
    }};
}

#[derive(Clone, Copy, PartialEq)]
enum RecordingStatus {
    Stopped,
    Recording,
    Paused,
}

impl RecordingStatus {
    pub fn display_text(&self) -> &str {
        match *self {
            RecordingStatus::Stopped => "Stopped",
            RecordingStatus::Recording => "Recording...",
            RecordingStatus::Paused => "Paused",
        }
    }
}

#[derive(Clone)]
struct RecordingState {
    // holds the current state of recording, recorder <-> overlay
    state: Arc<RwLock<RecordingStatus>>,
    // setting for opacity of overlay, main app <-> overlay
    opacity: Arc<RwLock<u8>>,
    // bootstrap progress bar, recorder <-> main app
    boostrap_progress: Arc<RwLock<f32>>,
}

impl RecordingState {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(RecordingStatus::Stopped)),
            opacity: Arc::new(RwLock::new(85)),
            boostrap_progress: Arc::new(RwLock::new(0.0)),
        }
    }
}
static VISIBLE: Mutex<bool> = Mutex::new(true);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let recording_state = RecordingState::new();

    // launch overlay on seperate thread so non-blocking
    let cloned_state = recording_state.clone();
    thread::spawn(move || {
        egui_overlay::start(OverlayApp::new(cloned_state));
    });

    // launch recorder on seperate thread so non-blocking
    let cloned_state = recording_state.clone();
    thread::spawn(move || {
        let _ = _main(cloned_state);
    });

    // main app built here on main thread, so if it's closed by user the entire program is killed
    let _tray_icon = TrayIconBuilder::new()
        .with_icon(load_icon_from_bytes!("../assets/owl-logo.png", tray_icon))
        .with_tooltip("Owl Control")
        .build()?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 600.0])
            .with_resizable(false)
            .with_title("Owl Control")
            .with_icon(load_icon_from_bytes!("../assets/owl-logo.png", egui_icon)),
        ..Default::default()
    };

    let cloned_state = recording_state.clone();
    let _ = eframe::run_native(
        "Owl Control",
        options,
        Box::new(move |cc| {
            let RawWindowHandle::Win32(handle) = cc.window_handle().unwrap().as_raw() else {
                panic!("Unsupported platform");
            };

            let context = cc.egui_ctx.clone();

            TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
                // println!("TrayIconEvent: {:?}", event);

                match event {
                    TrayIconEvent::Click {
                        button_state: MouseButtonState::Down,
                        ..
                    } => {
                        let mut visible = VISIBLE.lock().unwrap();

                        if *visible {
                            let window_handle = HWND(handle.hwnd.get() as *mut std::ffi::c_void);
                            unsafe {
                                let _ = ShowWindow(window_handle, SW_HIDE);
                            }
                            *visible = false;
                        } else {
                            let window_handle = HWND(handle.hwnd.get() as *mut std::ffi::c_void);
                            unsafe {
                                let _ = ShowWindow(window_handle, SW_SHOWDEFAULT);
                            }
                            *visible = true;
                        }

                        context.request_repaint();
                    }
                    _ => return,
                }
            }));

            Ok(Box::new(MainApp::new(cloned_state).unwrap()))
        }),
    );

    Ok(())
}

pub struct OverlayApp {
    frame: u64,
    recording_state: RecordingState,
    overlay_opacity: u8,         // local opacity tracker
    rec_status: RecordingStatus, // local rec status
}
impl OverlayApp {
    fn new(recording_state: RecordingState) -> Self {
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
                    let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                    ex_style |= WS_EX_TOOLWINDOW.0 as isize; // Hide from taskbar
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
            rounding: Rounding::ZERO,                              // No rounded corners
            shadow: Default::default(),                            // Default shadow settings
            inner_margin: egui::Margin::same(8.0),                 // Inner padding
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

pub struct MainApp {
    recording_state: RecordingState,
    frame: u64,
    cached_progress: f32,           // from 0-1
    configs: ConfigManager,         // this is the cache that is actually saved to file
    local_credentials: Credentials, // local copy of the settings that is used to track
    local_preferences: Preferences, // user inputs before being saved to the ConfigManager
}
impl MainApp {
    fn new(recording_state: RecordingState) -> Result<Self, Box<dyn std::error::Error>> {
        Ok({
            let cm = ConfigManager::new()?;
            let local_credentials = cm.credentials.clone();
            let local_preferences = cm.preferences.clone();
            // write the cached overlay opacity
            *recording_state.opacity.write().unwrap() = local_preferences.overlay_opacity;
            Self {
                recording_state,
                frame: 0,
                cached_progress: 0.0,
                configs: cm,
                local_credentials: local_credentials,
                local_preferences: local_preferences,
            }
        })
    }
}
impl eframe::App for MainApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(egui::RichText::new("Settings").size(36.0).strong());
            ui.label(egui::RichText::new("Configure your recording preferences").size(20.0));
            ui.add_space(10.0);

            // progress bar for obs bootstrapper
            if let Ok(progress) = self.recording_state.boostrap_progress.try_read() {
                self.cached_progress = *progress;
            }
            if self.cached_progress <= 1.0 {
                ui.add(egui::ProgressBar::new(self.cached_progress).text("Loading OBS..."));
                ctx.request_repaint();
            };
            ui.add_space(10.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                // OWL API Token Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("OWL API Token").size(18.0).strong());
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        ui.label("API Token:");
                        ui.add_sized(
                            [400.0, 15.0],
                            egui::TextEdit::singleline(&mut self.local_credentials.api_key),
                        );
                    });

                    ui.add_space(5.0);
                    ui.label(
                        egui::RichText::new(
                            "Keep your API token secure and don't share it with others.",
                        )
                        .italics()
                        .color(egui::Color32::GRAY),
                    );
                });
                ui.add_space(15.0);

                // Upload Settings Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Upload Settings").size(18.0).strong());
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        ui.label("Default Quality:");
                        ui.checkbox(
                            &mut self.local_preferences.delete_uploaded_files,
                            "Delete local files after successful upload",
                        )
                    });
                });
                ui.add_space(15.0);

                // Keyboard Shortcuts Section
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("Keyboard Shortcuts")
                            .size(18.0)
                            .strong(),
                    );
                    ui.separator();
                    ui.add_space(10.0);

                    // TODO: eventually implement a better keyboard shortcut system
                    ui.horizontal(|ui| {
                        ui.label("Start Recording:");
                        // ui.code(&mut self.local_preferences.start_recording_key);
                        ui.add_sized(
                            [60.0, 15.0],
                            egui::TextEdit::singleline(
                                &mut self.local_preferences.start_recording_key,
                            ),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.label("Stop Recording:");
                        // ui.code(&mut self.local_preferences.stop_recording_key);
                        ui.add_sized(
                            [60.0, 15.0],
                            egui::TextEdit::singleline(
                                &mut self.local_preferences.stop_recording_key,
                            ),
                        );
                    });
                });
                ui.add_space(15.0);

                // Overlay Settings Section
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("Overlay Customization")
                            .size(18.0)
                            .strong(),
                    );
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        ui.label("Opacity:");
                        let mut opacity_guard = self.recording_state.opacity.write().unwrap();
                        self.local_preferences.overlay_opacity = *opacity_guard;
                        ui.add(egui::Slider::new(&mut *opacity_guard, 1..=255));
                    });
                });

                ui.add_space(15.0);

                // Upload Manager Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Upload Manager").size(18.0).strong());
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui.button("Upload Recordings").clicked() {
                            // Handle upload
                            if !is_upload_bridge_running_async() {
                                let api_key = self.local_credentials.api_key.clone();
                                tokio::spawn(async move {
                                    start_upload_bridge_async(&api_key).await;
                                });
                            }
                        }
                    });
                });

                ui.add_space(30.0);

                // Save/Reset buttons at the bottom
                ui.separator();
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Save Settings").clicked() {
                        // Handle save settings
                        self.configs.credentials = self.local_credentials.clone();
                        self.configs.preferences = self.local_preferences.clone();
                        let _ = self.configs.save_config();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("Wayfarer Labs")
                                .italics()
                                .color(egui::Color32::GRAY),
                        );
                    });
                });
            });
        });
        self.frame += 1;
    }
}

#[tokio::main]
async fn _main(recording_state: RecordingState) -> Result<()> {
    let Args {
        recording_location,
        start_key,
        stop_key,
    } = Args::parse();

    let start_key =
        lookup_keycode(&start_key).ok_or_else(|| eyre!("Invalid start key: {start_key}"))?;
    let stop_key =
        lookup_keycode(&stop_key).ok_or_else(|| eyre!("Invalid stop key: {stop_key}"))?;

    let mut recorder = match Recorder::new(
        || {
            recording_location.join(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string(),
            )
        },
        recording_state.boostrap_progress.clone(),
    )
    .await
    {
        Ok(recorder) => recorder,
        Err(e) => {
            // so technically the best practice would be to create a custom error type and check that, but cba make it just to use it once
            if e.to_string()
                .contains("OBS restart required during initialization")
            {
                // Defer the restart to the ObsContext::spawn_updater(). All we have to do is kill the main thread.
                tracing::info!("Restarting OBS!");
                // give it a sec to cleanup, no sense wasting the progress bar visuals either ;p
                *recording_state.boostrap_progress.write().unwrap() = 1.0;
                std::thread::sleep(Duration::from_secs(1));
                std::process::exit(0);
            } else {
                // Handle other errors
                tracing::error!(e=?e, "Failed to initialize recorder");
                return Err(e);
            }
        }
    };

    // give it a moment for the user to see that loading has actually completed
    std::thread::sleep(Duration::from_millis(300));
    *recording_state.boostrap_progress.write().unwrap() = 1.337;

    tracing::info!("recorder initialized");
    let mut input_rx = listen_for_raw_inputs();

    let mut stop_rx = wait_for_ctrl_c();

    let mut idleness_tracker = IdlenessTracker::new(MAX_IDLE_DURATION);
    let mut start_on_activity = false;

    let mut perform_checks = tokio::time::interval(Duration::from_secs(1));
    perform_checks.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            r = &mut stop_rx => {
                r.expect("signal handler was closed early");
                break;
            },
            e = input_rx.recv() => {
                let e = e.expect("raw input reader was closed early");
                recorder.seen_input(e).await?;
                let mut state_writer = recording_state.state.write().unwrap();
                if let Some(key) = keycode_from_event(&e) {
                    if key == start_key {
                        tracing::info!("Start key pressed, starting recording");
                        recorder.start().await?;
                        *state_writer = RecordingStatus::Recording;
                    } else if key == stop_key {
                        tracing::info!("Stop key pressed, stopping recording");
                        recorder.stop().await?;
                        *state_writer = RecordingStatus::Stopped;
                        start_on_activity = false;
                    }
                } else if start_on_activity {
                    tracing::info!("Input detected, restarting recording");
                    recorder.start().await?;
                        *state_writer = RecordingStatus::Recording;
                    start_on_activity = false;
                }
                idleness_tracker.update_activity();
            },
            _ = perform_checks.tick() => {
                if let Some(recording) = recorder.recording() {
                    let mut state_writer = recording_state.state.write().unwrap();
                    if !does_process_exist(recording.pid())? {
                        tracing::info!(pid=recording.pid().0, "Game process no longer exists, stopping recording");
                        recorder.stop().await?;
                        *state_writer = RecordingStatus::Stopped;
                    } else if idleness_tracker.is_idle() {
                        tracing::info!("No input detected for 5 seconds, stopping recording");
                        recorder.stop().await?;
                        *state_writer = RecordingStatus::Paused;
                        start_on_activity = true;
                    } else if recording.elapsed() > MAX_RECORDING_DURATION {
                        tracing::info!("Recording duration exceeded {} s, restarting recording", MAX_RECORDING_DURATION.as_secs());
                        recorder.stop().await?;
                        *state_writer = RecordingStatus::Stopped;
                        recorder.start().await?;
                        *state_writer = RecordingStatus::Recording;
                        idleness_tracker.update_activity();
                    };
                }
            },
        }
    }

    recorder.stop().await?;

    Ok(())
}

fn keycode_from_event(event: &raw_input::Event) -> Option<u16> {
    if let raw_input::Event::KeyPress {
        key,
        press_state: PressState::Pressed,
    } = event
    {
        Some(*key)
    } else {
        None
    }
}

fn listen_for_raw_inputs() -> mpsc::Receiver<raw_input::Event> {
    let (input_tx, input_rx) = mpsc::channel(1);

    std::thread::spawn(move || {
        let mut raw_input = Some(RawInput::initialize().expect("raw input failed to initialize"));
        let mut debouncer = EventDebouncer::new();

        RawInput::run_queue(|event| {
            if !debouncer.debounce(event) {
                return;
            }
            if input_tx.blocking_send(event).is_err() {
                tracing::debug!("Input channel closed, stopping raw input listener");
                raw_input.take();
            }
        })
        .expect("failed to run windows message queue");
    });
    input_rx
}

fn wait_for_ctrl_c() -> oneshot::Receiver<()> {
    let (ctrl_c_tx, ctrl_c_rx) = oneshot::channel();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for Ctrl+C signal");
        let _ = ctrl_c_tx.send(());
    });
    ctrl_c_rx
}
