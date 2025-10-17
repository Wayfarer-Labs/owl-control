use color_eyre::eyre::{Context, Result, eyre};
use serde::{Deserialize, Deserializer, Serialize};
use std::{fs, path::PathBuf, str::FromStr};

use libobs_wrapper::encoders::ObsVideoEncoderType;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VideoSettings {
    #[serde(
        serialize_with = "serialize_encoder_type",
        deserialize_with = "deserialize_encoder_type"
    )]
    pub encoder_type: ObsVideoEncoderType,
    pub bitrate: u32,
    pub preset: String,
    pub tune: String,
    pub profile: String,
    pub rate_control: String,
    pub bf: i64,
    pub psycho_aq: bool,
    pub lookahead: bool,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            encoder_type: ObsVideoEncoderType::OBS_X264,
            bitrate: 2500,
            preset: "p5".to_string(),
            tune: "hq".to_string(),
            profile: "high".to_string(),
            rate_control: "cbr".to_string(),
            bf: 2,
            psycho_aq: true,
            lookahead: true,
        }
    }
}

impl VideoSettings {
    /// Apply video encoder settings to ObsData based on encoder type.
    /// Sets common settings first, then applies encoder-specific settings.
    /// Because for some fuckshit reason obs-ffmpeg-nvenc uses "preset2" instead of just "preset"
    /// https://github.com/obsproject/obs-studio/blob/0b1229632063a13dfd26cf1cd9dd43431d8c68f6/plugins/obs-ffmpeg/obs-ffmpeg-nvenc.c#L417
    /// I suspect that in the future if we want to support more encoders they will have their own
    /// custom settings strings, so I made it extensible for encoder-specific settings that have different names
    /// you can just add onto the match arm.
    pub async fn apply_encoder_settings(
        &self,
        mut data: libobs_wrapper::data::ObsData,
    ) -> color_eyre::Result<libobs_wrapper::data::ObsData> {
        use libobs_wrapper::encoders::ObsVideoEncoderType;

        // Apply common settings shared by all encoders
        data.bulk_update()
            .set_int("bitrate", self.bitrate.into())
            .set_string("rate_control", self.rate_control.as_str())
            .set_string("profile", self.profile.as_str())
            .set_int("bf", self.bf)
            .set_bool("psycho_aq", self.psycho_aq)
            .set_bool("lookahead", self.lookahead)
            .update()
            .await?;

        // Apply encoder-specific settings
        match &self.encoder_type {
            ObsVideoEncoderType::OBS_X264 => {
                data.bulk_update()
                    .set_string("preset", self.preset.as_str())
                    .update()
                    .await?;
            }
            ObsVideoEncoderType::FFMPEG_NVENC => {
                data.bulk_update()
                    .set_string("preset2", self.preset.as_str())
                    .update()
                    .await?;
            }
            ObsVideoEncoderType::OBS_QSV11
            | ObsVideoEncoderType::OBS_QSV11_AV1
            | ObsVideoEncoderType::JIM_AV1_NVENC
            | ObsVideoEncoderType::H265_TEXTURE_AMF
            | ObsVideoEncoderType::FFMPEG_HEVC_NVENC
            | ObsVideoEncoderType::H264_TEXTURE_AMF
            | ObsVideoEncoderType::AV1_TEXTURE_AMF
            | ObsVideoEncoderType::Other(_) => {
                // For other encoders, apply all available settings
                // They will be ignored by the encoder if not applicable
                data.bulk_update()
                    .set_string("preset", self.preset.as_str())
                    .update()
                    .await?;
            }
        }

        Ok(data)
    }
}

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

// very convenient that libobs provides inbuilt from_str conversion for ObsVideoEncoderType that we can use for serialization
fn deserialize_encoder_type<'de, D>(deserializer: D) -> Result<ObsVideoEncoderType, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(ObsVideoEncoderType::from_str(&s).unwrap())
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
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
