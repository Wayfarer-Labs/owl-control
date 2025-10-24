use serde::{Deserialize, Serialize};

/// Supported video encoder types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoEncoderType {
    X264,
    NvEnc,
    Amf,
    Qsv,
}
impl std::fmt::Display for VideoEncoderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoEncoderType::X264 => write!(f, "OBS x264 (CPU)"),
            VideoEncoderType::NvEnc => write!(f, "NVIDIA NVENC (GPU)"),
            VideoEncoderType::Amf => write!(f, "AMD HW H.264 (AVC)"),
            VideoEncoderType::Qsv => write!(f, "QuickSync H.264"),
        }
    }
}
impl VideoEncoderType {
    pub fn id(&self) -> &str {
        match self {
            VideoEncoderType::X264 => "x264",
            VideoEncoderType::NvEnc => "nvenc",
            VideoEncoderType::Amf => "amf",
            VideoEncoderType::Qsv => "qsv",
        }
    }
}

/// Preset options for different encoder types
/// https://github.com/obsproject/obs-studio/blob/5ec3af3f6d6465122dc2b0abff9661cbe64b406b/plugins/obs-x264/obs-x264.c
pub const X264_PRESETS: &[&str] = &["fast", "faster", "veryfast"];

/// https://github.com/obsproject/obs-studio/blob/0b1229632063a13dfd26cf1cd9dd43431d8c68f6/plugins/obs-nvenc/nvenc-properties.c#L145
pub const NVENC_PRESETS: &[&str] = &["p7", "p6", "p5", "p4", "p3", "p2", "p1"];

/// https://github.com/obsproject/obs-studio/blob/c025f210d36ada93c6b9ef2affd0f671b34c9775/plugins/obs-qsv11/obs-qsv11.c#L293-L311
pub const QSV_TARGET_USAGES: &[&str] = &[
    "quality", "balanced", "speed", "veryfast", "faster", "fast", "medium",
];

/// https://github.com/obsproject/obs-studio/blob/c025f210d36ada93c6b9ef2affd0f671b34c9775/plugins/obs-ffmpeg/texture-amf.cpp#L1276-L1284
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
