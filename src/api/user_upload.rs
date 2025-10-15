use chrono::{DateTime, Utc};
use color_eyre::eyre::{self, Context as _};
use serde::Deserialize;

use crate::api::{API_BASE_URL, ApiClient, check_for_response_success};
use crate::config::UploadStats;

/// this struct has to be public for config defining UploadStats to reference
#[derive(Deserialize, Debug, Clone)]
#[allow(unused)]
pub struct Upload {
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
    ) -> eyre::Result<UploadStats> {
        // Response structs for the user info endpoint
        #[derive(Deserialize, Debug)]
        #[allow(unused)]
        struct UserStatsResponse {
            success: bool,
            user_id: String,
            statistics: Statistics,
            uploads: Vec<Upload>, // idk format for this one
        }

        #[derive(Deserialize, Debug)]
        #[allow(unused)]
        struct Statistics {
            total_uploads: u64,
            total_data: DataSize,
            total_video_time: VideoTime,
            verified_uploads: u32,
        }

        #[derive(Deserialize, Debug)]
        #[allow(unused)]
        struct DataSize {
            bytes: u64,
            megabytes: f64,
            gigabytes: f64,
        }

        #[derive(Deserialize, Debug)]
        #[allow(unused)]
        struct VideoTime {
            seconds: f64,
            minutes: f64,
            hours: f64,
            formatted: String,
        }

        let response = self
            .client
            .get(format!("{API_BASE_URL}/tracker/uploads/user/{user_id}"))
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .send()
            .await
            .context("failed to get user upload stats")?;

        let response =
            check_for_response_success(response, "User upload stats unavailable").await?;

        let server_stats = response
            .json::<UserStatsResponse>()
            .await
            .context("failed to parse user upload stats response")?;

        Ok(UploadStats {
            total_duration_uploaded: server_stats.statistics.total_video_time.seconds,
            total_files_uploaded: server_stats.statistics.total_uploads,
            total_volume_uploaded: server_stats.statistics.total_data.bytes,
            last_upload_date: server_stats
                .uploads
                .first()
                .map(|upload| upload.created_at.with_timezone(&chrono::Local)),
            uploads: server_stats.uploads,
        })
    }
}
