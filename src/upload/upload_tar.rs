use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

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
        max_retries: u32,
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
                let bytes_before_chunk = progress_sender.lock().unwrap().bytes_uploaded();

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

                        // Update progress state with new chunk and save to file
                        guard.paused_mut().mutate_upload_progress(|progress| {
                            progress
                                .chunk_etags
                                .push(CompleteMultipartUploadChunk { chunk_number, etag });
                        });

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

struct Chunk<'a> {
    data: &'a [u8],
    hash: &'a str,
    number: u64,
}

#[derive(Debug)]
pub enum UploadSingleChunkError {
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
impl UploadSingleChunkError {
    /// Returns true if this error is due to a network connectivity issue
    pub fn is_network_error(&self) -> bool {
        match self {
            UploadSingleChunkError::Reqwest(e) => e.is_connect() || e.is_timeout(),
            UploadSingleChunkError::Api { error, .. } => error.is_network_error(),
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
