use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use futures::TryStreamExt as _;
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt as _},
    sync::mpsc,
    task::JoinError,
};

use crate::{
    api::{ApiClient, ApiError, CompleteMultipartUploadChunk, InitMultipartUploadArgs},
    app_state::{self, AppState, AsyncRequest, UiUpdate, UiUpdateUnreliable},
    record::{LocalRecording, LocalRecordingPaused, UploadProgressState},
    validation::{ValidationResult, validate_folder},
};

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
pub struct FileProgress {
    pub current_file: String,
    pub files_remaining: u64,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
pub struct ProgressData {
    pub bytes_uploaded: u64,
    pub total_bytes: u64,
    pub speed_mbps: f64,
    pub eta_seconds: f64,
    pub percent: f64,
    pub file_progress: FileProgress,
}

pub async fn start(
    app_state: Arc<AppState>,
    api_client: Arc<ApiClient>,
    recording_location: PathBuf,
) {
    let reliable_tx = app_state.ui_update_tx.clone();
    let unreliable_tx = app_state.ui_update_unreliable_tx.clone();
    let cancel_flag = app_state.upload_cancel_flag.clone();

    // Reset cancel flag at start of upload
    cancel_flag.store(false, std::sync::atomic::Ordering::SeqCst);

    let (api_token, unreliable_connection, delete_uploaded) = {
        let config = app_state.config.read().unwrap();
        (
            config.credentials.api_key.clone(),
            config.preferences.unreliable_connection,
            config.preferences.delete_uploaded_files,
        )
    };

    app_state
        .ui_update_unreliable_tx
        .send(UiUpdateUnreliable::UpdateUploadProgress(Some(
            ProgressData::default(),
        )))
        .ok();

    if let Err(e) = run(
        &recording_location,
        api_client,
        api_token,
        unreliable_connection,
        delete_uploaded,
        reliable_tx.clone(),
        unreliable_tx.clone(),
        app_state.async_request_tx.clone(),
        cancel_flag,
    )
    .await
    {
        tracing::error!(e=?e, "Error uploading recordings");
    }

    for req in [
        AsyncRequest::LoadUploadStats,
        AsyncRequest::LoadLocalRecordings,
    ] {
        app_state.async_request_tx.send(req).await.ok();
    }
    unreliable_tx
        .send(UiUpdateUnreliable::UpdateUploadProgress(None))
        .ok();
}

/// Separate function to allow for fallibility
#[allow(clippy::too_many_arguments)]
async fn run(
    recording_location: &Path,
    api_client: Arc<ApiClient>,
    api_token: String,
    unreliable_connection: bool,
    delete_uploaded: bool,
    reliable_tx: app_state::UiUpdateSender,
    unreliable_tx: app_state::UiUpdateUnreliableSender,
    async_req_tx: mpsc::Sender<AsyncRequest>,
    pause_flag: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), UploadFolderError> {
    // Scan all local recordings and filter to only Paused and Unuploaded
    let recordings_to_upload: Vec<_> = LocalRecording::scan_directory(recording_location)
        .into_iter()
        .filter(|rec| {
            matches!(
                rec,
                LocalRecording::Paused(_) | LocalRecording::Unuploaded { .. }
            )
        })
        .collect();

    let total_files_to_upload = recordings_to_upload.len() as u64;
    let mut files_uploaded = 0u64;

    let mut last_upload_time = std::time::Instant::now();
    let reload_every_n_files = 5;
    let reload_if_at_least_has_passed = std::time::Duration::from_secs(2 * 60);
    for recording in recordings_to_upload {
        // Check if upload has been cancelled
        if pause_flag.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }

        let info = recording.info().clone();
        let path = info.folder_path.clone();

        let file_progress = FileProgress {
            current_file: info.folder_name.clone(),
            files_remaining: total_files_to_upload.saturating_sub(files_uploaded),
        };

        let result = upload_folder(
            recording,
            api_client.clone(),
            &api_token,
            unreliable_connection,
            unreliable_tx.clone(),
            pause_flag.clone(),
            file_progress,
        )
        .await;

        let recording_to_delete = match result {
            Ok(UploadTarOutput::Success(recording)) => Some(recording),
            Ok(UploadTarOutput::ServerInvalid(_recording)) => {
                // We intentionally choose not to delete server invalid recordings, so that the user can learn why it was invalidated
                None
            }
            Ok(UploadTarOutput::Paused(_recording)) => {
                // We intentionally choose not to delete paused recordings, as they are still valid and can be resumed
                None
            }
            Err(e) => {
                tracing::error!("Error uploading folder {}: {:?}", path.display(), e);
                reliable_tx.send(UiUpdate::UploadFailed(e.to_string())).ok();
                continue;
            }
        };

        files_uploaded += 1;

        // delete the uploaded recording directory if the preference is enabled
        if delete_uploaded && let Some(uploaded_recording) = recording_to_delete {
            let path = path.display();
            match uploaded_recording.delete(&api_client, &api_token).await {
                Ok(_) => {
                    tracing::info!("Deleted uploaded directory: {path}");
                }
                Err(e) => {
                    tracing::error!("Failed to delete uploaded directory {path}: {e:?}");
                }
            }
        }

        let should_reload = if files_uploaded.is_multiple_of(reload_every_n_files) {
            tracing::info!(
                "{} files uploaded, reloading upload stats and local recordings",
                files_uploaded
            );
            true
        } else if last_upload_time.elapsed() > reload_if_at_least_has_passed {
            tracing::info!(
                "{} seconds since last upload, reloading upload stats and local recordings",
                last_upload_time.elapsed().as_secs()
            );
            true
        } else {
            false
        };

        if should_reload {
            for req in [
                AsyncRequest::LoadUploadStats,
                AsyncRequest::LoadLocalRecordings,
            ] {
                async_req_tx.send(req).await.ok();
            }
        }
        last_upload_time = std::time::Instant::now();
    }

