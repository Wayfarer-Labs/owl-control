use color_eyre::eyre::{Context, Result, eyre};
use constants::obs::{AMF_PRESETS, NVENC_PRESETS, NVENC_TUNE_OPTIONS, QSV_PRESETS, X264_PRESETS};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fs, path::PathBuf, str::FromStr};

use libobs_wrapper::encoders::ObsVideoEncoderType;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// camel case renames are legacy from old existing configs, we want it to be backwards-compatible with previous owl releases that used electron
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default = "default_start_key")]
    pub start_recording_key: String,
    #[serde(default = "default_stop_key")]
    pub stop_recording_key: String,
    #[serde(default)]
    pub stop_hotkey_enabled: bool,
    #[serde(default)]
    pub unreliable_connection: bool,
    #[serde(default)]
    pub overlay_location: OverlayLocation,
    #[serde(default = "default_opacity")]
    pub overlay_opacity: u8,
    #[serde(default)]
    pub delete_uploaded_files: bool,
    #[serde(default)]
    pub honk: bool,
    #[serde(default)]
    pub recording_backend: RecordingBackend,
    #[serde(default)]
    pub video_settings: VideoSettings,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            start_recording_key: default_start_key(),
            stop_recording_key: default_stop_key(),
            stop_hotkey_enabled: Default::default(),
            unreliable_connection: Default::default(),
            overlay_location: Default::default(),
            overlay_opacity: default_opacity(),
            delete_uploaded_files: Default::default(),
            honk: Default::default(),
            recording_backend: Default::default(),
            video_settings: Default::default(),
        }
    }
}
impl Preferences {
    pub fn start_recording_key(&self) -> &str {
        &self.start_recording_key
    }
    pub fn stop_recording_key(&self) -> &str {
        if self.stop_hotkey_enabled {
            &self.stop_recording_key
        } else {
            &self.start_recording_key
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum RecordingBackend {
    #[default]
    Embedded,
    Socket,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum OverlayLocation {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}
impl OverlayLocation {
    pub const ALL: [OverlayLocation; 4] = [
        OverlayLocation::TopLeft,
        OverlayLocation::TopRight,
        OverlayLocation::BottomLeft,
        OverlayLocation::BottomRight,
    ];
}
impl std::fmt::Display for OverlayLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverlayLocation::TopLeft => write!(f, "Top Left"),
            OverlayLocation::TopRight => write!(f, "Top Right"),
            OverlayLocation::BottomLeft => write!(f, "Bottom Left"),
            OverlayLocation::BottomRight => write!(f, "Bottom Right"),
        }
    }
}

// by default now start and stop recording are mapped to same key
// f5 instead of f4 so users can alt+f4 properly.
fn default_start_key() -> String {
    "F5".to_string()
}
fn default_stop_key() -> String {
    "F5".to_string()
}
fn default_opacity() -> u8 {
    85
}

// For some reason, previous electron configs saved hasConsented as a string instead of a boolean? So now we need a custom deserializer
// to take that into account for backwards compatibility
fn deserialize_string_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Bool(b) => Ok(b),
        serde_json::Value::String(s) => match s.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(Error::custom(format!("Invalid boolean string: {}", s))),
        },
        _ => Err(Error::custom("Expected boolean or string")),
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    #[serde(default)]
    pub api_key: String,
    #[serde(default, deserialize_with = "deserialize_string_bool")]
    pub has_consented: bool,
}
impl Credentials {
    pub fn logout(&mut self) {
        self.api_key = String::new();
        self.has_consented = false;
    }
}

