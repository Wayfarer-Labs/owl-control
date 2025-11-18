#![cfg_attr(
    all(target_os = "windows", not(debug_assertions),),
    windows_subsystem = "windows"
)]

mod api;
mod app_state;
mod assets;
mod config;
mod output_types;
mod record;
mod system;
mod tokio_thread;
mod ui;
mod upload;
mod util;
mod validation;

use color_eyre::Result;
use egui_wgpu::wgpu;
use tracing_subscriber::{Layer, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use std::sync::Arc;

use crate::system::ensure_single_instance::ensure_single_instance;

fn main() -> Result<()> {
    // Set up logging, including to file
    let log_path = config::get_persistent_dir()?.join("owl-control-debug.log");
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let mut env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into());
    for crate_name in [
        "wgpu_hal",
        "symphonia_core",
        "symphonia_bundle_mp3",
        "egui_window_glfw_passthrough",
        "egui_overlay",
        "egui_render_glow",
    ] {
        env_filter = env_filter.add_directive(format!("{crate_name}=warn").parse().unwrap());
    }

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_filter(env_filter.clone()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(log_file)
                .with_ansi(false)
                .with_filter(env_filter),
        )
        .init();

    tracing::info!(
        "OWL Control v{} ({})",
        env!("CARGO_PKG_VERSION"),
        git_version::git_version!()
    );

    color_eyre::install()?;

    // Ensure only one instance is running
    ensure_single_instance()?;

    let wgpu_instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter_infos = wgpu_instance
        .enumerate_adapters(wgpu::Backends::DX12)
        .into_iter()
        .map(|a| a.get_info())
        .collect::<Vec<_>>();
    tracing::info!("Available adapters: {adapter_infos:?}");

    let (async_request_tx, async_request_rx) = tokio::sync::mpsc::channel(200);
    let (ui_update_tx, ui_update_rx) = app_state::UiUpdateSender::build();
    // A broadcast channel is used as older entries will be dropped if the channel is full.
    let (ui_update_unreliable_tx, ui_update_unreliable_rx) = tokio::sync::broadcast::channel(200);
    let app_state = Arc::new(app_state::AppState::new(
        async_request_tx,
        ui_update_tx,
        ui_update_unreliable_tx,
        adapter_infos,
    ));

    // launch tokio (which hosts the recorder) on seperate thread
    let (stopped_tx, stopped_rx) = tokio::sync::broadcast::channel(1);
    let tokio_thread = std::thread::spawn({
        let app_state = app_state.clone();
        let stopped_tx = stopped_tx.clone();
        let stopped_rx = stopped_rx.resubscribe();
        move || {
            let result =
                tokio_thread::run(app_state.clone(), log_path, async_request_rx, stopped_rx);

            if let Err(e) = result {
                tracing::error!("Error in tokio thread: {e}");
            }

            // note: this is usually the ctrl+c shut down path, but its a known bug that if the app is minimized to tray,
            // killing it via ctrl+c will not kill the app immediately, the MainApp will not receive the stop signal until
            // you click on the tray icon to re-open it, triggering the main loop repaint to run. Killing it via tray icon quit
            // works as we just force the app to reopen for a split second to trigger refresh, but no clean way to implement this
            // from here, so we just have to live with it for now.
            tracing::info!("Tokio thread shut down, propagating stop signal");
            match stopped_tx.send(()) {
                Ok(_) => {}
                Err(e) => tracing::error!("Failed to send stop signal: {}", e),
            };
            app_state
                .ui_update_tx
                .send(app_state::UiUpdate::ForceUpdate)
                .ok();
            tracing::info!("Tokio thread shut down complete");
        }
    });

    let app_state_for_shutdown = app_state.clone();
    ui::start(
        wgpu_instance,
        app_state,
        ui_update_rx,
        ui_update_unreliable_rx,
        stopped_tx,
        stopped_rx,
    )?;
    tracing::info!("UI thread shut down, joining tokio thread");
    tokio_thread.join().unwrap();
    tracing::info!("Tokio thread joined, saving final play time state");

    // Save final play time state before shutdown
    {
        let play_time = app_state_for_shutdown.play_time_state.read().unwrap();
        if let Err(e) = play_time.save() {
            tracing::error!("Failed to save play time state on shutdown: {}", e);
        }
    }

    tracing::info!("Shutting down");

    Ok(())
}