    Ok(())
}

#[derive(Debug)]
enum UploadFolderError {
    Io(std::io::Error),
    InitMultipartUpload(ApiError),
    CreateTar(CreateTarError),
    FailedToGetFileSize(PathBuf, std::io::Error),
    MissingFilename(PathBuf),
    UploadTar(UploadTarError),
    Json(serde_json::Error),
    Join(JoinError),
    Validation(color_eyre::eyre::Report),
}
impl std::error::Error for UploadFolderError {}
impl std::fmt::Display for UploadFolderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UploadFolderError::Io(e) => write!(f, "I/O error: {e}"),
            UploadFolderError::InitMultipartUpload(e) => {
                write!(f, "Init multipart upload error: {e}")
            }
            UploadFolderError::CreateTar(e) => write!(f, "Create tar error: {e}"),
            UploadFolderError::FailedToGetFileSize(path, e) => {
                write!(f, "Failed to get file size for {path:?}: {e}")
            }
            UploadFolderError::MissingFilename(path) => write!(f, "Missing filename: {path:?}"),
            UploadFolderError::UploadTar(e) => write!(f, "Upload tar error: {e}"),
            UploadFolderError::Json(e) => write!(f, "JSON error: {e}"),
            UploadFolderError::Join(e) => write!(f, "Join error: {e}"),
            UploadFolderError::Validation(e) => write!(f, "Validation error: {e}"),
        }
    }
}
impl From<std::io::Error> for UploadFolderError {
    fn from(e: std::io::Error) -> Self {
        UploadFolderError::Io(e)
    }
}
impl From<CreateTarError> for UploadFolderError {
    fn from(e: CreateTarError) -> Self {
        UploadFolderError::CreateTar(e)
    }
}
impl From<UploadTarError> for UploadFolderError {
    fn from(e: UploadTarError) -> Self {
        UploadFolderError::UploadTar(e)
    }
}
impl From<serde_json::Error> for UploadFolderError {
    fn from(e: serde_json::Error) -> Self {
        UploadFolderError::Json(e)
    }
}
impl From<JoinError> for UploadFolderError {
    fn from(e: JoinError) -> Self {
        UploadFolderError::Join(e)
    }
}
async fn upload_folder(
    recording: LocalRecording,
    api_client: Arc<ApiClient>,
    api_token: &str,
    unreliable_connection: bool,
    unreliable_tx: app_state::UiUpdateUnreliableSender,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    file_progress: FileProgress,
) -> Result<UploadTarOutput, UploadFolderError> {
    // Validate paused recording (may convert to Unuploaded if expired/invalid)
    let recording = validate_potentially_paused_recording(recording, &api_client, api_token).await;
    let path = recording.info().folder_path.clone();

    // Branch on variant - two paths: resume existing or start fresh
    let paused = match recording {
        LocalRecording::Paused(paused) => {
            // Resume: use existing tar and state directly (skip folder validation)
            tracing::info!("Resuming upload from saved state");
            tracing::info!(
                "Using existing tar file: {}",
                paused.upload_progress.tar_path.display()
            );

            paused
        }
        LocalRecording::Unuploaded { info, metadata } => {
            // Fresh: validate, create tar, init upload
            tracing::info!("Validating folder {}", path.display());
            let validation = tokio::task::spawn_blocking({
                let path = path.clone();
                move || validate_folder(&path)
            })
            .await?
            .map_err(UploadFolderError::Validation)?;

            tracing::info!("Creating tar file for {}", path.display());
            let tar_path = create_tar_file(&path, &validation).await?;

            // Initialize new upload session
            let file_size = std::fs::metadata(&tar_path)
                .map(|m| m.len())
                .map_err(|e| UploadFolderError::FailedToGetFileSize(tar_path.to_owned(), e))?;

            fn get_filename(path: &Path) -> Result<String, UploadFolderError> {
                Ok(path
                    .file_name()
                    .ok_or_else(|| UploadFolderError::MissingFilename(path.to_owned()))?
                    .to_string_lossy()
                    .as_ref()
                    .to_string())
            }

            let init_response = api_client
                .init_multipart_upload(
                    api_token,
                    tar_path
                        .file_name()
                        .ok_or_else(|| UploadTarError::FailedToGetTarFilename(tar_path.to_owned()))?
                        .to_string_lossy()
                        .as_ref(),
                    file_size,
                    &crate::system::hardware_id::get()
                        .map_err(|e| UploadTarError::FailedToGetHardwareId(e.to_string()))?,
                    InitMultipartUploadArgs {
                        tags: None,
                        video_filename: Some(&get_filename(&validation.mp4_path)?),
                        control_filename: Some(&get_filename(&validation.csv_path)?),
                        video_duration_seconds: Some(validation.metadata.duration),
                        video_width: Some(constants::RECORDING_WIDTH),
                        video_height: Some(constants::RECORDING_HEIGHT),
                        video_fps: Some(constants::FPS as f32),
                        video_codec: None,
                        chunk_size_bytes: if unreliable_connection {
                            Some(5 * 1024 * 1024)
                        } else {
                            None
                        },
                        additional_metadata: serde_json::to_value(&validation.metadata)?,
                        uploading_owl_control_version: Some(env!("CARGO_PKG_VERSION")),
                    },
                )
                .await
                .map_err(UploadFolderError::InitMultipartUpload)?;

            let upload_progress = UploadProgressState::new(
                init_response.upload_id,
                init_response.game_control_id,
                tar_path.to_path_buf(),
                init_response.total_chunks,
                init_response.chunk_size_bytes,
                init_response.expires_at,
            );

            LocalRecordingPaused {
                info,
                metadata,
                upload_progress,
            }
        }
        _ => unreachable!("only Paused and Unuploaded recordings should reach upload_folder"),
    };

    Ok(upload_tar(
        paused,
        api_client.clone(),
        api_token,
        unreliable_tx,
        cancel_flag,
        file_progress,
    )
    .await?)
}

