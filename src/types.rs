use compact_str::CompactString;
use crossbeam::queue::ArrayQueue;
use serde::de::{self, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn bincode_serialize<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, String> {
    bincode::serialize(value).map_err(|e| format!("bincode serialize: {}", e))
}

pub fn bincode_deserialize<'de, T: serde::Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, String> {
    bincode::deserialize(bytes).map_err(|e| format!("bincode deserialize: {}", e))
}

pub fn bincode_serialize_direct<T: serde::Serialize>(
    value: &T,
) -> Result<Vec<u8>, Box<bincode::ErrorKind>> {
    bincode::serialize(value)
}

pub fn bincode_deserialize_direct<'de, T: serde::Deserialize<'de>>(
    bytes: &'de [u8],
) -> Result<T, Box<bincode::ErrorKind>> {
    bincode::deserialize(bytes)
}
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use uuid::Uuid;

pub struct IdPool {
    pool: Arc<ArrayQueue<Uuid>>,
    batch_size: usize,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl IdPool {
    pub fn new(capacity: usize, batch_size: usize) -> Self {
        let pool = Arc::new(ArrayQueue::new(capacity));
        let running = Arc::new(AtomicBool::new(true));

        for _ in 0..batch_size.min(capacity) {
            let _ = pool.push(Uuid::new_v4());
        }

        let pool_clone = pool.clone();
        let running_clone = running.clone();
        let handle = thread::spawn(move || loop {
            if !running_clone.load(Ordering::Relaxed) {
                break;
            }
            for _ in 0..batch_size {
                if pool_clone.push(Uuid::new_v4()).is_err() {
                    break;
                }
            }
            thread::sleep(Duration::from_micros(1));
        });

        IdPool {
            pool,
            batch_size,
            running,
            handle: Some(handle),
        }
    }

    pub fn next_id(&self) -> Uuid {
        self.pool.pop().unwrap_or_else(|| {
            for _ in 0..self.batch_size {
                let _ = self.pool.push(Uuid::new_v4());
            }
            self.pool.pop().unwrap_or_else(|| Uuid::new_v4())
        })
    }
}

impl Drop for IdPool {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub static ID_POOL: once_cell::sync::Lazy<IdPool> =
    once_cell::sync::Lazy::new(|| IdPool::new(65536, 16384));

pub static NEXT_ID_TAG: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Hash)]
#[repr(u8)]
pub enum OrderType {
    Market = 0,
    Limit = 1,
    Stop = 2,
    StopLimit = 3,
    SWAP = 4,
}

impl Serialize for OrderType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for OrderType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(OrderType::Market),
            1 => Ok(OrderType::Limit),
            2 => Ok(OrderType::Stop),
            3 => Ok(OrderType::StopLimit),
            4 => Ok(OrderType::SWAP),
            v => Err(de::Error::invalid_value(
                Unexpected::Unsigned(v as u64),
                &"0..=4",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash)]
#[repr(u8)]
pub enum OrderSide {
    Buy = 0,
    Sell = 1,
}

