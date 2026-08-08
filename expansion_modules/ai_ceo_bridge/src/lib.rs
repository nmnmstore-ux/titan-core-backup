pub mod analyzer;
pub mod bridge;
pub mod ollama_client;
pub mod prompts;
pub mod routes;

pub use analyzer::{
    Analyzer, AnalyzerConfig, ChainLiquidity, LiquidityFinding, LiquidityReport, MarketSnapshot,
    ModeDrivers, ModeSignal, PoolSnapshot, SlippageAnomaly, SlippageReport, TradingMode,
    VenueKind, VenueQuote, AnomalySeverity,
};
pub use bridge::{
    AiCeoBridge, BridgeConfig, BridgeSnapshot, BridgeStats, HealthReport, InMemoryTelemetry,
    LiquidityTelemetry, TelemetryProvider,
};
pub use ollama_client::{
    ApiFlavor, CompletionRequest, CompletionResponse, HealthStatus, LlmProvider, OllamaClient,
    OllamaConfig,
};
pub use routes::{router, AppState};

pub const MODULE_NAME: &str = "ai_ceo_bridge";
pub const API_PREFIX: &str = "/api/v1/ai";

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("transport failure talking to {endpoint}: {source}")]
    Transport {
        endpoint: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("llm backend unavailable at {endpoint}")]
    Unavailable { endpoint: String },

    #[error("model `{0}` is not loaded on the backend")]
    ModelNotFound(String),

    #[error("backend returned status {status}: {body}")]
    BackendStatus { status: u16, body: String },

    #[error("empty completion returned by `{0}`")]
    EmptyCompletion(String),

    #[error("failed to parse model output: {0}")]
    Parse(String),

    #[error("request timed out after {0}ms")]
    Timeout(u64),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("serialization failure: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("client build failure: {0}")]
    ClientBuild(String),

    #[error("llm provider error: {0}")]
    Provider(String),

    #[error("telemetry unavailable: {0}")]
    Telemetry(String),
}

pub type Result<T> = std::result::Result<T, BridgeError>;

impl BridgeError {
    pub fn code(&self) -> &'static str {
        match self {
            BridgeError::Transport { .. } => "transport_error",
            BridgeError::Unavailable { .. } => "backend_unavailable",
            BridgeError::ModelNotFound(_) => "model_not_found",
            BridgeError::BackendStatus { .. } => "backend_status",
            BridgeError::EmptyCompletion(_) => "empty_completion",
            BridgeError::Parse(_) => "parse_error",
            BridgeError::Timeout(_) => "timeout",
            BridgeError::InvalidRequest(_) => "invalid_request",
            BridgeError::Serialization(_) => "serialization_error",
            BridgeError::ClientBuild(_) => "client_build_error",
            BridgeError::Provider(_) => "provider_error",
            BridgeError::Telemetry(_) => "telemetry_error",
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            BridgeError::Transport { .. }
                | BridgeError::Unavailable { .. }
                | BridgeError::Timeout(_)
                | BridgeError::EmptyCompletion(_)
        )
    }
}
