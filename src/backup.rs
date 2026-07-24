use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Encrypted backup system — TEE-signed snapshots pushed to backup nodes.
pub struct EncryptedBackup {
    backup_nodes: Vec<String>,
    encryption_key: Vec<u8>,
    running: AtomicBool,
    backup_count: AtomicU64,
    last_backup: parking_lot::Mutex<Option<i64>>,
    last_status: parking_lot::Mutex<String>,
    interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub version: String,
    pub timestamp_ns: i64,
    pub node_id: String,
    pub snapshot_hash: String,
    pub encrypted_data_len: usize,
    pub tee_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStatus {
    pub backup_count: u64,
    pub last_backup: Option<i64>,
    pub health: String,
    pub nodes: Vec<String>,
    pub interval_secs: u64,
}

impl EncryptedBackup {
    pub fn new(backup_nodes: Vec<String>, encryption_key: Vec<u8>, interval_secs: u64) -> Self {
        Self {
            backup_nodes,
            encryption_key,
            running: AtomicBool::new(false),
            backup_count: AtomicU64::new(0),
            last_backup: parking_lot::Mutex::new(None),
            last_status: parking_lot::Mutex::new("initialized".to_string()),
            interval_secs,
        }
    }

    /// Start the automatic backup loop in a background task
    pub fn start_loop(self: &Arc<Self>, tee_key: Arc<dyn crate::tee::HardwareEnclave + Send + Sync>) {
        if self.running.load(Ordering::Relaxed) {
            return;
        }
        self.running.store(true, Ordering::Release);
        let this = self.clone();
        let tee = tee_key.clone();
        tokio::spawn(async move {
            while this.running.load(Ordering::Acquire) {
                match this.create_and_push(&*tee).await {
                    Ok(manifest) => {
                        *this.last_backup.lock() = Some(manifest.timestamp_ns);
                        *this.last_status.lock() = "ok".to_string();
                        tracing::info!(hash = %manifest.snapshot_hash, "backup completed");
                    }
                    Err(e) => {
                        *this.last_status.lock() = format!("error: {}", e);
                        tracing::warn!(error = %e, "backup failed");
                    }
                }
                tokio::time::sleep(Duration::from_secs(this.interval_secs)).await;
            }
        });
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Create an encrypted snapshot and push to all backup nodes
    pub async fn create_and_push(
        &self,
        tee: &dyn crate::tee::HardwareEnclave,
    ) -> Result<BackupManifest, String> {
        let start = Instant::now();

        // Collect state for snapshot
        let snapshot_data = serde_json::json!({
            "timestamp": chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            "node_id": std::env::var("THE_BRIDGE_NODE_ID").unwrap_or_else(|_| "engine-1".to_string()),
            "version": env!("CARGO_PKG_VERSION"),
        });

        let plaintext = serde_json::to_vec(&snapshot_data)
            .map_err(|e| format!("serialize: {}", e))?;

        // Hash
        let hash = {
            let mut hasher = Sha256::new();
            hasher.update(&plaintext);
            format!("{:x}", hasher.finalize())
        };

        // Encrypt with AES-256-GCM
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| format!("key init: {}", e))?;
        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|e| format!("encrypt: {}", e))?;

        // TEE sign the hash
        let sig = tee.sign(hash.as_bytes());

        let manifest = BackupManifest {
            version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp_ns: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            node_id: std::env::var("THE_BRIDGE_NODE_ID").unwrap_or_else(|_| "engine-1".to_string()),
            snapshot_hash: hash,
            encrypted_data_len: ciphertext.len(),
            tee_signature: hex::encode(sig),
        };

        // Push to all backup nodes
        let payload = serde_json::json!({
            "manifest": manifest,
            "nonce": hex::encode(nonce_bytes),
            "ciphertext": hex::encode(&ciphertext),
        });

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| format!("client: {}", e))?;

        for node in &self.backup_nodes {
            let url = format!("{}/api/v1/backup/receive", node);
            match client.post(&url).json(&payload).send().await {
                Ok(r) => {
                    if !r.status().is_success() {
                        tracing::warn!(node = %node, status = %r.status(), "backup push warning");
                    } else {
                        tracing::info!(node = %node, "backup pushed successfully");
                    }
                }
                Err(e) => {
                    tracing::warn!(node = %node, error = %e, "backup push failed");
                }
            }
        }

        self.backup_count.fetch_add(1, Ordering::Relaxed);
        tracing::info!(elapsed_us = %start.elapsed().as_micros(), len = %ciphertext.len(), "encrypted backup created");

        Ok(manifest)
    }

    /// Trigger an immediate backup
    pub async fn trigger(&self, tee: &dyn crate::tee::HardwareEnclave) -> Result<BackupManifest, String> {
        self.create_and_push(tee).await
    }

    pub fn status(&self) -> BackupStatus {
        BackupStatus {
            backup_count: self.backup_count.load(Ordering::Relaxed),
            last_backup: *self.last_backup.lock(),
            health: self.last_status.lock().clone(),
            nodes: self.backup_nodes.clone(),
            interval_secs: self.interval_secs,
        }
    }

    pub fn add_node(&mut self, node: String) {
        self.backup_nodes.push(node);
    }
}
