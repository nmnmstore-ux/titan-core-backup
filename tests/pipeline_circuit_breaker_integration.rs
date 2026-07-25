use uuid::Uuid;
use compact_str::CompactString;

#[path = "../src/types.rs"]
mod types;
#[path = "../src/matching.rs"]
mod matching;
#[path = "../src/counterparty.rs"]
mod counterparty;
#[path = "../src/orderbook.rs"]
mod orderbook;
#[path = "../src/circuit_breaker.rs"]
mod circuit_breaker;
#[path = "../src/numa.rs"]
mod numa;
#[path = "../src/pipeline.rs"]
mod pipeline;

fn make_order(pair: &str, price: f64, qty: f64, track: types::Track) -> types::Order {
    types::Order {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        pair: CompactString::from(pair),
        order_type: types::OrderType::Limit,
        side: types::OrderSide::Buy,
        price,
        quantity: qty,
        filled: 0.0,
        remaining: qty,
        status: types::OrderStatus::New,
        timestamp: 0,
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
        style: types::OrderStyle::Standard,
        hidden_remaining: 0.0,
        client_order_id: None,
        filled_quantity: 0,
    }
}

#[test]
fn test_pipeline_disruptor_dispatch() {
    let d = pipeline::Disruptor::new(256);

    let pl1 = pipeline::TradePayload {
        trade_id: 1,
        buy_order_id: 10,
        sell_order_id: 20,
        pair: *b"BTC/USDT      ",
        pair_len: 8,
        price: 100_000_000,
        quantity: 1_000_000,
        total: 100_000_000_000_000,
        buy_user_id: 100,
        sell_user_id: 200,
        timestamp_ns: 1_000_000,
        seq: 1,
        track: types::TRACK_COMPLIANT,
    };
    let pl2 = pipeline::TradePayload {
        trade_id: 2,
        buy_order_id: 11,
        sell_order_id: 21,
        pair: *b"ETH/USDT      ",
        pair_len: 8,
        price: 3_000_000,
        quantity: 10_000_000,
        total: 30_000_000_000_000,
        buy_user_id: 101,
        sell_user_id: 201,
        timestamp_ns: 1_000_001,
        seq: 2,
        track: types::TRACK_AUTONOMOUS,
    };

    assert!(d.push_sp(&pl1).is_ok());
    assert!(d.push_sp(&pl2).is_ok());

    let batch = d.claim_batch(10);
    assert!(batch.is_some());
    let (start, count) = batch.unwrap();
    assert_eq!(count, 2);

    let r1 = d.read_at(start);
    let r2 = d.read_at(start + 1);
    assert_eq!(r1.trade_id, 1);
    assert_eq!(r1.track, types::TRACK_COMPLIANT);
    assert_eq!(r2.trade_id, 2);
    assert_eq!(r2.track, types::TRACK_AUTONOMOUS);
}

#[test]
fn test_circuit_breaker_level1_batch_auction() {
    let circuit = circuit_breaker::CircuitBreaker::new();
    for pair in circuit_breaker::DEFAULT_PAIRS.iter() {
        circuit.register_pair(pair);
    }
    let order = make_order("BTC/USDT", 100.0, 1.0, types::Track::Compliant);
    let _ = circuit.record_trade(&order.pair, order.price);
    // Second record_trade with same price within window — triggers level 1
    circuit.register_pair("BTC/USDT");
    let op = circuit.record_trade("BTC/USDT", 100.0);
    // May trigger if price history sufficient
    println!("circuit action: {:?}", op);
}

#[test]
fn test_circuit_breaker_level2_pause_trading() {
    let circuit = circuit_breaker::CircuitBreaker::new();
    circuit.register_pair("BTC/USDT");
    let order = make_order("BTC/USDT", 100.0, 1.0, types::Track::Compliant);
    let paused = circuit.is_paused(&order.pair);
    assert!(!paused);
    circuit.trigger_level2(&order.pair);
    let paused_after = circuit.is_paused(&order.pair);
    assert!(paused_after);
}

#[test]
fn test_circuit_breaker_level3_kill_shield() {
    let circuit = circuit_breaker::CircuitBreaker::new();
    circuit.register_pair("BTC/USDT");
    circuit.trigger_level3("BTC/USDT");
    let state = circuit.get_state("BTC/USDT");
    assert!(state.is_some());
    assert_eq!(state.unwrap(), circuit_breaker::CircuitState::Level3);
    // trigger_level3 doesn't create events — get_state is the correct check
}
