use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolId(String);

impl PoolId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityData {
    pub pool_id: PoolId,
    pub token_a: String,
    pub token_b: String,
    pub tvl: Decimal,
    pub volume_24h: Decimal,
    pub fee_apr: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlippageReport {
    pub pool_id: PoolId,
    pub symbol: String,
    pub expected_price: Decimal,
    pub actual_price: Decimal,
    pub slippage_bps: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Display)]
pub enum TradingMode {
    Aggressive,
    Normal,
    Conservative,
    Defensive,
    Halted,
}

impl TradingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradingMode::Aggressive => "aggressive",
            TradingMode::Normal => "normal",
            TradingMode::Conservative => "conservative",
            TradingMode::Defensive => "defensive",
            TradingMode::Halted => "halted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Display)]
pub enum VenueKind {
    Cex,
    Dex,
    Aggregator,
    Bridge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueInfo {
    pub name: String,
    pub kind: VenueKind,
    pub latency_ms: u64,
    pub weight: f64,
}
