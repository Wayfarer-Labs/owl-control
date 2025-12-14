use crate::{
    api::ApiClient,
    app_state::{
        AppState, AsyncRequest, ForegroundedGame, GitHubRelease, ListeningForNewHotkey,
        RecordingStatus, UiUpdate,
    },
    assets::load_cue_bytes,
    play_time::PlayTimeTransition,
    record::LocalRecording,
    system::keycode::name_to_virtual_keycode,
    ui::notification::error_message_box,
    upload,
    util::version::is_version_newer,
};
use std::{
    collections::HashMap,
    io::Cursor,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use color_eyre::{Result, eyre::Context};

use constants::{GH_ORG, GH_REPO, MAX_FOOTAGE, MAX_IDLE_DURATION, supported_games::SupportedGames};
use game_process::does_process_exist;
use input_capture::{Event, InputCapture};
use rodio::{Decoder, Sink, Source};
use tokio::{sync::oneshot, time::MissedTickBehavior};
use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::GetForegroundWindow};

use crate::{
    record::{Recorder, get_recording_base_resolution},
    system::raw_input_debouncer::EventDebouncer,
};

pub fn run(
    app_state: Arc<AppState>,
    log_path: PathBuf,
    async_request_rx: tokio::sync::mpsc::Receiver<AsyncRequest>,
    stopped_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    tracing::debug!("Creating tokio runtime");
    tokio::runtime::Runtime::new().unwrap().block_on(main(
        app_state,
        log_path,
        async_request_rx,
        stopped_rx,
    ))
}

