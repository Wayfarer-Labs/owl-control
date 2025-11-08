use serde::Deserialize;

mod multipart_upload;
pub use multipart_upload::*;

mod user_upload;
pub use user_upload::*;

const API_BASE_URL: &str = "https://api.openworldlabs.ai";

#[derive(Debug)]
pub enum ApiError {
    Reqwest(reqwest::Error),
    ApiKeyValidationFailure(String),
    ApiFailure {
        context: String,
        error: String,
        status: Option<reqwest::StatusCode>,
    },
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Reqwest(err) => write!(f, "Failed to make API request: {err}"),
            ApiError::ApiKeyValidationFailure(err) => write!(f, "API key validation failed: {err}"),
            ApiError::ApiFailure {
                context,
                error,
                status,
            } => write!(f, "{context}: {error} ({status:?})"),
        }
    }
}
impl std::error::Error for ApiError {}
impl From<reqwest::Error> for ApiError {
    fn from(err: reqwest::Error) -> Self {
        ApiError::Reqwest(err)
    }
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
    pub async fn validate_api_key(&self, api_key: &str) -> Result<String, ApiError> {
        // Response struct for the user info endpoint
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UserIdResponse {
            user_id: String,
        }

        let client = &self.client;

        // Validate input
        if api_key.is_empty() || api_key.trim().is_empty() {
            return Err(ApiError::ApiKeyValidationFailure(
                "API key cannot be empty".into(),
            ));
        }

        // Simple validation - check if it starts with 'sk_'
        if !api_key.starts_with("sk_") {
            return Err(ApiError::ApiKeyValidationFailure(
                "Invalid API key format".into(),
            ));
        }

        // Make the API request
        let response = client
            .get(format!("{API_BASE_URL}/api/v1/user/info"))
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .send()
            .await?;

        let response =
            check_for_response_success(response, "Invalid API key, or server unavailable").await?;

        // Parse the JSON response
        let user_info = response.json::<UserIdResponse>().await?;

        Ok(user_info.user_id)
    }
}

async fn check_for_response_success(
    response: reqwest::Response,
    context: &str,
) -> Result<reqwest::Response, ApiError> {
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
        return Err(ApiError::ApiFailure {
            context: context.into(),
            error: detail.into(),
            status: Some(status),
        });
    }
    Ok(response)
}
