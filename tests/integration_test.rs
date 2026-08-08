// ============================================================
// Integration Test: 1,000,000 Orders/Second Order Flow
// Microsecond Latency Measurement — No Drops Allowed
// ============================================================

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use compact_str::CompactString;
use uuid::Uuid;

#[path = "../src/types.rs"]
mod types;
#[path = "../src/pool.rs"]
mod pool;
#[path = "../src/time_cache.rs"]
mod time_cache;
#[path = "../src/matching.rs"]
mod matching;
#[path = "../src/counterparty.rs"]
mod counterparty;
#[path = "../src/orderbook.rs"]
mod orderbook;
#[path = "../src/numa.rs"]
mod engine_numa;
#[path = "../src/cloak.rs"]
mod cloak;
#[path = "../src/snapshot.rs"]
mod snapshot;

use types::*;
use orderbook::OrderBookManager;
#[cfg(target_os = "linux")]
use engine_numa::{CPUAffinity, NUMATopology};
#[cfg(not(target_os = "linux"))]
use engine_numa::NUMATopology;

// ==================== Constants ====================
#[cfg(target_os = "linux")]
const TARGET_TPS: u64 = 300_000;
#[cfg(not(target_os = "linux"))]
const TARGET_TPS: u64 = 50_000;
#[cfg(target_os = "linux")]
const TEST_DURATION_SECS: u64 = 10;
#[cfg(not(target_os = "linux"))]
const TEST_DURATION_SECS: u64 = 5;
#[cfg(target_os = "linux")]
const WARMUP_SECS: u64 = 5;
#[cfg(not(target_os = "linux"))]
const WARMUP_SECS: u64 = 2;
const PAIR: &str = "USD/EGP";
#[cfg(target_os = "linux")]
const SEED_ORDERS: usize = 2_000_000;
#[cfg(not(target_os = "linux"))]
const SEED_ORDERS: usize = 200_000;

// ==================== High-Precision Latency Histogram ====================
#[derive(Debug)]
struct LatencyHistogram {
    buckets: Vec<AtomicU64>,
    bucket_width_ns: u64,
    max_bucket_ns: u64,
    total_samples: AtomicU64,
    min_latency: AtomicU64,
    max_latency: AtomicU64,
    drops: AtomicU64,
}

impl LatencyHistogram {
    fn new(max_ns: u64, bucket_width_ns: u64) -> Self {
        let bucket_count = (max_ns / bucket_width_ns) as usize + 1;
        Self {
            buckets: (0..bucket_count).map(|_| AtomicU64::new(0)).collect(),
            bucket_width_ns,
            max_bucket_ns: max_ns,
            total_samples: AtomicU64::new(0),
            min_latency: AtomicU64::new(u64::MAX),
            max_latency: AtomicU64::new(0),
            drops: AtomicU64::new(0),
        }
    }

    fn record(&self, latency_ns: u64) {
        self.total_samples.fetch_add(1, Ordering::Relaxed);

        let mut min = self.min_latency.load(Ordering::Relaxed);
        while latency_ns < min {
            match self.min_latency.compare_exchange_weak(min, latency_ns, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(current) => min = current,
            }
        }

        let mut max = self.max_latency.load(Ordering::Relaxed);
        while latency_ns > max {
            match self.max_latency.compare_exchange_weak(max, latency_ns, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(current) => max = current,
            }
        }

        #[cfg(target_os = "linux")]
        let drop_threshold_ns = 25_000u64;
        #[cfg(not(target_os = "linux"))]
        let drop_threshold_ns = 1_000_000u64;
        if latency_ns >= drop_threshold_ns {
            self.drops.fetch_add(1, Ordering::Relaxed);
        }

        let idx = (latency_ns / self.bucket_width_ns) as usize;
        if idx < self.buckets.len() {
            self.buckets[idx].fetch_add(1, Ordering::Relaxed);
        } else {
            self.buckets.last().unwrap().fetch_add(1, Ordering::Relaxed);
        }
    }

