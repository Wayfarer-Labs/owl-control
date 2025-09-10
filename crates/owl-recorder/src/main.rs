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
    path::PathBuf,
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

// use eframe::egui;
use egui::DragValue;
use egui_overlay::EguiOverlay;
use egui_render_three_d::ThreeDBackend as DefaultGfxBackend;

use std::sync::Mutex;
use tray_icon::{Icon, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOWDEFAULT};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};


// TODO: integration with https://github.com/coderedart/egui_overlay/blob/master/src/lib.rs
// use https://docs.rs/egui_window_glfw_passthrough/latest/egui_window_glfw_passthrough/ directly instead of through egui_overlay


static VISIBLE: Mutex<bool> = Mutex::new(true);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    egui_overlay::start(HelloWorld { frame: 0 });

    /*
    let mut icon_data: Vec<u8> = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..256 {
        // all red
        icon_data.extend_from_slice(&[255, 0, 0, 255]);
    }
    let icon = Icon::from_rgba(icon_data, 16, 16)?;
    let _tray_icon = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("My App")
        .build()?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "My egui App",
        options,
        Box::new(|cc| {
            let RawWindowHandle::Win32(handle) = cc.window_handle().unwrap().as_raw() else {
                panic!("Unsupported platform");
            };

            // let context = cc.egui_ctx.clone();

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

                        // context.request_repaint();
                    }
                    _ => return,
                }
            }));

            Box::new(MyApp::default())
        }),
    );
    */
    Ok(())
}

pub struct HelloWorld {
    pub frame: u64,
}
impl EguiOverlay for HelloWorld {
    fn gui_run(
        &mut self,
        egui_context: &egui::Context,
        _default_gfx_backend: &mut DefaultGfxBackend,
        glfw_backend: &mut egui_window_glfw_passthrough::GlfwBackend,
    ) {
        // just some controls to show how you can use glfw_backend
        egui::Window::new("controls").show(egui_context, |ui| {
            ui.set_width(300.0);
            self.frame += 1;
            ui.label(format!("current frame number: {}", self.frame));
            // sometimes, you want to see the borders to understand where the overlay is.
            let mut borders = glfw_backend.window.is_decorated();
            if ui.checkbox(&mut borders, "window borders").changed() {
                glfw_backend.window.set_decorated(borders);
            }

            ui.label(format!(
                "pixels_per_virtual_unit: {}",
                glfw_backend.physical_pixels_per_virtual_unit
            ));
            ui.label(format!("window scale: {}", glfw_backend.scale));
            ui.label(format!("cursor pos x: {}", glfw_backend.cursor_pos[0]));
            ui.label(format!("cursor pos y: {}", glfw_backend.cursor_pos[1]));

            ui.label(format!(
                "passthrough: {}",
                glfw_backend.window.is_mouse_passthrough()
            ));
            // how to change size.
            // WARNING: don't use drag value, because window size changing while dragging ui messes things up.
            let mut size = glfw_backend.window_size_logical;
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("width: ");
                ui.add_enabled(false, DragValue::new(&mut size[0]));
                if ui.button("inc").clicked() {
                    size[0] += 10.0;
                    changed = true;
                }
                if ui.button("dec").clicked() {
                    size[0] -= 10.0;
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("height: ");
                ui.add_enabled(false, DragValue::new(&mut size[1]));
                if ui.button("inc").clicked() {
                    size[1] += 10.0;
                    changed = true;
                }
                if ui.button("dec").clicked() {
                    size[1] -= 10.0;
                    changed = true;
                }
            });
            if changed {
                glfw_backend.set_window_size(size);
            }
            // how to change size.
            // WARNING: don't use drag value, because window size changing while dragging ui messes things up.
            let mut pos = glfw_backend.window_position;
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("x: ");
                ui.add_enabled(false, DragValue::new(&mut pos[0]));
                if ui.button("inc").clicked() {
                    pos[0] += 10;
                    changed = true;
                }
                if ui.button("dec").clicked() {
                    pos[0] -= 10;
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("y: ");
                ui.add_enabled(false, DragValue::new(&mut pos[1]));
                if ui.button("inc").clicked() {
                    pos[1] += 10;
                    changed = true;
                }
                if ui.button("dec").clicked() {
                    pos[1] -= 10;
                    changed = true;
                }
            });
            if changed {
                glfw_backend.window.set_pos(pos[0], pos[1]);
            }
        });

        // here you decide if you want to be passthrough or not.
        if egui_context.wants_pointer_input() || egui_context.wants_keyboard_input() {
            // we need input, so we need the window to be NOT passthrough
            glfw_backend.set_passthrough(false);
        } else {
            // we don't care about input, so the window can be passthrough now
            glfw_backend.set_passthrough(true)
        }
        egui_context.request_repaint();
    }
}
/*
struct MyApp {
    // Your app state here
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            // Initialize your app state here
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello World!");
        });
    }
}
*/

#[tokio::main]
async fn _main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();

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
