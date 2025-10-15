#![allow(dead_code)]

use std::path::Path;

use chrono::{DateTime, Utc};
use color_eyre::eyre::{self, Context, ContextCompat};
use serde::{Deserialize, Serialize};

use crate::config::UploadStats;

const API_BASE_URL: &str = "https://api.openworldlabs.ai";

#[derive(Default, Debug, Clone)]
pub struct InitMultipartUploadArgs<'a> {
    pub tags: Option<&'a [String]>,
    pub video_filename: Option<&'a str>,
    pub control_filename: Option<&'a str>,
    pub video_duration_seconds: Option<f32>,
    pub video_width: Option<u32>,
    pub video_height: Option<u32>,
    pub video_codec: Option<&'a str>,
    pub video_fps: Option<f32>,
    pub chunk_size_bytes: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub struct InitMultipartUploadResponse {
    pub upload_id: String,
    pub game_control_id: String,
    pub total_chunks: u64,
    pub chunk_size_bytes: u64,
    /// Unix timestamp
    pub expires_at: u64,
}

#[derive(Deserialize, Debug)]
pub struct UploadMultipartChunkResponse {
    pub upload_url: String,
    pub chunk_number: u64,
    /// Unix timestamp
    pub expires_at: u64,
}

#[derive(Serialize, Debug)]
pub struct CompleteMultipartUploadChunk {
    pub chunk_number: u64,
    pub etag: String,
}

#[derive(Deserialize, Debug)]
pub struct CompleteMultipartUploadResponse {
    pub success: bool,
    pub game_control_id: String,
    pub object_key: String,
    pub message: String,
    #[serde(default)]
    pub verified: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct AbortMultipartUploadResponse {
    pub success: bool,
    pub message: String,
}

/// this struct has to be public for config defining UploadStats to reference
#[derive(Deserialize, Debug, Clone)]
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
    pub async fn validate_api_key(&self, api_key: &str) -> eyre::Result<String> {
        // Response struct for the user info endpoint
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UserIdResponse {
            user_id: String,
        }

        let client = self.client.clone();

        // Validate input
        if api_key.is_empty() || api_key.trim().is_empty() {
            eyre::bail!("API key cannot be empty");
        }

        // Simple validation - check if it starts with 'sk_'
        if !api_key.starts_with("sk_") {
            eyre::bail!("Invalid API key format");
        }

        // Make the API request
        let response = client
            .get(format!("{API_BASE_URL}/api/v1/user/info"))
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .send()
            .await
            .context("failed to validate API key")?;

        let response =
            check_for_response_success(response, "Invalid API key, or server unavailable").await?;

        // Parse the JSON response
        let user_info = response
            .json::<UserIdResponse>()
            .await
            .context("failed to validate API key")?;

        Ok(user_info.user_id)
    }

    pub async fn get_user_upload_stats(
        &self,
        api_key: &str,
        user_id: &str,
    ) -> eyre::Result<UploadStats> {
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

    pub async fn init_multipart_upload<'a>(
        &self,
        api_key: &str,
        archive_path: &Path,
        total_size_bytes: u64,
        args: InitMultipartUploadArgs<'a>,
    ) -> eyre::Result<InitMultipartUploadResponse> {
        #[derive(Serialize, Debug)]
        struct InitMultipartUploadRequest<'a> {
            filename: &'a str,
            content_type: &'a str,
            total_size_bytes: u64,
            #[serde(skip_serializing_if = "Option::is_none")]
            chunk_size_bytes: Option<u64>,

            #[serde(skip_serializing_if = "Option::is_none")]
            tags: Option<&'a [String]>,

            #[serde(skip_serializing_if = "Option::is_none")]
            video_filename: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            control_filename: Option<&'a str>,

            #[serde(skip_serializing_if = "Option::is_none")]
            video_duration_seconds: Option<f32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            video_width: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            video_height: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            video_codec: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            video_fps: Option<f32>,

            uploader_hwid: &'a str,
            upload_timestamp: &'a str,
        }

