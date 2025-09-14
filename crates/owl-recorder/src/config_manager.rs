use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    // camel case renames are legacy from old existing configs, we want it to be backwards-compatible with previous owl releases that used electron
    #[serde(rename = "startRecordingKey", default = "default_start_key")]
    pub start_recording_key: String,
    #[serde(rename = "stopRecordingKey", default = "default_stop_key")]
    pub stop_recording_key: String,
    #[serde(rename = "apiToken", default)]
    pub api_token: String,
    #[serde(rename = "deleteUploadedFiles", default)]
    pub delete_uploaded_files: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            start_recording_key: "F4".to_string(),
            stop_recording_key: "F5".to_string(),
            api_token: String::new(),
            delete_uploaded_files: false,
        }
    }
}

fn default_start_key() -> String {
    "F4".to_string()
}

fn default_stop_key() -> String {
    "F5".to_string()
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(
        rename = "hasConsented",
        default,
        deserialize_with = "deserialize_string_bool"
    )]
    pub has_consented: bool,
}

impl Default for Credentials {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            has_consented: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigManager {
    #[serde(default)]
    pub credentials: Credentials,
    #[serde(default)]
    pub preferences: Preferences,
    #[serde(skip)]
    pub config_path: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
        let config_path = Self::get_config_path()?;
        let mut manager = Self {
            credentials: Credentials::default(),
            preferences: Preferences::default(),
            config_path,
        };
        manager.load_config()?;
        Ok(manager)
    }

    fn get_config_path() -> Result<PathBuf> {
        // Get user data directory (equivalent to app.getPath("userData"))
        let user_data_dir = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find user data directory"))?
            .join("vg-control");
        tracing::info!(
            "Config manager using data dir: {}",
            user_data_dir.to_string_lossy()
        );
        // Create directory if it doesn't exist
        fs::create_dir_all(&user_data_dir)?;

        Ok(user_data_dir.join("config.json"))
    }

    pub fn load_config(&mut self) -> Result<()> {
        if !self.config_path.exists() {
            // Config doesn't exist, use defaults
            return Ok(());
        }

        match fs::read_to_string(&self.config_path) {
            Ok(contents) => {
                match serde_json::from_str::<ConfigManager>(&contents) {
                    Ok(mut config) => {
                        // Preserve the config_path
                        config.config_path = self.config_path.clone();
                        *self = config;

                        // Ensure hotkeys have default values if not set
                        if self.preferences.start_recording_key.is_empty() {
                            self.preferences.start_recording_key = "F4".to_string();
                        }
                        if self.preferences.stop_recording_key.is_empty() {
                            self.preferences.stop_recording_key = "F5".to_string();
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

    pub fn save_config(&self) -> Result<()> {
        let contents = serde_json::to_string_pretty(&self)?;
        fs::write(&self.config_path, contents)?;
        Ok(())
    }

    // Credential getters
    pub fn get_api_key(&self) -> &str {
        &self.credentials.api_key
    }

    pub fn has_consented(&self) -> bool {
        self.credentials.has_consented
    }

    // Preference getters
    pub fn get_start_recording_key(&self) -> &str {
        &self.preferences.start_recording_key
    }

    pub fn get_stop_recording_key(&self) -> &str {
        &self.preferences.stop_recording_key
    }

    pub fn get_api_token(&self) -> &str {
        &self.preferences.api_token
    }

    pub fn get_delete_uploaded_files(&self) -> bool {
        self.preferences.delete_uploaded_files
    }

    // Credential setters
    pub fn set_api_key(&mut self, api_key: String) {
        self.credentials.api_key = api_key;
    }

    pub fn set_has_consented(&mut self, consented: bool) {
        self.credentials.has_consented = consented;
    }

    // Preference setters
    pub fn set_start_recording_key(&mut self, key: String) {
        self.preferences.start_recording_key = key;
    }

    pub fn set_stop_recording_key(&mut self, key: String) {
        self.preferences.stop_recording_key = key;
    }

    pub fn set_api_token(&mut self, token: String) {
        self.preferences.api_token = token;
    }

    pub fn set_delete_uploaded_files(&mut self, delete: bool) {
        self.preferences.delete_uploaded_files = delete;
    }
}
