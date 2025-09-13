mod find_game;
mod hardware_id;
mod hardware_specs;
mod idle;
mod input_recorder;
mod keycode;
mod raw_input_debouncer;
mod recorder;
mod recording;

use std::{
    path::PathBuf, thread, time::{Duration, SystemTime, UNIX_EPOCH}
};

use clap::Parser;
use color_eyre::{eyre::eyre, Result};

use game_process::does_process_exist;
use raw_input::{PressState, RawInput};
use tokio::{
    sync::{mpsc, oneshot},
    time::MissedTickBehavior,
};

use crate::{
    idle::IdlenessTracker, keycode::lookup_keycode,
    raw_input_debouncer::EventDebouncer, recorder::Recorder,
};




#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(long)]
    recording_location: PathBuf,

    #[arg(long, default_value = "F4")]
    start_key: String,

    #[arg(long, default_value = "F5")]
    stop_key: String,
}

const MAX_IDLE_DURATION: Duration = Duration::from_secs(30);
const MAX_RECORDING_DURATION: Duration = Duration::from_secs(10 * 60);

use eframe::egui;
use egui::{Align2, Vec2, Color32, Rounding, Stroke};
use egui_overlay::EguiOverlay;
use egui_render_three_d::ThreeDBackend as DefaultGfxBackend;

use std::any::TypeId;
use std::sync::{Mutex, Arc, RwLock};
use tray_icon::{Icon, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOWDEFAULT};
use windows::Win32::{
    UI::WindowsAndMessaging::{SetWindowLongPtrW, GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TOOLWINDOW, WS_EX_APPWINDOW},
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};


// TODO: tray icon integration. rn it's a bit jank because it's counted as two instances of the the application, one for the overlay, and one for the main menu
// but the main issue is that the tray icon library when minimized actually has disgustingly high cpu usage for some reason? so putting that on hold while we work on
// main overlay first.
// TODOs: 
// Arc RWLock to transfer recording state to from main thread to overlay for display
// OBS bootstrapper deferred restart to client, now has to be restarted by egui app, which should be cleaner than wtv the fuck philpax did with listening to stdout
// Actually design the main app UI and link up all the buttons and stuff to look BETTER than the normal owl-recorder


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
    state: Arc<RwLock<RecordingStatus>>,
}

impl RecordingState {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(RecordingStatus::Stopped)),
        }
    }
}
static VISIBLE: Mutex<bool> = Mutex::new(true);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    color_eyre::install()?;
    tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();

    let recording_state = RecordingState::new();
    let cloned_state = recording_state.clone();
    // launch on seperate thread so non-blocking
    thread::spawn(move || {
        egui_overlay::start(OverlayApp { frame: 0, recording_state: cloned_state });
    });

    // thread::spawn(move || {
    //     _main();
    // });

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
                            let window_handle = HWND(handle.hwnd.into());
                            unsafe {
                                ShowWindow(window_handle, SW_HIDE);
                            }
                            *visible = false;
                        } else {
                            let window_handle = HWND(handle.hwnd.into());
                            unsafe {
                                ShowWindow(window_handle, SW_SHOWDEFAULT);
                            }
                            *visible = true;
                        }

                        context.request_repaint();
                    }
                    _ => return,
                }
            }));

            Ok(Box::new(MainApp { recording_state: cloned_state }))
        }),
    );

    Ok(())
}

pub struct OverlayApp {
    frame: u64,
    recording_state: RecordingState,
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

