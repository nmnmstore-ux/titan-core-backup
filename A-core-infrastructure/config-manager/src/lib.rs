#![warn(missing_docs)]

//! Config Manager - Centralized configuration management for THE-BRIDGE matching engine
//!
//! This crate provides:
//! - Encrypted configuration storage (AES-256-GCM)
//! - Multi-network support (Ethereum, Arbitrum, Sepolia, Optimism, Base, Polygon, Solana)
//! - Multi-provider support (Alchemy, QuickNode, Flashbots, Binance, Coinbase, OKX)
//! - RESTful API endpoints for live management
//! - Secure key rotation
//!
//! يجب تحميل جميع مفاتيح API والأسرار عبر هذا النظام الموحد

use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use serde::Deserialize as _;

fn serialize_secret<S: serde::Serializer>(s: &SecretString, ser: S) -> Result<S::Ok, S::Error> {
    ser.serialize_str(s.expose_secret())
}

fn serialize_opt_secret<S: serde::Serializer>(s: &Option<SecretString>, ser: S) -> Result<S::Ok, S::Error> {
    match s {
        Some(v) => serialize_secret(v, ser),
        None => ser.serialize_none(),
    }
}

fn deserialize_secret<'de, D: serde::Deserializer<'de>>(de: D) -> Result<SecretString, D::Error> {
    let s: String = serde::Deserialize::deserialize(de)?;
    Ok(SecretString::from(s))
}

