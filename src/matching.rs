use crate::orderbook::OrderBook;
use crate::types::*;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use uuid::Uuid;

pub struct MatchingEngine;

impl MatchingEngine {
    pub fn match_order(
        book: &mut OrderBook,
        order: &Order,
        check: &dyn Fn(&Order, &Order) -> bool,
    ) -> (Vec<Trade>, f64) {
        let mut trades = Vec::new();
        let mut remaining = order.quantity;

        while remaining > 0.0 {
            let best_key = match order.side {
                OrderSide::Buy => book.asks.first_key_value().map(|(k, _)| *k),
                OrderSide::Sell => book.bids.last_key_value().map(|(k, _)| *k),
            };
            let key = match best_key {
                Some(k) => k,
                None => break,
            };

            let maker_price = match order.side {
                OrderSide::Buy => book.asks.get(&key).and_then(|l| l.front().map(|o| o.price)),
                OrderSide::Sell => book.bids.get(&key).and_then(|l| l.front().map(|o| o.price)),
            };
            let maker_price = match maker_price {
                Some(p) => p,
                None => {
                    match order.side {
                        OrderSide::Buy => { book.asks.remove(&key); }
                        OrderSide::Sell => { book.bids.remove(&key); }
                    }
                    continue;
                }
            };

            let matched = match order.side {
                OrderSide::Buy => order.price >= maker_price,
                OrderSide::Sell => order.price <= maker_price,
            };
            if !matched {
                break;
            }

            let popped = match order.side {
                OrderSide::Buy => book.asks.get_mut(&key).and_then(|l| l.pop_front()),
                OrderSide::Sell => book.bids.get_mut(&key).and_then(|l| l.pop_front()),
            };

            match popped {
                None => {
                    match order.side {
                        OrderSide::Buy => { book.asks.remove(&key); }
                        OrderSide::Sell => { book.bids.remove(&key); }
                    }
                    continue;
                }
                Some(mut maker) => {
                    if !check(order, &maker) {
                        if maker.remaining > 0.0 {
                            match order.side {
                                OrderSide::Buy => { book.asks.entry(key).or_default().push_back(maker); }
                                OrderSide::Sell => { book.bids.entry(key).or_default().push_back(maker); }
                            }
                        }
                        continue;
                    }

                    if let Some(floor) = maker.hard_floor {
                        let maker_floor_violates = match maker.side {
                            OrderSide::Buy => maker_price > floor,
                            OrderSide::Sell => maker_price < floor,
                        };
                        if maker_floor_violates {
                            if maker.remaining > 0.0 {
                                match order.side {
                                    OrderSide::Buy => { book.asks.entry(key).or_default().push_front(maker); }
                                    OrderSide::Sell => { book.bids.entry(key).or_default().push_front(maker); }
                                }
                            }
                            continue;
                        }
                    }

                    let fill_qty = remaining.min(maker.remaining);
                    let price = maker.price;

                    let trade = Trade {
                        id: Uuid::new_v4(),
                        buy_order_id: if order.side == OrderSide::Buy { order.id } else { maker.id },
                        sell_order_id: if order.side == OrderSide::Sell { order.id } else { maker.id },
                        pair: order.pair.clone(),
                        price,
                        quantity: fill_qty,
                        total: price * fill_qty,
                        buy_user_id: if order.side == OrderSide::Buy { order.user_id } else { maker.user_id },
                        sell_user_id: if order.side == OrderSide::Sell { order.user_id } else { maker.user_id },
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        dot_settled: false,
                        tee_notarized: false,
                    };

                    trades.push(trade);

                    remaining -= fill_qty;
                    maker.remaining -= fill_qty;

                    // Iceberg replenish: if visible slice exhausted but hidden remains
                    if maker.remaining <= 0.0 {
                        if let OrderStyle::Iceberg { display_quantity } = maker.style {
                            if maker.hidden_remaining > 0.0 {
                                let slice = maker.hidden_remaining.min(display_quantity);
                                maker.hidden_remaining -= slice;
                                maker.remaining = slice;
                            }
                        }
                    }

                    if maker.remaining > 0.0 {
                        match order.side {
                            OrderSide::Buy => { book.asks.entry(key).or_default().push_front(maker); }
                            OrderSide::Sell => { book.bids.entry(key).or_default().push_front(maker); }
                        }
                    } else if remaining > 0.0 {
                        match order.side {
                            OrderSide::Buy => { book.asks.remove(&key); }
                            OrderSide::Sell => { book.bids.remove(&key); }
                        }
                    }
                }
            }
        }

        (trades, remaining)
    }