            // hide glfw overlay icon from taskbar and alt+tab
            let hwnd = glfw_backend.window.get_win32_window() as isize;
            if hwnd != 0 {
                unsafe {
                    let hwnd = HWND(hwnd);
                    let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                    ex_style |= WS_EX_TOOLWINDOW.0 as isize;  // Hide from taskbar
                    ex_style &= !(WS_EX_APPWINDOW.0 as isize); // Remove from Alt+Tab
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
                }
            }
        }

        let frame = egui::containers::Frame {
            fill: Color32::from_black_alpha(80),    // Transparent background
            stroke: Stroke::NONE,                // No border
            rounding: Rounding::ZERO,            // No rounded corners
            shadow: Default::default(),          // Default shadow settings
            inner_margin: egui::Margin::same(8.0), // Inner padding
            outer_margin: egui::Margin::ZERO,    // No outer margin
        };
        
        egui::Window::new("recording overlay").title_bar(false)                    // No title bar
            .resizable(false)                    // Non-resizable
            .scroll([false, false])             // Non-scrollable (both x and y)
            .collapsible(false)                  // Non-collapsible (removes collapse button)
            .anchor(Align2::LEFT_TOP, Vec2{x: 10.0, y: 10.0}) // Anchored to top-right corner
            .auto_sized()
            .frame(frame)
            .show(egui_context, |ui| {
                self.frame += 1;
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(egui::include_image!("../assets/owl.png"))
                                    .fit_to_exact_size(Vec2{x: 24.0, y: 24.0})
                                    .tint(Color32::from_white_alpha(50)));
                    ui.label(self.recording_state.state.read().unwrap().display_text());
                });
        });

        // don't show transparent window outline
        glfw_backend.window.set_decorated(false);
        glfw_backend.set_window_size([200.0, 50.0]);
        // anchor top left always
        glfw_backend.window.set_pos(0, 0);
        glfw_backend.window.maximize();
        // always allow input to passthrough
        glfw_backend.set_passthrough(true);

        egui_context.request_repaint();
        // update delay, not like it needs to be super responsive
        // and it also reduces cpu usage by a ton
        thread::sleep(Duration::from_millis(100));  
    }
}

pub struct MainApp {
    recording_state: RecordingState,
}
impl eframe::App for MainApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(egui::RichText::new("Settings").size(36.0).strong());
            ui.label(egui::RichText::new("Configure your recording preferences").size(20.0));
            if ui.button("Cycle Recording status").clicked() {
                // Proof of concept: how to share state between main thread and overlay
                let mut state = self.recording_state.state.write().unwrap();
                *state = match *state {
                    RecordingStatus::Stopped => RecordingStatus::Recording,
                    RecordingStatus::Recording => RecordingStatus::Paused,
                    RecordingStatus::Paused => RecordingStatus::Stopped,
                };
            }
            ui.add_space(10.0);
            ui.heading(self.recording_state.state.read().unwrap().display_text());
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(20.0);

                // Account Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Account").size(18.0).strong());
                    ui.separator();
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Username:");
                        ui.text_edit_singleline(&mut String::from("user@example.com"));
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Status:");
                        ui.label("Connected");
                    });
                    
                    ui.add_space(10.0);
                    if ui.button("Sign Out").clicked() {
                        // Handle sign out
                    }
                });

                ui.add_space(15.0);

                // OWL API Token Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("OWL API Token").size(18.0).strong());
                    ui.separator();
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("API Token:");
                        ui.text_edit_singleline(&mut String::from("••••••••••••••••"));
                    });
                    
                    ui.horizontal(|ui| {
                        if ui.button("Generate New Token").clicked() {
                            // Handle token generation
                        }
                        if ui.button("Revoke Token").clicked() {
                            // Handle token revocation
                        }
                    });
                    
                    ui.add_space(5.0);
                    ui.label(egui::RichText::new("Keep your API token secure and don't share it with others.").italics().color(egui::Color32::GRAY));
                });

                ui.add_space(15.0);

                // Upload Settings Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Upload Settings").size(18.0).strong());
                    ui.separator();
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Default Quality:");
                        egui::ComboBox::from_label("")
                            .selected_text("High")
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut "High", "High", "High");
                                ui.selectable_value(&mut "Medium", "Medium", "Medium");
                                ui.selectable_value(&mut "Low", "Low", "Low");
                            });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Auto-upload:");
                        ui.checkbox(&mut true, "Enable automatic uploads");
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Max file size (MB):");
                        ui.add(egui::Slider::new(&mut 100, 1..=1000));
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Allowed file types:");
                        ui.text_edit_singleline(&mut String::from(".jpg, .png, .gif, .mp4"));
                    });
                });

                ui.add_space(15.0);

                // Keyboard Shortcuts Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Keyboard Shortcuts").size(18.0).strong());
                    ui.separator();
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Upload file:");
                        ui.code("Ctrl+U");
                        if ui.small_button("Change").clicked() {
                            // Handle shortcut change
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Quick screenshot:");
                        ui.code("Ctrl+Shift+S");
                        if ui.small_button("Change").clicked() {
                            // Handle shortcut change
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Open settings:");
                        ui.code("Ctrl+,");
                        if ui.small_button("Change").clicked() {
                            // Handle shortcut change
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Toggle upload manager:");
                        ui.code("Ctrl+M");
                        if ui.small_button("Change").clicked() {
                            // Handle shortcut change
                        }
                    });
                    
                    ui.add_space(10.0);
                    if ui.button("Reset to Defaults").clicked() {
                        // Handle reset shortcuts
                    }
                });

                ui.add_space(15.0);

                // Upload Manager Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Upload Manager").size(18.0).strong());
                    ui.separator();
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Show notifications:");
                        ui.checkbox(&mut true, "Upload completed");
                        ui.checkbox(&mut true, "Upload failed");
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Concurrent uploads:");
                        ui.add(egui::Slider::new(&mut 3, 1..=10));
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Retry failed uploads:");
                        ui.checkbox(&mut true, "Auto-retry up to 3 times");
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("History retention:");
                        egui::ComboBox::from_label("")
                            .selected_text("30 days")
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut "30 days", "30 days", "30 days");
                                ui.selectable_value(&mut "90 days", "90 days", "90 days");
                                ui.selectable_value(&mut "1 year", "1 year", "1 year");
                                ui.selectable_value(&mut "Forever", "Forever", "Forever");
                            });
                    });
                    
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Clear Upload History").clicked() {
                            // Handle clear history
                        }
                        if ui.button("Export History").clicked() {
                            // Handle export history
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
                    }
                    if ui.button("Reset All").clicked() {
                        // Handle reset all settings
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("Settings are automatically saved").italics().color(egui::Color32::GRAY));
                    });
                });
            });
        });
    }
}

