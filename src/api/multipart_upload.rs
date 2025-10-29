use std::path::Path;

use color_eyre::eyre::{self, Context as _, ContextCompat as _};
use serde::{Deserialize, Serialize};

use crate::api::{API_BASE_URL, ApiClient, check_for_response_success};

#[derive(Default, Debug, Clone)]
#[allow(unused)]
pub struct InitMultipartUploadArgs<'a> {
    pub tags: Option<&'a [String]>,
    pub video_filename: Option<&'a str>,
    pub control_filename: Option<&'a str>,
    pub video_duration_seconds: Option<f64>,
    pub video_width: Option<u32>,
    pub video_height: Option<u32>,
    pub video_codec: Option<&'a str>,
    pub video_fps: Option<f32>,
    pub chunk_size_bytes: Option<u64>,
}

#[derive(Deserialize, Debug)]
#[allow(unused)]
pub struct InitMultipartUploadResponse {
    pub upload_id: String,
    pub game_control_id: String,
    pub total_chunks: u64,
    pub chunk_size_bytes: u64,
    /// Unix timestamp
    pub expires_at: u64,
}

#[derive(Deserialize, Debug)]
#[allow(unused)]
pub struct UploadMultipartChunkResponse {
    pub upload_url: String,
    pub chunk_number: u64,
    /// Unix timestamp
    pub expires_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompleteMultipartUploadChunk {
    pub chunk_number: u64,
    pub etag: String,
}

#[derive(Deserialize, Debug)]
#[allow(unused)]
pub struct CompleteMultipartUploadResponse {
    pub success: bool,
    pub game_control_id: String,
    pub object_key: String,
    pub message: String,
    #[serde(default)]
    pub verified: Option<bool>,
}

#[derive(Deserialize, Debug)]
#[allow(unused)]
pub struct AbortMultipartUploadResponse {
    pub success: bool,
    pub message: String,
}

impl ApiClient {
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
            video_duration_seconds: Option<f64>,
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
