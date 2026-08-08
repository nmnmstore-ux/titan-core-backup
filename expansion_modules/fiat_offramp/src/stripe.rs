use crate::{FiatConfig, FiatError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, instrument};

#[derive(Clone)]
pub struct StripeClient {
    config: FiatConfig,
    client: Client,
}

impl StripeClient {
    pub fn new(config: FiatConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .build()
            .unwrap_or_default();
        Self { config, client }
    }

    pub async fn health(&self) -> bool {
        let url = format!("{}/accounts", self.config.stripe_base_url);
        self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.stripe_secret_key))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    #[instrument(skip(self))]
    pub async fn create_cardholder(
        &self,
        user_id: &str,
    ) -> Result<StripeCardholder> {
        debug!("creating stripe cardholder for user {}", user_id);
        let resp = self
            .client
            .post(&format!("{}/customers", self.config.stripe_base_url))
            .bearer_auth(&self.config.stripe_secret_key)
            .form(&serde_json::json!({
                "email": format!("user-{}@thebridge.io", user_id),
                "name": format!("THE-BRIDGE User {}", user_id),
                "description": "Web3 Card User",
            }))
            .send()
            .await
            .map_err(|e| FiatError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(FiatError::External(format!(
                "stripe cardholder creation failed: {} - {}",
                status,
                body
            )));
        }

        let data: StripeCardholder = resp
            .json()
            .await
            .map_err(|e| FiatError::Serialization(e.to_string()))?;
        Ok(data)
    }

    pub async fn create_card(
        &self,
        cardholder_id: &str,
        user_id: &str,
        currency: &str,
    ) -> Result<StripeCard> {
        debug!("creating stripe virtual card for cardholder {}", cardholder_id);
        let resp = self
            .client
            .post(&format!("{}/issuing/cards", self.config.stripe_base_url))
            .bearer_auth(&self.config.stripe_secret_key)
            .form(&serde_json::json!({
                "cardholder": cardholder_id,
                "type": "virtual",
                "currency": currency,
                "status": "active",
                "metadata": {
                    "user_id": user_id,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                },
            }))
            .send()
            .await
            .map_err(|e| FiatError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(FiatError::External(format!(
                "stripe card creation failed: {} - {}",
                status,
                body
            )));
        }

        let data: StripeCard = resp
            .json()
            .await
            .map_err(|e| FiatError::Serialization(e.to_string()))?;
        Ok(data)
    }

    pub async fn get_card_status(&self, card_id: &str) -> Result<StripeCard> {
        let resp = self
            .client
            .get(&format!(
                "{}/issuing/cards/{}",
                self.config.stripe_base_url, card_id
            ))
            .bearer_auth(&self.config.stripe_secret_key)
            .send()
            .await
            .map_err(|e| FiatError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(FiatError::External(format!(
                "stripe card status failed: {}",
                resp.status()
            )));
        }

        resp.json::<StripeCard>()
            .await
            .map_err(|e| FiatError::Serialization(e.to_string()))
    }

    pub async fn activate_card(&self, card_id: &str) -> Result<StripeCard> {
        self.update_card(card_id, "active").await
    }

    pub async fn freeze_card(&self, card_id: &str) -> Result<StripeCard> {
        self.update_card(card_id, "inactive").await
    }

    pub async fn cancel_card(&self, card_id: &str) -> Result<StripeCard> {
        self.update_card(card_id, "canceled").await
    }

    pub async fn create_payout(
        &self,
        amount: i64,
        currency: &str,
        destination: &str,
        card_id: Option<&str>,
    ) -> Result<StripePayout> {
        let mut payload = serde_json::json!({
            "amount": amount,
            "currency": currency,
        });

        if let Some(card) = card_id {
            payload["card"] = serde_json::json!(card);
        } else {
            payload["destination_payment"] = serde_json::json!(destination);
        }

        let resp = self
            .client
            .post(&format!("{}/payouts", self.config.stripe_base_url))
            .bearer_auth(&self.config.stripe_secret_key)
            .form(&payload)
            .send()
            .await
            .map_err(|e| FiatError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(FiatError::External(format!(
                "stripe payout failed: {} - {}",
                status,
                body
            )));
        }

        let data: StripePayout = resp
            .json()
            .await
            .map_err(|e| FiatError::Serialization(e.to_string()))?;
        Ok(data)
    }

    pub async fn get_payout_status(&self, payout_id: &str) -> Result<StripePayout> {
        let resp = self
            .client
            .get(&format!(
                "{}/payouts/{}",
                self.config.stripe_base_url, payout_id
            ))
            .bearer_auth(&self.config.stripe_secret_key)
            .send()
            .await
            .map_err(|e| FiatError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(FiatError::External(format!(
                "stripe payout status failed: {}",
                resp.status()
            )));
        }

        resp.json::<StripePayout>()
            .await
            .map_err(|e| FiatError::Serialization(e.to_string()))
    }

    async fn update_card(&self, card_id: &str, status: &str) -> Result<StripeCard> {
        let resp = self
            .client
            .post(&format!(
                "{}/issuing/cards/{}",
                self.config.stripe_base_url, card_id
            ))
            .bearer_auth(&self.config.stripe_secret_key)
            .form(&serde_json::json!({ "status": status }))
            .send()
            .await
            .map_err(|e| FiatError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(FiatError::External(format!(
                "stripe card update failed: {}",
                resp.status()
            )));
        }

        resp.json::<StripeCard>()
            .await
            .map_err(|e| FiatError::Serialization(e.to_string()))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StripeCardholder {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub status: String,
    pub created: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StripeCard {
    pub id: String,
    pub cardholder: String,
    pub last4: Option<String>,
    pub brand: Option<String>,
    pub funding: String,
    pub status: String,
    pub currency: String,
    pub wallet_address: Option<String>,
    #[serde(default)]
    pub pan: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StripePayout {
    pub id: String,
    pub amount: i64,
    pub currency: String,
    pub status: String,
    pub created: i64,
    pub description: Option<String>,
}