#[tokio::main]
async fn _main() -> Result<()> {
    // color_eyre::install()?;
    // tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();

    let Args {
        recording_location,
        start_key,
        stop_key,
    } = Args::parse();

    let start_key =
        lookup_keycode(&start_key).ok_or_else(|| eyre!("Invalid start key: {start_key}"))?;
    let stop_key =
        lookup_keycode(&stop_key).ok_or_else(|| eyre!("Invalid stop key: {stop_key}"))?;

    let mut recorder = Recorder::new({
        let recording_location = recording_location.clone();
        move || recording_location.join(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .to_string(),
        )
    }).await?;

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
                if let Some(key) = keycode_from_event(&e) {
                    if key == start_key {
                        tracing::info!("Start key pressed, starting recording");
                        recorder.start().await?;
                    } else if key == stop_key {
                        tracing::info!("Stop key pressed, stopping recording");
                        recorder.stop().await?;
                        start_on_activity = false;
                    }
                } else if start_on_activity {
                    tracing::info!("Input detected, restarting recording");
                    recorder.start().await?;
                    start_on_activity = false;
                }
                idleness_tracker.update_activity();
            },
            _ = perform_checks.tick() => {
                if let Some(recording) = recorder.recording() {
                    if !does_process_exist(recording.pid())? {
                        tracing::info!(pid=recording.pid().0, "Game process no longer exists, stopping recording");
                        recorder.stop().await?;
                    } else if idleness_tracker.is_idle() {
                        tracing::info!("No input detected for 5 seconds, stopping recording");
                        recorder.stop().await?;
                        start_on_activity = true;
                    } else if recording.elapsed() > MAX_RECORDING_DURATION {
                        tracing::info!("Recording duration exceeded {} s, restarting recording", MAX_RECORDING_DURATION.as_secs());
                        recorder.stop().await?;
                        recorder.start().await?;
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
