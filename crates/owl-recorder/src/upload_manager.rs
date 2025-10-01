use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::{env, fs, thread};

use serde::Deserialize;

static IS_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize)]
pub struct ProgressData {
    phase: Option<String>,
    bytes_uploaded: Option<u64>,
    total_bytes: Option<u64>,
    speed_mbps: Option<f64>,
    eta_seconds: Option<f64>,
    percent: Option<f64>,
    action: Option<String>,
}

pub fn start_upload_bridge(api_token: &str) -> bool {
    let progress_filepath: PathBuf = env::temp_dir().join("owl-control-upload-progress.json");
    // Check if already running
    if IS_RUNNING.load(Ordering::SeqCst) {
        tracing::info!("Upload bridge is already running, skipping...");
        return true;
    }

    // Set running flag
    IS_RUNNING.store(true, Ordering::SeqCst);

    tracing::info!("Starting upload bridge module from vg_control package");

    let root_dir = {
        if cfg!(debug_assertions) {
            // Development mode
            std::env::current_dir().unwrap()
        } else {
            // TODO: Release mode. @philpax erm how?
            std::env::current_dir().unwrap()
        }
    };

    let mut child = match Command::new("uv")
        .args([
            "run",
            "-m",
            "vg_control.upload_bridge",
            "--api-token",
            api_token,
        ])
        .current_dir(root_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            tracing::error!("Error starting upload bridge: {}", error);
            IS_RUNNING.store(false, Ordering::SeqCst); // Reset flag on error
            return false;
        }
    };

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // upload is complete, exit loop
                if let Some(code) = status.code() {
                    tracing::info!("Upload bridge process exited with code {}", code);
                } else {
                    tracing::info!("Upload bridge process terminated by signal");
                }
                break;
            }
            Ok(None) => {
                // Still uploading, continue to update progress
                if progress_filepath.exists() {
                    match fs::read_to_string(&progress_filepath) {
                        Ok(content) => {
                            match serde_json::from_str::<ProgressData>(&content) {
                                Ok(progress_data) => {
                                    println!("{:#?}", progress_data);
                                    thread::sleep(Duration::from_millis(500));
                                    /*
                                    // Update progress state with file data
                                    if progress_data.phase.as_deref() == Some("upload") {
                                        progress_state.bytes_uploaded =
                                            progress_data.bytes_uploaded.unwrap_or(0);
                                        progress_state.total_bytes =
                                            progress_data.total_bytes.unwrap_or(0);

                                        // Format speed
                                        let speed_mbps = progress_data.speed_mbps.unwrap_or(0.0);
                                        progress_state.speed = if speed_mbps > 0.0 {
                                            format!("{:.1} MB/s", speed_mbps)
                                        } else {
                                            "0 MB/s".to_string()
                                        };

                                        // Format ETA
                                        let eta_seconds = progress_data.eta_seconds.unwrap_or(0.0);
                                        progress_state.eta =
                                            if eta_seconds > 0.0 && eta_seconds < 3600.0 {
                                                let minutes = (eta_seconds / 60.0).floor() as u32;
                                                let seconds = (eta_seconds % 60.0).floor() as u32;
                                                if minutes > 0 {
                                                    format!("{}m {}s", minutes, seconds)
                                                } else {
                                                    format!("{}s", seconds)
                                                }
                                            } else if eta_seconds > 0.0 {
                                                "Calculating...".to_string()
                                            } else {
                                                "Complete".to_string()
                                            };

                                        // Update current file status
                                        let percent =
                                            progress_data.percent.unwrap_or(0.0).round() as u32;
                                        progress_state.current_file = if progress_data
                                            .action
                                            .as_deref()
                                            == Some("complete")
                                        {
                                            "Upload complete!".to_string()
                                        } else {
                                            format!("Uploading... {}%", percent)
                                        };

                                        // Set file progress to 1 of 1 when uploading
                                        progress_state.total_files = 1;
                                        progress_state.uploaded_files = if progress_data
                                            .action
                                            .as_deref()
                                            == Some("complete")
                                        {
                                            1
                                        } else {
                                            0
                                        };
                                    }
                                    */
                                }
                                Err(e) => {
                                    eprintln!("Error parsing progress file: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error reading progress file: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                // Handle error and exit loop
                tracing::error!("Error waiting for process: {}", e);
                break;
            }
        }
    }
    // Reset running flag when process ends
    IS_RUNNING.store(false, Ordering::SeqCst);

    true
}

/// Check if upload bridge is currently running
pub fn is_upload_bridge_running() -> bool {
    IS_RUNNING.load(Ordering::SeqCst)
}