/// Validates a potentially paused recording to determine if its upload can be resumed.
/// For Paused recordings: checks if valid (tar exists, >15min remaining). If invalid, cleans up and returns as Unuploaded.
/// For Unuploaded recordings: returns as-is.
async fn validate_potentially_paused_recording(
    recording: LocalRecording,
    api_client: &ApiClient,
    api_token: &str,
) -> LocalRecording {
    let LocalRecording::Paused(paused) = recording else {
        return recording;
    };

    let path = &paused.info.folder_path;
    let state = &paused.upload_progress;

    // Per Philpax: We should avoid resuming uploads if there's less than 15 minutes remaining on the timer;
    // we've seen upload speeds of 0.3MB/s, which would take 11 minutes to upload 200MB. 15 minutes is safer.
    const MIN_TIME_REMAINING_SECONDS: i64 = 15 * 60; // 15 minutes
    let seconds_left = state.seconds_until_expiration();
    if !(state.seconds_until_expiration() > MIN_TIME_REMAINING_SECONDS && state.tar_path.is_file())
    {
        // if tar file does not exist, we want to restart upload as there is no guarantee the
        // recreated tar file will be the same
        if !state.tar_path.is_file() {
            tracing::warn!(
                "Tar file for {} does not exist, starting fresh",
                path.display()
            );
        }
        // Also indicate if expired in logs, since expiry and tar files missing both above can happen independently
        if state.is_expired() {
            tracing::warn!(
                "Upload progress for {} has expired, starting fresh",
                path.display()
            );
        } else {
            tracing::warn!(
                "Upload progress for {} has insufficient time remaining ({}s < 15min), starting fresh",
                path.display(),
                seconds_left
            );
        }

        // abort the existing multipart request in the error case here, so that the server isn't left hanging
        // maybe this will break if the state.is_expired() is true? not quite sure about what the server will respond with.
        api_client
            .abort_multipart_upload(api_token, &state.upload_id)
            .await
            .ok();

        // Clean up expired progress and tar file
        paused.cleanup_upload_artifacts();

        return LocalRecording::Unuploaded {
            info: paused.info,
            metadata: paused.metadata,
        };
    }

    tracing::info!(
        "Resuming upload for {} from chunk {}/{} (expires in {}s)",
        path.display(),
        state.next_chunk_number(),
        state.total_chunks,
        seconds_left
    );
    LocalRecording::Paused(paused)
}

