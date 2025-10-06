mod find_game;
mod hardware_id;
mod hardware_specs;
mod idle;
mod input_recorder;
mod keycode;
mod raw_input_debouncer;
mod recorder;
mod recording;
mod window_recorder;

use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use color_eyre::{Result, eyre::eyre};

use game_process::does_process_exist;
use input_capture::InputCapture;
use tokio::{sync::oneshot, time::MissedTickBehavior};
use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::GetForegroundWindow};

use crate::{
    idle::IdlenessTracker, keycode::lookup_keycode, raw_input_debouncer::EventDebouncer,
    recorder::Recorder,
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

const MAX_IDLE_DURATION: Duration = Duration::from_secs(90);
const MAX_RECORDING_DURATION: Duration = Duration::from_secs(10 * 60);

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    tracing::info!(
        "OWL Recorder v{} ({})",
        env!("CARGO_PKG_VERSION"),
        git_version::git_version!()
    );

    let Args {
        recording_location,
        start_key,
        stop_key,
    } = Args::parse();

    let start_key =
        lookup_keycode(&start_key).ok_or_else(|| eyre!("Invalid start key: {start_key}"))?;
    let stop_key =
        lookup_keycode(&stop_key).ok_or_else(|| eyre!("Invalid stop key: {stop_key}"))?;

    let mut recorder = Recorder::new(|| {
        recording_location.join(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .to_string(),
        )
    });

    let (_input_capture, mut input_rx) = InputCapture::new()?;

    let mut stop_rx = wait_for_ctrl_c();

    let mut idleness_tracker = IdlenessTracker::new(MAX_IDLE_DURATION);
    let mut start_on_activity = false;
    let mut actively_recording_window: Option<HWND> = None;

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
                        actively_recording_window = recorder.recording().as_ref().map(|r| r.hwnd());
                        tracing::info!("Recording started with HWND {actively_recording_window:?}");
                    } else if key == stop_key {
                        tracing::info!("Stop key pressed, stopping recording");
                        recorder.stop().await?;
                        actively_recording_window = None;
                        start_on_activity = false;
                    }
                } else if start_on_activity && actively_recording_window.is_some_and(is_window_focused) {
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
                    } else if let Some(window) = actively_recording_window && !is_window_focused(window) {
                        tracing::info!("Window {window:?} lost focus, stopping recording");
                        recorder.stop().await?;
                    }
                } else if let Some(window) = actively_recording_window && is_window_focused(window) && !start_on_activity {
                    // If we're not currently in a recording, but we were actively recording this window, and this window
                    // is now focused, and we're not waiting on input, let's restart the recording.
                    tracing::info!("Window {window:?} regained focus, restarting recording");
                    recorder.start().await?;
                }
            },
        }
    }

    recorder.stop().await?;

    Ok(())
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