/// The directory in which all persistent config data should be stored.
pub fn get_persistent_dir() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| eyre!("Could not find user data directory"))?
        .join("OWL Control");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Config {
    #[serde(default)]
    pub credentials: Credentials,
    #[serde(default)]
    pub preferences: Preferences,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = match (Self::get_path(), Self::get_legacy_path()) {
            (Ok(path), _) if path.exists() => {
                tracing::info!("Loading from standard config path");
                path
            }
            (_, Ok(path)) if path.exists() => {
                tracing::info!("Loading from legacy config path");
                path
            }
            _ => {
                tracing::warn!("No config file found, using defaults");
                return Ok(Self::default());
            }
        };

        let contents = fs::read_to_string(&config_path).context("Failed to read config file")?;
        let mut config =
            serde_json::from_str::<Config>(&contents).context("Failed to parse config file")?;

        // Ensure hotkeys have default values if not set
        if config.preferences.start_recording_key.is_empty() {
            config.preferences.start_recording_key = default_start_key();
        }
        if config.preferences.stop_recording_key.is_empty() {
            config.preferences.stop_recording_key = default_stop_key();
        }

        Ok(config)
    }

    fn get_legacy_path() -> Result<PathBuf> {
        // Get user data directory (equivalent to app.getPath("userData"))
        let user_data_dir = dirs::data_dir()
            .ok_or_else(|| eyre!("Could not find user data directory"))?
            .join("vg-control");

        Ok(user_data_dir.join("config.json"))
    }

    fn get_path() -> Result<PathBuf> {
        Ok(get_persistent_dir()?.join("config.json"))
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::get_path()?;
        tracing::info!("Saving configs to {}", config_path.to_string_lossy());
        fs::write(&config_path, serde_json::to_string_pretty(&self)?)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VideoSettings {
    #[serde(default)]
    pub enc_specific: EncoderSpecific,
    #[serde(default = "default_bitrate")]
    pub bitrate: u32,
    #[serde(default = "default_profile")]
    pub profile: String,
    #[serde(default = "default_rate_control")]
    pub rate_control: String,
    #[serde(default = "default_bf")]
    pub bf: i64,
    #[serde(default = "default_psycho_aq")]
    pub psycho_aq: bool,
    #[serde(default = "default_lookahead")]
    pub lookahead: bool,
}

// Default values for VideoSettings fields
fn default_bitrate() -> u32 {
    2500
}
fn default_profile() -> String {
    "high".to_string()
}
fn default_rate_control() -> String {
    "cbr".to_string()
}
fn default_bf() -> i64 {
    2
}
fn default_psycho_aq() -> bool {
    true
}
fn default_lookahead() -> bool {
    true
}

/// Encoder-specific settings with variants for each encoder type
/// In order to keep it flexible and allow arbitrary custom encoder specific
/// fields while also storing it cleanly in an enum we utilise serde to convert
/// all enum variant named fields into a hashmap when required, as it's the only
/// way to expose an iterface to cleanly iterate through enum variant fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
#[allow(non_camel_case_types)]
pub enum EncoderSpecific {
    OBS_X264 { preset: String, tune: String },
    FFMPEG_NVENC { preset2: String, tune: String },
    // the rest are placeholder
    OBS_QSV11 { preset: String },
    OBS_QSV11_AV1 { preset: String },
    JIM_AV1_NVENC { preset: String },
    H265_TEXTURE_AMF { preset: String },
    FFMPEG_HEVC_NVENC { preset: String },
    H264_TEXTURE_AMF { preset: String },
    AV1_TEXTURE_AMF { preset: String },
    Other { preset: String },
}

impl Default for EncoderSpecific {
    fn default() -> Self {
        EncoderSpecific::OBS_X264 {
            preset: "high".to_string(),
            tune: "".to_string(),
        }
    }
}

impl EncoderSpecific {
    /// Convert variant fields to HashMap (excludes the "type" tag)
    pub fn to_hashmap(&self) -> HashMap<String, Value> {
        let json = serde_json::to_value(self).unwrap();
        match json {
            Value::Object(mut map) => {
                // Remove the "type" field since that's the variant name
                map.remove("type");
                map.into_iter().collect()
            }
            _ => HashMap::new(),
        }
    }
    /// Helper fn to get a specific field value
    #[allow(dead_code)]
    pub fn get_field(&self, field: &str) -> Option<Value> {
        self.to_hashmap().get(field).cloned()
    }
    /// Get a mutable reference to a specific field by name
    /// No way around it, egui needs to get a mutable reference to the actual enum value
    /// in order to update it based on user input.
    pub fn get_field_mut(&mut self, field: &str) -> Option<&mut String> {
        match (self, field) {
            (EncoderSpecific::OBS_X264 { preset, .. }, "preset") => Some(preset),
            (EncoderSpecific::OBS_X264 { tune, .. }, "tune") => Some(tune),
            (EncoderSpecific::FFMPEG_NVENC { preset2, .. }, "preset2") => Some(preset2),
            (EncoderSpecific::FFMPEG_NVENC { tune, .. }, "tune") => Some(tune),
            (EncoderSpecific::OBS_QSV11 { preset }, "preset") => Some(preset),
            (EncoderSpecific::OBS_QSV11_AV1 { preset }, "preset") => Some(preset),
            (EncoderSpecific::JIM_AV1_NVENC { preset }, "preset") => Some(preset),
            (EncoderSpecific::H265_TEXTURE_AMF { preset }, "preset") => Some(preset),
            (EncoderSpecific::FFMPEG_HEVC_NVENC { preset }, "preset") => Some(preset),
            (EncoderSpecific::H264_TEXTURE_AMF { preset }, "preset") => Some(preset),
            (EncoderSpecific::AV1_TEXTURE_AMF { preset }, "preset") => Some(preset),
            (EncoderSpecific::Other { preset }, "preset") => Some(preset),
            _ => None,
        }
    }
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            enc_specific: EncoderSpecific::default(),
            bitrate: default_bitrate(),
            profile: default_profile(),
            rate_control: default_rate_control(),
            bf: default_bf(),
            psycho_aq: default_psycho_aq(),
            lookahead: default_lookahead(),
        }
    }
}

