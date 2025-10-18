use libobs_wrapper::encoders::ObsVideoEncoderType;

/// Video encoder constants
/// List of supported video encoders that will be displayed for user to select
// pub const SUPPORTED_VIDEO_ENCODERS: [ObsVideoEncoderType; 2] = [
//     ObsVideoEncoderType::OBS_X264,
//     ObsVideoEncoderType::FFMPEG_NVENC,
// ];

/// Preset options for different encoder types
/// https://github.com/obsproject/obs-studio/blob/5ec3af3f6d6465122dc2b0abff9661cbe64b406b/plugins/obs-x264/obs-x264.c
pub const X264_PRESETS: [&str; 3] = ["veryfast", "faster", "fast"];

/// https://github.com/obsproject/obs-studio/blob/0b1229632063a13dfd26cf1cd9dd43431d8c68f6/plugins/obs-nvenc/nvenc-properties.c#L145
pub const NVENC_PRESETS: [&str; 3] = ["p5", "p6", "p7"];

/// Placeholders for now as we only expose obsx264 and ffmpeg nvenc
pub const QSV_PRESETS: [&str; 7] = [
    "speed", "balanced", "quality", "veryfast", "faster", "fast", "medium",
];
pub const AMF_PRESETS: [&str; 3] = ["speed", "balanced", "quality"];

/// Tune options for obs x264 shouldn't need to be shown to users (should default to ""), but it's here anyway
/// see: https://superuser.com/questions/564402/explanation-of-x264-tune
pub const X264_TUNE_OPTIONS: [&str; 9] = [
    "film",
    "animation",
    "grain",
    "stillimage",
    "fastdecode",
    "zerolatency",
    "psnr",
    "ssim",
    "",
];

/// ffmpeg-nvenc: https://github.com/obsproject/obs-studio/blob/0b1229632063a13dfd26cf1cd9dd43431d8c68f6/plugins/obs-ffmpeg/obs-ffmpeg-nvenc.c#L504
/// obs-nvenc: https://github.com/obsproject/obs-studio/blob/0b1229632063a13dfd26cf1cd9dd43431d8c68f6/plugins/obs-nvenc/nvenc-properties.c#L159
/// both are the same
pub const NVENC_TUNE_OPTIONS: [&str; 3] = ["hq", "ll", "ull"];

/// List of allowed video profiles (you really only want "high", but it's a list in case you want to include "main" ig)
pub const VIDEO_PROFILES: [&str; 1] = ["high"];
