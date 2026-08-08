pub mod config;
pub mod webauthn;
pub mod jwt;
pub mod session;
pub mod middleware;
pub mod routes;

pub use config::AuthConfig;
pub use jwt::{Claims, TokenPair, generate_token, verify_token, refresh_access_token as refresh_jwt};
pub use session::{Session, SessionStore, InMemorySessionStore};
pub use webauthn::{Credential, RegistrationChallenge, LoginChallenge, WebAuthnManager};
pub use routes::{router, AppState};

pub const MODULE_NAME: &str = "pwa_auth";
pub const API_PREFIX: &str = "/api/v1/auth";

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("unauthenticated")]
    Unauthenticated,
    #[error("invalid challenge")]
    InvalidChallenge,
    #[error("challenge expired")]
    ChallengeExpired,
    #[error("credential not found")]
    CredentialNotFound,
    #[error("signature verification failed")]
    SignatureFailed,
    #[error("token expired")]
    TokenExpired,
    #[error("invalid token")]
    InvalidToken,
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, AuthError>;
