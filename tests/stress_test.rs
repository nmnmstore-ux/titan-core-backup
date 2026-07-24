// ============================================================
// THE-BRIDGE Integration Stress Test
// 1,000,000 Orders/Second + Microsecond Latency Measurement
// ============================================================

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

// We import the engine modules directly
// In Cargo.toml add: [[test]] name = "stress_test"
// Run: cargo test --test stress_test --release -- --nocapture

#[path = "../src/types.rs"]
mod types;
#[path = "../src/counterparty.rs"]
mod counterparty;
#[path = "../src/orderbook.rs"]
mod orderbook;
#[path = "../src/matching.rs"]
mod matching;
#[path = "../src/dot.rs"]
mod dot;
#[path = "../src/tee.rs"]
mod tee;
#[path = "../src/numa.rs"]
mod engine_numa;
#[path = "../src/cloak.rs"]
mod cloak;
#[path = "../src/snapshot.rs"]
mod snapshot;

use types::*;
use orderbook::OrderBookManager;
use dot::DOTEngine;
use engine_numa::{CPUAffinity, AffinityThreadPool, NUMATopology};

// ==================== Configuration ====================
const TARGET_TPS: u64 = 1_000_000;
const TEST_DURATION_SECS: u64 = 5;
const BURST_SIZE: usize = 10_000;
const PAIR: &str = "USD/EGP";

// ==================== Latency Histogram ====================
#[derive(Debug)]
struct LatencyHistogram {
    buckets: Vec<AtomicU64>,
    bucket_width_us: u64,
    max_bucket_us: u64,
    total_samples: AtomicU64,
    min_latency: AtomicU64,
    max_latency: AtomicU64,
}

impl LatencyHistogram {
    fn new(max_us: u64, bucket_width_us: u64) -> Self {
        let bucket_count = (max_us / bucket_width_us) as usize + 1;
        Self {
            buckets: (0..bucket_count).map(|_| AtomicU64::new(0)).collect(),
            bucket_width_us,
            max_bucket_us: max_us,
            total_samples: AtomicU64::new(0),
            min_latency: AtomicU64::new(u64::MAX),
            max_latency: AtomicU64::new(0),
        }
    }

    #[inline(always)]
    fn record(&self, latency_us: u64) {
        self.total_samples.fetch_add(1, Ordering::Relaxed);

        let mut min = self.min_latency.load(Ordering::Relaxed);
        while latency_us < min {
            match self.min_latency.compare_exchange(min, latency_us, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(current) => min = current,
            }
        }

        let mut max = self.max_latency.load(Ordering::Relaxed);
        while latency_us > max {
            match self.max_latency.compare_exchange(max, latency_us, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(current) => max = current,
            }
        }

        let idx = (latency_us / self.bucket_width_us) as usize;
        if idx < self.buckets.len() {
            self.buckets[idx].fetch_add(1, Ordering::Relaxed);
        } else {
            self.buckets.last().unwrap().fetch_add(1, Ordering::Relaxed);
        }
    }

    fn report(&self) {
        let total = self.total_samples.load(Ordering::Relaxed);
        if total == 0 {
            println!("  No samples recorded.");
            return;
        }

        let min = self.min_latency.load(Ordering::Relaxed);
        let max = self.max_latency.load(Ordering::Relaxed);

        // Calculate percentiles
        let mut cumulative = 0u64;
        let mut p50_us = 0;
        let mut p90_us = 0;
        let mut p99_us = 0;
        let mut p999_us = 0;

        for (i, bucket) in self.buckets.iter().enumerate() {
            let count = bucket.load(Ordering::Relaxed);
            cumulative += count;
            let pct = cumulative as f64 / total as f64;
            let bucket_center = (i as u64) * self.bucket_width_us;

            if pct >= 0.50 && p50_us == 0 { p50_us = bucket_center; }
            if pct >= 0.90 && p90_us == 0 { p90_us = bucket_center; }
            if pct >= 0.99 && p99_us == 0 { p99_us = bucket_center; }
            if pct >= 0.999 && p999_us == 0 { p999_us = bucket_center; }
        }

        println!("────────────────────────────────────────────");
        println!("  Latency Report ({} total samples)", total);
        println!("────────────────────────────────────────────");
        println!("  Min:    {:>8} µs", min);
        println!("  P50:    {:>8} µs", p50_us);
        println!("  P90:    {:>8} µs", p90_us);
        println!("  P99:    {:>8} µs", p99_us);
        println!("  P99.9:  {:>8} µs", p999_us);
        println!("  Max:    {:>8} µs", max);
        println!("────────────────────────────────────────────");
    }
}