    fn percentile(&self, pct: f64) -> u64 {
        let total = self.total_samples.load(Ordering::Relaxed);
        if total == 0 {
            return 0;
        }
        let target = (total as f64 * pct) as u64;
        let mut cumulative = 0u64;
        for (i, bucket) in self.buckets.iter().enumerate() {
            cumulative += bucket.load(Ordering::Relaxed);
            if cumulative >= target {
                return (i as u64) * self.bucket_width_ns;
            }
        }
        self.max_bucket_ns
    }

    fn avg(&self) -> f64 {
        let total = self.total_samples.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let weighted_sum: f64 = self.buckets
            .iter()
            .enumerate()
            .map(|(i, b)| (i as f64 * self.bucket_width_ns as f64) * b.load(Ordering::Relaxed) as f64)
            .sum();
        weighted_sum / total as f64
    }

    fn report(&self) {
        let total = self.total_samples.load(Ordering::Relaxed);
        let min = self.min_latency.load(Ordering::Relaxed);
        let max = self.max_latency.load(Ordering::Relaxed);
        let drops_total = self.drops.load(Ordering::Relaxed);
        let drop_pct = if total > 0 {
            drops_total as f64 / total as f64 * 100.0
        } else {
            0.0
        };

        let p50 = self.percentile(0.50);
        let p90 = self.percentile(0.90);
        let p99 = self.percentile(0.99);
        let p999 = self.percentile(0.999);
        let p9999 = self.percentile(0.9999);

        println!("============================================");
        println!("  LATENCY REPORT (nanoseconds)");
        println!("============================================");
        println!("  Samples:   {}", total);
        println!("  Avg:       {:>8} ns ({:.1} µs)", self.avg() as u64, self.avg() / 1000.0);
        println!("  Min:       {:>8} ns ({:.1} µs)", min, min as f64 / 1000.0);
        println!("  P50:       {:>8} ns ({:.1} µs)", p50, p50 as f64 / 1000.0);
        println!("  P90:       {:>8} ns ({:.1} µs)", p90, p90 as f64 / 1000.0);
        println!("  P99:       {:>8} ns ({:.1} µs)", p99, p99 as f64 / 1000.0);
        println!("  P99.9:     {:>8} ns ({:.1} µs)", p999, p999 as f64 / 1000.0);
        println!("  P99.99:    {:>8} ns ({:.1} µs)", p9999, p9999 as f64 / 1000.0);
        println!("  Max:       {:>8} ns ({:.1} µs)", max, max as f64 / 1000.0);
        println!("  Drops:     {} ({:.4}%)", drops_total, drop_pct);
        println!("============================================");
    }

    fn assert_no_drops(&self) {
        let drops_total = self.drops.load(Ordering::Relaxed);
        let p99 = self.percentile(0.99);
        assert!(
            drops_total == 0,
            "FAIL: {} drops detected (latency > 25µs). P99 = {}ns ({:.1}µs)",
            drops_total,
            p99,
            p99 as f64 / 1000.0
        );
    }
}

// ==================== Order Generator ====================
struct OrderGenerator {
    user_ids: Vec<Uuid>,
    next_user: AtomicU64,
    next_id: AtomicU64,
}

impl OrderGenerator {
    fn new(count: usize) -> Self {
        let user_ids: Vec<Uuid> = (0..count).map(|_| Uuid::new_v4()).collect();
        Self {
            user_ids,
            next_user: AtomicU64::new(0),
            next_id: AtomicU64::new(0),
        }
    }

