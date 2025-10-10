use crate::{
    MAX_IDLE_DURATION,
    api::ApiClient,
    app_state::{AppState, AsyncRequest, RecordingStatus, UiUpdate},
    keycode::lookup_keycode,
    ui::tray_icon,
    upload,
};
use std::{
    io::Cursor,
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use color_eyre::{Result, eyre::eyre};

use constants::MAX_FOOTAGE;
use game_process::does_process_exist;
use input_capture::InputCapture;
use rodio::{Decoder, Sink};
use tokio::{sync::oneshot, time::MissedTickBehavior};
use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::GetForegroundWindow};

use crate::{idle::IdlenessTracker, raw_input_debouncer::EventDebouncer, recorder::Recorder};

const HONK_0_BYTES: &[u8] = include_bytes!("../assets/goose_honk0.mp3");
const HONK_1_BYTES: &[u8] = include_bytes!("../assets/goose_honk1.mp3");

pub fn run(
    app_state: Arc<AppState>,
    start_key: String,
    stop_key: String,
    recording_location: PathBuf,
    async_request_rx: tokio::sync::mpsc::Receiver<AsyncRequest>,
    stopped_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    tokio::runtime::Runtime::new().unwrap().block_on(main(
        app_state,
        start_key,
        stop_key,
        recording_location,
        async_request_rx,
        stopped_rx,
    ))
}

