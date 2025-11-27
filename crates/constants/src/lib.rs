use std::time::Duration;

pub mod encoding;
pub mod supported_games;

pub const FPS: u32 = 60;
pub const RECORDING_WIDTH: u32 = 640;
pub const RECORDING_HEIGHT: u32 = 360;

/// Minimum free space required to record (in megabytes)
pub const MIN_FREE_SPACE_MB: u64 = 512;

/// Minimum footage length
pub const MIN_FOOTAGE: Duration = Duration::from_secs(20);
/// Maximum footage length
pub const MAX_FOOTAGE: Duration = Duration::from_secs(10 * 60);
/// Maximum idle duration before stopping recording
pub const MAX_IDLE_DURATION: Duration = Duration::from_secs(30);
/// Maximum time to wait for OBS to hook into the application before stopping recording
pub const HOOK_TIMEOUT: Duration = Duration::from_secs(5);

/// Minimum average FPS. We allow some leeway below 60 FPS, but we want to make sure
/// we aren't getting 30-40 FPS data.
pub const MIN_AVERAGE_FPS: f64 = FPS as f64 * 0.9;

/// Play-time tracker
/// Threshold before showing overlay
// pub const PLAY_TIME_THRESHOLD: Duration = Duration::from_secs(2 * 60 * 60);
pub const PLAY_TIME_THRESHOLD: Duration = Duration::from_secs(60);
/// Display granularity - how coarsely to round time values for display
// pub const PLAY_TIME_DISPLAY_GRANULARITY: Duration = Duration::from_secs(30 * 60);
pub const PLAY_TIME_DISPLAY_GRANULARITY: Duration = Duration::from_secs(60);
/// Break threshold - reset after this much idle time
pub const PLAY_TIME_BREAK_THRESHOLD: Duration = Duration::from_secs(4 * 60 * 60);
/// Rolling window - reset after this much time since last break
pub const PLAY_TIME_ROLLING_WINDOW: Duration = Duration::from_secs(8 * 60 * 60);
/// Display message
pub const PLAY_TIME_MESSAGE: &str = "Active {duration}";

/// GitHub organization
pub const GH_ORG: &str = "Wayfarer-Labs";
/// GitHub repository
pub const GH_REPO: &str = "owl-control";

pub mod filename {
    pub mod recording {
        /// Reasons that a recording is invalid
        pub const INVALID: &str = ".invalid";
        /// Reasons that a server invalidated a recording
        pub const SERVER_INVALID: &str = ".server_invalid";
        /// Indicates the file was uploaded; contains information about the upload
        pub const UPLOADED: &str = ".uploaded";
        /// Stores upload progress state for pause/resume functionality
        pub const UPLOAD_PROGRESS: &str = ".upload-progress";
        /// The video recording file
        pub const VIDEO: &str = "recording.mp4";
        /// The input recording file
        pub const INPUTS: &str = "inputs.csv";
        /// The metadata file
        pub const METADATA: &str = "metadata.json";
    }

    pub mod persistent {
        /// The config file, stored in persistent data directory
        pub const CONFIG: &str = "config.json";
        /// The play time state file, stored in persistent data directory
        pub const PLAY_TIME_STATE: &str = "play_time.json";
    }
}
