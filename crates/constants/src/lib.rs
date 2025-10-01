pub mod unsupported_games;

// Keep in sync with vg_control/constants.py (for now!)
pub const FPS: u32 = 60;
pub const RECORDING_WIDTH: u32 = 640;
pub const RECORDING_HEIGHT: u32 = 360;

/// Minimum free space required to record (in megabytes)
pub const MIN_FREE_SPACE_MB: u64 = 512;