#[derive(Debug)]
enum CreateTarError {
    Join(JoinError),
    InvalidFilename(PathBuf),
    Io(std::io::Error),
}
impl std::fmt::Display for CreateTarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateTarError::Join(e) => write!(f, "Join error: {e}"),
            CreateTarError::InvalidFilename(path) => write!(f, "Invalid filename: {path:?}"),
            CreateTarError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}
impl std::error::Error for CreateTarError {}
impl From<JoinError> for CreateTarError {
    fn from(e: JoinError) -> Self {
        CreateTarError::Join(e)
    }
}
impl From<std::io::Error> for CreateTarError {
    fn from(e: std::io::Error) -> Self {
        CreateTarError::Io(e)
    }
}
async fn create_tar_file(
    recording_path: &Path,
    validation: &ValidationResult,
) -> Result<PathBuf, CreateTarError> {
    tokio::task::spawn_blocking({
        let recording_path = recording_path.to_path_buf();
        let validation = validation.clone();
        move || {
            // Create tar file inside the recording folder
            let tar_path = recording_path.join(format!(
                "{}.tar",
                &uuid::Uuid::new_v4().simple().to_string()[0..16]
            ));
            let mut tar = tar::Builder::new(std::fs::File::create(&tar_path)?);
            for path in [
                &validation.mp4_path,
                &validation.csv_path,
                &validation.meta_path,
            ] {
                tar.append_file(
                    path.file_name()
                        .ok_or_else(|| CreateTarError::InvalidFilename(path.to_owned()))?,
                    &mut std::fs::File::open(path)?,
                )?;
            }

            Ok(tar_path)
        }
    })
    .await?
}

