use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiatConfig {
    pub stripe_secret_key: String,
    pub stripe_webhook_secret: String,
    pub banxa_api_key: String,
    pub banxa_api_secret: String,
    pub banxa_webhook_secret: String,
    pub mode: FiatMode,
    pub default_currency: String,
    pub kyc_required: bool,
    pub stripe_base_url: String,
    pub banxa_base_url: String,
    pub request_timeout_secs: u64,
    pub card_ttl_days: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FiatMode {
    Test,
    Live,
}

impl Default for FiatConfig {
    fn default() -> Self {
        Self {
            stripe_secret_key: std::env::var("STRIPE_SECRET_KEY").unwrap_or_default(),
            stripe_webhook_secret: std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default(),
            banxa_api_key: std::env::var("BANXA_API_KEY").unwrap_or_default(),
            banxa_api_secret: std::env::var("BANXA_API_SECRET").unwrap_or_default(),
            banxa_webhook_secret: std::env::var("BANXA_WEBHOOK_SECRET").unwrap_or_default(),
            mode: FiatMode::Test,
            default_currency: "USD".to_string(),
            kyc_required: true,
            stripe_base_url: "https://api.stripe.com/v1".to_string(),
            banxa_base_url: "https://api.banxa.com".to_string(),
            request_timeout_secs: 30,
            card_ttl_days: 365,
        }
    }
}

impl FiatConfig {
    pub fn builder() -> FiatConfigBuilder {
        FiatConfigBuilder::default()
    }

    pub fn is_test(&self) -> bool {
        self.mode == FiatMode::Test
    }
}

#[derive(Debug, Clone, Default)]
pub struct FiatConfigBuilder {
    stripe_secret_key: Option<String>,
    stripe_webhook_secret: Option<String>,
    banxa_api_key: Option<String>,
    banxa_api_secret: Option<String>,
    banxa_webhook_secret: Option<String>,
    mode: Option<FiatMode>,
    default_currency: Option<String>,
    kyc_required: Option<bool>,
}

impl FiatConfigBuilder {
    pub fn stripe_keys(mut self, secret: impl Into<String>, webhook: impl Into<String>) -> Self {
        self.stripe_secret_key = Some(secret.into());
        self.stripe_webhook_secret = Some(webhook.into());
        self
    }

    pub fn banxa_keys(mut self, api_key: impl Into<String>, api_secret: impl Into<String>, webhook: impl Into<String>) -> Self {
        self.banxa_api_key = Some(api_key.into());
        self.banxa_api_secret = Some(api_secret.into());
        self.banxa_webhook_secret = Some(webhook.into());
        self
    }

    pub fn mode(mut self, mode: FiatMode) -> Self {
        self.mode = Some(mode);
        self
    }

    pub fn build(self) -> FiatConfig {
        let base = FiatConfig::default();
        FiatConfig {
            stripe_secret_key: self.stripe_secret_key.unwrap_or(base.stripe_secret_key),
            stripe_webhook_secret: self.stripe_webhook_secret.unwrap_or(base.stripe_webhook_secret),
            banxa_api_key: self.banxa_api_key.unwrap_or(base.banxa_api_key),
            banxa_api_secret: self.banxa_api_secret.unwrap_or(base.banxa_api_secret),
            banxa_webhook_secret: self.banxa_webhook_secret.unwrap_or(base.banxa_webhook_secret),
            mode: self.mode.unwrap_or(base.mode),
            default_currency: self.default_currency.unwrap_or(base.default_currency),
            kyc_required: self.kyc_required.unwrap_or(base.kyc_required),
            stripe_base_url: base.stripe_base_url.clone(),
            banxa_base_url: base.banxa_base_url.clone(),
            request_timeout_secs: base.request_timeout_secs,
            card_ttl_days: base.card_ttl_days,
        }
    }
}

pub fn request_timeout(config: &FiatConfig) -> Duration {
    Duration::from_secs(config.request_timeout_secs)
}

pub fn now() -> DateTime<Utc> {
    Utc::now()
}
