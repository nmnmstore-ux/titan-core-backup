use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSnapshot {
    pub timestamp: i64,
    pub version: String,
    pub order_books: Vec<BookSnapshot>,
    pub dot_pending: Vec<DOTSnapshot>,
    pub mempool_txs: Vec<String>,
    pub metrics: serde_json::Value,
    pub tee_attestation: String,
    pub numa_topology: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSnapshot {
    pub pair: String,
    pub bids: Vec<OrderSnapshot>,
    pub asks: Vec<OrderSnapshot>,
    pub last_price: f64,
    pub volume_24h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSnapshot {
    pub id: String,
    pub user_id: String,
    pub side: String,
    pub price: f64,
    pub quantity: f64,
    pub remaining: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DOTSnapshot {
    pub id: String,
    pub from: String,
    pub to: String,
    pub amount: f64,
    pub status: String,
}
