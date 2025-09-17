use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

// Simple atomic flag for async version
static IS_RUNNING: AtomicBool = AtomicBool::new(false);

pub async fn start_upload_bridge_async(api_token: &str) -> bool {
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
        .args(&[
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

    // Wait for process completion
    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) => {
                if let Some(code) = status.code() {
                    tracing::info!("Upload bridge process exited with code {}", code);
                } else {
                    tracing::info!("Upload bridge process terminated by signal");
                }
            }
            Err(e) => tracing::error!("Error waiting for process: {}", e),
        }
        // Reset running flag when process ends
        IS_RUNNING.store(false, Ordering::SeqCst);
    });

    true
}

/// Check if async upload bridge is currently running (not async!)
pub fn is_upload_bridge_running_async() -> bool {
    IS_RUNNING.load(Ordering::SeqCst)
}

/// Force reset the async upload bridge flag (not async!)
pub fn reset_upload_bridge_flag_async() {
    IS_RUNNING.store(false, Ordering::SeqCst);
    tracing::info!("Async upload bridge running flag has been reset");
}
