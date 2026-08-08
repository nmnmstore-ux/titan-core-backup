use crate::types::*;
fn track_to_i32(track: &Track) -> i32 {
    match track {
        Track::Autonomous => 1,
        _ => 0,
    }
}

fn i32_to_track(v: i32) -> Track {
    match v {
        1 => Track::Autonomous,
        _ => Track::Compliant,
    }
}
use compact_str::CompactString;
use serde_json;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::info;
use uuid::Uuid;

const MIGRATIONS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS orders (
        id UUID PRIMARY KEY,
        user_id UUID NOT NULL,
        pair TEXT NOT NULL,
        order_type TEXT NOT NULL,
        side TEXT NOT NULL,
        price DOUBLE PRECISION NOT NULL,
        quantity DOUBLE PRECISION NOT NULL,
        filled DOUBLE PRECISION NOT NULL DEFAULT 0,
        remaining DOUBLE PRECISION NOT NULL,
        status TEXT NOT NULL DEFAULT 'New',
        timestamp BIGINT NOT NULL,
        ttl_ms BIGINT,
        is_swap BOOLEAN NOT NULL DEFAULT false,
        swap_target_currency TEXT,
        tee_signed BOOLEAN NOT NULL DEFAULT false,
        dot_verified BOOLEAN NOT NULL DEFAULT false,
        stealth BOOLEAN NOT NULL DEFAULT false,
        trailing_offset DOUBLE PRECISION,
        trigger_price DOUBLE PRECISION,
        hard_floor DOUBLE PRECISION,
        track INTEGER NOT NULL DEFAULT 0,
        style_json TEXT NOT NULL DEFAULT '{}',
        hidden_remaining DOUBLE PRECISION NOT NULL DEFAULT 0,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )",
    "CREATE TABLE IF NOT EXISTS trades (
        id UUID PRIMARY KEY,
        buy_order_id UUID NOT NULL,
        sell_order_id UUID NOT NULL,
        pair TEXT NOT NULL,
        price DOUBLE PRECISION NOT NULL,
        quantity DOUBLE PRECISION NOT NULL,
        total DOUBLE PRECISION NOT NULL,
        buy_user_id UUID NOT NULL,
        sell_user_id UUID NOT NULL,
        timestamp BIGINT NOT NULL,
        dot_settled BOOLEAN NOT NULL DEFAULT false,
        tee_notarized BOOLEAN NOT NULL DEFAULT false,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )",
    "CREATE TABLE IF NOT EXISTS wal_entries (
        seq BIGSERIAL PRIMARY KEY,
        record_type TEXT NOT NULL,
        record_json TEXT NOT NULL,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )",
    "CREATE INDEX IF NOT EXISTS idx_orders_pair ON orders(pair)",
    "CREATE INDEX IF NOT EXISTS idx_orders_user ON orders(user_id)",
    "CREATE INDEX IF NOT EXISTS idx_orders_status ON orders(status)",
    "CREATE INDEX IF NOT EXISTS idx_trades_pair ON trades(pair)",
    "CREATE INDEX IF NOT EXISTS idx_trades_timestamp ON trades(timestamp)",
    "CREATE INDEX IF NOT EXISTS idx_wal_entries_created ON wal_entries(created_at)",
];

#[derive(Clone)]
pub struct PgStore {
    pool: Option<PgPool>,
}

