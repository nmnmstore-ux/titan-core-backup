use crate::orderbook::OrderBook;
use crate::types::*;

pub struct MatchingEngine;

impl MatchingEngine {
    pub fn match_order(
        book: &OrderBook,
        order: &Order,
        check: &dyn Fn(&Order, &Order) -> bool,
    ) -> (Vec<Trade>, f64) {
        book.match_order(order, check)
    }

    #[allow(dead_code)]
    pub fn execute_market(
        book: &OrderBook,
        order: Order,
        check: &dyn Fn(&Order, &Order) -> bool,
    ) -> Result<PlaceOrderResult, String> {
        book.execute_market_internal(order, check)
    }

    pub fn execute_batch_auction(
        _book: &OrderBook,
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
                    id: ID_POOL.next_id(),
                    buy_order_id: buy.id,
                    sell_order_id: sell.id,
                    pair: buy.pair.clone(),
                    price: clearing_price,
                    quantity: fill_qty,
                    total: clearing_price * fill_qty,
                    buy_user_id: buy.user_id,
                    sell_user_id: sell.user_id,
                    timestamp: crate::time_cache::fast_now_ms(),
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

fn find_sucp(buys: &[Order], sells: &[Order]) -> Option<f64> {
    if buys.is_empty() || sells.is_empty() {
        return None;
    }
    if buys[0].price < sells[0].price {
        return None;
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

pub fn compute_jitter_micros(jitter_range_micros: u64, window_number: u64) -> i64 {
    if jitter_range_micros == 0 {
        return 0;
    }
    let tsc = rdtsc();
    let mixed = splitmix64(window_number.wrapping_add(tsc & 0xFFFF));
    let jitter_abs = mixed % (jitter_range_micros as u64 * 2 + 1);
    (jitter_abs as i64).wrapping_sub(jitter_range_micros as i64)
}

fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

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

pub fn actual_window_ns(window_ns: u64, jitter_range_micros: u64, window_number: u64) -> u64 {
    let jitter = compute_jitter_micros(jitter_range_micros, window_number);
    if jitter >= 0 {
        window_ns.wrapping_add((jitter as u64) * 1000)
    } else {
        window_ns.saturating_sub((-jitter) as u64 * 1000)
    }
}
