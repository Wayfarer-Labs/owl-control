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
mod raw_input_debouncer;
mod recorder;
mod recording;
mod recording_thread;
mod ui;
mod upload;

use std::{path::PathBuf, time::Duration};

use clap::Parser;
use color_eyre::Result;
use tracing_subscriber::{Layer, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use std::sync::Arc;

const MAX_IDLE_DURATION: Duration = Duration::from_secs(90);
const MAX_RECORDING_DURATION: Duration = Duration::from_secs(10 * 60);

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

    let (ui_update_tx, ui_update_rx) = app_state::UiUpdateSender::build(16);
    let app_state = Arc::new(app_state::AppState::new(ui_update_tx));

    // launch recorder on seperate thread so non-blocking
    std::thread::spawn({
        let app_state = app_state.clone();
        move || {
            recording_thread::run(app_state, start_key, stop_key, recording_location).unwrap();
        }
    });

    ui::start(app_state, ui_update_rx)
}
