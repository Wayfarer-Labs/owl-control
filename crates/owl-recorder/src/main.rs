#![windows_subsystem = "windows"]

mod app_state;
mod auth_service;
mod config;
mod find_game;
mod hardware_id;
mod hardware_specs;
mod idle;
mod input_recorder;
mod keycode;
mod obs_embedded_recorder;
mod obs_socket_recorder;
mod output_types;
mod raw_input_debouncer;
mod recorder;
mod recording;
mod tokio_thread;
mod ui;
mod upload;

use std::{path::PathBuf, time::Duration};

use clap::Parser;
use color_eyre::Result;
use tracing_subscriber::{Layer, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use std::sync::Arc;

const MAX_IDLE_DURATION: Duration = Duration::from_secs(90);

fn main() -> Result<()> {
    #[derive(Parser, Debug)]
    #[command(version, about)]
    struct Args {
        #[arg(long, default_value = "./data_dump/games")]
        recording_location: PathBuf,

        #[arg(long, default_value = "F4")]
        start_key: String,

        #[arg(long, default_value = "F5")]
        stop_key: String,
    }

    // Set up logging, including to file
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("owl-control-debug.log"))?;

    let env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into());

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .pretty()
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
        "OWL Recorder v{} ({})",
        env!("CARGO_PKG_VERSION"),
        git_version::git_version!()
    );

    let Args {
        recording_location,
        start_key,
        stop_key,
    } = Args::parse();

    color_eyre::install()?;

    let (async_request_tx, async_request_rx) = tokio::sync::mpsc::channel(16);
    let (ui_update_tx, ui_update_rx) = app_state::UiUpdateSender::build(16);
    let app_state = Arc::new(app_state::AppState::new(async_request_tx, ui_update_tx));

    // launch tokio (which hosts the recorder) on seperate thread
    let (stopped_tx, stopped_rx) = tokio::sync::broadcast::channel(1);
    let tokio_thread = std::thread::spawn({
        let app_state = app_state.clone();
        let stopped_tx = stopped_tx.clone();
        let stopped_rx = stopped_rx.resubscribe();
        move || {
            tokio_thread::run(
                app_state.clone(),
                start_key,
                stop_key,
                recording_location,
                async_request_rx,
                stopped_rx,
            )
            .unwrap();

            // note: this is usually the ctrl+c shut down path, but its a known bug that if the app is minimized to tray,
            // killing it via ctrl+c will not kill the app immediately, the MainApp will not receive the stop signal until
            // you click on the tray icon to re-open it, triggering the main loop repaint to run. Killing it via tray icon quit
            // works as we just force the app to reopen for a split second to trigger refresh, but no clean way to implement this
            // from here, so we just have to live with it for now.
            tracing::info!("Tokio thread shut down, propagating stop signal");
            stopped_tx.send(()).unwrap();
            app_state
                .ui_update_tx
                .blocking_send(app_state::UiUpdate::ForceUpdate)
                .ok();
        }
    });

    ui::start(app_state, ui_update_rx, stopped_tx, stopped_rx)?;
    tracing::info!("UI thread shut down, joining tokio thread");
    tokio_thread.join().unwrap();
    tracing::info!("Tokio thread joined, shutting down");

    Ok(())
}
