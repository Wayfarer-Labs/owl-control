use std::path::{Path, PathBuf};

use crate::output_types::Metadata;

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
    Uploaded {
        info: LocalRecordingInfo,
        #[allow(dead_code)]
        game_control_id: String,
    },
}

impl LocalRecording {
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

        if invalid_file_path.is_file() {
            // Read the error reasons from the .invalid file
            let error_reasons = std::fs::read_to_string(&invalid_file_path)
                .unwrap_or_else(|_| "Unknown error".to_string())
                .lines()
                .map(|s| s.to_string())
                .collect();

            Some(LocalRecording::Invalid {
                info,
                error_reasons,
            })
        } else if uploaded_file_path.is_file() {
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
            let metadata: Option<Metadata> = std::fs::read_to_string(metadata_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok());

            Some(LocalRecording::Unuploaded {
                info,
                metadata: metadata.map(Box::new),
            })
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
