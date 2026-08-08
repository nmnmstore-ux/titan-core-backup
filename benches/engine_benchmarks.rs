use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::sync::Arc;
use tokio::runtime::Runtime;

fn bench_order_placement(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let book = Arc::new(the_bridge_matching_engine::orderbook::OrderBookManager::new());
    book.create_book("ETH/USDC");

    let mut group = c.benchmark_group("order_placement");
    for batch_size in [1, 10, 100, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(batch_size), &batch_size, |b, &size| {
            b.iter(|| {
                rt.block_on(async {
                    for i in 0..size {
                        let order = the_bridge_matching_engine::types::Order {
                            id: uuid::Uuid::new_v4(),
                            id_tag: 0,
                            user_id: uuid::Uuid::new_v4(),
                            pair: "ETH/USDC".into(),
                            side: if i % 2 == 0 { the_bridge_matching_engine::types::OrderSide::Buy } else { the_bridge_matching_engine::types::OrderSide::Sell },
                            order_type: the_bridge_matching_engine::types::OrderType::Limit,
                            price: 3000.0 + (i as f64 * 0.1),
                            quantity: 1.0,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as i64,
                            ..Default::default()
                        };
                        let _ = book.place_order(order);
                    }
                });
            });
        });
    }
    group.finish();
}

fn bench_matching_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let book = Arc::new(the_bridge_matching_engine::orderbook::OrderBookManager::new());
    book.create_book("ETH/USDC");

    // Pre-populate book with liquidity
    rt.block_on(async {
        for i in 0..100 {
            let bid = the_bridge_matching_engine::types::Order {
                id: uuid::Uuid::new_v4(),
                id_tag: 0,
                user_id: uuid::Uuid::new_v4(),
                pair: "ETH/USDC".into(),
                side: the_bridge_matching_engine::types::OrderSide::Buy,
                order_type: the_bridge_matching_engine::types::OrderType::Limit,
                price: 2990.0 + i as f64,
                quantity: 10.0,
                timestamp: 0,
                ..Default::default()
            };
            let ask = the_bridge_matching_engine::types::Order {
                id: uuid::Uuid::new_v4(),
                id_tag: 0,
                user_id: uuid::Uuid::new_v4(),
                pair: "ETH/USDC".into(),
                side: the_bridge_matching_engine::types::OrderSide::Sell,
                order_type: the_bridge_matching_engine::types::OrderType::Limit,
                price: 3010.0 - i as f64,
                quantity: 10.0,
                timestamp: 0,
                ..Default::default()
            };
            let _ = book.place_order(bid);
            let _ = book.place_order(ask);
        }
    });

    let mut group = c.benchmark_group("matching_latency");
    group.bench_function("market_order_cross", |b| {
        b.iter(|| {
            rt.block_on(async {
                let order = the_bridge_matching_engine::types::Order {
                    id: uuid::Uuid::new_v4(),
                    id_tag: 0,
                    user_id: uuid::Uuid::new_v4(),
                    pair: "ETH/USDC".into(),
                    side: the_bridge_matching_engine::types::OrderSide::Buy,
                    order_type: the_bridge_matching_engine::types::OrderType::Market,
                    price: 3020.0,
                    quantity: 1.0,
                    timestamp: 0,
                    ..Default::default()
                };
                let _ = book.place_order(order);
            });
        });
    });
    group.finish();
}

fn bench_concurrent_orders(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let book = Arc::new(the_bridge_matching_engine::orderbook::OrderBookManager::new());
    book.create_book("ETH/USDC");

    let mut group = c.benchmark_group("concurrent_orders");
    group.bench_function("100_concurrent_writes", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut handles = Vec::new();
                for i in 0..100 {
                    let book = book.clone();
                    handles.push(tokio::spawn(async move {
                        let order = the_bridge_matching_engine::types::Order {
                            id: uuid::Uuid::new_v4(),
                            id_tag: 0,
                            user_id: uuid::Uuid::new_v4(),
                            pair: "ETH/USDC".into(),
                            side: if i % 2 == 0 { the_bridge_matching_engine::types::OrderSide::Buy } else { the_bridge_matching_engine::types::OrderSide::Sell },
                            order_type: the_bridge_matching_engine::types::OrderType::Limit,
                            price: 3000.0 + (i as f64 * 0.01),
                            quantity: 0.1,
                            timestamp: 0,
                            ..Default::default()
                        };
                        let _ = book.place_order(order);
                    }));
                }
                for h in handles { let _ = h.await; }
            });
        });
    });
    group.finish();
}

criterion_group!(benches, bench_order_placement, bench_matching_latency, bench_concurrent_orders);
criterion_main!(benches);
