use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use color_eyre::eyre;
use serde::Deserialize;

use crate::{
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

pub async fn start(app_state: Arc<AppState>, recording_location: PathBuf) {
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

    match run(&recording_location, api_token, unreliable_connection).await {
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
    _api_token: String,
    _unreliable_connection: bool,
) -> eyre::Result<FinalStats> {
    for entry in recording_location.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if path.join(".invalid").is_file() || path.join(".uploaded").is_file() {
            continue;
        }

        tracing::info!("Validating folder {}", path.display());

        if let Err(e) = validate_folder(&path) {
            tracing::error!("Error validating folder {}: {:?}", path.display(), e);
            std::fs::write(path.join(".invalid"), e.join("\n")).ok();
            continue;
        }
    }

    Ok(FinalStats::default())
}

// This is a bit messy - I don't love using a Vec of Strings for the errors -
// but I wanted to capture the multi-error nature of the validation process
//
// TODO: Think of a better way to handle this
fn validate_folder(path: &Path) -> Result<(), Vec<String>> {
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
        Ok(())
    } else {
        Err(invalid_reasons)
    }
}