#[derive(Debug)]
enum UploadTarError {
    Io(std::io::Error),
    FailedToGetTarFilename(PathBuf),
    FailedToGetHardwareId(String),
    Serde(serde_json::Error),
    Api {
        api_request: &'static str,
        error: ApiError,
    },
    FailedToUploadChunk {
        chunk_number: u64,
        total_chunks: u64,
        max_retries: u32,
        error: UploadSingleChunkError,
    },
    FailedToCompleteMultipartUpload(String),
}
impl std::fmt::Display for UploadTarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UploadTarError::Io(e) => write!(f, "I/O error: {e}"),
            UploadTarError::FailedToGetTarFilename(path) => {
                write!(f, "Failed to get tar filename: {path:?}")
            }
            UploadTarError::FailedToGetHardwareId(id) => {
                write!(f, "Failed to get hardware ID: {id}")
            }
            UploadTarError::Serde(e) => {
                write!(f, "Serde error: {e}")
            }
            UploadTarError::Api { api_request, error } => {
                write!(f, "API error for {api_request}: {error}")
            }
            UploadTarError::FailedToUploadChunk {
                chunk_number,
                total_chunks,
                max_retries,
                error,
            } => {
                write!(
                    f,
                    "Failed to upload chunk {chunk_number}/{total_chunks} after {max_retries} attempts: {error:?}"
                )
            }
            UploadTarError::FailedToCompleteMultipartUpload(message) => {
                write!(f, "Failed to complete multipart upload: {message}")
            }
        }
    }
}
impl std::error::Error for UploadTarError {}
impl From<std::io::Error> for UploadTarError {
    fn from(e: std::io::Error) -> Self {
        UploadTarError::Io(e)
    }
}
impl From<serde_json::Error> for UploadTarError {
    fn from(e: serde_json::Error) -> Self {
        UploadTarError::Serde(e)
    }
}

/// Result type for `upload_tar` that distinguishes between different outcomes.
enum UploadTarOutput {
    /// Upload completed successfully, recording is now Uploaded variant
    Success(LocalRecording),
    /// Server rejected the upload, recording is now Invalid variant
    ServerInvalid(LocalRecording),
    /// Upload was paused by user
    Paused(LocalRecording),
}

