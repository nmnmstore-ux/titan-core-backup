use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

// ==================== Internal test helpers (direct engine access) ====================
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
#[path = "../src/wal.rs"]
mod wal;
#[path = "../src/metrics.rs"]
mod metrics;

use types::*;
use orderbook::OrderBookManager;
use wal::{WriteAheadLog, WALRecord};
use metrics::MetricsCollector;

const API_BASE: &str = "http://localhost:3001";
const API_KEY: &str = "tb_e10797fa_9cc5f530c082ce9f2cafc7468f0970df09cb200c27270a3c734ae8d23a9f991f";
const USER_ID: &str = "14f0c128-991b-43f0-ace8-fdf50cb10183";
const USER: Uuid = Uuid::nil(); // replaced at runtime in each benchmark
const PAIRS: &[&str] = &["USD/EUR", "USD/EGP", "USD/SAR", "USD/AED", "USD/GBP", "EUR/EGP"];

fn make_order_body(pair: &str, side: &str, price: f64, qty: f64) -> serde_json::Value {
    serde_json::json!({
        "id": Uuid::nil().to_string(),
        "user_id": USER_ID,
        "pair": pair,
        "order_type": "Limit",
        "side": side,
        "price": price,
        "quantity": qty,
        "filled": 0.0,
        "remaining": qty,
        "status": "New",
        "timestamp": 0,
        "ttl_ms": null,
        "is_swap": false,
        "swap_target_currency": null,
        "tee_signed": false,
        "dot_verified": false,
        "stealth": false,
        "trailing_offset": null,
        "trigger_price": null,
        "hard_floor": null,
        "track": "Compliant",
        "style": "Standard",
        "hidden_remaining": 0.0,
        "filled_quantity": 0,
        "client_order_id": null
    })
}

fn post_order(body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("{}/api/v1/order", API_BASE))
        .header("Authorization", format!("Bearer {}", API_KEY))
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .map_err(|e| format!("request error: {}", e))?;
    let status = resp.status();
    let text = resp.text().map_err(|e| format!("read error: {}", e))?;
    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status, text));
    }
    serde_json::from_str(&text).map_err(|e| format!("json error: {}", e))
}

fn post_order_latency(body: &serde_json::Value) -> Result<(serde_json::Value, Duration), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let t0 = Instant::now();
    let resp = client
        .post(format!("{}/api/v1/order", API_BASE))
        .header("Authorization", format!("Bearer {}", API_KEY))
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .map_err(|e| format!("request error: {}", e))?;
    let elapsed = t0.elapsed();
    let status = resp.status();
    let text = resp.text().map_err(|e| format!("read error: {}", e))?;
    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status, text));
    }
    let val: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("json error: {}", e))?;
    Ok((val, elapsed))
}

fn make_limit_order(user: Uuid, side: OrderSide, price: f64, qty: f64) -> Order {
    Order::new_limit(user, PAIRS[0].to_string(), side, price, qty)
}

