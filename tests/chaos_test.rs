// ============================================================
// Chaos Engineering Test Suite — THE-BRIDGE
// Tests: WAL corruption, crash recovery, network partition,
//        stop-loss persistence, order book consistency
// ============================================================

use std::sync::Arc;
use std::fs;
use uuid::Uuid;
use compact_str::CompactString;

#[path = "../src/types.rs"]
mod types;
#[path = "../src/io.rs"]
mod io;
#[path = "../src/orderbook.rs"]
mod orderbook;
#[path = "../src/matching.rs"]
mod matching;
#[path = "../src/wal.rs"]
mod wal;
#[path = "../src/consensus.rs"]
mod consensus;
#[path = "../src/crdt.rs"]
mod crdt;
#[path = "../src/counterparty.rs"]
mod counterparty;
#[path = "../src/snapshot.rs"]
mod snapshot;

use types::*;
use orderbook::OrderBookManager;

fn make_order(pair: &str, price: f64, qty: f64) -> Order {
    let uid = Uuid::new_v4();
    Order::new_limit(uid, pair.to_string(), OrderSide::Buy, price, qty)
}

fn make_sell(pair: &str, price: f64, qty: f64) -> Order {
    let uid = Uuid::new_v4();
    Order::new_limit(uid, pair.to_string(), OrderSide::Sell, price, qty)
}

