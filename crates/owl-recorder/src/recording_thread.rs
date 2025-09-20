use crate::{
    AppState, HONK_0_BYTES, HONK_1_BYTES, LOGO_DEFAULT_BYTES, LOGO_RECORDING_BYTES,
    MAX_IDLE_DURATION, MAX_RECORDING_DURATION, RecordingStatus, app_icon::set_app_icon_windows,
    keycode::lookup_keycode, load_icon_from_bytes,
};
use std::{
    io::Cursor,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use color_eyre::{Result, eyre::eyre};

use game_process::does_process_exist;
use input_capture::InputCapture;
use rodio::{Decoder, Sink};
use tokio::{sync::oneshot, time::MissedTickBehavior};

use crate::{
    bootstrap_recorder::BootstrapRecorder, idle::IdlenessTracker,
    raw_input_debouncer::EventDebouncer, recorder::Recorder,
};

pub fn run(
    app_state: Arc<AppState>,
    start_key: String,
    stop_key: String,
    recording_location: PathBuf,
) -> Result<()> {
    tokio::runtime::Runtime::new().unwrap().block_on(main(
        app_state,
        start_key,
        stop_key,
        recording_location,
    ))
}

async fn main(
    app_state: Arc<AppState>,
    start_key: String,
    stop_key: String,
    recording_location: PathBuf,
) -> Result<()> {
    let start_key =
        lookup_keycode(&start_key).ok_or_else(|| eyre!("Invalid start key: {start_key}"))?;
    let stop_key =
        lookup_keycode(&stop_key).ok_or_else(|| eyre!("Invalid stop key: {stop_key}"))?;

    let mut recorder: Recorder<_, BootstrapRecorder> = match Recorder::new(
        || {
            recording_location.join(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string(),
            )
        },
        app_state.clone(),
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
                *app_state.boostrap_progress.write().unwrap() = 1.0;
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
    *app_state.boostrap_progress.write().unwrap() = 1.337;

    tracing::info!("recorder initialized");
    let (_input_capture, mut input_rx) = InputCapture::new()?;

    let mut stop_rx = wait_for_ctrl_c();

    let mut idleness_tracker = IdlenessTracker::new(MAX_IDLE_DURATION);
    let mut start_on_activity = false;

    let mut perform_checks = tokio::time::interval(Duration::from_secs(1));
    perform_checks.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut debouncer = EventDebouncer::new();

    loop {
        tokio::select! {
            r = &mut stop_rx => {
                r.expect("signal handler was closed early");
                break;
            },
            e = input_rx.recv() => {
                let e = e.expect("raw input reader was closed early");
                if !debouncer.debounce(e) {
                    continue;
                }

                recorder.seen_input(e).await?;
                if let Some(key) = e.key_press_keycode() {
                    if key == start_key {
                        tracing::info!("Start key pressed, starting recording");
                        recorder.start().await?;
                        rec_start(&app_state.sink, app_state.configs.read().unwrap().preferences.honk);
                    } else if key == stop_key {
                        tracing::info!("Stop key pressed, stopping recording");
                        recorder.stop().await?;
                        rec_stop(&app_state.sink, app_state.configs.read().unwrap().preferences.honk);
                        start_on_activity = false;
                    }
                } else if start_on_activity {
                    tracing::info!("Input detected, restarting recording");
                    recorder.start().await?;
                    rec_start(&app_state.sink, app_state.configs.read().unwrap().preferences.honk);
                    start_on_activity = false;
                }
                idleness_tracker.update_activity();
            },
            _ = perform_checks.tick() => {
                if let Some(recording) = recorder.recording() {
                    if !does_process_exist(recording.pid())? {
                        tracing::info!(pid=recording.pid().0, "Game process no longer exists, stopping recording");
                        recorder.stop().await?;
                        rec_stop(&app_state.sink, app_state.configs.read().unwrap().preferences.honk);
                    } else if idleness_tracker.is_idle() {
                        tracing::info!("No input detected for 5 seconds, stopping recording");
                        recorder.stop().await?;
                        rec_stop(&app_state.sink, app_state.configs.read().unwrap().preferences.honk);
                        {
                            let mut state_writer = app_state.state.write().unwrap();
                            *state_writer = RecordingStatus::Paused;
                        }
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

// TOOD: find some way to change tray icon during runtime. rn tray icon can only run in main event loop,
// and can't be moved between threads, but it just also won't run at all when the app is minimized.
fn rec_start(sink: &Sink, honk: bool) {
    // let _ = tray_icon.set_icon(Some(load_icon_from_bytes!(LOGO_RECORDING_BYTES, tray_icon)));
    set_app_icon_windows(&load_icon_from_bytes!(LOGO_RECORDING_BYTES, egui_icon));
    if honk {
        sink.append(Decoder::new_mp3(Cursor::new(HONK_0_BYTES)).expect("Cannot decode honk :("));
    }
}

fn rec_stop(sink: &Sink, honk: bool) {
    // let _ = tray_icon.set_icon(Some(load_icon_from_bytes!(LOGO_DEFAULT_BYTES, tray_icon)));
    set_app_icon_windows(&load_icon_from_bytes!(LOGO_DEFAULT_BYTES, egui_icon));
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
