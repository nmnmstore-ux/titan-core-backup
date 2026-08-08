use crate::{FiatConfig, FiatError, Result};
use reqwest::Client;
use hmac::{Hmac, Mac};
use sha2::Sha512;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, instrument};

type HmacSha512 = Hmac<Sha512>;

#[derive(Clone)]
pub struct BanxaClient {
    config: FiatConfig,
    client: Client,
}

impl BanxaClient {
    pub fn new(config: FiatConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .build()
            .unwrap_or_default();
        Self { config, client }
    }

    pub async fn health(&self) -> bool {
        let url = format!("{}/api/v1/ping", self.config.banxa_base_url);
        self.client
            .get(&url)
            .header("X-API-Key", &self.config.banxa_api_key)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn build_signature(&self, method: &str, endpoint: &str, body: &str, timestamp: u64) -> String {
        let payload = format!("{}|{}|{}|{}|{}", method, endpoint, body, timestamp, "");
        let mut mac = HmacSha512::new_from_slice(self.config.banxa_api_secret.as_bytes())
            .unwrap_or_else(|_| HmacSha512::new_from_slice(b"").unwrap());
        mac.update(payload.as_bytes());
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }

    fn get_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    #[instrument(skip(self))]
    pub async fn create_order(
        &self,
        user_id: &str,
        amount: &str,
        currency: &str,
        crypto_currency: &str,
        type_: &str,
    ) -> Result<BanxaOrder> {
        debug!("creating banxa order for user {}", user_id);
        let timestamp = Self::get_timestamp();
        let body_str = serde_json::json!({
            "currency": currency,
            "amount": amount,
            "crypto_asset": crypto_currency,
            "type": type_,
            "metadata": {
                "user_id": user_id,
            },
        }).to_string();

        let endpoint = "/api/v1/orders";
        let signature = self.build_signature("POST", endpoint, &body_str, timestamp);

        let resp = self
            .client
            .post(&format!("{}{}", self.config.banxa_base_url, endpoint))
            .header("X-API-Key", &self.config.banxa_api_key)
            .header("X-Signature", &signature)
            .header("X-Timestamp", timestamp.to_string())
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await
            .map_err(|e| FiatError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(FiatError::External(format!(
                "banxa order creation failed: {} - {}",
                status,
                body
            )));
        }

        let data: BanxaOrder = resp
            .json()
            .await
            .map_err(|e| FiatError::Serialization(e.to_string()))?;
        Ok(data)
    }

    pub async fn get_order_status(&self, order_id: &str) -> Result<BanxaOrder> {
        let timestamp = Self::get_timestamp();
        let endpoint = format!("/api/v1/orders/{}", order_id);
        let signature = self.build_signature("GET", &endpoint, "", timestamp);

        let resp = self
            .client
            .get(&format!("{}{}", self.config.banxa_base_url, endpoint))
            .header("X-API-Key", &self.config.banxa_api_key)
            .header("X-Signature", &signature)
            .header("X-Timestamp", timestamp.to_string())
            .send()
            .await
            .map_err(|e| FiatError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(FiatError::External(format!(
                "banxa order status failed: {}",
                resp.status()
            )));
        }

        resp.json::<BanxaOrder>()
            .await
            .map_err(|e| FiatError::Serialization(e.to_string()))
    }

    pub async fn get_pricing(
        &self,
        currency: &str,
        crypto_currency: &str,
        type_: &str,
        amount: &str,
    ) -> Result<BanxaPrice> {
        let timestamp = Self::get_timestamp();
        let endpoint = format!(
            "/api/v1/pricing?currency={}&crypto_asset={}&type={}&amount={}",
            currency, crypto_currency, type_, amount
        );
        let signature = self.build_signature("GET", &endpoint, "", timestamp);

        let resp = self
            .client
            .get(&format!("{}{}", self.config.banxa_base_url, endpoint))
            .header("X-API-Key", &self.config.banxa_api_key)
            .header("X-Signature", &signature)
            .header("X-Timestamp", timestamp.to_string())
            .send()
            .await
            .map_err(|e| FiatError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(FiatError::External(format!(
                "banxa pricing failed: {}",
                resp.status()
            )));
        }

        resp.json::<BanxaPrice>()
            .await
            .map_err(|e| FiatError::Serialization(e.to_string()))
    }

    pub async fn get_fiat_currencies(&self) -> Result<Vec<BanxaCurrency>> {
        let resp = self
            .client
            .get(&format!("{}/api/v1/fiat_currencies", self.config.banxa_base_url))
            .header("X-API-Key", &self.config.banxa_api_key)
            .send()
            .await
            .map_err(|e| FiatError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(FiatError::External("banxa currency list failed".to_string()));
        }

        let data: BanxaCurrencyList = resp
            .json()
            .await
            .map_err(|e| FiatError::Serialization(e.to_string()))?;
        Ok(data.currencies)
    }

    pub async fn get_crypto_currencies(&self) -> Result<Vec<BanxaCurrency>> {
        let resp = self
            .client
            .get(&format!("{}/api/v1/crypto_currencies", self.config.banxa_base_url))
            .header("X-API-Key", &self.config.banxa_api_key)
            .send()
            .await
            .map_err(|e| FiatError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(FiatError::External("banxa crypto list failed".to_string()));
        }

        let data: BanxaCurrencyList = resp
            .json()
            .await
            .map_err(|e| FiatError::Serialization(e.to_string()))?;
        Ok(data.currencies)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BanxaOrder {
    pub id: String,
    pub order_id: String,
    pub status: String,
    pub amount: String,
    pub currency: String,
    pub crypto_asset: String,
    pub type_: String,
    pub rate: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BanxaPrice {
    pub rate: String,
    pub currency: String,
    pub crypto_asset: String,
    pub amount: String,
    pub crypto_amount: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BanxaCurrency {
    pub currency: String,
    pub name: String,
    pub min_limit: String,
    pub max_limit: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BanxaCurrencyList {
    currencies: Vec<BanxaCurrency>,
}