async fn main(
    app_state: Arc<AppState>,
    log_path: PathBuf,
    mut async_request_rx: tokio::sync::mpsc::Receiver<AsyncRequest>,
    mut stopped_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    tracing::debug!("Tokio async main started");
    tracing::debug!("Initializing audio stream");
    let stream_handle =
        rodio::OutputStreamBuilder::open_default_stream().expect("open default audio stream");

    tracing::debug!("Initializing recorder");
    let recorder = Recorder::new(
        Box::new({
            let app_state = app_state.clone();
            move || {
                let base = app_state
                    .config
                    .read()
                    .unwrap()
                    .preferences
                    .recording_location
                    .clone();
                base.join(
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

    // Reset our encoder to x264 if the previously-set encoder is no longer available,
    // and update the available video encoders in the UI.
    {
        let encoders = recorder.available_video_encoders();

        {
            let mut config = app_state.config.write().unwrap();
            if !encoders.contains(&config.preferences.encoder.encoder) {
                tracing::warn!("Currently-set encoder is no longer available, resetting to x264");
                config.preferences.encoder.encoder = constants::encoding::VideoEncoderType::X264;
            }
        }

        app_state
            .ui_update_tx
            .send(UiUpdate::UpdateAvailableVideoEncoders(encoders.to_vec()))
            .ok();
    }

    tracing::info!("recorder initialized");
    // I initially tried to move this into `Recorder`, so that it could be passed down to
    // the relevant methods, but this caused the Windows event loop to hang.
    //
    // Absolutely no idea why, but I'm willing to accept this as-is for now.
    tracing::debug!("Initializing input capture");
    let (input_capture, mut input_rx) = InputCapture::new()?;
    tracing::debug!("Input capture initialized");

    let mut ctrlc_rx = wait_for_ctrl_c();

    let mut perform_checks = tokio::time::interval(Duration::from_secs(1));
    perform_checks.set_missed_tick_behavior(MissedTickBehavior::Delay);

    tracing::debug!("Initializing event debouncer");
    let mut debouncer = EventDebouncer::new();

    tracing::debug!("Initializing API client");
    let api_client = Arc::new(ApiClient::new());
    let mut valid_api_key_and_user_id: Option<(String, String)> = None;

    let mut state = State {
        recording_state: RecordingState::Idle,
        recorder,
        input_capture,
        sink: Sink::connect_new(stream_handle.mixer()),
        app_state: app_state.clone(),
        cue_cache: HashMap::new(),
        last_active: Instant::now(),
        actively_recording_window: None,
    };

    // Initial async requests to GitHub/server
    tracing::debug!("Spawning startup requests task");
    tokio::spawn(startup_requests(app_state.clone()));
    tracing::debug!("Tokio thread initialization complete, entering main loop");

    loop {
        tokio::select! {
            r = &mut ctrlc_rx => {
                r.expect("ctrl-c signal handler was closed early");
                break;
            },
            r = stopped_rx.recv() => {
                r.expect("stopped signal handler was closed early");
                // might seem redundant but sometimes there's an unreproducible bug where if the MainApp isn't
                // performing repaints it won't receive the shut down signal until user interacts with the window
                app_state.ui_update_tx.send(UiUpdate::ForceUpdate).ok();
                break;
            },
            e = input_rx.recv() => {
                let e = e.expect("raw input reader was closed early");
                if !debouncer.debounce(e) {
                    continue;
                }

                let listening_for_new_hotkey = *app_state.listening_for_new_hotkey.read().unwrap();
                match listening_for_new_hotkey {
                    ListeningForNewHotkey::Listening { target } => {
                        if let Some(key) = e.key_press_keycode() { *app_state.listening_for_new_hotkey.write().unwrap() = ListeningForNewHotkey::Captured { target, key } }
                    },
                    ListeningForNewHotkey::NotListening => {
                        state.on_input(e).await;
                    },
                    _ => {},
                }
            },
            e = async_request_rx.recv() => {
                let e = e.expect("async request reader was closed early");
                let recording_location = {
                    app_state
                        .config
                        .read()
                        .unwrap()
                        .preferences
                        .recording_location
                        .clone()
                };
                match e {
                    AsyncRequest::ValidateApiKey { api_key } => {
                        // Check if offline mode is enabled
                        if app_state.offline_mode.load(std::sync::atomic::Ordering::SeqCst) {
                            tracing::info!("Offline mode enabled, skipping API key validation");
                            app_state.async_request_tx.send(AsyncRequest::SetOfflineMode { enabled: true, offline_reason: None }).await.ok();
                        } else {
                            let response = api_client.validate_api_key(&api_key).await;
                            tracing::info!("Received response from API key validation: {response:?}");

                            match response {
                                Err(e) if e.is_network_error() => {
                                    // Network error or server unavailable (502/503/504) - switch to offline mode
                                    tracing::warn!("API server unavailable, switching to offline mode: {e}");
                                    app_state.async_request_tx.send(AsyncRequest::SetOfflineMode { enabled: true, offline_reason: Some(e.to_string()) }).await.ok();
                                }
                                Err(e) => {
                                    // API key validation failed - don't switch to offline mode
                                    tracing::warn!("API key validation failed: {e}");
                                    app_state
                                        .ui_update_tx
                                        .send(UiUpdate::UpdateUserId(Err(e.to_string())))
                                        .ok();
                                }
                                Ok(user_id) => {
                                    valid_api_key_and_user_id = Some((api_key.clone(), user_id.clone()));
                                    app_state
                                        .ui_update_tx
                                        .send(UiUpdate::UpdateUserId(Ok(user_id)))
                                        .ok();

                                    app_state.async_request_tx.send(AsyncRequest::LoadUploadStats).await.ok();
                                }
                            }
                            // no matter if offline or online, local recordings should be loaded
                            app_state.async_request_tx.send(AsyncRequest::LoadLocalRecordings).await.ok();
                        }
                    }
                    AsyncRequest::UploadData => {
                        if app_state.offline_mode.load(std::sync::atomic::Ordering::SeqCst) {
                            tracing::info!("Offline mode enabled, skipping upload");
                            app_state
                                .ui_update_tx
                                .send(UiUpdate::UploadFailed("Offline mode is enabled. Uploads are disabled.".to_string()))
                                .ok();
                        } else {
                            tokio::spawn(upload::start(app_state.clone(), api_client.clone(), recording_location.clone()));
                        }
                    }
                    AsyncRequest::PauseUpload => {
                        app_state.upload_pause_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        // Clear the auto-upload queue when pausing
                        let prev_queue_count = app_state
                            .auto_upload_queue_count
                            .load(std::sync::atomic::Ordering::SeqCst);
                        tracing::info!(
                            "Upload pause requested, auto-upload queue cleared (was {prev_queue_count} recordings)"
                        );
                        set_auto_upload_queue_count(&app_state, 0);
                    }
                    AsyncRequest::OpenDataDump => {
                        if !recording_location.exists() {
                            let _ = std::fs::create_dir_all(&recording_location);
                        }
                        let absolute_path = std::fs::canonicalize(&recording_location)
                            .unwrap_or(recording_location);
                        opener::open(&absolute_path).ok();
                    }
                    AsyncRequest::OpenLog => {
                        opener::reveal(&log_path).ok();
                    }
                    AsyncRequest::OpenFolder(path) => {
                        opener::open(&path).ok();
                    }
                    AsyncRequest::UpdateSupportedGames(new_games) => {
                        let mut supported_games = app_state.supported_games.write().unwrap();
                        let old_game_count = supported_games.games.len();
                        *supported_games = new_games;
                        tracing::info!(
                            "Updated supported games: {old_game_count} -> {} total, {} installed",
                            supported_games.games.len(),
                            supported_games.installed().count()
                        );
                    }
                    AsyncRequest::LoadUploadStats => {
                        if app_state.offline_mode.load(std::sync::atomic::Ordering::SeqCst) {
                            tracing::info!("Offline mode enabled, skipping upload stats load");
                            // Don't send any update - UI will show no upload stats
                        } else {
                            match valid_api_key_and_user_id.clone() {
                                Some((api_key, user_id)) => {
                                    tokio::spawn({
                                        let app_state = app_state.clone();
                                        let api_client = api_client.clone();
                                        async move {
                                            let stats = match api_client.get_user_upload_stats(&api_key, &user_id).await {
                                                Ok(stats) => stats,
                                                Err(e) => {
                                                    tracing::error!(e=?e, "Failed to get user upload stats");
                                                    return;
                                                }
                                            };
                                            tracing::info!(stats=?stats.statistics, "Loaded upload stats");
                                            app_state.ui_update_tx.send(UiUpdate::UpdateUserUploads(stats)).ok();
                                        }
                                    });
                                }
                                None => {
                                    tracing::error!("API key and user ID not found, skipping upload stats load");
                                }
                            }
                        }
                    }
                    AsyncRequest::LoadLocalRecordings => {
                        tokio::spawn({
                            let app_state = app_state.clone();
                            async move {
                                let local_recordings = tokio::task::spawn_blocking(move || {
                                    LocalRecording::scan_directory(&recording_location)
                                }).await.unwrap_or_default();

                                tracing::info!("Found {} local recordings", local_recordings.len());
                                app_state
                                    .ui_update_tx
                                    .send(UiUpdate::UpdateLocalRecordings(local_recordings))
                                    .ok();
                            }
                        });
                    }
                    AsyncRequest::DeleteAllInvalidRecordings => {
                        let Some((api_key, _)) = valid_api_key_and_user_id.clone() else {
                            tracing::error!("Cannot delete invalid recordings without valid API key");
                            continue;
                        };

                        tokio::spawn({
                            let app_state = app_state.clone();
                            let api_client = api_client.clone();
                            async move {
                                // Get current list of local recordings
                                let local_recordings = tokio::task::spawn_blocking({
                                    let recording_location = recording_location.clone();
                                    move || LocalRecording::scan_directory(&recording_location)
                                }).await.unwrap_or_default();

                                // Filter only invalid recordings
                                let invalid_recordings: Vec<_> = local_recordings
                                    .into_iter()
                                    .filter(|r| matches!(r, LocalRecording::Invalid { .. }))
                                    .collect();

                                if invalid_recordings.is_empty() {
                                    tracing::info!("No invalid recordings to delete");
                                    return;
                                }

                                let total_count = invalid_recordings.len();
                                tracing::info!("Deleting {} invalid recordings", total_count);

                                // Delete all invalid recording folders
                                let mut errors = vec![];
                                for recording in invalid_recordings {
                                    let info = recording.info().clone();
                                    if let Err(e) = recording.delete(&api_client, &api_key).await {
                                        tracing::error!("Failed to delete {}: {:?}", info, e);
                                        errors.push(info.folder_name);
                                    } else {
                                        tracing::info!("Deleted invalid recording: {}", info);
                                    }
                                }

                                if errors.is_empty() {
                                    tracing::info!("Successfully deleted all {total_count} invalid recordings");
                                } else {
                                    tracing::warn!("Failed to delete {} recordings: {:?}", errors.len(), errors);
                                }


                                app_state.async_request_tx.send(AsyncRequest::LoadLocalRecordings).await.ok();
                            }
                        });
                    }
                    AsyncRequest::DeleteAllUploadedLocalRecordings => {
                        let Some((api_key, _)) = valid_api_key_and_user_id.clone() else {
                            tracing::error!("Cannot delete invalid recordings without valid API key");
                            continue;
                        };

                        tokio::spawn({
                            let app_state = app_state.clone();
                            let api_client = api_client.clone();
                            async move {
                                // Get current list of local recordings
                                let mut local_recordings = tokio::task::spawn_blocking({
                                    let recording_location = recording_location.clone();
                                    move || LocalRecording::scan_directory(&recording_location)
                                }).await.unwrap_or_default();

                                local_recordings.retain(|r| matches!(r, LocalRecording::Uploaded { .. }));

                                if local_recordings.is_empty() {
                                    tracing::info!("No uploaded recordings to delete");
                                    return;
                                }

                                let total_count = local_recordings.len();
                                tracing::info!("Deleting {total_count} uploaded recordings");

                                // Delete all uploaded recording folders
                                let mut errors = vec![];
                                for recording in local_recordings {
                                    let info = recording.info().clone();
                                    if let Err(e) = recording.delete(&api_client, &api_key).await {
                                        tracing::error!(e=?e, "Failed to delete uploaded recording: {info}");
                                        errors.push(info.folder_name);
                                    } else {
                                        tracing::info!("Deleted uploaded recording: {info}");
                                    }
                                }

                                if errors.is_empty() {
                                    tracing::info!("Successfully deleted all {total_count} uploaded recordings");
                                } else {
                                    tracing::warn!("Failed to delete {} recordings: {:?}", errors.len(), errors);
                                }

                                app_state.async_request_tx.send(AsyncRequest::LoadLocalRecordings).await.ok();
                            }
                        });
                    }
                    AsyncRequest::DeleteRecording(path) => {
                        let Some((api_key, _)) = valid_api_key_and_user_id.as_ref() else {
                            tracing::error!("Cannot delete recording without valid API key: {}", path.display());
                            app_state.async_request_tx.send(AsyncRequest::LoadLocalRecordings).await.ok();
                            continue;
                        };

                        if let Some(recording) = LocalRecording::from_path(&path) {
                            if let Err(e) = recording.delete(&api_client, api_key).await {
                                tracing::error!(e=?e, "Failed to delete recording: {}", path.display());
                            } else {
                                tracing::info!("Deleted recording: {}", path.display());
                            }
                        } else {
                            tracing::error!("Cannot delete non-recording folder: {}", path.display());
                        }

                        app_state.async_request_tx.send(AsyncRequest::LoadLocalRecordings).await.ok();
                    }
                    AsyncRequest::MoveRecordingsFolder { from, to } => {
                        tokio::spawn(move_recordings_folder(app_state.clone(), from, to));
                    }
                    AsyncRequest::PickRecordingFolder { current_location } => {
                        tokio::spawn(pick_recording_folder(app_state.clone(), current_location));
                    }
                    AsyncRequest::PlayCue { cue } => {
                        play_cue(&state.sink, &app_state, &cue, &mut state.cue_cache, |s| s);
                    }
                    AsyncRequest::UploadCompleted { uploaded_count } => {
                        // Subtract the number of recordings that were just uploaded from the queue
                        let prev_count = app_state
                            .auto_upload_queue_count
                            .load(std::sync::atomic::Ordering::SeqCst);
                        let new_count = prev_count.saturating_sub(uploaded_count);

                        tracing::info!(
                            "Upload completed: {} recordings uploaded, queue count {} -> {}",
                            uploaded_count,
                            prev_count,
                            new_count
                        );

                        // If there are still queued recordings, start another upload batch
                        if new_count > 0 {
                            tracing::info!(
                                "Auto-upload queue has {} remaining, starting next upload batch",
                                new_count
                            );
                            app_state
                                .async_request_tx
                                .send(AsyncRequest::UploadData)
                                .await
                                .ok();
                        }

                        set_auto_upload_queue_count(&app_state, new_count);
                    }
                    AsyncRequest::ClearAutoUploadQueue => {
                        let prev_count = app_state
                            .auto_upload_queue_count
                            .load(std::sync::atomic::Ordering::SeqCst);
                        tracing::info!(
                            "Auto-upload queue cleared (was {} recordings)",
                            prev_count
                        );
                        set_auto_upload_queue_count(&app_state, 0);
                    }
                    AsyncRequest::SetOfflineMode { enabled, offline_reason } => {
                        tracing::info!("Setting offline mode to {}", enabled);
                        app_state.offline_mode.store(enabled, std::sync::atomic::Ordering::SeqCst);
                        let mut offline_str = "Offline".to_string();

                        if let Some(reason) = offline_reason {
                            offline_str.push_str(&format!(" ({reason})"));
                        }
                        app_state
                            .ui_update_tx
                            .send(UiUpdate::UpdateUserId(Ok(offline_str)))
                            .ok();
                    }
                }
            },
            _ = perform_checks.tick() => {
                // Update play-time tracking
                app_state.play_time_state.write().unwrap().tick(&app_state.state.read().unwrap());

                // Flush pending input events to disk
                if let Err(e) = state.recorder.flush_input_events().await {
                    tracing::error!(e=?e, "Failed to flush input events");
                }
                // Check foregrounded game
                *app_state.last_foregrounded_game.write().unwrap() = get_foregrounded_game(&app_state.supported_games.read().unwrap(), &state.recorder);
                // Tick state machine
                state.tick().await;
                // Periodically force the UI to rerender so that it will process events, even if not visible
                app_state.ui_update_tx.send(UiUpdate::ForceUpdate).ok();
            },
        }
    }

    if let Err(e) = state.recorder.stop(&state.input_capture).await {
        tracing::error!(e=?e, "Failed to stop recording on shutdown");
    }
    Ok(())
}

/// State machine-esque representation of the recording state. This is only accessible from tokio_thread.
/// We want to somehow be able to manipulate the recording state with appropriate transitions, however its
/// not trivial to handle diff function signatures for on_input, tick, etc. for every state. This would indicate
/// that RecordingState should be a struct for each state, but that's disgustingly overcomplicated and would mean match
/// statements in the tokio thread itself to match the correct function signatures anyway, which defeats the purpose.
/// This then indicates that we should move all the variables into RecordingState, but thats not possible with enums we would
/// have to split it into a struct and the enum portion. This seems the cleanest possible, and we would have
/// on_input/tick() as non-arg accepting fns (or like maybe 1 arg for the tracing str reason, something consistent),
/// then match statements within the fn itself to handle the diff states.
#[derive(Clone, PartialEq, Debug)]
enum RecordingState {
    /// Waiting for user to start recording
    Idle,
    /// In process of recording
    Recording,
    /// Recording paused due to idle or unfocused window, and will restart
    /// upon both input & window focus detected. Stores the PID of the paused
    /// application to detect if it closes while paused.
    Paused { pid: game_process::Pid },
}
struct State {
    recording_state: RecordingState,
    recorder: Recorder,
    input_capture: InputCapture,
    sink: Sink,
    app_state: Arc<AppState>,
    cue_cache: HashMap<String, Vec<u8>>,
    last_active: Instant,
    actively_recording_window: Option<HWND>,
}
impl State {
    async fn on_input(&mut self, e: Event) {
        let (start_key, stop_key) = {
            let cfg = self.app_state.config.read().unwrap();
            (
                name_to_virtual_keycode(cfg.preferences.start_recording_key()),
                name_to_virtual_keycode(cfg.preferences.stop_recording_key()),
            )
        };
        if let Err(e) = self.recorder.seen_input(e).await {
            tracing::error!(e=?e, "Failed to seen input");
        }
        self.last_active = Instant::now();
        if let Err(e) = match (&self.recording_state, e.key_press_keycode()) {
            (RecordingState::Idle, key) if key == start_key => {
                if self
                    .app_state
                    .is_out_of_date
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    error_message_box(concat!(
                        "You are using an outdated version of OWL Control. ",
                        "Please update to the latest version to continue.\n\n",
                        "Recording and uploading will be blocked until you update."
                    ));
                    return;
                }
                self.handle_transition(RecordingState::Recording).await
            }
            (RecordingState::Recording | RecordingState::Paused { .. }, key) if key == stop_key => {
                self.handle_transition(RecordingState::Idle).await
            }
            (RecordingState::Paused { .. }, _) => {
                // key_press_keycode returned None, meaning some other input event that isn't keypress was detected,
                // then check that window is also focused before resuming recording
                if self
                    .actively_recording_window
                    .is_some_and(is_window_focused)
                {
                    tracing::info!("Input detected for focused window, restarting recording");
                    self.handle_transition(RecordingState::Recording).await
                } else {
                    return;
                }
            }
            _ => return,
        } {
            tracing::error!(e=?e, "Failed to handle recording state transition on input");
        }
    }

    async fn tick(&mut self) {
        if let RecordingState::Recording = self.recording_state {
            let Some(recording) = self.recorder.recording() else {
                tracing::error!("Expected recording to exist in Recording state, but found None");
                return;
            };

            // Extract game name early to avoid borrow issues later
            let game_name = recording.game_exe().to_string();

            let state_request: Option<(RecordingState, &str)> =
                if !does_process_exist(recording.pid()).unwrap_or_default() {
                    // game closed
                    tracing::info!(
                        pid = recording.pid().0,
                        "Game process no longer exists, stopping recording"
                    );
                    Some((RecordingState::Idle, "stop recording on game process exit"))
                } else if self.last_active.elapsed() > MAX_IDLE_DURATION {
                    // idle timeout
                    tracing::info!(
                        "No input detected for {} seconds, stopping recording",
                        MAX_IDLE_DURATION.as_secs()
                    );
                    Some((
                        RecordingState::Paused {
                            pid: recording.pid(),
                        },
                        "stop recording on idle timeout",
                    ))
                } else if recording.elapsed() > MAX_FOOTAGE {
                    // restart recording once max duration met
                    tracing::info!(
                        "Recording duration exceeded {} s, restarting recording",
                        MAX_FOOTAGE.as_secs()
                    );
                    Some((
                        RecordingState::Recording,
                        "restart recording on recording duration exceeded",
                    ))
                } else if self
                    .actively_recording_window
                    .is_some_and(|window| !is_window_focused(window))
                {
                    // user alt-tabbed out
                    tracing::info!(
                        "Window {:?} lost focus, pausing recording",
                        self.actively_recording_window
                    );
                    Some((
                        RecordingState::Paused {
                            pid: recording.pid(),
                        },
                        "pause recording on window lost focus",
                    ))
                } else if let Ok(current_resolution) =
                    get_recording_base_resolution(recording.hwnd())
                    && current_resolution != recording.game_resolution()
                {
                    // Check if the window resolution has changed and restart the recording
                    tracing::info!(
                        old_resolution=?recording.game_resolution(),
                        new_resolution=?current_resolution,
                        "Window resolution changed, restarting recording"
                    );
                    Some((
                        RecordingState::Recording,
                        "restart recording on window resolution changed",
                    ))
                } else if self.recorder.check_hook_timeout().await {
                    // OBS failed to hook the application
                    tracing::error!(
                        "OBS failed to hook application after {} seconds, stopping recording",
                        constants::HOOK_TIMEOUT.as_secs()
                    );

                    let message = format!(
                        "Failed to hook into {}.\n\n\
                     OWL Control was unable to capture the game window after {} seconds.\n\n\
                     This may happen if:\n\
                     - The game has anti-cheat software\n\
                     - The game is running with elevated privileges\n\
                     - The game uses a rendering method that OBS cannot capture\n\n\
                     Please try:\n\
                     - Running OWL Control as administrator\n\
                     - Checking if the game is on the supported games list\n\
                     - Testing a different game on the supported games list",
                        game_name,
                        constants::HOOK_TIMEOUT.as_secs()
                    );
                    crate::ui::notification::warning_message_box(&message);
                    Some((RecordingState::Idle, "stop recording on hook timeout"))
                } else {
                    None
                };
            if let Some((to_state, task)) = state_request
                && let Err(e) = self.handle_transition(to_state).await
            {
                tracing::error!(e=?e, "Failed to {task}");
            }
        } else if let RecordingState::Paused { pid } = self.recording_state {
            // Check if the paused application has closed
            if !does_process_exist(pid).unwrap_or_default() {
                tracing::info!(
                    pid = pid.0,
                    "Paused game process no longer exists, transitioning to idle"
                );
                if let Err(e) = self.handle_transition(RecordingState::Idle).await {
                    tracing::error!(e=?e, "Failed to transition from paused to idle on process exit");
                }
            }
        }

        // Remember to poll the recorder for its own internal work
        self.recorder.poll().await;
    }

    async fn handle_transition(&mut self, to_state: RecordingState) -> Result<()> {
        tracing::info!(
            "Recording state changing: {:?} -> {:?}",
            self.recording_state,
            to_state
        );

        self.recording_state = match (&self.recording_state, to_state) {
            (RecordingState::Idle | RecordingState::Paused { .. }, RecordingState::Recording) => {
                // Start recording from Idle or Paused state
                let honk = self.app_state.config.read().unwrap().preferences.honk;
                let supported_games = self.app_state.supported_games.read().unwrap().clone();
                start_recording_safely(
                    &mut self.recorder,
                    &self.input_capture,
                    &supported_games,
                    Some((&self.sink, honk, &self.app_state)),
                    &mut self.cue_cache,
                )
                .await?;
                self.actively_recording_window =
                    self.recorder.recording().as_ref().map(|r| r.hwnd());
                tracing::info!(
                    "Recording started with HWND {:?}",
                    self.actively_recording_window
                );
                self.last_active = Instant::now();
                // Notify play time tracker of recording start
                self.app_state
                    .play_time_state
                    .write()
                    .unwrap()
                    .handle_transition(PlayTimeTransition {
                        is_recording: true,
                        due_to_idle: false,
                    });
                RecordingState::Recording
            }
            (RecordingState::Recording, RecordingState::Idle) => {
                // Stop recording and return to Idle
                let honk = self.app_state.config.read().unwrap().preferences.honk;
                stop_recording_with_notification(
                    &mut self.recorder,
                    &self.input_capture,
                    Some((&self.sink, honk, &self.app_state)),
                    &mut self.cue_cache,
                )
                .await?;
                // Notify play time tracker of recording stop
                self.app_state
                    .play_time_state
                    .write()
                    .unwrap()
                    .handle_transition(PlayTimeTransition {
                        is_recording: false,
                        due_to_idle: false,
                    });
                // Trigger auto-upload if enabled
                self.maybe_trigger_auto_upload().await;
                RecordingState::Idle
            }
            (RecordingState::Recording, RecordingState::Paused { pid }) => {
                // Pause recording (due to idle or unfocused window)
                // Check if this was due to idle timeout before we stop
                let due_to_idle = self.last_active.elapsed() > MAX_IDLE_DURATION;
                let honk = self.app_state.config.read().unwrap().preferences.honk;
                stop_recording_with_notification(
                    &mut self.recorder,
                    &self.input_capture,
                    Some((&self.sink, honk, &self.app_state)),
                    &mut self.cue_cache,
                )
                .await?;
                *self.app_state.state.write().unwrap() = RecordingStatus::Paused;
                // Notify play time tracker of pause (with idle buffer cancellation if due to idle)
                self.app_state
                    .play_time_state
                    .write()
                    .unwrap()
                    .handle_transition(PlayTimeTransition {
                        is_recording: false,
                        due_to_idle,
                    });
                // Trigger auto-upload if enabled (recording was saved)
                self.maybe_trigger_auto_upload().await;
                RecordingState::Paused { pid }
            }
            (RecordingState::Paused { .. }, RecordingState::Idle) => {
                let honk = self.app_state.config.read().unwrap().preferences.honk;
                // When user stop keys recording while paused, or when the paused app closes
                *self.app_state.state.write().unwrap() = RecordingStatus::Stopped;
                // Play a mild version of the stop recording cue to signal we're done
                let stop_recording_cue = self
                    .app_state
                    .config
                    .read()
                    .unwrap()
                    .preferences
                    .audio_cues
                    .stop_recording
                    .clone();
                if honk {
                    play_cue(
                        &self.sink,
                        &self.app_state,
                        &stop_recording_cue,
                        &mut self.cue_cache,
                        // TODO: find a better effect / sound for this. I wanted to use a reversed-start cue,
                        // but that doesn't seem to be something that can be easily done with rodio
                        |s| Box::new(s.low_pass(500).amplify(1.5)),
                    );
                }
                // Notify play time tracker (already paused, just confirming stop)
                self.app_state
                    .play_time_state
                    .write()
                    .unwrap()
                    .handle_transition(PlayTimeTransition {
                        is_recording: false,
                        due_to_idle: false,
                    });
                RecordingState::Idle
            }
            (RecordingState::Recording, RecordingState::Recording) => {
                // Restart the currently active recording
                // Here we intentionally set honk to false, we don't want audio cue to occur
                // on an intended recording restart and confuse the user
                let supported_games = self.app_state.supported_games.read().unwrap().clone();
                stop_recording_with_notification(
                    &mut self.recorder,
                    &self.input_capture,
                    Some((&self.sink, false, &self.app_state)),
                    &mut self.cue_cache,
                )
                .await?;
                start_recording_safely(
                    &mut self.recorder,
                    &self.input_capture,
                    &supported_games,
                    Some((&self.sink, false, &self.app_state)),
                    &mut self.cue_cache,
                )
                .await?;
                self.last_active = Instant::now();
                RecordingState::Recording
            }
            (old_state, new_state) => {
                // ????
                panic!("Invalid state transition: {old_state:?} -> {new_state:?}");
            }
        };
        Ok(())
    }

    /// Triggers auto-upload if the preference is enabled.
    /// Should be called after a recording is completed/saved.
    async fn maybe_trigger_auto_upload(&self) {
        let auto_upload_enabled = self
            .app_state
            .config
            .read()
            .unwrap()
            .preferences
            .auto_upload_on_completion;

        if !auto_upload_enabled {
            return;
        }

        let upload_in_progress = self
            .app_state
            .upload_in_progress
            .load(std::sync::atomic::Ordering::SeqCst);

        if upload_in_progress {
            // Upload already in progress, queue this one
            let current_count = self
                .app_state
                .auto_upload_queue_count
                .load(std::sync::atomic::Ordering::SeqCst);
            let new_count = current_count + 1;
            tracing::info!(
                "Auto-upload: upload in progress, queued recording (queue count: {})",
                new_count
            );
            set_auto_upload_queue_count(&self.app_state, new_count);
        } else {
            // No upload in progress, start one now
            tracing::info!("Auto-upload: starting upload for completed recording");
            self.app_state
                .async_request_tx
                .send(AsyncRequest::UploadData)
                .await
                .ok();
        }
    }
}

/// Attempts to start the recording.
/// If it fails, it will emit an error and stop the current recording, in whatever state it may be in.
///
/// If `notification_state` is `Some`, it will be used to notify of the recording state change.
/// TODO: refactor the function signature to match the Result<()> pattern used in stop_recording
async fn start_recording_safely(
    recorder: &mut Recorder,
    input_capture: &InputCapture,
    supported_games: &SupportedGames,
    notification_state: Option<(&Sink, bool, &AppState)>,
    cue_cache: &mut HashMap<String, Vec<u8>>,
) -> Result<()> {
    if let Err(e) = recorder.start(input_capture, supported_games).await {
        tracing::error!(e=?e, "Failed to start recording");
        error_message_box(&e.to_string());
        recorder.stop(input_capture).await.ok();
        Err(e)
    } else {
        if let Some((sink, honk, app_state)) = notification_state {
            notify_of_recording_state_change(sink, honk, app_state, true, cue_cache);
        }
        Ok(())
    }
}

async fn stop_recording_with_notification(
    recorder: &mut Recorder,
    input_capture: &InputCapture,
    notification_state: Option<(&Sink, bool, &AppState)>,
    cue_cache: &mut HashMap<String, Vec<u8>>,
) -> Result<()> {
    recorder.stop(input_capture).await?;
    if let Some((sink, honk, app_state)) = notification_state {
        notify_of_recording_state_change(sink, honk, app_state, false, cue_cache);
        // refresh the uploads
        app_state
            .async_request_tx
            .send(AsyncRequest::LoadLocalRecordings)
            .await
            .ok();
    }
    Ok(())
}

fn notify_of_recording_state_change(
    sink: &Sink,
    should_play_sound: bool,
    app_state: &AppState,
    is_recording: bool,
    cue_cache: &mut HashMap<String, Vec<u8>>,
) {
    app_state
        .ui_update_tx
        .send(UiUpdate::UpdateRecordingState(is_recording))
        .ok();
    if should_play_sound {
        // Get selected cue filenames
        let cue_filename = {
            let cfg = app_state.config.read().unwrap();
            if is_recording {
                cfg.preferences.audio_cues.start_recording.clone()
            } else {
                cfg.preferences.audio_cues.stop_recording.clone()
            }
        };
        play_cue(sink, app_state, &cue_filename, cue_cache, |s| s);
    }
}

fn play_cue(
    sink: &Sink,
    app_state: &AppState,
    filename: &str,
    cue_cache: &mut HashMap<String, Vec<u8>>,
    source_transformer: impl FnOnce(
        Box<dyn Source + Send + 'static>,
    ) -> Box<dyn Source + Send + 'static>,
) {
    // Apply configured honk volume (0-255 -> 0.0-1.0)
    let volume =
        (app_state.config.read().unwrap().preferences.honk_volume as f32 / 255.0).clamp(0.0, 1.0);

    sink.set_volume(volume);

    // Load the selected cue file with a per-thread cache
    let cue_bytes = cue_cache
        .entry(filename.to_string())
        .or_insert_with(|| load_cue_bytes(filename))
        .clone();
    let source = match Decoder::new_mp3(Cursor::new(cue_bytes)) {
        Ok(source) => source,
        Err(e) => {
            tracing::error!(e=?e, "Failed to decode recording notification sound");
            return;
        }
    };
    let source = source_transformer(Box::new(source));

    // Stop any currently playing audio and clear the queue, then play new audio cue immediately
    sink.stop();
    sink.append(source);
    sink.play();
}

/// Helper to update the auto-upload queue count in AppState and notify the UI.
/// Always use this instead of directly modifying `auto_upload_queue_count` to keep them in sync.
fn set_auto_upload_queue_count(app_state: &AppState, count: usize) {
    app_state
        .auto_upload_queue_count
        .store(count, std::sync::atomic::Ordering::SeqCst);
    app_state
        .ui_update_tx
        .send(UiUpdate::UpdateAutoUploadQueueCount(count))
        .ok();
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

fn get_foregrounded_game(
    supported_games: &SupportedGames,
    recorder: &Recorder,
) -> Option<ForegroundedGame> {
    let (exe_name, _, hwnd) = crate::record::get_foregrounded_game().ok().flatten()?;

    // Check if game is supported
    let exe_without_ext = std::path::Path::new(&exe_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&exe_name)
        .to_lowercase();

    let supported_game = supported_games.get(&exe_without_ext.clone());
    let unsupported_reason = if supported_game.is_none() {
        Some("Not on the games list.".to_string())
    } else if !recorder.is_window_capturable(hwnd) {
        Some(
            "Recorder cannot capture this window. Try running OWL Control in admin mode."
                .to_string(),
        )
    } else {
        None
    };

    Some(ForegroundedGame {
        exe_name: Some(exe_name),
        unsupported_reason,
    })
}

async fn pick_recording_folder(app_state: Arc<AppState>, current_location: PathBuf) {
    let mut dialog = rfd::AsyncFileDialog::new();
    if current_location.exists() {
        dialog = dialog.set_directory(&current_location);
    };

    if let Some(picked) = dialog.pick_folder().await {
        // Send the result back to the UI
        app_state
            .ui_update_tx
            .send(UiUpdate::FolderPickerResult {
                old_path: current_location,
                new_path: picked.path().into(),
            })
            .ok();
    }
}

async fn move_recordings_folder(app_state: Arc<AppState>, from: PathBuf, to: PathBuf) {
    // Check if the directories are the same
    if from == to {
        tracing::info!("Source and destination are the same, skipping move operation");
        return;
    }

    tracing::info!(
        "Moving recordings from {} to {}",
        from.display(),
        to.display()
    );

    // Ensure the destination directory exists
    if let Err(e) = tokio::fs::create_dir_all(&to).await {
        tracing::error!(
            "Failed to create destination directory {}: {:?}",
            to.display(),
            e
        );
        tracing::error!(
            "Move operation failed: Failed to create destination directory: {}",
            e
        );
        return;
    }

    // Read all entries in the source directory
    let mut entries = match tokio::fs::read_dir(&from).await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::error!(
                "Failed to read source directory {}: {:?}",
                from.display(),
                e
            );
            tracing::error!(
                "Move operation failed: Failed to read source directory: {}",
                e
            );
            return;
        }
    };

    let mut moved_count = 0;
    let mut errors = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        let source_path = entry.path();
        let file_name = match source_path.file_name() {
            Some(name) => name,
            None => continue,
        };

        let dest_path = to.join(file_name);

        // Move the file or directory
        if let Err(e) = tokio::fs::rename(&source_path, &dest_path).await {
            tracing::error!(
                "Failed to move {} to {}: {:?}",
                source_path.display(),
                dest_path.display(),
                e
            );
            errors.push(file_name.to_string_lossy().to_string());
        } else {
            moved_count += 1;
        }
    }

    if errors.is_empty() {
        tracing::info!("Successfully moved {} recordings", moved_count);
        tracing::info!("Move operation completed: {} items moved", moved_count);
    } else {
        tracing::warn!(
            "Moved {} recordings, but failed to move {} items: {:?}",
            moved_count,
            errors.len(),
            errors
        );
        tracing::error!(
            "Move operation completed with errors: Failed to move {} items",
            errors.len()
        );
    }

    // Refresh the local recordings list
    let recording_location = app_state
        .config
        .read()
        .unwrap()
        .preferences
        .recording_location
        .clone();

    let local_recordings =
        tokio::task::spawn_blocking(move || LocalRecording::scan_directory(&recording_location))
            .await
            .unwrap_or_default();

    app_state
        .ui_update_tx
        .send(UiUpdate::UpdateLocalRecordings(local_recordings))
        .ok();
}

async fn startup_requests(app_state: Arc<AppState>) {
    if cfg!(debug_assertions) {
        tracing::info!("Skipping fetch of supported games in dev/debug build");
    } else {
        tokio::spawn({
            let async_request_tx = app_state.async_request_tx.clone();
            async move {
                match get_supported_games().await {
                    Ok(games) => {
                        async_request_tx
                            .send(AsyncRequest::UpdateSupportedGames(games))
                            .await
                            .ok();
                    }
                    Err(e) => {
                        tracing::error!(e=?e, "Failed to get supported games from GitHub");
                    }
                }
            }
        });
    }

    tokio::spawn(async move {
        if let Err(e) = check_for_updates(app_state).await {
            tracing::error!(e=?e, "Failed to check for updates");
        }
    });
}

async fn get_supported_games() -> Result<SupportedGames> {
    let text = reqwest::get(format!("https://raw.githubusercontent.com/{GH_ORG}/{GH_REPO}/refs/heads/main/crates/constants/src/supported_games.json"))
        .await
        .context("Failed to request supported games from GitHub")?
        .text()
        .await
        .context("Failed to get text of supported games from GitHub")?;

    // Use spawn_blocking since load_from_str now does Steam detection (blocking I/O)
    tokio::task::spawn_blocking(move || {
        SupportedGames::load_from_str(&text).context("Failed to parse supported games from GitHub")
    })
    .await
    .unwrap()
}

async fn check_for_updates(app_state: Arc<AppState>) -> Result<()> {
    #[derive(serde::Deserialize, Debug, Clone)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }

    #[derive(serde::Deserialize, Debug, Clone)]
    struct Release {
        html_url: String,
        published_at: Option<chrono::DateTime<chrono::Utc>>,
        tag_name: String,
        name: String,
        draft: bool,
        prerelease: bool,
        assets: Vec<Asset>,
    }

    let current_version = env!("CARGO_PKG_VERSION");

    let releases = reqwest::Client::builder()
        .build()?
        .get(format!(
            "https://api.github.com/repos/{GH_ORG}/{GH_REPO}/releases"
        ))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", format!("OWL Control v{current_version}"))
        .send()
        .await
        .context("Failed to get releases from GitHub")?
        .json::<Vec<Release>>()
        .await
        .context("Failed to parse releases from GitHub")?;

    let latest_valid_release = releases.iter().find(|r| {
        !r.draft
        // filter out prereleases that we don't want users to automatically install
        && !r.prerelease
    });
    tracing::info!(latest_valid_release=?latest_valid_release, "Fetched latest valid release");

    if let Some(latest_valid_release) = latest_valid_release.cloned()
        && is_version_newer(current_version, &latest_valid_release.tag_name)
    {
        // Find the Windows installer asset (.exe file)
        let download_url = latest_valid_release
            .assets
            .iter()
            .find(|asset| asset.name.ends_with(".exe"))
            .map(|asset| asset.browser_download_url.clone())
            .unwrap_or(latest_valid_release.html_url.clone());

        app_state
            .ui_update_tx
            .send(UiUpdate::UpdateNewerReleaseAvailable(GitHubRelease {
                name: latest_valid_release.name,
                release_notes_url: latest_valid_release.html_url,
                download_url,
                release_date: latest_valid_release.published_at,
            }))
            .ok();

        app_state
            .is_out_of_date
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    Ok(())
}
