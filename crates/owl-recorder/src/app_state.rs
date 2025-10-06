use std::{
    sync::{OnceLock, RwLock},
    time::{Duration, Instant},
};

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
    pub tx: CommandSender,
}

impl AppState {
    pub fn new(tx: CommandSender) -> Self {
        Self {
            state: RwLock::new(RecordingStatus::Stopped),
            config: RwLock::new(Config::load().expect("failed to init configs")),
            upload_stats: RwLock::new(UploadStats::load().expect("failed to init upload stats")),
            tx,
        }
    }
}

/// implementation for app state mpsc, allowing other threads to use a cloned tx
/// to send information back to the rx running on the main UI thread
pub enum Command {
    UpdateUserId(Result<String, String>),
    UpdateUploadProgress(Option<ProgressData>),
}

#[derive(Clone)]
pub struct CommandSender {
    tx: mpsc::Sender<Command>,
    pub ctx: OnceLock<egui::Context>,
}

impl CommandSender {
    pub fn try_send(&self, cmd: Command) -> Result<(), mpsc::error::TrySendError<Command>> {
        // if the UI is not focused the ctx never repaints so the message queue is never flushed. so if uploading
        // is occuring we have to force the app to repaint periodically, and pop messages from the message queue
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint_after(Duration::from_millis(10))
        }
        self.tx.try_send(cmd)
    }

    pub fn blocking_send(&self, cmd: Command) -> Result<(), mpsc::error::SendError<Command>> {
        let res = self.tx.blocking_send(cmd);
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint_after(Duration::from_millis(10))
        }
        res
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
    (
        CommandSender {
            tx,
            ctx: OnceLock::new(),
        },
        CommandReceiver { rx },
    )
}