        let response = self
            .client
            .post(format!(
                "{API_BASE_URL}/tracker/upload/game_control/multipart/init"
            ))
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .json(&InitMultipartUploadRequest {
                filename: archive_path
                    .file_name()
                    .with_context(|| format!("Archive path {archive_path:?} has no filename"))?
                    .to_string_lossy()
                    .as_ref(),
                content_type: "application/x-tar",
                total_size_bytes,
                chunk_size_bytes: args.chunk_size_bytes,

                tags: args.tags,

                video_filename: args.video_filename,
                control_filename: args.control_filename,

                video_duration_seconds: args.video_duration_seconds,
                video_width: args.video_width,
                video_height: args.video_height,
                video_codec: args.video_codec,
                video_fps: args.video_fps,

                uploader_hwid: &crate::system::hardware_id::get()
                    .with_context(|| "Failed to get hardware ID")?,
                upload_timestamp: &chrono::Local::now().to_rfc3339(),
            })
            .send()
            .await
            .context("failed to send init multipart upload request")?;

        Ok(
            check_for_response_success(response, "Multipart upload initialization failed")
                .await?
                .json()
                .await?,
        )
    }

    pub async fn upload_multipart_chunk(
        &self,
        api_key: &str,
        upload_id: &str,
        chunk_number: u64,
        chunk_hash: &str,
    ) -> eyre::Result<UploadMultipartChunkResponse> {
        #[derive(Serialize, Debug)]
        struct UploadMultipartChunkRequest<'a> {
            upload_id: &'a str,
            chunk_number: u64,
            chunk_hash: &'a str,
        }

        let response = self
            .client
            .post(format!(
                "{API_BASE_URL}/tracker/upload/game_control/multipart/chunk"
            ))
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .json(&UploadMultipartChunkRequest {
                upload_id,
                chunk_number,
                chunk_hash,
            })
            .send()
            .await
            .context("failed to send upload multipart chunk request")?;
        Ok(
            check_for_response_success(response, "Upload multipart chunk request failed")
                .await?
                .json()
                .await?,
        )
    }

    pub async fn complete_multipart_upload(
        &self,
        api_key: &str,
        upload_id: &str,
        chunk_etags: &[CompleteMultipartUploadChunk],
    ) -> eyre::Result<CompleteMultipartUploadResponse> {
        #[derive(Serialize, Debug)]
        struct CompleteMultipartUploadRequest<'a> {
            upload_id: &'a str,
            chunk_etags: &'a [CompleteMultipartUploadChunk],
        }

        let response = self
            .client
            .post(format!(
                "{API_BASE_URL}/tracker/upload/game_control/multipart/complete"
            ))
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .json(&CompleteMultipartUploadRequest {
                upload_id,
                chunk_etags,
            })
            .send()
            .await
            .context("failed to send complete multipart upload request")?;

        Ok(
            check_for_response_success(response, "Complete multipart upload request failed")
                .await?
                .json()
                .await?,
        )
    }

    pub async fn abort_multipart_upload(
        &self,
        api_key: &str,
        upload_id: &str,
    ) -> eyre::Result<AbortMultipartUploadResponse> {
        let response = self
            .client
            .delete(format!(
                "{API_BASE_URL}/tracker/upload/game_control/multipart/abort/{upload_id}"
            ))
            .header("X-API-Key", api_key)
            .send()
            .await
            .context("failed to send abort multipart upload request")?;

        Ok(
            check_for_response_success(response, "Abort multipart upload request failed")
                .await?
                .json()
                .await?,
        )
    }
}

async fn check_for_response_success(
    response: reqwest::Response,
    context: &str,
) -> eyre::Result<reqwest::Response> {
    let status = response.status();
    if !status.is_success() {
        let value = response
            .json::<serde_json::Value>()
            .await
            .unwrap_or_default();
        let detail = value
            .get("detail")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        eyre::bail!("{context} ({status}: {detail})");
    }
    Ok(response)
}
