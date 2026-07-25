#![allow(dead_code)]

use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

#[path = "../src/types.rs"]
mod types;
#[path = "../src/matching.rs"]
mod matching;
#[path = "../src/orderbook.rs"]
mod orderbook;
#[path = "../src/counterparty.rs"]
mod counterparty;
#[path = "../src/wal.rs"]
mod wal;
#[path = "../src/metrics.rs"]
mod metrics;

use types::*;
use orderbook::OrderBookManager;
use wal::{WriteAheadLog, WALRecord};
use metrics::MetricsCollector;

const PAIR: &str = "BTC/USDT";

// ==================== Helpers ====================

fn make_user() -> Uuid {
    Uuid::new_v4()
}

fn make_limit_order(user: Uuid, side: OrderSide, price: f64, qty: f64) -> Order {
    Order::new_limit(user, PAIR.to_string(), side, price, qty)
}

fn make_market_order(user: Uuid, side: OrderSide, qty: f64) -> Order {
    Order::new_market(user, PAIR.to_string(), side, qty)
}

// ==================== Percentile helper ====================

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((p / 100.0) * sorted.len() as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn print_latencies(name: &str, latencies: &mut Vec<u64>) {
    latencies.sort_unstable();
    let min = latencies[0];
    let max = latencies[latencies.len() - 1];
    let p50 = percentile(latencies, 50.0);
    let p99 = percentile(latencies, 99.0);
    let p999 = percentile(latencies, 99.9);
    let avg: u64 = latencies.iter().sum::<u64>() / latencies.len() as u64;

    println!(
        "  {:<40} min={:>6}µs  avg={:>6}µs  P50={:>6}µs  P99={:>6}µs  P999={:>7}µs  max={:>6}µs",
        name, min, avg, p50, p99, p999, max
    );
}

// ==================== 1. Order Placement Latency ====================

#[test]
fn bench_order_placement_latency() {
    let sep = "=".repeat(70);
    println!("\n{}", sep);
    println!("  BENCHMARK 1: Order Placement Latency (100,000 limit orders)");
    println!("{}", sep);

    let books = OrderBookManager::new();
    books.create_book(PAIR);

    let n = 100_000;
    let mut latencies = Vec::with_capacity(n);
    let user = make_user();

    // Warmup
    for i in 0..1000 {
        let o = make_limit_order(user, OrderSide::Buy, 100.0 + (i as f64 * 0.01), 1.0);
        let _ = books.place_order(o);
    }

    let start = Instant::now();
    for i in 0..n {
        let side = if i % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell };
        // Spread orders so most don't match (rest on book) — pure insertion
        let price = 100.0 + (i as f64 * 0.001);
        let o = make_limit_order(user, side, price, 1.0);
        let t0 = Instant::now();
        let _ = books.place_order(o);
        latencies.push(t0.elapsed().as_micros() as u64);
    }
    let elapsed = start.elapsed();

    print_latencies("place_order (all)", &mut latencies);
    let tps = n as f64 / elapsed.as_secs_f64();
    println!("  {:<40} {:.0} orders/sec", "Throughput:", tps);
    println!("  {:<40} {:.2?}", "Total time:", elapsed);
    println!();
}

// ==================== 2. Matching Latency ====================

#[test]
fn bench_matching_latency() {
    let sep = "=".repeat(70);
    println!("\n{}", sep);
    println!("  BENCHMARK 2: Matching Latency (10,000 market buys vs 1000 asks)");
    println!("{}", sep);

    let books = OrderBookManager::new();
    books.create_book(PAIR);

    // Pre-populate 1000 asks at prices 100.00 .. 100.99 (step 0.01)
    let ask_user = make_user();
    for i in 0..1000 {
        let price = 100.0 + (i as f64 * 0.01);
        let o = make_limit_order(ask_user, OrderSide::Sell, price, 10.0);
        let _ = books.place_order(o);
    }

    // Place 10,000 market buy orders — each buys 1 unit, will match best ask
    let n = 10_000;
    let mut latencies = Vec::with_capacity(n);
    let buy_user = make_user();

    // Warmup
    for _ in 0..100 {
        let o = make_market_order(buy_user, OrderSide::Buy, 0.1);
        let _ = books.place_order(o);
    }

    let start = Instant::now();
    for _ in 0..n {
        let o = make_market_order(buy_user, OrderSide::Buy, 1.0);
        let t0 = Instant::now();
        let _ = books.place_order(o);
        latencies.push(t0.elapsed().as_micros() as u64);
    }
    let elapsed = start.elapsed();

    print_latencies("market_buy match", &mut latencies);
    let tps = n as f64 / elapsed.as_secs_f64();
    println!("  {:<40} {:.0} orders/sec", "Throughput:", tps);
    println!();
}

// ==================== 3. WAL Append Throughput ====================

