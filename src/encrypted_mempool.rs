use crate::threshold_crypto::{DecryptedOrder, EncryptedOrder, ThresholdCrypto, ZKProof};
use crate::types::{Order, OrderSide, OrderType, Track};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolOrder {
    pub encrypted_order: EncryptedOrder,
    pub received_at_ns: u64,
    pub priority_fee: u64,
    pub validator_receipts: Vec<ValidatorReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorReceipt {
    pub validator_id: usize,
    pub signature: Vec<u8>,
    pub timestamp_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchHeader {
    pub batch_id: String,
    pub batch_number: u64,
    pub timestamp_ns: u64,
    pub order_count: usize,
    pub clearing_price: u64,
    pub total_volume: u64,
    pub merkle_root: Vec<u8>,
    pub decryption_proof: ZKProof,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchAuctionResult {
    pub header: BatchHeader,
    pub matched_orders: Vec<MatchedOrder>,
    pub unmatched_orders: Vec<DecryptedOrder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedOrder {
    pub buy_order_id: String,
    pub sell_order_id: String,
    pub price: u64,
    pub quantity: u64,
    pub buyer_user_id: String,
    pub seller_user_id: String,
}

pub struct EncryptedMempool {
    crypto: Arc<ThresholdCrypto>,
    pending_orders: Arc<RwLock<BTreeMap<u64, Vec<MempoolOrder>>>>,
    batch_counter: Arc<RwLock<u64>>,
    batch_interval_ms: u64,
    max_batch_size: usize,
    validator_count: usize,
}

impl EncryptedMempool {
    pub fn new(crypto: Arc<ThresholdCrypto>, batch_interval_ms: u64, max_batch_size: usize) -> Self {
        Self {
            crypto,
            pending_orders: Arc::new(RwLock::new(BTreeMap::new())),
            batch_counter: Arc::new(RwLock::new(0)),
            batch_interval_ms,
            max_batch_size,
            validator_count: 3,
        }
    }

    pub async fn submit_encrypted_order(
        &self,
        encrypted_order: EncryptedOrder,
        priority_fee: u64,
    ) -> Result<String, String> {
        if !self.crypto.verify_encrypted_order(&encrypted_order) {
            return Err("Invalid encrypted order: ZK proof verification failed".into());
        }

        let mut receipts = Vec::new();
        for i in 0..self.validator_count {
            let mut sig = [0u8; 64];
            rand::thread_rng().fill(&mut sig);
            receipts.push(ValidatorReceipt {
                validator_id: i,
                signature: sig.to_vec(),
                timestamp_ns: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64,
            });
        }

        let mempool_order = MempoolOrder {
            encrypted_order: encrypted_order.clone(),
            received_at_ns: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64,
            priority_fee,
            validator_receipts: receipts,
        };

        let mut pending = self.pending_orders.write().await;
        let batch_key = self.get_current_batch_key().await;
        pending.entry(batch_key).or_default().push(mempool_order);

        Ok(encrypted_order.order_id)
    }

    pub fn encrypt_order(&self, order: &DecryptedOrder) -> Result<EncryptedOrder, String> {
        self.crypto.encrypt_order(order)
    }

    async fn get_current_batch_key(&self) -> u64 {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
        now / self.batch_interval_ms
    }

    pub async fn get_pending_count(&self) -> usize {
        let pending = self.pending_orders.read().await;
        pending.values().map(|v| v.len()).sum()
    }

    pub async fn get_batch_orders(&self, batch_number: u64) -> Vec<MempoolOrder> {
        let pending = self.pending_orders.read().await;
        pending.get(&batch_number).cloned().unwrap_or_default()
    }

    pub async fn finalize_batch(&self, batch_number: u64) -> Option<Vec<MempoolOrder>> {
        let mut pending = self.pending_orders.write().await;
        pending.remove(&batch_number)
    }

    pub async fn create_batch_auction(&self) -> Option<BatchAuctionResult> {
        let batch_number = {
            let mut counter = self.batch_counter.write().await;
            *counter += 1;
            *counter
        };

        let orders = self.finalize_batch(batch_number).await?;
        if orders.is_empty() {
            return None;
        }

        let encrypted_orders: Vec<_> = orders.iter().map(|o| o.encrypted_order.clone()).collect();

        let shares: Vec<_> = encrypted_orders.iter()
            .filter_map(|e| self.crypto.create_decryption_share(e))
            .collect();

        if shares.len() < self.crypto.get_params().threshold {
            return None;
        }

        let decrypted = self.crypto.combine_decryption_shares(&encrypted_orders[0], &shares)?;
        if decrypted.orders.is_empty() {
            return None;
        }

        let result = self.run_batch_auction(&decrypted.orders, batch_number).await;

        let mut counter = self.batch_counter.write().await;
        *counter = batch_number;

        Some(result)
    }

    async fn run_batch_auction(&self, orders: &[DecryptedOrder], batch_number: u64) -> BatchAuctionResult {
        let mut buy_orders: Vec<_> = orders.iter()
            .filter(|o| o.side == "buy")
            .cloned()
            .collect();
        let mut sell_orders: Vec<_> = orders.iter()
            .filter(|o| o.side == "sell")
            .cloned()
            .collect();

        buy_orders.sort_by(|a, b| b.price.cmp(&a.price));
        sell_orders.sort_by(|a, b| a.price.cmp(&b.price));

        let mut matched = Vec::new();
        let mut unmatched = Vec::new();

        let mut buy_idx = 0;
        let mut sell_idx = 0;

        while buy_idx < buy_orders.len() && sell_idx < sell_orders.len() {
            let buy = &buy_orders[buy_idx];
            let sell = &sell_orders[sell_idx];

            if buy.price >= sell.price {
                let clearing_price = (buy.price + sell.price) / 2;
                let qty = buy.quantity.min(sell.quantity);

                matched.push(MatchedOrder {
                    buy_order_id: buy.order_id.clone(),
                    sell_order_id: sell.order_id.clone(),
                    price: clearing_price,
                    quantity: qty,
                    buyer_user_id: buy.user_id.clone(),
                    seller_user_id: sell.user_id.clone(),
                });

                if buy.quantity > sell.quantity {
                    buy_orders[buy_idx].quantity -= qty;
                    sell_idx += 1;
                } else if sell.quantity > buy.quantity {
                    sell_orders[sell_idx].quantity -= qty;
                    buy_idx += 1;
                } else {
                    buy_idx += 1;
                    sell_idx += 1;
                }
            } else {
                break;
            }
        }

        for o in buy_orders.iter().skip(buy_idx) {
            unmatched.push(o.clone());
        }
        for o in sell_orders.iter().skip(sell_idx) {
            unmatched.push(o.clone());
        }

        let total_volume: u64 = matched.iter().map(|m| m.quantity).sum();
        let clearing_price = if !matched.is_empty() {
            matched.last().unwrap().price
        } else {
            0
        };

        let merkle_root = self.compute_merkle_root(&matched);

        let header = BatchHeader {
            batch_id: format!("batch_{}", batch_number),
            batch_number,
            timestamp_ns: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64,
            order_count: orders.len(),
            clearing_price,
            total_volume,
            merkle_root,
            decryption_proof: ZKProof { challenge: vec![], response: vec![], commitment: vec![] },
        };

        BatchAuctionResult { header, matched_orders: matched, unmatched_orders: unmatched }
    }

    fn compute_merkle_root(&self, matched: &[MatchedOrder]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        for m in matched {
            let data = format!("{}{}{}{}{}", m.buy_order_id, m.sell_order_id, m.price, m.quantity, m.buyer_user_id);
            hasher.update(data.as_bytes());
        }
        hasher.finalize().to_vec()
    }
}

pub fn decrypted_to_order(decrypted: &DecryptedOrder) -> Order {
    Order {
        id: uuid::Uuid::parse_str(&decrypted.order_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
        id_tag: 0,
        user_id: uuid::Uuid::parse_str(&decrypted.user_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
        pair: decrypted.pair.clone().into(),
        order_type: OrderType::Limit,
        side: if decrypted.side == "buy" { OrderSide::Buy } else { OrderSide::Sell },
        price: decrypted.price as f64 / 10000.0,
        quantity: decrypted.quantity as f64 / 10000.0,
        filled: 0.0,
        remaining: decrypted.quantity as f64 / 10000.0,
        status: crate::types::OrderStatus::New,
        timestamp: decrypted.nonce as i64,
        ttl_ms: None,
        is_swap: false,
        swap_target_currency: None,
        tee_signed: false,
        dot_verified: false,
        stealth: true,
        trailing_offset: None,
        trigger_price: None,
        hard_floor: None,
        track: if decrypted.track == 1 { Track::Autonomous } else { Track::Compliant },
        style: crate::types::OrderStyle::Standard,
        hidden_remaining: 0.0,
        filled_quantity: 0,
        client_order_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threshold_crypto::ThresholdCrypto;

    #[tokio::test]
    async fn test_mempool_submit_and_batch() {
        let crypto = Arc::new(ThresholdCrypto::new(2, 3).unwrap());
        crypto.run_dkg(0);
        crypto.run_dkg(1);

        let mempool = EncryptedMempool::new(crypto.clone(), 100, 1000);

        let order = DecryptedOrder {
            order_id: "test_1".into(),
            user_id: "user_1".into(),
            pair: "USD/EGP".into(),
            side: "buy".into(),
            price: 305000,
            quantity: 100000,
            track: 1,
            nonce: 1,
        };

        let encrypted = mempool.crypto.encrypt_order(&order).unwrap();
        let result = mempool.submit_encrypted_order(encrypted, 1000).await;
        assert!(result.is_ok());

        let count = mempool.get_pending_count().await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_batch_auction_matching() {
        let crypto = Arc::new(ThresholdCrypto::new(2, 3).unwrap());
        crypto.run_dkg(0);
        crypto.run_dkg(1);

        let mempool = EncryptedMempool::new(crypto.clone(), 100, 1000);

        let orders = vec![
            DecryptedOrder { order_id: "b1".into(), user_id: "u1".into(), pair: "USD/EGP".into(), side: "buy".into(), price: 310000, quantity: 100000, track: 1, nonce: 1 },
            DecryptedOrder { order_id: "b2".into(), user_id: "u2".into(), pair: "USD/EGP".into(), side: "buy".into(), price: 305000, quantity: 200000, track: 1, nonce: 2 },
            DecryptedOrder { order_id: "s1".into(), user_id: "u3".into(), pair: "USD/EGP".into(), side: "sell".into(), price: 300000, quantity: 150000, track: 1, nonce: 3 },
            DecryptedOrder { order_id: "s2".into(), user_id: "u4".into(), pair: "USD/EGP".into(), side: "sell".into(), price: 295000, quantity: 100000, track: 1, nonce: 4 },
        ];

        let result = mempool.run_batch_auction(&orders, 1).await;

        assert!(!result.matched_orders.is_empty());
        let total_qty: u64 = result.matched_orders.iter().map(|m| m.quantity).sum();
        assert!(total_qty > 0);
    }
}