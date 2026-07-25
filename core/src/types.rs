use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub address: [u8; 20],
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Price {
    pub value: u128,
    pub decimals: u8,
    pub timestamp: u64,
}

impl Price {
    pub fn new(value: u128, decimals: u8) -> Self {
        Self { value, decimals, timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlashLoanProvider {
    AaveV3, UniswapV3, Balancer, Maker, Dydx,
}
impl FlashLoanProvider {
    pub fn name(&self) -> &'static str {
        match self { Self::AaveV3 => "aave-v3", Self::UniswapV3 => "uniswap-v3", Self::Balancer => "balancer", Self::Maker => "maker", Self::Dydx => "dydx" }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pool {
    pub address: [u8; 20], pub token_a: Token, pub token_b: Token, pub fee: u32, pub liquidity: u128, pub sqrt_price: u128, pub tick: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevenueType { FlashLoanFee, ArbitrageProfit, MevShare, LiquidationFee, TradingFee, Subscription }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStrategy { FlashLoan, DirectSwap, BatchSwap, MevBundle }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueEvent {
    pub timestamp: u64, pub event_type: RevenueType, pub amount: u128, pub token: Token, pub profit: i128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: u64, pub tps: f64, pub tps_peak: f64,
    pub latency_p50: std::time::Duration, pub latency_p90: std::time::Duration, pub latency_p99: std::time::Duration, pub latency_p999: std::time::Duration,
    pub order_count: u64, pub match_count: u64, pub circuit_breaker_triggers: u64, pub revenue_events: Vec<RevenueEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMetrics {
    pub total_value_locked: u128, pub daily_volume: u128, pub total_fees: u128,
    pub active_users: u64, pub total_transactions: u64, pub peak_tps: f64,
}
impl Default for ProtocolMetrics {
    fn default() -> Self { Self { total_value_locked: 0, daily_volume: 0, total_fees: 0, active_users: 0, total_transactions: 0, peak_tps: 0.0 } }
}

// Test constants
pub const AAVE_V3_POOL: &str = "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2";
pub const UNISWAP_V3_FACTORY: &str = "0x1F98431c8aD98523631AE4a59f267346ea31F984";
pub const MIN_TICK: i32 = -887272;
pub const MAX_TICK: i32 = 887272;
pub const MIN_SQRT_RATIO: u128 = 4295128739;
pub const MAX_SQRT_RATIO: u128 = 340282366920938463463374607431768211455;