async fn upload_tar(
    paused: LocalRecordingPaused,
    api_client: Arc<ApiClient>,
    api_token: &str,
    unreliable_tx: app_state::UiUpdateUnreliableSender,
    pause_flag: Arc<std::sync::atomic::AtomicBool>,
    file_progress: FileProgress,
) -> Result<UploadTarOutput, UploadTarError> {
    let tar_path = paused.upload_progress.tar_path.clone();
    let file_size = std::fs::metadata(&tar_path).map(|m| m.len())?;
    unreliable_tx
        .send(UiUpdateUnreliable::UpdateUploadProgress(Some(
            ProgressData::default(),
        )))
        .ok();

    let mut chunk_etags = paused.upload_progress.chunk_etags.clone();
    let start_chunk = paused.upload_progress.next_chunk_number();

    tracing::info!(
        "Starting upload of {} bytes in {} chunks of {} bytes each; upload_id={}, game_control_id={}",
        file_size,
        paused.upload_progress.total_chunks,
        paused.upload_progress.chunk_size_bytes,
        paused.upload_progress.upload_id,
        paused.upload_progress.game_control_id
    );

    // Auto-abort guard: on unexpected drop, abort the upload and save progress for resume.
    struct AbortUploadOnDrop {
        api_client: Arc<ApiClient>,
        api_token: String,
        paused: Option<LocalRecordingPaused>,
    }
    impl AbortUploadOnDrop {
        fn new(
            api_client: Arc<ApiClient>,
            api_token: String,
            paused: LocalRecordingPaused,
        ) -> Self {
            Self {
                api_client,
                api_token,
                paused: Some(paused),
            }
        }

        /// Access the paused recording (for save_upload_progress calls during upload)
        fn paused(&self) -> &LocalRecordingPaused {
            self.paused
                .as_ref()
                .expect("paused recording taken prematurely")
        }

        /// Mutably access the paused recording (for updating progress state)
        fn paused_mut(&mut self) -> &mut LocalRecordingPaused {
            self.paused
                .as_mut()
                .expect("paused recording taken prematurely")
        }

        /// Take ownership of the paused recording, disarming the drop handler.
        /// Call this on successful completion or controlled exit.
        fn take_paused(&mut self) -> LocalRecordingPaused {
            self.paused.take().expect("paused recording already taken")
        }
    }
    impl Drop for AbortUploadOnDrop {
        fn drop(&mut self) {
            // Only runs if paused recording wasn't taken (unexpected drop)
            if let Some(ref paused) = self.paused {
                tracing::info!(
                    "Aborting upload of {} (guard drop / unexpected failure)",
                    paused.upload_progress.upload_id
                );

                paused.cleanup_upload_artifacts();

                // Abort server upload
                let api_client = self.api_client.clone();
                let api_token = self.api_token.clone();
                let upload_id = paused.upload_progress.upload_id.clone();
                tokio::spawn(async move {
                    api_client
                        .abort_multipart_upload(&api_token, &upload_id)
                        .await
                        .ok();
                });
            }
        }
    }

    let chunk_size_bytes = paused.upload_progress.chunk_size_bytes;
    let total_chunks = paused.upload_progress.total_chunks;
    let upload_id = paused.upload_progress.upload_id.clone();
    let mut guard = AbortUploadOnDrop::new(api_client.clone(), api_token.to_string(), paused);

    {
        let mut file = tokio::fs::File::open(tar_path).await?;

        // If resuming, seek to the correct position in the file
        if start_chunk > 1 {
            let bytes_to_skip = (start_chunk - 1) * chunk_size_bytes;
            file.seek(std::io::SeekFrom::Start(bytes_to_skip))
                .await
                .map_err(UploadTarError::Io)?;
            tracing::info!(
                "Seeking to byte {} to resume from chunk {}",
                bytes_to_skip,
                start_chunk
            );
        }

        // TODO: make this less sloppy.
        // Instead of allocating a chunk-sized buffer, and then allocating that buffer
        // again for each chunk's stream, figure out a way to stream each chunk from the file
        // directly into the hasher, and then stream each chunk directly into the uploader
        let mut buffer = vec![0u8; chunk_size_bytes as usize];
        let client = reqwest::Client::new();

        // Initialize progress sender with bytes already uploaded
        let bytes_already_uploaded = (start_chunk - 1) * chunk_size_bytes;
        let progress_sender = Arc::new(Mutex::new({
            let mut sender = ProgressSender::new(unreliable_tx.clone(), file_size, file_progress);
            sender.set_bytes_uploaded(bytes_already_uploaded);
            sender
        }));

        for chunk_number in start_chunk..=total_chunks {
            // Check if upload has been cancelled (user-initiated pause)
            if pause_flag.load(std::sync::atomic::Ordering::SeqCst) {
                // Ensure the latest progress is saved for resume
                if let Err(e) = guard.paused().save_upload_progress() {
                    tracing::error!("Failed to save upload progress on pause: {:?}", e);
                }
                // Disarm by taking the paused recording - keeps server/session state for resume
                let paused = guard.take_paused();
                return Ok(UploadTarOutput::Paused(LocalRecording::Paused(paused)));
            }

            // Check if upload session is about to expire
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if now >= guard.paused().upload_progress.expires_at {
                return Err(UploadTarError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Upload session has expired",
                )));
            }
            let seconds_left = guard.paused().upload_progress.expires_at as i64 - now as i64;
            if seconds_left < 60 && chunk_number % 10 == 0 {
                tracing::warn!("Upload session expires in {} seconds!", seconds_left);
            }

            tracing::info!(
                "Uploading chunk {chunk_number}/{total_chunks} for upload_id {upload_id}"
            );

            // Read chunk data from file (only once per chunk, not per retry)
            let mut buffer_start = 0;
            loop {
                let bytes_read = file.read(&mut buffer[buffer_start..]).await?;
                if bytes_read == 0 {
                    break;
                }
                buffer_start += bytes_read;
            }
            let chunk_size = buffer_start;
            let chunk_data = buffer[..chunk_size].to_vec();
            let chunk_hash = sha256::digest(&chunk_data);

            const MAX_RETRIES: u32 = 5;

            for attempt in 1..=MAX_RETRIES {
                // Store bytes_uploaded before attempting the chunk
                let bytes_before_chunk = progress_sender.lock().unwrap().bytes_uploaded;

                let chunk = Chunk {
                    data: &chunk_data,
                    hash: &chunk_hash,
                    number: chunk_number,
                };

                match upload_single_chunk(
                    chunk,
                    &api_client,
                    api_token,
                    &upload_id,
                    progress_sender.clone(),
                    &client,
                )
                .await
                {
                    Ok(etag) => {
                        progress_sender.lock().unwrap().send();

                        chunk_etags.push(CompleteMultipartUploadChunk { chunk_number, etag });

                        // Update progress state with new chunk and save to file
                        guard.paused_mut().upload_progress.chunk_etags = chunk_etags.clone();
                        if let Err(e) = guard.paused().save_upload_progress() {
                            tracing::error!("Failed to save upload progress: {:?}", e);
                        }

                        tracing::info!(
                            "Uploaded chunk {chunk_number}/{total_chunks} for upload_id {upload_id}"
                        );
                        break; // Success, move to next chunk
                    }
                    Err(error) => {
                        // Reset bytes_uploaded to what it was before the chunk attempt
                        {
                            let mut progress_sender = progress_sender.lock().unwrap();
                            progress_sender.set_bytes_uploaded(bytes_before_chunk);
                        }

                        tracing::warn!(
                            "Failed to upload chunk {chunk_number}/{total_chunks} (attempt {attempt}/{MAX_RETRIES}): {error:?}"
                        );

                        if attempt == MAX_RETRIES {
                            return Err(UploadTarError::FailedToUploadChunk {
                                chunk_number,
                                total_chunks,
                                max_retries: MAX_RETRIES,
                                error,
                            });
                        }

                        // Optional: add a small delay before retrying
                        tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64))
                            .await;
                    }
                }
            }
        }
    }
    let completion_result = match api_client
        .complete_multipart_upload(api_token, &upload_id, &chunk_etags)
        .await
    {
        Ok(result) => result,
        Err(ApiError::ServerInvalidation(message)) => {
            // Server rejected the upload - take paused recording and mark as server invalid
            let paused = guard.take_paused();
            return match paused.mark_as_server_invalid(&message) {
                Ok(invalid_recording) => Ok(UploadTarOutput::ServerInvalid(invalid_recording)),
                Err(e) => Err(UploadTarError::Io(e)),
            };
        }
        Err(e) => {
            return Err(UploadTarError::Api {
                api_request: "complete_multipart_upload",
                error: e,
            });
        }
    };

    // Take ownership of the paused recording, disarming the drop guard
    let paused = guard.take_paused();

    if !completion_result.success {
        return Err(UploadTarError::FailedToCompleteMultipartUpload(
            completion_result.message,
        ));
    }

    tracing::info!(
        "Upload completed successfully! Game Control ID: {}, Object Key: {}, Verified: {}",
        completion_result.game_control_id,
        completion_result.object_key,
        completion_result.verified.unwrap_or_default()
    );

    // Mark the recording as uploaded using the encapsulated method
    match paused.mark_as_uploaded(completion_result.game_control_id) {
        Ok(uploaded_recording) => Ok(UploadTarOutput::Success(uploaded_recording)),
        Err(e) => Err(UploadTarError::Io(e)),
    }
}

