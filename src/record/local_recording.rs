use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use color_eyre::{Result, eyre};
use egui_wgpu::wgpu;
use serde::{Deserialize, Serialize};

use crate::{
    api::{ApiClient, ApiError, CompleteMultipartUploadChunk},
    output_types::Metadata,
    system::{hardware_id, hardware_specs},
};

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
    /// Create a new upload progress state from a fresh upload session
    pub fn new(
        upload_id: String,
        game_control_id: String,
        tar_path: PathBuf,
        total_chunks: u64,
        chunk_size_bytes: u64,
        expires_at: u64,
    ) -> Self {
        Self {
            upload_id,
            game_control_id,
            tar_path,
            chunk_etags: vec![],
            total_chunks,
            chunk_size_bytes,
            expires_at,
        }
    }

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
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let mut stream =
            serde_json::Deserializer::from_reader(reader).into_iter::<serde_json::Value>();

        // Read the first object which should be the UploadProgressState
        let first_value = stream
            .next()
            .ok_or_else(|| eyre::eyre!("Empty progress file"))??;
        let mut state: Self = serde_json::from_value(first_value)?;

        // If the state was saved in the old format (single JSON object with populated etags),
        // we're done (the etags are already in state.chunk_etags).
        // If it was saved in the new format (header + log lines), state.chunk_etags might be empty,
        // and we need to read the rest of the file.

        // Read subsequent objects as CompleteMultipartUploadChunk
        for value in stream {
            let chunk: CompleteMultipartUploadChunk = serde_json::from_value(value?)?;
            // Avoid duplicates if we're migrating or recovering from a weird state
            if !state
                .chunk_etags
                .iter()
                .any(|c| c.chunk_number == chunk.chunk_number)
            {
                state.chunk_etags.push(chunk);
            }
        }

        Ok(state)
    }

    /// Save progress state to a file (Snapshot + Log format)
    pub fn save_to_file(&self, path: &Path) -> eyre::Result<()> {
        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);

        // 1. Write the base state with EMPTY chunk_etags to the first line.
        // We clone to clear the vector without modifying self.
        let mut header_state = self.clone();
        header_state.chunk_etags.clear();
        serde_json::to_writer(&mut writer, &header_state)?;
        use std::io::Write;
        writeln!(&mut writer)?;

        // 2. Write all existing etags as subsequent lines
        for chunk in &self.chunk_etags {
            serde_json::to_writer(&mut writer, chunk)?;
            writeln!(&mut writer)?;
        }

        writer.flush()?;
        Ok(())
    }

    /// Append a single chunk completion to the log file
    pub fn append_chunk_to_file(&self, chunk: &CompleteMultipartUploadChunk) -> eyre::Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).append(true).open(
            &self
                .tar_path
                .parent()
                .unwrap()
                .join(constants::filename::recording::UPLOAD_PROGRESS),
        )?;

        // Ensure we start on a new line (though save_to_file guarantees ending with newline)
        // serde_json::to_writer doesn't add a newline
        serde_json::to_writer(&mut file, chunk)?;
        use std::io::Write;
        writeln!(&mut file)?;

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

    /// Get the total number of bytes uploaded so far
    pub fn uploaded_bytes(&self) -> u64 {
        self.chunk_etags.len() as u64 * self.chunk_size_bytes
    }

    /// Cleans up the tar file associated with this upload progress.
    pub fn cleanup_tar_file(&self) {
        std::fs::remove_file(&self.tar_path).ok();
    }
}

#[derive(Debug, Clone)]
pub struct LocalRecordingInfo {
    pub folder_name: String,
    pub folder_path: PathBuf,
    pub folder_size: u64,
    pub timestamp: Option<std::time::SystemTime>,
}

impl std::fmt::Display for LocalRecordingInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.folder_name, self.folder_path.display())
    }
}

/// A recording that has a paused upload in progress.
/// This struct guarantees that the upload state has been validated and is ready to resume.
#[derive(Debug, Clone)]
pub struct LocalRecordingPaused {
    pub info: LocalRecordingInfo,
    pub metadata: Option<Box<Metadata>>,
    upload_progress: UploadProgressState,
}

impl LocalRecordingPaused {
    pub fn new(
        info: LocalRecordingInfo,
        metadata: Option<Box<Metadata>>,
        upload_progress: UploadProgressState,
    ) -> Self {
        Self {
            info,
            metadata,
            upload_progress,
        }
    }

