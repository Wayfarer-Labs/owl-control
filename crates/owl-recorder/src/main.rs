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

use std::sync::Mutex;
use tray_icon::{Icon, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOWDEFAULT};
use windows::Win32::{
    UI::WindowsAndMessaging::{SetWindowLongPtrW, GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TOOLWINDOW, WS_EX_APPWINDOW},
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};


// TODO: integration with https://github.com/coderedart/egui_overlay/blob/master/src/lib.rs
// TODO: tray icon integration. rn it's a bit jank because it's counted as two instances of the the application, one for the overlay, and one for the main menu
// but the main issue is that the tray icon library when minimized actually has disgustingly high cpu usage for some reason? so putting that on hold while we work on
// main overlay first.
// TODOs: 
// Arc RWLock to transfer recording state to from main thread to overlay for display
// OBS bootstrapper deferred restart to client, now has to be restarted by egui app, which should be cleaner than wtv the fuck philpax did with listening to stdout
// Actually design the main app UI and link up all the buttons and stuff to look BETTER than the normal owl-recorder

static VISIBLE: Mutex<bool> = Mutex::new(true);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    color_eyre::install()?;
    tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();

    // launch on seperate thread so non-blocking
    thread::spawn(move || {
        egui_overlay::start(OverlayApp { frame: 0 });
    });

    // thread::spawn(move || {
    //     _main();
    // });
    
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

            Ok(Box::new(MainApp::default()))
        }),
    );

    Ok(())
}

pub struct OverlayApp {
    frame: u64,
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
            fill: Color32::from_black_alpha(80),          // Transparent background
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
                    ui.label("Recording...");
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
        // update every half a second, not like it needs to be super responsive
        // and it also reduces cpu usage by a ton
        thread::sleep(Duration::from_millis(500));  
    }
}

pub struct MainApp {

}
impl eframe::App for MainApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello World!");
        });
    }
}

impl Default for MainApp {
    fn default() -> Self {
        Self {
            // Initialize your app state here
        }
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