fn deserialize_opt_secret<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Option<SecretString>, D::Error> {
    Ok(Option::<String>::deserialize(de)?.map(SecretString::from))
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to encrypt data: {0}")]
    EncryptionError(String),
    #[error("Failed to decrypt data: {0}")]
    DecryptionError(String),
    #[error("Configuration file not found: {0}")]
    FileNotFound(String),
    #[error("Failed to parse configuration: {0}")]
    ParseError(String),
    #[error("Failed to write configuration: {0}")]
    WriteError(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NetworkConfig {
    pub name: String,
    pub chain_id: u64,
    #[serde(serialize_with = "serialize_secret", deserialize_with = "deserialize_secret")]
    pub rpc_url: SecretString,
    #[serde(default, serialize_with = "serialize_opt_secret", deserialize_with = "deserialize_opt_secret")]
    pub gas_sponsorship_id: Option<SecretString>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub provider_type: ProviderType,
    #[serde(default, serialize_with = "serialize_opt_secret", deserialize_with = "deserialize_opt_secret")]
    pub api_key: Option<SecretString>,
    #[serde(default, serialize_with = "serialize_opt_secret", deserialize_with = "deserialize_opt_secret")]
    pub api_secret: Option<SecretString>,
    pub networks: Vec<String>, // network names this provider supports
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ProviderType {
    Alchemy,
    QuickNode,
    Flashbots,
    Binance,
    Coinbase,
    OKX,
    Custom(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EngineConfig {
    pub mev_enabled: bool,
    pub mev_min_profit_usd: f64,
    pub mev_scan_interval_ms: u64,
    pub flash_loan_enabled: bool,
    pub flash_loan_scan_interval_ms: u64,
    pub cross_venue_enabled: bool,
    pub cross_venue_scan_interval_ms: u64,
    pub cross_venue_min_profit_usd: f64,
    pub super_arb_enabled: bool,
    pub super_arb_scan_interval_ms: u64,
    pub super_arb_min_profit_usd: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub version: String,
    pub networks: std::collections::HashMap<String, NetworkConfig>,
    pub providers: std::collections::HashMap<String, ProviderConfig>,
    pub engine: EngineConfig,
}

pub mod encryption {
    use super::*;
    use aes_gcm::{Aes256Gcm, Nonce};
    use aes_gcm::aead::{Aead, KeyInit};
    use base64::{Engine as _, engine::general_purpose};
    use secrecy::Secret;
    use std::fs;

    /// encrypts config to file using AES-256-GCM
    pub fn encrypt_config(config: &Config, path: &str, master_key: &str) -> Result<(), ConfigError> {
        let json = serde_json::to_string(config)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        let cipher = Aes256Gcm::new_from_slice(master_key.as_bytes())
            .map_err(|e| ConfigError::EncryptionError(e.to_string()))?;

        let nonce_bytes = rand::random::<[u8; 12]>();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let encrypted = cipher.encrypt(nonce, json.as_bytes())
            .map_err(|e| ConfigError::EncryptionError(e.to_string()))?;

        let combined = [nonce_bytes.as_slice(), &encrypted].concat();
        let encoded = general_purpose::STANDARD.encode(combined);

        fs::write(path, encoded)
            .map_err(|e| ConfigError::WriteError(e.to_string()))?;

        Ok(())
    }

    /// decrypts config from file using AES-256-GCM
    pub fn decrypt_config(path: &str, master_key: &str) -> Result<Config, ConfigError> {
        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::FileNotFound(e.to_string()))?;

        let decoded = general_purpose::STANDARD.decode(content)
            .map_err(|e| ConfigError::DecryptionError(e.to_string()))?;

        if decoded.len() < 12 {
            return Err(ConfigError::DecryptionError("Invalid encrypted data".to_string()));
        }

        let (nonce_bytes, encrypted) = decoded.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(master_key.as_bytes())
            .map_err(|e| ConfigError::DecryptionError(e.to_string()))?;

        let decrypted = cipher.decrypt(nonce, encrypted.as_ref())
            .map_err(|e| ConfigError::DecryptionError(e.to_string()))?;

        let json = String::from_utf8(decrypted)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        serde_json::from_str(&json)
            .map_err(|e| ConfigError::ParseError(e.to_string()))
    }
}

pub mod api {
    use super::*;
    use axum::{extract::State, Json, response::IntoResponse, routing::post, Router};
    use std::sync::Arc;

    impl IntoResponse for ConfigError {
        fn into_response(self) -> axum::response::Response {
            let status = match &self {
                ConfigError::FileNotFound(_) => axum::http::StatusCode::NOT_FOUND,
                ConfigError::ParseError(_) => axum::http::StatusCode::BAD_REQUEST,
                _ => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, self.to_string()).into_response()
        }
    }

    /// axum state
    #[derive(Clone)]
    pub struct AppState {
        pub config_path: String,
        pub master_key: SecretString,
    }

    /// load config from encrypted file
    async fn load_config(
        State(state): State<AppState>
    ) -> Result<impl IntoResponse, ConfigError> {
        let config = encryption::decrypt_config(
            &state.config_path,
            state.master_key.expose_secret()
        )?;

        Ok(Json(config).into_response())
    }

    /// update config
    async fn update_config(
        State(state): State<AppState>,
        Json(config): Json<Config>
    ) -> Result<impl IntoResponse, ConfigError> {
        encryption::encrypt_config(
            &config,
            &state.config_path,
            state.master_key.expose_secret()
        )?;

        Ok("Configuration updated successfully".into_response())
    }

    /// rotate a specific key (placeholder for key rotation)
    async fn rotate_key(
        State(state): State<AppState>,
        Json(request): Json<KeyRotationRequest>
    ) -> Result<impl IntoResponse, ConfigError> {
        let mut config = encryption::decrypt_config(
            &state.config_path,
            state.master_key.expose_secret()
        )?;

        match request.provider_type {
            ProviderType::Alchemy => {
                if let Some(provider) = config.providers.get_mut(&request.provider_name) {
                    if let Some(ref mut api_key) = provider.api_key {
                        *api_key = SecretString::from(generate_new_api_key());
                    }
                }
            },
            ProviderType::Binance => {
                if let Some(provider) = config.providers.get_mut(&request.provider_name) {
                    if let Some(ref mut api_key) = provider.api_key {
                        *api_key = SecretString::from(generate_new_api_key());
                    }
                    if let Some(ref mut api_secret) = provider.api_secret {
                        *api_secret = SecretString::from(generate_new_secret());
                    }
                }
            },
            _ => return Err(ConfigError::ParseError("Unsupported provider for rotation".to_string())),
        }

        encryption::encrypt_config(
            &config,
            &state.config_path,
            state.master_key.expose_secret()
        )?;

        Ok("Key rotated successfully".into_response())
    }

    /// key rotation request
    #[derive(Debug, serde::Deserialize)]
    struct KeyRotationRequest {
        provider_type: ProviderType,
        provider_name: String,
    }

    /// generate new api key (mock)
    fn generate_new_api_key() -> String {
        use rand::distributions::Alphanumeric;
        use rand::{thread_rng, Rng};
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect()
    }

    /// generate new secret (mock)
    fn generate_new_secret() -> String {
        use rand::distributions::Alphanumeric;
        use rand::{thread_rng, Rng};
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(64)
            .map(char::from)
            .collect()
    }

    /// build router
    pub fn router(config_path: String, master_key: SecretString) -> Router {
        let state = AppState { config_path, master_key };

        Router::new()
            .route("/api/v1/config/load", post(load_config))
            .route("/api/v1/config/update", post(update_config))
            .route("/api/v1/config/rotate", post(rotate_key))
            .with_state(state)
    }
}
