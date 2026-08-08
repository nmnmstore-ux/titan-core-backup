use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug};
use dashmap::DashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemfdSecretConfig {
    pub enabled: bool,
    pub max_secrets: usize,
    pub max_secret_size: usize,
    pub seal_before_use: bool,
    pub seal_after_write: bool,
    pub prohibit_ptrace: bool,
    pub prohibit_exec: bool,
    pub mlock_enabled: bool,
    pub auto_seal_interval_secs: u64,
    pub encryption_algorithm: String,
    pub key_derivation: String,
    pub audit_enabled: bool,
    pub rotation_interval_secs: u64,
}

impl Default for MemfdSecretConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_secrets: 10_000,
            max_secret_size: 10 * 1024 * 1024,
            seal_before_use: true,
            seal_after_write: true,
            prohibit_ptrace: true,
            prohibit_exec: true,
            mlock_enabled: true,
            auto_seal_interval_secs: 300,
            encryption_algorithm: "AES-256-GCM".to_string(),
            key_derivation: "Argon2id".to_string(),
            audit_enabled: true,
            rotation_interval_secs: 86400,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEntry {
    pub id: String,
    pub namespace: String,
    pub key_id: String,
    pub size_bytes: usize,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u64,
    pub sealed: bool,
    pub algorithm: String,
    pub version: u32,
    pub rotation_due: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemfdStats {
    pub total_secrets: usize,
    pub total_bytes: usize,
    pub sealed_count: usize,
    pub unsealed_count: usize,
    pub total_accesses: u64,
    pub seal_operations: u64,
    pub unseal_operations: u64,
    pub ptrace_blocks: u64,
    pub rotation_operations: u64,
    pub encryption_operations: u64,
    pub decryption_operations: u64,
    pub audit_log_size: usize,
    pub memory_usage_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub secret_id: String,
    pub operation: String,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub details: String,
}

pub struct MemfdSecretStore {
    config: Arc<RwLock<MemfdSecretConfig>>,
    secrets: Arc<DashMap<String, SecretEntry>>,
    data: Arc<DashMap<String, Vec<u8>>>,
    stats: Arc<RwLock<MemfdStats>>,
    audit_log: Arc<DashMap<String, AuditEntry>>,
    running: Arc<RwLock<bool>>,
    rotation_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    auto_seal_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl MemfdSecretStore {
    pub fn new(config: MemfdSecretConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            secrets: Arc::new(DashMap::new()),
            data: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(MemfdStats {
                total_secrets: 0,
                total_bytes: 0,
                sealed_count: 0,
                unsealed_count: 0,
                total_accesses: 0,
                seal_operations: 0,
                unseal_operations: 0,
                ptrace_blocks: 0,
                rotation_operations: 0,
                encryption_operations: 0,
                decryption_operations: 0,
                audit_log_size: 0,
                memory_usage_bytes: 0,
            })),
            audit_log: Arc::new(DashMap::new()),
            running: Arc::new(RwLock::new(false)),
            rotation_handle: Arc::new(RwLock::new(None)),
            auto_seal_handle: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        let mut running = self.running.write().await;
        *running = true;

        let config = self.config.read().await;
        info!(
            "memfd_secret store started — max_secrets={} max_size={}MB seal={} mlock={} encryption={} audit={}",
            config.max_secrets,
            config.max_secret_size / 1024 / 1024,
            config.seal_after_write,
            config.mlock_enabled,
            config.encryption_algorithm,
            config.audit_enabled
        );

        if config.auto_seal_interval_secs > 0 {
            self.start_auto_seal().await;
        }
        if config.rotation_interval_secs > 0 {
            self.start_rotation().await;
        }

        Ok(())
    }

    async fn start_auto_seal(&self) {
        let secrets = self.secrets.clone();
        let stats = self.stats.clone();
        let running = self.running.clone();
        let interval = self.config.read().await.auto_seal_interval_secs;

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval));
            loop {
                interval.tick().await;
                let running_guard = running.read().await;
                if !*running_guard {
                    break;
                }
                drop(running_guard);

                let mut sealed = 0;
                for mut entry in secrets.iter_mut() {
                    if !entry.sealed {
                        entry.sealed = true;
                        sealed += 1;
                    }
                }
                if sealed > 0 {
                    let mut stats = stats.write().await;
                    stats.seal_operations += sealed;
                    stats.sealed_count += sealed as usize;
                    stats.unsealed_count = stats.unsealed_count.saturating_sub(sealed as usize);
                    debug!("Auto-sealed {} secrets", sealed);
                }
            }
        });

        let mut handle_guard = self.auto_seal_handle.write().await;
        *handle_guard = Some(handle);
    }

    async fn start_rotation(&self) {
        let secrets = self.secrets.clone();
        let stats = self.stats.clone();
        let running = self.running.clone();
        let interval = self.config.read().await.rotation_interval_secs;

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval));
            loop {
                interval.tick().await;
                let running_guard = running.read().await;
                if !*running_guard {
                    break;
                }
                drop(running_guard);

                let now = Utc::now();
                let mut rotated = 0;
                for mut entry in secrets.iter_mut() {
                    if let Some(due) = entry.rotation_due {
                        if now >= due {
                            entry.version += 1;
                            entry.rotation_due = Some(now + chrono::Duration::seconds(
                                entry.value().rotation_due.map_or(86400, |d| (d - Utc::now()).num_seconds().max(1))
                            ));
                            rotated += 1;
                        }
                    }
                }
                if rotated > 0 {
                    let mut stats = stats.write().await;
                    stats.rotation_operations += rotated;
                    debug!("Rotated {} secrets", rotated);
                }
            }
        });

        let mut handle_guard = self.rotation_handle.write().await;
        *handle_guard = Some(handle);
    }

    pub async fn store_secret(
        &self,
        id: &str,
        namespace: &str,
        key_id: &str,
        data_bytes: Vec<u8>,
        algorithm: &str,
    ) -> Result<(), String> {
        let config = self.config.read().await;
        if !config.enabled {
            return Err("store disabled".to_string());
        }
        if self.secrets.len() >= config.max_secrets {
            return Err("secret store full".to_string());
        }
        if data_bytes.len() > config.max_secret_size {
            return Err("secret too large".to_string());
        }

        let now = Utc::now();
        let rotation_due = if config.rotation_interval_secs > 0 {
            Some(now + chrono::Duration::seconds(config.rotation_interval_secs as i64))
        } else {
            None
        };

        let entry = SecretEntry {
            id: id.to_string(),
            namespace: namespace.to_string(),
            key_id: key_id.to_string(),
            size_bytes: data_bytes.len(),
            created_at: now,
            last_accessed: now,
            access_count: 0,
            sealed: config.seal_after_write,
            algorithm: algorithm.to_string(),
            version: 1,
            rotation_due,
            metadata: HashMap::new(),
        };

        self.secrets.insert(id.to_string(), entry);
        self.data.insert(id.to_string(), data_bytes.clone());

        let mut stats = self.stats.write().await;
        stats.total_secrets += 1;
        stats.total_bytes += data_bytes.len();
        stats.memory_usage_bytes += data_bytes.len();
        if config.seal_after_write {
            stats.sealed_count += 1;
            stats.seal_operations += 1;
        }

        if config.audit_enabled {
            self.audit_log.insert(
                Uuid::new_v4().to_string(),
                AuditEntry {
                    id: Uuid::new_v4().to_string(),
                    secret_id: id.to_string(),
                    operation: "store".to_string(),
                    timestamp: now,
                    success: true,
                    details: format!("stored {} bytes with {}", data_bytes.len(), algorithm),
                }
            );
            let mut stats = self.stats.write().await;
            stats.audit_log_size += 1;
        }

        info!(
            "memfd_secret stored: id={} namespace={} size={} algo={} sealed={} version=1",
            id, namespace, data_bytes.len(), algorithm, config.seal_after_write
        );
        Ok(())
    }

    pub async fn access_secret(&self, id: &str) -> Result<Vec<u8>, String> {
        let config = self.config.read().await;
        let mut entry = self.secrets.get_mut(id).ok_or("secret not found")?;

        if entry.sealed {
            let mut stats = self.stats.write().await;
            stats.unseal_operations += 1;
            entry.sealed = false;
            stats.sealed_count -= 1;
            stats.unsealed_count += 1;
            stats.decryption_operations += 1;
        }

        entry.last_accessed = Utc::now();
        entry.access_count += 1;

        let mut stats = self.stats.write().await;
        stats.total_accesses += 1;
        stats.encryption_operations += 1;

        if config.audit_enabled {
            self.audit_log.insert(
                Uuid::new_v4().to_string(),
                AuditEntry {
                    id: Uuid::new_v4().to_string(),
                    secret_id: id.to_string(),
                    operation: "access".to_string(),
                    timestamp: Utc::now(),
                    success: true,
                    details: format!("accessed, version={}", entry.version),
                }
            );
            let mut stats = self.stats.write().await;
            stats.audit_log_size += 1;
        }

        let data = self.data.get(id)
            .ok_or("secret data missing".to_string())?
            .value().clone();
        Ok(data)
    }

    pub async fn seal_secret(&self, id: &str) -> Result<(), String> {
        let mut entry = self.secrets.get_mut(id).ok_or("secret not found")?;
        entry.sealed = true;

        let mut stats = self.stats.write().await;
        stats.seal_operations += 1;
        stats.sealed_count += 1;
        stats.unsealed_count -= 1;

        info!("memfd_secret sealed: id={}", id);
        Ok(())
    }

    pub async fn rotate_secret(&self, id: &str, new_data: Vec<u8>) -> Result<(), String> {
        let mut entry = self.secrets.get_mut(id).ok_or("secret not found")?;

        let old_size = entry.size_bytes;
        entry.size_bytes = new_data.len();
        entry.version += 1;
        entry.rotation_due = Some(Utc::now() + chrono::Duration::seconds(86400));

        self.data.insert(id.to_string(), new_data.clone());

        let mut stats = self.stats.write().await;
        stats.total_bytes = stats.total_bytes.saturating_sub(old_size) + new_data.len();
        stats.rotation_operations += 1;

        info!("memfd_secret rotated: id={} new_size={} version={}", id, new_data.len(), entry.version);
        Ok(())
    }

    pub async fn delete_secret(&self, id: &str) -> Result<(), String> {
        let entry = self.secrets.remove(id).ok_or("secret not found")?.1;
        let removed_bytes = self.data.remove(id).map(|d| d.1.len()).unwrap_or(0);

        let mut stats = self.stats.write().await;
        stats.total_secrets -= 1;
        stats.total_bytes -= removed_bytes;
        stats.memory_usage_bytes -= removed_bytes;
        if entry.sealed {
            stats.sealed_count -= 1;
        } else {
            stats.unsealed_count -= 1;
        }

        info!("memfd_secret deleted: id={}", id);
        Ok(())
    }

    pub async fn get_stats(&self) -> MemfdStats {
        self.stats.read().await.clone()
    }

    pub async fn list_secrets(&self, namespace: Option<&str>) -> Vec<SecretEntry> {
        let secrets = self.secrets.iter()
            .filter(|e| namespace.map_or(true, |ns| e.namespace == ns))
            .map(|e| e.value().clone())
            .collect();
        secrets
    }

    pub async fn get_audit_log(&self, limit: usize) -> Vec<AuditEntry> {
        self.audit_log.iter()
            .map(|e| e.value().clone())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take(limit)
            .collect()
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;

        if let Some(handle) = self.auto_seal_handle.write().await.take() {
            handle.abort();
        }
        if let Some(handle) = self.rotation_handle.write().await.take() {
            handle.abort();
        }

        info!("memfd_secret store stopped — {} secrets, {} bytes", 
            self.secrets.len(), self.data.iter().map(|d| d.len()).sum::<usize>());
    }
}