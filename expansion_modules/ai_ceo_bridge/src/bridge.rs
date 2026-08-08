use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info};

use crate::analyzer::{Analyzer, AnalyzerConfig, MarketSnapshot, ModeSignal, TradingMode};
use crate::ollama_client::{CompletionRequest, OllamaClient, OllamaConfig};
use crate::Result;

#[async_trait]
pub trait TelemetryProvider: Send + Sync {
    async fn get_market_snapshot(&self, symbol: &str) -> MarketSnapshot;
    async fn get_current_mode(&self) -> TradingMode;
    async fn send_signal(&self, signal: &ModeSignal) -> bool;
}

#[async_trait]
pub trait LiquidityTelemetry: TelemetryProvider {
    async fn get_pool_liquidity(&self, pool_id: &str) -> Option<crate::analyzer::PoolSnapshot>;
    async fn get_venue_quotes(&self, symbol: &str) -> Vec<crate::analyzer::VenueQuote>;
    async fn get_chain_total_tvl(&self, chain: &str) -> Decimal;
}

#[derive(Debug, Clone)]
pub struct InMemoryTelemetry {
    snapshots: Arc<tokio::sync::RwLock<HashMap<String, MarketSnapshot>>>,
    mode: Arc<tokio::sync::RwLock<TradingMode>>,
}

impl InMemoryTelemetry {
    pub fn new() -> Self {
        Self {
            snapshots: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            mode: Arc::new(tokio::sync::RwLock::new(TradingMode::Normal)),
        }
    }

    pub async fn update_snapshot(&self, symbol: &str, snapshot: MarketSnapshot) {
        self.snapshots.write().await.insert(symbol.to_string(), snapshot);
    }

    pub async fn set_mode(&self, mode: TradingMode) {
        *self.mode.write().await = mode;
    }
}

impl Default for InMemoryTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelemetryProvider for InMemoryTelemetry {
    async fn get_market_snapshot(&self, symbol: &str) -> MarketSnapshot {
        self.snapshots
            .read()
            .await
            .get(symbol)
            .cloned()
            .unwrap_or(MarketSnapshot {
                symbol: symbol.to_string(),
                venues: vec![],
                pools: vec![],
                timestamp: Utc::now(),
            })
    }

    async fn get_current_mode(&self) -> TradingMode {
        *self.mode.read().await
    }

    async fn send_signal(&self, signal: &ModeSignal) -> bool {
        info!(
            mode = %signal.mode,
            confidence = signal.confidence,
            "Mode signal generated",
        );
        true
    }
}

#[async_trait]
impl LiquidityTelemetry for InMemoryTelemetry {
    async fn get_pool_liquidity(&self, pool_id: &str) -> Option<crate::analyzer::PoolSnapshot> {
        for snapshot in self.snapshots.read().await.values() {
            for pool in &snapshot.pools {
                if pool.pool_id == pool_id {
                    return Some(pool.clone());
                }
            }
        }
        None
    }

    async fn get_venue_quotes(&self, symbol: &str) -> Vec<crate::analyzer::VenueQuote> {
        let snapshot = self.get_market_snapshot(symbol).await;
        snapshot.venues
    }

