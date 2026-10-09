use std::error::Error as StdError;

/// Every way a [`LoopsClient`](crate::LoopsClient) call can fail.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoopsError {
    /// Loops answered with a failure status, or with a body this SDK did not expect.
    #[error("{}", with_status(*.status, message))]
    Api {
        /// Human-readable description, including the `message` Loops sent when it sent one.
        message: String,
        /// HTTP status code of the response.
        status: Option<u16>,
        /// The response body, when it was JSON.
        body: Option<serde_json::Value>,
    },

    /// The arguments were rejected before any request was sent.
    #[error("{0}")]
    InvalidInput(String),

    /// The client is missing configuration it needs, such as an API key.
    #[error("{0}")]
    Configuration(String),

    /// `create_contact` found a contact with the same email or user ID (HTTP 409).
    #[error("[409] {message}")]
    ContactExists {
        /// The message Loops sent.
        message: String,
    },

    /// Safe mode is on and the address is outside the allowed domains.
    #[error(
        "Email '{email}' is not allowed in safe mode. Allowed domains: {}",
        allowed_domains.join(", ")
    )]
    UnsafeEmail {
        /// The rejected address.
        email: String,
        /// The domains safe mode allows.
        allowed_domains: Vec<String>,
    },

    /// Loops rate-limited the request (HTTP 429).
    #[error("[429] Rate limit exceeded")]
    RateLimit {
        /// The `x-ratelimit-limit` header, or 0 when absent.
        limit: u64,
        /// The `x-ratelimit-remaining` header, or 0 when absent.
        remaining: u64,
    },

    /// The request could not be sent or its response could not be read.
    #[error("HTTP transport error: {0}")]
    Http(#[from] reqwest::Error),

    /// A success response did not match the shape the OpenAPI spec promises.
    #[error("[{status}] Failed to parse {action} response: {source}")]
    Decode {
        /// The operation that was being performed, e.g. `"get campaign"`.
        action: &'static str,
        /// HTTP status code of the response.
        status: u16,
        /// The underlying parse error.
        source: Box<dyn StdError + Send + Sync>,
    },
}

impl LoopsError {
    /// The HTTP status code tied to this error, when there is one.
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => *status,
            Self::ContactExists { .. } => Some(409),
            Self::RateLimit { .. } => Some(429),
            Self::Decode { status, .. } => Some(*status),
            Self::Http(err) => err.status().map(|s| s.as_u16()),
            Self::InvalidInput(_) | Self::Configuration(_) | Self::UnsafeEmail { .. } => None,
        }
    }

    /// The JSON body Loops answered a failed request with, when there is one.
    pub fn response_body(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Api { body, .. } => body.as_ref(),
            _ => None,
        }
    }
}

fn with_status(status: Option<u16>, message: &str) -> String {
    match status {
        Some(status) => format!("[{status}] {message}"),
        None => message.to_owned(),
    }
}

/// Result alias used throughout the SDK.
pub type Result<T, E = LoopsError> = std::result::Result<T, E>;