    #[allow(dead_code)]
    pub fn execute_market(
        book: &mut OrderBook,
        mut order: Order,
        check: &dyn Fn(&Order, &Order) -> bool,
    ) -> Result<PlaceOrderResult, String> {
        let mut total_filled = 0.0;
        let mut trades = Vec::new();
        let mut remaining = order.quantity;

        while remaining > 0.0 {
            let best_key = match order.side {
                OrderSide::Buy => book.asks.first_key_value().map(|(k, _)| *k),
                OrderSide::Sell => book.bids.last_key_value().map(|(k, _)| *k),
            };
            let key = match best_key {
                Some(k) => k,
                None => break,
            };

            let popped = match order.side {
                OrderSide::Buy => book.asks.get_mut(&key).and_then(|l| l.pop_front()),
                OrderSide::Sell => book.bids.get_mut(&key).and_then(|l| l.pop_front()),
            };

            match popped {
                None => {
                    match order.side {
                        OrderSide::Buy => { book.asks.remove(&key); }
                        OrderSide::Sell => { book.bids.remove(&key); }
                    }
                    continue;
                }
                Some(mut maker) => {
                    if !check(&order, &maker) {
                        if maker.remaining > 0.0 {
                            match order.side {
                                OrderSide::Buy => { book.asks.entry(key).or_default().push_back(maker); }
                                OrderSide::Sell => { book.bids.entry(key).or_default().push_back(maker); }
                            }
                        }
                        continue;
                    }

                    let fill_qty = remaining.min(maker.remaining);
                    let price = maker.price;

                    if let Some(floor) = order.hard_floor {
                        let violates = match order.side {
                            OrderSide::Buy => price > floor,
                            OrderSide::Sell => price < floor,
                        };
                        if violates { break; }
                    }
                    if let Some(floor) = maker.hard_floor {
                        let violates = match maker.side {
                            OrderSide::Buy => price > floor,
                            OrderSide::Sell => price < floor,
                        };
                        if violates {
                            if maker.remaining > 0.0 {
                                match order.side {
                                    OrderSide::Buy => { book.asks.entry(key).or_default().push_front(maker); }
                                    OrderSide::Sell => { book.bids.entry(key).or_default().push_front(maker); }
                                }
                            }
                            continue;
                        }
                    }

                    let trade = Trade {
                        id: Uuid::new_v4(),
                        buy_order_id: if order.side == OrderSide::Buy { order.id } else { maker.id },
                        sell_order_id: if order.side == OrderSide::Sell { order.id } else { maker.id },
                        pair: order.pair.clone(),
                        price,
                        quantity: fill_qty,
                        total: price * fill_qty,
                        buy_user_id: if order.side == OrderSide::Buy { order.user_id } else { maker.user_id },
                        sell_user_id: if order.side == OrderSide::Sell { order.user_id } else { maker.user_id },
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        dot_settled: false,
                        tee_notarized: false,
                    };

                    trades.push(trade);

                    total_filled += fill_qty;
                    remaining -= fill_qty;
                    order.price = price;
                    maker.remaining -= fill_qty;

                    // Iceberg replenish: if visible slice exhausted but hidden remains
                    if maker.remaining <= 0.0 {
                        if let OrderStyle::Iceberg { display_quantity } = maker.style {
                            if maker.hidden_remaining > 0.0 {
                                let slice = maker.hidden_remaining.min(display_quantity);
                                maker.hidden_remaining -= slice;
                                maker.remaining = slice;
                            }
                        }
                    }

                    if maker.remaining > 0.0 {
                        match order.side {
                            OrderSide::Buy => { book.asks.entry(key).or_default().push_front(maker); }
                            OrderSide::Sell => { book.bids.entry(key).or_default().push_front(maker); }
                        }
                    } else if remaining > 0.0 {
                        match order.side {
                            OrderSide::Buy => { book.asks.remove(&key); }
                            OrderSide::Sell => { book.bids.remove(&key); }
                        }
                    }
                }
            }
        }

        order.filled = total_filled;
        order.remaining = remaining;
        order.status = if remaining <= 0.0 { OrderStatus::Filled } else { OrderStatus::PartiallyFilled };

        Ok(PlaceOrderResult { order, trades, remaining })
    }