async fn main(
    app_state: Arc<AppState>,
    start_key: String,
    stop_key: String,
    recording_location: PathBuf,
    mut async_request_rx: tokio::sync::mpsc::Receiver<AsyncRequest>,
    mut stopped_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    let mut start_key = start_key;
    let mut stop_key = stop_key;
    let mut start_keycode =
        lookup_keycode(&start_key).ok_or_else(|| eyre!("Invalid start key: {start_key}"))?;
    let mut stop_keycode =
        lookup_keycode(&stop_key).ok_or_else(|| eyre!("Invalid stop key: {stop_key}"))?;

    let stream_handle =
        rodio::OutputStreamBuilder::open_default_stream().expect("open default audio stream");
    let sink = Sink::connect_new(stream_handle.mixer());

    let mut recorder = Recorder::new(
        Box::new({
            let recording_location = recording_location.clone();
            move || {
                recording_location.join(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        .to_string(),
                )
            }
        }),
        app_state.clone(),
    )
    .await?;

    tracing::info!("recorder initialized");
    let (_input_capture, mut input_rx) = InputCapture::new()?;

    let mut ctrlc_rx = wait_for_ctrl_c();

    let mut idleness_tracker = IdlenessTracker::new(MAX_IDLE_DURATION);
    let mut start_on_activity = false;
    let mut actively_recording_window: Option<HWND> = None;

    let mut perform_checks = tokio::time::interval(Duration::from_secs(1));
    perform_checks.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut debouncer = EventDebouncer::new();

    let api_client = Arc::new(ApiClient::new());

    loop {
        let honk: bool;
        let start_key_cfg: String;
        let stop_key_cfg: String;
        {
            let cfg = app_state.config.read().unwrap();
            honk = cfg.preferences.honk;
            start_key_cfg = cfg.preferences.start_recording_key.clone();
            stop_key_cfg = cfg.preferences.stop_recording_key.clone();
        }
        // instead of performing lookup_keycode every iteration, we check if it's changed from original
        // and only then do we do the lookup
        if start_key_cfg != start_key {
            start_key = start_key_cfg;
            start_keycode = lookup_keycode(&start_key)
                .ok_or_else(|| eyre!("Invalid start key: {start_key}"))?;
        }
        if stop_key_cfg != stop_key {
            stop_key = stop_key_cfg;
            stop_keycode =
                lookup_keycode(&stop_key).ok_or_else(|| eyre!("Invalid stop key: {stop_key}"))?;
        }
        tokio::select! {
            r = &mut ctrlc_rx => {
                r.expect("ctrl-c signal handler was closed early");
                break;
            },
            r = stopped_rx.recv() => {
                r.expect("stopped signal handler was closed early");
                break;
            },
            e = input_rx.recv() => {
                let e = e.expect("raw input reader was closed early");
                if !debouncer.debounce(e) {
                    continue;
                }

                recorder.seen_input(e).await?;
                if let Some(key) = e.key_press_keycode() && !app_state.is_currently_rebinding.load(Ordering::Relaxed) {
                    if key == start_keycode {
                        tracing::info!("Start key pressed, starting recording");
                        recorder.start().await?;
                        rec_start(&sink, honk);

                        actively_recording_window = recorder.recording().as_ref().map(|r| r.hwnd());
                        tracing::info!("Recording started with HWND {actively_recording_window:?}");
                    } else if key == stop_keycode {
                        tracing::info!("Stop key pressed, stopping recording");
                        recorder.stop().await?;
                        rec_stop(&sink, honk);

                        actively_recording_window = None;
                        start_on_activity = false;
                    }
                } else if start_on_activity && actively_recording_window.is_some_and(is_window_focused) {
                    tracing::info!("Input detected, restarting recording");
                    recorder.start().await?;
                    rec_start(&sink, honk);
                    start_on_activity = false;
                }
                idleness_tracker.update_activity();
            },
            e = async_request_rx.recv() => {
                let e = e.expect("async request reader was closed early");
                match e {
                    AsyncRequest::ValidateApiKey { api_key } => {
                        tracing::info!("API KEY VALIDATION RUN");
                        let response = api_client.validate_api_key(api_key).await;
                        tracing::info!("API KEY VALIDATION RESPONSE: {response:?}");
                        app_state
                            .ui_update_tx
                            .try_send(UiUpdate::UpdateUserId(response))
                            .ok();
                    }
                    AsyncRequest::UploadData => {
                        tokio::spawn(upload::start(app_state.clone(), api_client.clone(), recording_location.clone()));
                    }
                    AsyncRequest::OpenDataDump => {
                        // opens the data_dump folder in file explorer
                        let exe_path = std::env::current_exe().unwrap_or_default();
                        let exe_dir =
                            exe_path.parent().unwrap_or(std::path::Path::new("."));
                        let data_dump_path = exe_dir.join("data_dump");
                        // Create directory if it doesn't exist
                        if !data_dump_path.exists() {
                            let _ = std::fs::create_dir_all(&data_dump_path);
                        }
                        // Convert to absolute path
                        let absolute_path = std::fs::canonicalize(&data_dump_path)
                            .unwrap_or(data_dump_path.clone());

                        #[cfg(target_os = "windows")]
                        {
                            open::with(&absolute_path, "explorer").ok();
                        }
                    }
                }
            },
            _ = perform_checks.tick() => {
                if let Some(recording) = recorder.recording() {
                    if !does_process_exist(recording.pid())? {
                        tracing::info!(pid=recording.pid().0, "Game process no longer exists, stopping recording");
                        recorder.stop().await?;
                        rec_stop(&sink, honk);
                    } else if idleness_tracker.is_idle() {
                        tracing::info!("No input detected for 5 seconds, stopping recording");
                        recorder.stop().await?;
                        rec_stop(&sink, honk);
                        *app_state.state.write().unwrap() = RecordingStatus::Paused;
                        start_on_activity = true;
                    } else if recording.elapsed() > MAX_FOOTAGE {
                        tracing::info!("Recording duration exceeded {} s, restarting recording", MAX_FOOTAGE.as_secs());
                        recorder.stop().await?;
                        recorder.start().await?;
                        idleness_tracker.update_activity();
                    } else if let Some(window) = actively_recording_window && !is_window_focused(window) {
                        tracing::info!("Window {window:?} lost focus, stopping recording");
                        recorder.stop().await?;
                        rec_stop(&sink, honk);
                    }
                } else if let Some(window) = actively_recording_window && is_window_focused(window) && !start_on_activity {
                    // If we're not currently in a recording, but we were actively recording this window, and this window
                    // is now focused, and we're not waiting on input, let's restart the recording.
                    tracing::info!("Window {window:?} regained focus, restarting recording");
                    recorder.start().await?;
                    rec_start(&sink, honk);
                }
            },
        }
    }

    recorder.stop().await?;

    Ok(())
}

// TOOD: find some way to change tray icon during runtime. rn tray icon can only run in main event loop,
// and can't be moved between threads, but it just also won't run at all when the app is minimized.
fn rec_start(sink: &Sink, honk: bool) {
    tray_icon::set_icon_recording(true);
    if honk {
        sink.append(Decoder::new_mp3(Cursor::new(HONK_0_BYTES)).expect("Cannot decode honk :("));
    }
}

fn rec_stop(sink: &Sink, honk: bool) {
    tray_icon::set_icon_recording(false);
    if honk {
        sink.append(Decoder::new_mp3(Cursor::new(HONK_1_BYTES)).expect("Cannot decode honk :("));
    }
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

fn is_window_focused(hwnd: HWND) -> bool {
    unsafe { GetForegroundWindow() == hwnd }
}
