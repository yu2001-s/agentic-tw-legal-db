use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TwlawError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    UpstreamBlocked(String),
    #[error("{0}")]
    ParseChanged(String),
    #[error("{0}")]
    Network(String),
    #[error("{0}")]
    Data(String),
}

impl TwlawError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_input",
            Self::NotFound(_) => "not_found",
            Self::UpstreamBlocked(_) => "upstream_blocked",
            Self::ParseChanged(_) => "parse_changed",
            Self::Network(_) => "network_error",
            Self::Data(_) => "data_error",
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidInput(_) => 2,
            Self::NotFound(_) => 3,
            Self::UpstreamBlocked(_) => 4,
            Self::ParseChanged(_) => 5,
            Self::Network(_) => 6,
            Self::Data(_) => 7,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "success": false,
            "error": {
                "code": self.code(),
                "message": self.to_string()
            },
            "retrieved_at": crate::retrieved_at()
        })
    }
}

impl From<reqwest::Error> for TwlawError {
    fn from(value: reqwest::Error) -> Self {
        Self::Network(value.to_string())
    }
}

impl From<url::ParseError> for TwlawError {
    fn from(value: url::ParseError) -> Self {
        Self::InvalidInput(value.to_string())
    }
}

impl From<serde_json::Error> for TwlawError {
    fn from(value: serde_json::Error) -> Self {
        Self::Data(value.to_string())
    }
}

impl From<csv::Error> for TwlawError {
    fn from(value: csv::Error) -> Self {
        Self::Data(value.to_string())
    }
}

impl From<std::io::Error> for TwlawError {
    fn from(value: std::io::Error) -> Self {
        Self::Data(value.to_string())
    }
}

impl From<zip::result::ZipError> for TwlawError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Data(value.to_string())
    }
}
