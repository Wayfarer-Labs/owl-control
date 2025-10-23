use std::{path::Path, str::FromStr as _};

use color_eyre::eyre::{self, Context as _};
use serde::{Deserialize, Serialize};

use crate::output_types::{InputEvent, InputEventType, Metadata};

pub mod gamepad;
pub mod keyboard;
pub mod mouse;
pub mod video;

#[derive(Serialize, Deserialize, Clone)]
pub struct InputStats {
    #[serde(flatten)]
    pub keyboard_stats: keyboard::KeyboardOutputStats,
    #[serde(flatten)]
    pub mouse_stats: mouse::MouseOutputStats,
    #[serde(flatten)]
    pub gamepad_stats: gamepad::GamepadOutputStats,
}

struct ValidationInput<'a> {
    pub start_time: f64,
    pub filtered_events: &'a [InputEvent],
    pub duration_minutes: f64,
}

pub fn for_recording(
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
