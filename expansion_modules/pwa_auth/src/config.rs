use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub rp_id: String,
    pub rp_name: String,
    pub origin: String,
    pub jwt_secret: Vec<u8>,
    pub token_ttl_seconds: u64,
    pub refresh_ttl_seconds: u64,
    pub challenge_ttl_seconds: u64,
    pub allowed_origins: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            rp_id: "thebridge.local".to_string(),
            rp_name: "THE-BRIDGE".to_string(),
            origin: "https://app.thebridge.io".to_string(),
            jwt_secret: vec![0u8; 32],
            token_ttl_seconds: 900,
            refresh_ttl_seconds: 604800,
            challenge_ttl_seconds: 30,
            allowed_origins: vec![
                "https://app.thebridge.io".to_string(),
                "https://staging.thebridge.io".to_string(),
            ],
        }
    }
}

impl AuthConfig {
    pub fn builder() -> AuthConfigBuilder {
        AuthConfigBuilder::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuthConfigBuilder {
    rp_id: Option<String>,
    rp_name: Option<String>,
    origin: Option<String>,
    jwt_secret: Option<Vec<u8>>,
    token_ttl_seconds: Option<u64>,
    refresh_ttl_seconds: Option<u64>,
    challenge_ttl_seconds: Option<u64>,
    allowed_origins: Option<Vec<String>>,
}

impl AuthConfigBuilder {
    pub fn rp_id(mut self, id: impl Into<String>) -> Self {
        self.rp_id = Some(id.into());
        self
    }

    pub fn rp_name(mut self, name: impl Into<String>) -> Self {
        self.rp_name = Some(name.into());
        self
    }

    pub fn origin(mut self, origin: impl Into<String>) -> Self {
        self.origin = Some(origin.into());
        self
    }

    pub fn jwt_secret(mut self, secret: Vec<u8>) -> Self {
        self.jwt_secret = Some(secret);
        self
    }

    pub fn token_ttl(mut self, secs: u64) -> Self {
        self.token_ttl_seconds = Some(secs);
        self
    }

    pub fn challenge_ttl(mut self, secs: u64) -> Self {
        self.challenge_ttl_seconds = Some(secs);
        self
    }

    pub fn allowed_origins(mut self, origins: Vec<String>) -> Self {
        self.allowed_origins = Some(origins);
        self
    }

    pub fn build(self) -> AuthConfig {
        let base = AuthConfig::default();
        AuthConfig {
            rp_id: self.rp_id.unwrap_or(base.rp_id),
            rp_name: self.rp_name.unwrap_or(base.rp_name),
            origin: self.origin.unwrap_or(base.origin),
            jwt_secret: self.jwt_secret.unwrap_or(base.jwt_secret),
            token_ttl_seconds: self.token_ttl_seconds.unwrap_or(base.token_ttl_seconds),
            refresh_ttl_seconds: self.refresh_ttl_seconds.unwrap_or(base.refresh_ttl_seconds),
            challenge_ttl_seconds: self.challenge_ttl_seconds.unwrap_or(base.challenge_ttl_seconds),
            allowed_origins: self.allowed_origins.unwrap_or(base.allowed_origins),
        }
    }
}

pub fn default_challenge_ttl() -> Duration {
    Duration::from_secs(30)
}

pub fn default_token_ttl() -> Duration {
    Duration::from_secs(900)
}