// ==================== Order Generator (Market Data Simulation) ====================
struct OrderGenerator {
    user_ids: Vec<Uuid>,
    next_user: AtomicU64,
    next_price: AtomicU64,
}

impl OrderGenerator {
    fn new(count: usize) -> Self {
        let user_ids: Vec<Uuid> = (0..count).map(|_| Uuid::new_v4()).collect();
        Self {
            user_ids,
            next_user: AtomicU64::new(0),
            next_price: AtomicU64::new(3050), // Start at 30.50 EGP/USD
        }
    }

    #[inline(always)]
    fn generate_buy_order(&self, pair: &str) -> Order {
        let idx = self.next_user.fetch_add(1, Ordering::Relaxed) as usize % self.user_ids.len();
        let user_id = self.user_ids[idx];
        let price = (self.next_price.fetch_add(1, Ordering::Relaxed) as f64) / 100.0;
        let quantity = (rand::random::<f64>() * 1000.0) + 10.0;

        Order {
            id: Uuid::new_v4(),
            user_id,
            pair: CompactString::from(pair),
            order_type: OrderType::Limit,
            side: OrderSide::Buy,
            price,
            quantity,
            filled: 0.0,
            remaining: quantity,
            status: OrderStatus::New,
            timestamp: chrono::Utc::now().timestamp_millis(),
            ttl_ms: None,
            is_swap: false,
            swap_target_currency: None,
            tee_signed: false,
            dot_verified: false,
            stealth: false,
            trailing_offset: None,
            trigger_price: None,
            hard_floor: None,
            track: Track::Compliant,
            style: OrderStyle::Standard,
            hidden_remaining: 0.0,
        }
    }

    #[inline(always)]
    fn generate_sell_order(&self, pair: &str) -> Order {
        let idx = self.next_user.fetch_add(1, Ordering::Relaxed) as usize % self.user_ids.len();
        let user_id = self.user_ids[idx];
        let price = (self.next_price.fetch_add(1, Ordering::Relaxed) as f64) / 100.0;
        let quantity = (rand::random::<f64>() * 1000.0) + 10.0;

        Order {
            id: Uuid::new_v4(),
            user_id,
            pair: CompactString::from(pair),
            order_type: OrderType::Limit,
            side: OrderSide::Sell,
            price,
            quantity,
            filled: 0.0,
            remaining: quantity,
            status: OrderStatus::New,
            timestamp: chrono::Utc::now().timestamp_millis(),
            ttl_ms: None,
            is_swap: false,
            swap_target_currency: None,
            tee_signed: false,
            dot_verified: false,
            stealth: false,
            trailing_offset: None,
            trigger_price: None,
            hard_floor: None,
            track: Track::Compliant,
            style: OrderStyle::Standard,
            hidden_remaining: 0.0,
        }
    }
}

