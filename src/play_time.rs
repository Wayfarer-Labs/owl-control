use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use color_eyre::eyre::Result;

use crate::app_state::RecordingStatus;

/// Tracks cumulative active play time across recording sessions.
pub struct PlayTimeTracker {
    /// Total active play time accumulated
    total_active_duration: Duration,
    /// When the current session started (if active)
    current_session_start: Option<Instant>,
    /// Last time we recorded activity (for break detection)
    last_activity_time: DateTime<Utc>,
    /// When the last break ended (for rolling window calculation)
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
        let current_session_time = self
            .current_session_start
            .map(|start| start.elapsed())
            .unwrap_or_default();
        self.total_active_duration + current_session_time
    }

    /// Returns true if currently in an active session
    pub fn is_active(&self) -> bool {
        self.current_session_start.is_some()
    }

    /// Called every tick to update state based on recording status
    pub fn tick(&mut self, recording_status: &RecordingStatus) {
        // Check for resets if not active
        if !self.is_active() && self.should_reset() {
            self.reset();
        }

        match recording_status {
            RecordingStatus::Recording { .. } => {
                if !self.is_active() {
                    self.start_session();
                }
                self.tick_activity();
            }
            RecordingStatus::Paused | RecordingStatus::Stopped => {
                if self.is_active() {
                    self.pause_session();
                }
            }
        }
    }

    /// Called on recording state transitions
    /// - `is_recording`: true if transitioning to recording, false if stopping/pausing
    /// - `due_to_idle`: true if stopping due to idle timeout (should cancel idle buffer)
    pub fn handle_transition(&mut self, is_recording: bool, due_to_idle: bool) {
        if is_recording {
            self.start_session();
        } else {
            if due_to_idle {
                self.cancel_idle_buffer();
            }
            self.pause_session();
        }
        // Save after transitions
        if let Err(e) = self.save() {
            tracing::warn!("Failed to save play time after transition: {}", e);
        }
    }

    /// Start a new session
    fn start_session(&mut self) {
        if self.current_session_start.is_none() {
            self.current_session_start = Some(Instant::now());
            self.last_activity_time = Utc::now();
        }
    }

    /// Pause the current session, accumulating time
    fn pause_session(&mut self) {
        if let Some(start) = self.current_session_start.take() {
            self.total_active_duration += start.elapsed();
        }
    }

    /// Mark activity (called during active recording)
    fn tick_activity(&mut self) {
        self.last_activity_time = Utc::now();
    }

    /// Cancel the idle buffer (subtract MAX_IDLE_DURATION from total time)
    fn cancel_idle_buffer(&mut self) {
        self.total_active_duration = self
            .total_active_duration
            .saturating_sub(constants::MAX_IDLE_DURATION);
    }

    /// Check if we should reset (4 hours idle or 8 hours since last break)
    pub fn should_reset(&self) -> bool {
        let now = Utc::now();
        let idle_duration = now
            .signed_duration_since(self.last_activity_time)
            .to_std()
            .unwrap_or(Duration::ZERO);
        let since_break = now
            .signed_duration_since(self.last_break_end)
            .to_std()
            .unwrap_or(Duration::ZERO);

        idle_duration >= constants::PLAY_TIME_BREAK_THRESHOLD
            || since_break >= constants::PLAY_TIME_ROLLING_WINDOW
    }

    /// Reset the tracker to initial state
    pub fn reset(&mut self) {
        let now = Utc::now();
        self.total_active_duration = Duration::ZERO;
        self.current_session_start = None;
        self.last_activity_time = now;
        self.last_break_end = now;
    }

    /// Save state to disk
    pub fn save(&self) -> Result<()> {
        let state = SerialPlayTimeState {
            total_active_secs: self.total_active_duration.as_secs(),
            last_activity_time: self.last_activity_time,
            last_break_end: self.last_break_end,
        };
        let path = crate::config::get_persistent_dir()?
            .join(constants::filename::persistent::PLAY_TIME_STATE);
        let json = serde_json::to_string_pretty(&state)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Load state from disk, or return a new tracker if not found
    pub fn load() -> Self {
        match load_from_file() {
            Ok(state) => state,
            Err(e) => {
                tracing::debug!("Failed to load play time state, using defaults: {}", e);
                Self::new()
            }
        }
    }
}

impl Default for PlayTimeTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PlayTimeTracker {
    fn drop(&mut self) {
        // Save state on shutdown
        if let Err(e) = self.save() {
            tracing::error!("Failed to save play time state on drop: {}", e);
        }
    }
}

fn load_from_file() -> Result<PlayTimeTracker> {
    let path = crate::config::get_persistent_dir()?
        .join(constants::filename::persistent::PLAY_TIME_STATE);
    let json = std::fs::read_to_string(&path)?;
    let state: SerialPlayTimeState = serde_json::from_str(&json)?;

    let mut tracker = PlayTimeTracker {
        total_active_duration: Duration::from_secs(state.total_active_secs),
        current_session_start: None,
        last_activity_time: state.last_activity_time,
        last_break_end: state.last_break_end,
    };

    // Check if we should reset based on loaded state
    if tracker.should_reset() {
        tracker.reset();
    }

    Ok(tracker)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerialPlayTimeState {
    total_active_secs: u64,
    last_activity_time: DateTime<Utc>,
    last_break_end: DateTime<Utc>,
}
