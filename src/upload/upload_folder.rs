use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::task::JoinError;

use crate::{
    api::{ApiClient, ApiError, InitMultipartUploadArgs},
    app_state::UiUpdateUnreliableSender,
    record::{LocalRecording, LocalRecordingPaused, UploadProgressState},
    upload::{
        FileProgress,
        create_tar::{CreateTarError, create_tar_file},
        upload_tar::{UploadTarError, UploadTarOutput},
    },
    validation::validate_folder,
};

#[derive(Debug)]
pub enum UploadFolderError {
    Io(std::io::Error),
    InitMultipartUpload(ApiError),
    CreateTar(CreateTarError),
    FailedToGetFileSize(PathBuf, std::io::Error),
    MissingFilename(PathBuf),
    MissingHardwareId(color_eyre::eyre::Report),
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
            UploadFolderError::MissingHardwareId(e) => write!(f, "Missing hardware ID: {e}"),
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
pub async fn upload_folder(
    recording: LocalRecording,
    api_client: Arc<ApiClient>,
    api_token: &str,
    unreliable_connection: bool,
    unreliable_tx: UiUpdateUnreliableSender,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    file_progress: FileProgress,
) -> Result<UploadTarOutput, UploadFolderError> {
    // Validate paused recording (may convert to Unuploaded if expired/invalid)
    let info = recording.info().clone();
    let metadata = recording.metadata().cloned().map(Box::new);
    let paused = if let LocalRecording::Paused(paused) = recording
        && let Some(paused) = validate_recording_paused(paused, &api_client, api_token).await
    {
        paused
    } else {
        // Fresh: validate, create tar, init upload
        let path = info.folder_path.clone();
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

        let hardware_id =
            crate::system::hardware_id::get().map_err(UploadFolderError::MissingHardwareId)?;

        let init_response = api_client
            .init_multipart_upload(
                api_token,
                InitMultipartUploadArgs {
                    filename: tar_path
                        .file_name()
                        .ok_or_else(|| UploadFolderError::MissingFilename(tar_path.to_owned()))?
                        .to_string_lossy()
                        .as_ref(),
                    total_size_bytes: file_size,
                    hardware_id: &hardware_id,
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

        LocalRecordingPaused::new(info, metadata, upload_progress)
    };

    Ok(super::upload_tar::run(
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
async fn validate_recording_paused(
    paused: LocalRecordingPaused,
    api_client: &ApiClient,
    api_token: &str,
) -> Option<LocalRecordingPaused> {
    let path = &paused.info.folder_path;
    let state = paused.upload_progress();

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

        paused.abort_and_cleanup(api_client, api_token).await.ok();

        return None;
    }

    tracing::info!(
        "Resuming upload for {} from chunk {}/{} (expires in {}s)",
        path.display(),
        state.next_chunk_number(),
        state.total_chunks,
        seconds_left
    );
    Some(paused)
}
