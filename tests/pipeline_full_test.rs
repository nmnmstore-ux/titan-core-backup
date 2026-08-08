use std::sync::Arc;
use uuid::Uuid;

#[path = "../src/types.rs"]
mod types;
#[path = "../src/pool.rs"]
mod pool;
#[path = "../src/time_cache.rs"]
mod time_cache;
#[path = "../src/matching.rs"]
mod matching;
#[path = "../src/orderbook.rs"]
mod orderbook;
#[path = "../src/dual_track.rs"]
mod dual_track;
#[path = "../src/ghost_integration.rs"]
mod ghost_integration;
#[path = "../src/smart_router.rs"]
mod smart_router;
#[path = "../src/universal_bridge.rs"]
mod universal_bridge;
#[path = "../src/pipeline.rs"]
mod pipeline;
#[path = "../src/counterparty.rs"]
mod counterparty;
#[path = "../src/cloak.rs"]
mod cloak;
#[path = "../src/snapshot.rs"]
mod snapshot;
#[path = "../src/numa.rs"]
mod numa;
#[path = "../src/threshold_crypto.rs"]
mod threshold_crypto;
#[path = "../src/encrypted_mempool.rs"]
mod encrypted_mempool;
#[path = "../src/batch_auction.rs"]
mod batch_auction;

use types::*;
use orderbook::OrderBookManager;
use dual_track::DualTrackRouter;
use ghost_integration::{GhostCloak, BrokerEvasionStrategy};
use universal_bridge::{UniversalBridge, Capability};

fn make_order(side: OrderSide, price: f64, qty: f64, track: Track) -> Order {
    Order {
        id: Uuid::new_v4(),
        id_tag: 0,
        user_id: Uuid::new_v4(),
        pair: "USD/EGP".into(),
        order_type: OrderType::Limit,
        side,
        price,
        quantity: qty,
        filled: 0.0,
        remaining: qty,
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
        track,
        style: OrderStyle::Standard,
        hidden_remaining: 0.0,
        client_order_id: None,
        filled_quantity: 0,
    }
}

// ===== Sync tests: these call .blocking_read/write on tokio RwLocks, so they MUST run outside tokio runtime =====

#[test]
fn test_pipeline_compliant_order() {
    let books = OrderBookManager::new();
    books.create_book("USD/EGP");
    let sell = make_order(OrderSide::Sell, 30.50, 100_000.0, Track::Compliant);
    books.place_order(sell).unwrap();
    let buy = make_order(OrderSide::Buy, 30.50, 1_000.0, Track::Compliant);
    let result = books.place_order(buy).unwrap();
    assert!(!result.trades.is_empty(), "Compliant: should fill");
}

#[test]
fn test_pipeline_ghost_order() {
    let books = OrderBookManager::new();
    books.create_book("USD/EGP");
    let sell = make_order(OrderSide::Sell, 30.50, 100_000.0, Track::Autonomous);
    books.place_order(sell).unwrap();
    let buy = make_order(OrderSide::Buy, 30.50, 500.0, Track::Autonomous);
    let result = books.place_order(buy).unwrap();
    assert!(!result.trades.is_empty(), "Ghost: should fill");
}

#[test]
fn test_ghost_cloak_and_split() {
    let ghost = GhostCloak::new();
    let cloaked = ghost.cloak_order("user_42", "order_99", "BTC/USD", 50000.0, &Track::Autonomous);
    assert!(cloaked.phantom_id.starts_with("phantom_"));
    assert_eq!(ghost.resolve_phantom(&cloaked.phantom_id), Some("user_42".to_string()));

    let plan = ghost.split_order("SPLIT_1", "whale", "USD/EGP", 50_000.0, 30.50, "buy", 1_525_000.0);
    assert!(plan.fragments.len() >= 2, "split into {} fragments", plan.fragments.len());
    let total: f64 = plan.fragments.iter().map(|f| f.quantity).sum();
    assert!((total - 50_000.0).abs() < 0.01, "total={}", total);
}

#[test]
fn test_ghost_broker_selection() {
    let ghost = GhostCloak::new();
    let b = ghost.pick_broker_for_order(100_000.0);
    assert!(b.is_some(), "should pick broker for $100K");
}

#[test]
fn test_disclosure_layers() {
    let dt = DualTrackRouter::new();
    assert_eq!(dt.disclosure_for_user("regulator", &Track::Compliant) as u8, 2);
    assert_eq!(dt.disclosure_for_user("regulator", &Track::Autonomous) as u8, 2);
    assert_eq!(dt.disclosure_for_user("institution", &Track::Compliant) as u8, 1);
    assert_eq!(dt.disclosure_for_user("institution", &Track::Autonomous) as u8, 3);
    assert_eq!(dt.disclosure_for_user("retail", &Track::Compliant) as u8, 0);
    assert_eq!(dt.disclosure_for_user("retail", &Track::Autonomous) as u8, 3);
}

#[test]
fn test_ghost_fees() {
    assert_eq!(ghost_integration::apply_ghost_fees(10, &Track::Compliant), 10);
    assert_eq!(ghost_integration::apply_ghost_fees(10, &Track::Autonomous), 30);
}

// ===== Async tests: these call .read().await on tokio RwLocks =====

#[tokio::test]
async fn test_ghost_timing() {
    let ghost = GhostCloak::new();
    let t = ghost.get_timing().await;
    assert!(t.base_delay_ms >= 5, "base_delay={}", t.base_delay_ms);
}

#[tokio::test]
async fn test_ghost_strategy() {
    let ghost = GhostCloak::new();
    ghost.set_evasion_strategy(BrokerEvasionStrategy::VolumeMatch).await;
    let s = ghost.get_evasion_strategy().await;
    assert!(matches!(s, BrokerEvasionStrategy::VolumeMatch));
}

