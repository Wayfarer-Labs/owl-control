use std::{
    fs,
    path::PathBuf,
    sync::{Arc, OnceLock, RwLock, atomic::AtomicBool},
    time::{Duration, Instant, SystemTime},
};

use constants::{encoding::VideoEncoderType, filename::persistent, unsupported_games::UnsupportedGames};
use egui_wgpu::wgpu;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use crate::{api::UserUploads, config::{get_persistent_dir, Config}, record::LocalRecording, upload::ProgressData};

/// Tracks active play time during recording sessions
#[derive(Debug, Clone, Copy)]
pub struct PlayTimeState {
    /// Total accumulated active time in current session
    pub total_active_duration: Duration,
    /// When current active period started (None if paused)
    pub current_session_start: Option<Instant>,
    /// Last time any activity was detected
    pub last_activity_time: Instant,
    /// When the tracker was last reset (used to enforce 8hr rolling window)
    pub last_break_end: Option<Instant>,
}

impl PlayTimeState {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            total_active_duration: Duration::ZERO,
            current_session_start: None,
            last_activity_time: now,
            last_break_end: Some(now),
        }
    }

    /// Get current total active time (accumulated + current session)
    pub fn get_total_active_time(&self) -> Duration {
        let mut total = self.total_active_duration;

        if let Some(session_start) = self.current_session_start {
            total += session_start.elapsed();
        }

        total
    }

    /// Check if currently in an active session
    pub fn is_active(&self) -> bool {
        self.current_session_start.is_some()
    }

    /// Start a new active session
    pub fn start_session(&mut self) {
        if self.current_session_start.is_none() {
            self.current_session_start = Some(Instant::now());
        }
    }

    /// Pause the current session, accumulating time
    pub fn pause_session(&mut self) {
        if let Some(session_start) = self.current_session_start.take() {
            self.total_active_duration += session_start.elapsed();
        }
    }

    /// Reset all tracking (called after 4hr+ break)
    pub fn reset(&mut self) {
        let now = Instant::now();
        self.total_active_duration = Duration::ZERO;
        self.current_session_start = None;
        self.last_break_end = Some(now);
    }

    /// Convert to a persisted form using SystemTime for serialization
    pub fn to_persisted(self) -> PlayTimeStatePersisted {
        PlayTimeStatePersisted {
            total_active_duration_secs: self.total_active_duration.as_secs_f64(),
            last_activity_time: SystemTime::now()
                .checked_sub(self.last_activity_time.elapsed())
                .unwrap_or(SystemTime::UNIX_EPOCH),
            last_break_end: self.last_break_end.map(|instant| {
                SystemTime::now()
                    .checked_sub(instant.elapsed())
                    .unwrap_or(SystemTime::UNIX_EPOCH)
            }),
        }
    }

    /// Create from a persisted form, converting SystemTime back to Instant
    pub fn from_persisted(persisted: PlayTimeStatePersisted) -> Self {
        let now = Instant::now();
        let now_system = SystemTime::now();

        // Calculate how long ago last_activity_time was
        let last_activity_time =
            if let Ok(elapsed) = now_system.duration_since(persisted.last_activity_time) {
                now.checked_sub(elapsed).unwrap_or(now)
            } else {
                now
            };

        // Calculate how long ago last_break_end was
        let last_break_end = persisted.last_break_end.and_then(|break_time| {
            now_system
                .duration_since(break_time)
                .ok()
                .and_then(|elapsed| now.checked_sub(elapsed))
        }).or(Some(now));

        Self {
            total_active_duration: Duration::from_secs_f64(persisted.total_active_duration_secs),
            current_session_start: None, // Don't restore active sessions
            last_activity_time,
            last_break_end,
        }
    }
}

/// Serializable version of PlayTimeState for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayTimeStatePersisted {
    /// Total accumulated active time in seconds
    pub total_active_duration_secs: f64,
    /// Last time any activity was detected
    pub last_activity_time: SystemTime,
    /// When the tracker was last reset
    pub last_break_end: Option<SystemTime>,
}

