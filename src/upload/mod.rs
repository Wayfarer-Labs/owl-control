use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use color_eyre::eyre::{self, Context as _, ContextCompat};
use futures::TryStreamExt as _;
use serde::Deserialize;
use tokio::{io::AsyncReadExt, sync::mpsc};

use crate::{
    api::{ApiClient, CompleteMultipartUploadChunk, InitMultipartUploadArgs},
    app_state::{self, AppState, AsyncRequest},
    output_types::Metadata,
};

pub mod validation;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ProgressData {
    pub bytes_uploaded: u64,
    pub total_bytes: u64,
    pub speed_mbps: f64,
    pub eta_seconds: f64,
    pub percent: f64,
}

#[derive(Debug, Clone)]
pub struct LocalRecordingInfo {
    pub folder_name: String,
    pub folder_path: PathBuf,
    pub folder_size: u64,
    pub timestamp: Option<std::time::SystemTime>,
}
#[derive(Debug, Clone)]
pub enum LocalRecording {
    Invalid {
        info: LocalRecordingInfo,
        error_reasons: Vec<String>,
    },
    Unuploaded {
        info: LocalRecordingInfo,
        metadata: Option<Box<Metadata>>,
    },
}
impl LocalRecording {
    pub fn info(&self) -> &LocalRecordingInfo {
        match self {
            LocalRecording::Invalid { info, .. } => info,
            LocalRecording::Unuploaded { info, .. } => info,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct FinalStats {
    pub total_files_uploaded: u64,
    pub total_duration_uploaded: f64,
    pub total_bytes_uploaded: u64,
}

pub async fn start(
    app_state: Arc<AppState>,
    api_client: Arc<ApiClient>,
    recording_location: PathBuf,
) {
    let tx = app_state.ui_update_tx.clone();
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

    tx.send(app_state::UiUpdate::UpdateUploadProgress(Some(
        ProgressData::default(),
    )))
    .await
    .ok();

    match run(
        &recording_location,
        api_client,
        api_token,
        unreliable_connection,
        delete_uploaded,
        tx.clone(),
        app_state.async_request_tx.clone(),
        cancel_flag,
    )
    .await
    {
        Ok(_final_stats) => {
            // Request a re-fetch of our upload stats and local recordings
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
        }
        Err(e) => {
            tx.send(app_state::UiUpdate::UploadFailed(e.to_string()))
                .await
                .ok();
        }
    }

    tx.send(app_state::UiUpdate::UpdateUploadProgress(None))
        .await
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
    tx: app_state::UiUpdateSender,
    async_req_tx: mpsc::Sender<AsyncRequest>,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
) -> eyre::Result<FinalStats> {
    let mut stats = FinalStats::default();

    for entry in recording_location.read_dir()? {
        // Check if upload has been cancelled
        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            eyre::bail!("Upload cancelled by user");
        }

        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if path.join(constants::filename::recording::INVALID).is_file()
            || path
                .join(constants::filename::recording::UPLOADED)
                .is_file()
        {
            continue;
        }

        let recording_stats = match upload_folder(
            &path,
            api_client.clone(),
            &api_token,
            unreliable_connection,
            tx.clone(),
            cancel_flag.clone(),
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
            if let Err(e) = std::fs::remove_dir_all(&path) {
                tracing::error!(
                    "Failed to delete uploaded directory {}: {:?}",
                    path.display(),
                    e
                );
            } else {
                tracing::info!("Deleted uploaded directory: {}", path.display());
            }
        }

        // every 5 files uploaded we check with server to update list of successfully uploaded files
        if stats.total_files_uploaded % 5 == 0 {
            let async_req_tx = async_req_tx.clone();
            tokio::spawn(async move {
                async_req_tx.send(AsyncRequest::LoadUploadStats).await.ok();
                async_req_tx
                    .send(AsyncRequest::LoadLocalRecordings)
                    .await
                    .ok();
            });
        }
    }

    Ok(stats)
}

struct RecordingStats {
    duration: f64,
    bytes: u64,
}
async fn upload_folder(
    path: &Path,
    api_client: Arc<ApiClient>,
    api_token: &str,
    unreliable_connection: bool,
    tx: app_state::UiUpdateSender,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
) -> eyre::Result<RecordingStats> {
    tracing::info!("Validating folder {}", path.display());
    let validation = match validate_folder(path) {
        Ok(validation_paths) => validation_paths,
        Err(e) => {
            std::fs::write(
                path.join(constants::filename::recording::INVALID),
                e.join("\n"),
            )
            .ok();
            eyre::bail!("Validation failures: {}", e.join("\n"));
        }
    };

    tracing::info!("Creating tar file for {}", path.display());
    let tar_path = tokio::task::spawn_blocking({
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
    .context("error creating tar file")?;

    struct DeleteFileOnDrop(PathBuf);
    impl Drop for DeleteFileOnDrop {
        fn drop(&mut self) {
            std::fs::remove_file(&self.0).ok();
        }
    }
    let tar_path = DeleteFileOnDrop(tar_path);

    let game_control_id = upload_tar(
        &tar_path.0,
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
        tx,
        cancel_flag,
    )
    .await
    .context("error uploading tar file")?;

    std::fs::write(
        path.join(constants::filename::recording::UPLOADED),
        game_control_id,
    )
    .ok();

    Ok(RecordingStats {
        duration: validation.metadata.duration as f64,
        bytes: std::fs::metadata(&tar_path.0)
            .map(|m| m.len())
            .unwrap_or_default(),
    })
}

// This is a bit messy - I don't love using a Vec of Strings for the errors -
// but I wanted to capture the multi-error nature of the validation process
//
// TODO: Think of a better way to handle this
#[derive(Clone)]
struct ValidationResult {
    mp4_path: PathBuf,
    csv_path: PathBuf,
    meta_path: PathBuf,
    metadata: Metadata,
}
fn validate_folder(path: &Path) -> Result<ValidationResult, Vec<String>> {
    // This is not guaranteed to be constants::recording::VIDEO_FILENAME if the WebSocket recorder
    // is being used, which is why we search for it
    let Some(mp4_path) = path
        .read_dir()
        .map_err(|e| vec![e.to_string()])?
        .flatten()
        .map(|e| e.path())
        .find(|e| e.extension().and_then(|e| e.to_str()) == Some("mp4"))
    else {
        return Err(vec![format!("No MP4 file found in {}", path.display())]);
    };
    let csv_path = path.join(constants::filename::recording::INPUTS);
    if !csv_path.is_file() {
        return Err(vec![format!(
            "No CSV file found in {} (expected {})",
            path.display(),
            csv_path.display()
        )]);
    }
    let meta_path = path.join(constants::filename::recording::METADATA);
    if !meta_path.is_file() {
        return Err(vec![format!(
            "No metadata file found in {} (expected {})",
            path.display(),
            meta_path.display()
        )]);
    }

    let metadata = std::fs::read_to_string(&meta_path)
        .map_err(|e| vec![format!("Error reading metadata file: {e:?}")])?;
    let mut metadata = serde_json::from_str::<Metadata>(&metadata)
        .map_err(|e| vec![format!("Error parsing metadata file: {e:?}")])?;

    let (input_stats, mut invalid_reasons) =
        validation::for_recording(&metadata, &mp4_path, &csv_path)
            .map_err(|e| vec![format!("Error validating recording at {path:?}: {e:?}")])?;

    metadata.input_stats = Some(input_stats);

    match serde_json::to_string_pretty(&metadata) {
        Ok(metadata) => {
            if let Err(e) = std::fs::write(&meta_path, metadata) {
                invalid_reasons.push(format!("Error writing metadata file: {e:?}"));
            }
        }
        Err(e) => invalid_reasons.push(format!("Error generating JSON for metadata file: {e:?}")),
    }

    if invalid_reasons.is_empty() {
        Ok(ValidationResult {
            mp4_path,
            csv_path,
            meta_path,
            metadata,
        })
    } else {
        Err(invalid_reasons)
    }
}

#[allow(clippy::too_many_arguments)]
async fn upload_tar(
    tar_path: &Path,
    api_client: Arc<ApiClient>,
    api_token: &str,
    unreliable_connection: bool,
    video_filename: &str,
    control_filename: &str,
    video_duration_seconds: f64,
    tx: app_state::UiUpdateSender,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
) -> eyre::Result<String> {
    let file_size = std::fs::metadata(tar_path)
        .map(|m| m.len())
        .context("failed to get file size")?;

    tx.send(app_state::UiUpdate::UpdateUploadProgress(Some(
        ProgressData::default(),
    )))
    .await
    .ok();

    let upload_session = api_client
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

    tracing::info!(
        "Starting upload of {} bytes in {} chunks of {} bytes each; upload_id={}, game_control_id={}",
        file_size,
        upload_session.total_chunks,
        upload_session.chunk_size_bytes,
        upload_session.upload_id,
        upload_session.game_control_id
    );

    // Set up auto-abort for upload
    struct AbortUploadOnDrop {
        api_client: Arc<ApiClient>,
        api_token: String,
        upload_id: Option<String>,
    }
    impl AbortUploadOnDrop {
        pub fn disarm(&mut self) {
            self.upload_id = None;
        }
    }
    impl Drop for AbortUploadOnDrop {
        fn drop(&mut self) {
            if let Some(upload_id) = self.upload_id.take() {
                tracing::info!("Aborting upload of {upload_id} (auto-abort)");
                let api_client = self.api_client.clone();
                let api_token = self.api_token.clone();
                tokio::spawn(async move {
                    api_client
                        .abort_multipart_upload(&api_token, &upload_id)
                        .await
                        .ok();
                });
            }
        }
    }
    let mut abort_upload_on_drop = AbortUploadOnDrop {
        api_client: api_client.clone(),
        api_token: api_token.to_string(),
        upload_id: Some(upload_session.upload_id.clone()),
    };

    let mut chunk_etags = vec![];

    {
        let mut file = tokio::fs::File::open(tar_path)
            .await
            .context("failed to open tar file")?;

        // TODO: make this less sloppy.
        // Instead of allocating a chunk-sized buffer, and then allocating that buffer
        // again for each chunk's stream, figure out a way to stream each chunk from the file
        // directly into the hasher, and then stream each chunk directly into the uploader
        let mut buffer = vec![0u8; upload_session.chunk_size_bytes as usize];
        let client = reqwest::Client::new();

        let progress_sender = Arc::new(Mutex::new(ProgressSender::new(tx.clone(), file_size)));
        for chunk_number in 1..=upload_session.total_chunks {
            // Check if upload has been cancelled
            if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                eyre::bail!("Upload cancelled by user");
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
    tx: app_state::UiUpdateSender,
    bytes_uploaded: u64,
    last_update_time: std::time::Instant,
    file_size: u64,
    start_time: std::time::Instant,
}
impl ProgressSender {
    pub fn new(tx: app_state::UiUpdateSender, file_size: u64) -> Self {
        Self {
            tx,
            bytes_uploaded: 0,
            last_update_time: std::time::Instant::now(),
            file_size,
            start_time: std::time::Instant::now(),
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
        if self.last_update_time.elapsed().as_millis() > 25 {
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
        };
        self.tx
            .try_send(app_state::UiUpdate::UpdateUploadProgress(Some(data)))
            .ok();
    }
}

/// Scans the recording location for folders with .invalid files or without .uploaded files and returns information about them
pub fn scan_local_recordings(recording_location: &Path) -> Vec<LocalRecording> {
    let mut local_recordings = Vec::new();

    let Ok(entries) = recording_location.read_dir() else {
        return local_recordings;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let invalid_file_path = path.join(constants::filename::recording::INVALID);
        let uploaded_file_path = path.join(constants::filename::recording::UPLOADED);
        let metadata_path = path.join(constants::filename::recording::METADATA);

        // Get the folder name
        let folder_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        // Parse the timestamp from the folder name (unix timestamp in seconds)
        // Surely the user won't change the folder name :cluegi:
        let timestamp = folder_name
            .parse::<u64>()
            .ok()
            .map(|secs| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs));

        let info = LocalRecordingInfo {
            folder_name,
            folder_size: folder_size(&path).unwrap_or_default(),
            folder_path: path,
            timestamp,
        };

        if invalid_file_path.is_file() {
            // Read the error reasons from the .invalid file
            let error_reasons = std::fs::read_to_string(&invalid_file_path)
                .unwrap_or_else(|_| "Unknown error".to_string())
                .lines()
                .map(|s| s.to_string())
                .collect();

            local_recordings.push(LocalRecording::Invalid {
                info,
                error_reasons,
            });
        } else if !uploaded_file_path.is_file() {
            // Not uploaded yet (and not invalid)
            let metadata: Option<Metadata> = std::fs::read_to_string(metadata_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok());
            local_recordings.push(LocalRecording::Unuploaded {
                info,
                metadata: metadata.map(Box::new),
            });
        }
    }

    // Sort by timestamp, most recent first
    local_recordings.sort_by(|a, b| {
        b.info()
            .timestamp
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .cmp(
                &a.info()
                    .timestamp
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            )
    });

    local_recordings
}

fn folder_size(path: &Path) -> Result<u64, std::io::Error> {
    let mut size = 0;
    for entry in path.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            size += path.metadata()?.len();
        }
    }
    Ok(size)
}
