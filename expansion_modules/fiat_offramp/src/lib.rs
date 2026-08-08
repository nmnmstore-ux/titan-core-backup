pub mod cards;
pub mod compliance;
pub mod config;
pub mod models;
pub mod routes;
pub mod stripe;
pub mod banxa;
pub mod webhooks;

pub use config::{FiatConfig, FiatConfigBuilder, FiatMode};
pub use models::*;
pub use stripe::StripeClient;
pub use banxa::BanxaClient;
pub use cards::CardManager;
pub use compliance::ComplianceChecker;
pub use routes::{router, AppState};

pub const MODULE_NAME: &str = "fiat_offramp";
pub const API_PREFIX: &str = "/api/v1/fiat";

#[derive(Debug, thiserror::Error)]
pub enum FiatError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("kyc not verified")]
    KycNotVerified,
    #[error("transaction limit exceeded")]
    LimitExceeded,
    #[error("sanctioned address")]
    Sanctioned,
    #[error("webhook signature verification failed")]
    SignatureFailed,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("external service error: {0}")]
    External(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, FiatError>;
