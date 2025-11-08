use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::api::{API_BASE_URL, ApiClient, ApiError, check_for_response_success};

#[derive(Debug, Clone)]
pub struct UserUploads {
    pub statistics: UserUploadStatistics,
    pub uploads: Vec<UserUpload>,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(unused)]
pub struct UserUploadStatistics {
    pub total_uploads: u64,
    pub total_data: UserUploadDataSize,
    pub total_video_time: UserUploadVideoTime,
    pub verified_uploads: u32,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(unused)]
pub struct UserUploadDataSize {
    pub bytes: u64,
    pub megabytes: f64,
    pub gigabytes: f64,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(unused)]
pub struct UserUploadVideoTime {
    pub seconds: f64,
    pub minutes: f64,
    pub hours: f64,
    pub formatted: String,
}

/// this struct has to be public for config defining UploadStats to reference
#[derive(Deserialize, Debug, Clone)]
#[allow(unused)]
pub struct UserUpload {
    pub content_type: String,
    pub created_at: DateTime<Utc>,
    pub file_size_bytes: u64,
    pub file_size_mb: f64,
    pub filename: String,
    pub id: String,
    pub tags: Option<serde_json::Value>,
    pub verified: bool,
    pub video_duration_seconds: Option<f64>,
}

impl ApiClient {
    pub async fn get_user_upload_stats(
        &self,
        api_key: &str,
        user_id: &str,
    ) -> Result<UserUploads, ApiError> {
        // Response structs for the user info endpoint
        #[derive(Deserialize, Debug)]
        #[allow(unused)]
        struct UserStatsResponse {
            success: bool,
            user_id: String,
            statistics: UserUploadStatistics,
            uploads: Vec<UserUpload>,
        }

        let response = self
            .client
            .get(format!("{API_BASE_URL}/tracker/uploads/user/{user_id}"))
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .send()
            .await?;

        let response =
            check_for_response_success(response, "User upload stats unavailable").await?;

        let server_stats = response.json::<UserStatsResponse>().await?;

        Ok(UserUploads {
            statistics: server_stats.statistics,
            uploads: server_stats.uploads,
        })
    }
}
