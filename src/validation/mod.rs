use std::{
    path::{Path, PathBuf},
    str::FromStr as _,
};

use color_eyre::eyre::{self, Context as _};
use serde::{Deserialize, Serialize};

use crate::output_types::{InputEvent, InputEventType, Metadata};

pub mod gamepad;
pub mod keyboard;
pub mod mouse;
pub mod video;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InputStats {
    #[serde(flatten)]
    pub keyboard_stats: keyboard::KeyboardOutputStats,
    #[serde(flatten)]
    pub mouse_stats: mouse::MouseOutputStats,
    #[serde(flatten)]
    pub gamepad_stats: gamepad::GamepadOutputStats,
}

#[derive(Clone)]
pub struct ValidationResult {
    pub mp4_path: PathBuf,
    pub csv_path: PathBuf,
    pub meta_path: PathBuf,
    pub metadata: Metadata,
}

/// Validates the given recording folder, creating a .invalid file if validation fails.
pub fn validate_folder(path: &Path) -> eyre::Result<ValidationResult> {
    match validate_folder_impl(path) {
        Ok(result) => Ok(result),
        Err(e) => {
            std::fs::write(
                path.join(constants::filename::recording::INVALID),
                e.join("\n"),
            )
            .ok();
            eyre::bail!("Validation failures: {}", e.join("\n"));
        }
    }
}

// This is a bit messy - I don't love using a Vec of Strings for the errors -
// but I wanted to capture the multi-error nature of the validation process
//
// TODO: Think of a better way to handle this
fn validate_folder_impl(path: &Path) -> Result<ValidationResult, Vec<String>> {
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

    let (input_stats, mut invalid_reasons) = validate_files(&metadata, &mp4_path, &csv_path)
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

struct ValidationInput<'a> {
    pub start_time: f64,
    pub filtered_events: &'a [InputEvent],
    pub duration_minutes: f64,
}
fn validate_files(
    metadata: &Metadata,
    mp4_path: &Path,
    csv_path: &Path,
) -> eyre::Result<(InputStats, Vec<String>)> {
    let events = std::fs::read_to_string(csv_path)
        .with_context(|| format!("Error reading CSV file at {csv_path:?})"))?
        .lines()
        .skip(1)
        .map(InputEvent::from_str)
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("Error parsing CSV file at {csv_path:?}"))?;

    let start_time = events
        .iter()
        .find(|event| matches!(event.event, InputEventType::Start { .. }))
        .map(|event| event.timestamp)
        .unwrap_or(0.0);

    let end_time = events
        .iter()
        .find(|event| matches!(event.event, InputEventType::End { .. }))
        .or_else(|| events.last())
        .map(|event| event.timestamp)
        .unwrap_or(0.0);

    let filtered_events: Vec<_> = events
        .iter()
        .filter(|event| event.timestamp >= start_time && event.timestamp <= end_time)
        .cloned()
        .collect();

    let input = ValidationInput {
        start_time,
        filtered_events: &filtered_events,
        duration_minutes: end_time - start_time,
    };

    let mut invalid_reasons = video::validate(mp4_path, metadata);
    let (keyboard_stats, keyboard_invalid_reasons) = keyboard::validate(&input);
    let (mouse_stats, mouse_invalid_reasons) = mouse::validate(&input);
    let (gamepad_stats, gamepad_invalid_reasons) = gamepad::validate(&input);

    // Only invalidate if all three input types are invalid
    if !(keyboard_invalid_reasons.is_empty()
        || mouse_invalid_reasons.is_empty()
        || gamepad_invalid_reasons.is_empty())
    {
        invalid_reasons.extend(keyboard_invalid_reasons);
        invalid_reasons.extend(mouse_invalid_reasons);
        invalid_reasons.extend(gamepad_invalid_reasons);
    }

    Ok((
        InputStats {
            keyboard_stats,
            mouse_stats,
            gamepad_stats,
        },
        invalid_reasons,
    ))
}
