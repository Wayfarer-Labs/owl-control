use std::{
    sync::{RwLock, atomic::AtomicU8},
    time::Instant,
};

use crate::config::Config;

#[derive(Clone, PartialEq)]
pub enum RecordingStatus {
    Stopped,
    Recording {
        start_time: Instant,
        game_exe: String,
    },
    Paused,
}

pub struct AppState {
    /// holds the current state of recording, recorder <-> overlay
    pub state: RwLock<RecordingStatus>,
    /// setting for opacity of overlay, main app <-> overlay
    pub opacity: AtomicU8,
    pub config: RwLock<Config>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(RecordingStatus::Stopped),
            opacity: AtomicU8::new(85),
            config: RwLock::new(Config::new().expect("failed to init configs")),
        }
    }
}
