use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use backoff::{Error as BackoffError, ExponentialBackoff, future::retry_notify};

use futures::TryStreamExt as _;
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};

use crate::{
    api::{ApiClient, ApiError, CompleteMultipartUploadChunk},
    app_state::{UiUpdateUnreliable, UiUpdateUnreliableSender},
    record::{LocalRecording, LocalRecordingPaused},
    upload::{FileProgress, ProgressSender},
};

/// Result type for `upload_tar` that distinguishes between different outcomes.
pub enum UploadTarOutput {
    /// Upload completed successfully, recording is now Uploaded variant
    Success(LocalRecording),
    /// Server rejected the upload, recording is now Invalid variant
    ServerInvalid(LocalRecording),
    /// Upload was paused by user
    Paused(LocalRecording),
}

#[derive(Debug)]
pub enum UploadTarError {
    Io(std::io::Error),
    Serde(serde_json::Error),
    UploadSessionExpired {
        upload_id: String,
        client_time: u64,
        expires_at: u64,
    },
    Api {
        api_request: &'static str,
        error: ApiError,
    },
    FailedToUploadChunk {
        chunk_number: u64,
        total_chunks: u64,
        retries: u32,
        error: UploadSingleChunkError,
    },
    FailedToCompleteMultipartUpload(String),
}
impl std::fmt::Display for UploadTarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UploadTarError::Io(e) => write!(f, "I/O error: {e}"),
            UploadTarError::Serde(e) => {
                write!(f, "Serde error: {e}")
            }
            UploadTarError::UploadSessionExpired {
                upload_id,
                client_time,
                expires_at,
            } => {
                write!(
                    f,
                    "Upload session expired: {upload_id} (client_time={client_time}, expires_at={expires_at})"
                )
            }
            UploadTarError::Api { api_request, error } => {
                write!(f, "API error for {api_request}: {error}")
            }
            UploadTarError::FailedToUploadChunk {
                chunk_number,
                total_chunks,
                retries,
                error,
            } => {
                write!(
                    f,
                    "Failed to upload chunk {chunk_number}/{total_chunks} after {retries} retries: {error:?}"
                )
            }
            UploadTarError::FailedToCompleteMultipartUpload(message) => {
                write!(f, "Failed to complete multipart upload: {message}")
            }
        }
    }
}
impl std::error::Error for UploadTarError {}
impl UploadTarError {
    /// Returns true if this error is due to a network connectivity issue
    pub fn is_network_error(&self) -> bool {
        match self {
            UploadTarError::Api { error, .. } => error.is_network_error(),
            UploadTarError::FailedToUploadChunk { error, .. } => error.is_network_error(),
            _ => false,
        }
    }
}
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

