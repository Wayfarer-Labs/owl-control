use std::{
    sync::{OnceLock, RwLock, atomic::AtomicBool},
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
    pub async_request_tx: mpsc::Sender<AsyncRequest>,
    pub ui_update_tx: UiUpdateSender,
    pub is_currently_rebinding: AtomicBool,
}

impl AppState {
    pub fn new(async_request_tx: mpsc::Sender<AsyncRequest>, ui_update_tx: UiUpdateSender) -> Self {
        Self {
            state: RwLock::new(RecordingStatus::Stopped),
            config: RwLock::new(Config::load().expect("failed to init configs")),
            upload_stats: RwLock::new(UploadStats::load().expect("failed to init upload stats")),
            async_request_tx,
            ui_update_tx,
            is_currently_rebinding: AtomicBool::new(false),
        }
    }
}

/// A request for some async action to happen. Response will be delivered via [`UiUpdate`].
pub enum AsyncRequest {
    ValidateApiKey { api_key: String },
    UploadData,
    OpenDataDump,
    OpenLog,
}

/// A message sent to the UI thread, usually in response to some action taken in another thread
pub enum UiUpdate {
    /// Dummy update to force the UI to repaint
    ForceUpdate,
    UpdateUserId(Result<String, String>),
    UpdateUploadProgress(Option<ProgressData>),
    UploadFailed(String),
}

/// A sender for [`UiUpdate`] messages. Will automatically repaint the UI after sending a message.
#[derive(Clone)]
pub struct UiUpdateSender {
    tx: mpsc::Sender<UiUpdate>,
    pub ctx: OnceLock<egui::Context>,
}
impl UiUpdateSender {
    pub fn build(buffer: usize) -> (Self, tokio::sync::mpsc::Receiver<UiUpdate>) {
        let (tx, rx) = tokio::sync::mpsc::channel(buffer);
        (
            Self {
                tx,
                ctx: OnceLock::new(),
            },
            rx,
        )
    }

    pub fn try_send(&self, cmd: UiUpdate) -> Result<(), mpsc::error::TrySendError<UiUpdate>> {
        // if the UI is not focused the ctx never repaints so the message queue is never flushed. so if uploading
        // is occuring we have to force the app to repaint periodically, and pop messages from the message queue
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint_after(Duration::from_millis(10))
        }
        self.tx.try_send(cmd)
    }

    pub fn blocking_send(&self, cmd: UiUpdate) -> Result<(), mpsc::error::SendError<UiUpdate>> {
        let res = self.tx.blocking_send(cmd);
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint_after(Duration::from_millis(10))
        }
        res
    }

    pub async fn send(&self, cmd: UiUpdate) -> Result<(), mpsc::error::SendError<UiUpdate>> {
        let res = self.tx.send(cmd).await;
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint_after(Duration::from_millis(10))
        }
        res
    }
}
