use crate::types::{Order, OrderSide, OrderType, Track, OrderStatus, OrderStyle};
use crate::orderbook::OrderBookManager;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOrder {
    pub order_id: String,
    pub user_id: String,
    pub pair: String,
    pub side: OrderSide,
    pub price: u64,
    pub quantity: u64,
    pub track: Track,
    pub timestamp_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchAuctionResult {
    pub batch_id: String,
    pub batch_number: u64,
    pub timestamp_ns: u64,
    pub clearing_price: u64,
    pub total_volume: u64,
    pub matched_trades: Vec<BatchTrade>,
    pub unmatched_buy: Vec<BatchOrder>,
    pub unmatched_sell: Vec<BatchOrder>,
    pub merkle_root: Vec<u8>,
    pub zk_proof: Option<ZKProof>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTrade {
    pub buy_order_id: String,
    pub sell_order_id: String,
    pub buyer_user_id: String,
    pub seller_user_id: String,
    pub price: u64,
    pub quantity: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZKProof {
    pub pi_a: Vec<String>,
    pub pi_b: Vec<Vec<String>>,
    pub pi_c: Vec<String>,
    pub protocol: String,
    pub curve: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchAuctionConfig {
    pub batch_interval_ms: u64,
    pub max_orders_per_batch: usize,
    pub min_orders_for_clearing: usize,
    pub price_precision: u64,
}

impl Default for BatchAuctionConfig {
    fn default() -> Self {
        Self {
            batch_interval_ms: 100,
            max_orders_per_batch: 10_000,
            min_orders_for_clearing: 2,
            price_precision: 10_000,
        }
    }
}

pub struct BatchAuctionEngine {
    config: BatchAuctionConfig,
    pending_buy: Arc<RwLock<Vec<BatchOrder>>>,
    pending_sell: Arc<RwLock<Vec<BatchOrder>>>,
    batch_counter: Arc<RwLock<u64>>,
    history: Arc<RwLock<Vec<BatchAuctionResult>>>,
    last_clearing_price: Arc<RwLock<Option<u64>>>,
}

impl BatchAuctionEngine {
    pub fn new(config: BatchAuctionConfig) -> Self {
        Self {
            config,
            pending_buy: Arc::new(RwLock::new(Vec::new())),
            pending_sell: Arc::new(RwLock::new(Vec::new())),
            batch_counter: Arc::new(RwLock::new(0)),
            history: Arc::new(RwLock::new(Vec::new())),
            last_clearing_price: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn submit_order(&self, order: BatchOrder) -> Result<(), String> {
        if order.side == OrderSide::Buy {
            let mut buys = self.pending_buy.write().await;
            if buys.len() >= self.config.max_orders_per_batch {
                return Err("Batch full".into());
            }
            buys.push(order);
        } else {
            let mut sells = self.pending_sell.write().await;
            if sells.len() >= self.config.max_orders_per_batch {
                return Err("Batch full".into());
            }
            sells.push(order);
        }
        Ok(())
    }

    pub async fn run_batch(&self) -> Option<BatchAuctionResult> {
        let mut buys = self.pending_buy.write().await;
        let mut sells = self.pending_sell.write().await;

        if buys.len() + sells.len() < self.config.min_orders_for_clearing {
            return None;
        }

        let mut counter = self.batch_counter.write().await;
        *counter += 1;
        let batch_number = *counter;

        buys.sort_by(|a, b| b.price.cmp(&a.price));
        sells.sort_by(|a, b| a.price.cmp(&b.price));

        let (matched_trades, clearing_price, total_volume) =
            self.match_orders(&mut buys, &mut sells);

        let merkle_root = self.compute_merkle_root(&matched_trades);
        let batch_id = format!("fba_{}", batch_number);
        let timestamp_ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64;

        let result = BatchAuctionResult {
            batch_id,
            batch_number,
            timestamp_ns,
            clearing_price,
            total_volume,
            matched_trades,
            unmatched_buy: buys.clone(),
            unmatched_sell: sells.clone(),
            merkle_root,
            zk_proof: None,
        };

        let mut history = self.history.write().await;
        history.push(result.clone());
        if history.len() > 1000 {
            history.remove(0);
        }

        let mut last_price = self.last_clearing_price.write().await;
        *last_price = Some(clearing_price);

        buys.clear();
        sells.clear();

        Some(result)
    }

    fn match_orders(
        &self,
        buys: &mut Vec<BatchOrder>,
        sells: &mut Vec<BatchOrder>,
    ) -> (Vec<BatchTrade>, u64, u64) {
        let mut trades = Vec::new();
        let mut buy_idx = 0;
        let mut sell_idx = 0;
        let mut total_volume = 0u64;
        let mut last_clearing_price = 0u64;

        while buy_idx < buys.len() && sell_idx < sells.len() {
            let buy = &buys[buy_idx];
            let sell = &sells[sell_idx];

            if buy.price >= sell.price {
                let clearing_price = (buy.price + sell.price) / 2;
                let qty = buy.quantity.min(sell.quantity);

                trades.push(BatchTrade {
                    buy_order_id: buy.order_id.clone(),
                    sell_order_id: sell.order_id.clone(),
                    buyer_user_id: buy.user_id.clone(),
                    seller_user_id: sell.user_id.clone(),
                    price: clearing_price,
                    quantity: qty,
                });

                total_volume += qty;
                last_clearing_price = clearing_price;

                if buy.quantity > sell.quantity {
                    buys[buy_idx].quantity -= qty;
                    sell_idx += 1;
                } else if sell.quantity > buy.quantity {
                    sells[sell_idx].quantity -= qty;
                    buy_idx += 1;
                } else {
                    buy_idx += 1;
                    sell_idx += 1;
                }
            } else {
                break;
            }
        }

        (trades, last_clearing_price, total_volume)
    }

    fn compute_merkle_root(&self, trades: &[BatchTrade]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        for trade in trades {
            let data = format!(
                "{}{}{}{}{}{}",
                trade.buy_order_id,
                trade.sell_order_id,
                trade.buyer_user_id,
                trade.seller_user_id,
                trade.price,
                trade.quantity
            );
            hasher.update(data.as_bytes());
        }
        hasher.finalize().to_vec()
    }

    pub async fn get_stats(&self) -> BatchAuctionStats {
        let buys = self.pending_buy.read().await;
        let sells = self.pending_sell.read().await;
        let history = self.history.read().await;
        let last_price = self.last_clearing_price.read().await;

        BatchAuctionStats {
            pending_buy: buys.len(),
            pending_sell: sells.len(),
            total_batches: *self.batch_counter.read().await,
            last_clearing_price: *last_price,
            avg_clearing_price: if history.is_empty() {
                0
            } else {
                history.iter().map(|b| b.clearing_price).sum::<u64>() / history.len() as u64
            },
            total_volume_all_time: history.iter().map(|b| b.total_volume).sum(),
        }
    }

    pub async fn get_recent_batches(&self, count: usize) -> Vec<BatchAuctionResult> {
        let history = self.history.read().await;
        history.iter().rev().take(count).cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchAuctionStats {
    pub pending_buy: usize,
    pub pending_sell: usize,
    pub total_batches: u64,
    pub last_clearing_price: Option<u64>,
    pub avg_clearing_price: u64,
    pub total_volume_all_time: u64,
}

pub struct FBAMatchingEngine {
    batch_engine: Arc<BatchAuctionEngine>,
    orderbook: Arc<OrderBookManager>,
    config: BatchAuctionConfig,
}

impl FBAMatchingEngine {
    pub fn new(orderbook: Arc<OrderBookManager>, config: BatchAuctionConfig) -> Self {
        Self {
            batch_engine: Arc::new(BatchAuctionEngine::new(config.clone())),
            orderbook,
            config,
        }
    }

    pub async fn submit_order_fba(&self, order: Order) -> Result<(), String> {
        let batch_order = BatchOrder {
            order_id: order.id.to_string(),
            user_id: order.user_id.to_string(),
            pair: order.pair.to_string(),
            side: order.side,
            price: (order.price * self.config.price_precision as f64) as u64,
            quantity: (order.quantity * self.config.price_precision as f64) as u64,
            track: order.track,
            timestamp_ns: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64,
        };
        self.batch_engine.submit_order(batch_order).await
    }

    pub async fn run_batch_auction(&self) -> Option<BatchAuctionResult> {
        let result = self.batch_engine.run_batch().await;
        if let Some(ref r) = result {
            self.settle_trades(r).await;
        }
        result
    }

    async fn settle_trades(&self, result: &BatchAuctionResult) {
        for trade in &result.matched_trades {
            let price = trade.price as f64 / self.config.price_precision as f64;
            let qty = trade.quantity as f64 / self.config.price_precision as f64;

            let buy_order = Order {
            client_order_id: None,
            filled_quantity: 0,
                id: uuid::Uuid::parse_str(&trade.buy_order_id).unwrap_or_default(),
                id_tag: 0,
                user_id: uuid::Uuid::parse_str(&trade.buyer_user_id).unwrap_or_default(),
                pair: result.matched_trades[0].buy_order_id.parse().unwrap_or_default(),
                order_type: OrderType::Limit,
                side: OrderSide::Buy,
                price,
                quantity: qty,
                filled: 0.0,
                remaining: qty,
                status: OrderStatus::New,
                timestamp: result.timestamp_ns as i64,
                ttl_ms: None,
                is_swap: false,
                swap_target_currency: None,
                tee_signed: false,
                dot_verified: false,
                stealth: true,
                trailing_offset: None,
                trigger_price: None,
                hard_floor: None,
                track: Track::Autonomous,
                style: OrderStyle::Standard,
                hidden_remaining: 0.0,
            };

            let _ = self.orderbook.place_order(buy_order);
        }
    }

    pub async fn get_fba_stats(&self) -> BatchAuctionStats {
        self.batch_engine.get_stats().await
    }

    pub fn batch_engine(&self) -> Arc<BatchAuctionEngine> {
        self.batch_engine.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OrderSide, Track};

    fn make_batch_order(id: &str, side: OrderSide, price: u64, qty: u64) -> BatchOrder {
        BatchOrder {
            order_id: id.into(),
            user_id: format!("user_{}", id),
            pair: "USD/EGP".into(),
            side,
            price,
            quantity: qty,
            track: Track::Autonomous,
            timestamp_ns: 0,
        }
    }

    #[tokio::test]
    async fn test_fba_simple_cross() {
        let engine = BatchAuctionEngine::new(BatchAuctionConfig::default());
        engine.submit_order(make_batch_order("b1", OrderSide::Buy, 310000, 1000)).await.unwrap();
        engine.submit_order(make_batch_order("s1", OrderSide::Sell, 300000, 1000)).await.unwrap();

        let result = engine.run_batch().await.unwrap();
        assert_eq!(result.matched_trades.len(), 1);
        assert_eq!(result.clearing_price, 305000);
        assert_eq!(result.total_volume, 1000);
    }

    #[tokio::test]
    async fn test_fba_multiple_orders() {
        let engine = BatchAuctionEngine::new(BatchAuctionConfig::default());
        engine.submit_order(make_batch_order("b1", OrderSide::Buy, 320000, 500)).await.unwrap();
        engine.submit_order(make_batch_order("b2", OrderSide::Buy, 310000, 1000)).await.unwrap();
        engine.submit_order(make_batch_order("b3", OrderSide::Buy, 305000, 800)).await.unwrap();
        engine.submit_order(make_batch_order("s1", OrderSide::Sell, 300000, 1000)).await.unwrap();
        engine.submit_order(make_batch_order("s2", OrderSide::Sell, 305000, 600)).await.unwrap();
        engine.submit_order(make_batch_order("s3", OrderSide::Sell, 310000, 700)).await.unwrap();

        let result = engine.run_batch().await.unwrap();
        assert!(!result.matched_trades.is_empty());
        assert!(result.total_volume > 0);
    }

    #[tokio::test]
    async fn test_fba_no_match() {
        let engine = BatchAuctionEngine::new(BatchAuctionConfig::default());
        engine.submit_order(make_batch_order("b1", OrderSide::Buy, 290000, 1000)).await.unwrap();
        engine.submit_order(make_batch_order("s1", OrderSide::Sell, 310000, 1000)).await.unwrap();

        let result = engine.run_batch().await.unwrap();
        assert!(result.matched_trades.is_empty());
        assert_eq!(result.unmatched_buy.len(), 1);
        assert_eq!(result.unmatched_sell.len(), 1);
    }

    #[tokio::test]
    async fn test_fba_price_priority() {
        let engine = BatchAuctionEngine::new(BatchAuctionConfig::default());
        engine.submit_order(make_batch_order("b1", OrderSide::Buy, 320000, 1000)).await.unwrap();
        engine.submit_order(make_batch_order("b2", OrderSide::Buy, 300000, 1000)).await.unwrap();
        engine.submit_order(make_batch_order("s1", OrderSide::Sell, 305000, 1000)).await.unwrap();

        let result = engine.run_batch().await.unwrap();
        assert_eq!(result.matched_trades.len(), 1);
        assert_eq!(result.matched_trades[0].buy_order_id, "b1");
    }
}