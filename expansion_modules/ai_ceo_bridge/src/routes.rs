use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

use crate::analyzer::{LiquidityReport, ModeSignal, SlippageReport};
use crate::bridge::{AiCeoBridge, BridgeConfig};

#[derive(Clone)]
pub struct AppState {
    pub bridge: Arc<AiCeoBridge>,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeRequest {
    pub symbol: String,
    pub target_size: String,
    pub venues: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct AnalyzeResponse {
    pub success: bool,
    pub liquidity: Option<LiquidityReport>,
    pub mode: Option<ModeSignal>,
    pub slippage: Option<SlippageReport>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
    pub ollama_available: bool,
    pub uptime_seconds: u64,
    pub stats: crate::bridge::BridgeStats,
}

#[derive(Debug, Deserialize)]
pub struct LiquidityScanRequest {
    pub symbol: String,
    pub target_size: String,
    pub depth: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct LiquidityScanResponse {
    pub success: bool,
    pub report: LiquidityReport,
}

#[derive(Debug, Deserialize)]
pub struct SlippageRequest {
    pub symbol: String,
    pub order_size: String,
    pub tolerance_bps: f64,
}

#[derive(Debug, Serialize)]
pub struct SlippageResponse {
    pub success: bool,
    pub report: SlippageReport,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/analyze", post(analyze_handler))
        .route("/health", get(health_handler))
        .route("/liquidity-scan", post(liquidity_scan_handler))
        .route("/slippage-detect", post(slippage_detect_handler))
        .route("/mode", get(get_mode_handler))
        .route("/snapshot", get(get_snapshot_handler))
        .with_state(state)
}

pub async fn analyze_handler(
    State(state): State<AppState>,
    Json(req): Json<AnalyzeRequest>,
) -> Result<Json<AnalyzeResponse>, (StatusCode, Json<serde_json::Value>)> {
    let signal = state
        .bridge
        .analyze_and_signal(&req.symbol)
        .await
        .map_err(|e| {
            tracing::error!("analyze error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                    "error_code": e.code(),
                })),
            )
        })?;
    let snapshot = state.bridge.telemetry.get_market_snapshot(&req.symbol).await;
    let liquidity = state
        .bridge
        .analyzer
        .analyze_liquidity(&req.symbol, req.target_size.parse().unwrap_or_default(), &[snapshot])
        .await;
    let snapshot = state.bridge.telemetry.get_market_snapshot(&req.symbol).await;
    let slippage = state
        .bridge
        .analyzer
        .detect_slippage(&req.symbol, req.target_size.parse().unwrap_or_default(), &[snapshot], 50.0)
        .await;

    Ok(Json(AnalyzeResponse {
        success: true,
        liquidity: Some(liquidity),
        mode: Some(signal),
        slippage: Some(slippage),
    }))
}

pub async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let health = state.bridge.health().await;
    Json(HealthResponse {
        ok: health.ok,
        service: "ai-ceo-bridge".to_string(),
        ollama_available: health.ollama.available,
        uptime_seconds: health.uptime_seconds,
        stats: health.stats,
    })
}

pub async fn liquidity_scan_handler(
    State(state): State<AppState>,
    Json(req): Json<LiquidityScanRequest>,
) -> Result<Json<LiquidityScanResponse>, (StatusCode, Json<serde_json::Value>)> {
    let start = Instant::now();
    let target_size = req.target_size.parse::<rust_decimal::Decimal>().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "invalid target_size"
            })),
        )
    })?;

    let snapshot = state.bridge.telemetry.get_market_snapshot(&req.symbol).await;
    let report = state
        .bridge
        .analyzer
        .analyze_liquidity(&req.symbol, target_size, &[snapshot])
        .await;

    let elapsed = start.elapsed().as_millis() as u64;
    tracing::info!(symbol = %req.symbol, score = report.liquidity_score, elapsed_ms = elapsed, "Liquidity analysis completed");

    Ok(Json(LiquidityScanResponse {
        success: true,
        report,
    }))
}

pub async fn slippage_detect_handler(
    State(state): State<AppState>,
    Json(req): Json<SlippageRequest>,
) -> Result<Json<SlippageResponse>, (StatusCode, Json<serde_json::Value>)> {
    let order_size = req.order_size.parse::<rust_decimal::Decimal>().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "invalid order_size"
            })),
        )
    })?;

    let snapshot = state.bridge.telemetry.get_market_snapshot(&req.symbol).await;
    let report = state
        .bridge
        .analyzer
        .detect_slippage(&req.symbol, order_size, &[snapshot], req.tolerance_bps)
        .await;

    Ok(Json(SlippageResponse {
        success: true,
        report,
    }))
}

pub async fn get_mode_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mode = state.bridge.telemetry.get_current_mode().await;
    Json(serde_json::json!({
        "mode": mode.as_str(),
        "timestamp": chrono::Utc::now(),
    }))
}

pub async fn get_snapshot_handler(State(state): State<AppState>) -> impl IntoResponse {
    let snapshot = state.bridge.get_snapshot().await;
    Json(snapshot)
}

#[allow(dead_code)]
pub fn build_state(config: BridgeConfig) -> AppState {
    let telemetry = Arc::new(crate::bridge::InMemoryTelemetry::new());
    let bridge = Arc::new(AiCeoBridge::new(config, telemetry));
    AppState { bridge }
}
