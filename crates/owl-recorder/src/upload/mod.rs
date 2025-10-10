use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use color_eyre::eyre::{self, Context as _, ContextCompat};
use serde::Deserialize;

use crate::{
    api::ApiClient,
    app_state::{self, AppState},
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
    let (api_token, unreliable_connection) = {
        let config = app_state.config.read().unwrap();
        (
            config.credentials.api_key.clone(),
            config.preferences.unreliable_connection,
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
    )
    .await
    {
        Ok(final_stats) => {
            if let Err(e) = app_state.upload_stats.write().unwrap().update(
                final_stats.total_duration_uploaded,
                final_stats.total_files_uploaded,
                final_stats.total_bytes_uploaded,
            ) {
                tracing::error!("Error updating upload stats: {e}");
            }
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
async fn run(
    recording_location: &Path,
    api_client: Arc<ApiClient>,
    _api_token: String,
    _unreliable_connection: bool,
) -> eyre::Result<FinalStats> {
    let mut stats = FinalStats::default();

    for entry in recording_location.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if path.join(".invalid").is_file() || path.join(".uploaded").is_file() {
            continue;
        }

        let recording_stats = match upload_folder(
            &path,
            api_client.clone(),
            &_api_token,
            _unreliable_connection,
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
) -> eyre::Result<RecordingStats> {
    tracing::info!("Validating folder {}", path.display());
    let validation = match validate_folder(path) {
        Ok(validation_paths) => validation_paths,
        Err(e) => {
            std::fs::write(path.join(".invalid"), e.join("\n")).ok();
            return Err(eyre::eyre!("Validation failures: {}", e.join("\n")));
        }
    };

    tracing::info!("Creating tar file for {}", path.display());
    let tar_path = tokio::task::spawn_blocking(|| {
        let tar_path = PathBuf::from(format!(
            "{}.tar",
            &uuid::Uuid::new_v4().simple().to_string()[0..16]
        ));
        let mut tar = tar::Builder::new(std::fs::File::create(&tar_path)?);
        for path in [
            validation.mp4_path,
            validation.csv_path,
            validation.meta_path,
        ] {
            tar.append_file(
                path.file_name().context("failed to get file name")?,
                &mut std::fs::File::open(&path)?,
            )?;
        }

        eyre::Ok(tar_path)
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

    upload_tar(&tar_path.0, api_client, api_token, unreliable_connection)
        .await
        .context("error uploading tar file")?;

    Ok(RecordingStats {
        duration: validation.metadata.duration as f64,
        bytes: std::fs::metadata(&tar_path.0)
            .map(|m| m.len())
            .unwrap_or_default(),
    })
}

async fn upload_tar(
    _tar_path: &Path,
    _api_client: Arc<ApiClient>,
    _api_token: &str,
    _unreliable_connection: bool,
) -> eyre::Result<()> {
    Ok(())
}

// This is a bit messy - I don't love using a Vec of Strings for the errors -
// but I wanted to capture the multi-error nature of the validation process
//
// TODO: Think of a better way to handle this
struct ValidationResult {
    mp4_path: PathBuf,
    csv_path: PathBuf,
    meta_path: PathBuf,
    metadata: Metadata,
}
fn validate_folder(path: &Path) -> Result<ValidationResult, Vec<String>> {
    let Some(mp4_path) = path
        .read_dir()
        .map_err(|e| vec![e.to_string()])?
        .flatten()
        .map(|e| e.path())
        .find(|e| e.extension().and_then(|e| e.to_str()) == Some("mp4"))
    else {
        return Err(vec![format!("No MP4 file found in {}", path.display())]);
    };
    let csv_path = path.join("inputs.csv");
    if !csv_path.is_file() {
        return Err(vec![format!(
            "No CSV file found in {} (expected {})",
            path.display(),
            csv_path.display()
        )]);
    }
    let meta_path = path.join("metadata.json");
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
