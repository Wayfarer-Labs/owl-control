use color_eyre::eyre::{Context, Result, eyre};
use constants::encoding::VideoEncoderType;
use serde::{Deserialize, Deserializer, Serialize};
use std::{fs, path::PathBuf};

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
    pub encoder: EncoderSettings,
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
            encoder: Default::default(),
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
        Ok(get_persistent_dir()?.join(constants::filename::persistent::CONFIG))
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::get_path()?;
        tracing::info!("Saving configs to {}", config_path.to_string_lossy());
        fs::write(&config_path, serde_json::to_string_pretty(&self)?)?;
        Ok(())
    }
}

/// Base struct containing common video encoder settings shared across all encoders
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct EncoderSettings {
    /// Encoder type
    pub encoder: VideoEncoderType,

    /// Encoder specific settings
    pub x264: ObsX264Settings,
    pub nvenc: FfmpegNvencSettings,
}
impl Default for EncoderSettings {
    fn default() -> Self {
        Self {
            encoder: VideoEncoderType::X264,
            x264: Default::default(),
            nvenc: Default::default(),
        }
    }
}
impl EncoderSettings {
    /// Apply encoder settings to ObsData
    pub fn apply_to_obs_data(
        &self,
        mut data: libobs_wrapper::data::ObsData,
    ) -> color_eyre::Result<libobs_wrapper::data::ObsData> {
        // Apply common settings shared by all encoders
        let mut updater = data.bulk_update();
        updater = updater
            .set_int("bitrate", constants::encoding::BITRATE)
            .set_string("rate_control", constants::encoding::RATE_CONTROL)
            .set_string("profile", constants::encoding::VIDEO_PROFILE)
            .set_int("bf", constants::encoding::B_FRAMES)
            .set_bool("psycho_aq", constants::encoding::PSYCHO_AQ)
            .set_bool("lookahead", constants::encoding::LOOKAHEAD);

        updater = match self.encoder {
            VideoEncoderType::X264 => self.x264.apply_to_data_updater(updater),
            VideoEncoderType::NvEnc => self.nvenc.apply_to_data_updater(updater),
        };
        updater.update()?;

        Ok(data)
    }
}

/// OBS x264 (CPU) encoder specific settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ObsX264Settings {
    pub preset: String,
    pub tune: String,
}
impl Default for ObsX264Settings {
    fn default() -> Self {
        Self {
            preset: constants::encoding::X264_PRESETS[0].to_string(),
            tune: String::new(),
        }
    }
}
impl ObsX264Settings {
    fn apply_to_data_updater(
        &self,
        updater: libobs_wrapper::data::ObsDataUpdater,
    ) -> libobs_wrapper::data::ObsDataUpdater {
        updater
            .set_string("preset", self.preset.as_str())
            .set_string("tune", self.tune.as_str())
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
            preset2: constants::encoding::NVENC_PRESETS[0].to_string(),
            tune: constants::encoding::NVENC_TUNE_OPTIONS[0].to_string(),
        }
    }
}
impl FfmpegNvencSettings {
    fn apply_to_data_updater(
        &self,
        updater: libobs_wrapper::data::ObsDataUpdater,
    ) -> libobs_wrapper::data::ObsDataUpdater {
        updater
            .set_string("preset2", self.preset2.as_str())
            .set_string("tune", self.tune.as_str())
    }
}
