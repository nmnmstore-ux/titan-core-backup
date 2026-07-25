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

#[allow(dead_code)]
pub struct OrderBook {
    pub pair: CompactString,
    pub bids: BTreeMap<i64, VecDeque<Order>>,
    pub asks: BTreeMap<i64, VecDeque<Order>>,
    pub trades: Vec<Trade>,
    pub trades_enabled: AtomicBool,
    pub last_price: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub volume_24h: f64,
    pub open_24h: f64,
}

impl OrderBook {
    fn new(pair: &str) -> Self {
        Self {
            pair: CompactString::from(pair),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            trades: Vec::with_capacity(4096),
            trades_enabled: AtomicBool::new(false),
            last_price: 0.0,
            high_24h: 0.0,
            low_24h: f64::MAX,
            volume_24h: 0.0,
            open_24h: 0.0,
        }
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

        // StopLoss: store in pending list, not in order book
        if let OrderStyle::StopLoss { .. } = order.style {
            self.stop_losses.entry(key).or_default().push(order.clone());
            let qty = order.quantity;
            let result = PlaceOrderResult { order, trades: vec![], remaining: qty };
            self.track_user_activity(&result);
            return Ok(result);
        }

        // TWAP: store for background scheduler
        if let OrderStyle::TWAP { duration_secs, interval_secs } = order.style {
            let qty = order.quantity;
            let slices = if interval_secs > 0 { duration_secs / interval_secs } else { 1 };
            let slice_size = qty / slices as f64;
            let state = TwapState {
                order: order.clone(),
                total_quantity: qty,
                filled_quantity: 0.0,
                interval_secs,
                next_slice_time: chrono::Utc::now().timestamp_millis() + (interval_secs as i64 * 1000),
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
                let mut book = self.books.get_mut(&key).ok_or_else(|| "pair not found".to_string())?;

                let check = |buy: &Order, sell: &Order| -> bool {
                    match &self.counterparty_visibility {
                        Some(store) => store.mutual_acceptance(&buy.user_id, &sell.user_id),
                        None => true,
                    }
                };
                let (trades, remaining) = MatchingEngine::match_order(&mut book, &order, &check);

                self.total_trades.fetch_add(trades.len() as u64, AtomicOrdering::Relaxed);

                if book.trades_enabled.load(AtomicOrdering::Relaxed) {
                    for t in &trades {
                        book.trades.push(t.clone());
                    }
                }

                for t in &trades {
                    book.last_price = t.price;
                    if t.price > book.high_24h { book.high_24h = t.price; }
                    if t.price < book.low_24h { book.low_24h = t.price; }
                    book.volume_24h += t.quantity;
                    if book.open_24h == 0.0 { book.open_24h = t.price; }
                }

                if remaining > 0.0 {
                    let p_key = price_key(order.price);
                    match order.side {
                        OrderSide::Buy => {
                            book.bids.entry(p_key).or_default().push_back(order.clone());
                        }
                        OrderSide::Sell => {
                            book.asks.entry(p_key).or_default().push_back(order.clone());
                        }
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
        let mut book = self.books.get_mut(pair).ok_or_else(|| "pair not found".to_string())?;

        let check = |buy: &Order, sell: &Order| -> bool {
            match &self.counterparty_visibility {
                Some(store) => store.mutual_acceptance(&buy.user_id, &sell.user_id),
                None => true,
            }
        };

        let window_number = state.window_number;
        let jitter = state.jitter_range_micros;
        let (trades, remaining_orders) = MatchingEngine::execute_batch_auction(
            &mut book,
            &mut state.orders,
            window_number,
            &check,
            jitter,
        );

        // Update book stats
        if !trades.is_empty() {
            book.last_price = trades.last().map(|t| t.price).unwrap_or(book.last_price);
            for t in &trades {
                if t.price > book.high_24h { book.high_24h = t.price; }
                if t.price < book.low_24h { book.low_24h = t.price; }
                book.volume_24h += t.quantity;
            }
        }

        if book.trades_enabled.load(AtomicOrdering::Relaxed) {
            for t in &trades {
                book.trades.push(t.clone());
            }
        }

        self.total_trades.fetch_add(trades.len() as u64, AtomicOrdering::Relaxed);
        state.window_number += 1;
        state.reset();

        Ok(PlaceOrderResult {
            order: Order {
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
            },
            trades,
            remaining: remaining_orders.iter().map(|o| o.remaining).sum(),
        })
    }

    pub fn cancel_order(&self, id: Uuid) -> Result<(), String> {
        // Check stop-loss orders
        for mut entry in self.stop_losses.iter_mut() {
            let orders = entry.value_mut();
            if let Some(pos) = orders.iter().position(|o| o.id == id) {
                orders.remove(pos);
                return Ok(());
            }
        }
        // Check TWAP orders
        if self.twap_orders.remove(&id).is_some() {
            return Ok(());
        }
        // Check order book
        for mut entry in self.books.iter_mut() {
            let book = entry.value_mut();
            for (_, orders) in book.bids.iter_mut() {
                if let Some(pos) = orders.iter().position(|o| o.id == id) {
                    orders.remove(pos);
                    return Ok(());
                }
            }
            for (_, orders) in book.asks.iter_mut() {
                if let Some(pos) = orders.iter().position(|o| o.id == id) {
                    orders.remove(pos);
                    return Ok(());
                }
            }
        }
        Err("order not found".to_string())
    }

    pub fn get_order(&self, id: Uuid) -> Option<Order> {
        // Check stop-loss orders
        for entry in self.stop_losses.iter() {
            for o in entry.value() {
                if o.id == id {
                    return Some(o.clone());
                }
            }
        }
        // Check TWAP orders
        if let Some(state) = self.twap_orders.get(&id) {
            return Some(state.order.clone());
        }
        // Check order book
        for entry in self.books.iter() {
            let book = entry.value();
            for orders in book.bids.values() {
                if let Some(o) = orders.iter().find(|o| o.id == id) {
                    return Some(o.clone());
                }
            }
            for orders in book.asks.values() {
                if let Some(o) = orders.iter().find(|o| o.id == id) {
                    return Some(o.clone());
                }
            }
        }
        None
    }

    pub fn get_book_summary(&self, pair: &str) -> Option<OrderBookSummary> {
        let key = pair.to_uppercase();
        self.books.get(&key).map(|book| {
            let best_bid = book.bids.last_key_value().and_then(|(_, v)| v.front()).map(|o| o.price).unwrap_or(0.0);
            let best_ask = book.asks.first_key_value().and_then(|(_, v)| v.front()).map(|o| o.price).unwrap_or(0.0);
            let spread_pct = if best_bid > 0.0 { ((best_ask - best_bid) / best_bid) * 100.0 } else { 0.0 };
            OrderBookSummary {
                pair: CompactString::from(key),
                best_bid,
                best_ask,
                last_price: book.last_price,
                volume_24h: book.volume_24h,
                bid_count: book.bids.values().map(|v| v.len()).sum(),
                ask_count: book.asks.values().map(|v| v.len()).sum(),
                spread_pct,
            }
        })
    }

    pub fn get_depth(&self, pair: &str, levels: usize) -> Option<MarketDepth> {
        let key = pair.to_uppercase();
        self.books.get(&key).map(|book| {
            let bids: Vec<DepthLevel> = book.bids.iter().rev().take(levels).map(|(_, orders)| {
                let price = orders.front().map(|o| o.price).unwrap_or(0.0);
                let qty: f64 = orders.iter().map(|o| o.remaining).sum();
                DepthLevel { price, quantity: qty, order_count: orders.len() as u32 }
            }).collect();
            let asks: Vec<DepthLevel> = book.asks.iter().take(levels).map(|(_, orders)| {
                let price = orders.front().map(|o| o.price).unwrap_or(0.0);
                let qty: f64 = orders.iter().map(|o| o.remaining).sum();
                DepthLevel { price, quantity: qty, order_count: orders.len() as u32 }
            }).collect();
            MarketDepth { pair: CompactString::from(key), bids, asks }
        })
    }

    pub fn get_ticker(&self, pair: &str) -> Option<Ticker> {
        let key = pair.to_uppercase();
        self.books.get(&key).map(|book| {
            let bid = book.bids.last_key_value().and_then(|(_, v)| v.front()).map(|o| o.price).unwrap_or(0.0);
            let ask = book.asks.first_key_value().and_then(|(_, v)| v.front()).map(|o| o.price).unwrap_or(0.0);
            let change = if book.open_24h > 0.0 { ((book.last_price - book.open_24h) / book.open_24h) * 100.0 } else { 0.0 };
            Ticker {
                pair: CompactString::from(key),
                bid,
                ask,
                last: book.last_price,
                high_24h: book.high_24h,
                low_24h: book.low_24h,
                volume_24h: book.volume_24h,
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
            } else {
                0u64
            };
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
        if deadline_passed {
            self.execute_batch_auction(&key)?;
        }
        Ok(())
    }

    /// Check and execute triggered stop-loss orders after a trade.
    /// Returns the stop-loss orders that were triggered and placed into the book.
    pub fn check_stop_losses(&self, pair: &str, last_price: f64) -> Vec<Order> {
        let key = pair.to_uppercase();
        let mut triggered = Vec::new();
        if let Some(mut entry) = self.stop_losses.get_mut(&key) {
            let mut remaining = Vec::new();
            for mut sl in entry.drain(..) {
                let should_trigger = match sl.style {
                    OrderStyle::StopLoss { trigger_price, .. } => {
                        match sl.side {
                            // Buy stop: trigger when price rises to/above trigger
                            OrderSide::Buy => last_price >= trigger_price,
                            // Sell stop: trigger when price falls to/below trigger
                            OrderSide::Sell => last_price <= trigger_price,
                        }
                    }
                    _ => false,
                };
                if should_trigger {
                    sl.status = OrderStatus::New;
                    sl.style = OrderStyle::Standard;
                    // Convert to market order at trigger
                    sl.order_type = OrderType::Market;
                    triggered.push(sl);
                } else {
                    remaining.push(sl);
                }
            }
            if !remaining.is_empty() {
                *entry = remaining;
            }
        }
        triggered
    }

    /// Execute a TWAP slice if its interval has elapsed.
    /// Returns Some(order) if a slice should be placed.
    pub fn process_twap(&self, now_ms: i64) -> Vec<Order> {
        let mut slices = Vec::new();
        let mut to_remove = Vec::new();
        for mut entry in self.twap_orders.iter_mut() {
            let state = entry.value_mut();
            if now_ms >= state.next_slice_time && state.slices_remaining > 0 {
                let mut slice = state.order.clone();
                let qty = state.slice_size.min(state.total_quantity - state.filled_quantity);
                slice.id = Uuid::new_v4();
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
        for id in to_remove {
            self.twap_orders.remove(&id);
        }
        slices
    }
}