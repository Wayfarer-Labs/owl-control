use serde::{Deserialize, Serialize};

use crate::{output_types::InputEventType, system::keycode::lookup_keycode};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KeyboardOutputStats {
    wasd_apm: f64,
    unique_keys: u64,
    button_diversity: f64,
    total_keyboard_events: u64,
}
impl From<KeyboardStats> for KeyboardOutputStats {
    fn from(stats: KeyboardStats) -> Self {
        Self {
            wasd_apm: stats.wasd_apm,
            unique_keys: stats.unique_keys,
            button_diversity: stats.button_diversity,
            total_keyboard_events: stats.total_keyboard_events,
        }
    }
}

pub(super) fn validate(input: &super::ValidationInput) -> (KeyboardOutputStats, Vec<String>) {
    let mut invalid_reasons = vec![];
    let stats = get_stats(input);

    if stats.wasd_apm < 10.0 {
        invalid_reasons.push(format!(
            "WASD actions per minute too low: {}",
            stats.wasd_apm
        ));
    }
    if stats.total_keyboard_events < 50 {
        invalid_reasons.push(format!(
            "Too few keyboard events: {}",
            stats.total_keyboard_events
        ));
    }

    (KeyboardOutputStats::from(stats), invalid_reasons)
}

struct KeyboardStats {
    pub wasd_apm: f64,
    pub unique_keys: u64,
    pub button_diversity: f64,
    pub total_keyboard_events: u64,
}
fn get_stats(input: &super::ValidationInput) -> KeyboardStats {
    // Get WASD keycodes
    let wasd_codes: Vec<u16> = ["W", "A", "S", "D"]
        .iter()
        .filter_map(|&key| lookup_keycode(key))
        .collect();

    // Filter for keyboard events only
    let keyboard_events: Vec<_> = input
        .filtered_events
        .iter()
        .filter(|event| matches!(event.event, InputEventType::Keyboard { .. }))
        .collect();

    let mut wasd_apm = 0.0;
    let mut unique_keys = 0;
    let mut diversity = 0.0;

    if !keyboard_events.is_empty() {
        // Count WASD presses
        let wasd_presses = keyboard_events
            .iter()
            .filter(|event| {
                if let InputEventType::Keyboard { key, pressed } = event.event {
                    pressed && wasd_codes.contains(&key)
                } else {
                    false
                }
            })
            .count();

        wasd_apm = if input.duration_minutes > 0.0 {
            wasd_presses as f64 / input.duration_minutes
        } else {
            0.0
        };

        // Get unique keys from pressed events
        let pressed_keys: HashSet<u16> = keyboard_events
            .iter()
            .filter_map(|event| {
                if let InputEventType::Keyboard { key, pressed } = event.event
                    && pressed
                {
                    Some(key)
                } else {
                    None
                }
            })
            .collect();

        unique_keys = pressed_keys.len() as u64;

        // Calculate button diversity using normalized entropy
        if !pressed_keys.is_empty() {
            let mut key_counts: HashMap<u16, u64> = HashMap::new();
            for event in &keyboard_events {
                if let InputEventType::Keyboard { key, pressed } = event.event
                    && pressed
                {
                    *key_counts.entry(key).or_insert(0) += 1;
                }
            }

            let total_presses: u64 = key_counts.values().sum();
            if total_presses > 0 {
                let entropy: f64 = key_counts
                    .values()
                    .map(|&count| {
                        let prob = count as f64 / total_presses as f64;
                        if prob > 0.0 { -prob * prob.log2() } else { 0.0 }
                    })
                    .sum();

                let max_entropy = (key_counts.len() as f64).log2();
                diversity = if max_entropy > 0.0 {
                    entropy / max_entropy
                } else {
                    0.0
                };
            }
        }
    }

    KeyboardStats {
        wasd_apm,
        unique_keys,
        button_diversity: diversity,
        total_keyboard_events: keyboard_events.len() as u64,
    }
}
