use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use color_eyre::eyre::Result;

use crate::app_state::RecordingStatus;

/// Returns the path to the play time state file
fn state_file_path() -> Result<PathBuf> {
    Ok(crate::config::get_persistent_dir()?.join(constants::filename::persistent::PLAY_TIME_STATE))
}

/// Tracks cumulative active play time across recording sessions.
pub struct PlayTimeTracker {
    total_active_duration: Duration,
    current_session_start: Option<Instant>,
    last_activity_time: DateTime<Utc>,
    last_break_end: DateTime<Utc>,
}

impl PlayTimeTracker {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            total_active_duration: Duration::ZERO,
            current_session_start: None,
            last_activity_time: now,
            last_break_end: now,
        }
    }

    /// Returns the total active time including any current session
    pub fn get_total_active_time(&self) -> Duration {
        self.total_active_duration
            + self
                .current_session_start
                .map_or(Duration::ZERO, |s| s.elapsed())
    }

    /// Returns true if currently in an active session
    pub fn is_active(&self) -> bool {
        self.current_session_start.is_some()
    }

    /// Called every tick to update state based on recording status
    pub fn tick(&mut self, recording_status: &RecordingStatus) {
        if !self.is_active() && self.should_reset() {
            self.reset();
        }

        match recording_status {
            RecordingStatus::Recording { .. } => {
                if !self.is_active() {
                    self.start_session();
                }
                self.last_activity_time = Utc::now();
            }
            RecordingStatus::Paused | RecordingStatus::Stopped => {
                if self.is_active() {
                    self.pause_session();
                }
            }
        }
    }

    /// Called on recording state transitions
    pub fn handle_transition(&mut self, is_recording: bool, due_to_idle: bool) {
        if is_recording {
            self.start_session();
        } else {
            if due_to_idle {
                self.total_active_duration = self
                    .total_active_duration
                    .saturating_sub(constants::MAX_IDLE_DURATION);
            }
            self.pause_session();
        }
        if let Err(e) = self.save() {
            tracing::warn!("Failed to save play time after transition: {e}");
        }
    }

    fn start_session(&mut self) {
        if self.current_session_start.is_none() {
            self.current_session_start = Some(Instant::now());
            self.last_activity_time = Utc::now();
        }
    }

    fn pause_session(&mut self) {
        if let Some(start) = self.current_session_start.take() {
            self.total_active_duration += start.elapsed();
        }
    }

    fn should_reset(&self) -> bool {
        let now = Utc::now();
        let idle = (now - self.last_activity_time).to_std().unwrap_or_default();
        let since_break = (now - self.last_break_end).to_std().unwrap_or_default();

        idle >= constants::PLAY_TIME_BREAK_THRESHOLD
            || since_break >= constants::PLAY_TIME_ROLLING_WINDOW
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn save(&self) -> Result<()> {
        let state = SerialState::from(self);
        std::fs::write(state_file_path()?, serde_json::to_string_pretty(&state)?)?;
        Ok(())
    }

    pub fn load() -> Self {
        Self::load_from_file().unwrap_or_else(|e| {
            tracing::debug!("Failed to load play time state: {e}");
            Self::new()
        })
    }

    fn load_from_file() -> Result<Self> {
        let state: SerialState =
            serde_json::from_str(&std::fs::read_to_string(state_file_path()?)?)?;
        let mut tracker = Self {
            total_active_duration: Duration::from_secs(state.total_active_secs),
            current_session_start: None,
            last_activity_time: state.last_activity_time,
            last_break_end: state.last_break_end,
        };
        if tracker.should_reset() {
            tracker.reset();
        }
        Ok(tracker)
    }
}

impl Default for PlayTimeTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PlayTimeTracker {
    fn drop(&mut self) {
        if let Err(e) = self.save() {
            tracing::error!("Failed to save play time state on drop: {e}");
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerialState {
    total_active_secs: u64,
    last_activity_time: DateTime<Utc>,
    last_break_end: DateTime<Utc>,
}

impl From<&PlayTimeTracker> for SerialState {
    fn from(t: &PlayTimeTracker) -> Self {
        Self {
            total_active_secs: t.total_active_duration.as_secs(),
            last_activity_time: t.last_activity_time,
            last_break_end: t.last_break_end,
        }
    }
}
