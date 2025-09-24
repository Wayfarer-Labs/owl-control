use chrono::{DateTime, Utc};
use reqwest;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

use crate::config_manager::UploadStats;

// Define the API base URL as a constant
const API_BASE_URL: &str = "https://api.openworldlabs.ai"; // Replace with actual URL

// Response struct for the user info endpoint
#[derive(Deserialize)]
struct UserIDResponse {
    #[serde(rename = "userId")]
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
    video_duration_seconds: Option<u64>, // Null in your examples, but could be a number
}
// Custom result types for better error handling
#[derive(Debug, Serialize)]
pub struct ValidationSuccess {
    pub success: bool,
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct ValidationError {
    pub success: bool,
    pub message: Option<String>,
}

pub struct ApiClient {
    api_key: Option<String>,
    user_id: Option<String>,
    client: reqwest::Client,
}

impl ApiClient {
    pub fn new() -> Self {
        Self {
            api_key: None,
            user_id: None,
            client: reqwest::Client::new(),
        }
    }

    pub async fn validate_api_key(
        &mut self,
        api_key: &str,
    ) -> Result<ValidationSuccess, ValidationError> {
        // Validate input
        if api_key.is_empty() || api_key.trim().is_empty() {
            return Err(ValidationError {
                success: false,
                message: Some("API key cannot be empty".to_string()),
            });
        }

        // Simple validation - check if it starts with 'sk_'
        if !api_key.starts_with("sk_") {
            return Err(ValidationError {
                success: false,
                message: Some("Invalid API key format".to_string()),
            });
        }

        // Make the API request
        let url = format!("{}/api/v1/user/info", API_BASE_URL);

        match self
            .client
            .get(&url)
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .send()
            .await
        {
            Ok(response) => {
                if !response.status().is_success() {
                    let error_msg = format!(
                        "Invalid API key, or server unavailable: {}",
                        response.status()
                    );
                    return Err(ValidationError {
                        success: false,
                        message: Some(error_msg),
                    });
                }

                // Parse the JSON response
                match response.json::<UserIDResponse>().await {
                    Ok(user_info) => {
                        println!(
                            "validateApiKey: Successfully validated API key - user ID: {}",
                            user_info.user_id
                        );

                        // Store the API key and user ID
                        self.api_key = Some(api_key.to_string());
                        self.user_id = Some(user_info.user_id.clone());

                        Ok(ValidationSuccess {
                            success: true,
                            user_id: user_info.user_id,
                        })
                    }
                    Err(e) => {
                        eprintln!("validateApiKey: Failed to parse response: {}", e);
                        Err(ValidationError {
                            success: false,
                            message: Some("API key validation failed".to_string()),
                        })
                    }
                }
            }
            Err(e) => {
                eprintln!("validateApiKey: API key validation error: {}", e);
                Err(ValidationError {
                    success: false,
                    message: Some("API key validation failed".to_string()),
                })
            }
        }
    }

    pub async fn get_user_upload_stats(&self) -> Result<UploadStats, ValidationError> {
        let url = format!(
            "{}/tracker/uploads/user/{}",
            API_BASE_URL,
            &self.user_id.clone().unwrap()
        );

        match self
            .client
            .get(&url)
            .header("Content-Type", "application/json")
            .header("X-API-Key", &self.api_key.clone().unwrap())
            .send()
            .await
        {
            Ok(response) => {
                let server_stats = response.json::<UserStatsResponse>().await.unwrap();
                let mut stats = UploadStats::default();

                stats.total_files_uploaded = server_stats.statistics.total_uploads;
                stats.total_duration_uploaded =
                    server_stats.statistics.total_video_time.seconds.into();
                stats.total_volume_uploaded = server_stats.statistics.total_data.megabytes as u64;
                stats.last_upload_date = match &server_stats.uploads[0] {
                    upload => upload.created_at.to_rfc3339(),
                    _ => "Never".to_string(),
                };
                Ok(stats)
            }
            Err(err) => Err(ValidationError {
                success: false,
                message: Some("Request failed".to_string()),
            }),
        }
    }
}
