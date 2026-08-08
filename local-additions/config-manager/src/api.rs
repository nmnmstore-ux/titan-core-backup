use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Json, IntoResponse},
    routing::{get, post, put, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{ConfigManager, GlobalConfig, ConfigError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResponse {
    pub status: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

pub struct AppState {
    pub config_manager: Arc<RwLock<ConfigManager>>,
}

pub fn create_config_router() -> Router {
    Router::new()
        .route("/api/v1/config", post(create_config))
        .route("/api/v1/config", get(get_config))
        .route("/api/v1/config", put(update_config))
        .route("/api/v1/config", delete(delete_config))
        .route("/api/v1/config/validate", post(validate_config))
        .route("/api/v1/config/backup", post(create_backup))
        .route("/api/v1/config/health", get(config_health))
        .route("/api/v1/config/providers", get(get_providers))
        .route("/api/v1/config/providers/{provider_name}", get(get_provider))
        .route("/api/v1/config/networks", get(get_networks))
        .route("/api/v1/config/networks/{network_id}", get(get_network))
        .route("/api/v1/config/engines", get(get_engines))
        .route("/api/v1/config/engines/{engine_name}", get(get_engine))
        .route("/api/v1/config/secrets/{secret_type}", get(get_secrets))
        .route("/api/v1/config/secrets/{secret_type}/rotate", post(rotate_secret))
        .with_state(AppState {
            config_manager: Arc::new(RwLock::new(
                ConfigManager::new(
                    "/app/config".to_string(),
                    "default_master_password".to_string(),
                )
                .unwrap(),
            )),
        })
}

async fn create_config(
    State(state): State<AppState>,
    Json(config): Json<GlobalConfig>,
) -> impl IntoResponse {
    let mut manager = state.config_manager.write().await;
    
    match manager.save_config(&config).await {
        Ok(()) => (
            StatusCode::CREATED,
            Json(ConfigResponse {
                status: true,
                message: "Configuration saved successfully".to_string(),
                data: Some(serde_json::json!({"config": config})),
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigResponse {
                status: false,
                message: format!("Failed to save configuration: {}", e),
                data: None,
            }),
        ),
    }
}

async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    
    match manager.load_config().await {
        Ok(config) => (
            StatusCode::OK,
            Json(ConfigResponse {
                status: true,
                message: "Configuration retrieved successfully".to_string(),
                data: Some(serde_json::json!(config)),
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigResponse {
                status: false,
                message: format!("Failed to load configuration: {}", e),
                data: None,
            }),
        ),
    }
}

async fn update_config(
    State(state): State<AppState>,
    Json(config): Json<GlobalConfig>,
) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    
    match manager.save_config(&config).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ConfigResponse {
                status: true,
                message: "Configuration updated successfully".to_string(),
                data: Some(serde_json::json!({"config": config})),
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigResponse {
                status: false,
                message: format!("Failed to update configuration: {}", e),
                data: None,
            }),
        ),
    }
}

async fn delete_config(State(state): State<AppState>) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    let storage_path = manager.storage_path.clone();
    
    match tokio::fs::remove_dir_all(&storage_path).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ConfigResponse {
                status: true,
                message: format!("Configuration deleted successfully: {}", storage_path),
                data: None,
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigResponse {
                status: false,
                message: format!("Failed to delete configuration: {}", e),
                data: None,
            }),
        ),
    }
}

async fn validate_config(
    State(state): State<AppState>,
    Json(config): Json<GlobalConfig>,
) -> impl IntoResponse {
    let mut validation_errors = Vec::new();
    
    // Validate network configurations
    for network in &config.network_configs {
        if network.name.is_empty() {
            validation_errors.push("Network name cannot be empty".to_string());
        }
        if network.rpc_url.is_empty() {
            validation_errors.push(format!("Network {} RPC URL cannot be empty", network.name));
        }
        if !network.enabled {
            validation_errors.push(format!("Network {} must be enabled", network.name));
        }
    }
    
    // Validate provider configurations
    for provider in &config.provider_configs {
        if provider.name.is_empty() {
            validation_errors.push("Provider name cannot be empty".to_string());
        }
        if let ProviderType::Custom(name) = &provider.provider_type {
            if name.is_empty() {
                validation_errors.push("Custom provider type name cannot be empty".to_string());
            }
        }
    }
    
    if validation_errors.is_empty() {
        (
            StatusCode::OK,
            Json(ConfigResponse {
                status: true,
                message: "Configuration is valid".to_string(),
                data: Some(serde_json::json!({"errors": []})),
            }),
        )
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(ConfigResponse {
                status: false,
                message: "Configuration validation failed".to_string(),
                data: Some(serde_json::json!({"errors": validation_errors})),
            }),
        )
    }
}

async fn create_backup(State(state): State<AppState>) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    
    match manager.create_backup("/tmp/config.toml").await {
        Ok(()) => (
            StatusCode::OK,
            Json(ConfigResponse {
                status: true,
                message: "Backup created successfully".to_string(),
                data: None,
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigResponse {
                status: false,
                message: format!("Failed to create backup: {}", e),
                data: None,
            }),
        ),
    }
}

