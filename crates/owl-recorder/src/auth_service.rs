use reqwest;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

// Define the API base URL as a constant
const API_BASE_URL: &str = "https://api.openworldlabs.ai"; // Replace with actual URL

// Response struct for the user info endpoint
#[derive(Deserialize)]
struct UserInfoResponse {
    #[serde(rename = "userId")]
    user_id: String,
}

#[derive(Deserialize)]
struct UserUploadStatsResponse {
    #[serde(rename = "userId")]
    user_id: String,
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
                match response.json::<UserInfoResponse>().await {
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

    pub async fn get_user_upload_stats(&self) -> Result<ValidationSuccess, ValidationError> {
        let url = format!("{}/tracker/uploads/user/{:?}", API_BASE_URL, self.user_id);

        match self
            .client
            .get(&url)
            .header("Content-Type", "application/json")
            .header("X-API-Key", &self.api_key.clone().unwrap())
            .send()
            .await
        {
            Ok(response) => {
                println!("{:?}", response.text().await);
                Ok(ValidationSuccess {
                    success: true,
                    user_id: self.user_id.clone().unwrap(),
                })
            }
            Err(err) => Err(ValidationError {
                success: false,
                message: Some("Request failed".to_string()),
            }),
        }
    }

    // Getter methods
    pub fn get_api_key(&self) -> Option<&String> {
        self.api_key.as_ref()
    }

    pub fn get_user_id(&self) -> Option<&String> {
        self.user_id.as_ref()
    }
}
