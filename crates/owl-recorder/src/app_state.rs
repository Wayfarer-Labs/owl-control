use std::{sync::RwLock, time::Instant};

use tokio::sync::mpsc;

use crate::{
    config::{Config, UploadStats},
    upload::ProgressData,
};

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
    pub config: RwLock<Config>,
    pub upload_stats: RwLock<UploadStats>,
    pub upload_progress: RwLock<Option<ProgressData>>,
    pub tx: CommandSender,
}

impl AppState {
    pub fn new(tx: CommandSender) -> Self {
        Self {
            state: RwLock::new(RecordingStatus::Stopped),
            config: RwLock::new(Config::load().expect("failed to init configs")),
            upload_stats: RwLock::new(UploadStats::load().expect("failed to init upload stats")),
            upload_progress: RwLock::new(None),
            tx,
        }
    }
}

/// implementation for app state mpsc, allowing other threads to use a cloned tx
/// to send information back to the rx running on the main UI thread
pub enum Command {
    UpdateUserID(String),
}

#[derive(Clone)]
pub struct CommandSender {
    tx: mpsc::Sender<Command>,
}

impl CommandSender {
    pub fn try_send(&self, cmd: Command) -> Result<(), mpsc::error::TrySendError<Command>> {
        self.tx.try_send(cmd)
    }
}

pub struct CommandReceiver {
    rx: mpsc::Receiver<Command>,
}

impl CommandReceiver {
    pub fn try_recv(&mut self) -> Result<Command, tokio::sync::mpsc::error::TryRecvError> {
        self.rx.try_recv()
    }
}

// Factory function to create the shared channel
pub fn command_channel(buffer: usize) -> (CommandSender, CommandReceiver) {
    let (tx, rx) = mpsc::channel(buffer);
    (CommandSender { tx }, CommandReceiver { rx })
}