pub struct AppState {
    /// holds the current state of recording, recorder <-> overlay
    pub state: RwLock<RecordingStatus>,
    pub config: RwLock<Config>,
    pub async_request_tx: mpsc::Sender<AsyncRequest>,
    pub ui_update_tx: UiUpdateSender,
    pub ui_update_unreliable_tx: broadcast::Sender<UiUpdateUnreliable>,
    pub adapter_infos: Vec<wgpu::AdapterInfo>,
    pub upload_cancel_flag: Arc<AtomicBool>,
    pub listening_for_new_hotkey: RwLock<ListeningForNewHotkey>,
    pub is_out_of_date: AtomicBool,
    pub play_time_state: RwLock<PlayTimeState>,
    pub last_foregrounded_game: RwLock<Option<ForegroundedGame>>,
}
impl AppState {
    pub fn new(
        async_request_tx: mpsc::Sender<AsyncRequest>,
        ui_update_tx: UiUpdateSender,
        ui_update_unreliable_tx: broadcast::Sender<UiUpdateUnreliable>,
        adapter_infos: Vec<wgpu::AdapterInfo>,
    ) -> Self {
        Self {
            state: RwLock::new(RecordingStatus::Stopped),
            config: RwLock::new(Config::load().expect("failed to init configs")),
            async_request_tx,
            ui_update_tx,
            ui_update_unreliable_tx,
            adapter_infos,
            upload_cancel_flag: Arc::new(AtomicBool::new(false)),
            listening_for_new_hotkey: RwLock::new(ListeningForNewHotkey::NotListening),
            is_out_of_date: AtomicBool::new(false),
            play_time_state: RwLock::new(load_play_time_state()),
            last_foregrounded_game: RwLock::new(None),
        }
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
    DeleteRecording(PathBuf),
    OpenFolder(PathBuf),
    MoveRecordingsFolder { from: PathBuf, to: PathBuf },
    PickRecordingFolder { current_location: PathBuf },
    PlayCue { cue: String },
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

/// Save play time state to persistent storage
pub fn save_play_time_state(state: &PlayTimeState) -> Result<(), Box<dyn std::error::Error>> {
    let path =
        crate::config::get_persistent_dir()?.join(persistent::PLAY_TIME_STATE);

    let persisted = state.to_persisted();
    let json = serde_json::to_string_pretty(&persisted)?;
    fs::write(&path, json)?;

    tracing::debug!("Saved play time state to {}", path.display());
    Ok(())
}

/// Load play time state from persistent storage
pub fn load_play_time_state() -> PlayTimeState {
    let path_result =
        get_persistent_dir().map(|dir| dir.join(persistent::PLAY_TIME_STATE));

    let path = match path_result {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Failed to get persistent directory: {}", e);
            return PlayTimeState::new();
        }
    };

    if !path.exists() {
        tracing::info!("No saved play time state found, using defaults");
        return PlayTimeState::new();
    }


    let mut state = match fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<PlayTimeStatePersisted>(&contents) {
            Ok(persisted) => {
                tracing::info!("Loaded play time state from {}", path.display());
                PlayTimeState::from_persisted(persisted)
            }
            Err(e) => {
                tracing::warn!("Failed to parse play time state: {}, using defaults", e);
                PlayTimeState::new()
            }
        },
        Err(e) => {
            tracing::warn!("Failed to read play time state file: {}, using defaults", e);
            PlayTimeState::new()
        }
    };

    // Validate loaded state
    let idle_duration = state.last_activity_time.elapsed();

    let should_reset = if idle_duration > constants::PLAY_TIME_BREAK_THRESHOLD {
        tracing::info!(
            "Idle duration ({:?}) exceeds break threshold ({:?}), resetting play time on load",
            idle_duration,
            constants::PLAY_TIME_BREAK_THRESHOLD
        );
        true
    } else if let Some(break_end) = state.last_break_end {
        if break_end.elapsed() > constants::PLAY_TIME_ROLLING_WINDOW {
            tracing::info!(
                "Time since last break ({:?}) exceeds rolling window ({:?}), resetting play time on load",
                break_end.elapsed(),
                constants::PLAY_TIME_ROLLING_WINDOW
            );
            true
        } else {
            false
        }
    } else {
        false
    };


    if should_reset {
        state.reset();
    }

    state
}