impl PgStore {
    pub async fn new(database_url: &str) -> Result<Self, String> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .connect(database_url)
            .await
            .map_err(|e| format!("PostgreSQL connection failed: {}", e))?;
        let store = Self { pool: Some(pool) };
        store.run_migrations().await?;
        info!("PostgreSQL persistence initialized");
        Ok(store)
    }

    pub fn disabled() -> Self {
        Self { pool: None }
    }

    pub fn is_enabled(&self) -> bool { self.pool.is_some() }

    pub fn pool(&self) -> Option<&PgPool> {
        self.pool.as_ref()
    }

    async fn run_migrations(&self) -> Result<(), String> {
        let pool = self.pool.as_ref().ok_or("pg not enabled")?;
        for (i, migration) in MIGRATIONS.iter().enumerate() {
            sqlx::query(migration)
                .execute(pool)
                .await
                .map_err(|e| format!("migration {} failed: {}", i, e))?;
        }
        info!("{} migrations applied", MIGRATIONS.len());
        Ok(())
    }

    pub async fn save_order(&self, order: &Order) -> Result<(), String> {
        let pool = match self.pool.as_ref() {
            Some(p) => p,
            None => return Ok(()),
        };
        let style_json = serde_json::to_string(&order.style).unwrap_or_default();
        sqlx::query(
            "INSERT INTO orders (id, user_id, pair, order_type, side, price, quantity, filled, remaining, status, timestamp, ttl_ms, is_swap, swap_target_currency, tee_signed, dot_verified, stealth, trailing_offset, trigger_price, hard_floor, track, style_json, hidden_remaining)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)
             ON CONFLICT (id) DO UPDATE SET
                filled = EXCLUDED.filled, remaining = EXCLUDED.remaining, status = EXCLUDED.status"
        )
            .bind(order.id)
            .bind(order.user_id)
            .bind(order.pair.as_str())
            .bind(format!("{:?}", order.order_type))
            .bind(format!("{:?}", order.side))
            .bind(order.price)
            .bind(order.quantity)
            .bind(order.filled)
            .bind(order.remaining)
            .bind(format!("{:?}", order.status))
            .bind(order.timestamp)
            .bind(order.ttl_ms)
            .bind(order.is_swap)
            .bind(&order.swap_target_currency)
            .bind(order.tee_signed)
            .bind(order.dot_verified)
            .bind(order.stealth)
            .bind(order.trailing_offset)
            .bind(order.trigger_price)
            .bind(order.hard_floor)
            .bind(track_to_i32(&order.track))
            .bind(&style_json)
            .bind(order.hidden_remaining)
            .execute(pool)
            .await
            .map_err(|e| format!("save_order failed: {}", e))?;
        Ok(())
    }

    pub async fn save_trade(&self, trade: &Trade) -> Result<(), String> {
        let pool = match self.pool.as_ref() {
            Some(p) => p,
            None => return Ok(()),
        };
        sqlx::query(
            "INSERT INTO trades (id, buy_order_id, sell_order_id, pair, price, quantity, total, buy_user_id, sell_user_id, timestamp, dot_settled, tee_notarized)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (id) DO NOTHING"
        )
            .bind(trade.id)
            .bind(trade.buy_order_id)
            .bind(trade.sell_order_id)
            .bind(trade.pair.as_str())
            .bind(trade.price)
            .bind(trade.quantity)
            .bind(trade.total)
            .bind(trade.buy_user_id)
            .bind(trade.sell_user_id)
            .bind(trade.timestamp)
            .bind(trade.dot_settled)
            .bind(trade.tee_notarized)
            .execute(pool)
            .await
            .map_err(|e| format!("save_trade failed: {}", e))?;
        Ok(())
    }

    pub async fn save_wal_entry(&self, record_type: &str, record_json: &str) -> Result<(), String> {
        let pool = match self.pool.as_ref() {
            Some(p) => p,
            None => return Ok(()),
        };
        sqlx::query("INSERT INTO wal_entries (record_type, record_json) VALUES ($1, $2)")
            .bind(record_type)
            .bind(record_json)
            .execute(pool)
            .await
            .map_err(|e| format!("save_wal_entry failed: {}", e))?;
        Ok(())
    }

    pub async fn get_orders(&self, pair: &str, limit: i64) -> Result<Vec<Order>, String> {
        let pool = match self.pool.as_ref() {
            Some(p) => p,
            None => return Ok(vec![]),
        };
        let rows = sqlx::query_as::<_, OrderRow>(
            "SELECT * FROM orders WHERE pair = $1 ORDER BY timestamp DESC LIMIT $2"
        )
            .bind(pair)
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(|e| format!("get_orders failed: {}", e))?;
        rows.into_iter().map(|r| r.into_order()).collect()
    }

    pub async fn get_trades(&self, pair: &str, limit: i64) -> Result<Vec<Trade>, String> {
        let pool = match self.pool.as_ref() {
            Some(p) => p,
            None => return Ok(vec![]),
        };
        let rows = sqlx::query_as::<_, TradeRow>(
            "SELECT * FROM trades WHERE pair = $1 ORDER BY timestamp DESC LIMIT $2"
        )
            .bind(pair)
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(|e| format!("get_trades failed: {}", e))?;
        rows.into_iter().map(|r| r.into_trade()).collect()
    }
}