fn make_market_order(user: Uuid, side: OrderSide, qty: f64) -> Order {
    Order::new_market(user, PAIRS[0].to_string(), side, qty)
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() { return 0; }
    let idx = ((p / 100.0) * sorted.len() as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn print_latencies(name: &str, latencies: &[u64]) {
    if latencies.is_empty() { return; }
    let mut sorted = latencies.to_vec();
    sorted.sort_unstable();
    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let p50 = percentile(&sorted, 50.0);
    let p90 = percentile(&sorted, 90.0);
    let p99 = percentile(&sorted, 99.0);
    let p999 = percentile(&sorted, 99.9);
    let avg: u64 = sorted.iter().sum::<u64>() / sorted.len() as u64;
    println!(
        "  {:<35} min={:>8}µs  avg={:>8}µs  P50={:>8}µs  P90={:>8}µs  P99={:>8}µs  P999={:>9}µs  max={:>8}µs",
        name, min, avg, p50, p90, p99, p999, max
    );
}

fn print_header(title: &str) {
    println!("\n{}", "=".repeat(90));
    println!("  {}", title);
    println!("{}", "=".repeat(90));
}

// ========================================================================
// BENCHMARK 1: Sequential Order Placement Throughput (HTTP)
// ========================================================================
#[test]
fn benchmark_sequential_throughput() {
    print_header("BENCHMARK 1: Sequential Order Placement (10,000 limit orders via HTTP)");
    let pair = PAIRS[0];
    let n = 10_000;
    let mut latencies = Vec::with_capacity(n);

    // Warmup
    let warmup = make_order_body(pair, "buy", 1.0, 0.01);
    for _ in 0..20 {
        let _ = post_order(&warmup);
    }

    let start = Instant::now();
    for i in 0..n {
        let price = 0.5 + (i as f64 * 0.0001);
        let body = make_order_body(pair, "buy", price, 0.01);
        match post_order_latency(&body) {
            Ok((_, dur)) => latencies.push(dur.as_micros() as u64),
            Err(e) => eprintln!("  [WARN] order {} failed: {}", i, e),
        }
    }
    let elapsed = start.elapsed();
    let success = latencies.len();

    print_latencies("place_order (HTTP)", &latencies);
    println!("  {:<35} {}/{} orders successful", "Result:", success, n);
    println!("  {:<35} {:.2?}", "Total time:", elapsed);
    if elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} orders/sec", "Throughput:", success as f64 / elapsed.as_secs_f64());
    }
}

// ========================================================================
// BENCHMARK 2: Concurrent Order Placement Throughput (HTTP)
// ========================================================================
#[test]
fn benchmark_concurrent_throughput() {
    print_header("BENCHMARK 2: Concurrent Order Placement (10,000 orders, 100 concurrent)");
    let pair = PAIRS[0];
    let n = 10_000;
    let concurrency = 100;

    let counter = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let handles: Vec<_> = (0..concurrency)
        .map(|thread_id| {
            let c = Arc::clone(&counter);
            let e = Arc::clone(&errors);
            std::thread::spawn(move || {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build().unwrap();
                let batch_size = n / concurrency;
                for i in 0..batch_size {
                    let price = 0.5 + ((thread_id * batch_size + i) as f64 * 0.0001);
                    let body = serde_json::json!({
                        "id": Uuid::nil().to_string(),
                        "user_id": USER_ID,
                        "pair": pair,
                        "order_type": "Limit",
                        "side": "buy",
                        "price": price,
                        "quantity": 0.01,
                        "filled": 0.0,
                        "remaining": 0.01,
                        "status": "New",
                        "timestamp": 0,
                        "ttl_ms": null,
                        "is_swap": false,
                        "swap_target_currency": null,
                        "tee_signed": false,
                        "dot_verified": false,
                        "stealth": false,
                        "trailing_offset": null,
                        "trigger_price": null,
                        "hard_floor": null,
                        "track": "Compliant",
                        "style": "Standard",
                        "hidden_remaining": 0.0,
                        "filled_quantity": 0,
                        "client_order_id": null
                    });
                    let resp = client
                        .post(format!("{}/api/v1/order", API_BASE))
                        .header("Authorization", format!("Bearer {}", API_KEY))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send();
                    match resp {
                        Ok(r) if r.status().is_success() => { c.fetch_add(1, Ordering::Relaxed); }
                        _ => { e.fetch_add(1, Ordering::Relaxed); }
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
    let elapsed = start.elapsed();
    let success = counter.load(Ordering::Relaxed);
    let errs = errors.load(Ordering::Relaxed);

    println!("  {:<35} {}", "Orders placed:", success);
    println!("  {:<35} {}", "Errors:", errs);
    println!("  {:<35} {:.2?}", "Total time:", elapsed);
    if elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} orders/sec", "Throughput:", success as f64 / elapsed.as_secs_f64());
    }
}

// ========================================================================
// BENCHMARK 3: Multi-Pair Throughput
// ========================================================================
#[test]
fn benchmark_multi_pair_throughput() {
    print_header("BENCHMARK 3: Multi-Pair Throughput (orders across 5 pairs)");
    let pairs = &PAIRS[..5];
    let orders_per_pair = 2_000;
    let n = orders_per_pair * pairs.len();

    let start = Instant::now();
    let mut success = 0u64;
    for &pair in pairs {
        for i in 0..orders_per_pair {
            let side = if i % 2 == 0 { "buy" } else { "sell" };
            let price = if side == "buy" { 0.5 + (i as f64 * 0.001) } else { 1.5 + (i as f64 * 0.001) };
            let body = make_order_body(pair, side, price, 0.01);
            match post_order(&body) {
                Ok(_) => success += 1,
                Err(e) => {
                    if i % 500 == 0 {
                        eprintln!("  [WARN] order {} on {} failed: {}", i, pair, e);
                    }
                }
            }
        }
    }
    let elapsed = start.elapsed();

    println!("  {:<35} {}/{} orders placed", "Result:", success, n);
    println!("  {:<35} {:.2?}", "Total time:", elapsed);
    if elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} orders/sec", "Throughput:", success as f64 / elapsed.as_secs_f64());
    }
}

// ========================================================================
// BENCHMARK 4: WAL Append Throughput (Internal)
// ========================================================================
#[test]
fn benchmark_wal_throughput() {
    print_header("BENCHMARK 4: WAL Append Throughput (50,000 records, internal)");
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let wal = WriteAheadLog::new("bench-wal", tmp_dir.path(), vec![])
        .expect("create WAL");

    let n = 50_000;
    let mut latencies = Vec::with_capacity(n);
    let user = Uuid::new_v4();
    let order = Order::new_limit(user, "BTC/USDT".to_string(), OrderSide::Buy, 50000.0, 1.5);

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

    print_latencies("wal.append()", &latencies);
    println!("  {:<35} {:.2?}", "Total time:", elapsed);
    if elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} records/sec", "Throughput:", n as f64 / elapsed.as_secs_f64());
    }
}

