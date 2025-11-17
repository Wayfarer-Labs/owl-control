use crate::{
    api::ApiClient,
    app_state::{
        AppState, AsyncRequest, ForegroundedGame, GitHubRelease, ListeningForNewHotkey,
        RecordingStatus, UiUpdate,
    },
    assets::load_cue_bytes,
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

use constants::{
    GH_ORG, GH_REPO, MAX_FOOTAGE, MAX_IDLE_DURATION, unsupported_games::UnsupportedGames,
};
use game_process::does_process_exist;
use input_capture::InputCapture;
use rodio::{Decoder, Sink};
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
    let stream_handle =
        rodio::OutputStreamBuilder::open_default_stream().expect("open default audio stream");
    let sink = Sink::connect_new(stream_handle.mixer());
    // Cache for audio cues
    let mut cue_cache: HashMap<String, Vec<u8>> = HashMap::new();

    let mut recorder = Recorder::new(
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
    let (input_capture, mut input_rx) = InputCapture::new()?;

    let mut ctrlc_rx = wait_for_ctrl_c();

    let mut last_active = Instant::now();
    let mut start_on_activity = false;
    let mut actively_recording_window: Option<HWND> = None;

    let mut perform_checks = tokio::time::interval(Duration::from_secs(1));
    perform_checks.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut debouncer = EventDebouncer::new();

    let api_client = Arc::new(ApiClient::new());
    let mut valid_api_key_and_user_id: Option<(String, String)> = None;

    let mut unsupported_games = UnsupportedGames::load_from_embedded();

    // Initial async requests to GitHub/server
    tokio::spawn(startup_requests(app_state.clone()));

    loop {
        let (honk, start_key, stop_key) = {
            let cfg = app_state.config.read().unwrap();
            (
                cfg.preferences.honk,
                name_to_virtual_keycode(cfg.preferences.start_recording_key()),
                name_to_virtual_keycode(cfg.preferences.stop_recording_key()),
            )
        };
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

                if let Some(key) = e.key_press_keycode() {
                    let listening_for_new_hotkey = *app_state.listening_for_new_hotkey.read().unwrap();
                    if let ListeningForNewHotkey::Listening { target } = listening_for_new_hotkey {
                        *app_state.listening_for_new_hotkey.write().unwrap() = ListeningForNewHotkey::Captured { target, key };
                    }
                }

                if let Err(e) = recorder.seen_input(e).await {
                    tracing::error!(e=?e, "Failed to seen input");
                }
                if let Some(key) = e.key_press_keycode() && *app_state.listening_for_new_hotkey.read().unwrap() == ListeningForNewHotkey::NotListening {
                    if Some(key) == start_key && recorder.recording().is_none() {
                        if !app_state.is_out_of_date.load(std::sync::atomic::Ordering::SeqCst) {
                            tracing::info!("Start key pressed, starting recording");
                            if start_recording_safely(
                                &mut recorder,
                                &input_capture,
                                &unsupported_games,
                                Some((&sink, honk, &app_state)),
                                &mut cue_cache,
                            )
                            .await
                            {
                                actively_recording_window = recorder.recording().as_ref().map(|r| r.hwnd());
                                tracing::info!("Recording started with HWND {actively_recording_window:?}");
                            }
                        } else {
                            error_message_box(
                                concat!(
                                    "You are using an outdated version of OWL Control. ",
                                    "Please update to the latest version to continue.\n\n",
                                    "Recording and uploading will be blocked until you update."
                                )
                            );
                        }
                    } else if Some(key) == stop_key && recorder.recording().is_some() {
                        tracing::info!("Stop key pressed, stopping recording");
                        if let Err(e) = stop_recording_with_notification(
                            &mut recorder,
                            &input_capture,
                            &sink,
                            honk,
                            &app_state,
                            &mut cue_cache,
                        )
                        .await
                        {
                            tracing::error!(e=?e, "Failed to stop recording on stop key");
                        }

                        actively_recording_window = None;
                        start_on_activity = false;
                    }
                } else if start_on_activity && actively_recording_window.is_some_and(is_window_focused) {
                    tracing::info!("Input detected, restarting recording");
                    if start_recording_safely(
                        &mut recorder,
                        &input_capture,
                        &unsupported_games,
                        Some((&sink, honk, &app_state)),
                        &mut cue_cache,
                    )
                    .await
                    {
                        start_on_activity = false;
                    }
                }
                last_active = Instant::now();
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
                        let response = api_client.validate_api_key(&api_key).await;
                        tracing::info!("Received response from API key validation: {response:?}");

                        valid_api_key_and_user_id = response.as_ref().ok().map(|s| (api_key.clone(), s.clone()));
                        app_state
                            .ui_update_tx
                            .send(UiUpdate::UpdateUserId(response.map_err(|e| e.to_string())))
                            .ok();

                        if valid_api_key_and_user_id.is_some() {
                            app_state.async_request_tx.send(AsyncRequest::LoadUploadStats).await.ok();
                            app_state.async_request_tx.send(AsyncRequest::LoadLocalRecordings).await.ok();
                        }
                    }
                    AsyncRequest::UploadData => {
                        tokio::spawn(upload::start(app_state.clone(), api_client.clone(), recording_location.clone()));
                    }
                    AsyncRequest::CancelUpload => {
                        app_state.upload_cancel_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        tracing::info!("Upload cancellation requested");
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
                    AsyncRequest::UpdateUnsupportedGames(new_games) => {
                        let old_game_count = unsupported_games.games.len();
                        unsupported_games = new_games.clone();
                        tracing::info!(
                            "Updated unsupported games, old count: {old_game_count}, new count: {}",
                            unsupported_games.games.len()
                        );
                    }
                    AsyncRequest::LoadUploadStats => {
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
                        tokio::spawn({
                            let app_state = app_state.clone();
                            async move {
                                // Get current list of local recordings
                                let local_recordings = tokio::task::spawn_blocking({
                                    let recording_location = recording_location.clone();
                                    move || LocalRecording::scan_directory(&recording_location)
                                }).await.unwrap_or_default();

                                // Filter only invalid recordings and collect paths to delete
                                let invalid_folders_to_delete: Vec<_> = local_recordings.iter()
                                    .filter_map(|r| {
                                        match r {
                                            LocalRecording::Invalid { info, .. } => {
                                                Some((info.folder_name.clone(), info.folder_path.clone()))
                                            }
                                            _ => None,
                                        }
                                    })
                                    .collect();

                                if invalid_folders_to_delete.is_empty() {
                                    tracing::info!("No invalid recordings to delete");
                                    return;
                                }

                                let total_count = invalid_folders_to_delete.len();
                                tracing::info!("Deleting {} invalid recordings", total_count);

                                // Delete all invalid recording folders
                                let mut errors = vec![];
                                for (folder_name, folder_path) in invalid_folders_to_delete.iter() {
                                    if let Err(e) = tokio::fs::remove_dir_all(folder_path).await {
                                        tracing::error!(
                                            "Failed to delete invalid recording folder {}: {:?}",
                                            folder_path.display(),
                                            e
                                        );
                                        errors.push(folder_name.clone());
                                    } else {
                                        tracing::info!(
                                            "Deleted invalid recording folder: {}",
                                            folder_path.display()
                                        );
                                    }
                                }

                                if errors.is_empty() {
                                    tracing::info!("Successfully deleted all {} invalid recordings", total_count);
                                } else {
                                    tracing::warn!("Failed to delete {} recordings: {:?}", errors.len(), errors);
                                }


                                app_state.async_request_tx.send(AsyncRequest::LoadLocalRecordings).await.ok();
                            }
                        });
                    }
                    AsyncRequest::DeleteRecording(path) => {
                        if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                            tracing::error!(e=?e, "Failed to delete recording folder {}: {:?}", path.display(), e);
                        } else {
                            tracing::info!("Deleted recording folder: {}", path.display());
                        }

                        app_state.async_request_tx.send(AsyncRequest::LoadLocalRecordings).await.ok();
                    }
                    AsyncRequest::MoveRecordingsFolder { from, to } => {
                        tokio::spawn(move_recordings_folder(app_state.clone(), from, to));
                    }
                    AsyncRequest::PickRecordingFolder { current_location } => {
                        tokio::spawn(pick_recording_folder(app_state.clone(), current_location));
                    }
                }
            },
            _ = perform_checks.tick() => {
                // Flush pending input events to disk
                if let Err(e) = recorder.flush_input_events().await {
                    tracing::error!(e=?e, "Failed to flush input events");
                }

                // Check foregrounded game
                *app_state.last_foregrounded_game.write().unwrap() = get_foregrounded_game(&unsupported_games, &recorder);

                // Update recording state
                if let Some(recording) = recorder.recording() {
                    if !does_process_exist(recording.pid()).unwrap_or_default() {
                        tracing::info!(pid=recording.pid().0, "Game process no longer exists, stopping recording");
                        if let Err(e) = stop_recording_with_notification(
                            &mut recorder,
                            &input_capture,
                            &sink,
                            honk,
                            &app_state,
                            &mut cue_cache,
                        )
                        .await
                        {
                            tracing::error!(e=?e, "Failed to stop recording on game process exit");
                        }
                    } else if last_active.elapsed() > MAX_IDLE_DURATION {
                        tracing::info!("No input detected for {} seconds, stopping recording", MAX_IDLE_DURATION.as_secs());
                        if let Err(e) = stop_recording_with_notification(
                            &mut recorder,
                            &input_capture,
                            &sink,
                            honk,
                            &app_state,
                            &mut cue_cache,
                        )
                        .await
                        {
                            tracing::error!(e=?e, "Failed to stop recording on idle timeout");
                        }
                        *app_state.state.write().unwrap() = RecordingStatus::Paused;
                        start_on_activity = true;
                    } else if recording.elapsed() > MAX_FOOTAGE {
                        tracing::info!("Recording duration exceeded {} s, restarting recording", MAX_FOOTAGE.as_secs());
                        // We intentionally do not notify of recording state change here because we're restarting the recording
                        if let Err(e) = recorder.stop(&input_capture).await {
                            tracing::error!(e=?e, "Failed to stop recording on recording duration exceeded");
                        }
                        start_recording_safely(
                            &mut recorder,
                            &input_capture,
                            &unsupported_games,
                            None,
                            &mut cue_cache,
                        )
                        .await;
                        last_active = Instant::now();
                    } else if let Some(window) = actively_recording_window && !is_window_focused(window) {
                        tracing::info!("Window {window:?} lost focus, stopping recording");
                        if let Err(e) = stop_recording_with_notification(
                            &mut recorder,
                            &input_capture,
                            &sink,
                            honk,
                            &app_state,
                            &mut cue_cache,
                        )
                        .await
                        {
                            tracing::error!(e=?e, "Failed to stop recording on window lost focus");
                        }
                    } else if let Ok(current_resolution) = get_recording_base_resolution(recording.hwnd()) {
                        // Check if the window resolution has changed and restart the recording
                        if current_resolution != recording.game_resolution() {
                            tracing::info!(
                                old_resolution=?recording.game_resolution(),
                                new_resolution=?current_resolution,
                                "Window resolution changed, restarting recording"
                            );
                            // We intentionally do not notify of recording state change here because we're restarting the recording
                            if let Err(e) = recorder.stop(&input_capture).await {
                                tracing::error!(e=?e, "Failed to stop recording on resolution change");
                            }
                            start_recording_safely(&mut recorder, &input_capture, &unsupported_games, None, &mut cue_cache).await;
                            last_active = Instant::now();
                        }
                    }
                } else if let Some(window) = actively_recording_window && is_window_focused(window) && !start_on_activity {
                    // If we're not currently in a recording, but we were actively recording this window, and this window
                    // is now focused, and we're not waiting on input, let's restart the recording.
                    tracing::info!("Window {window:?} regained focus, restarting recording");
                    start_recording_safely(
                        &mut recorder,
                        &input_capture,
                        &unsupported_games,
                        Some((&sink, honk, &app_state)),
                        &mut cue_cache,
                    )
                    .await;
                }

                recorder.poll().await;

                // Periodically force the UI to rerender so that it will process events, even if not visible
                app_state.ui_update_tx.send(UiUpdate::ForceUpdate).ok();
            },
        }
    }

    if let Err(e) = recorder.stop(&input_capture).await {
        tracing::error!(e=?e, "Failed to stop recording on shutdown");
    }
    Ok(())
}

