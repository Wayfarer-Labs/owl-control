use serde::{Deserialize, Serialize};

/// Supported video encoder types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoEncoderType {
    X264,
    NvEnc,
}
impl std::fmt::Display for VideoEncoderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoEncoderType::X264 => write!(f, "OBS x264 (CPU)"),
            VideoEncoderType::NvEnc => write!(f, "NVIDIA NVENC (GPU)"),
        }
    }
}

/// Video encoder constants
/// List of supported video encoders that will be displayed for user to select
pub const SUPPORTED_VIDEO_ENCODERS: &[VideoEncoderType] =
    &[VideoEncoderType::X264, VideoEncoderType::NvEnc];

/// Preset options for different encoder types
/// https://github.com/obsproject/obs-studio/blob/5ec3af3f6d6465122dc2b0abff9661cbe64b406b/plugins/obs-x264/obs-x264.c
pub const X264_PRESETS: &[&str] = &["fast", "faster", "veryfast"];

/// https://github.com/obsproject/obs-studio/blob/0b1229632063a13dfd26cf1cd9dd43431d8c68f6/plugins/obs-nvenc/nvenc-properties.c#L145
pub const NVENC_PRESETS: &[&str] = &["p7", "p6", "p5", "p4", "p3", "p2", "p1"];

// Placeholders for now as we only expose obsx264 and ffmpeg nvenc
pub const QSV_PRESETS: &[&str] = &[
    "quality", "balanced", "speed", "veryfast", "faster", "fast", "medium",
];
pub const AMF_PRESETS: &[&str] = &["quality", "balanced", "speed"];

/// ffmpeg-nvenc: https://github.com/obsproject/obs-studio/blob/0b1229632063a13dfd26cf1cd9dd43431d8c68f6/plugins/obs-ffmpeg/obs-ffmpeg-nvenc.c#L504
/// obs-nvenc: https://github.com/obsproject/obs-studio/blob/0b1229632063a13dfd26cf1cd9dd43431d8c68f6/plugins/obs-nvenc/nvenc-properties.c#L159
/// both are the same
pub const NVENC_TUNE_OPTIONS: &[&str] = &["hq", "ll", "ull"];

/// We lock to the high profile for now. Other profiles are not of much use to us.
pub const VIDEO_PROFILE: &str = "high";

/// Bitrate (kbps)
pub const BITRATE: i64 = 2500;

/// Rate control
pub const RATE_CONTROL: &str = "CBR";

/// B-frames
pub const B_FRAMES: i64 = 2;

/// Psycho AQ
pub const PSYCHO_AQ: bool = true;

/// Lookahead
pub const LOOKAHEAD: bool = true;