// ========================================================================
// BENCHMARK 5: Full Pipeline Latency (HTTP POST to response)
// ========================================================================
#[test]
fn benchmark_full_pipeline_latency() {
    print_header("BENCHMARK 5: Full Pipeline Latency (10,000 HTTP POST -> response)");
    let pair = PAIRS[0];
    let n = 10_000;
    let mut latencies = Vec::with_capacity(n);

    let start = Instant::now();
    for i in 0..n {
        let price = 0.5 + (i as f64 * 0.0001);
        let body = make_order_body(pair, "buy", price, 0.01);
        match post_order_latency(&body) {
            Ok((_, dur)) => latencies.push(dur.as_micros() as u64),
            Err(e) => eprintln!("  [WARN] order {} failed: {}", i, e),
        }
    }
    let elapsed = start.elapsed();
    let success = latencies.len();

    print_latencies("full pipeline (HTTP)", &latencies);
    println!("  {:<35} {}/{} successful", "Result:", success, n);
    println!("  {:<35} {:.2?}", "Total time:", elapsed);
    if elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} req/sec", "Throughput:", success as f64 / elapsed.as_secs_f64());
    }
}

// ========================================================================
// BENCHMARK 6: Matching Throughput (Internal, pre-loaded book)
// ========================================================================
#[test]
fn benchmark_matching_throughput() {
    print_header("BENCHMARK 6: Matching Throughput (100K pre-loaded, 10K market orders)");
    let books = OrderBookManager::new();
    books.create_book(PAIRS[0]);

    // Pre-load 50,000 asks at price 1.0.. (step 0.001)
    let ask_user = Uuid::new_v4();
    for i in 0..50_000 {
        let price = 1.0 + (i as f64 * 0.001);
        let o = Order::new_limit(ask_user, PAIRS[0].to_string(), OrderSide::Sell, price, 10.0);
        let _ = books.place_order(o);
    }
    // Pre-load 50,000 bids at price 0.99.. (step 0.001 downwards)
    let bid_user = Uuid::new_v4();
    for i in 0..50_000 {
        let price = 0.99 - (i as f64 * 0.001);
        let o = Order::new_limit(bid_user, PAIRS[0].to_string(), OrderSide::Buy, price, 10.0);
        let _ = books.place_order(o);
    }

    let n = 10_000;
    let mut latencies = Vec::with_capacity(n);
    let buy_user = Uuid::new_v4();

    // Warmup
    for _ in 0..100 {
        let o = Order::new_market(buy_user, PAIRS[0].to_string(), OrderSide::Buy, 0.1);
        let _ = books.place_order(o);
    }

    let start = Instant::now();
    for _ in 0..n {
        let o = Order::new_market(buy_user, PAIRS[0].to_string(), OrderSide::Buy, 1.0);
        let t0 = Instant::now();
        let result = books.place_order(o);
        let dur = t0.elapsed();
        match result {
            Ok(r) => {
                latencies.push(dur.as_micros() as u64);
                if r.trades.is_empty() {
                    // Book exhausted — repopulate
                    for j in 0..1000 {
                        let price = 1.0 + (j as f64 * 0.001);
                        let o = Order::new_limit(ask_user, PAIRS[0].to_string(), OrderSide::Sell, price, 10.0);
                        let _ = books.place_order(o);
                    }
                }
            }
            Err(e) => eprintln!("  [WARN] market order failed: {}", e),
        }
    }
    let elapsed = start.elapsed();
    let with_trades = latencies.len();

    print_latencies("market_buy (matching)", &latencies);
    println!("  {:<35} {} orders matched", "Result:", with_trades);
    println!("  {:<35} {:.2?}", "Total time:", elapsed);
    if elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} matches/sec", "Throughput:", with_trades as f64 / elapsed.as_secs_f64());
    }
}

