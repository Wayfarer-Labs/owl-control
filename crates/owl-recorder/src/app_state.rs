use std::sync::Arc;
use std::{sync::RwLock, time::Instant};

use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::config::Config;
use crate::upload_manager::ProgressData;

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
    pub state: Arc<RwLock<RecordingStatus>>,
    pub config: Arc<RwLock<Config>>,
    pub tx: CommandSender,
}

impl AppState {
    pub fn new(tx: CommandSender) -> Self {
        Self {
            state: Arc::new(RwLock::new(RecordingStatus::Stopped)),
            config: Arc::new(RwLock::new(Config::new().expect("failed to init configs"))),
            tx,
        }
    }
}

/// implementation for app state mpsc, allowing other threads to use a cloned tx
/// to send information back to the rx running on the main UI thread
pub enum Command {
    UpdateUserID(String),
    UpdateUploadProgress(ProgressData),
}

#[derive(Clone)]
pub struct CommandSender {
    tx: Sender<Command>,
}

impl CommandSender {
    pub fn try_send(&self, cmd: Command) -> Result<(), mpsc::error::TrySendError<Command>> {
        self.tx.try_send(cmd)
    }
}

pub struct CommandReceiver {
    rx: Receiver<Command>,
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
