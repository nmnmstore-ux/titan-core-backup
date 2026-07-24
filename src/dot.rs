use crate::tee::{HardwareEnclave, TEEEnclave};
use crate::types::*;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

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

    pub fn validate_order(&self, order: &Order) -> Result<Order, String> {
        let mut validated = order.clone();
        validated.dot_verified = true;
        Ok(validated)
    }

    pub fn execute_transfer(&self, tx: DOTTransfer) -> Result<DOTReceipt, String> {
        let sig = self.sign_transfer(&tx);

        let receipt = DOTReceipt {
            transfer_id: tx.id,
            from_user: tx.from_user.clone(),
            to_user: tx.to_user.clone(),
            currency: tx.currency.clone(),
            amount: tx.amount,
            status: DOTStatus::Settled,
            dot_signature: sig,
            settled_at: chrono::Utc::now().timestamp_millis(),
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

    fn sign_transfer(&self, tx: &DOTTransfer) -> String {
        let mut msg = Vec::with_capacity(64);
        msg.extend_from_slice(tx.id.to_string().as_bytes());
        msg.extend_from_slice(tx.from_user.to_string().as_bytes());
        msg.extend_from_slice(tx.to_user.to_string().as_bytes());
        msg.extend_from_slice(tx.currency.as_bytes());
        msg.extend_from_slice(&tx.amount.to_le_bytes());
        msg.extend_from_slice(&tx.timestamp.to_le_bytes());
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
