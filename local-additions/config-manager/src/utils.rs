use arc_swap::{ArcSwap, ArcSwapOption};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::path::Path;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Utility functions for configuration management
pub struct ConfigUtils;

impl ConfigUtils {
    /// Load configuration from a TOML file with proper error handling
    pub async fn load_toml_config<P: AsRef<Path>>(path: P) -> Result<serde_json::Value, ConfigError> {
        let path = path.as_ref();
        
        if !path.exists() {
            return Err(ConfigError::FileNotFound(path.to_string_lossy().to_string()));
        }
        
        let mut file = File::open(path).await
            .map_err(|e| ConfigError::Io(e))?;
        
        let mut contents = String::new();
        file.read_to_string(&mut contents).await
            .map_err(|e| ConfigError::Io(e))?;
        
        let config: toml::Value = toml::from_str(&contents)
            .map_err(|e| ConfigError::Serialization(e))?;
        
        Ok(serde_json::json!(config))
    }
    
    /// Save configuration to a TOML file with atomic write
    pub async fn save_toml_config<P: AsRef<Path>>(path: P, config: &serde_json::Value) -> Result<(), ConfigError> {
        let path = path.as_ref();
        let parent_dir = path.parent().ok_or_else(|| {
            ConfigError::ValidationError("Invalid file path".to_string())
        })?;
        
        if !parent_dir.exists() {
            tokio::fs::create_dir_all(parent_dir).await
                .map_err(|e| ConfigError::Io(e))?;
        }
        
        let temp_path = format!("{}.tmp", path.to_string_lossy());
        let content = toml::to_string(config)
            .map_err(|e| ConfigError::Serialization(e))?;
        
        tokio::fs::write(&temp_path, content).await
            .map_err(|e| ConfigError::Io(e))?;
        
        // Atomic rename
        tokio::fs::rename(&temp_path, path).await
            .map_err(|e| ConfigError::Io(e))?;
        
        Ok(())
    }
    
    /// Validate configuration against schema
    pub fn validate_config_schema(config: &serde_json::Value) -> Result<(), ConfigError> {
        // Basic validation - check if required fields exist
        if !config.is_object() {
            return Err(ConfigError::ValidationError("Config must be an object".to_string()));
        }
        
        // Check for version field
        if let Some(version) = config.get("version") {
            if !version.is_string() {
                return Err(ConfigError::ValidationError("Config version must be a string".to_string()));
            }
        }
        
        Ok(())
    }
    
    /// Generate a unique identifier for the configuration
    pub fn generate_config_id() -> String {
        use uuid::Uuid;
        Uuid::new_v4().to_string()
    }
    
    /// Backup configuration with timestamp
    pub async fn backup_config<P: AsRef<Path>>(path: P, backup_dir: P) -> Result<(), ConfigError> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let filename = format!("config_{}.toml", timestamp);
        let backup_path = backup_dir.as_ref().join(filename);
        
        tokio::fs::create_dir_all(backup_dir.as_ref()).await
            .map_err(|e| ConfigError::Io(e))?;
        
        tokio::fs::copy(path.as_ref(), backup_path).await
            .map_err(|e| ConfigError::Io(e))?;
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AtomicConfig {
    pub data: ArcSwap<GlobalConfig>,
    pub version: ArcSwap<u64>,
    pub lock: Arc<RwLock<()>>,
}

impl AtomicConfig {
    pub fn new(initial: GlobalConfig) -> Self {
        Self {
            data: ArcSwap::new(Arc::new(initial)),
            version: ArcSwap::new(Arc::new(0)),
            lock: Arc::new(RwLock::new(())),
        }
    }
    
    pub fn get(&self) -> Arc<GlobalConfig> {
        self.data.load()
    }
    
    pub fn update(&self, new_config: GlobalConfig) {
        let _lock = self.lock.read();
        let new_version = *self.version.load() + 1;
        
        self.data.store(Arc::new(new_config));
        self.version.store(Arc::new(new_version));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub name: String,
    pub variables: std::collections::HashMap<String, String>,
    pub secrets: std::collections::HashMap<String, Secret>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    pub environments: Vec<EnvironmentConfig>,
    pub deployment_strategy: DeploymentStrategy,
    pub rollback_config: RollbackConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeploymentStrategy {
    BlueGreen,
    Rolling,
    Canary,
    Recreate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackConfig {
    pub enabled: bool,
    pub max_rollback_version: u32,
    pub rollback_delay: u64,
}