// ========================================================================
// BENCHMARK 7: Concurrent Clients (10 clients x 1000 orders)
// ========================================================================
#[test]
fn benchmark_concurrent_clients() {
    print_header("BENCHMARK 7: Concurrent Clients (10 clients x 1000 orders each)");
    let pair = PAIRS[0];
    let num_clients = 10;
    let orders_per_client = 1000;
    let n = num_clients * orders_per_client;

    let counter = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let handles: Vec<_> = (0..num_clients)
        .map(|client_id| {
            let c = Arc::clone(&counter);
            let e = Arc::clone(&errors);
            std::thread::spawn(move || {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build().unwrap();
                for i in 0..orders_per_client {
                    let price = 0.5 + ((client_id * orders_per_client + i) as f64 * 0.001);
                    let body = serde_json::json!({
                        "id": Uuid::nil().to_string(),
                        "user_id": USER_ID,
                        "pair": pair,
                        "order_type": "Limit",
                        "side": "buy",
                        "price": price,
                        "quantity": 0.01,
                        "filled": 0.0,
                        "remaining": 0.01,
                        "status": "New",
                        "timestamp": 0,
                        "ttl_ms": null,
                        "is_swap": false,
                        "swap_target_currency": null,
                        "tee_signed": false,
                        "dot_verified": false,
                        "stealth": false,
                        "trailing_offset": null,
                        "trigger_price": null,
                        "hard_floor": null,
                        "track": "Compliant",
                        "style": "Standard",
                        "hidden_remaining": 0.0,
                        "filled_quantity": 0,
                        "client_order_id": None::<String>
                    });
                    let resp = client
                        .post(format!("{}/api/v1/order", API_BASE))
                        .header("Authorization", format!("Bearer {}", API_KEY))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send();
                    match resp {
                        Ok(r) if r.status().is_success() => { c.fetch_add(1, Ordering::Relaxed); }
                        _ => { e.fetch_add(1, Ordering::Relaxed); }
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
    let elapsed = start.elapsed();
    let success = counter.load(Ordering::Relaxed);
    let errs = errors.load(Ordering::Relaxed);
    let error_rate = if n > 0 { errs as f64 / n as f64 * 100.0 } else { 0.0 };

    println!("  {:<35} {}/{}", "Orders completed:", success, n);
    println!("  {:<35} {} ({:.2}%)", "Errors:", errs, error_rate);
    println!("  {:<35} {:.2?}", "Total time:", elapsed);
    if elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} orders/sec", "Throughput:", success as f64 / elapsed.as_secs_f64());
    }
}

// ========================================================================
// BENCHMARK 8: Internal Engine Throughput (OrderBookManager direct)
// ========================================================================
#[test]
fn benchmark_internal_engine_throughput() {
    print_header("BENCHMARK 8: Internal Engine Throughput (100K orders, direct)");
    let books = OrderBookManager::new();
    books.create_book(PAIRS[0]);

    let n = 100_000;
    let mut latencies = Vec::with_capacity(n);
    let user = Uuid::new_v4();

    // Warmup
    for i in 0..1000 {
        let o = Order::new_limit(user, PAIRS[0].to_string(), OrderSide::Buy, 0.5 + (i as f64 * 0.0001), 1.0);
        let _ = books.place_order(o);
    }

    let start = Instant::now();
    for i in 0..n {
        let side = if i % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell };
        let price = 0.5 + (i as f64 * 0.00001);
        let o = Order::new_limit(user, PAIRS[0].to_string(), side, price, 1.0);
        let t0 = Instant::now();
        let _ = books.place_order(o);
        latencies.push(t0.elapsed().as_micros() as u64);
    }
    let elapsed = start.elapsed();

    print_latencies("place_order (direct)", &latencies);
    println!("  {:<35} {:.2?}", "Total time:", elapsed);
    if elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} orders/sec", "Throughput:", n as f64 / elapsed.as_secs_f64());
    }
    println!("  {:<35} {}", "Total orders in books:", books.total_orders());
}

// ========================================================================
// BENCHMARK 9: Memory Usage Measurement
// ========================================================================
#[test]
fn benchmark_memory_usage() {
    print_header("BENCHMARK 9: Memory Usage (100K orders, then 50K cancellations)");
    let books = OrderBookManager::new();
    books.create_book(PAIRS[0]);

    let n = 100_000;
    let mut order_ids = Vec::with_capacity(n);
    let user = Uuid::new_v4();

    // Place 100K orders with unique prices (no crossing)
    for i in 0..n {
        let side = if i % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell };
        let price = 0.5 + (i as f64 * 0.00001);
        let o = Order::new_limit(user, PAIRS[0].to_string(), side, price, 1.0);
        let id = o.id;
        let _ = books.place_order(o);
        order_ids.push(id);
    }

    let order_struct_size = std::mem::size_of::<Order>();
    println!("  {:<35} {} bytes", "Order struct size:", order_struct_size);
    println!("  {:<35} {}", "Orders placed:", books.total_orders());
    println!("  {:<35} {:.2} MB", "Approx memory (structs only):", n as f64 * order_struct_size as f64 / 1_048_576.0);

    // Cancel 50K orders
    let cancel_start = Instant::now();
    for i in 0..50_000 {
        let _ = books.cancel_order(order_ids[i]);
    }
    let cancel_elapsed = cancel_start.elapsed();
    println!("  {:<35} 50,000", "Orders cancelled:");
    println!("  {:<35} {:.2?}", "Cancellation time:", cancel_elapsed);
    if cancel_elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} cancels/sec", "Cancel throughput:", 50_000f64 / cancel_elapsed.as_secs_f64());
    }
    println!();
    println!("  {:<35}", "NOTE: Memory freed depends on BTreeMap rebalancing.");
    println!("  {:<35}", "DashMap retains shards — expect partial RSS drop.");
    println!("  {:<35}", "For true leak check: use process RSS before/after with valgrind or /proc/self/status.");
}

