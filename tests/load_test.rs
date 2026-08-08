use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[path = "../src/types.rs"]
mod types;
#[path = "../src/time_cache.rs"]
mod time_cache;
#[path = "../src/orderbook.rs"]
mod orderbook;
#[path = "../src/matching.rs"]
mod matching;
#[path = "../src/counterparty.rs"]
mod counterparty;

use types::*;
use orderbook::OrderBookManager;

const PAIR: &str = "USD/EGP";

#[test]
fn test_load_100k_orders() {
    let bm = Arc::new(OrderBookManager::new());
    bm.create_book(PAIR);

    let n = 100_000;
    let start = Instant::now();
    for i in 0..n {
        let user = Uuid::new_v4();
        let buy = Order::new_limit(user, PAIR.to_string(), OrderSide::Buy, 30.00 + (i % 1000) as f64 * 0.01, 1.0);
        let _ = bm.place_order(buy);
    }
    let elapsed = start.elapsed();
    let tps = n as f64 / elapsed.as_secs_f64();
    eprintln!("LOAD: {} orders in {:?} = {:.0} TPS", n, elapsed, tps);
    assert!(tps > 10_000.0, "TPS too low: {:.0}", tps);
}

#[test]
fn test_load_bid_ask_match() {
    let bm = Arc::new(OrderBookManager::new());
    bm.create_book(PAIR);

    for i in 0..5000 {
        let user = Uuid::new_v4();
        let price = 30.00 + (i % 100) as f64 * 0.01;
        let sell = Order::new_limit(user, PAIR.to_string(), OrderSide::Sell, price, 10.0);
        let _ = bm.place_order(sell);
    }

    let n = 10_000;
    let start = Instant::now();
    for _ in 0..n {
        let user = Uuid::new_v4();
        let buy = Order::new_limit(user, PAIR.to_string(), OrderSide::Buy, 100.0, 1.0);
        let result = bm.place_order(buy).unwrap();
        assert!(!result.trades.is_empty(), "limit buy at 100 should cross best ask");
    }
    let elapsed = start.elapsed();
    let tps = n as f64 / elapsed.as_secs_f64();
    eprintln!("LOAD MATCH: {} orders in {:?} = {:.0} TPS", n, elapsed, tps);
    assert!(tps > 5_000.0, "Match TPS too low: {:.0}", tps);
}

#[test]
fn test_load_sustained_throughput() {
    let bm = Arc::new(OrderBookManager::new());
    bm.create_book(PAIR);

    for i in 0..1000 {
        let user = Uuid::new_v4();
        let sell = Order::new_limit(user, PAIR.to_string(), OrderSide::Sell, 30.00 + i as f64 * 0.01, 100.0);
        let _ = bm.place_order(sell);
    }

    let duration = Duration::from_secs(2);
    let mut count = 0u64;
    let start = Instant::now();
    while start.elapsed() < duration {
        let user = Uuid::new_v4();
        let buy = Order::new_limit(user, PAIR.to_string(), OrderSide::Buy, 100.0, 0.1);
        let _ = bm.place_order(buy);
        count += 1;
    }
    let elapsed = start.elapsed();
    let tps = count as f64 / elapsed.as_secs_f64();
    eprintln!("LOAD SUSTAINED: {} orders in {:?} = {:.0} TPS", count, elapsed, tps);
    assert!(tps > 5_000.0, "Sustained TPS too low: {:.0}", tps);
}
