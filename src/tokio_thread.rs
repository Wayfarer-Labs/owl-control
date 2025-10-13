use crate::{
    MAX_IDLE_DURATION,
    api::ApiClient,
    app_state::{AppState, AsyncRequest, RecordingStatus, UiUpdate},
    assets::{get_honk_0_bytes, get_honk_1_bytes},
    keycode::lookup_keycode,
    ui::{
        notification::{NotificationType, show_notification},
        tray_icon,
    },
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

pub fn run(
    app_state: Arc<AppState>,
    start_key: String,
    stop_key: String,
    recording_location: PathBuf,
    log_path: PathBuf,
    async_request_rx: tokio::sync::mpsc::Receiver<AsyncRequest>,
    stopped_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    let recorder = tokio::runtime::Runtime::new().unwrap().block_on(main(
        app_state,
        start_key,
        stop_key,
        recording_location,
        log_path,
        async_request_rx,
        stopped_rx,
    ))?;

    // This is a very disgusting workaround but there doesn't seem to be any other solution.
    // The ObsContext's Drop implementation deadlocks when called after a tokio
    // runtime has been active on the thread. This is because _ObsRuntimeGuard::drop uses
    // futures::executor::block_on which tries to lock a tokio::sync::Mutex, but tokio Mutex
    // requires an active tokio runtime for .await to work. We can't exactly fix that since
    // it's part of libobs.rs. Since we cannot safely drop the ObsContext, we intentionally leak it here.
    tracing::warn!(
        "Leaking recorder to avoid deadlock (resources will be cleaned up by OS at process exit)"
    );
    std::mem::forget(recorder);

    Ok(())
}

async fn main(
    app_state: Arc<AppState>,
    start_key: String,
    stop_key: String,
    recording_location: PathBuf,
    log_path: PathBuf,
    mut async_request_rx: tokio::sync::mpsc::Receiver<AsyncRequest>,
    mut stopped_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<Recorder> {
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
                // might seem redundant but sometimes there's an unreproducible bug where if the MainApp isn't
                // performing repaints it won't receive the shut down signal until user interacts with the window
                app_state.ui_update_tx.try_send(UiUpdate::ForceUpdate).ok();
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
                        if start_recording_safely(&mut recorder).await {
                            rec_start(&sink, honk);

                            actively_recording_window = recorder.recording().as_ref().map(|r| r.hwnd());
                            tracing::info!("Recording started with HWND {actively_recording_window:?}");
                        }
                    } else if key == stop_keycode {
                        tracing::info!("Stop key pressed, stopping recording");
                        recorder.stop().await?;
                        rec_stop(&sink, honk);

                        actively_recording_window = None;
                        start_on_activity = false;
                    }
                } else if start_on_activity && actively_recording_window.is_some_and(is_window_focused) {
                    tracing::info!("Input detected, restarting recording");
                    if start_recording_safely(&mut recorder).await {
                        rec_start(&sink, honk);
                        start_on_activity = false;
                    }
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
                            .try_send(UiUpdate::UpdateUserId(response.map_err(|e| e.to_string())))
                            .ok();
                    }
                    AsyncRequest::UploadData => {
                        tokio::spawn(upload::start(app_state.clone(), api_client.clone(), recording_location.clone()));
                    }
                    AsyncRequest::OpenDataDump => {
                        // Create directory if it doesn't exist
                        if !recording_location.exists() {
                            let _ = std::fs::create_dir_all(&recording_location);
                        }
                        let absolute_path = std::fs::canonicalize(&recording_location)
                            .unwrap_or(recording_location.clone());
                        opener::open(&absolute_path).ok();
                    }
                    AsyncRequest::OpenLog => {
                        opener::reveal(&log_path).ok();
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
                        start_recording_safely(&mut recorder).await;
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
                    if start_recording_safely(&mut recorder).await {
                        recorder.start().await?;
                        rec_start(&sink, honk);
                    }
                }
            },
        }
    }

    recorder.stop().await?;
    // Return the recorder to be "dropped" outside of the tokio runtime
    // to avoid deadlock. See above for more details.
    Ok(recorder)
}

/// Attempts to start the recording.
/// If it fails, it will emit an error and stop the current recording, in whatever state it may be in.
async fn start_recording_safely(recorder: &mut Recorder) -> bool {
    if let Err(e) = recorder.start().await {
        tracing::error!(e=?e, "Failed to start recording");
        show_notification(
            "OWL Control - Error",
            &e.to_string(),
            "",
            NotificationType::Error,
        );
        recorder.stop().await.ok();
        false
    } else {
        true
    }
}

// TOOD: find some way to change tray icon during runtime. rn tray icon can only run in main event loop,
// and can't be moved between threads, but it just also won't run at all when the app is minimized.
fn rec_start(sink: &Sink, honk: bool) {
    tray_icon::set_icon_recording(true);
    if honk {
        sink.append(
            Decoder::new_mp3(Cursor::new(get_honk_0_bytes())).expect("Cannot decode honk :("),
        );
    }
}

fn rec_stop(sink: &Sink, honk: bool) {
    tray_icon::set_icon_recording(false);
    if honk {
        sink.append(
            Decoder::new_mp3(Cursor::new(get_honk_1_bytes())).expect("Cannot decode honk :("),
        );
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
