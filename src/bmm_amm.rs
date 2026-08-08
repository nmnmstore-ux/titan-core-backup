use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use compact_str::CompactString;
use crate::types::{OrderSide, Trade};

const BMM_FEE_BPS: u32 = 50;
const BMM_MIN_RESERVE_USD: f64 = 10_000.0;
const BMM_MAX_SLIPPAGE_BPS: u32 = 500;
const BMM_REBALANCE_INTERVAL_SECS: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmmConfig {
    pub fee_bps: u32,
    pub min_reserve_usd: f64,
    pub max_slippage_bps: u32,
    pub rebalance_interval_secs: u64,
    pub enabled_pairs: Vec<String>,
    pub initial_liquidity_usd: f64,
}

impl Default for BmmConfig {
    fn default() -> Self {
        Self {
            fee_bps: BMM_FEE_BPS,
            min_reserve_usd: BMM_MIN_RESERVE_USD,
            max_slippage_bps: BMM_MAX_SLIPPAGE_BPS,
            rebalance_interval_secs: BMM_REBALANCE_INTERVAL_SECS,
            enabled_pairs: vec![
                "BTC/USDT".to_string(),
                "ETH/USDT".to_string(),
                "ETH/BTC".to_string(),
            ],
            initial_liquidity_usd: 10_000_000.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolState {
    pub pair: String,
    pub reserve_x: f64,
    pub reserve_y: f64,
    pub k: f64,
    pub total_lp_tokens: f64,
    pub fee_accumulated_x: f64,
    pub fee_accumulated_y: f64,
    pub trade_count: u64,
    pub last_price: f64,
    pub last_update: i64,
}

impl PoolState {
    pub fn new(pair: &str, reserve_x: f64, reserve_y: f64) -> Self {
        let k = reserve_x.powi(4) * reserve_y;
        Self {
            pair: pair.to_string(),
            reserve_x,
            reserve_y,
            k,
            total_lp_tokens: (reserve_x * reserve_y).sqrt(),
            fee_accumulated_x: 0.0,
            fee_accumulated_y: 0.0,
            trade_count: 0,
            last_price: reserve_y / reserve_x,
            last_update: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn spot_price(&self) -> f64 {
        if self.reserve_x <= 0.0 {
            return f64::MAX;
        }
        self.reserve_y / self.reserve_x
    }

    pub fn marginal_price(&self) -> f64 {
        if self.reserve_x <= 0.0 {
            return f64::MAX;
        }
        4.0 * self.reserve_y / self.reserve_x
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmmQuote {
    pub pair: String,
    pub side: String,
    pub amount_in: f64,
    pub amount_out: f64,
    pub price_impact_pct: f64,
    pub fee: f64,
    pub min_received: f64,
    pub spot_before: f64,
    pub spot_after: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmmPoolStats {
    pub pools: Vec<PoolState>,
    pub total_volume_usd: f64,
    pub total_fees_usd: f64,
    pub total_trades: u64,
}

pub struct BmmEngine {
    config: BmmConfig,
    pools: Arc<RwLock<HashMap<String, PoolState>>>,
    running: Arc<RwLock<bool>>,
    volume_tracker: Arc<RwLock<f64>>,
}

impl BmmEngine {
    pub fn new(config: BmmConfig) -> Self {
        Self {
            config,
            pools: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
            volume_tracker: Arc::new(RwLock::new(0.0)),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        {
            let mut running = self.running.write().await;
            *running = true;
        }

        let mut pools = self.pools.write().await;
        for pair in &self.config.enabled_pairs {
            let initial = self.config.initial_liquidity_usd;
            let reserve_x = initial * 0.5;
            let reserve_y = initial * 0.5;
            let pool = PoolState::new(pair, reserve_x, reserve_y);
            let k = pool.k;
            pools.insert(pair.clone(), pool);
            info!(
                "BMM pool initialized: {} | reserve_x={:.2} reserve_y={:.2} K={:.6e}",
                pair, reserve_x, reserve_y, k
            );
        }

        info!(
            "BMM X⁴Y=K Engine started — {} pools, fee={}bps",
            pools.len(),
            self.config.fee_bps
        );
        Ok(())
    }

    pub async fn get_quote(&self, pair: &str, side: OrderSide, amount_in: f64) -> Option<BmmQuote> {
        let pools = self.pools.read().await;
        let pool = pools.get(pair)?;

        let fee_rate = self.config.fee_bps as f64 / 10_000.0;
        let amount_after_fee = amount_in * (1.0 - fee_rate);

        let (amount_out, spot_before, spot_after) = match side {
            OrderSide::Buy => {
                // Buying Y with X: Y out = reserve_y - (K / (reserve_x + dx)^4)^{1/1}
                // Simplified: new_reserve_x = reserve_x + dx, new_reserve_y = K / new_reserve_x^4
                // But K = reserve_x^4 * reserve_y, so:
                // new_reserve_y = (reserve_x^4 * reserve_y) / (reserve_x + dx)^4
                // Y_out = reserve_y - new_reserve_y = reserve_y * (1 - (reserve_x / (reserve_x + dx))^4)
                let spot_before = pool.spot_price();
                let ratio = pool.reserve_x / (pool.reserve_x + amount_after_fee);
                let new_reserve_y = pool.reserve_y * ratio.powi(4);
                let y_out = pool.reserve_y - new_reserve_y;
                let spot_after = new_reserve_y / (pool.reserve_x + amount_after_fee);
                (y_out, spot_before, spot_after)
            }
            OrderSide::Sell => {
                // Selling Y for X: X out = reserve_x - (K / (reserve_y + dy)^{1/4})
                // K = reserve_x^4 * reserve_y
                // new_reserve_y = reserve_y + dy
                // new_reserve_x = (K / new_reserve_y)^{1/4} = reserve_x * (reserve_y / (reserve_y + dy))^{1/4}
                // X_out = reserve_x - new_reserve_x = reserve_x * (1 - (reserve_y / (reserve_y + dy))^{1/4})
                let spot_before = 1.0 / pool.spot_price();
                let ratio = pool.reserve_y / (pool.reserve_y + amount_after_fee);
                let new_reserve_x = pool.reserve_x * ratio.powf(0.25);
                let x_out = pool.reserve_x - new_reserve_x;
                let spot_after = (pool.reserve_y + amount_after_fee) / new_reserve_x;
                (x_out, spot_before, 1.0 / spot_after)
            }
        };

        if amount_out <= 0.0 || !amount_out.is_finite() {
            return None;
        }

        let price_impact_pct = ((spot_after - spot_before).abs() / spot_before * 100.0).abs();
        let slippage_bps = (price_impact_pct * 100.0) as u32;

        if slippage_bps > self.config.max_slippage_bps {
            warn!(
                "BMM quote rejected: {} slippage {}bps > max {}bps",
                pair, slippage_bps, self.config.max_slippage_bps
            );
            return None;
        }

        let fee = amount_in * fee_rate;

        Some(BmmQuote {
            pair: pair.to_string(),
            side: match side {
                OrderSide::Buy => "buy".to_string(),
                OrderSide::Sell => "sell".to_string(),
            },
            amount_in,
            amount_out,
            price_impact_pct,
            fee,
            min_received: amount_out * 0.995,
            spot_before,
            spot_after,
        })
    }

    pub async fn execute_swap(
        &self,
        pair: &str,
        side: OrderSide,
        amount_in: f64,
        user_id: Uuid,
    ) -> Option<Trade> {
        let _quote = self.get_quote(pair, side, amount_in).await?;

        let mut pools = self.pools.write().await;
        let pool = pools.get_mut(pair)?;

        let fee_rate = self.config.fee_bps as f64 / 10_000.0;
        let amount_after_fee = amount_in * (1.0 - fee_rate);

        match side {
            OrderSide::Buy => {
                let ratio = pool.reserve_x / (pool.reserve_x + amount_after_fee);
                let new_reserve_y = pool.reserve_y * ratio.powi(4);
                let y_out = pool.reserve_y - new_reserve_y;

                pool.reserve_x += amount_after_fee;
                pool.reserve_y = new_reserve_y;
                pool.k = pool.reserve_x.powi(4) * pool.reserve_y;
                pool.fee_accumulated_x += amount_in * fee_rate;
                pool.trade_count += 1;
                pool.last_price = pool.spot_price();
                pool.last_update = chrono::Utc::now().timestamp_millis();

                let trade = Trade {
                    id: Uuid::new_v4(),
                    buy_order_id: Uuid::new_v4(),
                    sell_order_id: Uuid::new_v4(),
                    pair: CompactString::from(pair),
                    price: pool.last_price,
                    quantity: y_out,
                    total: y_out * pool.last_price,
                    buy_user_id: user_id,
                    sell_user_id: Uuid::nil(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    dot_settled: false,
                    tee_notarized: false,
                };

                info!(
                    "BMM swap: BUY {} | in={:.4} out={:.4} price={:.4} fee={:.4}",
                    pair, amount_in, y_out, pool.last_price, amount_in * fee_rate
                );

                let mut vol = self.volume_tracker.write().await;
                *vol += amount_in;

                Some(trade)
            }
            OrderSide::Sell => {
                let ratio = pool.reserve_y / (pool.reserve_y + amount_after_fee);
                let new_reserve_x = pool.reserve_x * ratio.powf(0.25);
                let x_out = pool.reserve_x - new_reserve_x;

                pool.reserve_y += amount_after_fee;
                pool.reserve_x = new_reserve_x;
                pool.k = pool.reserve_x.powi(4) * pool.reserve_y;
                pool.fee_accumulated_y += amount_in * fee_rate;
                pool.trade_count += 1;
                pool.last_price = pool.spot_price();
                pool.last_update = chrono::Utc::now().timestamp_millis();

                let trade = Trade {
                    id: Uuid::new_v4(),
                    buy_order_id: Uuid::new_v4(),
                    sell_order_id: Uuid::new_v4(),
                    pair: CompactString::from(pair),
                    price: pool.last_price,
                    quantity: x_out,
                    total: x_out * pool.last_price,
                    buy_user_id: Uuid::nil(),
                    sell_user_id: user_id,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    dot_settled: false,
                    tee_notarized: false,
                };

                info!(
                    "BMM swap: SELL {} | in={:.4} out={:.4} price={:.4} fee={:.4}",
                    pair, amount_in, x_out, pool.last_price, amount_in * fee_rate
                );

                let mut vol = self.volume_tracker.write().await;
                *vol += amount_in;

                Some(trade)
            }
        }
    }

    pub async fn add_liquidity(&self, pair: &str, amount_x: f64, amount_y: f64) -> Option<f64> {
        let mut pools = self.pools.write().await;
        let pool = pools.get_mut(pair)?;

        let lp_tokens = (amount_x * amount_y).sqrt();
        let x_ratio = amount_x / pool.reserve_x;
        let y_ratio = amount_y / pool.reserve_y;
        let share = x_ratio.min(y_ratio);

        pool.reserve_x += amount_x;
        pool.reserve_y += amount_y;
        pool.k = pool.reserve_x.powi(4) * pool.reserve_y;
        pool.total_lp_tokens += lp_tokens;

        info!(
            "BMM add liquidity: {} | +{:.4}X +{:.4}Y → {} LP tokens ({:.2}% share)",
            pair, amount_x, amount_y, lp_tokens, share * 100.0
        );

        Some(lp_tokens)
    }

    pub async fn remove_liquidity(&self, pair: &str, lp_tokens: f64) -> Option<(f64, f64)> {
        let mut pools = self.pools.write().await;
        let pool = pools.get_mut(pair)?;

        if pool.total_lp_tokens <= 0.0 || lp_tokens > pool.total_lp_tokens {
            return None;
        }

        let share = lp_tokens / pool.total_lp_tokens;
        let amount_x = pool.reserve_x * share;
        let amount_y = pool.reserve_y * share;

        pool.reserve_x -= amount_x;
        pool.reserve_y -= amount_y;
        pool.k = pool.reserve_x.powi(4) * pool.reserve_y;
        pool.total_lp_tokens -= lp_tokens;

        info!(
            "BMM remove liquidity: {} | -{:.4}X -{:.4}Y from {} LP tokens",
            pair, amount_x, amount_y, lp_tokens
        );

        Some((amount_x, amount_y))
    }

    pub async fn get_pool(&self, pair: &str) -> Option<PoolState> {
        let pools = self.pools.read().await;
        pools.get(pair).cloned()
    }

    pub async fn get_stats(&self) -> BmmPoolStats {
        let pools = self.pools.read().await;
        let vol = self.volume_tracker.read().await;

        let total_volume = *vol;
        let mut total_fees = 0.0;
        let mut total_trades = 0;
        let pool_list: Vec<PoolState> = pools.values().cloned().collect();

        for pool in pool_list.iter() {
            total_fees += pool.fee_accumulated_x + pool.fee_accumulated_y;
            total_trades += pool.trade_count;
        }

        BmmPoolStats {
            pools: pool_list,
            total_volume_usd: total_volume,
            total_fees_usd: total_fees,
            total_trades,
        }
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("BMM X⁴Y=K Engine stopped");
    }
}
