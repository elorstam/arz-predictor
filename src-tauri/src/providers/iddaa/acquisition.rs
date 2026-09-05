use std::{error::Error, time::Duration};

use serde::Serialize;

pub const EVENTS_URL: &str =
    "https://sportsbookv2.iddaa.com/sportsbook/events?st=1&type=0&version=0";
pub const COMPETITIONS_URL: &str = "https://sportsbookv2.iddaa.com/sportsbook/competitions";
const MAX_RESPONSE_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct NetworkError {
    pub category: String,
    pub message: String,
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.category, self.message)
    }
}

impl Error for NetworkError {}

fn client() -> Result<reqwest::Client, NetworkError> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("Football Predictor/0.1 (Iddaa bulletin adapter)")
        .build()
        .map_err(|error| NetworkError {
            category: "client_configuration".to_string(),
            message: error.to_string(),
        })
}

pub async fn download_json(url: &str) -> Result<Vec<u8>, NetworkError> {
    let mut response = client()?.get(url).send().await.map_err(categorize)?;
    if !response.status().is_success() {
        return Err(NetworkError {
            category: if response.status().is_redirection() {
                "redirect"
            } else {
                "http_status"
            }
            .to_string(),
            message: format!("server returned HTTP {}", response.status()),
        });
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(NetworkError {
            category: "response_too_large".to_string(),
            message: format!("declared response exceeds {MAX_RESPONSE_BYTES} bytes"),
        });
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(categorize)? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(NetworkError {
                category: "response_too_large".to_string(),
                message: format!("response exceeds {MAX_RESPONSE_BYTES} bytes"),
            });
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub fn read_local_json(path: &std::path::Path) -> Result<Vec<u8>, NetworkError> {
    let metadata = std::fs::metadata(path).map_err(|error| NetworkError {
        category: "local_file".to_string(),
        message: format!("failed to inspect '{}': {error}", path.display()),
    })?;
    if metadata.len() > MAX_RESPONSE_BYTES as u64 {
        return Err(NetworkError {
            category: "response_too_large".to_string(),
            message: format!("local JSON exceeds {MAX_RESPONSE_BYTES} bytes"),
        });
    }
    let bytes = std::fs::read(path).map_err(|error| NetworkError {
        category: "local_file".to_string(),
        message: format!("failed to read '{}': {error}", path.display()),
    })?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(NetworkError {
            category: "response_too_large".to_string(),
            message: format!("local JSON exceeds {MAX_RESPONSE_BYTES} bytes"),
        });
    }
    Ok(bytes)
}

fn categorize(error: reqwest::Error) -> NetworkError {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        message.push_str(": ");
        message.push_str(&error.to_string());
        source = error.source();
    }
    let lower = message.to_lowercase();
    let category = if error.is_timeout() {
        "timeout"
    } else if error.is_redirect() {
        "redirect"
    } else if error.is_status() {
        "http_status"
    } else if lower.contains("dns")
        || lower.contains("no such host")
        || lower.contains("lookup address")
    {
        "dns"
    } else if lower.contains("tls") || lower.contains("certificate") || lower.contains("handshake")
    {
        "tls"
    } else if error.is_connect() {
        "connection"
    } else if error.is_body() || error.is_decode() {
        "response_body"
    } else {
        "transport"
    };
    NetworkError {
        category: category.to_string(),
        message,
    }
}