    #[allow(dead_code)]
    fn shuffle_by_price_level(orders: &mut Vec<Order>, seed: [u8; 32]) -> Vec<Order> {
        if orders.is_empty() { return Vec::new(); }
        orders.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap_or(std::cmp::Ordering::Equal));

        let mut groups: Vec<Vec<Order>> = Vec::new();
        let mut current_group: Vec<Order> = Vec::new();
        if let Some(first) = orders.first() {
            let first_key = crate::orderbook::price_key(first.price);
            let mut current_key = first_key;
            let sorted = std::mem::take(orders);
            for o in sorted {
                let pk = crate::orderbook::price_key(o.price);
                if pk == current_key {
                    current_group.push(o);
                } else {
                    groups.push(std::mem::take(&mut current_group));
                    current_group.push(o);
                    current_key = pk;
                }
            }
            if !current_group.is_empty() {
                groups.push(current_group);
            }
        }

        let mut rng = StdRng::from_seed(seed);
        for group in &mut groups {
            group.shuffle(&mut rng);
        }

        groups.into_iter().flatten().collect()
    }

    #[allow(dead_code)]
    pub fn execute_batch_auction(
        _book: &mut OrderBook,
        orders: &mut Vec<Order>,
        window_number: u64,
        check: &dyn Fn(&Order, &Order) -> bool,
        _jitter_range_micros: u64,
    ) -> (Vec<Trade>, Vec<Order>) {
        let _ = window_number;
        let mut all_trades: Vec<Trade> = Vec::new();

        let buy_orders: Vec<Order> = orders.iter().filter(|o| o.side == OrderSide::Buy).cloned().collect();
        let sell_orders: Vec<Order> = orders.iter().filter(|o| o.side == OrderSide::Sell).cloned().collect();
        orders.clear();

        if buy_orders.is_empty() || sell_orders.is_empty() {
            orders.extend(buy_orders);
            orders.extend(sell_orders);
            return (all_trades, orders.clone());
        }

        let clearing_price = match find_sucp(&buy_orders, &sell_orders) {
            Some(p) => p,
            None => {
                orders.extend(buy_orders);
                orders.extend(sell_orders);
                return (all_trades, orders.clone());
            }
        };

        let mut matched = Vec::new();
        for (i, buy) in buy_orders.iter().enumerate() {
            if buy.remaining <= 0.0 || buy.price < clearing_price { continue; }
            for (j, sell) in sell_orders.iter().enumerate() {
                if sell.quantity <= 0.0 || sell.price > clearing_price { continue; }
                if buy.remaining <= 0.0 { break; }
                if !check(buy, sell) { continue; }

                if let Some(floor) = buy.hard_floor {
                    if clearing_price > floor { continue; }
                }
                if let Some(floor) = sell.hard_floor {
                    if clearing_price < floor { continue; }
                }

                let fill_qty = buy.remaining.min(sell.quantity);
                let trade = Trade {
                    id: Uuid::new_v4(),
                    buy_order_id: buy.id,
                    sell_order_id: sell.id,
                    pair: buy.pair.clone(),
                    price: clearing_price,
                    quantity: fill_qty,
                    total: clearing_price * fill_qty,
                    buy_user_id: buy.user_id,
                    sell_user_id: sell.user_id,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    dot_settled: false,
                    tee_notarized: false,
                };
                all_trades.push(trade);
                matched.push((i, j, fill_qty));
            }
        }

        for (bi, _, fill) in &matched {
            if let Some(b) = buy_orders.get(*bi) {
                let rem = b.remaining - fill;
                if rem > 0.0 {
                    orders.push(b.clone());
                }
            }
        }
        for (_, si, fill) in &matched {
            if let Some(s) = sell_orders.get(*si) {
                let rem = s.quantity - fill;
                if rem > 0.0 {
                    orders.push(s.clone());
                }
            }
        }

        (all_trades, orders.clone())
    }
}

