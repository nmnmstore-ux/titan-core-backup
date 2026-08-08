use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
#[error(transparent)]
pub enum BridgeError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("unauthenticated")]
    Unauthenticated,
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("service unavailable: {0}")]
    Unavailable(String),
    #[error("timeout")]
    Timeout,
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("external service error: {0}")]
    ExternalService(String),
    #[error("kyc not verified")]
    KycNotVerified,
    #[error("transaction limit exceeded")]
    LimitExceeded,
    #[error("sanctioned address")]
    Sanctioned,
}

impl BridgeError {
    pub fn code(&self) -> &'static str {
        match self {
            BridgeError::InvalidArgument(_) => "invalid_argument",
            BridgeError::Unauthenticated => "unauthenticated",
            BridgeError::PermissionDenied(_) => "permission_denied",
            BridgeError::NotFound(_) => "not_found",
            BridgeError::AlreadyExists(_) => "already_exists",
            BridgeError::Internal(_) => "internal",
            BridgeError::Unavailable(_) => "unavailable",
            BridgeError::Timeout => "timeout",
            BridgeError::Serialization(_) => "serialization_error",
            BridgeError::ExternalService(_) => "external_service_error",
            BridgeError::KycNotVerified => "kyc_not_verified",
            BridgeError::LimitExceeded => "limit_exceeded",
            BridgeError::Sanctioned => "sanctioned",
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            BridgeError::Unavailable(_)
                | BridgeError::Timeout
                | BridgeError::ExternalService(_)
        )
    }

    pub fn status_code(&self) -> u16 {
        match self {
            BridgeError::InvalidArgument(_) => 400,
            BridgeError::Unauthenticated => 401,
            BridgeError::PermissionDenied(_) => 403,
            BridgeError::NotFound(_) => 404,
            BridgeError::AlreadyExists(_) => 409,
            BridgeError::Internal(_) => 500,
            BridgeError::Unavailable(_) => 503,
            BridgeError::Timeout => 408,
            BridgeError::Serialization(_) => 400,
            BridgeError::ExternalService(_) => 502,
            BridgeError::KycNotVerified => 403,
            BridgeError::LimitExceeded => 429,
            BridgeError::Sanctioned => 403,
        }
    }
}

impl From<serde_json::Error> for BridgeError {
    fn from(e: serde_json::Error) -> Self {
        BridgeError::Serialization(e.to_string())
    }
}

impl From<rust_decimal::Error> for BridgeError {
    fn from(e: rust_decimal::Error) -> Self {
        BridgeError::InvalidArgument(e.to_string())
    }
}
