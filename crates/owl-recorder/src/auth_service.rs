#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::{
    app_state::{Command, CommandSender},
    config::UploadStats,
};

const API_BASE_URL: &str = "https://api.openworldlabs.ai";

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
    video_duration_seconds: Option<u64>,
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

#[derive(Clone)]
pub struct ApiClient {
    sender: CommandSender,
    client: reqwest::Client,
}

impl ApiClient {
    pub fn new(sender: CommandSender) -> Self {
        Self {
            sender,
            client: reqwest::Client::new(),
        }
    }

    pub async fn validate_api_key(
        &mut self,
        api_key: String,
    ) -> Result<ValidationSuccess, ValidationError> {
        let sender = self.sender.clone();
        let client = self.client.clone();

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

        match client
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

                        let _ = sender.try_send(Command::UpdateUserID(user_info.user_id.clone()));

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

    pub async fn get_user_upload_stats(
        &self,
        api_key: &str,
        user_id: &str,
    ) -> Result<UploadStats, ValidationError> {
        let url = format!("{}/tracker/uploads/user/{}", API_BASE_URL, user_id);

        match self
            .client
            .get(&url)
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .send()
            .await
        {
            Ok(response) => {
                let server_stats = response.json::<UserStatsResponse>().await.unwrap();
                Ok(UploadStats {
                    total_duration_uploaded: server_stats
                        .statistics
                        .total_video_time
                        .seconds
                        .into(),
                    total_files_uploaded: server_stats.statistics.total_uploads,
                    total_volume_uploaded: server_stats.statistics.total_data.megabytes as u64,
                    last_upload_date: match server_stats.uploads.first() {
                        Some(upload) => upload.created_at.to_rfc3339(),
                        None => "Never".to_string(),
                    },
                })
            }
            Err(err) => Err(ValidationError {
                success: false,
                message: Some(format!("Request failed: {err}")),
            }),
        }
    }
}
