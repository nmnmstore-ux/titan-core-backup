use crate::tee::{HardwareEnclave, TEEEnclave};
use crate::types::*;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::instrument;
use uuid::Uuid;

#[derive(Debug, Error, serde::Serialize)]
pub enum DOTError {
    #[error("DOT validation failed: user_id is nil UUID")]
    NilUserId,
    #[error("DOT validation failed: quantity must be positive and finite, got {0}")]
    InvalidQuantity(f64),
    #[error("DOT validation failed: quantity exceeds maximum allowed (1B), got {0}")]
    QuantityExceedsMaximum(f64),
    #[error("DOT validation failed: unsupported asset pair '{0}'")]
    UnsupportedPair(String),
    #[error("DOT validation failed: order type {0:?} is not compatible with DOT settlement")]
    IncompatibleOrderType(OrderType),
    #[error("DOT validation failed: order style {0:?} requires complex settlement, not DOT-compatible")]
    IncompatibleOrderStyle(OrderStyle),
    #[error("DOT validation failed: price must be non-negative and finite for {0:?} orders, got {1}")]
    InvalidPrice(OrderType, f64),
    #[error("DOT transfer signing failed: {0}")]
    SigningFailed(String),
}

impl From<DOTError> for String {
    fn from(e: DOTError) -> String {
        e.to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DOTReceipt {
    pub transfer_id: Uuid,
    pub from_user: Uuid,
    pub to_user: Uuid,
    pub currency: String,
    pub amount: f64,
    pub status: DOTStatus,
    pub dot_signature: String,
    pub settled_at: i64,
    pub tee_notarized: bool,
    pub immutable: bool,
}

pub struct DOTEngine {
    transfers: DashMap<Uuid, DOTReceipt>,
    total_settled: std::sync::atomic::AtomicU64,
    total_volume: std::sync::atomic::AtomicU64,
    tee: Arc<TEEEnclave>,
}

impl DOTEngine {
    pub fn new(tee: Arc<TEEEnclave>) -> Self {
        Self {
            transfers: DashMap::new(),
            total_settled: std::sync::atomic::AtomicU64::new(0),
            total_volume: std::sync::atomic::AtomicU64::new(0),
            tee,
        }
    }

    #[instrument(skip(self), fields(order_id = %order.id, pair = %order.pair))]
    pub fn validate_order(&self, order: &Order) -> Result<Order, DOTError> {
        if order.user_id.is_nil() {
            let err = DOTError::NilUserId;
            tracing::warn!(order_id = %order.id, "{}", err);
            return Err(err);
        }

        if order.quantity <= 0.0 || !order.quantity.is_finite() {
            let err = DOTError::InvalidQuantity(order.quantity);
            tracing::warn!(order_id = %order.id, "{}", err);
            return Err(err);
        }

        if order.quantity > 1_000_000_000.0 {
            let err = DOTError::QuantityExceedsMaximum(order.quantity);
            tracing::warn!(order_id = %order.id, "{}", err);
            return Err(err);
        }

        const SUPPORTED_PAIRS: &[&str] = &[
            "BTC/USDT", "ETH/USDT", "SOL/USDT", "BTC/ETH",
            "ETH/BTC", "XAU/USD", "EUR/USD", "GBP/USD", "USD/JPY",
            "BTC/USD", "ETH/USD", "SOL/USD", "USDC/USDT",
        ];
        let pair_upper = order.pair.to_uppercase();
        if !SUPPORTED_PAIRS.iter().any(|p| *p == pair_upper) {
            let err = DOTError::UnsupportedPair(order.pair.to_string());
            tracing::warn!(order_id = %order.id, "{}", err);
            return Err(err);
        }

        match order.order_type {
            OrderType::Stop | OrderType::StopLimit => {
                let err = DOTError::IncompatibleOrderType(order.order_type);
                tracing::warn!(order_id = %order.id, "{}", err);
                return Err(err);
            }
            _ => {}
        }

        match &order.style {
            OrderStyle::TWAP { .. } | OrderStyle::StopLoss { .. } => {
                let err = DOTError::IncompatibleOrderStyle(order.style.clone());
                tracing::warn!(order_id = %order.id, "{}", err);
                return Err(err);
            }
            _ => {}
        }

        if order.price < 0.0 || !order.price.is_finite() {
            if order.order_type != OrderType::Market {
                let err = DOTError::InvalidPrice(order.order_type, order.price);
                tracing::warn!(order_id = %order.id, "{}", err);
                return Err(err);
            }
        }

        tracing::info!(order_id = %order.id, "DOT validation passed");
        let mut validated = order.clone();
        validated.dot_verified = true;
        Ok(validated)
    }

    #[instrument(skip(self), fields(transfer_id = %tx.id, from = %tx.from_user, to = %tx.to_user, amount = tx.amount, currency = %tx.currency))]
    pub fn execute_transfer(&self, tx: DOTTransfer) -> Result<DOTReceipt, DOTError> {
        let settled_at = chrono::Utc::now().timestamp_millis();
        let sig = self.sign_transfer(&tx, settled_at);

        let receipt = DOTReceipt {
            transfer_id: tx.id,
            from_user: tx.from_user.clone(),
            to_user: tx.to_user.clone(),
            currency: tx.currency.clone(),
            amount: tx.amount,
            status: DOTStatus::Settled,
            dot_signature: sig,
            settled_at,
            tee_notarized: true,
            immutable: true,
        };

        self.transfers.insert(tx.id, receipt.clone());
        self.total_settled.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.total_volume.fetch_add((tx.amount * 100.0) as u64, std::sync::atomic::Ordering::Relaxed);

        tracing::info!(
            target: "dot",
            transfer_id = %tx.id,
            from = %tx.from_user,
            to = %tx.to_user,
            amount = tx.amount,
            currency = %tx.currency,
            "DOT transfer settled in {}ms",
            chrono::Utc::now().timestamp_millis() - tx.timestamp
        );

        Ok(receipt)
    }

    pub fn get_transfer(&self, id: Uuid) -> Option<DOTReceipt> {
        self.transfers.get(&id).map(|t| t.clone())
    }

    fn sign_transfer(&self, tx: &DOTTransfer, settled_at: i64) -> String {
        let mut msg = Vec::with_capacity(64);
        msg.extend_from_slice(tx.id.to_string().as_bytes());
        msg.extend_from_slice(tx.from_user.to_string().as_bytes());
        msg.extend_from_slice(tx.to_user.to_string().as_bytes());
        msg.extend_from_slice(tx.currency.as_bytes());
        msg.extend_from_slice(&tx.amount.to_le_bytes());
        msg.extend_from_slice(&settled_at.to_le_bytes());
        let sig = self.tee.sign(&msg);
        format!("DOT_ED25519_{}", hex::encode(sig))
    }

    pub fn verify_transfer_signature(&self, receipt: &DOTReceipt) -> bool {
        let hex_sig = receipt.dot_signature.strip_prefix("DOT_ED25519_").unwrap_or(&receipt.dot_signature);
        let sig_bytes = match hex::decode(hex_sig) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let mut msg = Vec::with_capacity(64);
        msg.extend_from_slice(receipt.transfer_id.to_string().as_bytes());
        msg.extend_from_slice(receipt.from_user.to_string().as_bytes());
        msg.extend_from_slice(receipt.to_user.to_string().as_bytes());
        msg.extend_from_slice(receipt.currency.as_bytes());
        msg.extend_from_slice(&receipt.amount.to_le_bytes());
        msg.extend_from_slice(&receipt.settled_at.to_le_bytes());
        self.tee.verify(&msg, &sig_bytes)
    }

    pub fn total_settlements(&self) -> u64 {
        self.total_settled.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn total_volume(&self) -> f64 {
        self.total_volume.load(std::sync::atomic::Ordering::Relaxed) as f64 / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Order, OrderSide, OrderStyle, OrderType};
    use std::sync::Arc;

    fn valid_order() -> Order {
        Order::new_limit(uuid::Uuid::new_v4(), "BTC/USDT".into(), OrderSide::Buy, 50000.0, 1.0)
    }

    fn engine() -> DOTEngine {
        DOTEngine::new(Arc::new(TEEEnclave::new()))
    }

    #[test]
    fn valid_order_passes_validation() {
        let eng = engine();
        let order = valid_order();
        let result = eng.validate_order(&order);
        assert!(result.is_ok());
        assert!(result.unwrap().dot_verified);
    }

    // Check 1: nil user_id
    #[test]
    fn reject_nil_user_id() {
        let eng = engine();
        let mut order = valid_order();
        order.user_id = uuid::Uuid::nil();
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("user_id is nil UUID"));
    }

    // Check 2a: quantity zero
    #[test]
    fn reject_zero_quantity() {
        let eng = engine();
        let mut order = valid_order();
        order.quantity = 0.0;
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("quantity must be positive and finite"));
    }

    // Check 2b: quantity negative
    #[test]
    fn reject_negative_quantity() {
        let eng = engine();
        let mut order = valid_order();
        order.quantity = -5.0;
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("quantity must be positive and finite"));
    }

