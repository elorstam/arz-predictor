use std::{
    error::Error,
    fs::File,
    io::{Read, Take},
    path::Path,
    time::{Duration, Instant},
};

use serde::Serialize;

pub const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct AcquisitionError {
    pub category: String,
    pub message: String,
}

impl std::fmt::Display for AcquisitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.category, self.message)
    }
}

impl Error for AcquisitionError {}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkDiagnostic {
    pub requested_url: String,
    pub final_url: Option<String>,
    pub success: bool,
    pub http_status: Option<u16>,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
    pub elapsed_ms: u128,
    pub error_category: Option<String>,
    pub error_message: Option<String>,
    pub proxy_environment_configured: bool,
}

pub struct AcquiredHttpData {
    pub bytes: Vec<u8>,
}

fn client() -> Result<reqwest::Client, AcquisitionError> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("Football Predictor/0.1 (football-data.co.uk importer)")
        .build()
        .map_err(|error| AcquisitionError {
            category: "client_configuration".to_string(),
            message: error.to_string(),
        })
}

fn error_chain(error: &dyn Error) -> String {
    let mut messages = vec![error.to_string()];
    let mut source = error.source();
    while let Some(error) = source {
        messages.push(error.to_string());
        source = error.source();
    }
    messages.join(": ")
}

pub(crate) fn categorize_reqwest_error(error: &reqwest::Error) -> AcquisitionError {
    let message = error_chain(error);
    let category = categorize_error_details(
        error.is_timeout(),
        error.is_redirect(),
        error.is_status(),
        error.is_connect(),
        error.is_body() || error.is_decode(),
        &message,
    );
    AcquisitionError {
        category: category.to_string(),
        message,
    }
}

pub(crate) fn categorize_error_details(
    is_timeout: bool,
    is_redirect: bool,
    is_status: bool,
    is_connect: bool,
    is_body: bool,
    message: &str,
) -> &'static str {
    let lower = message.to_lowercase();
    if is_timeout {
        "timeout"
    } else if is_redirect {
        "redirect"
    } else if is_status {
        "http_status"
    } else if lower.contains("proxy") {
        "proxy"
    } else if lower.contains("dns")
        || lower.contains("no such host")
        || lower.contains("failed to lookup address")
        || lower.contains("name or service not known")
        || lower.contains("nodename nor servname")
    {
        "dns"
    } else if lower.contains("tls")
        || lower.contains("ssl")
        || lower.contains("certificate")
        || lower.contains("handshake")
        || lower.contains("unknown issuer")
    {
        "tls"
    } else if is_connect {
        "connection"
    } else if is_body {
        "response_body"
    } else {
        "transport"
    }
}

async fn send(url: &str) -> Result<reqwest::Response, AcquisitionError> {
    let response = client()?
        .get(url)
        .send()
        .await
        .map_err(|error| categorize_reqwest_error(&error))?;
    if response.status().is_redirection() {
        return Err(AcquisitionError {
            category: "redirect".to_string(),
            message: format!("redirect was not resolved: HTTP {}", response.status()),
        });
    }
    if !response.status().is_success() {
        return Err(AcquisitionError {
            category: "http_status".to_string(),
            message: format!("server returned HTTP {}", response.status()),
        });
    }
    Ok(response)
}

pub async fn download_csv(url: &str) -> Result<AcquiredHttpData, AcquisitionError> {
    let mut response = send(url).await?;
    let content_length = response.content_length();
    if content_length.is_some_and(|length| length > MAX_RESPONSE_BYTES as u64) {
        return Err(AcquisitionError {
            category: "response_too_large".to_string(),
            message: format!(
                "declared response size exceeds maximum of {MAX_RESPONSE_BYTES} bytes"
            ),
        });
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| categorize_reqwest_error(&error))?
    {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(AcquisitionError {
                category: "response_too_large".to_string(),
                message: format!("response exceeds maximum of {MAX_RESPONSE_BYTES} bytes"),
            });
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(AcquiredHttpData { bytes })
}

pub fn read_local_csv(path: &Path) -> Result<Vec<u8>, AcquisitionError> {
    let file = File::open(path).map_err(|error| AcquisitionError {
        category: "local_file".to_string(),
        message: format!("failed to open '{}': {error}", path.display()),
    })?;
    let declared_size = file.metadata().ok().map(|metadata| metadata.len());
    if declared_size.is_some_and(|size| size > MAX_RESPONSE_BYTES as u64) {
        return Err(AcquisitionError {
            category: "response_too_large".to_string(),
            message: format!("local file exceeds maximum of {MAX_RESPONSE_BYTES} bytes"),
        });
    }
    let mut bytes = Vec::new();
    let mut limited: Take<File> = file.take(MAX_RESPONSE_BYTES as u64 + 1);
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| AcquisitionError {
            category: "local_file".to_string(),
            message: format!("failed to read '{}': {error}", path.display()),
        })?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(AcquisitionError {
            category: "response_too_large".to_string(),
            message: format!("local file exceeds maximum of {MAX_RESPONSE_BYTES} bytes"),
        });
    }
    Ok(bytes)
}