#[derive(sqlx::FromRow)]
struct OrderRow {
    id: Uuid,
    user_id: Uuid,
    pair: String,
    order_type: String,
    side: String,
    price: f64,
    quantity: f64,
    filled: f64,
    remaining: f64,
    status: String,
    timestamp: i64,
    ttl_ms: Option<i64>,
    is_swap: bool,
    swap_target_currency: Option<String>,
    tee_signed: bool,
    dot_verified: bool,
    stealth: bool,
    trailing_offset: Option<f64>,
    trigger_price: Option<f64>,
    hard_floor: Option<f64>,
    track: i32,
    style_json: String,
    hidden_remaining: f64,
}

impl OrderRow {
    fn into_order(self) -> Result<Order, String> {
        let order_type = match self.order_type.as_str() {
            "Limit" => OrderType::Limit,
            "Market" => OrderType::Market,
            "SWAP" => OrderType::SWAP,
            _ => return Err(format!("unknown order_type: {}", self.order_type)),
        };
        let side = match self.side.as_str() {
            "Buy" => OrderSide::Buy,
            "Sell" => OrderSide::Sell,
            _ => return Err(format!("unknown side: {}", self.side)),
        };
        let status = match self.status.as_str() {
            "New" => OrderStatus::New,
            "PartiallyFilled" => OrderStatus::PartiallyFilled,
            "Filled" => OrderStatus::Filled,
            "Cancelled" => OrderStatus::Cancelled,
            "Rejected" => OrderStatus::Rejected,
            "Expired" => OrderStatus::Expired,
            _ => OrderStatus::New,
        };
        let track = i32_to_track(self.track);
        let style: OrderStyle = serde_json::from_str(&self.style_json).unwrap_or(OrderStyle::Standard);
        Ok(Order {
            id: self.id,
            id_tag: 0,
            user_id: self.user_id,
            pair: CompactString::from(self.pair),
            order_type,
            side,
            price: self.price,
            quantity: self.quantity,
            filled: self.filled,
            remaining: self.remaining,
            status,
            timestamp: self.timestamp,
            ttl_ms: self.ttl_ms,
            is_swap: self.is_swap,
            swap_target_currency: self.swap_target_currency,
            tee_signed: self.tee_signed,
            dot_verified: self.dot_verified,
            stealth: self.stealth,
            trailing_offset: self.trailing_offset,
            trigger_price: self.trigger_price,
            hard_floor: self.hard_floor,
            track,
            style,
            hidden_remaining: self.hidden_remaining,
            client_order_id: None,
            filled_quantity: 0,
        })
    }
}

#[derive(sqlx::FromRow)]
struct TradeRow {
    id: Uuid,
    buy_order_id: Uuid,
    sell_order_id: Uuid,
    pair: String,
    price: f64,
    quantity: f64,
    total: f64,
    buy_user_id: Uuid,
    sell_user_id: Uuid,
    timestamp: i64,
    dot_settled: bool,
    tee_notarized: bool,
}

impl TradeRow {
    fn into_trade(self) -> Result<Trade, String> {
        Ok(Trade {
            id: self.id,
            buy_order_id: self.buy_order_id,
            sell_order_id: self.sell_order_id,
            pair: CompactString::from(self.pair),
            price: self.price,
            quantity: self.quantity,
            total: self.total,
            buy_user_id: self.buy_user_id,
            sell_user_id: self.sell_user_id,
            timestamp: self.timestamp,
            dot_settled: self.dot_settled,
            tee_notarized: self.tee_notarized,
        })
    }
}
