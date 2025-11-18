use crate::config::get_persistent_dir;
use constants::filename::persistent;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fs,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod serial_secs {
    use super::*;

    pub fn default_time() -> SystemTime {
        SystemTime::now()
    }

    pub fn serialize<S>(t: &SystemTime, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let ts = t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        s.serialize_u64(ts)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(d)?;
        Ok(UNIX_EPOCH + Duration::from_secs(secs))
    }
}

mod serial_dur {
    use super::*;
    // Same logic, just distinct for Duration vs SystemTime types if needed
    pub fn serialize<S>(d: &Duration, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_u64(d.as_secs())
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(d)?;
        Ok(Duration::from_secs(secs))
    }
}

// REMOVED: Copy trait (SystemTime is not Copy)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayTimeState {
    #[serde(with = "serial_dur")]
    pub total_active_duration: Duration,

    #[serde(skip)]
    pub current_session_start: Option<Instant>,

    #[serde(with = "serial_secs")]
    pub last_activity_time: SystemTime,

    // CHANGED: Points to specific default function
    #[serde(default = "serial_secs::default_time", with = "serial_secs")]
    pub last_break_end: SystemTime,
}

impl PlayTimeState {
    pub fn new() -> Self {
        Self {
            total_active_duration: Duration::ZERO,
            current_session_start: None,
            last_activity_time: SystemTime::now(),
            last_break_end: SystemTime::now(),
        }
    }

    pub fn get_total_active_time(&self) -> Duration {
        self.total_active_duration
            + self
                .current_session_start
                .map_or(Duration::ZERO, |s| s.elapsed())
    }

    pub fn is_active(&self) -> bool {
        self.current_session_start.is_some()
    }

    pub fn start_session(&mut self) {
        if self.current_session_start.is_none() {
            self.current_session_start = Some(Instant::now());
            // Also update activity time immediately
            self.last_activity_time = SystemTime::now();
        }
    }

    /// Called periodically while recording is active to mark ongoing activity
    pub fn tick_activity(&mut self) {
        // Keep updating the "Last Seen" time to Now while playing.
        // This ensures that when the recording stops, this timestamp
        // marks the exact end of the session.
        self.last_activity_time = SystemTime::now();
    }

    pub fn pause_session(&mut self) {
        if let Some(start) = self.current_session_start.take() {
            self.total_active_duration += start.elapsed();
        }
        // Ensure we mark the end of the session
        self.last_activity_time = SystemTime::now();
    }

    /// Called ONLY when the Recorder stops specifically due to IDLE
    pub fn cancel_idle_buffer(&mut self) {
        // We subtract the idle duration (e.g. 30s) from the total
        // because the Recorder includes it before triggering the stop.
        let idle_buffer = constants::MAX_IDLE_DURATION;
        if self.total_active_duration >= idle_buffer {
            self.total_active_duration -= idle_buffer;
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn should_reset(&self) -> bool {
        let idle = self.last_activity_time.elapsed().unwrap_or_default();
        let since_break = self.last_break_end.elapsed().unwrap_or_default();

        idle > constants::PLAY_TIME_BREAK_THRESHOLD
            || since_break > constants::PLAY_TIME_ROLLING_WINDOW
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = get_persistent_dir()?.join(persistent::PLAY_TIME_STATE);
        fs::write(&path, serde_json::to_string_pretty(self)?)?;
        tracing::debug!("Saved play time state to {}", path.display());
        Ok(())
    }

    pub fn load() -> Self {
        let state = (|| -> Result<Self, Box<dyn std::error::Error>> {
            let path = get_persistent_dir()?.join(persistent::PLAY_TIME_STATE);
            Ok(serde_json::from_str(&fs::read_to_string(&path)?)?)
        })()
        .unwrap_or_else(|e| {
            tracing::info!("Using default play time state: {}", e);
            Self::new()
        });

        if state.should_reset() {
            tracing::info!("Resetting play time state based on thresholds");
            return Self::new();
        }
        state
    }
}

impl Default for PlayTimeState {
    fn default() -> Self {
        Self::new()
    }
}