// ========================================================================
// BENCHMARK 10: Cross-Pair Matching Throughput (Internal)
// ========================================================================
#[test]
fn benchmark_cross_pair_matching() {
    print_header("BENCHMARK 10: Cross-Pair Matching (5 pairs, 10K market orders each)");
    let books = OrderBookManager::new();
    let test_pairs = &PAIRS[..5];

    for &pair in test_pairs {
        books.create_book(pair);
        let ask_user = Uuid::new_v4();
        for i in 0..5_000 {
            let price = 1.0 + (i as f64 * 0.001);
            let o = Order::new_limit(ask_user, pair.to_string(), OrderSide::Sell, price, 10.0);
            let _ = books.place_order(o);
        }
    }

    let mut total_matched = 0u64;
    let start = Instant::now();
    for &pair in test_pairs {
        let buy_user = Uuid::new_v4();
        for _ in 0..10_000 {
            let o = Order::new_market(buy_user, pair.to_string(), OrderSide::Buy, 1.0);
            if let Ok(result) = books.place_order(o) {
                if !result.trades.is_empty() {
                    total_matched += 1;
                }
            }
        }
    }
    let elapsed = start.elapsed();

    println!("  {:<35} {} across {} pairs", "Orders matched:", total_matched, test_pairs.len());
    println!("  {:<35} {:.2?}", "Total time:", elapsed);
    if elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} matches/sec", "Throughput:", total_matched as f64 / elapsed.as_secs_f64());
    }
}