#[allow(dead_code)]
fn find_sucp(buys: &[Order], sells: &[Order]) -> Option<f64> {
    if buys.is_empty() || sells.is_empty() {
        return None;
    }
    if buys[0].price < sells[0].price {
        return None; // Highest buy below lowest sell — no overlap
    }

    let mut cum_buy = 0.0;
    let mut cum_sell = 0.0;
    let mut b = 0;
    let mut s = 0;

    loop {
        let buy_price = buys.get(b).map(|o| o.price).unwrap_or(0.0);
        let sell_price = sells.get(s).map(|o| o.price).unwrap_or(f64::MAX);

        if buy_price < sell_price {
            break;
        }

        let cur_buy_price = buy_price;
        while b < buys.len() && (buys[b].price - cur_buy_price).abs() < 0.000001 {
            cum_buy += buys[b].remaining;
            b += 1;
        }

        let cur_sell_price = sell_price;
        while s < sells.len() && (sells[s].price - cur_sell_price).abs() < 0.000001 {
            cum_sell += sells[s].quantity;
            s += 1;
        }

        if cum_buy >= cum_sell && cum_buy > 0.0 && cum_sell > 0.0 {
            return Some((cur_buy_price + cur_sell_price) / 2.0);
        }

        if b >= buys.len() || s >= sells.len() {
            break;
        }
    }

    let total_buy: f64 = buys.iter().map(|o| o.remaining).sum();
    let total_sell: f64 = sells.iter().map(|o| o.quantity).sum();
    if total_buy > 0.0 && total_sell > 0.0 {
        let last_buy = buys.last().map(|o| o.price).unwrap_or(0.0);
        let last_sell = sells.last().map(|o| o.price).unwrap_or(0.0);
        Some((last_buy + last_sell) / 2.0)
    } else {
        None
    }
}

/// Anti-sniping jitter: returns microsecond offset in [-jitter_range, +jitter_range].
pub fn compute_jitter_micros(jitter_range_micros: u64, window_number: u64) -> i64 {
    if jitter_range_micros == 0 {
        return 0;
    }
    let tsc = rdtsc();
    let mixed = splitmix64(window_number.wrapping_add(tsc & 0xFFFF));
    let jitter_abs = mixed % (jitter_range_micros as u64 * 2 + 1);
    (jitter_abs as i64).wrapping_sub(jitter_range_micros as i64)
}

/// Single-cycle avalanche mixer (splitmix64).
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// CPU cycle counter — sub-nanosecond resolution, zero-lock.
#[cfg(target_arch = "x86_64")]
fn rdtsc() -> u64 {
    unsafe { std::arch::x86_64::_rdtsc() }
}

#[cfg(not(target_arch = "x86_64"))]
fn rdtsc() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

/// Compute the actual window ns including anti-sniping jitter.
pub fn actual_window_ns(window_ns: u64, jitter_range_micros: u64, window_number: u64) -> u64 {
    let jitter = compute_jitter_micros(jitter_range_micros, window_number);
    if jitter >= 0 {
        window_ns.wrapping_add((jitter as u64) * 1000)
    } else {
        window_ns.saturating_sub((-jitter) as u64 * 1000)
    }
}
