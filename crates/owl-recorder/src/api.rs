#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::config::{LastUploadDate, UploadStats};

const API_BASE_URL: &str = "https://api.openworldlabs.ai";

// Response struct for the user info endpoint
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserIdResponse {
    user_id: String,
}

// Response structs for the user info endpoint
#[derive(Deserialize, Debug)]
struct UserStatsResponse {
    success: bool,
    user_id: String,
    statistics: Statistics,
    uploads: Vec<Upload>, // idk format for this one
}

#[derive(Deserialize, Debug)]
struct Statistics {
    total_uploads: u64,
    total_data: DataSize,
    total_video_time: VideoTime,
    verified_uploads: u32,
}

#[derive(Deserialize, Debug)]
struct DataSize {
    bytes: u64,
    megabytes: f64,
    gigabytes: f64,
}

#[derive(Deserialize, Debug)]
struct VideoTime {
    seconds: u32,
    minutes: f64,
    hours: f64,
    formatted: String,
}

#[derive(Deserialize, Debug)]
struct Upload {
    content_type: String,
    created_at: DateTime<Utc>,
    file_size_bytes: u64,
    file_size_mb: f64,
    filename: String,
    id: String,
    tags: Option<serde_json::Value>,
    verified: bool,
    video_duration_seconds: Option<u64>,
}

pub struct ApiClient {
    client: reqwest::Client,
}
impl ApiClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Attempts to validate the API key. Returns an error if the API key is invalid or the server is unavailable.
    /// Returns the user ID if the API key is valid.
    pub async fn validate_api_key(&self, api_key: String) -> Result<String, String> {
        let client = self.client.clone();

        // Validate input
        if api_key.is_empty() || api_key.trim().is_empty() {
            return Err("API key cannot be empty".to_string());
        }

        // Simple validation - check if it starts with 'sk_'
        if !api_key.starts_with("sk_") {
            return Err("Invalid API key format".to_string());
        }

        // Make the API request
        let url = format!("{}/api/v1/user/info", API_BASE_URL);

        let response = client
            .get(&url)
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .send()
            .await
            .map_err(|e| format!("API key validation error: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "Invalid API key, or server unavailable: {}",
                response.status()
            ));
        }

        // Parse the JSON response
        let user_info = response
            .json::<UserIdResponse>()
            .await
            .map_err(|e| format!("API key validation error: {e}"))?;

        Ok(user_info.user_id)
    }

    pub async fn get_user_upload_stats(
        &self,
        api_key: &str,
        user_id: &str,
    ) -> Result<UploadStats, String> {
        let url = format!("{}/tracker/uploads/user/{}", API_BASE_URL, user_id);

        let response = self
            .client
            .get(&url)
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .send()
            .await
            .map_err(|e| format!("Get user upload stats failed: {e}"))?;

        let server_stats = response
            .json::<UserStatsResponse>()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        Ok(UploadStats {
            total_duration_uploaded: server_stats.statistics.total_video_time.seconds.into(),
            total_files_uploaded: server_stats.statistics.total_uploads,
            total_volume_uploaded: server_stats.statistics.total_data.megabytes as u64,
            last_upload_date: match server_stats.uploads.first() {
                Some(upload) => {
                    LastUploadDate::Date(upload.created_at.with_timezone(&chrono::Local))
                }
                None => LastUploadDate::Never,
            },
        })
    }
}