    /// Cleans up upload artifacts (progress file and tar file).
    pub fn cleanup_upload_artifacts(self) {
        std::fs::remove_file(self.upload_progress_path()).ok();
        self.upload_progress.cleanup_tar_file();
        tracing::info!(
            "Cleaned up upload artifacts for upload_id={}",
            self.upload_progress.upload_id
        );
    }

    /// Get a reference to the upload progress state.
    pub fn upload_progress(&self) -> &UploadProgressState {
        &self.upload_progress
    }

    /// Mutate the upload progress state and save to file.
    pub fn mutate_upload_progress<R>(
        &mut self,
        f: impl FnOnce(&mut UploadProgressState) -> R,
    ) -> R {
        let r = f(&mut self.upload_progress);
        self.save_upload_progress().ok();
        r
    }

    /// Records a successful chunk upload: updates in-memory state and appends to the log file.
    pub fn record_chunk_completion(
        &mut self,
        chunk: CompleteMultipartUploadChunk,
    ) -> eyre::Result<()> {
        // Update in-memory
        self.upload_progress.chunk_etags.push(chunk.clone());

        // Append to disk (efficient log append)
        // We construct the path manually here or use the one from info,
        // but UploadProgressState doesn't store the full progress file path, only tar path.
        // We can use the helper method on self.

        let path = self.upload_progress_path();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .create(false) // Should already exist
            .open(path)?;

        serde_json::to_writer(&mut file, &chunk)?;
        use std::io::Write;
        writeln!(&mut file)?;

        Ok(())
    }

    /// Save upload progress state to .upload-progress file.
    pub fn save_upload_progress(&self) -> eyre::Result<()> {
        self.upload_progress
            .save_to_file(&self.upload_progress_path())
    }

    pub async fn abort_and_cleanup(
        self,
        api_client: &ApiClient,
        api_token: &str,
    ) -> Result<(), ApiError> {
        let response = api_client
            .abort_multipart_upload(api_token, &self.upload_progress.upload_id)
            .await;
        tracing::info!(
            "Aborted multipart upload for upload_id={}",
            self.upload_progress.upload_id
        );
        self.cleanup_upload_artifacts();
        response.map(|_| ())
    }

    /// Mark recording as uploaded, writing .uploaded marker file.
    /// Consumes self and returns Uploaded LocalRecording variant.
    pub fn mark_as_uploaded(self, game_control_id: String) -> std::io::Result<LocalRecording> {
        let info = self.info.clone();
        self.cleanup_upload_artifacts();
        std::fs::write(
            info.folder_path
                .join(constants::filename::recording::UPLOADED),
            &game_control_id,
        )?;
        tracing::info!(
            "Marked recording as uploaded: game_control_id={}, folder_path={}",
            game_control_id,
            info.folder_path.display()
        );
        Ok(LocalRecording::Uploaded {
            info,
            game_control_id,
        })
    }

    /// Mark recording as server-invalid, writing .server_invalid marker.
    /// Consumes self and returns Invalid LocalRecording variant.
    pub fn mark_as_server_invalid(self, message: &str) -> std::io::Result<LocalRecording> {
        let info = self.info.clone();
        let metadata = self.metadata.clone();
        self.cleanup_upload_artifacts();
        std::fs::write(
            info.folder_path
                .join(constants::filename::recording::SERVER_INVALID),
            message,
        )?;
        tracing::info!(
            "Marked recording as server-invalid: message={}, folder_path={}",
            message,
            info.folder_path.display()
        );
        Ok(LocalRecording::Invalid {
            info,
            metadata,
            error_reasons: message.lines().map(String::from).collect(),
            by_server: true,
        })
    }

    fn upload_progress_path(&self) -> PathBuf {
        self.info
            .folder_path
            .join(constants::filename::recording::UPLOAD_PROGRESS)
    }
}

#[derive(Debug, Clone)]
pub enum LocalRecording {
    Invalid {
        info: LocalRecordingInfo,
        metadata: Option<Box<Metadata>>,
        error_reasons: Vec<String>,
        by_server: bool,
    },
    Unuploaded {
        info: LocalRecordingInfo,
        metadata: Option<Box<Metadata>>,
    },
    Paused(LocalRecordingPaused),
    Uploaded {
        info: LocalRecordingInfo,
        #[allow(dead_code)]
        game_control_id: String,
    },
}