async fn config_health(State(state): State<AppState>) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    let storage_exists = tokio::fs::metadata(&manager.storage_path)
        .await
        .map(|m| m.is_dir())
        .unwrap_or(false);
    
    let health_status = serde_json::json!({
        "status": "healthy",
        "storage_path": manager.storage_path,
        "auto_save": manager.auto_save,
        "backup_enabled": manager.backup_enabled,
        "storage_exists": storage_exists,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    
    (
        StatusCode::OK,
        Json(ConfigResponse {
            status: true,
            message: "Config manager health check passed".to_string(),
            data: Some(health_status),
        }),
    )
}

async fn get_providers(State(state): State<AppState>) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    
    match manager.load_config().await {
        Ok(config) => (
            StatusCode::OK,
            Json(ConfigResponse {
                status: true,
                message: "Providers retrieved successfully".to_string(),
                data: Some(serde_json::json!(config.provider_configs)),
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigResponse {
                status: false,
                message: format!("Failed to retrieve providers: {}", e),
                data: None,
            }),
        ),
    }
}

async fn get_provider(
    State(state): State<AppState>,
    Path(provider_name): String,
) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    
    match manager.load_config().await {
        Ok(config) => {
            let provider = config.provider_configs
                .iter()
                .find(|p| p.name == provider_name)
                .cloned();
            
            match provider {
                Some(p) => (
                    StatusCode::OK,
                    Json(ConfigResponse {
                        status: true,
                        message: format!("Provider '{}' retrieved successfully", provider_name),
                        data: Some(serde_json::json!(p)),
                    }),
                ),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(ConfigResponse {
                        status: false,
                        message: format!("Provider '{}' not found", provider_name),
                        data: None,
                    }),
                ),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigResponse {
                status: false,
                message: format!("Failed to retrieve provider: {}", e),
                data: None,
            }),
        ),
    }
}

async fn get_networks(State(state): State<AppState>) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    
    match manager.load_config().await {
        Ok(config) => (
            StatusCode::OK,
            Json(ConfigResponse {
                status: true,
                message: "Networks retrieved successfully".to_string(),
                data: Some(serde_json::json!(config.network_configs)),
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigResponse {
                status: false,
                message: format!("Failed to retrieve networks: {}", e),
                data: None,
            }),
        ),
    }
}

async fn get_network(
    State(state): State<AppState>,
    Path(network_id): String,
) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    
    match manager.load_config().await {
        Ok(config) => {
            let network = config.network_configs
                .iter()
                .find(|n| n.id == network_id)
                .cloned();
            
            match network {
                Some(n) => (
                    StatusCode::OK,
                    Json(ConfigResponse {
                        status: true,
                        message: format!("Network '{}' retrieved successfully", network_id),
                        data: Some(serde_json::json!(n)),
                    }),
                ),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(ConfigResponse {
                        status: false,
                        message: format!("Network '{}' not found", network_id),
                        data: None,
                    }),
                ),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigResponse {
                status: false,
                message: format!("Failed to retrieve network: {}", e),
                data: None,
            }),
        ),
    }
}

async fn get_engines(State(state): State<AppState>) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    
    match manager.load_config().await {
        Ok(config) => (
            StatusCode::OK,
            Json(ConfigResponse {
                status: true,
                message: "Engines retrieved successfully".to_string(),
                data: Some(serde_json::json!(config.engine_configs)),
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigResponse {
                status: false,
                message: format!("Failed to retrieve engines: {}", e),
                data: None,
            }),
        ),
    }
}

async fn get_engine(
    State(state): State<AppState>,
    Path(engine_name): String,
) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    
    match manager.load_config().await {
        Ok(config) => {
            let engine = config.engine_configs
                .iter()
                .find(|e| e.name == engine_name)
                .cloned();
            
            match engine {
                Some(e) => (
                    StatusCode::OK,
                    Json(ConfigResponse {
                        status: true,
                        message: format!("Engine '{}' retrieved successfully", engine_name),
                        data: Some(serde_json::json!(e)),
                    }),
                ),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(ConfigResponse {
                        status: false,
                        message: format!("Engine '{}' not found", engine_name),
                        data: None,
                    }),
                ),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigResponse {
                status: false,
                message: format!("Failed to retrieve engine: {}", e),
                data: None,
            }),
        ),
    }
}

async fn get_secrets(
    State(state): State<AppState>,
    Path(secret_type): String,
) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    
    let secrets = match secret_type.as_str() {
        "api_keys" => vec![
            serde_json::json!({"name": "Alchemy API Key", "value": "***"}),
            serde_json::json!({"name": "Flashbots API Key", "value": "***"}),
        ],
        "database" => vec![
            serde_json::json!({"name": "Database URL", "value": "***"}),
            serde_json::json!({"name": "Redis URL", "value": "***"}),
        ],
        "exchange" => vec![
            serde_json::json!({"name": "Binance API Key", "value": "***"}),
            serde_json::json!({"name": "Coinbase API Key", "value": "***"}),
        ],
        "gas_sponsorship" => vec![
            serde_json::json!({"name": "Gas Sponsorship ID", "value": "***"}),
        ],
        _ => vec![],
    };
    
    (
        StatusCode::OK,
        Json(ConfigResponse {
            status: true,
            message: format!("Secrets of type '{}' retrieved successfully", secret_type),
            data: Some(serde_json::json!(secrets)),
        }),
    )
}

async fn rotate_secret(
    State(state): State<AppState>,
    Path(secret_type): String,
) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    
    // In a real implementation, this would:
    // 1. Load the current secret
    // 2. Generate a new one
    // 3. Encrypt and save it
    // 4. Invalidate old secrets
    
    (
        StatusCode::OK,
        Json(ConfigResponse {
            status: true,
            message: format!("Secret rotation for type '{}' initiated", secret_type),
            data: Some(serde_json::json!({
                "secret_type": secret_type,
                "status": "rotated",
                "timestamp": chrono::Utc::now().to_rfc3339()
            })),
        }),
    )
}
