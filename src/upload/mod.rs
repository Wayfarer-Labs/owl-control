use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use color_eyre::eyre::{self, Context as _, ContextCompat};
use futures::TryStreamExt as _;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncReadExt, sync::mpsc};

use crate::{
    api::{ApiClient, CompleteMultipartUploadChunk, InitMultipartUploadArgs},
    app_state::{self, AppState, AsyncRequest, UiUpdate, UiUpdateUnreliable},
    record::LocalRecording,
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

#[derive(Debug, Deserialize, Clone, Default)]
pub struct FinalStats {
    pub total_files_uploaded: u64,
    pub total_duration_uploaded: f64,
    pub total_bytes_uploaded: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UploadProgressState {
    pub upload_id: String,
    pub game_control_id: String,
    pub tar_path: PathBuf,
    pub chunk_etags: Vec<CompleteMultipartUploadChunk>,
    pub total_chunks: u64,
    pub chunk_size_bytes: u64,
    /// Unix timestamp when the upload session expires
    pub expires_at: u64,
}

impl UploadProgressState {
    /// Check if the upload session has expired
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now >= self.expires_at
    }

    /// Get the number of seconds until expiration
    pub fn seconds_until_expiration(&self) -> i64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.expires_at as i64 - now as i64
    }

    /// Load progress state from a file
    pub fn load_from_file(path: &Path) -> eyre::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let state: Self = serde_json::from_str(&content)?;
        Ok(state)
    }

    /// Save progress state to a file
    pub fn save_to_file(&self, path: &Path) -> eyre::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get the next chunk number to upload (after the last completed chunk)
    pub fn next_chunk_number(&self) -> u64 {
        self.chunk_etags
            .iter()
            .map(|c| c.chunk_number)
            .max()
            .map(|n| n + 1)
            .unwrap_or(1)
    }
}

pub async fn start(
    app_state: Arc<AppState>,
    api_client: Arc<ApiClient>,
    recording_location: PathBuf,
) {
    let tx = app_state.ui_update_tx.clone();
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
        unreliable_tx.clone(),
        app_state.async_request_tx.clone(),
        cancel_flag,
    )
    .await
    {
        tx.send(UiUpdate::UploadFailed(e.to_string())).ok();
    }

    app_state
        .async_request_tx
        .send(AsyncRequest::LoadUploadStats)
        .await
        .ok();
    app_state
        .async_request_tx
        .send(AsyncRequest::LoadLocalRecordings)
        .await
        .ok();

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
    unreliable_tx: app_state::UiUpdateUnreliableSender,
    async_req_tx: mpsc::Sender<AsyncRequest>,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
) -> eyre::Result<FinalStats> {
    let mut stats = FinalStats::default();

    // Scan all local recordings and filter to only unuploaded ones
    let recordings_to_upload: Vec<_> = LocalRecording::scan_directory(recording_location)
        .into_iter()
        .filter_map(|rec| match rec {
            LocalRecording::Unuploaded { info, .. } => Some(info),
            _ => None,
        })
        .collect();

    let total_files_to_upload = recordings_to_upload.len() as u64;

    let mut last_upload_time = std::time::Instant::now();
    let reload_every_n_files = 5;
    let reload_if_at_least_has_passed = std::time::Duration::from_secs(2 * 60);
    for info in recordings_to_upload {
        // Check if upload has been cancelled
        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            eyre::bail!("Upload cancelled by user");
        }

        let path = &info.folder_path;

        let file_progress = FileProgress {
            current_file: info.folder_name.clone(),
            files_remaining: total_files_to_upload.saturating_sub(stats.total_files_uploaded),
        };

        let recording_stats = match upload_folder(
            path,
            api_client.clone(),
            &api_token,
            unreliable_connection,
            unreliable_tx.clone(),
            cancel_flag.clone(),
            file_progress,
        )
        .await
        {
            Ok(recording_stats) => recording_stats,
            Err(e) => {
                tracing::error!("Error uploading folder {}: {:?}", path.display(), e);
                continue;
            }
        };

        stats.total_duration_uploaded += recording_stats.duration;
        stats.total_files_uploaded += 1;
        stats.total_bytes_uploaded += recording_stats.bytes;

        // delete the uploaded recording directory if the preference is enabled
        if delete_uploaded {
            if let Err(e) = tokio::fs::remove_dir_all(path).await {
                tracing::error!(
                    "Failed to delete uploaded directory {}: {:?}",
                    path.display(),
                    e
                );
            } else {
                tracing::info!("Deleted uploaded directory: {}", path.display());
            }
        }

        let should_reload = if stats.total_files_uploaded % reload_every_n_files == 0 {
            tracing::info!(
                "{} files uploaded, reloading upload stats and local recordings",
                stats.total_files_uploaded
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
            async_req_tx.send(AsyncRequest::LoadUploadStats).await.ok();
            async_req_tx
                .send(AsyncRequest::LoadLocalRecordings)
                .await
                .ok();
        }
        last_upload_time = std::time::Instant::now();
    }

    Ok(stats)
}

