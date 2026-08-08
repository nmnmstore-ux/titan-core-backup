use serde::{Deserialize, Serialize};
use std::sync::Arc;
use the_bridge_mev_protection::mev_extraction_engine::{
    MevExtractionEngine, MevExtractionConfig,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MEVStatus {
    pub running: bool,
    pub uptime_seconds: u64,
    pub total_scans: u64,
    pub mempool_size: usize,
    pub circuit_breaker: bool,
    pub last_scan_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MEVThreat {
    pub id: String,
    pub op_type: String,
    pub target_tx_hash: String,
    pub protocol: String,
    pub amount_eth: f64,
    pub estimated_profit_usd: f64,
    pub estimated_gas_usd: f64,
    pub net_profit_usd: f64,
    pub confidence: f64,
    pub block_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MEVStats {
    pub total_bundles: u64,
    pub confirmed_bundles: u64,
    pub failed_bundles: u64,
    pub success_rate: f64,
    pub total_profit_usd: f64,
    pub total_gas_usd: f64,
    pub average_profit_usd: f64,
    pub best_trade_usd: f64,
    pub worst_trade_usd: f64,
    pub daily_pnl: f64,
    pub weekly_pnl: f64,
    pub monthly_pnl: f64,
    pub total_analyzed: u64,
    pub sandwiches: u64,
    pub liquidations: u64,
    pub backruns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MEVIncident {
    pub id: String,
    pub opportunity_id: String,
    pub op_type: String,
    pub block_number: u64,
    pub profit_usd: f64,
    pub gas_cost_usd: f64,
    pub success: bool,
    pub confirmed: bool,
    pub error: Option<String>,
    pub executed_at: u64,
    pub duration_ms: u64,
}

pub struct MEVProtectionAPI {
    engine: Arc<MevExtractionEngine>,
}

impl MEVProtectionAPI {
    pub fn new() -> Self {
        let config = MevExtractionConfig::default();
        Self {
            engine: Arc::new(MevExtractionEngine::new(config)),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        self.engine.run().await
    }

    pub async fn get_status(&self) -> MEVStatus {
        let stats = self.engine.get_stats().await;
        MEVStatus {
            running: self.engine.is_running(),
            uptime_seconds: stats.uptime_seconds,
            total_scans: stats.total_scans,
            mempool_size: stats.mempool_size,
            circuit_breaker: stats.circuit_breaker,
            last_scan_time: stats.last_scan_time,
        }
    }

    pub async fn get_threats(&self) -> Vec<MEVThreat> {
        let _mempool_stats = self.engine.mempool.get_stats().await;
        let mut threats = Vec::new();
        let pending = self.engine.mempool.drain_opportunities().await;
        let block_number = 20000000;
        let opportunities = self.engine.detector.analyze(pending, block_number).await;

        for opp in opportunities.into_iter().take(20) {
            threats.push(MEVThreat {
                id: opp.id,
                op_type: opp.op_type,
                target_tx_hash: opp.target_tx_hash,
                protocol: opp.protocol,
                amount_eth: opp.amount_eth,
                estimated_profit_usd: opp.estimated_profit_usd,
                estimated_gas_usd: opp.estimated_gas_usd,
                net_profit_usd: opp.net_profit_usd,
                confidence: opp.confidence,
                block_number: opp.block_number,
            });
        }
        threats
    }

    pub async fn get_stats(&self) -> MEVStats {
        let pnl = self.engine.get_pnl().await;
        let detector_stats = self.engine.detector.get_stats().await;
        MEVStats {
            total_bundles: pnl.total_bundles,
            confirmed_bundles: pnl.confirmed_bundles,
            failed_bundles: pnl.failed_bundles,
            success_rate: pnl.success_rate,
            total_profit_usd: pnl.total_profit_usd,
            total_gas_usd: pnl.total_gas_usd,
            average_profit_usd: pnl.average_profit_usd,
            best_trade_usd: pnl.best_trade_usd,
            worst_trade_usd: pnl.worst_trade_usd,
            daily_pnl: pnl.daily_pnl,
            weekly_pnl: pnl.weekly_pnl,
            monthly_pnl: pnl.monthly_pnl,
            total_analyzed: detector_stats.total_analyzed,
            sandwiches: detector_stats.sandwiches,
            liquidations: detector_stats.liquidations,
            backruns: detector_stats.backruns,
        }
    }

    pub async fn get_history(&self, n: usize) -> Vec<MEVIncident> {
        let trades = self.engine.get_recent_trades(n).await;
        trades
            .into_iter()
            .map(|t| MEVIncident {
                id: t.id,
                opportunity_id: t.opportunity_id,
                op_type: t.op_type,
                block_number: t.block_number,
                profit_usd: t.profit_usd,
                gas_cost_usd: t.gas_cost_usd,
                success: t.success,
                confirmed: t.confirmed,
                error: t.error,
                executed_at: t.executed_at,
                duration_ms: t.duration_ms,
            })
            .collect()
    }
}