// ========================================================================
// BENCHMARK 11: Sustained Throughput (2-second burn)
// ========================================================================
#[test]
#[ignore]
fn benchmark_sustained_throughput() {
    print_header("BENCHMARK 11: Sustained Throughput (5-second burn, internal)");
    let books = OrderBookManager::new();
    books.create_book(PAIRS[0]);

    let user = Uuid::new_v4();
    let duration = Duration::from_secs(5);
    let mut count = 0u64;
    let start = Instant::now();

    while start.elapsed() < duration {
        let side = if count % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell };
        let price = 0.5 + ((count % 100_000) as f64 * 0.00001);
        let o = Order::new_limit(user, PAIRS[0].to_string(), side, price, 1.0);
        let _ = books.place_order(o);
        count += 1;
    }
    let elapsed = start.elapsed();

    println!("  {:<35} {}", "Orders placed:", count);
    println!("  {:<35} {:.2?}", "Duration:", elapsed);
    if elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} orders/sec", "Sustained TPS:", count as f64 / elapsed.as_secs_f64());
    }
}

// ========================================================================
// BENCHMARK 12: WAL with Metrics Pipeline
// ========================================================================
#[test]
fn benchmark_wal_metrics_pipeline() {
    print_header("BENCHMARK 12: WAL + Metrics Pipeline (20K operations)");
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let wal = WriteAheadLog::new("bench-pipeline", tmp_dir.path(), vec![])
        .expect("create WAL");
    let metrics = MetricsCollector::new();

    let n = 20_000;
    let mut latencies = Vec::with_capacity(n);
    let user = Uuid::new_v4();
    let order = Order::new_limit(user, "BTC/USDT".to_string(), OrderSide::Buy, 50000.0, 1.5);

    // Warmup
    for _ in 0..100 {
        let _ = wal.append(WALRecord::PlaceOrder(order.clone()));
        metrics.inc_orders();
    }

    let start = Instant::now();
    for _ in 0..n {
        let t0 = Instant::now();
        let _ = wal.append(WALRecord::PlaceOrder(order.clone()));
        metrics.inc_orders();
        metrics.inc_trades();
        latencies.push(t0.elapsed().as_micros() as u64);
    }
    let elapsed = start.elapsed();

    print_latencies("wal+metrics pipeline", &latencies);
    println!("  {:<35} {:.2?}", "Total time:", elapsed);
    if elapsed.as_secs_f64() > 0.0 {
        println!("  {:<35} {:.0} ops/sec", "Throughput:", n as f64 / elapsed.as_secs_f64());
    }
    let snap = metrics.snapshot();
    println!("  {:<35} {}", "Metrics orders:", snap["tps_peak"]);
}

// ========================================================================
// MAIN TEST: Run all benchmarks in sequence
// ========================================================================
#[test]
fn run_all_benchmarks() {
    println!();
    println!("{}", "#".repeat(90));
    println!("  THE-BRIDGE — COMPREHENSIVE PERFORMANCE BENCHMARK SUITE");
    println!("  Server: {} | Node: engine-1", API_BASE);
    println!("  Date: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
    println!("{}", "#".repeat(90));

    // Run internal benchmarks (don't require server)
    benchmark_wal_throughput();
    benchmark_internal_engine_throughput();
    benchmark_matching_throughput();
    benchmark_cross_pair_matching();
    benchmark_memory_usage();
    benchmark_wal_metrics_pipeline();

    // Run HTTP benchmarks (require server on port 3001)
    benchmark_sequential_throughput();
    benchmark_concurrent_throughput();
    benchmark_multi_pair_throughput();
    benchmark_full_pipeline_latency();
    benchmark_concurrent_clients();

    println!("\n{}", "=".repeat(90));
    println!("  BENCHMARK SUITE COMPLETE");
    println!("{}", "=".repeat(90));
}
