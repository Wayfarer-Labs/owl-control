use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use color_eyre::Result;
use egui_wgpu::wgpu;

use crate::{
    output_types::Metadata,
    system::{hardware_id, hardware_specs},
};

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
        metadata: Option<Box<Metadata>>,
        error_reasons: Vec<String>,
    },
    Unuploaded {
        info: LocalRecordingInfo,
        metadata: Option<Box<Metadata>>,
    },
    Uploaded {
        info: LocalRecordingInfo,
        #[allow(dead_code)]
        game_control_id: String,
    },
}

impl LocalRecording {
    /// Creates the recording folder at the given path if it doesn't already exist.
    /// Returns basic info about the folder. Called at .start() of recording.
    pub fn create_at(path: &Path) -> Result<LocalRecordingInfo> {
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
            folder_size: folder_size(path).unwrap_or_default(),
            folder_path: path.to_path_buf(),
            timestamp,
        };

        Ok(info)
    }
    /// Get the common info for any recording variant
    pub fn info(&self) -> &LocalRecordingInfo {
        match self {
            LocalRecording::Invalid { info, .. } => info,
            LocalRecording::Unuploaded { info, .. } => info,
            LocalRecording::Uploaded { info, .. } => info,
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

    /// Convenience accessor for metadata (only for Unuploaded variant)
    #[allow(dead_code)]
    pub fn metadata(&self) -> Option<&Metadata> {
        match self {
            LocalRecording::Unuploaded { metadata, .. } => metadata.as_deref(),
            _ => None,
        }
    }

    /// Scans a single recording folder and returns its state
    pub fn from_path(path: &Path) -> Option<LocalRecording> {
        if !path.is_dir() {
            return None;
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
                // Read the error reasons from the .invalid file
                let error_reasons = std::fs::read_to_string(&invalid_file_path)
                    .unwrap_or_else(|_| "Unknown error".to_string())
                    .lines()
                    .map(|s| s.to_string())
                    .collect();

                Some(LocalRecording::Invalid {
                    info,
                    metadata,
                    error_reasons,
                })
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
    /// Creates a .invalid file if validation fails.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn write_metadata_and_validate(
        recording_location: PathBuf,
        game_exe: String,
        game_resolution: (u32, u32),
        start_instant: Instant,
        start_time: SystemTime,
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
        };

        // Write metadata to disk
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        tokio::fs::write(&metadata_path, &metadata_json).await?;

        // Validate the recording immediately after stopping to create .invalid file if needed
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
        if path.is_file() {
            size += path.metadata()?.len();
        }
    }
    Ok(size)
}