#[test]
fn bench_wal_throughput() {
    let sep = "=".repeat(70);
    println!("\n{}", sep);
    println!("  BENCHMARK 3: WAL Append Throughput (100,000 records)");
    println!("{}", sep);

    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let wal = WriteAheadLog::new("bench-wal", tmp_dir.path(), vec![])
        .expect("create WAL");

    let n = 100_000;
    let mut latencies = Vec::with_capacity(n);
    let user = make_user();

    let order = make_limit_order(user, OrderSide::Buy, 50000.0, 1.5);

    // Warmup
    for _ in 0..100 {
        let _ = wal.append(WALRecord::PlaceOrder(order.clone()));
    }

    let start = Instant::now();
    for _ in 0..n {
        let t0 = Instant::now();
        let _ = wal.append(WALRecord::PlaceOrder(order.clone()));
        latencies.push(t0.elapsed().as_micros() as u64);
    }
    let elapsed = start.elapsed();

    print_latencies("wal.append()", &mut latencies);
    let tps = n as f64 / elapsed.as_secs_f64();
    println!("  {:<40} {:.0} records/sec", "Throughput:", tps);
    println!();
}

// ==================== 4. Full Pipeline Throughput ====================

#[test]
fn bench_full_pipeline() {
    let sep = "=".repeat(70);
    println!("\n{}", sep);
    println!("  BENCHMARK 4: Full Pipeline (order -> match -> WAL -> metrics)");
    println!("{}", sep);

    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let books = Arc::new(OrderBookManager::new());
    books.create_book(PAIR);
    let wal = Arc::new(WriteAheadLog::new("bench-pipeline", tmp_dir.path(), vec![]).expect("create WAL"));
    let metrics = Arc::new(MetricsCollector::new());

    let n = 50_000;
    let mut latencies = Vec::with_capacity(n);
    let user = make_user();

    // Pre-populate asks so market orders can match
    let ask_user = make_user();
    for i in 0..5000 {
        let o = make_limit_order(ask_user, OrderSide::Sell, 100.0 + (i as f64 * 0.01), 100.0);
        let _ = books.place_order(o);
    }

    // Warmup
    for _ in 0..500 {
        let o = make_market_order(user, OrderSide::Buy, 0.01);
        let _ = books.place_order(o);
    }

    let start = Instant::now();
    for _ in 0..n {
        let o = make_market_order(user, OrderSide::Buy, 0.01);
        let t0 = Instant::now();

        // 1. WAL append
        let _ = wal.append(WALRecord::PlaceOrder(o.clone()));
        // 2. Order placement + matching
        let result = books.place_order(o).expect("place_order");
        // 3. Metrics
        metrics.inc_orders();
        metrics.inc_trades_by(result.trades.len() as u64);
        for t in &result.trades {
            metrics.add_volume(t.total);
        }

        latencies.push(t0.elapsed().as_micros() as u64);
    }
    let elapsed = start.elapsed();

    print_latencies("full pipeline", &mut latencies);
    let tps = n as f64 / elapsed.as_secs_f64();
    let snap = metrics.snapshot();
    println!("  {:<40} {:.0} orders/sec", "TPS:", tps);
    println!("  {:<40} {}", "Total orders:", snap["tps_current"]);
    println!("  {:<40} {}", "Total trades:", snap["trades"]);
    println!();
}

// ==================== 5. Memory Usage Estimation ====================

#[test]
fn bench_memory_usage() {
    let sep = "=".repeat(70);
    println!("\n{}", sep);
    println!("  BENCHMARK 5: Memory Usage (1,000,000 orders)");
    println!("{}", sep);

    let books = OrderBookManager::new();
    books.create_book(PAIR);

    let n = 1_000_000;
    let user = make_user();

    // Estimate Order struct size
    let order_size = std::mem::size_of::<Order>();
    println!("  {:<40} {} bytes", "Order struct size:", order_size);

    // Place orders — spread across price range to avoid matching
    let start = Instant::now();
    for i in 0..n {
        let side = if i % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell };
        let price = 100.0 + (i as f64 * 0.0001);
        let o = make_limit_order(user, side, price, 1.0);
        let _ = books.place_order(o);
    }
    let elapsed = start.elapsed();

    let total_orders = books.total_orders();
    let total_trades = books.total_trades();
    let approx_memory = order_size as u64 * n;

    println!("  {:<40} {}", "Orders placed:", total_orders);
    println!("  {:<40} {}", "Trades generated:", total_trades);
    println!("  {:<40} {:.2} MB", "Approx memory (Order structs):", approx_memory as f64 / 1_048_576.0);
    println!("  {:<40} {:.2?}", "Placement time:", elapsed);
    println!("  {:<40} {:.0} orders/sec", "Throughput:", n as f64 / elapsed.as_secs_f64());
    println!();
}