fn proxy_environment_configured() -> bool {
    ["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
}

pub async fn network_diagnostic(url: &str) -> NetworkDiagnostic {
    let started = Instant::now();
    let response = match client() {
        Ok(client) => client.get(url).send().await,
        Err(error) => {
            return NetworkDiagnostic {
                requested_url: url.to_string(),
                final_url: None,
                success: false,
                http_status: None,
                content_type: None,
                content_length: None,
                elapsed_ms: started.elapsed().as_millis(),
                error_category: Some(error.category),
                error_message: Some(error.message),
                proxy_environment_configured: proxy_environment_configured(),
            };
        }
    };
    let mut response = match response {
        Ok(response) => response,
        Err(error) => {
            let error = categorize_reqwest_error(&error);
            return NetworkDiagnostic {
                requested_url: url.to_string(),
                final_url: None,
                success: false,
                http_status: None,
                content_type: None,
                content_length: None,
                elapsed_ms: started.elapsed().as_millis(),
                error_category: Some(error.category),
                error_message: Some(error.message),
                proxy_environment_configured: proxy_environment_configured(),
            };
        }
    };
    let final_url = Some(response.url().to_string());
    let http_status = Some(response.status().as_u16());
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let declared_length = response.content_length();
    if !response.status().is_success() {
        return NetworkDiagnostic {
            requested_url: url.to_string(),
            final_url,
            success: false,
            http_status,
            content_type,
            content_length: declared_length,
            elapsed_ms: started.elapsed().as_millis(),
            error_category: Some(if response.status().is_redirection() {
                "redirect".to_string()
            } else {
                "http_status".to_string()
            }),
            error_message: Some(format!("server returned HTTP {}", response.status())),
            proxy_environment_configured: proxy_environment_configured(),
        };
    }
    if declared_length.is_some_and(|length| length > MAX_RESPONSE_BYTES as u64) {
        return NetworkDiagnostic {
            requested_url: url.to_string(),
            final_url,
            success: false,
            http_status,
            content_type,
            content_length: declared_length,
            elapsed_ms: started.elapsed().as_millis(),
            error_category: Some("response_too_large".to_string()),
            error_message: Some(format!(
                "declared response size exceeds maximum of {MAX_RESPONSE_BYTES} bytes"
            )),
            proxy_environment_configured: proxy_environment_configured(),
        };
    }
    let mut received = 0usize;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                received += chunk.len();
                if received > MAX_RESPONSE_BYTES {
                    return NetworkDiagnostic {
                        requested_url: url.to_string(),
                        final_url,
                        success: false,
                        http_status,
                        content_type,
                        content_length: Some(received as u64),
                        elapsed_ms: started.elapsed().as_millis(),
                        error_category: Some("response_too_large".to_string()),
                        error_message: Some(format!(
                            "response exceeds maximum of {MAX_RESPONSE_BYTES} bytes"
                        )),
                        proxy_environment_configured: proxy_environment_configured(),
                    };
                }
            }
            Ok(None) => break,
            Err(error) => {
                let error = categorize_reqwest_error(&error);
                return NetworkDiagnostic {
                    requested_url: url.to_string(),
                    final_url,
                    success: false,
                    http_status,
                    content_type,
                    content_length: Some(received as u64),
                    elapsed_ms: started.elapsed().as_millis(),
                    error_category: Some(error.category),
                    error_message: Some(error.message),
                    proxy_environment_configured: proxy_environment_configured(),
                };
            }
        }
    }
    NetworkDiagnostic {
        requested_url: url.to_string(),
        final_url,
        success: true,
        http_status,
        content_type,
        content_length: declared_length.or(Some(received as u64)),
        elapsed_ms: started.elapsed().as_millis(),
        error_category: None,
        error_message: None,
        proxy_environment_configured: proxy_environment_configured(),
    }
}
