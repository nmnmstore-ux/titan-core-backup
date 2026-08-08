use crate::counterparty::CounterpartyVisibilityStore;
use crate::matching::MatchingEngine;
use crate::types::*;
use compact_str::CompactString;
use dashmap::DashMap;

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::Instant;
use tracing::info;
use uuid::Uuid;

pub const PRICE_MULTIPLIER: i64 = 10_000;

pub fn price_key(price: f64) -> i64 {
    (price * PRICE_MULTIPLIER as f64).round() as i64
}

pub const ORDER_BOOK_SHARDS: usize = 16;
const SHARD_MASK: usize = ORDER_BOOK_SHARDS - 1;

fn shard_index(id: &Uuid) -> usize {
    (id.as_u128() as u64 as usize) & SHARD_MASK
}

fn store_f64(atomic: &AtomicU64, val: f64) {
    atomic.store(val.to_bits(), AtomicOrdering::Relaxed);
}

fn load_f64(atomic: &AtomicU64) -> f64 {
    f64::from_bits(atomic.load(AtomicOrdering::Relaxed))
}

fn fetch_add_f64(atomic: &AtomicU64, val: f64) -> f64 {
    loop {
        let cur = atomic.load(AtomicOrdering::Relaxed);
        let cur_f = f64::from_bits(cur);
        let new_f = cur_f + val;
        if atomic.compare_exchange_weak(cur, new_f.to_bits(), AtomicOrdering::Relaxed, AtomicOrdering::Relaxed).is_ok() {
            return new_f;
        }
    }
}

fn fetch_max_f64(atomic: &AtomicU64, val: f64) {
    let bits = val.to_bits();
    loop {
        let cur = atomic.load(AtomicOrdering::Relaxed);
        if bits <= cur { break; }
        if atomic.compare_exchange_weak(cur, bits, AtomicOrdering::Relaxed, AtomicOrdering::Relaxed).is_ok() { break; }
    }
}

fn fetch_min_f64(atomic: &AtomicU64, val: f64) {
    let bits = val.to_bits();
    loop {
        let cur = atomic.load(AtomicOrdering::Relaxed);
        if bits >= cur { break; }
        if atomic.compare_exchange_weak(cur, bits, AtomicOrdering::Relaxed, AtomicOrdering::Relaxed).is_ok() { break; }
    }
}

struct OrderBookShard {
    bids: parking_lot::RwLock<BTreeMap<i64, VecDeque<Order>>>,
    asks: parking_lot::RwLock<BTreeMap<i64, VecDeque<Order>>>,
}

impl OrderBookShard {
    fn new() -> Self {
        Self {
            bids: parking_lot::RwLock::new(BTreeMap::new()),
            asks: parking_lot::RwLock::new(BTreeMap::new()),
        }
    }
}

struct BatchAuctionState {
    orders: Vec<Order>,
    deadline: Instant,
    window_ns: u64,
    window_number: u64,
    jitter_range_micros: u64,
}

impl BatchAuctionState {
    fn new(window_ns: u64, jitter_range_micros: u64) -> Self {
        Self {
            orders: Vec::with_capacity(1024),
            deadline: Self::compute_deadline(window_ns, jitter_range_micros, 0),
            window_ns,
            window_number: 0,
            jitter_range_micros,
        }
    }

    fn compute_deadline(base_ns: u64, jitter_range: u64, window_num: u64) -> Instant {
        if jitter_range == 0 {
            return Instant::now() + std::time::Duration::from_nanos(base_ns);
        }
        let effective = crate::matching::actual_window_ns(base_ns, jitter_range, window_num);
        Instant::now() + std::time::Duration::from_nanos(effective)
    }

    fn reset(&mut self) {
        self.deadline = Self::compute_deadline(self.window_ns, self.jitter_range_micros, self.window_number);
    }
}