pub async fn run(
    paused: LocalRecordingPaused,
    api_client: Arc<ApiClient>,
    api_token: &str,
    unreliable_tx: UiUpdateUnreliableSender,
    pause_flag: Arc<std::sync::atomic::AtomicBool>,
    file_progress: FileProgress,
) -> Result<UploadTarOutput, UploadTarError> {
    let (tar_path, chunk_size_bytes, total_chunks, upload_id, game_control_id, expires_at) = {
        let progress = paused.upload_progress();
        (
            progress.tar_path.clone(),
            progress.chunk_size_bytes,
            progress.total_chunks,
            progress.upload_id.clone(),
            progress.game_control_id.clone(),
            progress.expires_at,
        )
    };

    let file_size = std::fs::metadata(&tar_path).map(|m| m.len())?;
    unreliable_tx
        .send(UiUpdateUnreliable::UpdateUploadProgress(Some(
            Default::default(),
        )))
        .ok();

    let start_chunk = paused.upload_progress().next_chunk_number();

    tracing::info!(
        "Starting upload of {file_size} bytes in {total_chunks} chunks of {chunk_size_bytes} bytes each; upload_id={upload_id}, game_control_id={game_control_id}"
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
            let Some(paused) = self.paused.take() else {
                return;
            };
            tracing::info!(
                "Aborting upload of {} (guard drop / unexpected failure)",
                paused.upload_progress().upload_id
            );

            // Abort server upload
            let api_client = self.api_client.clone();
            let api_token = self.api_token.clone();
            tokio::spawn(async move {
                paused.abort_and_cleanup(&api_client, &api_token).await.ok();
            });
        }
    }

    let mut guard = AbortUploadOnDrop::new(api_client.clone(), api_token.to_string(), paused);

    {
        let file = tokio::fs::File::open(tar_path.clone()).await?;

        // Pipeline Channels
        // Channel 1: Producer -> Signer
        // Payload: (Chunk Data, Chunk Hash, Chunk Number)
        let (tx_hashed, mut rx_hashed) = tokio::sync::mpsc::channel(2);

        // Channel 2: Signer -> Uploader
        // Payload: (Chunk Data, Upload URL, Chunk Number)
        let (tx_signed, mut rx_signed) = tokio::sync::mpsc::channel(2);

        // --- STAGE 1: PRODUCER (Read & Hash) ---
        let producer_handle = tokio::spawn({
            let mut file = file;
            let pause_flag = pause_flag.clone();
            async move {
                // If resuming, seek to the correct position in the file
                if start_chunk > 1 {
                    let bytes_to_skip = (start_chunk - 1) * chunk_size_bytes;
                    if let Err(e) = file.seek(std::io::SeekFrom::Start(bytes_to_skip)).await {
                        return Err(UploadTarError::Io(e));
                    }
                    tracing::info!(
                        "Seeking to byte {} to resume from chunk {}",
                        bytes_to_skip,
                        start_chunk
                    );
                }

                let mut buffer = vec![0u8; chunk_size_bytes as usize];

                for chunk_number in start_chunk..=total_chunks {
                    // Check pause
                    if pause_flag.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }

                    // Read chunk
                    let mut buffer_start = 0;
                    loop {
                        match file.read(&mut buffer[buffer_start..]).await {
                            Ok(0) => break, // EOF
                            Ok(n) => buffer_start += n,
                            Err(e) => return Err(UploadTarError::Io(e)),
                        }
                    }

                    let chunk_size = buffer_start;
                    if chunk_size == 0 {
                        break;
                    }

                    let chunk_data = buffer[..chunk_size].to_vec();

                    // Offload Hashing to blocking thread
                    let hash_result = tokio::task::spawn_blocking({
                        let data = chunk_data.clone();
                        move || sha256::digest(&data)
                    })
                    .await;

                    let chunk_hash = match hash_result {
                        Ok(hash) => hash,
                        Err(join_err) => {
                            return Err(UploadTarError::from(std::io::Error::other(join_err)));
                        }
                    };

                    if tx_hashed
                        .send(Ok((chunk_data, chunk_hash, chunk_number)))
                        .await
                        .is_err()
                    {
                        break; // Receiver dropped
                    }
                }
                Ok(())
            }
        });

        // --- STAGE 2: SIGNER (Get Upload URL) ---
        let signer_handle = tokio::spawn({
            let api_client = api_client.clone();
            let api_token = api_token.to_string();
            let upload_id = upload_id.clone();
            let pause_flag = pause_flag.clone();
            async move {
                while let Some(msg) = rx_hashed.recv().await {
                    if pause_flag.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }

                    let (chunk_data, chunk_hash, chunk_number) = match msg {
                        Ok(val) => val,
                        Err(e) => {
                            let _ = tx_signed.send(Err(e)).await;
                            break;
                        }
                    };

                    // Retry with exponential backoff for getting signed URL
                    let backoff = ExponentialBackoff {
                        initial_interval: Duration::from_millis(500),
                        max_interval: Duration::from_secs(8),
                        max_elapsed_time: Some(Duration::from_secs(16)),
                        multiplier: 2.0,
                        randomization_factor: 0.25,
                        ..Default::default()
                    };

                    let upload_url_result = retry_notify(
                        backoff,
                        || async {
                            api_client
                                .upload_multipart_chunk(
                                    &api_token,
                                    &upload_id,
                                    chunk_number,
                                    &chunk_hash,
                                )
                                .await
                                .map(|resp| resp.upload_url)
                                .map_err(BackoffError::transient)
                        },
                        |err, dur| {
                            tracing::warn!(
                                "Failed to get signed URL for chunk {}, retrying in {dur:?}: {err:?}",
                                chunk_number
                            );
                        },
                    )
                    .await;

                    match upload_url_result {
                        Ok(url) => {
                            if tx_signed
                                .send(Ok((chunk_data, url, chunk_number)))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(e) => {
                            let err = UploadTarError::Api {
                                api_request: "upload_multipart_chunk",
                                error: e,
                            };
                            let _ = tx_signed.send(Err(err)).await;
                            break;
                        }
                    }
                }
            }
        });

        // Initialize progress sender with bytes already uploaded
        let bytes_already_uploaded = (start_chunk - 1) * chunk_size_bytes;
        let progress_sender = Arc::new(Mutex::new({
            let mut sender = ProgressSender::new(unreliable_tx.clone(), file_size, file_progress);
            sender.set_bytes_uploaded(bytes_already_uploaded);
            sender
        }));

        let client = reqwest::Client::new();

        // --- STAGE 3: UPLOADER (PUT Data) ---
        while let Some(msg) = rx_signed.recv().await {
            // Check for error from previous stages
            let (chunk_data, upload_url, chunk_number) = match msg {
                Ok(val) => val,
                Err(e) => return Err(e),
            };

            // Check if upload has been paused (user-initiated pause)
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
            if now >= expires_at {
                tracing::error!(
                    "Upload session expired: upload_id={}, client_time={}, expires_at={}, diff={}s. If this is a fresh upload, the system clock may be incorrect.",
                    upload_id,
                    now,
                    expires_at,
                    now as i64 - expires_at as i64
                );
                return Err(UploadTarError::UploadSessionExpired {
                    upload_id,
                    client_time: now,
                    expires_at,
                });
            }
            let seconds_left = expires_at as i64 - now as i64;
            if seconds_left < 60 && chunk_number % 10 == 0 {
                tracing::warn!("Upload session expires in {} seconds!", seconds_left);
            }

            tracing::info!(
                "Uploading chunk {chunk_number}/{total_chunks} for upload_id {upload_id}"
            );

            // Store bytes_uploaded before attempting the chunk
            let bytes_before_chunk = progress_sender.lock().unwrap().bytes_uploaded();
            let mut retries = 0u32;

            // Should be about 5-6 retries
            let backoff = ExponentialBackoff {
                initial_interval: Duration::from_millis(500),
                max_interval: Duration::from_secs(8),
                max_elapsed_time: Some(Duration::from_secs(16)),
                multiplier: 2.0,
                randomization_factor: 0.25,
                ..Default::default()
            };

            let etag = retry_notify(
                backoff,
                || async {
                    // Reset progress before each attempt
                    progress_sender
                        .lock()
                        .unwrap()
                        .set_bytes_uploaded(bytes_before_chunk);

                    // Create a stream that wraps chunk_data and tracks upload progress
                    let progress_stream =
                        tokio_util::io::ReaderStream::new(std::io::Cursor::new(chunk_data.clone()))
                            .inspect_ok({
                                let progress_sender = progress_sender.clone();
                                move |bytes| {
                                    progress_sender
                                        .lock()
                                        .unwrap()
                                        .increment_bytes_uploaded(bytes.len() as u64);
                                }
                            });

                    let res = client
                        .put(&upload_url)
                        .header("Content-Type", "application/octet-stream")
                        .header("Content-Length", chunk_data.len())
                        .body(reqwest::Body::wrap_stream(progress_stream))
                        .send()
                        .await
                        .map_err(|e| BackoffError::transient(UploadSingleChunkError::Reqwest(e)))?;

                    if !res.status().is_success() {
                        return Err(BackoffError::transient(
                            UploadSingleChunkError::ChunkUploadFailed(res.status()),
                        ));
                    }

                    let etag = res
                        .headers()
                        .get("etag")
                        .and_then(|h| h.to_str().ok())
                        .map(|s| s.trim_matches('"').to_owned())
                        .ok_or_else(|| {
                            BackoffError::transient(UploadSingleChunkError::NoEtagHeaderFound)
                        })?;

                    Ok(etag)
                },
                |err, dur| {
                    retries += 1;
                    tracing::warn!(
                        "Failed to upload chunk {chunk_number}/{total_chunks}, retrying in {dur:?}: {err:?}"
                    );
                },
            )
            .await
            .map_err(|e| UploadTarError::FailedToUploadChunk {
                chunk_number,
                total_chunks,
                retries,
                error: e,
            })?;

            progress_sender.lock().unwrap().send();

            // Update progress state with new chunk and save to file (APPEND ONLY)
            if let Err(e) = guard
                .paused_mut()
                .record_chunk_completion(CompleteMultipartUploadChunk {
                    chunk_number,
                    etag,
                })
            {
                tracing::error!("Failed to append chunk completion to log: {:?}", e);
            }

            tracing::info!(
                "Uploaded chunk {chunk_number}/{total_chunks} for upload_id {upload_id}"
            );
        }

        // Ensure producer and signer tasks didn't crash
        if let Err(e) = producer_handle.await {
            tracing::error!("Producer task failed: {:?}", e);
            return Err(UploadTarError::from(std::io::Error::other(
                "Producer task failed",
            )));
        }
        if let Err(e) = signer_handle.await {
            tracing::error!("Signer task failed: {:?}", e);
            return Err(UploadTarError::from(std::io::Error::other(
                "Signer task failed",
            )));
        }
    }

    let completion_result = match api_client
        .complete_multipart_upload(
            api_token,
            &upload_id,
            &guard.paused().upload_progress().chunk_etags,
        )
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

#[derive(Debug)]
pub enum UploadSingleChunkError {
    Io(std::io::Error),
    Reqwest(reqwest::Error),
    ChunkUploadFailed(reqwest::StatusCode),
    NoEtagHeaderFound,
}
impl std::fmt::Display for UploadSingleChunkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UploadSingleChunkError::Io(e) => write!(f, "I/O error: {e}"),
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
impl UploadSingleChunkError {
    /// Returns true if this error is due to a network connectivity issue
    pub fn is_network_error(&self) -> bool {
        match self {
            UploadSingleChunkError::Reqwest(e) => e.is_connect() || e.is_timeout(),
            _ => false,
        }
    }
}
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
