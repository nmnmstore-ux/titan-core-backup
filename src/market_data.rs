use crate::orderbook::OrderBookManager;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthSnapshot {
    pub pair: String,
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickerSnapshot {
    pub pair: String,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub volume_24h: f64,
    pub change_24h_pct: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub pair: String,
    pub interval: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MarketEvent {
    #[serde(rename = "depth")]
    Depth(DepthSnapshot),
    #[serde(rename = "ticker")]
    Ticker(TickerSnapshot),
    #[serde(rename = "candle")]
    Candle(Candle),
    #[serde(rename = "trade")]
    Trade(Trade),
}

pub struct MarketDataStream {
    pub tx: broadcast::Sender<MarketEvent>,
}

impl MarketDataStream {
    pub fn new(_books: Arc<OrderBookManager>) -> (Self, Arc<Self>) {
        let (tx, _) = broadcast::channel(4096);
        let stream = Arc::new(Self { tx: tx.clone() });
        (Self { tx: tx.clone() }, stream)
    }

    pub fn start(self: Arc<Self>, books: Arc<OrderBookManager>) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut depth_interval = tokio::time::interval(std::time::Duration::from_millis(200));
            let mut candle_interval = tokio::time::interval(std::time::Duration::from_secs(1));
            let mut candle_state: BTreeMap<(String, String), Candle> = BTreeMap::new();

            loop {
                tokio::select! {
                    _ = depth_interval.tick() => {
                        let pairs: Vec<String> = books.books.iter().map(|e| e.key().clone()).collect();
                        for pair in pairs {
                            if let Some(summary) = books.get_book_summary(&pair) {
                                let depth = books.get_depth(&pair, 10);
                                let bids: Vec<(f64, f64)> = depth.as_ref().map_or(Vec::new(), |d|
                                    d.bids.iter().map(|l| (l.price, l.quantity)).collect()
                                );
                                let asks: Vec<(f64, f64)> = depth.as_ref().map_or(Vec::new(), |d|
                                    d.asks.iter().map(|l| (l.price, l.quantity)).collect()
                                );
                                let _ = tx.send(MarketEvent::Depth(DepthSnapshot {
                                    pair: pair.clone(),
                                    bids,
                                    asks,
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                }));

                                let ticker = TickerSnapshot {
                                    pair: pair.clone(),
                                    bid: summary.best_bid,
                                    ask: summary.best_ask,
                                    last: summary.last_price,
                                    high_24h: 0.0,
                                    low_24h: 0.0,
                                    volume_24h: summary.volume_24h,
                                    change_24h_pct: 0.0,
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                };
                                let _ = tx.send(MarketEvent::Ticker(ticker));
                            }
                        }
                    }
                    _ = candle_interval.tick() => {
                        for ref entry in books.books.iter() {
                            let book = entry.value();
                            let pair_key = entry.key().clone();
                            let price = book.get_last_price();
                            if price <= 0.0 { continue; }
                            let intervals = ["1m", "5m", "15m", "1h"];
                            let now = chrono::Utc::now().timestamp_millis();
                            for interval in intervals {
                                let granularity = match interval {
                                    "1m" => 60_000,
                                    "5m" => 300_000,
                                    "15m" => 900_000,
                                    "1h" => 3_600_000,
                                    _ => 60_000,
                                };
                                let candle_key = (pair_key.clone(), interval.to_string());
                                let rounded_ts = (now / granularity) * granularity;

                                if let Some(candle) = candle_state.get_mut(&candle_key) {
                                    candle.high = candle.high.max(price);
                                    candle.low = candle.low.min(price);
                                    candle.close = price;
                                    if candle.timestamp < rounded_ts - granularity / 2 {
                                        let _ = tx.send(MarketEvent::Candle(candle.clone()));
                                        *candle = Candle {
                                            pair: pair_key.clone(),
                                            interval: interval.to_string(),
                                            open: price,
                                            high: price,
                                            low: price,
                                            close: price,
                                            volume: book.get_volume_24h(),
                                            timestamp: rounded_ts,
                                        };
                                    }
                                } else {
                                    candle_state.insert(candle_key.clone(), Candle {
                                        pair: pair_key.clone(),
                                        interval: interval.to_string(),
                                        open: price,
                                        high: price,
                                        low: price,
                                        close: price,
                                        volume: book.get_volume_24h(),
                                        timestamp: rounded_ts,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}