impl Serialize for OrderSide {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for OrderSide {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(OrderSide::Buy),
            1 => Ok(OrderSide::Sell),
            v => Err(de::Error::invalid_value(
                Unexpected::Unsigned(v as u64),
                &"0..=1",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash)]
#[repr(u8)]
pub enum OrderStatus {
    New = 0,
    PartiallyFilled = 1,
    Filled = 2,
    Cancelled = 3,
    Rejected = 4,
    Expired = 5,
}

impl Serialize for OrderStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for OrderStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(OrderStatus::New),
            1 => Ok(OrderStatus::PartiallyFilled),
            2 => Ok(OrderStatus::Filled),
            3 => Ok(OrderStatus::Cancelled),
            4 => Ok(OrderStatus::Rejected),
            5 => Ok(OrderStatus::Expired),
            v => Err(de::Error::invalid_value(
                Unexpected::Unsigned(v as u64),
                &"0..=5",
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderStyle {
    Standard,
    Iceberg {
        display_quantity: f64,
    },
    TWAP {
        duration_secs: u64,
        interval_secs: u64,
    },
    StopLoss {
        trigger_price: f64,
        limit_price: Option<f64>,
    },
}

impl Default for OrderStyle {
    fn default() -> Self {
        OrderStyle::Standard
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: Uuid,
    #[serde(skip)]
    pub id_tag: u64,
    pub user_id: Uuid,
    pub pair: CompactString,
    pub order_type: OrderType,
    pub side: OrderSide,
    pub price: f64,
    pub quantity: f64,
    pub filled: f64,
    pub remaining: f64,
    pub status: OrderStatus,
    pub timestamp: i64,
    #[serde(default)]
    pub ttl_ms: Option<i64>,
    #[serde(default)]
    pub is_swap: bool,
    #[serde(default)]
    pub swap_target_currency: Option<String>,
    #[serde(default)]
    pub tee_signed: bool,
    #[serde(default)]
    pub dot_verified: bool,
    #[serde(default)]
    pub stealth: bool,
    #[serde(default)]
    pub trailing_offset: Option<f64>,
    #[serde(default)]
    pub trigger_price: Option<f64>,
    #[serde(default)]
    pub hard_floor: Option<f64>,
    #[serde(default)]
    pub track: Track,
    #[serde(default)]
    pub style: OrderStyle,
    #[serde(default)]
    pub hidden_remaining: f64,

    pub filled_quantity: u128,
    #[serde(default)]
    pub client_order_id: Option<String>,
}

impl Default for Order {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            id_tag: 0,
            user_id: Uuid::nil(),
            pair: CompactString::new(""),
            order_type: OrderType::Limit,
            side: OrderSide::Buy,
            price: 0.0,
            quantity: 0.0,
            filled: 0.0,
            remaining: 0.0,
            status: OrderStatus::New,
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
            track: Track::Compliant,
            style: OrderStyle::Standard,
            hidden_remaining: 0.0,
            filled_quantity: 0,
            client_order_id: None,
        }
    }
}

impl Order {
    pub fn new_limit(
        user_id: Uuid,
        pair: String,
        side: OrderSide,
        price: f64,
        quantity: f64,
    ) -> Self {
        Self {
            id: ID_POOL.next_id(),
            id_tag: NEXT_ID_TAG.fetch_add(1, Ordering::Relaxed),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Limit,
            side,
            price,
            quantity,
            filled: 0.0,
            remaining: quantity,
            status: OrderStatus::New,
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
            filled_quantity: 0,
            client_order_id: None,
        }
    }

    pub fn new_stealth_trailing(
        user_id: Uuid,
        pair: String,
        side: OrderSide,
        price: f64,
        quantity: f64,
        trail_offset: f64,
        trigger: f64,
    ) -> Self {
        Self {
            id: ID_POOL.next_id(),
            id_tag: NEXT_ID_TAG.fetch_add(1, Ordering::Relaxed),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Limit,
            side,
            price,
            quantity,
            filled: 0.0,
            remaining: quantity,
            status: OrderStatus::New,
            timestamp: crate::time_cache::fast_now_ms(),
            ttl_ms: None,
            is_swap: false,
            swap_target_currency: None,
            tee_signed: false,
            dot_verified: false,
            stealth: true,
            trailing_offset: Some(trail_offset),
            trigger_price: Some(trigger),
            hard_floor: None,
            track: Track::Compliant,
            style: OrderStyle::Standard,
            hidden_remaining: 0.0,
            filled_quantity: 0,
            client_order_id: None,
        }
    }

    pub fn new_hard_floor(
        user_id: Uuid,
        pair: String,
        side: OrderSide,
        price: f64,
        quantity: f64,
        floor: f64,
    ) -> Self {
        Self {
            id: ID_POOL.next_id(),
            id_tag: NEXT_ID_TAG.fetch_add(1, Ordering::Relaxed),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Limit,
            side,
            price,
            quantity,
            filled: 0.0,
            remaining: quantity,
            status: OrderStatus::New,
            timestamp: crate::time_cache::fast_now_ms(),
            ttl_ms: None,
            is_swap: false,
            swap_target_currency: None,
            tee_signed: false,
            dot_verified: false,
            stealth: false,
            trailing_offset: None,
            trigger_price: None,
            hard_floor: Some(floor),
            track: Track::Compliant,
            style: OrderStyle::Standard,
            hidden_remaining: 0.0,
            filled_quantity: 0,
            client_order_id: None,
        }
    }

    pub fn new_market(user_id: Uuid, pair: String, side: OrderSide, quantity: f64) -> Self {
        Self {
            id: ID_POOL.next_id(),
            id_tag: NEXT_ID_TAG.fetch_add(1, Ordering::Relaxed),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Market,
            side,
            price: 0.0,
            quantity,
            filled: 0.0,
            remaining: quantity,
            status: OrderStatus::New,
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
            filled_quantity: 0,
            client_order_id: None,
        }
    }

    pub fn new_swap(user_id: Uuid, from: String, to: String, quantity: f64) -> Self {
        Self {
            id: ID_POOL.next_id(),
            id_tag: NEXT_ID_TAG.fetch_add(1, Ordering::Relaxed),
            user_id,
            pair: CompactString::from(format!("{}/{}", from, to).to_uppercase()),
            order_type: OrderType::SWAP,
            side: OrderSide::Sell,
            price: 0.0,
            quantity,
            filled: 0.0,
            remaining: quantity,
            status: OrderStatus::New,
            timestamp: crate::time_cache::fast_now_ms(),
            ttl_ms: Some(5000),
            is_swap: true,
            swap_target_currency: Some(to),
            tee_signed: false,
            dot_verified: false,
            stealth: false,
            trailing_offset: None,
            trigger_price: None,
            hard_floor: None,
            track: Track::Compliant,
            style: OrderStyle::Standard,
            hidden_remaining: 0.0,
            filled_quantity: 0,
            client_order_id: None,
        }
    }

    pub fn new_iceberg(
        user_id: Uuid,
        pair: String,
        side: OrderSide,
        price: f64,
        quantity: f64,
        display_qty: f64,
    ) -> Self {
        let visible = display_qty.min(quantity);
        Self {
            id: ID_POOL.next_id(),
            id_tag: NEXT_ID_TAG.fetch_add(1, Ordering::Relaxed),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Limit,
            side,
            price,
            quantity,
            filled: 0.0,
            remaining: visible,
            status: OrderStatus::New,
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
            style: OrderStyle::Iceberg {
                display_quantity: display_qty,
            },
            hidden_remaining: quantity - visible,
            filled_quantity: 0,
            client_order_id: None,
        }
    }

    pub fn new_stop_loss(
        user_id: Uuid,
        pair: String,
        side: OrderSide,
        quantity: f64,
        trigger: f64,
        limit_price: Option<f64>,
    ) -> Self {
        Self {
            id: ID_POOL.next_id(),
            id_tag: NEXT_ID_TAG.fetch_add(1, Ordering::Relaxed),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Market,
            side,
            price: limit_price.unwrap_or(0.0),
            quantity,
            filled: 0.0,
            remaining: quantity,
            status: OrderStatus::New,
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
            style: OrderStyle::StopLoss {
                trigger_price: trigger,
                limit_price,
            },
            hidden_remaining: 0.0,
            filled_quantity: 0,
            client_order_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: Uuid,
    pub buy_order_id: Uuid,
    pub sell_order_id: Uuid,
    pub pair: CompactString,
    pub price: f64,
    pub quantity: f64,
    pub total: f64,
    pub buy_user_id: Uuid,
    pub sell_user_id: Uuid,
    pub timestamp: i64,
    #[serde(default)]
    pub dot_settled: bool,
    #[serde(default)]
    pub tee_notarized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DOTTransfer {
    pub id: Uuid,
    pub from_user: Uuid,
    pub to_user: Uuid,
    pub currency: String,
    pub amount: f64,
    pub timestamp: i64,
    pub status: DOTStatus,
    #[serde(default)]
    pub tee_attested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum DOTStatus {
    Pending = 0,
    Settled = 1,
    Failed = 2,
    Disputed = 3,
}

impl Serialize for DOTStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for DOTStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(DOTStatus::Pending),
            1 => Ok(DOTStatus::Settled),
            2 => Ok(DOTStatus::Failed),
            3 => Ok(DOTStatus::Disputed),
            v => Err(de::Error::invalid_value(
                Unexpected::Unsigned(v as u64),
                &"0..=3",
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSummary {
    pub pair: CompactString,
    pub best_bid: f64,
    pub best_ask: f64,
    pub last_price: f64,
    pub volume_24h: f64,
    pub bid_count: usize,
    pub ask_count: usize,
    pub spread_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthLevel {
    pub price: f64,
    pub quantity: f64,
    pub order_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDepth {
    pub pair: CompactString,
    pub bids: Vec<DepthLevel>,
    pub asks: Vec<DepthLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    pub pair: CompactString,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub volume_24h: f64,
    pub change_24h_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceOrderResult {
    pub order: Order,
    pub trades: Vec<Trade>,
    pub remaining: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Track {
    Compliant = 0,
    Autonomous = 1,
}

impl Serialize for Track {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for Track {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(Track::Compliant),
            1 => Ok(Track::Autonomous),
            v => Err(de::Error::invalid_value(
                Unexpected::Unsigned(v as u64),
                &"0..=1",
            )),
        }
    }
}

impl Default for Track {
    fn default() -> Self {
        Track::Compliant
    }
}

impl Track {
    pub fn is_compliant(&self) -> bool {
        matches!(self, Track::Compliant)
    }
    pub fn is_autonomous(&self) -> bool {
        matches!(self, Track::Autonomous)
    }
}

pub const TRACK_COMPLIANT: u8 = 0;
pub const TRACK_AUTONOMOUS: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MatchingMode {
    Continuous,
    BatchAuction {
        window_ns: u64,
        #[serde(default)]
        jitter_range_micros: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DisclosureLevel {
    Public,
    Verified,
    Institutional,
    Sovereign,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwapState {
    pub order: Order,
    pub total_quantity: f64,
    pub filled_quantity: f64,
    pub interval_secs: u64,
    pub next_slice_time: i64,
    pub slice_size: f64,
    pub slices_remaining: u64,
}
