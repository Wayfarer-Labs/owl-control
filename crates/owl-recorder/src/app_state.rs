use std::{
    sync::{RwLock, atomic::AtomicU8},
    time::Instant,
};

use rodio::{OutputStream, Sink};

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
    /// for honking
    pub sink: Sink,
    /// stream handle needs to stay alive for the sink to play audio
    pub _stream_handle: OutputStream,
}

impl AppState {
    pub fn new() -> Self {
        let stream_handle =
            rodio::OutputStreamBuilder::open_default_stream().expect("open default audio stream");
        let sink = Sink::connect_new(stream_handle.mixer());
        Self {
            state: RwLock::new(RecordingStatus::Stopped),
            opacity: AtomicU8::new(85),
            config: RwLock::new(Config::new().expect("failed to init configs")),
            sink,
            _stream_handle: stream_handle,
        }
    }
}