    async fn get_chain_total_tvl(&self, chain: &str) -> Decimal {
        let mut total = Decimal::ZERO;
        for snapshot in self.snapshots.read().await.values() {
            for pool in &snapshot.pools {
                if pool.chain == chain {
                    total += pool.tvl;
                }
            }
        }
        total
    }
}

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub ollama: OllamaConfig,
    pub analyzer: AnalyzerConfig,
    pub auto_analyze_interval_secs: u64,
    pub signal_threshold: f64,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            ollama: OllamaConfig::default(),
            analyzer: AnalyzerConfig::default(),
            auto_analyze_interval_secs: 15,
            signal_threshold: 0.7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeSnapshot {
    pub timestamp: DateTime<Utc>,
    pub ollama_available: bool,
    pub current_mode: TradingMode,
    pub liquidity_score: f64,
    pub active_breaches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeStats {
    pub total_analyses: u64,
    pub total_signals: u64,
    pub avg_response_ms: u64,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub ok: bool,
    pub ollama: crate::ollama_client::HealthStatus,
    pub uptime_seconds: u64,
    pub stats: BridgeStats,
}

#[derive(Clone)]
pub struct AiCeoBridge {
    pub(crate) config: BridgeConfig,
    pub(crate) ollama: Arc<OllamaClient>,
    pub(crate) analyzer: Arc<Analyzer>,
    pub(crate) telemetry: Arc<dyn LiquidityTelemetry>,
    running: Arc<tokio::sync::RwLock<bool>>,
    pub(crate) stats: Arc<tokio::sync::RwLock<BridgeStats>>,
    pub(crate) started_at: Instant,
}

impl AiCeoBridge {
    pub fn new(
        config: BridgeConfig,
        telemetry: Arc<dyn LiquidityTelemetry>,
    ) -> Self {
        let ollama = Arc::new(OllamaClient::new(config.ollama.clone()));
        let analyzer = Arc::new(Analyzer::new(config.analyzer.clone()));
        Self {
            config,
            ollama,
            analyzer,
            telemetry,
            running: Arc::new(tokio::sync::RwLock::new(false)),
            stats: Arc::new(tokio::sync::RwLock::new(BridgeStats {
                total_analyses: 0,
                total_signals: 0,
                avg_response_ms: 0,
                uptime_seconds: 0,
            })),
            started_at: Instant::now(),
        }
    }

    pub async fn health(&self) -> HealthReport {
        let available = self.ollama.health().await;
        let mut status = self.ollama.health_status();
        status.available = available;
        let stats = self.stats.read().await.clone();
        HealthReport {
            ok: available,
            ollama: status,
            uptime_seconds: self.started_at.elapsed().as_secs(),
            stats,
        }
    }

    pub async fn start_auto_analysis(&self) -> Result<()> {
        *self.running.write().await = true;
        let interval = Duration::from_secs(self.config.auto_analyze_interval_secs);
        let bridge = self.clone();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                if !*bridge.running.read().await {
                    break;
                }
                ticker.tick().await;
                let _ = bridge.run_auto_analysis().await;
            }
        });

        info!("AI CEO Bridge auto-analysis started");
        Ok(())
    }

    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("AI CEO Bridge stopped");
    }

    async fn run_auto_analysis(&self) -> Result<()> {
        let symbols = vec!["ETH/USDT".to_string(), "BTC/USDT".to_string()];
        for symbol in symbols {
            if let Err(e) = self.analyze_and_signal(&symbol).await {
                debug!("Auto-analysis failed for {}: {}", symbol, e);
            }
        }
        Ok(())
    }

    pub async fn analyze_and_signal(&self, symbol: &str) -> Result<ModeSignal> {
        let start = Instant::now();

        let snapshot = self.telemetry.get_market_snapshot(symbol).await;
        let snapshots = vec![snapshot];

        let liquidity_report = self
            .analyzer
            .analyze_liquidity(symbol, Decimal::from(1000000), &snapshots)
            .await;

        let slippage_report = self
            .analyzer
            .detect_slippage(symbol, Decimal::from(1000000), &snapshots, 50.0)
            .await;

        let mode_signal = self
            .analyzer
            .recommend_mode(&liquidity_report, &slippage_report)
            .await;

        let mode_changed = {
            let current_mode = self.telemetry.get_current_mode().await;
            current_mode != mode_signal.mode
        };

        if mode_signal.confidence >= self.config.signal_threshold || mode_changed {
            self.telemetry.send_signal(&mode_signal).await;
            self.increment_signals().await;
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        self.update_avg_response(elapsed_ms).await;
        self.increment_analyses().await;

        Ok(mode_signal)
    }

    pub async fn manual_analysis(
        &self,
        prompt: &str,
        context: &str,
    ) -> Result<String> {
        let full_prompt = format!("{}\n\nContext:\n{}", prompt, context);
        let req = CompletionRequest {
            prompt: full_prompt,
            temperature: Some(0.7),
            max_tokens: Some(8192),
            top_p: Some(0.9),
        };
        let resp = self
            .ollama
            .complete(req)
            .await
            .map_err(|e| crate::BridgeError::Provider(e.to_string()))?;
        Ok(resp.content)
    }

    pub async fn get_snapshot(&self) -> BridgeSnapshot {
        let health = self.health().await;
        let mode = self.telemetry.get_current_mode().await;
        let current_symbol = "ETH/USDT";
        let liquidity = self
            .analyzer
            .analyze_liquidity(
                current_symbol,
                Decimal::from(1000000),
                &[self.telemetry.get_market_snapshot(current_symbol).await],
            )
            .await;

        let slippage = self
            .analyzer
            .detect_slippage(
                current_symbol,
                Decimal::from(1000000),
                &[self.telemetry.get_market_snapshot(current_symbol).await],
                50.0,
            )
            .await;

        BridgeSnapshot {
            timestamp: Utc::now(),
            ollama_available: health.ok,
            current_mode: mode,
            liquidity_score: liquidity.liquidity_score,
            active_breaches: slippage.total_breaches,
        }
    }

    async fn increment_analyses(&self) {
        let mut stats = self.stats.write().await;
        stats.total_analyses += 1;
    }

    async fn increment_signals(&self) {
        let mut stats = self.stats.write().await;
        stats.total_signals += 1;
    }

    async fn update_avg_response(&self, ms: u64) {
        let mut stats = self.stats.write().await;
        let total = stats.total_analyses;
        if total > 0 {
            stats.avg_response_ms = ((stats.avg_response_ms * total) + ms) / (total + 1);
        } else {
            stats.avg_response_ms = ms;
        }
    }
}