#[allow(dead_code)]
pub struct OrderBookManager {
    pub(crate) books: DashMap<String, OrderBook>,
    total_orders: AtomicU64,
    total_trades: AtomicU64,
    tps_count: AtomicU64,
    tps_peak: AtomicU64,
    matching: MatchingEngine,
    matching_mode: parking_lot::RwLock<MatchingMode>,
    auction_state: DashMap<String, BatchAuctionState>,
    pub counterparty_visibility: Option<Arc<CounterpartyVisibilityStore>>,
    pub stop_losses: DashMap<String, Vec<Order>>,
    pub twap_orders: DashMap<Uuid, TwapState>,
    pub user_orders: DashMap<Uuid, Vec<Order>>,
    pub user_trades: DashMap<Uuid, Vec<Trade>>,
}

/// Per-pair sharded order book.
pub struct OrderBook {
    pub pair: CompactString,
    shards: Vec<OrderBookShard>,
    pub trades: parking_lot::Mutex<Vec<Trade>>,
    pub trades_enabled: AtomicBool,
    last_price: AtomicU64,
    high_24h: AtomicU64,
    low_24h: AtomicU64,
    volume_24h: AtomicU64,
    open_24h: AtomicU64,
}

impl OrderBook {
    pub fn new(pair: &str) -> Self {
        let mut shards = Vec::with_capacity(ORDER_BOOK_SHARDS);
        for _ in 0..ORDER_BOOK_SHARDS {
            shards.push(OrderBookShard::new());
        }
        Self {
            pair: CompactString::from(pair),
            shards,
            trades: parking_lot::Mutex::new(Vec::with_capacity(4096)),
            trades_enabled: AtomicBool::new(false),
            last_price: AtomicU64::new(0u64.to_le()),
            high_24h: AtomicU64::new(0u64.to_le()),
            low_24h: AtomicU64::new(f64::MAX.to_bits()),
            volume_24h: AtomicU64::new(0u64.to_le()),
            open_24h: AtomicU64::new(0u64.to_le()),
        }
    }

    pub fn get_last_price(&self) -> f64 { load_f64(&self.last_price) }
    pub fn get_high_24h(&self) -> f64 { load_f64(&self.high_24h) }
    pub fn get_low_24h(&self) -> f64 { load_f64(&self.low_24h) }
    pub fn get_volume_24h(&self) -> f64 { load_f64(&self.volume_24h) }
    pub fn get_open_24h(&self) -> f64 { load_f64(&self.open_24h) }

    fn update_aggregates(&self, price: f64, qty: f64) {
        store_f64(&self.last_price, price);
        fetch_max_f64(&self.high_24h, price);
        fetch_min_f64(&self.low_24h, price);
        fetch_add_f64(&self.volume_24h, qty);
        let cur = self.open_24h.load(AtomicOrdering::Relaxed);
        if cur == 0u64 || f64::from_bits(cur) == 0.0 {
            let _ = self.open_24h.compare_exchange(cur, price.to_bits(), AtomicOrdering::Relaxed, AtomicOrdering::Relaxed);
        }
    }

    pub fn insert_bid(&self, price_key: i64, order: Order) {
        let idx = shard_index(&order.id);
        self.shards[idx].bids.write().entry(price_key).or_default().push_back(order);
    }

    pub fn insert_ask(&self, price_key: i64, order: Order) {
        let idx = shard_index(&order.id);
        self.shards[idx].asks.write().entry(price_key).or_default().push_back(order);
    }

    pub fn match_order(&self, order: &Order, check: &dyn Fn(&Order, &Order) -> bool) -> (Vec<Trade>, f64) {
        let mut trades: Vec<Trade> = Vec::new();
        let mut remaining = order.quantity;

        match order.side {
            OrderSide::Buy => {
                let mut ask_guards: Vec<_> = self.shards.iter().map(|s| s.asks.write()).collect();
                Self::execute_match_loop(&mut ask_guards, order, check, &mut trades, &mut remaining, true);
                drop(ask_guards);
            }
            OrderSide::Sell => {
                let mut bid_guards: Vec<_> = self.shards.iter().map(|s| s.bids.write()).collect();
                Self::execute_match_loop(&mut bid_guards, order, check, &mut trades, &mut remaining, false);
                drop(bid_guards);
            }
        }

        if let Some(t) = trades.last() {
            let total_qty: f64 = trades.iter().map(|t| t.quantity).sum();
            self.update_aggregates(t.price, total_qty);
            if self.trades_enabled.load(AtomicOrdering::Relaxed) {
                let mut ts = self.trades.lock();
                for t in &trades { ts.push(t.clone()); }
            }
        }

        (trades, remaining)
    }

