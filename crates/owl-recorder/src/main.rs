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
mod overlay;
mod raw_input_debouncer;
mod recorder;
mod recording;
mod recording_thread;
mod ui;
mod upload_manager;

use std::{path::PathBuf, thread, time::Duration};

use clap::Parser;
use color_eyre::Result;

use crate::overlay::OverlayApp;

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

    let Args {
        recording_location,
        start_key,
        stop_key,
    } = Args::parse();

    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let app_state = Arc::new(app_state::AppState::new());

    // launch overlay on seperate thread so non-blocking
    thread::spawn({
        let app_state = app_state.clone();
        move || {
            egui_overlay::start(OverlayApp::new(app_state));
        }
    });

    // launch recorder on seperate thread so non-blocking
    thread::spawn({
        let app_state = app_state.clone();
        move || {
            recording_thread::run(app_state, start_key, stop_key, recording_location).unwrap();
        }
    });

    ui::start(app_state)
}
