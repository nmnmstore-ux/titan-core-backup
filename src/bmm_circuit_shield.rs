use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};
use uuid::Uuid;

use crate::bmm_amm::{BmmEngine, BmmQuote};
use crate::circuit_breaker::{CircuitAction, CircuitBreaker, CircuitBreakerConfig, CircuitState};
use crate::types::{OrderSide, Trade};

const BMM_FEE_BPS: u32 = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmmShieldConfig {
    pub cooldown_secs: u64,
    pub max_move_percent: f64,
}

impl Default for BmmShieldConfig {
    fn default() -> Self {
        Self {
            cooldown_secs: 30,
            max_move_percent: 10.0,
        }
    }
}

impl BmmShieldConfig {
    fn cooldown_duration(&self) -> Duration {
        Duration::from_secs(self.cooldown_secs)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmmShieldStatus {
    pub pair: String,
    pub circuit_state: CircuitState,
    pub cooldown_remaining_ms: u64,
    pub trades_protected: u64,
    pub revenue_protected_usd: f64,
    pub halted_swaps: u64,
}

pub enum BmmShieldedSwap {
    Executed(Trade),
    Halted { reason: String },
    NoQuote,
}

pub struct BmmCircuitShield {
    engine: Arc<BmmEngine>,
    breaker: Arc<CircuitBreaker>,
    config: BmmShieldConfig,
    last_trigger: DashMap<String, std::time::Instant>,
    trades_protected: AtomicU64,
    halted_swaps: AtomicU64,
    revenue_protected_usd: Mutex<f64>,
}

impl BmmCircuitShield {
    pub fn new(engine: Arc<BmmEngine>, config: BmmShieldConfig) -> Self {
        Self {
            engine,
            breaker: Arc::new(CircuitBreaker::new()),
            config,
            last_trigger: DashMap::new(),
            trades_protected: AtomicU64::new(0),
            halted_swaps: AtomicU64::new(0),
            revenue_protected_usd: Mutex::new(0.0),
        }
    }

    pub fn breaker(&self) -> &Arc<CircuitBreaker> {
        &self.breaker
    }

    pub fn register_pair(&self, pair: &str) {
        if self.breaker.get_config(pair).is_none() {
            let breaker_config = CircuitBreakerConfig {
                max_move_percent: self.config.max_move_percent,
                ..CircuitBreakerConfig::default()
            };
            self.breaker.register_pair_with_config(pair, breaker_config);
        }
    }

    fn is_halted(&self, pair: &str) -> bool {
        matches!(
            self.breaker.get_state(pair),
            Some(CircuitState::Level2) | Some(CircuitState::Level3)
        )
    }

    fn cooldown_active(&self, pair: &str) -> bool {
        let key = pair.to_uppercase();
        match self.last_trigger.get(&key) {
            Some(trigger) => trigger.elapsed() < self.config.cooldown_duration(),
            None => false,
        }
    }

    fn protect_revenue(&self, amount_in: f64) {
        if let Ok(mut rev) = self.revenue_protected_usd.lock() {
            *rev += amount_in * (BMM_FEE_BPS as f64 / 10_000.0);
        }
    }

    pub fn feed_price(&self, pair: &str, price: f64) -> Option<CircuitAction> {
        self.register_pair(pair);
        let action = self.breaker.record_trade(pair, price);
        if matches!(
            action,
            Some(CircuitAction::PauseTrading) | Some(CircuitAction::ActivateKillShield)
        ) {
            self.last_trigger.insert(pair.to_uppercase(), std::time::Instant::now());
        }
        action
    }

    async fn quote_price(&self, pair: &str, side: OrderSide, amount_in: f64) -> Option<BmmQuote> {
        self.engine.get_quote(pair, side, amount_in).await
    }

    pub async fn execute_swap_shielded(
        &self,
        pair: &str,
        side: OrderSide,
        amount_in: f64,
        user_id: Uuid,
    ) -> BmmShieldedSwap {
        self.register_pair(pair);

        if self.is_halted(pair) {
            if self.cooldown_active(pair) {
                self.halted_swaps.fetch_add(1, Ordering::Relaxed);
                self.protect_revenue(amount_in);
                let status = self.breaker.get_state(pair).unwrap_or(CircuitState::Normal);
                warn!(
                    "BMM shield: swap REJECTED for {} (state={}, cooldown active)",
                    pair, status
                );
                let reason = format!("circuit_breaker halted: {} cooldown active", status);
                return BmmShieldedSwap::Halted { reason };
            }
            self.breaker.reset(pair);
            self.last_trigger.remove(&pair.to_uppercase());
            info!("BMM shield: cooldown expired, {} resumed", pair);
        }

        let quote = match self.quote_price(pair, side, amount_in).await {
            Some(q) => q,
            None => return BmmShieldedSwap::NoQuote,
        };

        if let Some(action) = self.feed_price(pair, quote.spot_after) {
            if matches!(
                action,
                CircuitAction::PauseTrading | CircuitAction::ActivateKillShield
            ) {
                self.halted_swaps.fetch_add(1, Ordering::Relaxed);
                self.protect_revenue(amount_in);
                warn!("BMM swap REJECTED for {}: {:?}", pair, action);
                return BmmShieldedSwap::Halted {
                    reason: format!("circuit_breaker triggered halt: {:?}", action),
                };
            }
            info!("BMM shield: {} level1 action, swap allowed", pair);
        }

        match self.engine.execute_swap(pair, side, amount_in, user_id).await {
            Some(trade) => {
                self.trades_protected.fetch_add(1, Ordering::Relaxed);
                BmmShieldedSwap::Executed(trade)
            }
            None => BmmShieldedSwap::NoQuote,
        }
    }

    pub async fn shield_status(&self, pair: &str) -> BmmShieldStatus {
        self.register_pair(pair);
        let state = self.breaker.get_state(pair).unwrap_or(CircuitState::Normal);

        let cooldown_remaining_ms = if self.is_halted(pair) {
            let key = pair.to_uppercase();
            match self.last_trigger.get(&key) {
                Some(t) => {
                    let elapsed = t.elapsed();
                    let total = self.config.cooldown_duration();
                    if elapsed >= total {
                        0
                    } else {
                        (total - elapsed).as_millis() as u64
                    }
                }
                None => 0,
            }
        } else {
            0
        };

        let revenue = match self.revenue_protected_usd.lock() {
            Ok(rev) => *rev,
            Err(poisoned) => *poisoned.into_inner(),
        };

        BmmShieldStatus {
            pair: pair.to_uppercase(),
            circuit_state: state,
            cooldown_remaining_ms,
            trades_protected: self.trades_protected.load(Ordering::Relaxed),
            revenue_protected_usd: revenue,
            halted_swaps: self.halted_swaps.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn make_engine() -> Arc<BmmEngine> {
        let mut config = crate::bmm_amm::BmmConfig::default();
        config.initial_liquidity_usd = 100_000_000.0;
        let engine = BmmEngine::new(config);
        engine.start().await.unwrap();
        Arc::new(engine)
    }

    fn new_shield(engine: Arc<BmmEngine>, cooldown_secs: u64) -> BmmCircuitShield {
        BmmCircuitShield::new(
            engine,
            BmmShieldConfig {
                cooldown_secs,
                max_move_percent: 10.0,
            },
        )
    }

    #[tokio::test]
    async fn normal_swap_does_not_halt() {
        let engine = make_engine().await;
        let shield = new_shield(engine, 30);

        let user = Uuid::new_v4();
        for _ in 0..5 {
            let result = shield
                .execute_swap_shielded("BTC/USDT", OrderSide::Buy, 1000.0, user)
                .await;
            assert!(matches!(result, BmmShieldedSwap::Executed(_)));
        }

        let status = shield.shield_status("BTC/USDT").await;
        assert_eq!(status.circuit_state, CircuitState::Normal);
        assert_eq!(status.halted_swaps, 0);
        assert_eq!(status.revenue_protected_usd, 0.0);
        assert_eq!(status.trades_protected, 5);
    }

    #[tokio::test]
    async fn violent_spike_halts_swap() {
        let engine = make_engine().await;
        let shield = new_shield(engine, 30);

        for _ in 0..3 {
            shield.feed_price("BTC/USDT", 100.0);
        }
        assert!(shield.feed_price("BTC/USDT", 100.0).is_none());
        assert_eq!(shield.shield_status("BTC/USDT").await.circuit_state, CircuitState::Normal);

        shield.feed_price("BTC/USDT", 500.0);
        shield.feed_price("BTC/USDT", 500.0);

        let status = shield.shield_status("BTC/USDT").await;
        assert!(matches!(
            status.circuit_state,
            CircuitState::Level2 | CircuitState::Level3
        ));

        let result = shield
            .execute_swap_shielded("BTC/USDT", OrderSide::Sell, 1000.0, Uuid::new_v4())
            .await;
        assert!(matches!(result, BmmShieldedSwap::Halted { .. }));
    }

    #[tokio::test]
    async fn cooldown_resumes_swaps() {
        let engine = make_engine().await;
        let shield = new_shield(engine, 1);

        for _ in 0..3 {
            shield.feed_price("ETH/USDT", 200.0);
        }
        shield.feed_price("ETH/USDT", 900.0);
        shield.feed_price("ETH/USDT", 900.0);

        let blocked = shield
            .execute_swap_shielded("ETH/USDT", OrderSide::Buy, 500.0, Uuid::new_v4())
            .await;
        assert!(matches!(blocked, BmmShieldedSwap::Halted { .. }));

        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let resumed = shield
            .execute_swap_shielded("ETH/USDT", OrderSide::Buy, 500.0, Uuid::new_v4())
            .await;
        assert!(matches!(resumed, BmmShieldedSwap::Executed(_)));
        assert_eq!(shield.shield_status("ETH/USDT").await.circuit_state, CircuitState::Normal);
    }

    #[tokio::test]
    async fn revenue_protected_increments_on_halt() {
        let engine = make_engine().await;
        let shield = new_shield(engine, 30);

        for _ in 0..3 {
            shield.feed_price("ETH/BTC", 0.5);
        }
        shield.feed_price("ETH/BTC", 3.0);
        shield.feed_price("ETH/BTC", 3.0);

        let amount_in = 10_000.0;
        let result = shield
            .execute_swap_shielded("ETH/BTC", OrderSide::Buy, amount_in, Uuid::new_v4())
            .await;
        assert!(matches!(result, BmmShieldedSwap::Halted { .. }));

        let status = shield.shield_status("ETH/BTC").await;
        assert_eq!(status.halted_swaps, 1);
        let expected = amount_in * (50.0 / 10_000.0);
        assert!((status.revenue_protected_usd - expected).abs() < 1e-6);
        assert_eq!(status.trades_protected, 0);
    }
}