    // Check 2c: quantity NaN
    #[test]
    fn reject_nan_quantity() {
        let eng = engine();
        let mut order = valid_order();
        order.quantity = f64::NAN;
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("quantity must be positive and finite"));
    }

    // Check 3: quantity exceeds 1B
    #[test]
    fn reject_quantity_exceeds_max() {
        let eng = engine();
        let mut order = valid_order();
        order.quantity = 1_000_000_001.0;
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("exceeds maximum allowed"));
    }

    // Check 4: unsupported pair
    #[test]
    fn reject_unsupported_pair() {
        let eng = engine();
        let mut order = valid_order();
        order.pair = "DOGE/USD".into();
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("unsupported asset pair"));
    }

    // Check 5a: Stop order type
    #[test]
    fn reject_stop_order_type() {
        let eng = engine();
        let mut order = valid_order();
        order.order_type = OrderType::Stop;
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("not compatible with DOT settlement"));
    }

    // Check 5b: StopLimit order type
    #[test]
    fn reject_stoplimit_order_type() {
        let eng = engine();
        let mut order = valid_order();
        order.order_type = OrderType::StopLimit;
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("not compatible with DOT settlement"));
    }

    // Check 6a: TWAP style
    #[test]
    fn reject_twap_style() {
        let eng = engine();
        let mut order = valid_order();
        order.style = OrderStyle::TWAP { duration_secs: 60, interval_secs: 5 };
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("requires complex settlement"));
    }

    // Check 6b: StopLoss style
    #[test]
    fn reject_stoploss_style() {
        let eng = engine();
        let mut order = valid_order();
        order.style = OrderStyle::StopLoss { trigger_price: 49000.0, limit_price: Some(48500.0) };
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("requires complex settlement"));
    }

    // Check 7: negative price on non-market
    #[test]
    fn reject_negative_price_limit_order() {
        let eng = engine();
        let mut order = valid_order();
        order.order_type = OrderType::Limit;
        order.price = -100.0;
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("price must be non-negative"));
    }

    // Check 8: NaN price on non-market
    #[test]
    fn reject_nan_price_limit_order() {
        let eng = engine();
        let mut order = valid_order();
        order.order_type = OrderType::Limit;
        order.price = f64::NAN;
        let err = eng.validate_order(&order).unwrap_err();
        assert!(err.to_string().contains("price must be non-negative"));
    }

    // Market order with bad price is allowed (price ignored for market)
    #[test]
    fn market_order_with_bad_price_passes() {
        let eng = engine();
        let order = Order::new_market(uuid::Uuid::new_v4(), "BTC/USDT".into(), OrderSide::Buy, 1.0);
        assert!(eng.validate_order(&order).is_ok());
    }

    #[test]
    fn execute_transfer_produces_signed_receipt() {
        let eng = engine();
        let tx = DOTTransfer {
            id: uuid::Uuid::new_v4(),
            from_user: uuid::Uuid::new_v4(),
            to_user: uuid::Uuid::new_v4(),
            currency: "USDT".into(),
            amount: 1000.0,
            timestamp: chrono::Utc::now().timestamp_millis(),
            status: DOTStatus::Pending,
            tee_attested: true,
        };

        let receipt = eng.execute_transfer(tx.clone()).unwrap();
        assert_eq!(receipt.status, DOTStatus::Settled);
        assert!(receipt.dot_signature.starts_with("DOT_ED25519_"));
        assert!(receipt.tee_notarized);

        // Verify the TEE signature
        assert!(eng.verify_transfer_signature(&receipt));
        assert_eq!(eng.total_settlements(), 1);
    }
}