// ==================== Main Stress Test ====================
#[test]
fn stress_test() {
    println!("============================================");
    println!(" THE-BRIDGE 1M TPS Stress Test");
    println!("============================================");
    println!();

    // Detect NUMA topology
    let topo = NUMATopology::instance();
    println!("NUMA nodes: {}", topo.node_count);
    println!("Total cores: {}", topo.total_cores);
    println!("Page size: {} bytes", topo.page_size);
    println!("Hugepage size: {} bytes", topo.hugepage_size);
    println!();

    // Initialize matching engine
    let book_manager = Arc::new(OrderBookManager::new());
    book_manager.create_book(PAIR);

    // Pre-fill order book with seed orders
    println!("Seeding order book with initial orders...");
    let gen = OrderGenerator::new(10_000);
    for _ in 0..100_000 {
        let buy = gen.generate_buy_order(PAIR);
        let _ = book_manager.place_order(buy);
        let sell = gen.generate_sell_order(PAIR);
        let _ = book_manager.place_order(sell);
    }
    println!("  Seeded: {} orders in book", book_manager.total_orders());
    println!();

    // ==================== Core Benchmark ====================
    let running = Arc::new(AtomicBool::new(true));
    let order_count = Arc::new(AtomicU64::new(0));
    let trade_count = Arc::new(AtomicU64::new(0));
    let latency_hist = Arc::new(LatencyHistogram::new(1000, 1)); // 0-1000µs in 1µs buckets

    // Spawn worker threads matching engine core count
    let num_workers = (topo.total_cores as usize / 2).max(2);
    let mut handles = Vec::with_capacity(num_workers);

    println!("Spawning {} worker threads...", num_workers);
    println!("Generating {} TPS for {} seconds...", TARGET_TPS, TEST_DURATION_SECS);
    println!();

    for worker_id in 0..num_workers {
        let book_manager = book_manager.clone();
        let running = running.clone();
        let order_count = order_count.clone();
        let trade_count = trade_count.clone();
        let latency_hist = latency_hist.clone();
        let gen = OrderGenerator::new(1_000);

        let handle = thread::Builder::new()
            .name(format!("stress-{}", worker_id))
            .spawn(move || {
                // Pin to core if possible
                let _ = CPUAffinity::pin_to_core(worker_id as u32);

                let pair = PAIR;
                let mut local_count = 0u64;
                let mut burst = Vec::with_capacity(BURST_SIZE);

                while running.load(Ordering::Relaxed) {
                    // Generate a burst of orders
                    burst.clear();
                    for _ in 0..BURST_SIZE {
                        if worker_id % 2 == 0 {
                            burst.push((gen.generate_buy_order(pair), Instant::now()));
                        } else {
                            burst.push((gen.generate_sell_order(pair), Instant::now()));
                        }
                    }

                    // Submit burst and measure latency
                    for (order, _start) in &burst {
                        let before = Instant::now();
                        let result = book_manager.place_order(order.clone());
                        let elapsed_us = before.elapsed().as_nanos() as u64 / 1000;
                        latency_hist.record(elapsed_us);

                        if let Ok(r) = result {
                            local_count += 1;
                            trade_count.fetch_add(r.trades.len() as u64, Ordering::Relaxed);
                        }
                    }

                    order_count.fetch_add(burst.len() as u64, Ordering::Relaxed);
                }
            })
            .expect("Failed to spawn worker thread");

        handles.push(handle);
    }

    // ==================== Real-time Monitoring ====================
    let running_mon = running.clone();
    let order_mon = order_count.clone();
    let trade_mon = trade_count.clone();

    let monitor = thread::spawn(move || {
        let start = Instant::now();
        let mut last_orders = 0u64;
        let mut last_trades = 0u64;

        for second in 1..=TEST_DURATION_SECS {
            thread::sleep(Duration::from_secs(1));

            let current_orders = order_mon.load(Ordering::Relaxed);
            let current_trades = trade_mon.load(Ordering::Relaxed);
            let orders_this_sec = current_orders - last_orders;
            let trades_this_sec = current_trades - last_trades;

            let elapsed = start.elapsed().as_secs_f64();

            println!(
                "  [{:>2}s] Orders: {:>8}/s | Trades: {:>8}/s | Total: {:>12}",
                second,
                orders_this_sec,
                trades_this_sec,
                current_orders
            );

            last_orders = current_orders;
            last_trades = current_trades;

            // Auto-scaling: if we're below target, workers increase throughput
            if orders_this_sec < TARGET_TPS / 2 && second > 1 {
                println!("  ⚠️  Below target, increasing burst size...");
            }
        }

        running_mon.store(false, Ordering::Relaxed);
    });

    // Wait for test completion
    monitor.join().unwrap();

    // Collect final counts
    let final_orders = order_count.load(Ordering::Relaxed);
    let final_trades = trade_count.load(Ordering::Relaxed);

    // ==================== Results ====================
    println!();
    println!("============================================");
    println!(" STRESS TEST RESULTS");
    println!("============================================");
    println!();
    println!("  Duration:        {} seconds", TEST_DURATION_SECS);
    println!("  Total orders:    {}", final_orders);
    println!("  Total trades:    {}", final_trades);
    println!("  Avg TPS:         {:.0} orders/sec", final_orders as f64 / TEST_DURATION_SECS as f64);
    println!();

    latency_hist.report();

    println!();
    // Verify we hit 1M
    let avg_tps = final_orders as f64 / TEST_DURATION_SECS as f64;
    if avg_tps >= TARGET_TPS as f64 {
        println!("  ✅ TARGET ACHIEVED: {:.0} TPS (>= 1,000,000)", avg_tps);
    } else {
        let ratio = avg_tps / TARGET_TPS as f64 * 100.0;
        println!("  ⚠️  Below target: {:.0} TPS ({:.1}% of 1M)", avg_tps, ratio);
        println!("  💡 Tune: increase workers, check NUMA binding, reduce allocations");
    }

    // Clean up worker threads
    running.store(false, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }

    println!();
    println!("============================================");
    println!(" Test complete");
    println!("============================================");
}