struct RecordingStats {
    duration: f64,
    bytes: u64,
}

async fn create_tar_file(validation: &ValidationResult) -> eyre::Result<PathBuf> {
    tokio::task::spawn_blocking({
        let validation = validation.clone();
        move || {
            let tar_path = PathBuf::from(format!(
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
                    path.file_name().context("failed to get file name")?,
                    &mut std::fs::File::open(path)?,
                )?;
            }

            eyre::Ok(tar_path)
        }
    })
    .await
    .map_err(eyre::Error::from)
    .flatten()
    .context("error creating tar file")
}

async fn upload_folder(
    path: &Path,
    api_client: Arc<ApiClient>,
    api_token: &str,
    unreliable_connection: bool,
    unreliable_tx: app_state::UiUpdateUnreliableSender,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    file_progress: FileProgress,
) -> eyre::Result<RecordingStats> {
    // Check for existing upload progress
    let progress_file_path = path.join(constants::filename::recording::UPLOAD_PROGRESS);
    let resume_state = if progress_file_path.is_file() {
        match UploadProgressState::load_from_file(&progress_file_path) {
            Ok(state) => {
                if state.is_expired() {
                    tracing::warn!(
                        "Upload progress for {} has expired, starting fresh",
                        path.display()
                    );
                    // Clean up expired progress and tar file
                    std::fs::remove_file(&progress_file_path).ok();
                    std::fs::remove_file(&state.tar_path).ok();
                    None
                } else {
                    let seconds_left = state.seconds_until_expiration();
                    tracing::info!(
                        "Resuming upload for {} from chunk {}/{} (expires in {}s)",
                        path.display(),
                        state.next_chunk_number(),
                        state.total_chunks,
                        seconds_left
                    );
                    if seconds_left < 300 {
                        tracing::warn!("Upload session expires in less than 5 minutes!");
                    }
                    Some(state)
                }
            }
            Err(e) => {
                tracing::error!("Failed to load upload progress: {:?}", e);
                std::fs::remove_file(&progress_file_path).ok();
                None
            }
        }
    } else {
        None
    };

    tracing::info!("Validating folder {}", path.display());
    let validation = tokio::task::spawn_blocking({
        let path = path.to_owned();
        move || validate_folder(&path)
    })
    .await??;

    // Use existing tar if resuming, otherwise create new one
    let tar_path = if let Some(ref state) = resume_state {
        if state.tar_path.is_file() {
            tracing::info!("Using existing tar file: {}", state.tar_path.display());
            state.tar_path.clone()
        } else {
            tracing::warn!("Tar file missing for resume, creating new one");
            create_tar_file(&validation).await?
        }
    } else {
        tracing::info!("Creating tar file for {}", path.display());
        create_tar_file(&validation).await?
    };

    let game_control_id = upload_tar(
        path,
        &tar_path,
        api_client,
        api_token,
        unreliable_connection,
        validation
            .mp4_path
            .file_name()
            .context("failed to get mp4 filename")?
            .to_string_lossy()
            .as_ref(),
        validation
            .csv_path
            .file_name()
            .context("failed to get csv filename")?
            .to_string_lossy()
            .as_ref(),
        validation.metadata.duration,
        unreliable_tx,
        cancel_flag,
        file_progress,
        resume_state,
    )
    .await
    .context("error uploading tar file")?;

    // Clean up progress file and tar after successful upload
    std::fs::remove_file(&progress_file_path).ok();
    std::fs::remove_file(&tar_path).ok();

    std::fs::write(
        path.join(constants::filename::recording::UPLOADED),
        game_control_id,
    )
    .ok();

    Ok(RecordingStats {
        duration: validation.metadata.duration as f64,
        bytes: std::fs::metadata(&tar_path)
            .map(|m| m.len())
            .unwrap_or_default(),
    })
}

