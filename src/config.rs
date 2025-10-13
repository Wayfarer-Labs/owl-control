use color_eyre::eyre::{Context, Result, eyre};
use serde::{Deserialize, Deserializer, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// camel case renames are legacy from old existing configs, we want it to be backwards-compatible with previous owl releases that used electron
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default = "default_start_key")]
    pub start_recording_key: String,
    #[serde(default = "default_stop_key")]
    pub stop_recording_key: String,
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
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            start_recording_key: default_start_key(),
            stop_recording_key: default_stop_key(),
            unreliable_connection: Default::default(),
            overlay_location: Default::default(),
            overlay_opacity: default_opacity(),
            delete_uploaded_files: Default::default(),
            honk: Default::default(),
            recording_backend: Default::default(),
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

fn default_start_key() -> String {
    "F4".to_string()
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

// Define the structure of your upload stats
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UploadStats {
    #[serde(rename = "totalDurationUploaded")]
    pub total_duration_uploaded: f64,
    #[serde(rename = "totalFilesUploaded")]
    pub total_files_uploaded: u64,
    #[serde(rename = "totalVolumeUploaded")]
    pub total_volume_uploaded: u64,
    #[serde(rename = "lastUploadDate")]
    pub last_upload_date: LastUploadDate,
}

impl Default for UploadStats {
    fn default() -> Self {
        Self {
            total_duration_uploaded: 0.,
            total_files_uploaded: 0,
            total_volume_uploaded: 0,
            last_upload_date: LastUploadDate::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LastUploadDate {
    None,
    Never,
    Date(chrono::DateTime<chrono::Local>),
}
impl serde::Serialize for LastUploadDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            LastUploadDate::None => serializer.serialize_str("None"),
            LastUploadDate::Never => serializer.serialize_str("Never"),
            LastUploadDate::Date(s) => serializer.serialize_str(&s.to_rfc3339()),
        }
    }
}
impl<'de> serde::Deserialize<'de> for LastUploadDate {
    fn deserialize<D>(deserializer: D) -> Result<LastUploadDate, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct LastUploadDateVisitor;
        impl<'de> serde::de::Visitor<'de> for LastUploadDateVisitor {
            type Value = LastUploadDate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string representing last upload date")
            }

            fn visit_str<E>(self, value: &str) -> Result<LastUploadDate, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "None" => Ok(LastUploadDate::None),
                    "Never" => Ok(LastUploadDate::Never),
                    other => Ok(chrono::DateTime::parse_from_rfc3339(other)
                        .map(|d| d.with_timezone(&chrono::Local))
                        .map(LastUploadDate::Date)
                        .unwrap_or(LastUploadDate::None)),
                }
            }
        }

        deserializer.deserialize_str(LastUploadDateVisitor)
    }
}
impl LastUploadDate {
    pub fn as_date(&self) -> Option<chrono::DateTime<chrono::Local>> {
        match self {
            LastUploadDate::Date(d) => Some(*d),
            _ => None,
        }
    }
}

impl UploadStats {
    pub fn load() -> Result<Self> {
        let path = match (Self::get_path(), Self::get_legacy_path()) {
            (Ok(path), _) if path.exists() => {
                tracing::info!("Loading from standard upload stats path");
                path
            }
            (_, path) if path.exists() => {
                tracing::info!("Loading from legacy upload stats path");
                path
            }
            _ => {
                tracing::info!("Upload stats file doesn't exist, keeping current stats");
                return Ok(Self::default());
            }
        };

        // Parse JSON
        serde_json::from_str(&fs::read_to_string(&path)?).context("Failed to parse upload stats")
    }

    fn get_path() -> Result<PathBuf> {
        Ok(get_persistent_dir()?.join("upload-stats.json"))
    }

    fn get_legacy_path() -> PathBuf {
        env::temp_dir().join("owl-control-upload-stats.json")
    }

    pub fn save(&self) -> Result<()> {
        fs::write(Self::get_path()?, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn update(
        &mut self,
        additional_duration: f64,
        additional_files: u64,
        additional_volume: u64,
    ) -> Result<()> {
        self.total_duration_uploaded += additional_duration;
        self.total_files_uploaded += additional_files;
        self.total_volume_uploaded += additional_volume;
        self.last_upload_date = LastUploadDate::Date(chrono::Local::now());
        self.save()
    }
}