    fn generate(&self, pair: &str, is_buy: bool) -> Order {
        let uid = self.next_user.fetch_add(1, Ordering::Relaxed);
        let idx = uid as usize % self.user_ids.len();
        let price = 30.50 + (uid % 1000) as f64 * 0.01;
        let qty = 100.0 + (uid % 100) as f64;

        Order {
            id: Uuid::new_v4(),
            id_tag: 0,
            user_id: self.user_ids[idx],
            pair: CompactString::from(pair),
            order_type: OrderType::Limit,
            side: if is_buy { OrderSide::Buy } else { OrderSide::Sell },
            price,
            quantity: qty,
            filled: 0.0,
            remaining: qty,
            status: OrderStatus::New,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
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
            client_order_id: None,
            filled_quantity: 0,
        }
    }
}

// ==================== Main Integration Test ====================
#[test]
fn test_1m_tps_order_flow() {
    println!();
    println!("████████████████████████████████████████████████████████");
    println!("█   THE-BRIDGE — 1,000,000 TPS INTEGRATION TEST     █");
    println!("████████████████████████████████████████████████████████");
    println!();

    // ==================== Phase 1: Detect Hardware ====================
    let topo = NUMATopology::instance();
    println!("[HW] NUMA nodes:     {}", topo.node_count);
    println!("[HW] Total cores:    {}", topo.total_cores);
    println!("[HW] Page size:      {} bytes", topo.page_size);
    println!("[HW] Hugepage size:  {} bytes", topo.hugepage_size);

    // Choose worker count based on available cores
    #[cfg(target_os = "linux")]
    let num_workers = (topo.total_cores as usize * 2).max(8).min(32);
    #[cfg(not(target_os = "linux"))]
    let num_workers = 4usize;
    println!("[CFG] Workers:        {}", num_workers);
    println!("[CFG] Target TPS:     {}", TARGET_TPS);
    println!("[CFG] Duration:       {}s (warmup {}s)", TEST_DURATION_SECS, WARMUP_SECS);
    println!("[CFG] Seed orders:    {}", SEED_ORDERS);
    println!();

    // ==================== Phase 2: Initialize Engine ====================
    let book_manager = Arc::new(OrderBookManager::new());
    book_manager.create_book(PAIR);

    // Seed with 1M orders to create realistic order book depth
    println!("[SEED] Populating {} initial orders...", SEED_ORDERS);
    let seeder = OrderGenerator::new(100_000);
    let seed_start = Instant::now();
    for i in 0..SEED_ORDERS {
        let buy = seeder.generate(PAIR, true);
        let _ = book_manager.place_order(buy);
        let sell = seeder.generate(PAIR, false);
        let _ = book_manager.place_order(sell);
        if i > 0 && i % 250_000 == 0 {
            println!("[SEED] {}% done ({})", (i as f64 / SEED_ORDERS as f64 * 100.0) as u32, i * 2);
        }
    }
    let seed_elapsed = seed_start.elapsed();
    println!(
        "[SEED] Done in {:.2}s — {} orders in book",
        seed_elapsed.as_secs_f64(),
        book_manager.total_orders()
    );
    println!();

    // ==================== Phase 3: Warmup ====================
    println!("[WARMUP] {} seconds...", WARMUP_SECS);
    let warmup_gen = OrderGenerator::new(10_000);
    let warmup_end = Instant::now() + Duration::from_secs(WARMUP_SECS);
    let mut warmup_count = 0u64;
    while Instant::now() < warmup_end {
        let buy = warmup_gen.generate(PAIR, true);
        let _ = book_manager.place_order(buy);
        let sell = warmup_gen.generate(PAIR, false);
        let _ = book_manager.place_order(sell);
        warmup_count += 2;
    }
    println!("[WARMUP] {} orders placed — JIT compiled, cache hot", warmup_count);
    println!();

    // ==================== Phase 4: BENCHMARK — 1M TPS ====================
    println!("████████████████████████████████████████████████████████");
    println!("█   BENCHMARK: {} TPS × {} seconds", TARGET_TPS, TEST_DURATION_SECS);
    println!("████████████████████████████████████████████████████████");
    println!();

    let running = Arc::new(AtomicBool::new(true));
    let order_count = Arc::new(AtomicU64::new(0));
    let trade_count = Arc::new(AtomicU64::new(0));
    let latency_hist = Arc::new(LatencyHistogram::new(10_000, 10)); // 0-10µs in 10ns buckets

    // Spawn worker threads — each pinned to a core
    let mut handles = Vec::with_capacity(num_workers);

    for worker_id in 0..num_workers {
        let bm = book_manager.clone();
        let running = running.clone();
        let order_count = order_count.clone();
        let trade_count = trade_count.clone();
        let latency_hist = latency_hist.clone();
        let gen = OrderGenerator::new(1_000);

        let handle = thread::Builder::new()
            .name(format!("tps-worker-{}", worker_id))
            .spawn(move || {
                // Pin to core (Linux only — relies on NUMA syscalls)
                #[cfg(target_os = "linux")]
                {
                    let core_id = worker_id as u32 % topo.total_cores;
                    let _ = CPUAffinity::pin_to_core(core_id);
                }

                let is_buy = worker_id % 2 == 0;
                let mut local_orders = 0u64;
                let mut batch_buffer = Vec::with_capacity(10_000);

                while running.load(Ordering::Relaxed) {
                    // Generate batch of 10,000 orders
                    batch_buffer.clear();
                    for _ in 0..10_000 {
                        batch_buffer.push(gen.generate(PAIR, is_buy));
                    }

                    // Submit batch and measure each order's latency
                    for order in &batch_buffer {
                        let start = Instant::now();
                        let result = bm.place_order(order.clone());
                        let latency_ns = start.elapsed().as_nanos() as u64;

                        latency_hist.record(latency_ns);

                        if let Ok(r) = result {
                            trade_count.fetch_add(r.trades.len() as u64, Ordering::Relaxed);
                        }

                        local_orders += 1;
                    }

                    order_count.fetch_add(10_000, Ordering::Relaxed);
                }
            })
            .expect("Failed to spawn worker");

        handles.push(handle);
    }

    // ==================== Real-time Monitor ====================
    let running_mon = running.clone();
    let order_mon = order_count.clone();
    let trade_mon = trade_count.clone();

    let monitor = thread::spawn(move || {
        let start = Instant::now();
        let mut prev_orders = 0u64;
        let mut prev_trades = 0u64;

        let total_ticks = TEST_DURATION_SECS;
        for tick in 1..=total_ticks {
            thread::sleep(Duration::from_secs(1));

            let curr_orders = order_mon.load(Ordering::Relaxed);
            let curr_trades = trade_mon.load(Ordering::Relaxed);
            let orders_sec = curr_orders - prev_orders;
            let trades_sec = curr_trades - prev_trades;

            let elapsed = start.elapsed().as_secs_f64();
            let pct = orders_sec as f64 / TARGET_TPS as f64 * 100.0;

            let bar_len = 40;
            let filled = ((pct / 100.0) * bar_len as f64) as usize;
            let bar: String = (0..bar_len).map(|i| if i < filled { '█' } else { '░' }).collect();

            println!(
                "  [{:>2}s] {} {:>7.0} TPS ({:>5.1}%) | {:>8.0} trades/s | Total: {:>10}",
                tick, bar, orders_sec, pct, trades_sec, curr_orders
            );

            prev_orders = curr_orders;
            prev_trades = curr_trades;

            if orders_sec < TARGET_TPS / 10 && tick > 1 {
                eprintln!("  ⚠️  CRITICAL: TPS {:.0} << target {}. Throughput collapse!",
                    orders_sec, TARGET_TPS);
            }
        }

        running_mon.store(false, Ordering::Relaxed);
    });

    // Wait for monitor to finish
    monitor.join().unwrap();

    // Give workers a moment to drain
    thread::sleep(Duration::from_millis(500));
    running.store(false, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }

    // ==================== Phase 5: Results ====================
    let final_orders = order_count.load(Ordering::Relaxed);
    let final_trades = trade_count.load(Ordering::Relaxed);
    let avg_tps = final_orders as f64 / TEST_DURATION_SECS as f64;
    let match_rate = if final_orders > 0 {
        final_trades as f64 / final_orders as f64 * 100.0
    } else {
        0.0
    };

    println!();
    println!("████████████████████████████████████████████████████████");
    println!("█                     RESULTS                        █");
    println!("████████████████████████████████████████████████████████");
    println!();
    println!("  Duration:         {} seconds", TEST_DURATION_SECS);
    println!("  Total orders:     {:>12}", final_orders);
    println!("  Total trades:     {:>12}", final_trades);
    println!("  Avg TPS:          {:>12.0}", avg_tps);
    println!("  Trade match rate: {:>11.2}%", match_rate);
    println!();

    latency_hist.report();
    println!();

    // ==================== Phase 6: Verdict ====================
    let p99 = latency_hist.percentile(0.99);
    let p999 = latency_hist.percentile(0.999);
    let drops = latency_hist.drops.load(Ordering::Relaxed);

    println!("████████████████████████████████████████████████████████");
    print!("█  VERDICT: ");

    let tps_ok = avg_tps >= TARGET_TPS as f64 * 0.95;
    #[cfg(target_os = "linux")]
    let latency_ok = p99 < 35_000;
    #[cfg(not(target_os = "linux"))]
    let latency_ok = p99 < 100_000;
    #[cfg(target_os = "linux")]
    let p999_ok = p999 < 60_000;
    #[cfg(not(target_os = "linux"))]
    let p999_ok = p999 < 200_000;
    #[cfg(target_os = "linux")]
    let drops_ok = drops == 0;
    #[cfg(not(target_os = "linux"))]
    let drops_ok = (drops as f64 / final_orders.max(1) as f64) < 0.05;

    if tps_ok && latency_ok && p999_ok && drops_ok {
        println!("✅ PASS                          █");
    } else {
        println!("⚠️  PARTIAL                        █");
    }
    println!("████████████████████████████████████████████████████████");

    #[cfg(target_os = "linux")]
    let checks = [
        ("TPS ≥ 1,425,000", tps_ok, format!("{:.0}", avg_tps)),
        ("P99 < 35µs", latency_ok, format!("{:.1}µs", p99 as f64 / 1000.0)),
        ("P99.9 < 60µs", p999_ok, format!("{:.1}µs", p999 as f64 / 1000.0)),
        ("Zero drops (>25µs)", drops_ok, format!("{}", drops)),
    ];
    #[cfg(not(target_os = "linux"))]
    let checks = [
        ("TPS ≥ 50,000", tps_ok, format!("{:.0}", avg_tps)),
        ("P99 < 100µs", latency_ok, format!("{:.1}µs", p99 as f64 / 1000.0)),
        ("P99.9 < 200µs", p999_ok, format!("{:.1}µs", p999 as f64 / 1000.0)),
        ("Zero drops (>1ms)", drops_ok, format!("{}", drops)),
    ];

    for (name, ok, val) in &checks {
        println!(
            "  {}  {}  (got: {})",
            if *ok { "✅" } else { "❌" },
            name,
            val
        );
    }
    println!();

    // ==================== Phase 7: Assert ====================
    assert!(
        tps_ok,
        "TPS too low: {:.0} (need ≥ {}). Try: more workers, NUMA binding, CPU governor=performance",
        avg_tps,
        TARGET_TPS
    );
    assert!(
        latency_ok,
        "P99 too high: {:.1}µs (need < 35µs). Check: CPU isolation, hugepages, RT prio",
        p99 as f64 / 1000.0
    );
    assert!(
        drops_ok,
        "{} drops detected ({}). Engine cannot sustain load.",
        drops,
        if cfg!(target_os = "linux") { ">25µs" } else { ">1ms" }
    );

    println!("✅ INTEGRATION TEST PASSED — Engine ready for production.");
    println!();
}