/// Attempts to start the recording.
/// If it fails, it will emit an error and stop the current recording, in whatever state it may be in.
///
/// If `notification_state` is `Some`, it will be used to notify of the recording state change.
async fn start_recording_safely(
    recorder: &mut Recorder,
    input_capture: &InputCapture,
    unsupported_games: &UnsupportedGames,
    notification_state: Option<(&Sink, bool, &AppState)>,
    cue_cache: &mut HashMap<String, Vec<u8>>,
) -> bool {
    if let Err(e) = recorder.start(input_capture, unsupported_games).await {
        tracing::error!(e=?e, "Failed to start recording");
        error_message_box(&e.to_string());
        recorder.stop(input_capture).await.ok();
        false
    } else {
        if let Some((sink, honk, app_state)) = notification_state {
            notify_of_recording_state_change(sink, honk, app_state, true, cue_cache);
        }
        true
    }
}

async fn stop_recording_with_notification(
    recorder: &mut Recorder,
    input_capture: &InputCapture,
    sink: &Sink,
    honk: bool,
    app_state: &AppState,
    cue_cache: &mut HashMap<String, Vec<u8>>,
) -> Result<()> {
    recorder.stop(input_capture).await?;
    notify_of_recording_state_change(sink, honk, app_state, false, cue_cache);
    // refresh the uploads
    app_state
        .async_request_tx
        .send(AsyncRequest::LoadLocalRecordings)
        .await
        .ok();
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
        // Apply configured honk volume (0-255 -> 0.0-1.0) and get selected cue filenames
        let (volume, cue_filename) = {
            let cfg = app_state.config.read().unwrap();
            let vol = (cfg.preferences.honk_volume as f32 / 255.0).clamp(0.0, 1.0);
            let filename = if is_recording {
                cfg.preferences.audio_cues.start_recording.clone()
            } else {
                cfg.preferences.audio_cues.stop_recording.clone()
            };
            (vol, filename)
        };

        sink.set_volume(volume);

        // Load the selected cue file with a per-thread cache
        let cue_bytes = cue_cache
            .entry(cue_filename.clone())
            .or_insert_with(|| load_cue_bytes(&cue_filename))
            .clone();
        let source = Decoder::new_mp3(Cursor::new(cue_bytes));

        match source {
            Ok(source) => {
                // Stop any currently playing audio and clear the queue, then play new audio cue immediately
                sink.stop();
                sink.append(source);
                sink.play();
            }
            Err(e) => {
                tracing::error!(e=?e, "Failed to decode recording notification sound");
            }
        }
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

fn get_foregrounded_game(
    unsupported_games: &UnsupportedGames,
    recorder: &Recorder,
) -> Option<ForegroundedGame> {
    let (exe_name, _, hwnd) = crate::record::get_foregrounded_game().ok().flatten()?;

    // Check if game is supported
    let exe_without_ext = std::path::Path::new(&exe_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&exe_name)
        .to_lowercase();

    let unsupported_game = unsupported_games.get(exe_without_ext.clone());
    let mut unsupported_reason = unsupported_game.map(|ug| ug.reason.to_string());

    if unsupported_game.is_none() && !recorder.is_window_capturable(hwnd) {
        unsupported_reason = Some("The recorder cannot capture this window.".to_string());
    }

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
        tracing::info!("Skipping fetch of unsupported games in dev/debug build");
    } else {
        tokio::spawn({
            let async_request_tx = app_state.async_request_tx.clone();
            async move {
                match get_unsupported_games().await {
                    Ok(games) => {
                        async_request_tx
                            .send(AsyncRequest::UpdateUnsupportedGames(games))
                            .await
                            .ok();
                    }
                    Err(e) => {
                        tracing::error!(e=?e, "Failed to get unsupported games from GitHub");
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

async fn get_unsupported_games() -> Result<UnsupportedGames> {
    let text = reqwest::get(format!("https://raw.githubusercontent.com/{GH_ORG}/{GH_REPO}/refs/heads/main/crates/constants/src/unsupported_games.json"))
        .await
        .context("Failed to request unsupported games from GitHub")?
        .text()
        .await
        .context("Failed to get text of unsupported games from GitHub")?;
    UnsupportedGames::load_from_str(&text).context("Failed to parse unsupported games from GitHub")
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