impl LocalRecording {
    /// Creates the recording folder at the given path if it doesn't already exist.
    /// Returns a LocalRecording::Unuploaded variant. Called at .start() of recording.
    pub fn create_at(path: &Path) -> Result<LocalRecording> {
        std::fs::create_dir_all(path)?;

        // Build info similar to from_path
        let folder_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let timestamp = folder_name
            .parse::<u64>()
            .ok()
            .map(|secs| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs));

        let info = LocalRecordingInfo {
            folder_name,
            folder_size: 0, // New folder, no content yet
            folder_path: path.to_path_buf(),
            timestamp,
        };

        Ok(LocalRecording::Unuploaded {
            info,
            metadata: None,
        })
    }

    /// Get the common info for any recording variant
    pub fn info(&self) -> &LocalRecordingInfo {
        match self {
            LocalRecording::Invalid { info, .. } => info,
            LocalRecording::Unuploaded { info, .. } => info,
            LocalRecording::Paused(paused) => &paused.info,
            LocalRecording::Uploaded { info, .. } => info,
        }
    }

    /// Get the metadata for any recording variant
    pub fn metadata(&self) -> Option<&Metadata> {
        match self {
            LocalRecording::Invalid { metadata, .. } => metadata.as_deref(),
            LocalRecording::Unuploaded { metadata, .. } => metadata.as_deref(),
            LocalRecording::Paused(paused) => paused.metadata.as_deref(),
            LocalRecording::Uploaded { .. } => None,
        }
    }

    /// Convenience accessor for error reasons (only for Invalid variant)
    #[allow(dead_code)]
    pub fn error_reasons(&self) -> Option<&[String]> {
        match self {
            LocalRecording::Invalid { error_reasons, .. } => Some(error_reasons),
            _ => None,
        }
    }

    /// Deletes the recording folder and cleans up server state.
    /// For Paused uploads, aborts the multipart upload on the server.
    pub async fn delete(self, api_client: &ApiClient, api_token: &str) -> std::io::Result<()> {
        let folder_path = self.info().folder_path.clone();

        // For Paused variant, abort the upload on the server first
        if let LocalRecording::Paused(paused) = self {
            paused.abort_and_cleanup(api_client, api_token).await.ok();
        }

        tokio::fs::remove_dir_all(&folder_path).await
    }

    /// Deletes the recording folder synchronously. Use this only in Drop handlers
    /// where async is not available. Does NOT abort server uploads.
    pub fn delete_without_abort_sync(&self) -> std::io::Result<()> {
        std::fs::remove_dir_all(&self.info().folder_path)
    }

    /// Scans a single recording folder and returns its state
    pub fn from_path(path: &Path) -> Option<LocalRecording> {
        if !path.is_dir() {
            return None;
        }

        let invalid_file_path = path.join(constants::filename::recording::INVALID);
        let server_invalid_file_path = path.join(constants::filename::recording::SERVER_INVALID);
        let uploaded_file_path = path.join(constants::filename::recording::UPLOADED);
        let upload_progress_file_path = path.join(constants::filename::recording::UPLOAD_PROGRESS);
        let metadata_path = path.join(constants::filename::recording::METADATA);

        // Get the folder name
        let folder_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        // Parse the timestamp from the folder name (unix timestamp in seconds)
        let timestamp = folder_name
            .parse::<u64>()
            .ok()
            .map(|secs| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs));

        let info = LocalRecordingInfo {
            folder_name,
            folder_size: folder_size(path).unwrap_or_default(),
            folder_path: path.to_path_buf(),
            timestamp,
        };

        if uploaded_file_path.is_file() {
            // Read the game_control_id from the .uploaded file
            let game_control_id = std::fs::read_to_string(&uploaded_file_path)
                .unwrap_or_else(|_| "unknown".to_string())
                .trim()
                .to_string();

            Some(LocalRecording::Uploaded {
                info,
                game_control_id,
            })
        } else {
            // Not uploaded yet (and not invalid)
            let metadata: Option<Box<Metadata>> = std::fs::read_to_string(metadata_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .map(Box::new);

            if invalid_file_path.is_file() {
                // Read the error reasons from the [`constants::filename::recording::INVALID`] file
                let error_reasons = std::fs::read_to_string(&invalid_file_path)
                    .unwrap_or_else(|_| "Unknown error".to_string())
                    .lines()
                    .map(|s| s.to_string())
                    .collect();

                Some(LocalRecording::Invalid {
                    info,
                    metadata,
                    error_reasons,
                    by_server: false,
                })
            } else if server_invalid_file_path.is_file() {
                // Read the error reasons from the [`constants::filename::recording::SERVER_INVALID`] file
                let error_reasons = std::fs::read_to_string(&server_invalid_file_path)
                    .unwrap_or_else(|_| "Unknown error".to_string())
                    .lines()
                    .map(|s| s.to_string())
                    .collect();

                Some(LocalRecording::Invalid {
                    info,
                    metadata,
                    error_reasons,
                    by_server: true,
                })
            } else if upload_progress_file_path.is_file() {
                // Upload was paused - there's a .upload-progress file
                match UploadProgressState::load_from_file(&upload_progress_file_path) {
                    Ok(upload_progress) => Some(LocalRecording::Paused(LocalRecordingPaused {
                        info,
                        metadata,
                        upload_progress,
                    })),
                    Err(e) => {
                        // Corrupted progress file - treat as unuploaded so fresh upload can be attempted
                        tracing::warn!(
                            "Failed to load upload progress for {}, treating as unuploaded: {:?}",
                            info.folder_name,
                            e
                        );
                        Some(LocalRecording::Unuploaded { info, metadata })
                    }
                }
            } else {
                Some(LocalRecording::Unuploaded { info, metadata })
            }
        }
    }

    /// Scans the recording directory for all local recordings
    pub fn scan_directory(recording_location: &Path) -> Vec<LocalRecording> {
        let mut local_recordings = Vec::new();

        let Ok(entries) = recording_location.read_dir() else {
            return local_recordings;
        };

        for entry in entries.flatten() {
            if let Some(recording) = Self::from_path(&entry.path()) {
                local_recordings.push(recording);
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

    /// Write metadata to disk and validate the recording.
    /// Creates a [`constants::filename::recording::INVALID`] file if validation fails.
    #[allow(clippy::too_many_arguments)]
    // TODO: refactor all of these arguments into a single struct
    pub(crate) async fn write_metadata_and_validate(
        recording_location: PathBuf,
        game_exe: String,
        game_resolution: (u32, u32),
        start_instant: Instant,
        start_time: SystemTime,
        average_fps: Option<f64>,
        window_name: Option<String>,
        adapter_infos: &[wgpu::AdapterInfo],
        gamepads: HashMap<input_capture::GamepadId, input_capture::GamepadMetadata>,
        recorder_id: &str,
        recorder_extra: Option<serde_json::Value>,
    ) -> Result<()> {
        // Resolve metadata path from recording location
        let metadata_path = recording_location.join(constants::filename::recording::METADATA);

        // Create metadata
        let duration = start_instant.elapsed().as_secs_f64();

        let start_timestamp = start_time.duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
        let end_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        let hardware_id = hardware_id::get()?;

        let hardware_specs = match hardware_specs::get_hardware_specs(
            adapter_infos
                .iter()
                .map(|a| hardware_specs::GpuSpecs::from_name(&a.name))
                .collect(),
        ) {
            Ok(specs) => Some(specs),
            Err(e) => {
                tracing::warn!("Failed to get hardware specs: {}", e);
                None
            }
        };

        let metadata = Metadata {
            game_exe,
            game_resolution: Some(game_resolution),
            owl_control_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            owl_control_commit: Some(
                git_version::git_version!(
                    args = ["--abbrev=40", "--always", "--dirty=-modified"],
                    fallback = "unknown"
                )
                .to_string(),
            ),
            session_id: uuid::Uuid::new_v4().to_string(),
            hardware_id,
            hardware_specs,
            gamepads: gamepads
                .into_iter()
                .map(|(id, metadata)| (id, metadata.into()))
                .collect(),
            start_timestamp,
            end_timestamp,
            duration,
            input_stats: None,
            recorder: Some(recorder_id.to_string()),
            recorder_extra,
            window_name,
            average_fps,
        };

        // Write metadata to disk
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        tokio::fs::write(&metadata_path, &metadata_json).await?;

        // Validate the recording immediately after stopping to create [`constants::filename::recording::INVALID`] file if needed
        tracing::info!("Validating recording at {}", recording_location.display());
        tokio::task::spawn_blocking(move || {
            if let Err(e) = crate::validation::validate_folder(&recording_location) {
                tracing::error!("Error validating recording on stop: {e}");
            }
        })
        .await
        .ok();

        Ok(())
    }
}

/// Calculate the total size of all files in a folder
fn folder_size(path: &Path) -> Result<u64, std::io::Error> {
    let mut size = 0;
    for entry in path.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().unwrap_or_default() != "tar" {
            size += path.metadata()?.len();
        }
    }
    Ok(size)
}
