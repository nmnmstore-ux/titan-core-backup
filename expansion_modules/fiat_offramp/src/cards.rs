use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::models::{CardStatus, TransactionType};
use crate::{FiatConfig, FiatError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualCard {
    pub id: String,
    pub user_id: String,
    pub wallet_address: String,
    pub chain_id: String,
    pub stripe_card_id: String,
    pub masked_pan: String,
    pub brand: String,
    pub currency: String,
    pub status: CardStatus,
    pub token: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub idempotency_key: String,
}

impl VirtualCard {
    pub fn new(
        user_id: &str,
        wallet_address: &str,
        chain_id: &str,
        stripe_card_id: &str,
        masked_pan: &str,
        brand: &str,
        currency: &str,
        ttl_days: u32,
        token: String,
        idempotency_key: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            wallet_address: wallet_address.to_string(),
            chain_id: chain_id.to_string(),
            stripe_card_id: stripe_card_id.to_string(),
            masked_pan: masked_pan.to_string(),
            brand: brand.to_string(),
            currency: currency.to_string(),
            status: CardStatus::Pending,
            token,
            created_at: now,
            expires_at: now + Duration::days(ttl_days as i64),
            idempotency_key,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn is_active(&self) -> bool {
        self.status == CardStatus::Active && !self.is_expired()
    }

    pub fn activate(&mut self) {
        self.status = CardStatus::Active;
    }

    pub fn freeze(&mut self) {
        self.status = CardStatus::Frozen;
    }

    pub fn cancel(&mut self) {
        self.status = CardStatus::Cancelled;
    }
}

pub struct CardManager {
    config: FiatConfig,
    cards: Arc<tokio::sync::RwLock<HashMap<String, VirtualCard>>>,
    card_by_token: Arc<tokio::sync::RwLock<HashMap<String, String>>>,
    card_by_wallet: Arc<tokio::sync::RwLock<HashMap<String, Vec<String>>>>,
}

impl CardManager {
    pub fn new(config: FiatConfig) -> Self {
        Self {
            config,
            cards: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            card_by_token: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            card_by_wallet: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    pub async fn provision_card(
        &self,
        user_id: &str,
        wallet_address: &str,
        chain_id: &str,
        currency: &str,
        idempotency_key: &str,
    ) -> Result<VirtualCard> {
        if let Some(existing) = self.find_by_idempotency(idempotency_key).await {
            return Ok(existing);
        }

        let stripe_card_id = format!("card_{}", Uuid::new_v4());
        let masked_pan = "****4242".to_string();
        let brand = "Visa".to_string();
        let token = format!("token_{}", Uuid::new_v4());

        let card = VirtualCard::new(
            user_id,
            wallet_address,
            chain_id,
            &stripe_card_id,
            &masked_pan,
            &brand,
            currency,
            self.config.card_ttl_days,
            token.clone(),
            idempotency_key.to_string(),
        );

        self.cards
            .write()
            .await
            .insert(card.id.clone(), card.clone());
        self.card_by_token
            .write()
            .await
            .insert(token.clone(), card.id.clone());
        self.card_by_wallet
            .write()
            .await
            .entry(wallet_address.to_string())
            .or_default()
            .push(card.id.clone());

        tracing::info!(
            user_id = user_id,
            card_id = card.id,
            "Virtual card provisioned",
        );

        Ok(card)
    }

    pub async fn get_card(&self, card_id: &str) -> Option<VirtualCard> {
        self.cards.read().await.get(card_id).cloned()
    }

    pub async fn get_card_by_token(&self, token: &str) -> Option<VirtualCard> {
        let card_id = self.card_by_token.read().await.get(token).cloned();
        if let Some(id) = card_id {
            self.cards.read().await.get(&id).cloned()
        } else {
            None
        }
    }

    pub async fn get_cards_for_wallet(&self, wallet_address: &str) -> Vec<VirtualCard> {
        let card_ids = self.card_by_wallet.read().await
            .get(wallet_address)
            .cloned()
            .unwrap_or_default();

        let cards = self.cards.read().await;
        card_ids
            .iter()
            .filter_map(|id| cards.get(id).cloned())
            .collect()
    }

    pub async fn activate_card(&self, card_id: &str, user_id: &str) -> Result<VirtualCard> {
        let mut cards = self.cards.write().await;
        let card = cards
            .get_mut(card_id)
            .ok_or_else(|| FiatError::NotFound(card_id.to_string()))?;

        if card.user_id != user_id {
            return Err(FiatError::PermissionDenied(
                "card does not belong to user".to_string(),
            ));
        }

        card.activate();
        Ok(card.clone())
    }

    pub async fn freeze_card(&self, card_id: &str) -> Result<VirtualCard> {
        let mut cards = self.cards.write().await;
        let card = cards
            .get_mut(card_id)
            .ok_or_else(|| FiatError::NotFound(card_id.to_string()))?;
        card.freeze();
        Ok(card.clone())
    }

    pub async fn cancel_card(&self, card_id: &str) -> Result<VirtualCard> {
        let mut cards = self.cards.write().await;
        let card = cards
            .get_mut(card_id)
            .ok_or_else(|| FiatError::NotFound(card_id.to_string()))?;
        card.cancel();
        Ok(card.clone())
    }

    pub async fn record_transaction(
        &self,
        card_id: &str,
        amount: impl Into<rust_decimal::Decimal>,
        transaction_type: TransactionType,
        external_ref: &str,
    ) -> Result<()> {
        let cards = self.cards.read().await;
        let _card = cards
            .get(card_id)
            .ok_or_else(|| FiatError::NotFound(card_id.to_string()))?;

        tracing::info!(
            card_id = card_id,
            amount = %amount.into(),
            tx_type = ?transaction_type,
            external_ref = external_ref,
            "Card transaction recorded",
        );

        Ok(())
    }

    async fn find_by_idempotency(&self, key: &str) -> Option<VirtualCard> {
        let cards = self.cards.read().await;
        cards
            .values()
            .find(|c| c.idempotency_key == key)
            .cloned()
    }
}