// ============================================================
// Test 1: WAL Corruption — engine must recover gracefully
// ============================================================
#[test]
fn test_wal_corruption_recovery() {
    let dir = std::env::temp_dir().join(format!("wal_corrupt_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // Create WAL, write some entries
    let node_id = "chaos-test-1";
    {
        let wal = wal::WriteAheadLog::new(node_id, &dir, vec![]).unwrap();
        let order = make_order("USD/EGP", 30.50, 100.0);
        wal.append(wal::WALRecord::PlaceOrder(order.clone())).unwrap();
        let order2 = make_order("USD/EGP", 30.60, 200.0);
        wal.append(wal::WALRecord::PlaceOrder(order2)).unwrap();
        wal.append(wal::WALRecord::CancelOrder(Uuid::new_v4())).unwrap();
    }

    // Corrupt the WAL file by writing garbage at the end
    let wal_file = dir.join(format!("{}.wal", node_id));
    if wal_file.exists() {
        let mut data = fs::read(&wal_file).unwrap();
        // Append garbage bytes to simulate corruption
        data.extend_from_slice(b"CORRUPTED_DATA_AT_END");
        fs::write(&wal_file, data).unwrap();
    }

    // Recovery must succeed (should truncate at last valid entry or skip corruption)
    let wal2 = wal::WriteAheadLog::new(node_id, &dir, vec![]).unwrap();
    let recovered = wal2.recover().unwrap();
    // At minimum the first 2 valid records should survive (third may be lost due to corruption)
    assert!(recovered.len() >= 2, "Expected at least 2 recovered entries, got {}", recovered.len());

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================
// Test 2: Crash Recovery — simulate restart after orders
// ============================================================
#[test]
fn test_crash_recovery_order_book_consistency() {
    let dir = std::env::temp_dir().join(format!("crash_recovery_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let pair = "USD/EGP";
    let node_id = "crash-test";

    // Phase 1: Place orders before "crash"
    let order_count = 50;
    {
        let books = OrderBookManager::new();
        books.create_book(pair);
        let wal = wal::WriteAheadLog::new(node_id, &dir, vec![]).unwrap();
        for i in 0..order_count {
            let price = 30.00 + (i as f64 * 0.01);
            let order = make_order(pair, price, 100.0);
            wal.append(wal::WALRecord::PlaceOrder(order.clone())).unwrap();
            let _ = books.place_order(order);
        }
        for i in 0..order_count/2 {
            let price = 31.00 + (i as f64 * 0.01);
            let order = make_sell(pair, price, 100.0);
            wal.append(wal::WALRecord::PlaceOrder(order.clone())).unwrap();
            let _ = books.place_order(order);
        }
        // WAL is dropped/synced — "crash" happens here
    }

    // Phase 2: "Restart" — create fresh components and replay WAL
    let books2 = OrderBookManager::new();
    books2.create_book(pair);
    let wal2 = wal::WriteAheadLog::new(node_id, &dir, vec![]).unwrap();
    let recovered = wal2.recover().unwrap();

    let mut _replayed_bids = 0u64;
    let mut _replayed_asks = 0u64;
    for record in &recovered {
        match record {
            wal::WALRecord::PlaceOrder(order) => {
                if books2.place_order(order.clone()).is_ok() {
                    if order.side == OrderSide::Buy {
                        _replayed_bids += 1;
                    } else {
                        _replayed_asks += 1;
                    }
                }
            }
            wal::WALRecord::CancelOrder(id) => {
                let _ = books2.cancel_order(*id);
            }
            _ => {}
        }
    }

    // Verify order book is consistent after replay
    let summary = books2.get_book_summary(pair).unwrap();
    assert!(summary.bid_count > 0, "Expected bids after replay");
    assert!(summary.ask_count > 0, "Expected asks after replay");
    assert!(summary.last_price > 0.0 || summary.bid_count > 0, "Expected price activity");

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================
// Test 3: StopLoss Order Validation
// ============================================================
#[test]
fn test_stop_loss_validation() {
    let pair = "USD/EGP";
    let user = Uuid::new_v4();

    // Verify stop-loss order is constructed correctly
    let sl = Order::new_stop_loss(user, pair.to_string(), OrderSide::Buy, 100.0, 29.50, None);
    assert_eq!(sl.style, OrderStyle::StopLoss { trigger_price: 29.50, limit_price: None });
    assert_eq!(sl.side, OrderSide::Buy);
    assert_eq!(sl.quantity, 100.0);

    // Verify iceberg has correct hidden_remaining
    let ib = Order::new_iceberg(user, pair.to_string(), OrderSide::Buy, 30.50, 1000.0, 100.0);
    assert_eq!(ib.remaining, 100.0, "visible = display_qty");
    assert_eq!(ib.hidden_remaining, 900.0, "hidden = total - visible");

    // Verify TWAP stores style
    let twap = Order {
        id: Uuid::new_v4(),
        user_id: user,
        pair: CompactString::from(pair),
        order_type: OrderType::Limit,
        side: OrderSide::Buy,
        price: 30.50,
        quantity: 1000.0,
        filled: 0.0,
        remaining: 1000.0,
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
        style: OrderStyle::TWAP { duration_secs: 60, interval_secs: 10 },
        hidden_remaining: 0.0,
        client_order_id: None,
        filled_quantity: 0,
    };
    assert_eq!(twap.style, OrderStyle::TWAP { duration_secs: 60, interval_secs: 10 });
}

// ============================================================
// Test 4: Network Partition — Consensus Still Operates Locally
// ============================================================
#[test]
fn test_consensus_network_partition() {
    let dir = std::env::temp_dir().join(format!("consensus_partition_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let node_id = "partition-node";
    let sk_bytes: [u8; 32] = [1u8; 32];
    // DAG runs with empty peer list (simulates partitioned node)
    let consensus = Arc::new(consensus::DAGConsensus::new(
        node_id,
        vec![],  // empty peers = partitioned
        &sk_bytes,
    ));

    // Even without peers, submit should not panic
    let order = make_order("USD/EGP", 30.50, 100.0);
    let op = consensus::ConsensusOp::PlaceOrder(order);
    futures::executor::block_on(consensus.submit(op));

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================
// Test 5: High Volume Iceberg Replenishment
// ============================================================
#[test]
fn test_iceberg_replenishment_loop() {
    let books = OrderBookManager::new();
    let pair = "USD/EGP";
    books.create_book(pair);

    // Place an Iceberg buy order: total 1000, display 100
    let user = Uuid::new_v4();
    let iceberg = Order::new_iceberg(user, pair.to_string(), OrderSide::Buy, 30.50, 1000.0, 100.0);
    assert_eq!(iceberg.remaining, 100.0, "Iceberg: visible = display_quantity");
    assert_eq!(iceberg.hidden_remaining, 900.0, "Iceberg: hidden = total - visible");

    // Place sell orders to match against the iceberg
    for _ in 0..15 {
        let seller = Uuid::new_v4();
        let price = 30.50;
        let sell = Order::new_limit(seller, pair.to_string(), OrderSide::Sell, price, 100.0);
        let result = books.place_order(sell).unwrap();
        if !result.trades.is_empty() {
            // Every 100 filled should replenish 100 more from hidden
            let remaining = books.get_order(iceberg.id);
            if let Some(remaining) = remaining {
                assert!(remaining.hidden_remaining < 900.0 || remaining.remaining > 0.0,
                        "Iceberg should replenish after visible slice is consumed");
            }
        }
    }
}

// ============================================================
// Test 6: TWAP Order Lifecycle
// ============================================================
#[test]
fn test_twap_order_lifecycle() {
    let books = OrderBookManager::new();
    let pair = "USD/EGP";
    books.create_book(pair);

    // Place a TWAP order
    let user = Uuid::new_v4();
    let twap = Order {
        id: Uuid::new_v4(),
        user_id: user,
        pair: CompactString::from(pair),
        order_type: OrderType::Limit,
        side: OrderSide::Buy,
        price: 30.50,
        quantity: 1000.0,
        filled: 0.0,
        remaining: 1000.0,
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
        style: OrderStyle::TWAP { duration_secs: 60, interval_secs: 10 },
        hidden_remaining: 0.0,
        client_order_id: None,
        filled_quantity: 0,
    };

    let result = books.place_order(twap.clone()).unwrap();
    assert!(result.trades.is_empty(), "TWAP should not trade immediately");

    // Should be in twap_orders map
    assert!(books.twap_orders.contains_key(&twap.id), "TWAP should be stored");

    // Process TWAP — should produce slices
    let now = chrono::Utc::now().timestamp_millis() + 11_000; // 11 seconds later (past first interval)
    let slices = books.process_twap(now);
    assert!(!slices.is_empty(), "TWAP should produce slices after interval elapsed");

    // Cancel the TWAP
    books.cancel_order(twap.id).unwrap();
    assert!(!books.twap_orders.contains_key(&twap.id), "TWAP should be removed after cancel");
}

// ============================================================
// Test 7: StopLoss Trigger After Trade
// ============================================================
#[test]
fn test_stop_loss_trigger_on_trade() {
    let books = OrderBookManager::new();
    let pair = "USD/EGP";
    books.create_book(pair);

    // Place a sell order at 30.00 (creates liquidity)
    let seller = Uuid::new_v4();
    let sell = Order::new_limit(seller, pair.to_string(), OrderSide::Sell, 30.00, 50.0);
    let _ = books.place_order(sell);

    // Place a BUY stop-loss: trigger when price RISES to 30.50
    let user = Uuid::new_v4();
    let sl = Order::new_stop_loss(user, pair.to_string(), OrderSide::Buy, 100.0, 30.50, None);
    let _ = books.place_order(sl);

    // Check at price below trigger — should NOT trigger (buy stop triggers on rise)
    let triggered = books.check_stop_losses(pair, 30.00);
    assert!(triggered.is_empty(), "Buy stop should not trigger at price below trigger");

    // Check at price at/above trigger — SHOULD trigger
    let triggered = books.check_stop_losses(pair, 30.50);
    assert!(!triggered.is_empty(), "Buy stop should trigger when price reaches trigger");
    assert_eq!(triggered[0].style, OrderStyle::Standard, "Triggered stop-loss should become standard order");

    // Now test SELL stop-loss: trigger when price FALLS to 25.00
    let sl2 = Order::new_stop_loss(user, pair.to_string(), OrderSide::Sell, 100.0, 25.00, None);
    let _ = books.place_order(sl2);

    // Above trigger — should NOT trigger
    let triggered = books.check_stop_losses(pair, 26.00);
    assert!(triggered.is_empty(), "Sell stop should not trigger at price above trigger");

    // At/below trigger — SHOULD trigger
    let triggered = books.check_stop_losses(pair, 25.00);
    assert!(!triggered.is_empty(), "Sell stop should trigger when price falls to trigger");
}

// ============================================================
// Test 8: OrderBook Manager Advanced Order Operations
// ============================================================
#[test]
fn test_orderbook_manager_advanced_orders() {
    let books = OrderBookManager::new();
    let pair = "USD/EGP";
    books.create_book(pair);

    let user = Uuid::new_v4();

    // Test StopLoss operations
    let sl = Order::new_stop_loss(user, pair.to_string(), OrderSide::Buy, 100.0, 31.00, None);
    let sl_id = sl.id;
    let result = books.place_order(sl).unwrap();
    assert!(result.trades.is_empty(), "StopLoss should not trade immediately");

    // Verify it's in stop_losses map
    assert!(books.stop_losses.get(pair).map_or(false, |v| v.iter().any(|o| o.id == sl_id)), "StopLoss should be stored");

    // Check at price below trigger — should NOT trigger
    let triggered = books.check_stop_losses(pair, 30.00);
    assert!(triggered.is_empty(), "Buy stop should not trigger at price below trigger");

    // Test ICEBERG visibility
    let ib = Order::new_iceberg(user, pair.to_string(), OrderSide::Buy, 30.50, 1000.0, 100.0);
    let mut result = books.place_order(ib).unwrap();
    assert!(result.trades.is_empty(), "Iceberg should not trade immediately");

    // Place matching sell
    let sell2 = Order::new_limit(Uuid::new_v4(), pair.to_string(), OrderSide::Sell, 30.50, 100.0);
    result = books.place_order(sell2).unwrap();
    assert!(!result.trades.is_empty(), "Iceberg should trade with sell");
    // After trade, iceberg should still be in book (replenished)
    assert!(books.get_book_summary(pair).unwrap().bid_count > 0, "Iceberg should replenish");

    // Test TWAP lifecycle
    let twap = Order {
        id: Uuid::new_v4(),
        user_id: user,
        pair: CompactString::from(pair),
        order_type: OrderType::Limit,
        side: OrderSide::Buy,
        price: 30.50,
        quantity: 500.0,
        filled: 0.0,
        remaining: 500.0,
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
        style: OrderStyle::TWAP { duration_secs: 30, interval_secs: 5 },
        hidden_remaining: 0.0,
        client_order_id: None,
        filled_quantity: 0,
    };

    let _result = books.place_order(twap.clone()).unwrap();
    assert!(books.twap_orders.contains_key(&twap.id), "TWAP should be stored");

    // Process TWAP
    let now = chrono::Utc::now().timestamp_millis() + 6_000;
    let slices = books.process_twap(now);
    assert!(!slices.is_empty(), "TWAP should produce slices");

    // Cancel TWAP
    books.cancel_order(twap.id).unwrap();
    assert!(!books.twap_orders.contains_key(&twap.id), "TWAP should be removed");
}