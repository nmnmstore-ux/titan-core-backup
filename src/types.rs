use serde::{Deserialize, Serialize};
use uuid::Uuid;
use compact_str::CompactString;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Hash)]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
    SWAP,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Hash)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Hash)]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Expired,
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
    fn default() -> Self { OrderStyle::Standard }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: Uuid,
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
    pub ttl_ms: Option<i64>,
    pub is_swap: bool,
    pub swap_target_currency: Option<String>,
    pub tee_signed: bool,
    pub dot_verified: bool,
    pub stealth: bool,
    pub trailing_offset: Option<f64>,
    pub trigger_price: Option<f64>,
    pub hard_floor: Option<f64>,
    #[serde(default)]
    pub track: Track,
    #[serde(default)]
    pub style: OrderStyle,
    #[serde(default)]
    pub hidden_remaining: f64,

    pub filled_quantity: u128,
    pub client_order_id: Option<String>,
}

impl Default for Order {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
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
    pub fn new_limit(user_id: Uuid, pair: String, side: OrderSide, price: f64, quantity: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Limit,
            side,
            price,
            quantity,
            filled: 0.0,
            remaining: quantity,
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
            style: OrderStyle::Standard,
            hidden_remaining: 0.0,
            filled_quantity: 0,
            client_order_id: None,
        }
    }

    pub fn new_stealth_trailing(user_id: Uuid, pair: String, side: OrderSide, price: f64, quantity: f64, trail_offset: f64, trigger: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Limit,
            side,
            price,
            quantity,
            filled: 0.0,
            remaining: quantity,
            status: OrderStatus::New,
            timestamp: chrono::Utc::now().timestamp_millis(),
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

    pub fn new_hard_floor(user_id: Uuid, pair: String, side: OrderSide, price: f64, quantity: f64, floor: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Limit,
            side,
            price,
            quantity,
            filled: 0.0,
            remaining: quantity,
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
            id: Uuid::new_v4(),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Market,
            side,
            price: 0.0,
            quantity,
            filled: 0.0,
            remaining: quantity,
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
            style: OrderStyle::Standard,
            hidden_remaining: 0.0,
            filled_quantity: 0,
            client_order_id: None,
        }
    }

    pub fn new_swap(user_id: Uuid, from: String, to: String, quantity: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            pair: CompactString::from(format!("{}/{}", from, to).to_uppercase()),
            order_type: OrderType::SWAP,
            side: OrderSide::Sell,
            price: 0.0,
            quantity,
            filled: 0.0,
            remaining: quantity,
            status: OrderStatus::New,
            timestamp: chrono::Utc::now().timestamp_millis(),
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

    pub fn new_iceberg(user_id: Uuid, pair: String, side: OrderSide, price: f64, quantity: f64, display_qty: f64) -> Self {
        let visible = display_qty.min(quantity);
        Self {
            id: Uuid::new_v4(),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Limit,
            side,
            price,
            quantity,
            filled: 0.0,
            remaining: visible,
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
            style: OrderStyle::Iceberg { display_quantity: display_qty },
            hidden_remaining: quantity - visible,
            filled_quantity: 0,
            client_order_id: None,
        }
    }

    pub fn new_stop_loss(user_id: Uuid, pair: String, side: OrderSide, quantity: f64, trigger: f64, limit_price: Option<f64>) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            pair: CompactString::from(pair.to_uppercase()),
            order_type: OrderType::Market,
            side,
            price: limit_price.unwrap_or(0.0),
            quantity,
            filled: 0.0,
            remaining: quantity,
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
            style: OrderStyle::StopLoss { trigger_price: trigger, limit_price },
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
    pub dot_settled: bool,
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
    pub tee_attested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DOTStatus {
    Pending,
    Settled,
    Failed,
    Disputed,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Track {
    Compliant,
    Autonomous,
}

impl Default for Track {
    fn default() -> Self { Track::Compliant }
}

impl Track {
    pub fn is_compliant(&self) -> bool { matches!(self, Track::Compliant) }
    pub fn is_autonomous(&self) -> bool { matches!(self, Track::Autonomous) }
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