use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum CoreError {
    #[error("Invalid input: {0}")] InvalidInput(String),
    #[error("Not found: {0}")] NotFound(String),
    #[error("Already exists: {0}")] AlreadyExists(String),
    #[error("Unauthorized: {0}")] Unauthorized(String),
    #[error("Timeout: {0}")] Timeout(String),
    #[error("Internal error: {0}")] Internal(String),
    #[error("Network error: {0}")] Network(String),
    #[error("Contract error: {0}")] Contract(String),
    #[error("Insufficient liquidity: {0}")] InsufficientLiquidity(String),
    #[error("Circuit breaker triggered: {0}")] CircuitBreaker(String),
    #[error("Serialization error: {0}")] Serialization(String),
    #[error("Provider error: {0}")] Provider(String),
}
impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self { Self::Internal(e.to_string()) }
}
impl From<serde_json::Error> for CoreError {
    fn from(e: serde_json::Error) -> Self { Self::Serialization(e.to_string()) }
}
