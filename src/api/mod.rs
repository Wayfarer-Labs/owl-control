use color_eyre::eyre::{self, Context};
use serde::Deserialize;

mod multipart_upload;
pub use multipart_upload::*;

mod user_upload;
pub use user_upload::*;

const API_BASE_URL: &str = "https://api.openworldlabs.ai";

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
