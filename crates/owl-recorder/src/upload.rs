use std::{
    io::BufRead as _,
    process::{Command, Stdio},
    sync::Arc,
};

use serde::Deserialize;

use crate::app_state::{self, AppState};

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

pub fn start(app_state: Arc<AppState>, api_token: &str, unreliable_connection: bool) -> bool {
    let tx = app_state.tx.clone();
    tracing::info!("Starting upload bridge module from vg_control package");
    let _ = tx.try_send(app_state::Command::UpdateUploadProgress(Some(
        ProgressData::default(),
    )));

    let root_dir = {
        if cfg!(debug_assertions) {
            // Development mode
            std::env::current_dir().unwrap()
        } else {
            // TODO: Release mode. @philpax erm how?
            std::env::current_dir().unwrap()
        }
    };

    let mut args = vec![
        "run",
        "-m",
        "vg_control.upload_bridge",
        "--api-token",
        api_token,
    ];
    if unreliable_connection {
        args.push("--unreliable-connections");
    }

    let mut child = match Command::new("uv")
        .args(args)
        .current_dir(root_dir)
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            tracing::error!("Error starting upload bridge: {}", error);
            return false;
        }
    };

    let Some(stderr) = child.stderr.take() else {
        tracing::error!("Error getting stderr from upload bridge");
        return false;
    };
    let reader = std::io::BufReader::new(stderr);
    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                tracing::error!("Error reading line from upload bridge: {e}");
                continue;
            }
        };

        if let Some(data) = line.strip_prefix("PROGRESS: ") {
            let data = match serde_json::from_str::<ProgressData>(data) {
                Ok(data) => data,
                Err(e) => {
                    tracing::error!("Error parsing progress data: {e}");
                    continue;
                }
            };

            let _ = tx.try_send(app_state::Command::UpdateUploadProgress(Some(data)));
        } else if let Some(data) = line.strip_prefix("FINAL_STATS: ") {
            let data = match serde_json::from_str::<FinalStats>(data) {
                Ok(data) => data,
                Err(e) => {
                    tracing::error!("Error parsing final stats data: {e}");
                    continue;
                }
            };

            if let Err(e) = app_state.upload_stats.write().unwrap().update(
                data.total_duration_uploaded,
                data.total_files_uploaded,
                data.total_bytes_uploaded,
            ) {
                tracing::error!("Error updating upload stats: {e}");
            }
        }
    }
    // force the thread to block until the update goes through, in case buffer is full
    let _ = tx.blocking_send(app_state::Command::UpdateUploadProgress(None));

    true
}
