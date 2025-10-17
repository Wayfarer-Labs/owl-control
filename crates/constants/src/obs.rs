use libobs_wrapper::encoders::ObsVideoEncoderType;

/// Video encoder constants
/// List of supported video encoders that will be displayed for user to select
pub const SUPPORTED_VIDEO_ENCODERS: [ObsVideoEncoderType; 2] = [
    ObsVideoEncoderType::OBS_X264,
    ObsVideoEncoderType::FFMPEG_NVENC,
];

// List of allowed video presets
pub const VIDEO_PRESETS: [&str; 3] = ["p5", "p6", "p7"];

/// List of allowed video profiles (you really only want "high", but it's a list in case you want to include "main" ig)
pub const VIDEO_PROFILES: [&str; 1] = ["high"];
