use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::output_types::InputEventType;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GamepadOutputStats {
    gamepad_button_apm: f64,
    gamepad_unique_buttons: u64,
    gamepad_button_diversity: f64,
    gamepad_total_events: u64,
    gamepad_axis_activity: f64,
    gamepad_axis_movement: f64,
    gamepad_max_axis_movement: f64,
}
impl From<GamepadStats> for GamepadOutputStats {
    fn from(stats: GamepadStats) -> Self {
        Self {
            gamepad_button_apm: stats.button_apm,
            gamepad_unique_buttons: stats.unique_buttons,
            gamepad_button_diversity: stats.button_diversity,
            gamepad_total_events: stats.total_button_events,
            gamepad_axis_activity: stats.axis_activity,
            gamepad_axis_movement: stats.axis_movement,
            gamepad_max_axis_movement: stats.max_axis_movement,
        }
    }
}

pub(super) fn validate(input: &super::ValidationInput) -> (GamepadOutputStats, Vec<String>) {
    let mut invalid_reasons = vec![];
    let stats = get_stats(input);

    // Only invalidate if BOTH button and axis activity are too low
    // This allows axis-only runs and button-only runs to be valid
    if stats.button_apm < 2.0 && stats.axis_movement < 0.01 {
        invalid_reasons.push(format!(
            "Gamepad activity too low: button APM {} and axis movement {}",
            stats.button_apm, stats.axis_movement
        ));
    }
    if stats.max_axis_movement > 2.0 {
        invalid_reasons.push(format!(
            "Gamepad axis movement too high: {}",
            stats.max_axis_movement
        ));
    }

    (GamepadOutputStats::from(stats), invalid_reasons)
}

struct GamepadStats {
    pub button_apm: f64,
    pub unique_buttons: u64,
    pub button_diversity: f64,
    pub total_button_events: u64,
    pub axis_activity: f64,
    pub axis_movement: f64,
    pub max_axis_movement: f64,
}
fn get_stats(input: &super::ValidationInput) -> GamepadStats {
    // Filter for gamepad events only
    let gamepad_events: Vec<_> = input
        .filtered_events
        .iter()
        .filter(|event| {
            matches!(
                event.event,
                InputEventType::GamepadButton { .. }
                    | InputEventType::GamepadButtonValue { .. }
                    | InputEventType::GamepadAxis { .. }
            )
        })
        .collect();

    let mut button_apm = 0.0;
    let mut unique_buttons = 0;
    let mut diversity = 0.0;
    let mut total_button_events = 0;
    let mut axis_activity = 0.0;
    let mut axis_movement = 0.0;
    let mut max_axis_movement = 0.0;

    // Separate different types of gamepad events
    let button_events: Vec<_> = gamepad_events
        .iter()
        .filter(|event| matches!(event.event, InputEventType::GamepadButton { .. }))
        .collect();

    let button_value_events: Vec<_> = gamepad_events
        .iter()
        .filter(|event| matches!(event.event, InputEventType::GamepadButtonValue { .. }))
        .collect();

    let axis_events: Vec<_> = gamepad_events
        .iter()
        .filter(|event| matches!(event.event, InputEventType::GamepadAxis { .. }))
        .collect();

    // Process button events
    if !button_events.is_empty() {
        // Count button presses
        let button_presses = button_events
            .iter()
            .filter(|event| {
                if let InputEventType::GamepadButton { pressed, .. } = event.event {
                    pressed
                } else {
                    false
                }
            })
            .count();

        button_apm = if input.duration_minutes > 0.0 {
            button_presses as f64 / input.duration_minutes
        } else {
            0.0
        };

        // Get unique buttons from pressed events
        let pressed_buttons: std::collections::HashSet<u16> = button_events
            .iter()
            .filter_map(|event| {
                if let InputEventType::GamepadButton { button, pressed } = event.event
                    && pressed
                {
                    Some(button)
                } else {
                    None
                }
            })
            .collect();

        unique_buttons = pressed_buttons.len() as u64;

        // Calculate button diversity using normalized entropy
        if !pressed_buttons.is_empty() {
            let mut button_counts: HashMap<u16, u64> = HashMap::new();
            for event in &button_events {
                if let InputEventType::GamepadButton { button, pressed } = event.event
                    && pressed
                {
                    *button_counts.entry(button).or_insert(0) += 1;
                }
            }

            let total_presses: u64 = button_counts.values().sum();
            if total_presses > 0 {
                let entropy: f64 = button_counts
                    .values()
                    .map(|&count| {
                        let prob = count as f64 / total_presses as f64;
                        if prob > 0.0 { -prob * prob.log2() } else { 0.0 }
                    })
                    .sum();

                let max_entropy = (button_counts.len() as f64).log2();
                diversity = if max_entropy > 0.0 {
                    entropy / max_entropy
                } else {
                    0.0
                };
            }
        }

        total_button_events = button_events.len() as u64;
    }

    // Process axis events
    if !axis_events.is_empty() {
        let axis_values: Vec<f64> = axis_events
            .iter()
            .filter_map(|event| {
                if let InputEventType::GamepadAxis { value, .. } = event.event {
                    Some(value as f64)
                } else {
                    None
                }
            })
            .collect();

        if !axis_values.is_empty() {
            // Calculate old axis activity metric (mean of absolute values) for compatibility
            axis_activity =
                axis_values.iter().map(|&val| val.abs()).sum::<f64>() / axis_values.len() as f64;

            max_axis_movement = axis_values
                .iter()
                .map(|&val| val.abs())
                .fold(0.0_f64, |acc, val| acc.max(val));

            // Calculate new axis movement metric (average absolute change between consecutive values)
            if axis_values.len() > 1 {
                let total_change: f64 = axis_values.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
                axis_movement = total_change / (axis_values.len() - 1) as f64;
            }
        }
    }

    // Process button value events (analog buttons like triggers)
    if !button_value_events.is_empty() {
        // Add button value events to total button events
        total_button_events += button_value_events.len() as u64;
    }

    GamepadStats {
        button_apm,
        unique_buttons,
        button_diversity: diversity,
        total_button_events,
        axis_activity,
        axis_movement,
        max_axis_movement,
    }
}