struct Chunk<'a> {
    data: &'a [u8],
    hash: &'a str,
    number: u64,
}

#[derive(Debug)]
enum UploadSingleChunkError {
    Io(std::io::Error),
    Api {
        api_request: &'static str,
        error: ApiError,
    },
    Reqwest(reqwest::Error),
    ChunkUploadFailed(reqwest::StatusCode),
    NoEtagHeaderFound,
}
impl std::fmt::Display for UploadSingleChunkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UploadSingleChunkError::Io(e) => write!(f, "I/O error: {e}"),
            UploadSingleChunkError::Api { api_request, error } => {
                write!(f, "API error for {api_request}: {error:?}")
            }
            UploadSingleChunkError::Reqwest(e) => write!(f, "Reqwest error: {e}"),
            UploadSingleChunkError::ChunkUploadFailed(status) => {
                write!(f, "Chunk upload failed with status: {status}")
            }
            UploadSingleChunkError::NoEtagHeaderFound => {
                write!(f, "No ETag header found after chunk upload")
            }
        }
    }
}
impl std::error::Error for UploadSingleChunkError {}
impl From<std::io::Error> for UploadSingleChunkError {
    fn from(e: std::io::Error) -> Self {
        UploadSingleChunkError::Io(e)
    }
}
impl From<reqwest::Error> for UploadSingleChunkError {
    fn from(e: reqwest::Error) -> Self {
        UploadSingleChunkError::Reqwest(e)
    }
}

