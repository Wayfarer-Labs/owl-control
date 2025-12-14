use std::{
    path::PathBuf,
    sync::{
        Arc, OnceLock, RwLock,
        atomic::{AtomicBool, AtomicUsize},
    },
    time::{Duration, Instant},
};

use constants::{encoding::VideoEncoderType, supported_games::SupportedGames};
use egui_wgpu::wgpu;
use tokio::sync::{broadcast, mpsc};

use crate::{
    api::UserUploads, config::Config, play_time::PlayTimeTracker, record::LocalRecording,
    upload::ProgressData,
};

pub struct AppState {
    /// holds the current state of recording, recorder <-> overlay
    pub state: RwLock<RecordingStatus>,
    pub config: RwLock<Config>,
    pub async_request_tx: mpsc::Sender<AsyncRequest>,
    pub ui_update_tx: UiUpdateSender,
    pub ui_update_unreliable_tx: broadcast::Sender<UiUpdateUnreliable>,
    pub adapter_infos: Vec<wgpu::AdapterInfo>,
    pub upload_pause_flag: Arc<AtomicBool>,
    /// Number of pending auto-upload requests queued (recordings completed while upload in progress)
    pub auto_upload_queue_count: Arc<AtomicUsize>,
    /// Flag indicating an upload is currently in progress
    pub upload_in_progress: Arc<AtomicBool>,
    pub listening_for_new_hotkey: RwLock<ListeningForNewHotkey>,
    pub is_out_of_date: AtomicBool,
    pub play_time_state: RwLock<PlayTimeTracker>,
    pub last_foregrounded_game: RwLock<Option<ForegroundedGame>>,
    pub supported_games: RwLock<SupportedGames>,
    /// Flag for offline mode - skips API server calls when enabled
    pub offline_mode: AtomicBool,
}
impl AppState {
    pub fn new(
        async_request_tx: mpsc::Sender<AsyncRequest>,
        ui_update_tx: UiUpdateSender,
        ui_update_unreliable_tx: broadcast::Sender<UiUpdateUnreliable>,
        adapter_infos: Vec<wgpu::AdapterInfo>,
    ) -> Self {
        tracing::debug!("AppState::new() called");
        tracing::debug!("Loading configuration");
        let state = Self {
            state: RwLock::new(RecordingStatus::Stopped),
            config: RwLock::new(Config::load().expect("failed to init configs")),
            async_request_tx,
            ui_update_tx,
            ui_update_unreliable_tx,
            adapter_infos,
            upload_pause_flag: Arc::new(AtomicBool::new(false)),
            auto_upload_queue_count: Arc::new(AtomicUsize::new(0)),
            upload_in_progress: Arc::new(AtomicBool::new(false)),
            listening_for_new_hotkey: RwLock::new(ListeningForNewHotkey::NotListening),
            is_out_of_date: AtomicBool::new(false),
            play_time_state: RwLock::new(PlayTimeTracker::load()),
            last_foregrounded_game: RwLock::new(None),
            supported_games: RwLock::new(SupportedGames::load_from_embedded()),
            offline_mode: AtomicBool::new(false),
        };
        tracing::debug!("AppState::new() complete");
        state
    }
}

#[derive(Clone, PartialEq)]
pub struct ForegroundedGame {
    pub exe_name: Option<String>,
    pub unsupported_reason: Option<String>,
}
impl ForegroundedGame {
    pub fn is_recordable(&self) -> bool {
        self.unsupported_reason.is_none()
    }
}

/// This is meant to be a read-only reflection of the current recording state that is
/// only updated by the recorder.rs object (not tokio_thread RecordingState), and read by UI and overlay threads.
/// We want the RecordingStatus to reflect ground truth, and its also more accurate to get ::Recording info
/// directly from the recorder object. Desync between RecordingStatus and RecordingState shouldn't occur either way.
#[derive(Clone, PartialEq)]
pub enum RecordingStatus {
    Stopped,
    Recording {
        start_time: Instant,
        game_exe: String,
        current_fps: Option<f64>,
    },
    Paused,
}
impl RecordingStatus {
    pub fn is_recording(&self) -> bool {
        matches!(self, RecordingStatus::Recording { .. })
    }
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
    ValidateApiKey {
        api_key: String,
    },
    UploadData,
    PauseUpload,
    OpenDataDump,
    OpenLog,
    UpdateSupportedGames(SupportedGames),
    LoadUploadStats,
    LoadLocalRecordings,
    DeleteAllInvalidRecordings,
    DeleteAllUploadedLocalRecordings,
    DeleteRecording(PathBuf),
    OpenFolder(PathBuf),
    MoveRecordingsFolder {
        from: PathBuf,
        to: PathBuf,
    },
    PickRecordingFolder {
        current_location: PathBuf,
    },
    PlayCue {
        cue: String,
    },
    /// Sent by upload::start() when upload batch completes, with count of recordings processed
    UploadCompleted {
        uploaded_count: usize,
    },
    /// Clear the auto-upload queue (called when unchecking auto-upload preference)
    ClearAutoUploadQueue,
    /// Switch to/from offline mode
    SetOfflineMode {
        enabled: bool,
        offline_reason: Option<String>,
    },
}

/// A message sent to the UI thread, usually in response to some action taken in another thread
pub enum UiUpdate {
    /// Dummy update to force the UI to repaint
    ForceUpdate,
    UpdateAvailableVideoEncoders(Vec<VideoEncoderType>),
    UpdateUserId(Result<String, String>),
    UploadFailed(String),
    UpdateRecordingState(bool),
    UpdateNewerReleaseAvailable(GitHubRelease),
    UpdateUserUploads(UserUploads),
    UpdateLocalRecordings(Vec<LocalRecording>),
    FolderPickerResult {
        old_path: PathBuf,
        new_path: PathBuf,
    },
    /// Update the auto-upload queue count displayed in the UI
    UpdateAutoUploadQueueCount(usize),
}

/// A message sent to the UI thread, usually in response to some action taken in another thread
/// but is not important enough to warrant a force update, or to be queued up.
#[derive(Clone, PartialEq)]
pub enum UiUpdateUnreliable {
    UpdateUploadProgress(Option<ProgressData>),
}

pub type UiUpdateUnreliableSender = broadcast::Sender<UiUpdateUnreliable>;

/// A sender for [`UiUpdate`] messages. Will automatically repaint the UI after sending a message.
#[derive(Clone)]
pub struct UiUpdateSender {
    tx: mpsc::UnboundedSender<UiUpdate>,
    pub ctx: OnceLock<egui::Context>,
}
impl UiUpdateSender {
    pub fn build() -> (Self, tokio::sync::mpsc::UnboundedReceiver<UiUpdate>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                tx,
                ctx: OnceLock::new(),
            },
            rx,
        )
    }

    pub fn send(&self, cmd: UiUpdate) -> Result<(), mpsc::error::SendError<UiUpdate>> {
        let res = self.tx.send(cmd);
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint_after(Duration::from_millis(10))
        }
        res
    }
}
