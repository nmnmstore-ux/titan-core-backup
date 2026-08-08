//! # Config Manager Integration for THE-BRIDGE Matching Engine
//!
//! This module integrates the config-manager crate into the main matching engine,
//! providing centralized configuration management for all engine components.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Json, IntoResponse},
    routing::{get, post, put, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::sync::RwLock;
use arc_swap::ArcSwap;
use parking_lot::RwLock as ParkingLotRwLock;

use config_manager::{ConfigManager, GlobalConfig, ConfigError};

pub mod api;
pub mod utils;

// Re-export commonly used types
pub use config_manager::{
    NetworkConfig,
    ProviderConfig,
    ProviderType,
    EngineConfig,
    EngineType,
    Secret,
    Environment,
    SecurityConfig,
    BackupConfig,
};

// Application state that includes both the matching engine state and config manager
#[derive(Clone)]
pub struct AppState {
    pub matching_engine_state: Arc<RwLock<MatchingEngineState>>,
    pub config_manager: Arc<RwLock<ConfigManager>>,
}

// Re-export API modules
pub use api::*;

// Initialize config manager on startup
pub async fn initialize_config_manager() -> Result<(), ConfigError> {
    let mut manager = match ConfigManager::new(
        "/app/config".to_string(),
        "default_master_password".to_string(),
    ) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to initialize config manager: {}", e);
            return Err(e);
        }
    };
    
    // Try to load existing config
    match manager.load_config().await {
        Ok(config) => {
            tracing::info!("Successfully loaded configuration from storage");
        }
        Err(e) => {
            tracing::info!("No existing configuration found, will create default");
        }
    }
    
    // Save updated manager state
    *manager = ConfigManager::new(
        "/app/config".to_string(),
        "default_master_password".to_string(),
    )?;
    
    Ok(())
}

// State structure for the matching engine's operational state
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct MatchingEngineState {
    pub engines_running: bool,
    pub active_network: String,
    pub total_pnl_usd: f64,
    pub uptime_seconds: u64,
    pub memory_usage_mb: u64,
    pub cpu_usage_percent: f64,
    pub last_updated: chrono::NaiveDateTime,
}

// Health check response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
    pub version: String,
    pub engine_state: MatchingEngineState,
    pub config_status: ConfigStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigStatus {
    pub loaded: bool,
    pub path: String,
    pub last_modified: Option<String>,
    pub errors: Vec<String>,
}

// Configuration management API endpoints
pub fn create_config_router() -> Router {
    Router::new()
        // Configuration CRUD operations
        .route("/api/v1/config", post(create_config))
        .route("/api/v1/config", get(get_config))
        .route("/api/v1/config", put(update_config))
        .route("/api/v1/config", delete(delete_config))
        
        // Configuration validation and backup
        .route("/api/v1/config/validate", post(validate_config))
        .route("/api/v1/config/backup", post(create_backup))
        .route("/api/v1/config/health", get(config_health))
        
        // Network configuration management
        .route("/api/v1/config/networks", get(get_networks))
        .route("/api/v1/config/networks/{network_id}", get(get_network))
        .route("/api/v1/config/networks/{network_id}", put(update_network))
        .route("/api/v1/config/networks", post(add_network))
        .route("/api/v1/config/networks/{network_id}", delete(delete_network))
        
        // Provider configuration management
        .route("/api/v1/config/providers", get(get_providers))
        .route("/api/v1/config/providers/{provider_name}", get(get_provider))
        .route("/api/v1/config/providers/{provider_name}", put(update_provider))
        .route("/api/v1/config/providers", post(add_provider))
        .route("/api/v1/config/providers/{provider_name}", delete(delete_provider))
        
        // Engine configuration management
        .route("/api/v1/config/engines", get(get_engines))
        .route("/api/v1/config/engines/{engine_name}", get(get_engine))
        .route("/api/v1/config/engines/{engine_name}", put(update_engine))
        .route("/api/v1/config/engines", post(add_engine))
        .route("/api/v1/config/engines/{engine_name}", delete(delete_engine))
        
        // Secret management
        .route("/api/v1/config/secrets/{secret_type}", get(get_secrets))
        .route("/api/v1/config/secrets/{secret_type}/rotate", post(rotate_secret))
        .route("/api/v1/config/secrets/{secret_type}/test", post(test_secret))
        
        // Environment and deployment management
        .route("/api/v1/config/environment", get(get_environment))
        .route("/api/v1/config/environment", put(update_environment))
        .route("/api/v1/config/deploy", post(deploy_configuration))
        .route("/api/v1/config/export", get(export_config))
        .route("/api/v1/config/import", post(import_config))
        
        // Monitoring and diagnostics
        .route("/api/v1/config/monitoring", get(get_monitoring))
        .route("/api/v1/config/metrics", get(get_metrics))
        .route("/api/v1/config/logs", get(get_logs))
        
        // System management
        .route("/api/v1/config/system/reload", post(reload_config))
        .route("/api/v1/config/system/restart", post(restart_engines))
        .route("/api/v1/config/system/status", get(get_system_status))
        
        .with_state(AppState {
            matching_engine_state: Arc::new(RwLock::new(MatchingEngineState::default())),
            config_manager: Arc::new(RwLock::new(
                ConfigManager::new(
                    "/app/config".to_string(),
                    "default_master_password".to_string(),
                )
                .unwrap(),
            )),
        })
}

