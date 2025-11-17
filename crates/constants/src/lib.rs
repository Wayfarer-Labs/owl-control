use std::time::Duration;

pub mod encoding;
pub mod unsupported_games;

// Keep in sync with vg_control/constants.py (for now!)
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

/// Play-time tracker: threshold before showing overlay (2 hours)
pub const PLAY_TIME_THRESHOLD: Duration = Duration::from_secs(2 * 60 * 60);
// pub const PLAY_TIME_THRESHOLD: Duration = Duration::from_secs(60); /// for testing
/// Play-time tracker: update interval (30 minutes)
pub const PLAY_TIME_UPDATE_INTERVAL: Duration = Duration::from_secs(30 * 60);
// pub const PLAY_TIME_UPDATE_INTERVAL: Duration = Duration::from_secs(60); /// for testing
/// Play-time tracker: break threshold - reset after this much idle time (4 hours)
pub const PLAY_TIME_BREAK_THRESHOLD: Duration = Duration::from_secs(4 * 60 * 60);
// pub const PLAY_TIME_BREAK_THRESHOLD: Duration = Duration::from_secs(2 * 60); /// for testing
/// Play-time tracker: rolling window - reset after this much time since last break (12 hours)
pub const PLAY_TIME_ROLLING_WINDOW: Duration = Duration::from_secs(12 * 60 * 60);
// pub const PLAY_TIME_ROLLING_WINDOW: Duration = Duration::from_secs(10 * 60); /// for testing
/// Play-time tracker: display message
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
    }
}