#[allow(clippy::too_many_arguments)]
async fn upload_tar(
    recording_path: &Path,
    tar_path: &Path,
    api_client: Arc<ApiClient>,
    api_token: &str,
    unreliable_connection: bool,
    video_filename: &str,
    control_filename: &str,
    video_duration_seconds: f64,
    unreliable_tx: app_state::UiUpdateUnreliableSender,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    file_progress: FileProgress,
    resume_state: Option<UploadProgressState>,
) -> eyre::Result<String> {
    let file_size = std::fs::metadata(tar_path)
        .map(|m| m.len())
        .context("failed to get file size")?;

    unreliable_tx
        .send(UiUpdateUnreliable::UpdateUploadProgress(Some(
            ProgressData::default(),
        )))
        .ok();

    // Use existing upload session if resuming, otherwise initialize new one
    let (upload_session, mut chunk_etags, start_chunk) =
        if let Some(state) = resume_state.as_ref().filter(|s| s.tar_path == tar_path) {
            // Resume from saved state
            tracing::info!("Resuming upload from saved state");
            let session = crate::api::InitMultipartUploadResponse {
                upload_id: state.upload_id.clone(),
                game_control_id: state.game_control_id.clone(),
                total_chunks: state.total_chunks,
                chunk_size_bytes: state.chunk_size_bytes,
                expires_at: state.expires_at,
            };
            let start_chunk = state.next_chunk_number();
            (session, state.chunk_etags.clone(), start_chunk)
        } else {
            // Initialize new upload session (either no resume state or tar path mismatch)
            if resume_state.is_some() {
                tracing::warn!("Resume state tar path mismatch, starting fresh upload");
            }
            let session = api_client
                .init_multipart_upload(
                    api_token,
                    tar_path,
                    file_size,
                    InitMultipartUploadArgs {
                        tags: None,
                        video_filename: Some(video_filename),
                        control_filename: Some(control_filename),
                        video_duration_seconds: Some(video_duration_seconds),
                        video_width: Some(constants::RECORDING_WIDTH),
                        video_height: Some(constants::RECORDING_HEIGHT),
                        video_fps: Some(constants::FPS as f32),
                        video_codec: None,
                        chunk_size_bytes: if unreliable_connection {
                            Some(5 * 1024 * 1024)
                        } else {
                            None
                        },
                    },
                )
                .await
                .context("failed to initialize multipart upload")?;
            (session, vec![], 1)
        };

    tracing::info!(
        "Starting upload of {} bytes in {} chunks of {} bytes each; upload_id={}, game_control_id={}",
        file_size,
        upload_session.total_chunks,
        upload_session.chunk_size_bytes,
        upload_session.upload_id,
        upload_session.game_control_id
    );

    // Auto-abort guard: on unexpected drop, abort the upload and clean up.
    struct AbortUploadOnDrop {
        api_client: Arc<ApiClient>,
        api_token: String,
        progress_state: Option<UploadProgressState>,
        progress_file_path: PathBuf,
    }
    impl AbortUploadOnDrop {
        pub fn disarm(&mut self) {
            self.progress_state = None;
        }
    }
    impl Drop for AbortUploadOnDrop {
        fn drop(&mut self) {
            if let Some(state) = self.progress_state.take() {
                tracing::info!(
                    "Aborting upload of {} (guard drop / unexpected failure)",
                    state.upload_id
                );
                let api_client = self.api_client.clone();
                let api_token = self.api_token.clone();
                let upload_id = state.upload_id.clone();
                let progress_file_path = self.progress_file_path.clone();
                let tar_path = state.tar_path.clone();

                tokio::spawn(async move {
                    api_client
                        .abort_multipart_upload(&api_token, &upload_id)
                        .await
                        .ok();

                    // Clean up progress file and tar file
                    std::fs::remove_file(&progress_file_path).ok();
                    std::fs::remove_file(&tar_path).ok();
                });
            }
        }
    }

    let progress_file_path = recording_path.join(constants::filename::recording::UPLOAD_PROGRESS);
    let mut abort_upload_on_drop = AbortUploadOnDrop {
        api_client: api_client.clone(),
        api_token: api_token.to_string(),
        progress_state: Some(UploadProgressState {
            upload_id: upload_session.upload_id.clone(),
            game_control_id: upload_session.game_control_id.clone(),
            tar_path: tar_path.to_path_buf(),
            chunk_etags: chunk_etags.clone(),
            total_chunks: upload_session.total_chunks,
            chunk_size_bytes: upload_session.chunk_size_bytes,
            expires_at: upload_session.expires_at,
        }),
        progress_file_path: progress_file_path.clone(),
    };

    {
        let mut file = tokio::fs::File::open(tar_path)
            .await
            .context("failed to open tar file")?;

        // If resuming, seek to the correct position in the file
        if start_chunk > 1 {
            let bytes_to_skip = (start_chunk - 1) * upload_session.chunk_size_bytes;
            use tokio::io::AsyncSeekExt;
            file.seek(std::io::SeekFrom::Start(bytes_to_skip))
                .await
                .context("failed to seek to resume position")?;
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
        let mut buffer = vec![0u8; upload_session.chunk_size_bytes as usize];
        let client = reqwest::Client::new();

        // Initialize progress sender with bytes already uploaded
        let bytes_already_uploaded = (start_chunk - 1) * upload_session.chunk_size_bytes;
        let progress_sender = Arc::new(Mutex::new({
            let mut sender = ProgressSender::new(unreliable_tx.clone(), file_size, file_progress);
            sender.set_bytes_uploaded(bytes_already_uploaded);
            sender
        }));

        for chunk_number in start_chunk..=upload_session.total_chunks {
            // Check if upload has been cancelled (user-initiated pause)
            if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                // Ensure the latest progress is saved for resume
                if let Some(state) = abort_upload_on_drop.progress_state.as_ref() {
                    if let Err(e) = state.save_to_file(&progress_file_path) {
                        tracing::error!("Failed to save upload progress on pause: {:?}", e);
                    }
                }
                // Disarm auto-abort to keep server/session state for resume
                abort_upload_on_drop.disarm();
                eyre::bail!("Upload cancelled by user");
            }

            // Check if upload session is about to expire
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if now >= upload_session.expires_at {
                eyre::bail!("Upload session has expired");
            }
            let seconds_left = upload_session.expires_at as i64 - now as i64;
            if seconds_left < 60 && chunk_number % 10 == 0 {
                tracing::warn!("Upload session expires in {} seconds!", seconds_left);
            }

            tracing::info!(
                "Uploading chunk {}/{} for upload_id {}",
                chunk_number,
                upload_session.total_chunks,
                upload_session.upload_id
            );

            // Read chunk data from file (only once per chunk, not per retry)
            let mut buffer_start = 0;
            loop {
                let bytes_read = file
                    .read(&mut buffer[buffer_start..])
                    .await
                    .context("failed to read chunk")?;
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
                    &upload_session.upload_id,
                    progress_sender.clone(),
                    &client,
                )
                .await
                {
                    Ok(etag) => {
                        progress_sender.lock().unwrap().send();

                        chunk_etags.push(CompleteMultipartUploadChunk { chunk_number, etag });

                        // Update progress state with new chunk
                        if let Some(ref mut state) = abort_upload_on_drop.progress_state {
                            state.chunk_etags = chunk_etags.clone();
                            // Save progress to file
                            if let Err(e) = state.save_to_file(&progress_file_path) {
                                tracing::error!("Failed to save upload progress: {:?}", e);
                            }
                        }

                        tracing::info!(
                            "Uploaded chunk {}/{} for upload_id {}",
                            chunk_number,
                            upload_session.total_chunks,
                            upload_session.upload_id
                        );
                        break; // Success, move to next chunk
                    }
                    Err(e) => {
                        // Reset bytes_uploaded to what it was before the chunk attempt
                        {
                            let mut progress_sender = progress_sender.lock().unwrap();
                            progress_sender.set_bytes_uploaded(bytes_before_chunk);
                        }

                        tracing::warn!(
                            "Failed to upload chunk {chunk_number}/{} (attempt {attempt}/{MAX_RETRIES}): {e:?}",
                            upload_session.total_chunks,
                        );

                        if attempt == MAX_RETRIES {
                            eyre::bail!(
                                "Failed to upload chunk {chunk_number}/{} after {MAX_RETRIES} attempts: {e}",
                                upload_session.total_chunks
                            );
                        }

                        // Optional: add a small delay before retrying
                        tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64))
                            .await;
                    }
                }
            }
        }
    }
    let completion_result = api_client
        .complete_multipart_upload(api_token, &upload_session.upload_id, &chunk_etags)
        .await
        .context("failed to complete multipart upload")?;

    abort_upload_on_drop.disarm();

    // Clean up progress file on successful completion
    std::fs::remove_file(&progress_file_path).ok();

    if !completion_result.success {
        eyre::bail!(
            "Failed to complete multipart upload: {}",
            completion_result.message
        );
    }

    tracing::info!(
        "Upload completed successfully! Game Control ID: {}, Object Key: {}, Verified: {}",
        completion_result.game_control_id,
        completion_result.object_key,
        completion_result.verified.unwrap_or_default()
    );

    Ok(completion_result.game_control_id)
}

struct Chunk<'a> {
    data: &'a [u8],
    hash: &'a str,
    number: u64,
}

async fn upload_single_chunk(
    chunk: Chunk<'_>,
    api_client: &Arc<ApiClient>,
    api_token: &str,
    upload_id: &str,
    progress_sender: Arc<Mutex<ProgressSender>>,
    client: &reqwest::Client,
) -> eyre::Result<String> {
    let multipart_chunk_response = api_client
        .upload_multipart_chunk(api_token, upload_id, chunk.number, chunk.hash)
        .await
        .context("failed to upload chunk")?;

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
        .await
        .context("failed to stream chunk to upload url")?;

    if !res.status().is_success() {
        eyre::bail!("Chunk upload failed with status: {}", res.status())
    }

    // Extract etag header from response
    let etag = res
        .headers()
        .get("etag")
        .and_then(|hv| hv.to_str().ok())
        .map(|s| s.trim_matches('"').to_owned())
        .ok_or_else(|| eyre::eyre!("No ETag header found after chunk upload"))?;

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
