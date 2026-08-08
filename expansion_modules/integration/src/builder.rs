use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::server::ExpansionServer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub http_bind: SocketAddr,
    pub grpc_bind: SocketAddr,
    pub tracing_filter: String,
    pub request_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            http_bind: "0.0.0.0:8080".parse().unwrap(),
            grpc_bind: "0.0.0.0:9090".parse().unwrap(),
            tracing_filter: "info".to_string(),
            request_timeout_secs: 30,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExpansionBuilder {
    config: ServerConfig,
    ai_ceo_enabled: bool,
    auth_enabled: bool,
    fiat_enabled: bool,
    ai_ceo_config: Option<ai_ceo_bridge::BridgeConfig>,
    auth_config: Option<pwa_auth::AuthConfig>,
    fiat_config: Option<fiat_offramp::FiatConfig>,
    enable_compression: bool,
    enable_tracing: bool,
}

impl ExpansionBuilder {
    pub fn new() -> Self {
        Self {
            config: ServerConfig::default(),
            ai_ceo_enabled: false,
            auth_enabled: false,
            fiat_enabled: false,
            ai_ceo_config: None,
            auth_config: None,
            fiat_config: None,
            enable_compression: true,
            enable_tracing: true,
        }
    }

    pub fn with_http(mut self, addr: SocketAddr) -> Self {
        self.config.http_bind = addr;
        self
    }

    pub fn with_grpc(mut self, addr: SocketAddr) -> Self {
        self.config.grpc_bind = addr;
        self
    }

    pub fn with_ai_ceo(mut self, config: ai_ceo_bridge::BridgeConfig) -> Self {
        self.ai_ceo_enabled = true;
        self.ai_ceo_config = Some(config);
        self
    }

    pub fn with_auth(mut self, config: pwa_auth::AuthConfig) -> Self {
        self.auth_enabled = true;
        self.auth_config = Some(config);
        self
    }

    pub fn with_fiat(mut self, config: fiat_offramp::FiatConfig) -> Self {
        self.fiat_enabled = true;
        self.fiat_config = Some(config);
        self
    }

    pub fn with_tracing(mut self, filter: &str) -> Self {
        self.config.tracing_filter = filter.to_string();
        self
    }

    pub fn disable_compression(mut self) -> Self {
        self.enable_compression = false;
        self
    }

    pub fn build(self) -> ExpansionServer {
        let mut ai_ceo_state: Option<ai_ceo_bridge::AppState> = None;
        if self.ai_ceo_enabled {
            let config = self.ai_ceo_config.unwrap_or_default();
            ai_ceo_state = Some(ai_ceo_bridge::routes::build_state(config));
        }

        let mut auth_state: Option<pwa_auth::AppState> = None;
        if self.auth_enabled {
            let config = self.auth_config.unwrap_or_default();
            auth_state = Some(pwa_auth::routes::build_state(config));
        }

        let mut fiat_state: Option<fiat_offramp::AppState> = None;
        if self.fiat_enabled {
            let config = self.fiat_config.unwrap_or_default();
            fiat_state = Some(fiat_offramp::routes::build_state(config));
        }

        ExpansionServer {
            config: self.config,
            ai_ceo_state,
            auth_state,
            fiat_state,
            enable_compression: self.enable_compression,
            enable_tracing: self.enable_tracing,
        }
    }
}

impl Default for ExpansionBuilder {
    fn default() -> Self {
        Self::new()
    }
}
