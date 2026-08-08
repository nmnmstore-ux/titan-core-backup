use serde::{Deserialize, Serialize};
use std::sync::Arc;
use the_bridge_arbitrage::flash_loan_arb_v2::{
    FlashLoanArbitrageEngine, FlashLoanArbConfig, ArbOpportunity,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashLoanStatus {
    pub running: bool,
    pub uptime_seconds: u64,
    pub total_scans: u64,
    pub total_opportunities_found: u64,
    pub total_trades_executed: u64,
    pub total_profit_usd: f64,
    pub pool_count: usize,
    pub active_trades: usize,
    pub circuit_breaker_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashLoanOpportunity {
    pub id: String,
    pub chain: String,
    pub dexes: Vec<String>,
    pub path: Vec<String>,
    pub expected_profit_usd: f64,
    pub expected_profit_bps: u32,
    pub net_profit_usd: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashLoanResult {
    pub trade_id: String,
    pub success: bool,
    pub net_profit_usd: f64,
    pub gas_cost_usd: f64,
    pub tx_hash: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashLoanTrade {
    pub id: String,
    pub opportunity_id: String,
    pub chain: String,
    pub path: Vec<String>,
    pub token_in: String,
    pub token_out: String,
    pub net_profit_usd: f64,
    pub success: bool,
    pub duration_ms: u64,
    pub executed_at: u64,
}

pub struct FlashLoanAPIV2 {
    engine: Arc<FlashLoanArbitrageEngine>,
}

impl FlashLoanAPIV2 {
    pub fn new() -> Self {
        let mut config = FlashLoanArbConfig::default();
        config.enabled_chains = vec!["ethereum".into(), "bsc".into(), "polygon".into(), "arbitrum".into(), "optimism".into()];
        config.scan_interval_ms = 800;
        config.min_profit_usd = 0.5;
        Self {
            engine: Arc::new(FlashLoanArbitrageEngine::new(config)),
        }
    }

    pub async fn get_status(&self) -> FlashLoanStatus {
        let stats = self.engine.get_engine_stats().await;
        let pnl = self.engine.get_pnl().await;
        FlashLoanStatus {
            running: self.engine.is_running(),
            uptime_seconds: stats.uptime_seconds,
            total_scans: stats.total_scans,
            total_opportunities_found: stats.total_opportunities_found,
            total_trades_executed: stats.total_trades_executed,
            total_profit_usd: pnl.total_net_profit,
            pool_count: stats.pool_count,
            active_trades: stats.current_active_trades,
            circuit_breaker_active: stats.circuit_breaker_active,
        }
    }

    pub async fn get_opportunities(&self) -> Vec<FlashLoanOpportunity> {
        let detector = &self.engine.detector;
        let queue_size = detector.queue_size().await;
        if queue_size == 0 {
            return Vec::new();
        }
        let mut opportunities = Vec::new();
        for _ in 0..queue_size.min(50) {
            if let Some(opp) = detector.next_opportunity().await {
                opportunities.push(FlashLoanOpportunity {
                    id: opp.id,
                    chain: opp.chain,
                    dexes: opp.dexes,
                    path: opp.path,
                    expected_profit_usd: opp.expected_profit_usd,
                    expected_profit_bps: opp.expected_profit_bps,
                    net_profit_usd: opp.net_profit_usd,
                    confidence: opp.confidence,
                });
            }
        }
        opportunities
    }

    pub async fn execute(&self, opportunity_id: &str) -> Result<FlashLoanResult, String> {
        let opportunities = self.get_opportunities().await;
        let opp = opportunities
            .iter()
            .find(|o| o.id == opportunity_id)
            .ok_or_else(|| format!("opportunity {} not found", opportunity_id))?;

        let arb_opp = ArbOpportunity {
            id: opp.id.clone(),
            chain: opp.chain.clone(),
            dexes: opp.dexes.clone(),
            path: opp.path.clone(),
            token_in: String::new(),
            token_out: String::new(),
            amount_in: 0,
            expected_amount_out: 0,
            expected_profit_usd: opp.expected_profit_usd,
            expected_profit_bps: opp.expected_profit_bps,
            gas_estimate_usd: 0.0,
            flash_loan_fee_usd: 0.0,
            net_profit_usd: opp.net_profit_usd,
            confidence: opp.confidence,
            slippage_bps: 30,
            max_position_size: 0,
            detected_at: 0,
        };

        let trade = self.engine.executor.execute(arb_opp).await;

        Ok(FlashLoanResult {
            trade_id: trade.id,
            success: trade.success,
            net_profit_usd: trade.net_profit_usd,
            gas_cost_usd: trade.gas_cost_usd,
            tx_hash: trade.tx_hash,
            error: trade.error,
        })
    }

    pub async fn get_history(&self, n: usize) -> Vec<FlashLoanTrade> {
        let trades = self.engine.get_recent_trades(n).await;
        trades
            .into_iter()
            .map(|t| FlashLoanTrade {
                id: t.id,
                opportunity_id: t.opportunity_id,
                chain: t.chain,
                path: t.path,
                token_in: t.token_in,
                token_out: t.token_out,
                net_profit_usd: t.net_profit_usd,
                success: t.success,
                duration_ms: t.duration_ms,
                executed_at: t.executed_at,
            })
            .collect()
    }

    pub async fn start(&self) -> Result<(), String> {
        self.engine.run().await
    }
}
