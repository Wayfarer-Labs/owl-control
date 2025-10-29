use std::{
    sync::{Arc, OnceLock, RwLock, atomic::AtomicBool},
    time::{Duration, Instant},
};

use constants::{encoding::VideoEncoderType, unsupported_games::UnsupportedGames};
use egui_wgpu::wgpu;
use tokio::sync::mpsc;

use crate::{api::UserUploads, config::Config, record::LocalRecording, upload::ProgressData};

pub struct AppState {
    /// holds the current state of recording, recorder <-> overlay
    pub state: RwLock<RecordingStatus>,
    pub config: RwLock<Config>,
    pub user_uploads: RwLock<Option<UserUploads>>,
    pub local_recordings: RwLock<Vec<LocalRecording>>,
    pub async_request_tx: mpsc::Sender<AsyncRequest>,
    pub ui_update_tx: UiUpdateSender,
    pub adapter_infos: Vec<wgpu::AdapterInfo>,
    pub upload_cancel_flag: Arc<AtomicBool>,
    pub listening_for_new_hotkey: RwLock<ListeningForNewHotkey>,
    pub is_out_of_date: AtomicBool,
}

impl AppState {
    pub fn new(
        async_request_tx: mpsc::Sender<AsyncRequest>,
        ui_update_tx: UiUpdateSender,
        adapter_infos: Vec<wgpu::AdapterInfo>,
    ) -> Self {
        Self {
            state: RwLock::new(RecordingStatus::Stopped),
            config: RwLock::new(Config::load().expect("failed to init configs")),
            user_uploads: RwLock::new(None),
            local_recordings: RwLock::new(Vec::new()),
            async_request_tx,
            ui_update_tx,
            adapter_infos,
            upload_cancel_flag: Arc::new(AtomicBool::new(false)),
            listening_for_new_hotkey: RwLock::new(ListeningForNewHotkey::NotListening),
            is_out_of_date: AtomicBool::new(false),
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum RecordingStatus {
    Stopped,
    Recording {
        start_time: Instant,
        game_exe: String,
    },
    Paused,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ListeningForNewHotkey {
    NotListening,
    Listening {
        target: HotkeyRebindTarget,
    },
    Captured {
        target: HotkeyRebindTarget,
        key: u16,
    },
}
impl ListeningForNewHotkey {
    pub fn listening_hotkey_target(&self) -> Option<HotkeyRebindTarget> {
        match self {
            ListeningForNewHotkey::Listening { target } => Some(*target),
            _ => None,
        }
    }
}

#[derive(PartialEq, Clone, Copy, Eq)]
pub enum HotkeyRebindTarget {
    /// Listening for start key
    Start,
    /// Listening for stop key
    Stop,
}

pub struct GitHubRelease {
    pub name: String,
    pub release_notes_url: String,
    pub download_url: String,
    pub release_date: Option<chrono::DateTime<chrono::Utc>>,
}

/// A request for some async action to happen. Response will be delivered via [`UiUpdate`].
pub enum AsyncRequest {
    ValidateApiKey { api_key: String },
    UploadData,
    CancelUpload,
    OpenDataDump,
    OpenLog,
    UpdateUnsupportedGames(UnsupportedGames),
    LoadUploadStats,
    LoadLocalRecordings,
    DeleteAllInvalidRecordings,
    OpenFolder(std::path::PathBuf),
}

/// A message sent to the UI thread, usually in response to some action taken in another thread
pub enum UiUpdate {
    /// Dummy update to force the UI to repaint
    ForceUpdate,
    UpdateAvailableVideoEncoders(Vec<VideoEncoderType>),
    UpdateUserId(Result<String, String>),
    UpdateUploadProgress(Option<ProgressData>),
    UploadFailed(String),
    UpdateTrayIconRecording(bool),
    UpdateNewerReleaseAvailable(GitHubRelease),
    UpdateLocalRecordings(Vec<LocalRecording>),
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
