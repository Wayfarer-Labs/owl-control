use std::collections::HashMap;

use crate::output_types::InputEventType;
use constants::FPS;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct MouseOutputStats {
    mouse_movement_std: f64,
    mouse_x_std: f64,
    mouse_y_std: f64,
    mouse_max_movement: f64,
    mouse_max_x: f64,
    mouse_max_y: f64,
}
impl From<MouseStats> for MouseOutputStats {
    fn from(stats: MouseStats) -> Self {
        Self {
            mouse_movement_std: stats.overall_std,
            mouse_x_std: stats.x_std,
            mouse_y_std: stats.y_std,
            mouse_max_movement: stats.overall_max,
            mouse_max_x: stats.max_x,
            mouse_max_y: stats.max_y,
        }
    }
}

pub(super) fn validate(input: &super::ValidationInput) -> (MouseOutputStats, Vec<String>) {
    let mut invalid_reasons = vec![];
    let stats = get_stats(input);

    if stats.overall_max < 0.05 {
        invalid_reasons.push(format!("Mouse movement too small: {}", stats.overall_max));
    }
    if stats.overall_max > 10_000.0 {
        invalid_reasons.push(format!("Mouse movement too large: {}", stats.overall_max));
    }

    (MouseOutputStats::from(stats), invalid_reasons)
}

struct MouseStats {
    pub overall_std: f64,
    pub x_std: f64,
    pub y_std: f64,
    pub overall_max: f64,
    pub max_x: f64,
    pub max_y: f64,
}

fn get_stats(input: &super::ValidationInput) -> MouseStats {
    let frame_duration = 1.0 / FPS as f64;

    // Extract mouse movement data
    let mouse_moves: Vec<_> = input
        .filtered_events
        .iter()
        .filter_map(|event| {
            if let InputEventType::MouseMove { dx, dy } = event.event {
                Some((event.timestamp, dx, dy))
            } else {
                None
            }
        })
        .collect();

    let mut overall_std = 0.0;
    let mut x_std = 0.0;
    let mut y_std = 0.0;
    let mut overall_max = 0.0;
    let mut max_x = 0.0;
    let mut max_y = 0.0;

    // Check if we have any mouse movement data
    if !mouse_moves.is_empty() {
        #[derive(Default)]
        struct Frame {
            dx: f64,
            dy: f64,
            count: usize,
        }

        // Group movements by frame
        let mut frame_data: HashMap<i32, Frame> = HashMap::new();

        for (timestamp, dx, dy) in mouse_moves {
            let frame = ((timestamp - input.start_time) / frame_duration) as i32;
            let entry = frame_data.entry(frame).or_default();
            entry.dx += dx as f64;
            entry.dy += dy as f64;
            entry.count += 1;
        }

        // Calculate average movement per frame
        let mut frame_movements: Vec<(f64, f64)> = Vec::new();
        for (_, Frame { dx, dy, count }) in frame_data {
            let avg_dx = dx / count as f64;
            let avg_dy = dy / count as f64;
            frame_movements.push((avg_dx, avg_dy));
        }

        if !frame_movements.is_empty() {
            // Calculate movement statistics
            let overall_magnitudes: Vec<f64> = frame_movements
                .iter()
                .map(|(dx, dy)| (dx * dx + dy * dy).sqrt())
                .collect();

            let x_movements: Vec<f64> = frame_movements.iter().map(|(dx, _)| *dx).collect();
            let y_movements: Vec<f64> = frame_movements.iter().map(|(_, dy)| *dy).collect();

            // Calculate standard deviations
            overall_std = calculate_std(&overall_magnitudes);
            x_std = calculate_std(&x_movements);
            y_std = calculate_std(&y_movements);

            // Calculate maximum values
            overall_max = overall_magnitudes
                .iter()
                .fold(0.0_f64, |acc, &val| acc.max(val));
            max_x = x_movements
                .iter()
                .map(|&val| val.abs())
                .fold(0.0_f64, |acc, val| acc.max(val));
            max_y = y_movements
                .iter()
                .map(|&val| val.abs())
                .fold(0.0_f64, |acc, val| acc.max(val));
        }
    }

    MouseStats {
        overall_std,
        x_std,
        y_std,
        overall_max,
        max_x,
        max_y,
    }
}

fn calculate_std(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance =
        values.iter().map(|&val| (val - mean).powi(2)).sum::<f64>() / values.len() as f64;

    variance.sqrt()
}
