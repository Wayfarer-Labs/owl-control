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

/// Minimum average FPS. We allow some leeway below 60 FPS, but we want to make sure
/// we aren't getting 30-40 FPS data.
pub const MIN_AVERAGE_FPS: f64 = FPS as f64 * 0.9;

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