#[tokio::test]
async fn test_ghost_snapshot() {
    let ghost = GhostCloak::new();
    let snap = ghost.snapshot().await;
    assert!(snap.get("cloaked_orders").is_some());
    assert!(snap.get("brokers").is_some());
    assert!(snap.get("strategy").is_some());
}

#[tokio::test]
async fn test_bridge_registration() {
    let b = UniversalBridge::new();
    b.register_project("test-settlement", "http://localhost:9999", "test-key",
        vec![Capability::ReceiveSettlements], "test");
    assert_eq!(b.project_count(), 1);
    assert_eq!(b.total_forwarded(), 0);
}

// ===== Performance Benchmarks =====

#[test]
fn benchmark_tps_orderbook_manager() {
    let books = OrderBookManager::new();
    books.create_book("USD/EGP");

    // Pre-load 10K sell orders
    for i in 0..10_000 {
        let o = make_order(OrderSide::Sell, 30.50, 100.0, Track::Compliant);
        books.place_order(o).unwrap();
    }

    // Measure buy order throughput
    let start = std::time::Instant::now();
    let count = 50_000;
    for i in 0..count {
        let o = make_order(OrderSide::Buy, 30.50, 1.0, Track::Compliant);
        books.place_order(o).unwrap();
    }
    let elapsed = start.elapsed();
    let tps = (count as f64 / elapsed.as_secs_f64()) as u64;
    eprintln!("BENCHMARK: OrderBookManager: {} orders in {:?} = {} TPS", count, elapsed, tps);
    assert!(tps > 20_000, "TPS too low: {} < 20K", tps);
}

#[test]
fn benchmark_latency_order_placement() {
    let books = OrderBookManager::new();
    books.create_book("USD/EGP");

    // Pre-load depth
    for i in 0..1000 {
        let o = make_order(OrderSide::Sell, 30.50, 1000.0, Track::Compliant);
        books.place_order(o).unwrap();
    }

    let mut latencies: Vec<std::time::Duration> = Vec::with_capacity(10_000);
    for _ in 0..10_000 {
        let o = make_order(OrderSide::Buy, 30.50, 1.0, Track::Compliant);
        let start = std::time::Instant::now();
        books.place_order(o).unwrap();
        latencies.push(start.elapsed());
    }

    latencies.sort();
    let p50 = latencies[latencies.len() / 2];
    let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];
    let p999 = latencies[(latencies.len() as f64 * 0.999) as usize];
    let avg = latencies.iter().sum::<std::time::Duration>() / latencies.len() as u32;

    eprintln!("BENCHMARK: Order placement latency (10K orders):");
    eprintln!("  Avg:  {:?}", avg);
    eprintln!("  P50:  {:?}", p50);
    eprintln!("  P99:  {:?}", p99);
    eprintln!("  P999: {:?}", p999);

    assert!(p50 < std::time::Duration::from_micros(200), "P50 latency too high: {:?}", p50);
    assert!(p99 < std::time::Duration::from_micros(500), "P99 latency too high: {:?}", p99);
}

#[test]
fn benchmark_smart_router() {
    use smart_router::{SmartOrderRouter, RouteRequest};
    use ghost_integration::BrokerEndpoint;
    let mut router = SmartOrderRouter::new();
    let brokers: Vec<_> = (0..8).map(|i| BrokerEndpoint {
        id: format!("b{}", i),
        name: format!("Broker {}", i),
        url: format!("http://b{}.local", i),
        weight: 0.5 + (i as f64 * 0.05),
        max_order_size: 1_000_000.0 * (i as f64 + 1.0),
        latency_base_us: 50 + (i as u64 * 20),
        is_active: true,
        total_routed: 0,
        last_used: 0,
    }).collect();

    let req = RouteRequest {
        user_id: "bench_user".into(),
        pair: "USD/EGP".into(),
        side: OrderSide::Buy,
        quantity: 500_000.0,
        price: 30.50,
        track: Track::Compliant,
        max_slippage_bps: 10,
        prefer_latency: false,
    };

    let start = std::time::Instant::now();
    let iterations = 100_000;
    for _ in 0..iterations {
        let _ = router.route(&req, &brokers);
    }
    let elapsed = start.elapsed();
    let ops = (iterations as f64 / elapsed.as_secs_f64()) as u64;
    let avg_ns = elapsed.as_nanos() / iterations as u128;

    eprintln!("BENCHMARK: SmartOrderRouter: {} routes in {:?} = {} ops/sec, {} ns/op", iterations, elapsed, ops, avg_ns);
    assert!(ops > 10_000, "SOR too slow: {} ops/sec", ops);
}

#[test]
fn benchmark_ghost_cloak() {
    let ghost = GhostCloak::new();
    let start = std::time::Instant::now();
    let iterations = 100_000;

    for i in 0..iterations {
        ghost.cloak_order(
            &format!("user_{}", i % 1000),
            &format!("order_{}", i),
            "USD/EGP",
            30.50,
            &Track::Autonomous,
        );
    }
    let elapsed = start.elapsed();
    let ops = (iterations as f64 / elapsed.as_secs_f64()) as u64;
    let avg_ns = elapsed.as_nanos() / iterations as u128;

    eprintln!("BENCHMARK: GhostCloak.cloak_order: {} ops in {:?} = {} ops/sec, {} ns/op", iterations, elapsed, ops, avg_ns);
    assert!(ops > 10_000, "Ghost cloak too slow: {} ops/sec", ops);
}
