use serde::{Deserialize, Serialize};
use std::sync::Arc;
use the_bridge_flash_loan::FlashLoanRouter;
use crate::liquidation::{LiquidationEngine, LiquidationConfig, LiquidationStats};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationStatus {
    pub running: bool,
    pub total_scans: u64,
    pub total_opportunities: u64,
    pub total_executed: u64,
    pub total_profit_usd: f64,
    pub positions_monitored: usize,
    pub current_gas_price_gwei: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskyPosition {
    pub user: String,
    pub collateral_asset: String,
    pub debt_asset: String,
    pub collateral_amount: u128,
    pub debt_amount: u128,
    pub health_factor: f64,
    pub ltv: f64,
    pub liquidation_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationEvent {
    pub opportunity_id: String,
    pub user: String,
    pub debt_covered: u128,
    pub collateral_received: u128,
    pub profit_usd: f64,
    pub tx_hash: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    pub timestamp: i64,
}

pub struct LiquidationAPI {
    engine: Arc<LiquidationEngine>,
}

impl LiquidationAPI {
    pub fn new(config: LiquidationConfig) -> Self {
        let router = Arc::new(FlashLoanRouter::new(vec![]));
        Self {
            engine: Arc::new(LiquidationEngine::new(config, router)),
        }
    }

    pub async fn get_status(&self) -> LiquidationStatus {
        let stats = self.engine.get_stats().await;
        LiquidationStatus {
            running: self.engine.is_running().await,
            total_scans: stats.total_scans,
            total_opportunities: stats.total_opportunities,
            total_executed: stats.total_executed,
            total_profit_usd: stats.total_profit_usd,
            positions_monitored: stats.positions_monitored,
            current_gas_price_gwei: stats.current_gas_price_gwei,
        }
    }

    pub async fn get_positions_at_risk(&self) -> Vec<RiskyPosition> {
        let positions = self.engine.get_positions().await;
        let config_min_hf = 1.05;
        positions
            .into_iter()
            .filter(|p| p.health_factor < config_min_hf)
            .map(|p| RiskyPosition {
                user: hex_simple(&p.user),
                collateral_asset: hex_simple(&p.collateral_asset),
                debt_asset: hex_simple(&p.debt_asset),
                collateral_amount: p.collateral_amount,
                debt_amount: p.debt_amount,
                health_factor: p.health_factor,
                ltv: p.ltv,
                liquidation_threshold: p.liquidation_threshold,
            })
            .collect()
    }

    pub async fn get_history(&self) -> Vec<LiquidationEvent> {
        let executed = self.engine.get_executed().await;
        executed
            .into_iter()
            .map(|r| LiquidationEvent {
                opportunity_id: r.opportunity_id,
                user: hex_simple(&r.user),
                debt_covered: r.debt_covered,
                collateral_received: r.collateral_received,
                profit_usd: r.profit_usd,
                tx_hash: r.tx_hash,
                success: r.success,
                error: r.error,
                timestamp: r.timestamp,
            })
            .collect()
    }

    pub async fn get_stats(&self) -> LiquidationStats {
        self.engine.get_stats().await
    }
}

fn hex_simple(bytes: &[u8]) -> String {
    let hex_chars = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(hex_chars[(b >> 4) as usize] as char);
        s.push(hex_chars[(b & 0x0f) as usize] as char);
    }
    s
}
