use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::futures_options::DerivativesEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesStatus {
    pub total_contracts: usize,
    pub active_positions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesPosition {
    pub user_id: String,
    pub contract_symbol: String,
    pub quantity: f64,
    pub entry_price: f64,
    pub margin_locked: f64,
    pub unrealized_pnl: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesInstrument {
    pub symbol: String,
    pub underlying: String,
    pub contract_type: String,
    pub strike_price: Option<f64>,
    pub expiry: i64,
    pub multiplier: f64,
    pub margin_requirement: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesStats {
    pub total_contracts: usize,
    pub active_positions: usize,
}

pub struct FuturesOptionsAPI {
    engine: Arc<DerivativesEngine>,
}

impl FuturesOptionsAPI {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(DerivativesEngine::new()),
        }
    }

    pub async fn get_status(&self) -> FuturesStatus {
        let snapshot = self.engine.snapshot();
        FuturesStatus {
            total_contracts: snapshot["total_contracts"].as_u64().unwrap_or(0) as usize,
            active_positions: snapshot["active_positions"].as_u64().unwrap_or(0) as usize,
        }
    }

    pub async fn get_positions(&self) -> Vec<FuturesPosition> {
        let mut all_positions = Vec::new();
        for entry in self.engine.positions.iter() {
            for pos in entry.value() {
                all_positions.push(FuturesPosition {
                    user_id: pos.user_id.clone(),
                    contract_symbol: pos.contract_symbol.clone(),
                    quantity: pos.quantity,
                    entry_price: pos.entry_price,
                    margin_locked: pos.margin_locked,
                    unrealized_pnl: pos.unrealized_pnl,
                });
            }
        }
        all_positions
    }

    pub async fn get_instruments(&self) -> Vec<FuturesInstrument> {
        self.engine
            .contracts
            .iter()
            .map(|entry| {
                let c = entry.value();
                FuturesInstrument {
                    symbol: c.symbol.clone(),
                    underlying: c.underlying.clone(),
                    contract_type: format!("{:?}", c.contract_type),
                    strike_price: c.strike_price,
                    expiry: c.expiry,
                    multiplier: c.multiplier,
                    margin_requirement: c.margin_requirement,
                }
            })
            .collect()
    }

    pub async fn get_stats(&self) -> FuturesStats {
        let snapshot = self.engine.snapshot();
        FuturesStats {
            total_contracts: snapshot["total_contracts"].as_u64().unwrap_or(0) as usize,
            active_positions: snapshot["active_positions"].as_u64().unwrap_or(0) as usize,
        }
    }
}
