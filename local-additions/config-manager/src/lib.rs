use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigManager {
    pub storage_path: String,
    pub encryption_key: Vec<u8>,
    pub master_password: String,
    pub auto_save: bool,
    pub backup_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub name: String,
    pub id: String,
    pub chain_id: u64,
    pub rpc_url: String,
    pub ws_url: Option<String>,
    pub explorer_url: Option<String>,
    pub native_currency: NativeCurrency,
    pub contracts: Contracts,
    pub gas_sponsorship_id: Option<String>,
    pub enabled: bool,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeCurrency {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contracts {
    pub nft: Option<String>,
    pub erc20: Option<String>,
    pub erc721: Option<String>,
    pub erc1155: Option<String>,
    pub pair_router: Option<String>,
    pub factory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub provider_type: ProviderType,
    pub api_key: Secret,
    pub api_secret: Option<Secret>,
    pub api_url: String,
    pub supported_networks: Vec<String>,
    pub rate_limit: Option<RateLimit>,
    pub enabled: bool,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderType {
    Alchemy,
    Flashbots,
    Binance,
    Coinbase,
    QuickNode,
    Ankr,
    Moralis,
    Infura,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub burst_limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    pub value: String,
    pub encrypted: bool,
    pub iv: Option<String>,
    pub key_id: Option<String>,
    pub algorithm: Option<String>,
    pub last_rotated: Option<chrono::NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub name: String,
    pub engine_type: EngineType,
    pub enabled: bool,
    pub provider: String,
    pub network: String,
    pub parameters: serde_json::Value,
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineType {
    FlashLoanArbitrage,
    MevExtraction,
    CrossVenueArbitrage,
    SuperArb,
    RevenueEngine,
    LiquidityEngine,
    RiskEngine,
    ComplianceEngine,
    WASMPolicy,
    DarkPool,
    CEXArb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub metrics_enabled: bool,
    pub logs_enabled: bool,
    pub alerts_enabled: bool,
    pub dashboard_url: Option<String>,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub version: String,
    pub environment: Environment,
    pub network_configs: Vec<NetworkConfig>,
    pub provider_configs: Vec<ProviderConfig>,
    pub engine_configs: Vec<EngineConfig>,
    pub security: SecurityConfig,
    pub backup: BackupConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Environment {
    Development,
    Staging,
    Production,
    Testing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub encryption_algorithm: String,
    pub key_rotation_days: u32,
    pub audit_logging: bool,
    pub secret_scanning: bool,
    pub ip_restrictions: bool,
    pub allowed_ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub enabled: bool,
    pub schedule: String,
    pub retention_days: u32,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub encryption_key_id: Option<String>,
}

impl ConfigManager {
    pub fn new(storage_path: String, master_password: String) -> Result<Self, ConfigError> {
        if !Path::new(&storage_path).exists() {
            std::fs::create_dir_all(&storage_path)?;
        }
        
        let encryption_key = Self::derive_key(&master_password);
        
        Ok(Self {
            storage_path,
            encryption_key,
            master_password,
            auto_save: true,
            backup_enabled: true,
        })
    }
    
    fn derive_key(password: &str) -> Vec<u8> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        hasher.finalize().to_vec()
    }
    
    pub async fn load_config(&self) -> Result<GlobalConfig, ConfigError> {
        let config_path = format!("{}/config.toml", self.storage_path);
        
        if !Path::new(&config_path).exists() {
            return Err(ConfigError::FileNotFound(config_path));
        }
        
        let content = tokio::fs::read_to_string(&config_path).await?;
        let mut config: GlobalConfig = toml::from_str(&content)?;
        
        // Decrypt secrets
        for provider in &mut config.provider_configs {
            if let Some(secret) = &mut provider.api_secret {
                if secret.encrypted {
                    secret.value = Self::decrypt_secret(&secret.value, &secret.iv, &self.encryption_key)?;
                    secret.encrypted = false;
                }
            }
        }
        
        Ok(config)
    }
    
    pub async fn save_config(&self, config: &GlobalConfig) -> Result<(), ConfigError> {
        let mut encrypted_config = config.clone();
        
        // Encrypt secrets before saving
        for provider in &mut encrypted_config.provider_configs {
            if let Some(secret) = &mut provider.api_secret {
                secret.value = Self::encrypt_secret(&secret.value, &self.encryption_key)?;
                secret.encrypted = true;
                secret.iv = Some(Self::generate_iv());
            }
        }
        
        let config_path = format!("{}/config.toml", self.storage_path);
        let content = toml::to_string(&encrypted_config)?;
        
        tokio::fs::write(&config_path, content).await?;
        
        if self.backup_enabled {
            self.create_backup(&config_path).await?;
        }
        
        Ok(())
    }
    
    fn encrypt_secret(&self, data: &str, key: &[u8]) -> Result<String, ConfigError> {
        use aes_gcm::{Aes256Gcm, Key, Nonce};
        use aes_gcm::aead::{Aead, KeyInit};
        
        let key = Key::<Aes256Gcm>::from_slice(key);
        let nonce = Self::generate_iv();
        let cipher = Aes256Gcm::new(key);
        
        let nonce_bytes = hex::decode(&nonce)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let ciphertext = cipher.encrypt(nonce, data.as_bytes())
            .map_err(|e| ConfigError::EncryptionError(e.to_string()))?;
        
        let encoded = base64::encode(ciphertext);
        Ok(format!("{}.{}", encoded, nonce))
    }
    
    fn decrypt_secret(&self, data: &str, iv: &Option<String>, key: &[u8]) -> Result<String, ConfigError> {
        use aes_gcm::{Aes256Gcm, Key, Nonce};
        use aes_gcm::aead::{Aead, KeyInit};
        
        let parts: Vec<&str> = data.split('.').collect();
        if parts.len() != 2 {
            return Err(ConfigError::InvalidSecretFormat);
        }
        
        let (ciphertext_b64, nonce_b64) = (parts[0], parts[1]);
        let ciphertext = base64::decode(ciphertext_b64)?;
        let nonce_bytes = hex::decode(nonce_b64)?;
        
        let key = Key::<Aes256Gcm>::from_slice(key);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let cipher = Aes256Gcm::new(key);
        
        let plaintext = cipher.decrypt(nonce, ciphertext.as_slice())
            .map_err(|e| ConfigError::DecryptionError(e.to_string()))?;
        
        Ok(String::from_utf8(plaintext)?)
    }
    
    fn generate_iv(&self) -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut iv_bytes = [0u8; 12];
        rng.fill(&mut iv_bytes);
        hex::encode(iv_bytes)
    }
    
    async fn create_backup(&self, config_path: &str) -> Result<(), ConfigError> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let backup_path = format!("{}/backups/config_{}.toml", self.storage_path, timestamp);
        
        std::fs::create_dir_all(format!("{}/backups", self.storage_path))?
        
        let content = tokio::fs::read_to_string(config_path).await?;
        tokio::fs::write(backup_path, content).await?;
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] toml::ser::Error),
    #[error("Deserialization error: {0}")]
    Deserialization(#[from] toml::de::Error),
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    #[error("Decryption error: {0}")]
    DecryptionError(String),
    #[error("Base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("Hex error: {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Invalid secret format")]
    InvalidSecretFormat,
    #[error("Validation error: {0}")]
    ValidationError(String),
}
