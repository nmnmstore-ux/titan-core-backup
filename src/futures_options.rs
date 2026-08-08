use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use dashmap::DashMap;
use chrono::Utc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DerivativeType {
    Future,
    CallOption,
    PutOption,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivativeContract {
    pub symbol: String,
    pub underlying: String,
    pub contract_type: DerivativeType,
    pub strike_price: Option<f64>,
    pub expiry: i64,
    pub multiplier: f64,
    pub margin_requirement: f64, // 0.1 = 10%
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPosition {
    pub user_id: String,
    pub contract_symbol: String,
    pub quantity: f64,
    pub entry_price: f64,
    pub margin_locked: f64,
    pub unrealized_pnl: f64,
}

pub struct DerivativesEngine {
    pub contracts: Arc<DashMap<String, DerivativeContract>>,
    pub positions: Arc<DashMap<String, Vec<OpenPosition>>>,
    pub next_contract_id: AtomicU64,
}

impl DerivativesEngine {
    pub fn new() -> Self {
        let engine = Self {
            contracts: Arc::new(DashMap::new()),
            positions: Arc::new(DashMap::new()),
            next_contract_id: AtomicU64::new(1),
        };
        engine.seed_defaults();
        engine
    }

    fn seed_defaults(&self) {
        let now = Utc::now().timestamp_millis();
        let expiry = now + (30 * 24 * 3600 * 1000); // 30 days

        self.contracts.insert("BTC-2026-FUT".into(), DerivativeContract {
            symbol: "BTC-2026-FUT".into(),
            underlying: "BTC/USD".into(),
            contract_type: DerivativeType::Future,
            strike_price: None,
            expiry,
            multiplier: 1.0,
            margin_requirement: 0.05,
        });

        self.contracts.insert("ETH-C-3500".into(), DerivativeContract {
            symbol: "ETH-C-3500".into(),
            underlying: "ETH/USD".into(),
            contract_type: DerivativeType::CallOption,
            strike_price: Some(3500.0),
            expiry,
            multiplier: 10.0,
            margin_requirement: 0.15,
        });
    }

    pub fn calculate_margin(&self, symbol: &str, quantity: f64, price: f64) -> Result<f64, String> {
        let contract = self.contracts.get(symbol).ok_or("Contract not found")?;
        Ok(quantity * price * contract.multiplier * contract.margin_requirement)
    }

    pub fn open_position(&self, user_id: String, symbol: &str, quantity: f64, price: f64) -> Result<OpenPosition, String> {
        let margin = self.calculate_margin(symbol, quantity, price)?;
        let position = OpenPosition {
            user_id: user_id.clone(),
            contract_symbol: symbol.to_string(),
            quantity,
            entry_price: price,
            margin_locked: margin,
            unrealized_pnl: 0.0,
        };
        self.positions.entry(user_id).or_default().push(position.clone());
        Ok(position)
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "total_contracts": self.contracts.len(),
            "active_positions": self.positions.len(),
        })
    }
}
