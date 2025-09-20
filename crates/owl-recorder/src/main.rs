mod app_icon;
mod bootstrap_recorder;
mod config_manager;
mod find_game;
mod hardware_id;
mod hardware_specs;
mod idle;
mod input_recorder;
mod keycode;
mod obs_socket_recorder;
mod overlay;
mod raw_input_debouncer;
mod recorder;
mod recording;
mod recording_thread;
mod upload_manager;

use std::{
    env,
    fs::File,
    io::{BufReader, Cursor},
    path::PathBuf,
    sync::{
        RwLock,
        atomic::{AtomicU8, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use color_eyre::Result;
use obws::client::Config;

use crate::{
    app_icon::set_app_icon_windows,
    config_manager::{ConfigManager, Credentials, Preferences, UploadStats},
    overlay::OverlayApp,
    upload_manager::{is_upload_bridge_running, start_upload_bridge},
};

use eframe::egui;
use egui::ViewportCommand;

use rodio::{Decoder, OutputStream, Sink};
use std::sync::{Arc, Mutex};
use tray_icon::{
    MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};
use windows::Win32::Foundation::HWND;
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
// const MAX_RECORDING_DURATION: Duration = Duration::from_secs(10);

// lots of repeated code to just load bytes, especially tray_icon needs different type, so use a macro here
macro_rules! load_icon_from_bytes {
    (@internal $rgba:expr, $width:expr, $height:expr, egui_icon) => {
        egui::IconData { rgba: $rgba, width: $width, height: $height }
    };

    (@internal $rgba:expr, $width:expr, $height:expr, tray_icon) => {
        tray_icon::Icon::from_rgba($rgba, $width, $height)
            .expect("Failed to create tray icon")
    };

    ($bytes:expr, $icon_type:ident) => {{
        let image = image::load_from_memory($bytes)
            .expect("Failed to load embedded icon")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        load_icon_from_bytes!(@internal rgba, width, height, $icon_type)
    }};
}
pub(crate) use load_icon_from_bytes;

const LOGO_DEFAULT_BYTES: &[u8] = include_bytes!("../assets/owl-logo.png");
const LOGO_RECORDING_BYTES: &[u8] = include_bytes!("../assets/owl-logo-recording.png");
const HONK_0_BYTES: &[u8] = include_bytes!("../assets/goose_honk0.mp3");
const HONK_1_BYTES: &[u8] = include_bytes!("../assets/goose_honk1.mp3");

#[derive(Clone, PartialEq)]
enum RecordingStatus {
    Stopped,
    Recording {
        start_time: Instant,
        game_exe: String,
    },
    Paused,
}

struct AppState {
    state: RwLock<RecordingStatus>, // holds the current state of recording, recorder <-> overlay
    opacity: AtomicU8,              // setting for opacity of overlay, main app <-> overlay
    boostrap_progress: RwLock<f32>, // bootstrap progress bar, recorder <-> main app
    configs: RwLock<ConfigManager>,
    sink: Sink,                   // for honking
    _stream_handle: OutputStream, // stream handle needs to stay alive for the sink to play audio
}

impl AppState {
    pub fn new() -> Self {
        let stream_handle =
            rodio::OutputStreamBuilder::open_default_stream().expect("open default audio stream");
        let sink = Sink::connect_new(&stream_handle.mixer());
        Self {
            state: RwLock::new(RecordingStatus::Stopped),
            opacity: AtomicU8::new(85),
            boostrap_progress: RwLock::new(0.0),
            configs: RwLock::new(ConfigManager::new().expect("failed to init configs")),
            sink,
            _stream_handle: stream_handle,
        }
    }
}
static VISIBLE: Mutex<bool> = Mutex::new(true);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args {
        recording_location,
        start_key,
        stop_key,
    } = Args::parse();

    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let app_state = Arc::new(AppState::new());

    // launch overlay on seperate thread so non-blocking
    let cloned_state = app_state.clone();
    thread::spawn(move || {
        egui_overlay::start(OverlayApp::new(cloned_state));
    });

    // main app built here on main thread, so if it's closed by user the entire program is killed
    // tray icon right click menu for quit option
    let quit_item = MenuItem::new("Quit", true, None);
    let quit_item_id = quit_item.id().clone();
    let tray_menu = Menu::new();
    let _ = tray_menu.append(&quit_item);
    // create tray icon
    let _tray_icon = TrayIconBuilder::new()
        .with_icon(load_icon_from_bytes!(LOGO_DEFAULT_BYTES, tray_icon))
        .with_tooltip("OWL Control")
        .with_menu(Box::new(tray_menu))
        .build()
        .unwrap();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        match event.id() {
            id if id == &quit_item_id => {
                // Close the application
                // TODO: probably should be a more graceful way to close the app?
                // probably need a atomicbool with close flag so we can close via
                // context_menu.send_viewport_cmd(egui::ViewportCommand::Close);
                // and then also clean up any uploading video process in progress.
                std::process::exit(0);
            }
            _ => {}
        }
    }));

    // launch recorder on seperate thread so non-blocking
    let cloned_state = app_state.clone();
    thread::spawn(move || {
        recording_thread::run(cloned_state, start_key, stop_key, recording_location).unwrap();
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 600.0])
            .with_resizable(false)
            .with_title("OWL Control")
            .with_icon(load_icon_from_bytes!(LOGO_DEFAULT_BYTES, egui_icon)),
        ..Default::default()
    };

    let cloned_state = app_state.clone();
    let _ = eframe::run_native(
        "OWL Control",
        options,
        Box::new(move |cc| {
            let RawWindowHandle::Win32(handle) = cc.window_handle().unwrap().as_raw() else {
                panic!("Unsupported platform");
            };

            let context = cc.egui_ctx.clone();
            TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
                match event {
                    TrayIconEvent::Click {
                        button: tray_icon::MouseButton::Left,
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
                            // set viewport visible true in case it was minimised to tray via closing the app
                            context.send_viewport_cmd(egui::ViewportCommand::Visible(true));
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

pub struct MainApp {
    app_state: Arc<AppState>,
    frame: u64,
    cached_progress: f32,           // from 0-1
    local_credentials: Credentials, // local copy of the settings that is used to track
    local_preferences: Preferences, // user inputs before being saved to the ConfigManager
    upload_stats: UploadStats,
}
impl MainApp {
    fn new(app_state: Arc<AppState>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok({
            let local_credentials: Credentials;
            let local_preferences: Preferences;
            {
                let configs = app_state.configs.read().unwrap();
                local_credentials = configs.credentials.clone();
                local_preferences = configs.preferences.clone();
            }
            // write the cached overlay opacity
            app_state
                .opacity
                .store(local_preferences.overlay_opacity, Ordering::Relaxed);
            Self {
                app_state,
                frame: 0,
                cached_progress: 0.0,
                local_credentials,
                local_preferences,
                upload_stats: UploadStats::new()?,
            }
        })
    }
}
impl eframe::App for MainApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        // if user closes the app instead minimize to tray
        if ctx.input(|i| i.viewport().close_requested()) {
            let mut visible = VISIBLE.lock().unwrap();
            *visible = false;
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(egui::RichText::new("Settings").size(36.0).strong());
            ui.label(egui::RichText::new("Configure your recording preferences").size(20.0));
            ui.add_space(10.0);

            // progress bar for obs bootstrapper
            if let Ok(progress) = self.app_state.boostrap_progress.try_read() {
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

                // Keyboard Shortcuts Section
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("Keyboard Shortcuts")
                            .size(18.0)
                            .strong(),
                    );
                    ui.separator();

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
                        egui::RichText::new("Recorder Customization")
                            .size(18.0)
                            .strong(),
                    );
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("Overlay Opacity:");
                        let mut stored_opacity = self.app_state.opacity.load(Ordering::Relaxed);

                        let mut egui_opacity = stored_opacity as f32 / 255.0 * 100.0;
                        ui.add(
                            egui::Slider::new(&mut egui_opacity, 0.0..=100.0)
                                .suffix("%")
                                .integer(),
                        );

                        stored_opacity = (egui_opacity / 100.0 * 255.0) as u8;
                        self.app_state
                            .opacity
                            .store(stored_opacity as u8, Ordering::Relaxed);
                        self.local_preferences.overlay_opacity = stored_opacity;
                    });

                    ui.horizontal(|ui| {
                        ui.label("Recording Audio Cue:");
                        let honk = self.local_preferences.honk;
                        ui.add(egui::Checkbox::new(
                            &mut self.local_preferences.honk,
                            match honk {
                                true => "Honk.",
                                false => "Honk?",
                            },
                        ));
                    })
                });

                ui.add_space(15.0);

                // Upload Manager Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Upload Manager").size(18.0).strong());
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        let available_width = ui.available_width() - 40.0;
                        let cell_width = available_width / 4.0;

                        // Cell 1: Total Uploaded
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                self.create_upload_cell(
                                    ui,
                                    "📊", // Icon
                                    "Total Uploaded",
                                    &self.upload_stats.get_total_duration_uploaded(),
                                );
                            },
                        );

                        // Cell 2: Files Uploaded
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                self.create_upload_cell(
                                    ui,
                                    "📁", // Icon
                                    "Files Uploaded",
                                    &self.upload_stats.get_total_files_uploaded(),
                                );
                            },
                        );

                        // Cell 3: Volume Uploaded
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                self.create_upload_cell(
                                    ui,
                                    "💾", // Icon
                                    "Volume Uploaded",
                                    &self.upload_stats.get_total_volume_uploaded(),
                                );
                            },
                        );

                        // Cell 4: Last Upload
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                self.create_upload_cell(
                                    ui,
                                    "🕒", // Icon
                                    "Last Upload",
                                    &self.upload_stats.get_last_upload_date(),
                                );
                            },
                        );
                    });

                    ui.add_space(15.0);
                    ui.centered_and_justified(|ui| {
                        if ui
                            .button(egui::RichText::new("Upload Recordings").size(12.0).strong())
                            .clicked()
                        {
                            // Handle upload
                            if !is_upload_bridge_running() {
                                let api_key = self.local_credentials.api_key.clone();
                                std::thread::spawn(move || {
                                    start_upload_bridge(&api_key);
                                });
                            }
                        }
                    });
                });

                // Save/Reset buttons at the bottom
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Save Settings").clicked() {
                        // Handle save settings
                        let mut configs = self.app_state.configs.write().unwrap();
                        configs.credentials = self.local_credentials.clone();
                        configs.preferences = self.local_preferences.clone();
                        let _ = configs.save_config();
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
impl MainApp {
    // constructor for each cell in upload manager section
    fn create_upload_cell(&self, ui: &mut egui::Ui, icon: &str, title: &str, value: &str) {
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

    fn get_upload_stats(&self) {
        let mut path = env::temp_dir();
        path.push("owl-control-upload-stats.json");
    }
}