impl VideoSettings {
    /// There's an argument here to be made that we could use the ObsVideoEncoderType::from_str() method
    /// with serde deserialization and accomplish this in a nicer way, but I think match arms just compile
    /// more efficiently and it's very explicitly readable.
    pub fn enc_type(&self) -> ObsVideoEncoderType {
        match &self.enc_specific {
            EncoderSpecific::OBS_X264 { .. } => ObsVideoEncoderType::OBS_X264,
            EncoderSpecific::FFMPEG_NVENC { .. } => ObsVideoEncoderType::FFMPEG_NVENC,
            EncoderSpecific::OBS_QSV11 { .. } => ObsVideoEncoderType::OBS_QSV11,
            EncoderSpecific::OBS_QSV11_AV1 { .. } => ObsVideoEncoderType::OBS_QSV11_AV1,
            EncoderSpecific::JIM_AV1_NVENC { .. } => ObsVideoEncoderType::JIM_AV1_NVENC,
            EncoderSpecific::H265_TEXTURE_AMF { .. } => ObsVideoEncoderType::H265_TEXTURE_AMF,
            EncoderSpecific::FFMPEG_HEVC_NVENC { .. } => ObsVideoEncoderType::FFMPEG_HEVC_NVENC,
            EncoderSpecific::H264_TEXTURE_AMF { .. } => ObsVideoEncoderType::H264_TEXTURE_AMF,
            EncoderSpecific::AV1_TEXTURE_AMF { .. } => ObsVideoEncoderType::AV1_TEXTURE_AMF,
            EncoderSpecific::Other { .. } => ObsVideoEncoderType::Other("other".to_string()),
        }
    }
    /// Generate an ordered list of encoder-specific field options for a given encoder type
    /// Returns a Vec of tuples where the first element is the field name (e.g., "preset", "tune")
    /// and the second is a Vec of possible options.
    /// This is used in the UI main.rs to allow user to select from dropdown box possible settings of the encoder
    /// I would have hoped to utilise .to_hashmap() to simplify this, but it's inevitable that *some* sort of
    /// mapping from ObsVideoEncoderType to the const array of possible values is done somewhere, so its done here.
    pub fn get_encoder_field_options(&self) -> Vec<(String, Vec<String>)> {
        match self.enc_type() {
            ObsVideoEncoderType::OBS_X264 => {
                vec![(
                    "preset".to_string(),
                    X264_PRESETS.iter().map(|s| s.to_string()).collect(),
                )]
                // I don't think users *should* have access to x264 tune options
                // it should stay as the default "" none.
            }
            ObsVideoEncoderType::FFMPEG_NVENC => {
                vec![
                    (
                        "preset2".to_string(),
                        NVENC_PRESETS.iter().map(|s| s.to_string()).collect(),
                    ),
                    (
                        "tune".to_string(),
                        NVENC_TUNE_OPTIONS.iter().map(|s| s.to_string()).collect(),
                    ),
                ]
            }
            ObsVideoEncoderType::FFMPEG_HEVC_NVENC => {
                vec![(
                    "preset".to_string(),
                    NVENC_PRESETS.iter().map(|s| s.to_string()).collect(),
                )]
            }
            ObsVideoEncoderType::JIM_AV1_NVENC => {
                vec![(
                    "preset".to_string(),
                    NVENC_PRESETS.iter().map(|s| s.to_string()).collect(),
                )]
            }
            ObsVideoEncoderType::OBS_QSV11 | ObsVideoEncoderType::OBS_QSV11_AV1 => {
                vec![(
                    "preset".to_string(),
                    QSV_PRESETS.iter().map(|s| s.to_string()).collect(),
                )]
            }
            ObsVideoEncoderType::H264_TEXTURE_AMF
            | ObsVideoEncoderType::H265_TEXTURE_AMF
            | ObsVideoEncoderType::AV1_TEXTURE_AMF => {
                vec![(
                    "preset".to_string(),
                    AMF_PRESETS.iter().map(|s| s.to_string()).collect(),
                )]
            }
            _ => {
                // Default fallback
                vec![(
                    "preset".to_string(),
                    NVENC_PRESETS.iter().map(|s| s.to_string()).collect(),
                )]
            }
        }
    }

    /// Apply video encoder settings to ObsData based on encoder type.
    /// Sets common settings first, then applies encoder-specific settings.
    /// Because for some fuckshit reason obs-ffmpeg-nvenc uses "preset2" instead of just "preset"
    /// https://github.com/obsproject/obs-studio/blob/0b1229632063a13dfd26cf1cd9dd43431d8c68f6/plugins/obs-ffmpeg/obs-ffmpeg-nvenc.c#L417
    pub async fn apply_encoder_settings(
        &self,
        mut data: libobs_wrapper::data::ObsData,
    ) -> color_eyre::Result<libobs_wrapper::data::ObsData> {
        // Apply common settings shared by all encoders
        let mut updater = data.bulk_update();
        updater = updater
            .set_int("bitrate", self.bitrate.into())
            .set_string("rate_control", self.rate_control.as_str())
            .set_string("profile", self.profile.as_str())
            .set_int("bf", self.bf)
            .set_bool("psycho_aq", self.psycho_aq)
            .set_bool("lookahead", self.lookahead);

        // iterate through encoder specific enum variant fields to assign them
        for (field_name, value) in self.enc_specific.to_hashmap() {
            updater = updater.set_string(field_name, value.to_string());
        }
        updater.update().await?;

        Ok(data)
    }
}
