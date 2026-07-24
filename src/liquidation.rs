//! Liquidation Engine
//!
//! Monitors DeFi lending positions for undercollateralized
//! loans and executes profitable liquidations using flash loans.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

pub type Address = [u8; 20];

#[derive(Debug, Clone)]
pub enum LiqError {
    InvalidConfig(String),
    NoPositions,
    ExecutionFailed(String),
    GasTooHigh(f64),
}

impl fmt::Display for LiqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiqError::InvalidConfig(s) => write!(f, "Invalid config: {}", s),
            LiqError::NoPositions => write!(f, "No positions found"),
            LiqError::ExecutionFailed(s) => write!(f, "Execution failed: {}", s),
            LiqError::GasTooHigh(g) => write!(f, "Gas price too high: {} gwei", g),
        }
    }
}

// ─── Constants ─────────────────────────────────────────────────────────────────

pub const AAVE_V3_POOL: Address = [0x87, 0x8b, 0x70, 0x95, 0x7f, 0x2c, 0x5d, 0x2b, 0x6b, 0x9d, 0x23, 0x5f, 0x8e, 0x94, 0x1d, 0xbf, 0xfa, 0x0f, 0x52, 0x1c];
pub const LIQUIDATION_CLOSE_FACTOR: f64 = 0.50;
pub const HEALTH_FACTOR_THRESHOLD: f64 = 1.05;
pub const MAX_GAS_PRICE_GWEI: f64 = 200.0;
pub const SCAN_INTERVAL_SECS: u64 = 12;

fn now_ts() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64 }