// Configuration API endpoints implementations
async fn create_config(
    State(state): State<AppState>,
    Json(config): Json<GlobalConfig>,
) -> impl IntoResponse {
    let mut manager = state.config_manager.write().await;
    
    match manager.save_config(&config).await {
        Ok(()) => (
            StatusCode::CREATED,
            Json(ApiResponse {
                success: true,
                message: "Configuration saved successfully".to_string(),
                data: Some(serde_json::json!({"config": config})),
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
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
            Json(ApiResponse {
                success: true,
                message: "Configuration retrieved successfully".to_string(),
                data: Some(serde_json::json!(config)),
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
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
            Json(ApiResponse {
                success: true,
                message: "Configuration updated successfully".to_string(),
                data: Some(serde_json::json!({"config": config})),
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
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
            Json(ApiResponse {
                success: true,
                message: format!("Configuration deleted successfully: {}", storage_path),
                data: None,
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
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
            Json(ApiResponse {
                success: true,
                message: "Configuration is valid".to_string(),
                data: Some(serde_json::json!({"errors": []})),
            }),
        )
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
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
            Json(ApiResponse {
                success: true,
                message: "Backup created successfully".to_string(),
                data: None,
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
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
        Json(ApiResponse {
            success: true,
            message: "Config manager health check passed".to_string(),
            data: Some(health_status),
        }),
    )
}

// Generic API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

// TODO: Implement remaining API endpoints for providers, networks, engines, secrets, etc.
async fn get_providers(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Providers endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn get_networks(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Networks endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn get_engines(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Engines endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn get_secrets(
    State(_state): State<AppState>,
    Path(_secret_type): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Secrets endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn get_environment(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Environment endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn deploy_configuration(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Deploy endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn export_config(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Export endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn import_config(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Import endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn get_monitoring(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Monitoring endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn get_metrics(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Metrics endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn get_logs(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Logs endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn reload_config(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Reload endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn restart_engines(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Restart endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn get_system_status(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "System status endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

// Provider configuration operations
async fn get_provider(
    State(_state): State<AppState>,
    Path(_provider_name): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Get provider endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn update_provider(
    State(_state): State<AppState>,
    Path(_provider_name): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Update provider endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn add_provider(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Add provider endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn delete_provider(
    State(_state): State<AppState>,
    Path(_provider_name): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Delete provider endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

// Network configuration operations
async fn get_network(
    State(_state): State<AppState>,
    Path(_network_id): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Get network endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn update_network(
    State(_state): State<AppState>,
    Path(_network_id): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Update network endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn add_network(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Add network endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn delete_network(
    State(_state): State<AppState>,
    Path(_network_id): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Delete network endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

// Engine configuration operations
async fn get_engine(
    State(_state): State<AppState>,
    Path(_engine_name): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Get engine endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn update_engine(
    State(_state): State<AppState>,
    Path(_engine_name): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Update engine endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn add_engine(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Add engine endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn delete_engine(
    State(_state): State<AppState>,
    Path(_engine_name): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Delete engine endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

// Secret management operations
async fn rotate_secret(
    State(_state): State<AppState>,
    Path(_secret_type): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Rotate secret endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

async fn test_secret(
    State(_state): State<AppState>,
    Path(_secret_type): String,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Test secret endpoint not yet implemented".to_string(),
            data: None,
        }),
    )
}

// Health check endpoint that combines engine and config status
pub fn create_health_router() -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/status", get(detailed_status))
        .with_state(AppState {
            matching_engine_state: Arc::new(RwLock::new(MatchingEngineState::default())),
            config_manager: Arc::new(RwLock::new(
                ConfigManager::new(
                    "/app/config".to_string(),
                    "default_master_password".to_string(),
                )
                .unwrap(),
            )),
        })
}

async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    let mut engine_state = state.matching_engine_state.write().await;
    
    // Update engine state
    engine_state.last_updated = chrono::Utc::now().naive_utc();
    
    let config_status = match manager.load_config().await {
        Ok(_) => ConfigStatus {
            loaded: true,
            path: manager.storage_path.clone(),
            last_modified: Some(chrono::Utc::now().to_rfc3339()),
            errors: vec![],
        },
        Err(e) => ConfigStatus {
            loaded: false,
            path: manager.storage_path.clone(),
            last_modified: None,
            errors: vec![format!("Config load error: {}", e)],
        },
    };
    
    let health_response = HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        engine_state: engine_state.clone(),
        config_status,
    };
    
    (
        StatusCode::OK,
        Json(health_response),
    )
}

async fn detailed_status(State(state): State<AppState>) -> impl IntoResponse {
    let manager = state.config_manager.read().await;
    let engine_state = state.matching_engine_state.read().await;
    
    let config_status = match manager.load_config().await {
        Ok(config) => ConfigStatus {
            loaded: true,
            path: manager.storage_path.clone(),
            last_modified: Some(chrono::Utc::now().to_rfc3339()),
            errors: vec![],
        },
        Err(e) => ConfigStatus {
            loaded: false,
            path: manager.storage_path.clone(),
            last_modified: None,
            errors: vec![format!("Config load error: {}", e)],
        },
    };
    
    let detailed_status = serde_json::json!({
        "engine_state": {
            "engines_running": engine_state.engines_running,
            "active_network": engine_state.active_network,
            "total_pnl_usd": engine_state.total_pnl_usd,
            "uptime_seconds": engine_state.uptime_seconds,
            "memory_usage_mb": engine_state.memory_usage_mb,
            "cpu_usage_percent": engine_state.cpu_usage_percent,
            "last_updated": engine_state.last_updated,
        },
        "config_status": config_status,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Detailed status retrieved successfully".to_string(),
            data: Some(detailed_status),
        }),
    )
}

// Binary entry point
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    info!("Starting THE-BRIDGE Config Manager");
    
    // Initialize config manager
    let config_manager = Arc::new(RwLock::new(
        ConfigManager::new(
            "/app/config".to_string(),
            "default_master_password".to_string(),
        )?
    ));
    
    // Create router with all API routes
    let app = Router::new()
        // Configuration CRUD operations
        .route("/api/v1/config", post(api::create_config))
        .route("/api/v1/config", get(api::get_config))
        .route("/api/v1/config", put(api::update_config))
        .route("/api/v1/config", delete(api::delete_config))
        
        // Configuration validation and backup
        .route("/api/v1/config/validate", post(api::validate_config))
        .route("/api/v1/config/backup", post(api::create_backup))
        .route("/api/v1/config/health", get(api::config_health))
        
        // Network configuration management
        .route("/api/v1/config/networks", get(api::get_networks))
        .route("/api/v1/config/networks/{network_id}", get(api::get_network))
        .route("/api/v1/config/networks/{network_id}", put(api::update_network))
        .route("/api/v1/config/networks", post(api::add_network))
        .route("/api/v1/config/networks/{network_id}", delete(api::delete_network))
        
        // Provider configuration management
        .route("/api/v1/config/providers", get(api::get_providers))
        .route("/api/v1/config/providers/{provider_name}", get(api::get_provider))
        .route("/api/v1/config/providers/{provider_name}", put(api::update_provider))
        .route("/api/v1/config/providers", post(api::add_provider))
        .route("/api/v1/config/providers/{provider_name}", delete(api::delete_provider))
        
        // Engine configuration management
        .route("/api/v1/config/engines", get(api::get_engines))
        .route("/api/v1/config/engines/{engine_name}", get(api::get_engine))
        .route("/api/v1/config/engines/{engine_name}", put(api::update_engine))
        .route("/api/v1/config/engines", post(api::add_engine))
        .route("/api/v1/config/engines/{engine_name}", delete(api::delete_engine))
        
        // Secret management
        .route("/api/v1/config/secrets/{secret_type}", get(api::get_secrets))
        .route("/api/v1/config/secrets/{secret_type}/rotate", post(api::rotate_secret))
        .route("/api/v1/config/secrets/{secret_type}/test", post(api::test_secret))
        
        // Environment and deployment management
        .route("/api/v1/config/environment", get(api::get_environment))
        .route("/api/v1/config/environment", put(api::update_environment))
        .route("/api/v1/config/deploy", post(api::deploy_configuration))
        .route("/api/v1/config/export", get(api::export_config))
        .route("/api/v1/config/import", post(api::import_config))
        
        // Monitoring and diagnostics
        .route("/api/v1/config/monitoring", get(api::get_monitoring))
        .route("/api/v1/config/metrics", get(api::get_metrics))
        .route("/api/v1/config/logs", get(api::get_logs))
        
        // System management
        .route("/api/v1/config/system/reload", post(api::reload_config))
        .route("/api/v1/config/system/restart", post(api::restart_engines))
        .route("/api/v1/config/system/status", get(api::get_system_status))
        
        .with_state(AppState {
            matching_engine_state: Arc::new(RwLock::new(MatchingEngineState::default())),
            config_manager: Arc::new(RwLock::new(
                ConfigManager::new(
                    "/app/config".to_string(),
                    "default_master_password".to_string(),
                )?
            )),
        });
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    info!("Config Manager API listening on port 8080");
    
    axum::serve(listener, app).await?;
    
    Ok(())
}
