use tracing::info;

use crate::builder::{ExpansionBuilder, ServerConfig};
use crate::{IntegrationError, IntegrationResult};

pub struct ExpansionServer {
    pub(crate) config: ServerConfig,
    pub(crate) ai_ceo_state: Option<ai_ceo_bridge::AppState>,
    pub(crate) auth_state: Option<pwa_auth::AppState>,
    pub(crate) fiat_state: Option<fiat_offramp::AppState>,
    pub(crate) enable_compression: bool,
    pub(crate) enable_tracing: bool,
}

impl ExpansionServer {
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    pub async fn run(self) -> IntegrationResult<()> {
        if self.enable_tracing {
            tracing_subscriber::fmt()
                .with_env_filter(&self.config.tracing_filter)
                .init();
        }

        info!("Expansion modules starting up");
        info!("HTTP bind: {}", self.config.http_bind);
        info!("gRPC bind: {}", self.config.grpc_bind);

        let mut router = axum::Router::new();

        if let Some(ref state) = self.ai_ceo_state {
            info!("Mounting AI CEO Bridge routes at /api/v1/ai");
            let ai_router = ai_ceo_bridge::routes::router(state.clone());
            router = router.nest("/api/v1/ai", ai_router);
        }

        if let Some(ref state) = self.auth_state {
            info!("Mounting PWA Auth routes at /api/v1/auth");
            let auth_router = pwa_auth::routes::router(state.clone());
            router = router.nest("/api/v1/auth", auth_router);
        }

        if let Some(ref state) = self.fiat_state {
            info!("Mounting Fiat Off-Ramp routes at /api/v1/fiat");
            let fiat_router = fiat_offramp::routes::router(state.clone());
            router = router.nest("/api/v1/fiat", fiat_router);
        }

        router = router.route("/health", axum::routing::get(health_handler));

        if self.enable_tracing {
            router = router.layer(tower_http::trace::TraceLayer::new_for_http());
        }

        if self.enable_compression {
            router = router.layer(tower_http::compression::CompressionLayer::new());
        }

        info!("Expansion server ready");

        let listener = tokio::net::TcpListener::bind(self.config.http_bind)
            .await
            .map_err(|e| IntegrationError::Http(e.to_string()))?;

        axum::serve(listener, router)
            .await
            .map_err(|e| IntegrationError::Http(e.to_string()))?;

        Ok(())
    }

    pub fn builder() -> ExpansionBuilder {
        ExpansionBuilder::new()
    }
}

async fn health_handler() -> impl axum::response::IntoResponse {
    axum::Json(serde_json::json!({
        "status": "ok",
        "modules": {
            "ai_ceo": true,
            "auth": true,
            "fiat": true,
        },
        "timestamp": serde_json::json!(chrono::Utc::now()),
    }))
}

pub async fn run_with_default() -> IntegrationResult<()> {
    let builder = ExpansionBuilder::new()
        .with_ai_ceo(ai_ceo_bridge::BridgeConfig::default())
        .with_auth(pwa_auth::AuthConfig::default())
        .with_fiat(fiat_offramp::FiatConfig::default());

    let server = builder.build();
    server.run().await
}
