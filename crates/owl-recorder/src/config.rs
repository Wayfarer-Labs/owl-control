use chrono::{DateTime, Local};
use color_eyre::eyre::{Result, eyre};
use serde::{Deserialize, Deserializer, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// camel case renames are legacy from old existing configs, we want it to be backwards-compatible with previous owl releases that used electron
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default = "default_start_key")]
    pub start_recording_key: String,
    #[serde(default = "default_stop_key")]
    pub stop_recording_key: String,
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
            overlay_opacity: default_opacity(),
            delete_uploaded_files: false,
            honk: false,
            recording_backend: RecordingBackend::default(),
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum RecordingBackend {
    #[default]
    Embedded,
    Socket,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub credentials: Credentials,
    #[serde(default)]
    pub preferences: Preferences,
    #[serde(skip)]
    pub config_path: PathBuf,
}

impl Config {
    pub fn new() -> Result<Self> {
        let config_path = Self::get_path()?;
        let mut manager = Self {
            credentials: Credentials::default(),
            preferences: Preferences::default(),
            config_path,
        };
        manager.load()?;
        Ok(manager)
    }

    fn get_path() -> Result<PathBuf> {
        // Get user data directory (equivalent to app.getPath("userData"))
        let user_data_dir = dirs::data_dir()
            .ok_or_else(|| eyre!("Could not find user data directory"))?
            .join("vg-control");
        tracing::info!(
            "Config manager using data dir: {}",
            user_data_dir.to_string_lossy()
        );
        // Create directory if it doesn't exist
        fs::create_dir_all(&user_data_dir)?;

        Ok(user_data_dir.join("config.json"))
    }

    pub fn load(&mut self) -> Result<()> {
        if !self.config_path.exists() {
            // Config doesn't exist, use defaults
            return Ok(());
        }

        match fs::read_to_string(&self.config_path) {
            Ok(contents) => {
                match serde_json::from_str::<Config>(&contents) {
                    Ok(mut config) => {
                        // Preserve the config_path
                        config.config_path = self.config_path.clone();
                        *self = config;

                        // Ensure hotkeys have default values if not set
                        if self.preferences.start_recording_key.is_empty() {
                            self.preferences.start_recording_key = default_start_key();
                        }
                        if self.preferences.stop_recording_key.is_empty() {
                            self.preferences.stop_recording_key = default_stop_key();
                        }
                    }
                    Err(e) => {
                        eprintln!("Error parsing config: {}", e);
                        // Keep defaults on parse error
                    }
                }
            }
            Err(e) => {
                eprintln!("Error loading config: {}", e);
            }
        }

        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        tracing::info!("Saving configs to {}", self.config_path.to_string_lossy());
        let contents = serde_json::to_string_pretty(&self)?;
        fs::write(&self.config_path, contents)?;
        Ok(())
    }
}

// Define the structure of your upload stats
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadStats {
    #[serde(rename = "totalDurationUploaded")]
    pub total_duration_uploaded: f64,
    #[serde(rename = "totalFilesUploaded")]
    pub total_files_uploaded: u64,
    #[serde(rename = "totalVolumeUploaded")]
    pub total_volume_uploaded: u64,
    #[serde(rename = "lastUploadDate")]
    pub last_upload_date: String,
}

impl Default for UploadStats {
    fn default() -> Self {
        Self {
            total_duration_uploaded: 0.,
            total_files_uploaded: 0,
            total_volume_uploaded: 0,
            last_upload_date: String::from("None"),
        }
    }
}

impl UploadStats {
    pub fn new() -> Result<Self> {
        let mut upload_stats = Self::default();
        upload_stats.load_from_disk()?;
        Ok(upload_stats)
    }

    pub fn load_from_disk(&mut self) -> Result<()> {
        let mut path = env::temp_dir();
        path.push("owl-control-upload-stats.json");

        // Check if file exists
        if !path.exists() {
            tracing::info!("Upload stats file doesn't exist, keeping current stats");
            return Ok(());
        }
        tracing::info!("Upload stats file found {:?}", path);

        // Read the file contents
        let json_string = fs::read_to_string(&path)?;
        tracing::info!("Upload stats read");
        // Parse JSON
        let stats: UploadStats =
            serde_json::from_str(&json_string).expect("Failed to parse upload stats");
        tracing::info!("Upload stats parsed");

        self.total_duration_uploaded = stats.total_duration_uploaded;
        self.total_files_uploaded = stats.total_files_uploaded;
        self.total_volume_uploaded = stats.total_volume_uploaded;
        self.last_upload_date = stats.last_upload_date;

        tracing::info!("Loaded upload stats: {:?}", self);
        Ok(())
    }

    pub fn get_total_duration_uploaded(&self) -> String {
        let seconds = self.total_duration_uploaded as u64;
        if seconds == 0 {
            return String::from("0 min");
        };

        // rust int div is floor
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        if hours > 0 && minutes > 0 {
            format!("{}h {}m", &hours, &minutes)
        } else if hours > 0 {
            format!("{}h", hours)
        } else {
            format!("{}m", minutes)
        }
    }

    pub fn get_total_files_uploaded(&self) -> String {
        self.total_files_uploaded.to_string()
    }

    pub fn get_total_volume_uploaded(&self) -> String {
        if self.total_volume_uploaded == 0 {
            return String::from("0 MB");
        }

        let k = 1024_f64;
        let mb = self.total_volume_uploaded as f64 / (k * k);
        let gb = mb / k;

        if gb >= 1.0 {
            format!("{:.1} GB", gb)
        } else {
            format!("{:.1} MB", mb)
        }
    }

    pub fn get_last_upload_date(&self) -> String {
        if self.last_upload_date == "Never" {
            return "Never".to_string();
        };

        DateTime::parse_from_rfc3339(&self.last_upload_date)
            .map(|dt| dt.with_timezone(&Local))
            .map(|dt| format!("{} at {}", dt.format("%m/%d/%Y"), dt.format("%I:%M:%S %p")))
            .unwrap_or_else(|_| "Unknown".to_string())
    }
}
