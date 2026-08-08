pub mod builder;
pub mod server;

pub use builder::{ExpansionBuilder, ServerConfig};
pub use server::ExpansionServer;

pub const MODULE_NAME: &str = "integration";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error("AI CEO bridge error: {0}")]
    AiCeo(String),
    #[error("Auth error: {0}")]
    Auth(String),
    #[error("Fiat error: {0}")]
    Fiat(String),
    #[error("gRPC error: {0}")]
    Grpc(String),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type IntegrationResult<T> = std::result::Result<T, IntegrationError>;
