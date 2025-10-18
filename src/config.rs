use color_eyre::eyre::{Context, Result, eyre};
use constants::obs::{NVENC_PRESETS, NVENC_TUNE_OPTIONS, X264_PRESETS};
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
    pub video_settings: EncoderSettings,
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

pub fn encoder_type_display_name(encoder_type: &ObsVideoEncoderType) -> &'static str {
    match encoder_type {
        ObsVideoEncoderType::OBS_X264 => "OBS x264 (CPU)",
        ObsVideoEncoderType::FFMPEG_NVENC => "NVIDIA NVENC (GPU)",
        _ => "HONK",
    }
}

/// Base struct containing common video encoder settings shared across all encoders
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct EncoderSettings {
    /// Encoder type
    #[serde(
        serialize_with = "serialize_encoder_type",
        deserialize_with = "deserialize_encoder_type"
    )]
    pub encoder: ObsVideoEncoderType,

    /// Encoder specific settings
    pub x264: ObsX264Settings,
    pub nvenc: FfmpegNvencSettings,

    /// Shared encoder settings
    pub bitrate: u32,
    pub profile: String,
    pub rate_control: String,
    pub bf: i64,
    pub psycho_aq: bool,
    pub lookahead: bool,
}
impl Default for EncoderSettings {
    fn default() -> Self {
        Self {
            encoder: ObsVideoEncoderType::OBS_X264,
            x264: Default::default(),
            nvenc: Default::default(),
            bitrate: 2500,
            profile: "high".to_string(),
            rate_control: "cbr".to_string(),
            bf: 2,
            psycho_aq: true,
            lookahead: true,
        }
    }
}
/// convert encoder specific settings (which impl serde Serialize) to HashMap
fn to_hashmap<T: Serialize>(settings: &T) -> HashMap<String, Value> {
    let json = serde_json::to_value(settings).unwrap();
    match json {
        Value::Object(map) => map.into_iter().collect(),
        _ => HashMap::new(),
    }
}
impl EncoderSettings {
    /// Apply encoder settings to ObsData
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

        let encoder_settings = match self.encoder {
            ObsVideoEncoderType::OBS_X264 => to_hashmap(&self.x264),
            ObsVideoEncoderType::FFMPEG_NVENC => to_hashmap(&self.nvenc),
            _ => to_hashmap(&self.x264),
        };
        // Apply encoder-specific settings
        for (field_name, value) in encoder_settings {
            updater = updater.set_string(field_name, value.as_str().unwrap_or(""));
        }
        updater.update().await?;

        Ok(data)
    }

    pub fn display_name(&self) -> &'static str {
        encoder_type_display_name(&self.encoder)
    }

    /// Generate an ordered list of encoder-specific field options for a given encoder type
    /// Returns a Vec of tuples where the first element is the field name (e.g., "preset", "tune")
    /// and the second is a Vec of possible options.
    /// This is used in the UI main.rs to allow user to select from dropdown box possible settings of the encoder
    /// I would have hoped to utilise .to_hashmap() to simplify this, but it's inevitable that *some* sort of
    /// mapping from ObsVideoEncoderType to the const array of possible values is done somewhere, so its done here.
    pub fn get_encoder_field_options(&self) -> Vec<(String, Vec<String>)> {
        match self.encoder {
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
            _ => {
                // Default fallback
                vec![(
                    "preset".to_string(),
                    NVENC_PRESETS.iter().map(|s| s.to_string()).collect(),
                )]
            }
        }
    }

    /// Get a mutable reference to a specific field by name
    /// No way around it, egui needs to get a mutable reference to the actual enum value
    /// in order to update it based on user input.
    pub fn get_field_mut(&mut self, field: &str) -> Option<&mut String> {
        match (&self.encoder, field) {
            (ObsVideoEncoderType::OBS_X264, "preset") => Some(&mut self.x264.preset),
            (ObsVideoEncoderType::OBS_X264, "tune") => Some(&mut self.x264.tune),
            (ObsVideoEncoderType::FFMPEG_NVENC, "preset2") => Some(&mut self.nvenc.preset2),
            (ObsVideoEncoderType::FFMPEG_NVENC, "tune") => Some(&mut self.nvenc.tune),
            _ => None,
        }
    }
}
/// very convenient that libobs provides inbuilt from_str conversion for ObsVideoEncoderType that we can use for serialization
fn serialize_encoder_type<S>(
    encoder_type: &ObsVideoEncoderType,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use libobs_wrapper::utils::ObsString;
    let obs_string: ObsString = encoder_type.clone().into();
    serializer.serialize_str(&obs_string.to_string())
}
fn deserialize_encoder_type<'de, D>(deserializer: D) -> Result<ObsVideoEncoderType, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(ObsVideoEncoderType::from_str(&s).unwrap())
}

/// OBS x264 (CPU) encoder specific settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ObsX264Settings {
    pub preset: String,
    pub tune: String,
}
impl Default for ObsX264Settings {
    fn default() -> Self {
        Self {
            preset: "veryfast".to_string(),
            tune: String::new(),
        }
    }
}

/// NVENC (NVIDIA GPU) encoder specific settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct FfmpegNvencSettings {
    pub preset2: String,
    pub tune: String,
}
impl Default for FfmpegNvencSettings {
    fn default() -> Self {
        Self {
            preset2: "p5".to_string(),
            tune: "hq".to_string(),
        }
    }
}