    fn execute_match_loop(
        guards: &mut [parking_lot::RwLockWriteGuard<BTreeMap<i64, VecDeque<Order>>>],
        order: &Order,
        check: &dyn Fn(&Order, &Order) -> bool,
        trades: &mut Vec<Trade>,
        remaining: &mut f64,
        is_buy: bool,
    ) {
        while *remaining > 0.0 {
            let best_key = if is_buy {
                guards.iter().filter_map(|g| g.first_key_value().map(|(k, _)| *k)).min()
            } else {
                guards.iter().filter_map(|g| g.last_key_value().map(|(k, _)| *k)).max()
            };

            let key = match best_key { Some(k) => k, None => break };

            let maker_price = guards.iter()
                .find_map(|g| g.get(&key).and_then(|l| l.front().map(|o| o.price)))
                .unwrap_or(0.0);

            let matched = if is_buy { order.price >= maker_price } else { order.price <= maker_price };
            if !matched { break; }

            let mut maker = None;
            let mut maker_shard = 0;

            for (si, guard) in guards.iter_mut().enumerate() {
                if let Some(orders) = guard.get_mut(&key) {
                    let mut temp: VecDeque<Order> = VecDeque::new();
                    while let Some(front) = orders.pop_front() {
                        if check(order, &front) { maker = Some(front); maker_shard = si; break; }
                        temp.push_back(front);
                    }
                    while let Some(o) = temp.pop_front() { orders.push_back(o); }
                    if maker.is_some() {
                        if orders.is_empty() { guard.remove(&key); }
                        break;
                    }
                    if orders.is_empty() { guard.remove(&key); }
                }
            }

            let mut maker = match maker {
                Some(m) => m,
                None => {
                    for guard in guards.iter_mut() { guard.remove(&key); }
                    continue;
                }
            };

            if let Some(floor) = maker.hard_floor {
                let violates = match maker.side {
                    OrderSide::Buy => maker_price > floor,
                    OrderSide::Sell => maker_price < floor,
                };
                if violates {
                    if maker.remaining > 0.0 {
                        let si = shard_index(&maker.id);
                        guards[si].entry(key).or_default().push_front(maker);
                    }
                    continue;
                }
            }

            let fill_qty = (*remaining).min(maker.remaining);
            let price = maker.price;

            let trade = Trade {
                id: ID_POOL.next_id(),
                buy_order_id: if order.side == OrderSide::Buy { order.id } else { maker.id },
                sell_order_id: if order.side == OrderSide::Sell { order.id } else { maker.id },
                pair: order.pair.clone(),
                price,
                quantity: fill_qty,
                total: price * fill_qty,
                buy_user_id: if order.side == OrderSide::Buy { order.user_id } else { maker.user_id },
                sell_user_id: if order.side == OrderSide::Sell { order.user_id } else { maker.user_id },
                timestamp: crate::time_cache::fast_now_ms(),
                dot_settled: false,
                tee_notarized: false,
            };

            trades.push(trade);
            *remaining -= fill_qty;
            maker.remaining -= fill_qty;

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
                let si = shard_index(&maker.id);
                guards[si].entry(key).or_default().push_front(maker);
            } else if *remaining > 0.0 {
                for guard in guards.iter_mut() {
                    if let Some(orders) = guard.get(&key) {
                        if orders.is_empty() { guard.remove(&key); }
                    }
                }
            }
        }
    }

    pub fn execute_market_internal(&self, mut order: Order, check: &dyn Fn(&Order, &Order) -> bool) -> Result<PlaceOrderResult, String> {
        let mut trades: Vec<Trade> = Vec::new();
        let mut remaining = order.quantity;

        match order.side {
            OrderSide::Buy => {
                let mut ask_guards: Vec<_> = self.shards.iter().map(|s| s.asks.write()).collect();
                Self::execute_market_loop(&mut ask_guards, &order, check, &mut trades, &mut remaining, true);
                drop(ask_guards);
            }
            OrderSide::Sell => {
                let mut bid_guards: Vec<_> = self.shards.iter().map(|s| s.bids.write()).collect();
                Self::execute_market_loop(&mut bid_guards, &order, check, &mut trades, &mut remaining, false);
                drop(bid_guards);
            }
        }

        if let Some(t) = trades.last() {
            self.update_aggregates(t.price, t.quantity);
            order.price = t.price;
            if self.trades_enabled.load(AtomicOrdering::Relaxed) {
                let mut ts = self.trades.lock();
                for t in &trades { ts.push(t.clone()); }
            }
        }

        let total_filled = order.quantity - remaining;
        order.filled = total_filled;
        order.remaining = remaining;
        order.status = if remaining <= 0.0 { OrderStatus::Filled } else { OrderStatus::PartiallyFilled };
        Ok(PlaceOrderResult { order, trades, remaining })
    }

    fn execute_market_loop(
        guards: &mut [parking_lot::RwLockWriteGuard<BTreeMap<i64, VecDeque<Order>>>],
        order: &Order,
        check: &dyn Fn(&Order, &Order) -> bool,
        trades: &mut Vec<Trade>,
        remaining: &mut f64,
        is_buy: bool,
    ) {
        while *remaining > 0.0 {
            let best_key = if is_buy {
                guards.iter().filter_map(|g| g.first_key_value().map(|(k, _)| *k)).min()
            } else {
                guards.iter().filter_map(|g| g.last_key_value().map(|(k, _)| *k)).max()
            };

            let key = match best_key { Some(k) => k, None => break };

            let mut maker = None;
            let mut maker_shard = 0;

            for (si, guard) in guards.iter_mut().enumerate() {
                if let Some(orders) = guard.get_mut(&key) {
                    let mut temp: VecDeque<Order> = VecDeque::new();
                    while let Some(front) = orders.pop_front() {
                        if check(order, &front) { maker = Some(front); maker_shard = si; break; }
                        temp.push_back(front);
                    }
                    while let Some(o) = temp.pop_front() { orders.push_back(o); }
                    if maker.is_some() {
                        if orders.is_empty() { guard.remove(&key); }
                        break;
                    }
                    if orders.is_empty() { guard.remove(&key); }
                }
            }

            let mut maker = match maker {
                Some(m) => m,
                None => {
                    for guard in guards.iter_mut() { guard.remove(&key); }
                    continue;
                }
            };

            let fill_qty = (*remaining).min(maker.remaining);
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
                        let si = shard_index(&maker.id);
                        guards[si].entry(key).or_default().push_front(maker);
                    }
                    continue;
                }
            }

            let trade = Trade {
                id: ID_POOL.next_id(),
                buy_order_id: if order.side == OrderSide::Buy { order.id } else { maker.id },
                sell_order_id: if order.side == OrderSide::Sell { order.id } else { maker.id },
                pair: order.pair.clone(),
                price,
                quantity: fill_qty,
                total: price * fill_qty,
                buy_user_id: if order.side == OrderSide::Buy { order.user_id } else { maker.user_id },
                sell_user_id: if order.side == OrderSide::Sell { order.user_id } else { maker.user_id },
                timestamp: crate::time_cache::fast_now_ms(),
                dot_settled: false,
                tee_notarized: false,
            };

            trades.push(trade);
            *remaining -= fill_qty;
            maker.remaining -= fill_qty;

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
                let si = shard_index(&maker.id);
                guards[si].entry(key).or_default().push_front(maker);
            } else if *remaining > 0.0 {
                for guard in guards.iter_mut() {
                    if let Some(orders) = guard.get(&key) {
                        if orders.is_empty() { guard.remove(&key); }
                    }
                }
            }
        }
    }

    pub fn remove_order(&self, id: Uuid) -> bool {
        for shard in &self.shards {
            {
                let mut bids = shard.bids.write();
                for orders in bids.values_mut() {
                    if let Some(pos) = orders.iter().position(|o| o.id == id) {
                        orders.remove(pos);
                        return true;
                    }
                }
            }
            {
                let mut asks = shard.asks.write();
                for orders in asks.values_mut() {
                    if let Some(pos) = orders.iter().position(|o| o.id == id) {
                        orders.remove(pos);
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn find_order(&self, id: Uuid) -> Option<Order> {
        for shard in &self.shards {
            {
                let bids = shard.bids.read();
                for orders in bids.values() {
                    if let Some(o) = orders.iter().find(|o| o.id == id) { return Some(o.clone()); }
                }
            }
            {
                let asks = shard.asks.read();
                for orders in asks.values() {
                    if let Some(o) = orders.iter().find(|o| o.id == id) { return Some(o.clone()); }
                }
            }
        }
        None
    }

    fn best_bid_price(&self) -> f64 {
        let mut best = 0u64;
        for shard in &self.shards {
            let bids = shard.bids.read();
            if let Some((_, orders)) = bids.last_key_value() {
                if let Some(front) = orders.front() {
                    let bits = front.price.to_bits();
                    if bits > best { best = bits; }
                }
            }
        }
        f64::from_bits(best)
    }

    fn best_ask_price(&self) -> f64 {
        let mut best = f64::MAX.to_bits();
        for shard in &self.shards {
            let asks = shard.asks.read();
            if let Some((_, orders)) = asks.first_key_value() {
                if let Some(front) = orders.front() {
                    let bits = front.price.to_bits();
                    if bits < best { best = bits; }
                }
            }
        }
        if best == f64::MAX.to_bits() { 0.0 } else { f64::from_bits(best) }
    }

    fn bid_count(&self) -> usize {
        let mut count = 0;
        for shard in &self.shards {
            count += shard.bids.read().values().map(|v| v.len()).sum::<usize>();
        }
        count
    }

    fn ask_count(&self) -> usize {
        let mut count = 0;
        for shard in &self.shards {
            count += shard.asks.read().values().map(|v| v.len()).sum::<usize>();
        }
        count
    }

    pub fn snapshot_orders(&self) -> (Vec<(i64, Vec<Order>)>, Vec<(i64, Vec<Order>)>) {
        let mut bids: Vec<(i64, Vec<Order>)> = Vec::new();
        let mut asks: Vec<(i64, Vec<Order>)> = Vec::new();
        for shard in &self.shards {
            for (k, orders) in shard.bids.read().iter() {
                bids.push((*k, orders.iter().cloned().collect()));
            }
            for (k, orders) in shard.asks.read().iter() {
                asks.push((*k, orders.iter().cloned().collect()));
            }
        }
        bids.sort_by_key(|(k, _)| *k);
        asks.sort_by_key(|(k, _)| *k);
        (bids, asks)
    }
}

impl OrderBookManager {
    pub fn new() -> Self {
        Self {
            books: DashMap::new(),
            total_orders: AtomicU64::new(0),
            total_trades: AtomicU64::new(0),
            tps_count: AtomicU64::new(0),
            tps_peak: AtomicU64::new(0),
            matching: MatchingEngine,
            matching_mode: parking_lot::RwLock::new(MatchingMode::Continuous),
            auction_state: DashMap::new(),
            counterparty_visibility: None,
            stop_losses: DashMap::new(),
            twap_orders: DashMap::new(),
            user_orders: DashMap::new(),
            user_trades: DashMap::new(),
        }
    }

    pub fn with_counterparty_visibility(mut self, store: Arc<CounterpartyVisibilityStore>) -> Self {
        self.counterparty_visibility = Some(store);
        self
    }

    pub fn create_book(&self, pair: &str) {
        let key = pair.to_uppercase();
        if !self.books.contains_key(&key) {
            self.books.insert(key.clone(), OrderBook::new(&key));
            info!(pair = %key, "Order book created");
        }
    }

    pub fn create_book_with_batch(&self, pair: &str, window_ns: u64, jitter: u64) {
        let key = pair.to_uppercase();
        self.create_book(&key);
        if !self.auction_state.contains_key(&key) {
            self.auction_state.insert(key.clone(), BatchAuctionState::new(window_ns, jitter));
        }
    }

    pub fn set_matching_mode(&self, mode: MatchingMode) {
        *self.matching_mode.write() = mode;
    }

    pub fn place_order(&self, order: Order) -> Result<PlaceOrderResult, String> {
        self.total_orders.fetch_add(1, AtomicOrdering::Relaxed);
        let key: String = order.pair.as_str().to_uppercase();

        if let OrderStyle::StopLoss { .. } = order.style {
            self.stop_losses.entry(key).or_default().push(order.clone());
            let qty = order.quantity;
            let result = PlaceOrderResult { order, trades: vec![], remaining: qty };
            self.track_user_activity(&result);
            return Ok(result);
        }

        if let OrderStyle::TWAP { duration_secs, interval_secs } = order.style {
            let qty = order.quantity;
            let slices = if interval_secs > 0 { duration_secs / interval_secs } else { 1 };
            let slice_size = qty / slices as f64;
            let state = TwapState {
                order: order.clone(),
                total_quantity: qty,
                filled_quantity: 0.0,
                interval_secs,
                next_slice_time: crate::time_cache::fast_now_ms() + (interval_secs as i64 * 1000),
                slice_size,
                slices_remaining: slices,
            };
            self.twap_orders.insert(order.id, state);
            let rem = order.quantity;
            let result = PlaceOrderResult { order, trades: vec![], remaining: rem };
            self.track_user_activity(&result);
            return Ok(result);
        }

        let mode = self.matching_mode.read().clone();

        let result = match mode {
            MatchingMode::BatchAuction { .. } => {
                if let Some(mut state) = self.auction_state.get_mut(&key) {
                    state.orders.push(order.clone());
                    if Instant::now() >= state.deadline {
                        let batch_result = self.execute_batch_auction(&key)?;
                        self.track_user_activity(&batch_result);
                        return Ok(batch_result);
                    }
                }
                let qty = order.quantity;
                PlaceOrderResult { order, trades: vec![], remaining: qty }
            }
            MatchingMode::Continuous => {
                let book = self.books.get(&key).ok_or_else(|| "pair not found".to_string())?;

                let check = |buy: &Order, sell: &Order| -> bool {
                    match &self.counterparty_visibility {
                        Some(store) => store.mutual_acceptance(&buy.user_id, &sell.user_id),
                        None => true,
                    }
                };
                let (trades, remaining) = MatchingEngine::match_order(&book, &order, &check);

                self.total_trades.fetch_add(trades.len() as u64, AtomicOrdering::Relaxed);

                if remaining > 0.0 {
                    let p_key = price_key(order.price);
                    match order.side {
                        OrderSide::Buy => { book.insert_bid(p_key, order.clone()); }
                        OrderSide::Sell => { book.insert_ask(p_key, order.clone()); }
                    }
                }

                PlaceOrderResult { order, trades, remaining }
            }
        };
        self.track_user_activity(&result);
        Ok(result)
    }

    fn track_user_activity(&self, result: &PlaceOrderResult) {
        self.user_orders.entry(result.order.user_id).or_default().push(result.order.clone());
        for trade in &result.trades {
            self.user_trades.entry(trade.buy_user_id).or_default().push(trade.clone());
            self.user_trades.entry(trade.sell_user_id).or_default().push(trade.clone());
        }
    }

    fn execute_batch_auction(&self, pair: &str) -> Result<PlaceOrderResult, String> {
        let mut state = self.auction_state.get_mut(pair).ok_or_else(|| "no batch state".to_string())?;
        let book = self.books.get(pair).ok_or_else(|| "pair not found".to_string())?;

        let check = |buy: &Order, sell: &Order| -> bool {
            match &self.counterparty_visibility {
                Some(store) => store.mutual_acceptance(&buy.user_id, &sell.user_id),
                None => true,
            }
        };

        let window_number = state.window_number;
        let jitter = state.jitter_range_micros;
        let (trades, remaining_orders) = MatchingEngine::execute_batch_auction(
            &book, &mut state.orders, window_number, &check, jitter,
        );

        if let Some(t) = trades.last() {
            book.update_aggregates(t.price, t.quantity);
        }

        if book.trades_enabled.load(AtomicOrdering::Relaxed) {
            let mut ts = book.trades.lock();
            for t in &trades { ts.push(t.clone()); }
        }

        self.total_trades.fetch_add(trades.len() as u64, AtomicOrdering::Relaxed);
        state.window_number += 1;
        state.reset();

        Ok(PlaceOrderResult {
            order: Order {
                id_tag: 0,
                client_order_id: None,
                filled_quantity: 0,
                id: Uuid::nil(),
                user_id: Uuid::nil(),
                pair: CompactString::from(pair),
                order_type: OrderType::Market,
                side: OrderSide::Buy,
                price: 0.0,
                quantity: 0.0,
                filled: 0.0,
                remaining: 0.0,
                status: OrderStatus::Filled,
                timestamp: crate::time_cache::fast_now_ms(),
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
            },
            trades,
            remaining: remaining_orders.iter().map(|o| o.remaining).sum(),
        })
    }

    pub fn cancel_order(&self, id: Uuid) -> Result<(), String> {
        for mut entry in self.stop_losses.iter_mut() {
            let orders = entry.value_mut();
            if let Some(pos) = orders.iter().position(|o| o.id == id) {
                orders.remove(pos);
                return Ok(());
            }
        }
        if self.twap_orders.remove(&id).is_some() {
            return Ok(());
        }
        for entry in self.books.iter() {
            if entry.value().remove_order(id) { return Ok(()); }
        }
        Err("order not found".to_string())
    }

    pub fn get_order(&self, id: Uuid) -> Option<Order> {
        for entry in self.stop_losses.iter() {
            for o in entry.value() {
                if o.id == id { return Some(o.clone()); }
            }
        }
        if let Some(state) = self.twap_orders.get(&id) {
            return Some(state.order.clone());
        }
        for entry in self.books.iter() {
            if let Some(o) = entry.value().find_order(id) { return Some(o); }
        }
        None
    }

    pub fn get_book_summary(&self, pair: &str) -> Option<OrderBookSummary> {
        let key = pair.to_uppercase();
        self.books.get(&key).map(|book| {
            let best_bid = book.best_bid_price();
            let best_ask = book.best_ask_price();
            let spread_pct = if best_bid > 0.0 { ((best_ask - best_bid) / best_bid) * 100.0 } else { 0.0 };
            OrderBookSummary {
                pair: CompactString::from(key),
                best_bid,
                best_ask,
                last_price: book.get_last_price(),
                volume_24h: book.get_volume_24h(),
                bid_count: book.bid_count(),
                ask_count: book.ask_count(),
                spread_pct,
            }
        })
    }

    pub fn get_depth(&self, pair: &str, levels: usize) -> Option<MarketDepth> {
        let key = pair.to_uppercase();
        self.books.get(&key).map(|book| {
            let mut bids_merged: BTreeMap<i64, (f64, f64, u32)> = BTreeMap::new();
            let mut asks_merged: BTreeMap<i64, (f64, f64, u32)> = BTreeMap::new();

            for shard in &book.shards {
                let b = shard.bids.read();
                for (k, orders) in b.iter() {
                    let price = orders.front().map(|o| o.price).unwrap_or(0.0);
                    let qty: f64 = orders.iter().map(|o| o.remaining).sum();
                    let entry = bids_merged.entry(*k).or_insert((price, 0.0, 0));
                    entry.1 += qty;
                    entry.2 += orders.len() as u32;
                }
            }

            for shard in &book.shards {
                let a = shard.asks.read();
                for (k, orders) in a.iter() {
                    let price = orders.front().map(|o| o.price).unwrap_or(0.0);
                    let qty: f64 = orders.iter().map(|o| o.remaining).sum();
                    let entry = asks_merged.entry(*k).or_insert((price, 0.0, 0));
                    entry.1 += qty;
                    entry.2 += orders.len() as u32;
                }
            }

            let bids: Vec<DepthLevel> = bids_merged.iter().rev().take(levels).map(|(_, &(price, qty, count))| {
                DepthLevel { price, quantity: qty, order_count: count }
            }).collect();

            let asks: Vec<DepthLevel> = asks_merged.iter().take(levels).map(|(_, &(price, qty, count))| {
                DepthLevel { price, quantity: qty, order_count: count }
            }).collect();

            MarketDepth { pair: CompactString::from(key), bids, asks }
        })
    }

    pub fn get_ticker(&self, pair: &str) -> Option<Ticker> {
        let key = pair.to_uppercase();
        self.books.get(&key).map(|book| {
            let bid = book.best_bid_price();
            let ask = book.best_ask_price();
            let last = book.get_last_price();
            let open = book.get_open_24h();
            let change = if open > 0.0 { ((last - open) / open) * 100.0 } else { 0.0 };
            Ticker {
                pair: CompactString::from(key),
                bid, ask, last,
                high_24h: book.get_high_24h(),
                low_24h: book.get_low_24h(),
                volume_24h: book.get_volume_24h(),
                change_24h_pct: change,
            }
        })
    }

    pub fn total_orders(&self) -> u64 { self.total_orders.load(AtomicOrdering::Relaxed) }
    pub fn total_trades(&self) -> u64 { self.total_trades.load(AtomicOrdering::Relaxed) }
    pub fn active_pairs(&self) -> u64 { self.books.len() as u64 }
    pub fn tps_current(&self) -> u64 { self.tps_count.load(AtomicOrdering::Relaxed) }
    pub fn tps_peak(&self) -> u64 { self.tps_peak.load(AtomicOrdering::Relaxed) }

    pub fn is_batch_mode(&self) -> bool {
        matches!(*self.matching_mode.read(), MatchingMode::BatchAuction { .. })
    }

    pub fn get_batch_info(&self, pair: &str) -> Option<serde_json::Value> {
        let key = pair.to_uppercase();
        self.auction_state.get(&key).map(|s| {
            let now = std::time::Instant::now();
            let remaining = if now < s.deadline {
                (s.deadline - now).as_micros() as u64
            } else { 0u64 };
            serde_json::json!({
                "window_ns": s.window_ns,
                "jitter_range_micros": s.jitter_range_micros,
                "window_number": s.window_number,
                "pending_orders": s.orders.len(),
                "deadline_remaining_micros": remaining,
            })
        })
    }

    pub fn execute_batch_auction_manual(&self, pair: &str) -> Result<(), String> {
        let key = pair.to_uppercase();
        let deadline_passed = self.auction_state.get(&key).map_or(false, |s| Instant::now() >= s.deadline);
        if deadline_passed { self.execute_batch_auction(&key)?; }
        Ok(())
    }

    pub fn check_stop_losses(&self, pair: &str, last_price: f64) -> Vec<Order> {
        let key = pair.to_uppercase();
        let mut triggered = Vec::new();
        if let Some(mut entry) = self.stop_losses.get_mut(&key) {
            let mut remaining = Vec::new();
            for mut sl in entry.drain(..) {
                let should_trigger = match sl.style {
                    OrderStyle::StopLoss { trigger_price, .. } => match sl.side {
                        OrderSide::Buy => last_price >= trigger_price,
                        OrderSide::Sell => last_price <= trigger_price,
                    },
                    _ => false,
                };
                if should_trigger {
                    sl.status = OrderStatus::New;
                    sl.style = OrderStyle::Standard;
                    sl.order_type = OrderType::Market;
                    triggered.push(sl);
                } else { remaining.push(sl); }
            }
            if !remaining.is_empty() { *entry = remaining; }
        }
        triggered
    }

    pub fn process_twap(&self, now_ms: i64) -> Vec<Order> {
        let mut slices = Vec::new();
        let mut to_remove = Vec::new();
        for mut entry in self.twap_orders.iter_mut() {
            let state = entry.value_mut();
            if now_ms >= state.next_slice_time && state.slices_remaining > 0 {
                let mut slice = state.order.clone();
                let qty = state.slice_size.min(state.total_quantity - state.filled_quantity);
                slice.id = ID_POOL.next_id();
                slice.quantity = qty;
                slice.remaining = qty;
                slice.status = OrderStatus::New;
                slice.filled = 0.0;
                slice.style = OrderStyle::Standard;
                slice.hidden_remaining = 0.0;
                slices.push(slice);
                state.filled_quantity += qty;
                state.slices_remaining -= 1;
                state.next_slice_time = now_ms + (state.interval_secs as i64 * 1000);
                if state.slices_remaining == 0 || state.filled_quantity >= state.total_quantity {
                    to_remove.push(state.order.id);
                }
            }
        }
        for id in to_remove { self.twap_orders.remove(&id); }
        slices
    }
}
