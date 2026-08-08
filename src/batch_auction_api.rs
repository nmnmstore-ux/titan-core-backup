use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::batch_auction::{
    BatchAuctionEngine, BatchAuctionConfig,
};

pub use crate::batch_auction::BatchOrder;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchAuctionStatus {
    pub pending_buy: usize,
    pub pending_sell: usize,
    pub total_batches: u64,
    pub last_clearing_price: Option<u64>,
    pub avg_clearing_price: u64,
    pub total_volume_all_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentAuction {
    pub pending_buy: usize,
    pub pending_sell: usize,
    pub last_clearing_price: Option<u64>,
    pub total_batches: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuctionResult {
    pub batch_id: String,
    pub batch_number: u64,
    pub timestamp_ns: u64,
    pub clearing_price: u64,
    pub total_volume: u64,
    pub matched_trades: usize,
    pub unmatched_buy: usize,
    pub unmatched_sell: usize,
}

pub struct BatchAuctionAPI {
    engine: Arc<BatchAuctionEngine>,
}

impl BatchAuctionAPI {
    pub fn new(config: BatchAuctionConfig) -> Self {
        Self {
            engine: Arc::new(BatchAuctionEngine::new(config)),
        }
    }

    pub async fn start_auction(&self, _pair: &str) -> Result<String, String> {
        let result = self.engine.run_batch().await;
        match result {
            Some(r) => Ok(r.batch_id),
            None => Err("no orders available for clearing".into()),
        }
    }

    pub async fn submit_order(&self, order: BatchOrder) -> Result<(), String> {
        self.engine.submit_order(order).await
    }

    pub async fn get_current_auction(&self) -> Option<CurrentAuction> {
        let stats = self.engine.get_stats().await;
        Some(CurrentAuction {
            pending_buy: stats.pending_buy,
            pending_sell: stats.pending_sell,
            last_clearing_price: stats.last_clearing_price,
            total_batches: stats.total_batches,
        })
    }

    pub async fn get_history(&self) -> Vec<AuctionResult> {
        let batches = self.engine.get_recent_batches(100).await;
        batches
            .into_iter()
            .map(|b| AuctionResult {
                batch_id: b.batch_id,
                batch_number: b.batch_number,
                timestamp_ns: b.timestamp_ns,
                clearing_price: b.clearing_price,
                total_volume: b.total_volume,
                matched_trades: b.matched_trades.len(),
                unmatched_buy: b.unmatched_buy.len(),
                unmatched_sell: b.unmatched_sell.len(),
            })
            .collect()
    }
}