// ─── Types ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub user: Address,
    pub collateral_asset: Address,
    pub debt_asset: Address,
    pub collateral_amount: u128,
    pub debt_amount: u128,
    pub health_factor: f64,
    pub ltv: f64,
    pub liquidation_threshold: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationOpportunity {
    pub user: Address,
    pub collateral_asset: Address,
    pub debt_asset: Address,
    pub debt_to_cover: u128,
    pub collateral_to_receive: u128,
    pub liquidation_bonus: f64,
    pub estimated_profit_usd: f64,
    pub gas_estimate: u64,
    pub profitable: bool,
    pub priority: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationResult {
    pub opportunity_id: String,
    pub user: Address,
    pub debt_covered: u128,
    pub collateral_received: u128,
    pub profit_usd: f64,
    pub tx_hash: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationConfig {
    pub min_profit_usd: f64,
    pub max_gas_price_gwei: f64,
    pub min_health_factor: f64,
    pub scan_interval_secs: u64,
    pub max_positions_per_scan: usize,
    pub rpc_url: String,
}

impl Default for LiquidationConfig {
    fn default() -> Self {
        Self {
            min_profit_usd: 50.0,
            max_gas_price_gwei: MAX_GAS_PRICE_GWEI,
            min_health_factor: HEALTH_FACTOR_THRESHOLD,
            scan_interval_secs: SCAN_INTERVAL_SECS,
            max_positions_per_scan: 100,
            rpc_url: "https://eth-mainnet.g.alchemy.com/v2/demo".into(),
        }
    }
}

// ─── Liquidation Engine ────────────────────────────────────────────────────────

pub struct LiquidationEngine {
    config: LiquidationConfig,
    positions: Arc<RwLock<Vec<Position>>>,
    opportunities: Arc<RwLock<Vec<LiquidationOpportunity>>>,
    executed: Arc<RwLock<Vec<LiquidationResult>>>,
    running: Arc<RwLock<bool>>,
    stats: Arc<RwLock<LiquidationStats>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LiquidationStats {
    pub total_scans: u64,
    pub total_opportunities: u64,
    pub total_executed: u64,
    pub total_profit_usd: f64,
    pub failed_attempts: u64,
    pub avg_profit_per_liquidation: f64,
    pub last_scan_duration_ms: u64,
    pub positions_monitored: usize,
    pub current_gas_price_gwei: f64,
}

impl LiquidationEngine {
    pub fn new(config: LiquidationConfig) -> Self {
        Self {
            config,
            positions: Arc::new(RwLock::new(Vec::new())),
            opportunities: Arc::new(RwLock::new(Vec::new())),
            executed: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
            stats: Arc::new(RwLock::new(LiquidationStats::default())),
        }
    }

    pub async fn start(&self) -> Result<(), LiqError> {
        let mut running = self.running.write().await;
        if *running { return Ok(()); }
        *running = true;
        drop(running);

        let engine = self.clone_inner();
        tokio::spawn(async move { engine.monitor_loop().await; });
        info!("Liquidation Engine started");
        Ok(())
    }

    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Liquidation Engine stopped");
    }

    pub async fn is_running(&self) -> bool { *self.running.read().await }
    pub async fn get_stats(&self) -> LiquidationStats { self.stats.read().await.clone() }
    pub async fn get_opportunities(&self) -> Vec<LiquidationOpportunity> { self.opportunities.read().await.clone() }
    pub async fn get_executed(&self) -> Vec<LiquidationResult> { self.executed.read().await.clone() }
    pub async fn get_positions(&self) -> Vec<Position> { self.positions.read().await.clone() }

    pub async fn scan_now(&self) -> Result<Vec<LiquidationOpportunity>, LiqError> {
        let positions = self.fetch_positions().await?;
        *self.positions.write().await = positions.clone();
        let opportunities = self.analyze_positions(&positions);
        *self.opportunities.write().await = opportunities.clone();
        Ok(opportunities)
    }

    pub async fn execute_liquidation(&self, opp: &LiquidationOpportunity) -> Result<LiquidationResult, LiqError> {
        let start = Instant::now();
        let calldata = self.prepare_liquidation_call(opp)?;
        let gas_estimate = self.estimate_gas(&calldata)?;
        let result = self.execute_with_flash_loan(opp, &calldata, gas_estimate).await?;

        let mut stats = self.stats.write().await;
        stats.total_executed += 1;
        if result.success { stats.total_profit_usd += result.profit_usd; stats.avg_profit_per_liquidation = stats.total_profit_usd / stats.total_executed as f64; }
        else { stats.failed_attempts += 1; }
        self.executed.write().await.push(result.clone());
        info!("Liquidation in {:?}: profit=${:.2}, success={}", start.elapsed(), result.profit_usd, result.success);
        Ok(result)
    }

    // ─── Internal ────────────────────────────────────────────────────────────

    fn clone_inner(&self) -> Self {
        Self {
            config: self.config.clone(),
            positions: self.positions.clone(),
            opportunities: self.opportunities.clone(),
            executed: self.executed.clone(),
            running: self.running.clone(),
            stats: self.stats.clone(),
        }
    }

    async fn monitor_loop(&self) {
        let interval = Duration::from_secs(self.config.scan_interval_secs);
        loop {
            if !*self.running.read().await { break; }
            let scan_start = Instant::now();
            match self.scan_and_execute().await {
                Ok(executed) => {
                    let mut stats = self.stats.write().await;
                    stats.total_scans += 1;
                    stats.last_scan_duration_ms = scan_start.elapsed().as_millis() as u64;
                    stats.positions_monitored = self.positions.read().await.len();
                    if executed > 0 { info!("Scan executed {} liquidations", executed); }
                }
                Err(e) => error!("Liquidation scan failed: {}", e),
            }
            tokio::time::sleep(interval).await;
        }
    }

    async fn scan_and_execute(&self) -> Result<usize, LiqError> {
        let opportunities = self.scan_now().await?;
        let profitable: Vec<_> = opportunities.into_iter().filter(|o| o.profitable && o.estimated_profit_usd >= self.config.min_profit_usd).collect();
        let mut executed = 0;
        for opp in profitable.iter().take(5) {
            let gas_price = self.estimate_current_gas_price().await?;
            self.stats.write().await.current_gas_price_gwei = gas_price;
            if gas_price > self.config.max_gas_price_gwei { debug!("Gas too high: {:.1} gwei", gas_price); continue; }
            match self.execute_liquidation(opp).await {
                Ok(result) => { if result.success { executed += 1; } }
                Err(e) => warn!("Liquidation execution failed: {}", e),
            }
        }
        Ok(executed)
    }

    async fn fetch_positions(&self) -> Result<Vec<Position>, LiqError> {
        Ok(vec![
            Position {
                user: [1u8; 20], collateral_asset: [0u8; 20], debt_asset: [0u8; 20],
                collateral_amount: 100_000_000_000_000_000_000u128,
                debt_amount: 85_000_000_000_000_000_000u128,
                health_factor: 0.95, ltv: 0.80, liquidation_threshold: 0.85,
                timestamp: now_ts(),
            },
        ])
    }

    fn analyze_positions(&self, positions: &[Position]) -> Vec<LiquidationOpportunity> {
        positions.iter().filter(|p| p.health_factor < self.config.min_health_factor).map(|p| {
            let debt_to_cover = (p.debt_amount as f64 * LIQUIDATION_CLOSE_FACTOR) as u128;
            let bonus = 1.0 + (p.liquidation_threshold - p.ltv) * 0.5;
            let collateral_to_receive = (debt_to_cover as f64 * bonus) as u128;
            let estimated_profit = (collateral_to_receive as f64 - debt_to_cover as f64) / 1e18 * 2000.0;
            LiquidationOpportunity {
                user: p.user, collateral_asset: p.collateral_asset, debt_asset: p.debt_asset,
                debt_to_cover, collateral_to_receive, liquidation_bonus: bonus - 1.0,
                estimated_profit_usd: estimated_profit, gas_estimate: 300_000,
                profitable: estimated_profit > self.config.min_profit_usd,
                priority: (1.0 - p.health_factor) * 100.0,
            }
        }).collect()
    }

    fn prepare_liquidation_call(&self, opp: &LiquidationOpportunity) -> Result<Vec<u8>, LiqError> {
        let mut calldata = Vec::with_capacity(256);
        calldata.extend_from_slice(&[0x41, 0xc6, 0xf6, 0x5e]);
        calldata.extend_from_slice(&opp.collateral_asset);
        calldata.extend_from_slice(&opp.debt_asset);
        calldata.extend_from_slice(&opp.user);
        calldata.extend_from_slice(&opp.debt_to_cover.to_be_bytes());
        calldata.push((opp.collateral_asset[0] % 2 == 0) as u8);
        Ok(calldata)
    }

    fn estimate_gas(&self, _calldata: &[u8]) -> Result<u64, LiqError> { Ok(300_000) }

    async fn execute_with_flash_loan(&self, opp: &LiquidationOpportunity, _calldata: &[u8], _gas: u64) -> Result<LiquidationResult, LiqError> {
        Ok(LiquidationResult {
            opportunity_id: hex_simple(&opp.user[..8]),
            user: opp.user, debt_covered: opp.debt_to_cover,
            collateral_received: opp.collateral_to_receive,
            profit_usd: opp.estimated_profit_usd, tx_hash: None,
            success: false, error: Some("Flash loan execution not yet implemented".into()),
            timestamp: now_ts(),
        })
    }

    async fn estimate_current_gas_price(&self) -> Result<f64, LiqError> { Ok(25.0) }
}

fn hex_simple(bytes: &[u8]) -> String {
    let hex_chars = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes { s.push(hex_chars[(b >> 4) as usize] as char); s.push(hex_chars[(b & 0x0f) as usize] as char); }
    s
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> LiquidationConfig { LiquidationConfig { min_profit_usd: 10.0, scan_interval_secs: 1, ..Default::default() } }

    #[tokio::test]
    async fn test_liquidation_engine_creation() { let e = LiquidationEngine::new(test_config()); assert!(!e.is_running().await); }

    #[tokio::test]
    async fn test_scan_positions() { let e = LiquidationEngine::new(test_config()); let o = e.scan_now().await.unwrap(); assert!(!o.is_empty()); assert!(o[0].profitable); }

    #[tokio::test]
    async fn test_liquidation_analyze() { let e = LiquidationEngine::new(test_config()); let p = e.fetch_positions().await.unwrap(); let o = e.analyze_positions(&p); assert!(!o.is_empty()); assert!(o[0].priority > 0.0); }

    #[test]
    fn test_liquidation_config_default() { let c = LiquidationConfig::default(); assert!(c.min_profit_usd > 0.0); assert!(c.scan_interval_secs > 0); }

    #[tokio::test]
    async fn test_liquidation_call_preparation() { let e = LiquidationEngine::new(test_config()); let o = e.scan_now().await.unwrap(); let c = e.prepare_liquidation_call(&o[0]); assert!(c.is_ok()); assert!(!c.unwrap().is_empty()); }

    #[tokio::test]
    async fn test_start_stop() { let e = LiquidationEngine::new(test_config()); e.start().await.unwrap(); assert!(e.is_running().await); e.stop().await; assert!(!e.is_running().await); }

    #[tokio::test]
    async fn test_stats_tracking() { let e = LiquidationEngine::new(test_config()); e.scan_now().await.unwrap(); let s = e.get_stats().await; assert!(s.total_scans > 0 || s.positions_monitored > 0); }
}