async fn upload_single_chunk(
    chunk: Chunk<'_>,
    api_client: &Arc<ApiClient>,
    api_token: &str,
    upload_id: &str,
    progress_sender: Arc<Mutex<ProgressSender>>,
    client: &reqwest::Client,
) -> Result<String, UploadSingleChunkError> {
    let multipart_chunk_response = api_client
        .upload_multipart_chunk(api_token, upload_id, chunk.number, chunk.hash)
        .await
        .map_err(|e| UploadSingleChunkError::Api {
            api_request: "upload_multipart_chunk",
            error: e,
        })?;

    // Create a stream that wraps chunk_data and tracks upload progress
    let progress_stream =
        tokio_util::io::ReaderStream::new(std::io::Cursor::new(chunk.data.to_vec())).inspect_ok({
            let progress_sender = progress_sender.clone();
            move |bytes| {
                progress_sender
                    .lock()
                    .unwrap()
                    .increment_bytes_uploaded(bytes.len() as u64);
            }
        });

    let res = client
        .put(&multipart_chunk_response.upload_url)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Length", chunk.data.len())
        .body(reqwest::Body::wrap_stream(progress_stream))
        .send()
        .await?;

    if !res.status().is_success() {
        return Err(UploadSingleChunkError::ChunkUploadFailed(res.status()));
    }

    // Extract etag header from response
    let etag = res
        .headers()
        .get("etag")
        .and_then(|hv| hv.to_str().ok())
        .map(|s| s.trim_matches('"').to_owned())
        .ok_or(UploadSingleChunkError::NoEtagHeaderFound)?;

    Ok(etag)
}

struct ProgressSender {
    tx: app_state::UiUpdateUnreliableSender,
    bytes_uploaded: u64,
    last_update_time: std::time::Instant,
    file_size: u64,
    start_time: std::time::Instant,
    file_progress: FileProgress,
}
impl ProgressSender {
    pub fn new(
        tx: app_state::UiUpdateUnreliableSender,
        file_size: u64,
        file_progress: FileProgress,
    ) -> Self {
        Self {
            tx,
            bytes_uploaded: 0,
            last_update_time: std::time::Instant::now(),
            file_size,
            start_time: std::time::Instant::now(),
            file_progress,
        }
    }

    pub fn increment_bytes_uploaded(&mut self, bytes: u64) {
        self.set_bytes_uploaded(self.bytes_uploaded + bytes);
    }

    pub fn set_bytes_uploaded(&mut self, bytes: u64) {
        self.bytes_uploaded = bytes;
        self.send();
    }

    fn send(&mut self) {
        if self.last_update_time.elapsed().as_millis() > 100 {
            self.send_impl();
            self.last_update_time = std::time::Instant::now();
        }
    }

    fn send_impl(&self) {
        let bps = self.bytes_uploaded as f64 / self.start_time.elapsed().as_secs_f64();
        let data = ProgressData {
            bytes_uploaded: self.bytes_uploaded,
            total_bytes: self.file_size,
            speed_mbps: bps / (1024.0 * 1024.0),
            eta_seconds: if bps > 0.0 {
                (self.file_size - self.bytes_uploaded) as f64 / bps
            } else {
                0.0
            },
            percent: if self.file_size > 0 {
                ((self.bytes_uploaded as f64 / self.file_size as f64) * 100.0).min(100.0)
            } else {
                0.0
            },
            file_progress: self.file_progress.clone(),
        };
        self.tx
            .send(UiUpdateUnreliable::UpdateUploadProgress(Some(data)))
            .ok();
    }
}
