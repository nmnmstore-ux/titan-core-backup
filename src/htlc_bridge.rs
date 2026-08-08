use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug};
use dashmap::DashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use sha2::{Sha256, Digest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HTLCConfig {
    pub default_timeout_secs: u64,
    pub min_timeout_secs: u64,
    pub max_timeout_secs: u64,
    pub supported_chains: Vec<String>,
    pub fee_bps: u32,
    pub max_concurrent_contracts: usize,
    pub auto_refund_enabled: bool,
    pub refund_grace_period_secs: u64,
    pub partial_claim_enabled: bool,
    pub multi_sig_threshold: Option<u8>,
    pub dispute_window_secs: u64,
    pub oracle_timeout_secs: u64,
}

impl Default for HTLCConfig {
    fn default() -> Self {
        Self {
            default_timeout_secs: 3600,
            min_timeout_secs: 300,
            max_timeout_secs: 86400,
            supported_chains: vec![
                "ethereum".to_string(),
                "polygon".to_string(),
                "bsc".to_string(),
                "arbitrum".to_string(),
                "optimism".to_string(),
                "avalanche".to_string(),
            ],
            fee_bps: 10,
            max_concurrent_contracts: 10000,
            auto_refund_enabled: true,
            refund_grace_period_secs: 300,
            partial_claim_enabled: true,
            multi_sig_threshold: None,
            dispute_window_secs: 86400,
            oracle_timeout_secs: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HTLCContract {
    pub contract_id: String,
    pub sender: String,
    pub receiver: String,
    pub hash_lock: Vec<u8>,
    pub amount: f64,
    pub token: String,
    pub chain: String,
    pub timeout: u64,
    pub created_at: DateTime<Utc>,
    pub status: HTLCStatus,
    pub tx_hash: Option<String>,
    pub claim_tx_hash: Option<String>,
    pub refund_tx_hash: Option<String>,
    pub preimage: Option<Vec<u8>>,
    pub partial_claims: Vec<PartialClaim>,
    pub multisig_signatures: Vec<String>,
    pub oracle_data: Option<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HTLCStatus {
    Pending,
    Funded,
    PartiallyClaimed,
    Claimed,
    Refunded,
    Expired,
    Disputed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialClaim {
    pub claim_id: String,
    pub amount: f64,
    pub preimage: Vec<u8>,
    pub claimed_at: DateTime<Utc>,
    pub tx_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HTLCStats {
    pub total_contracts: u64,
    pub active: u64,
    pub claimed: u64,
    pub refunded: u64,
    pub expired: u64,
    pub disputed: u64,
    pub cancelled: u64,
    pub total_volume: f64,
    pub total_fees: f64,
    pub avg_claim_time_secs: f64,
    pub success_rate: f64,
}

pub struct HTLCBridge {
    config: Arc<RwLock<HTLCConfig>>,
    contracts: Arc<DashMap<String, HTLCContract>>,
    stats: Arc<RwLock<HTLCStats>>,
    running: Arc<RwLock<bool>>,
    monitor_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl HTLCBridge {
    pub fn new(config: HTLCConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            contracts: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(HTLCStats {
                total_contracts: 0,
                active: 0,
                claimed: 0,
                refunded: 0,
                expired: 0,
                disputed: 0,
                cancelled: 0,
                total_volume: 0.0,
                total_fees: 0.0,
                avg_claim_time_secs: 0.0,
                success_rate: 0.0,
            })),
            running: Arc::new(RwLock::new(false)),
            monitor_handle: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        let mut running = self.running.write().await;
        *running = true;

        let config = self.config.read().await;
        info!(
            "HTLC Bridge started — chains={} timeout={}s fee={}bps max_contracts={} auto_refund={}",
            config.supported_chains.len(), config.default_timeout_secs, config.fee_bps, config.max_concurrent_contracts, config.auto_refund_enabled
        );

        self.start_monitor().await;
        Ok(())
    }

    async fn start_monitor(&self) {
        let contracts = self.contracts.clone();
        let stats = self.stats.clone();
        let running = self.running.clone();
        let config = self.config.clone();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let running_guard = running.read().await;
                if !*running_guard {
                    break;
                }
                drop(running_guard);

                let now = Utc::now().timestamp() as u64;
                let mut expired_count = 0;
                let mut refunded_count = 0;

                for mut entry in contracts.iter_mut() {
                    let contract = entry.value_mut();
                    if contract.status == HTLCStatus::Funded || contract.status == HTLCStatus::PartiallyClaimed {
                        if now >= contract.timeout {
                            let config_guard = config.read().await;
                            if config_guard.auto_refund_enabled {
                                contract.status = HTLCStatus::Refunded;
                                refunded_count += 1;
                            } else {
                                contract.status = HTLCStatus::Expired;
                                expired_count += 1;
                            }
                        }
                    }
                }

                if expired_count > 0 || refunded_count > 0 {
                    let mut stats = stats.write().await;
                    stats.expired += expired_count;
                    stats.refunded += refunded_count;
                    stats.active = stats.active.saturating_sub(expired_count + refunded_count);
                    
                    let total = stats.claimed + stats.refunded + stats.expired;
                    stats.success_rate = if total > 0 { stats.claimed as f64 / total as f64 } else { 0.0 };
                    
                    debug!("HTLC Monitor: expired={} refunded={}", expired_count, refunded_count);
                }
            }
        });

        let mut handle_guard = self.monitor_handle.write().await;
        *handle_guard = Some(handle);
    }

    pub async fn create_contract(
        &self,
        sender: &str,
        receiver: &str,
        amount: f64,
        token: &str,
        chain: &str,
        hash_lock: Vec<u8>,
        timeout: Option<u64>,
        multisig: Option<Vec<String>>,
    ) -> Result<HTLCContract, String> {
        let config = self.config.read().await;
        if !config.supported_chains.contains(&chain.to_string()) {
            return Err(format!("unsupported chain: {}", chain));
        }
        if self.contracts.len() >= config.max_concurrent_contracts {
            return Err("max concurrent contracts reached".to_string());
        }

        let timeout_secs = timeout.unwrap_or(config.default_timeout_secs);
        if timeout_secs < config.min_timeout_secs || timeout_secs > config.max_timeout_secs {
            return Err(format!(
                "timeout must be between {} and {} seconds",
                config.min_timeout_secs, config.max_timeout_secs
            ));
        }

        if hash_lock.len() != 32 {
            return Err("hash_lock must be 32 bytes (SHA256)".to_string());
        }

        let contract_id = format!("htlc_{}", Uuid::new_v4());
        let contract = HTLCContract {
            contract_id: contract_id.clone(),
            sender: sender.to_string(),
            receiver: receiver.to_string(),
            hash_lock,
            amount,
            token: token.to_string(),
            chain: chain.to_string(),
            timeout: now_timestamp() + timeout_secs,
            created_at: Utc::now(),
            status: HTLCStatus::Pending,
            tx_hash: None,
            claim_tx_hash: None,
            refund_tx_hash: None,
            preimage: None,
            partial_claims: vec![],
            multisig_signatures: multisig.unwrap_or_default(),
            oracle_data: None,
            metadata: HashMap::new(),
        };

        self.contracts.insert(contract_id.clone(), contract.clone());

        let mut stats = self.stats.write().await;
        stats.total_contracts += 1;
        stats.active += 1;
        stats.total_volume += amount;

        let fee = amount * config.fee_bps as f64 / 10_000.0;
        stats.total_fees += fee;

        info!(
            "HTLC contract created: id={} amount={} {} on {} timeout={}s",
            contract_id, amount, token, chain, timeout_secs
        );
        Ok(contract)
    }

    pub async fn fund_contract(&self, contract_id: &str, tx_hash: &str) -> Result<(), String> {
        let mut contract = self.contracts.get_mut(contract_id).ok_or("contract not found")?;
        
        if contract.status != HTLCStatus::Pending {
            return Err(format!("contract in invalid state: {:?}", contract.status));
        }

        contract.status = HTLCStatus::Funded;
        contract.tx_hash = Some(tx_hash.to_string());

        let mut stats = self.stats.write().await;
        stats.active += 1;

        info!("HTLC contract funded: id={} tx={}", contract_id, tx_hash);
        Ok(())
    }

    pub async fn claim(&self, contract_id: &str, preimage: &[u8]) -> Result<(), String> {
        let mut hasher = Sha256::new();
        hasher.update(preimage);
        let hash = hasher.finalize().to_vec();

        let mut contract = self.contracts.get_mut(contract_id).ok_or("contract not found")?;

        if contract.hash_lock != hash {
            return Err("invalid preimage".to_string());
        }

        if contract.status != HTLCStatus::Funded && contract.status != HTLCStatus::PartiallyClaimed {
            return Err(format!("contract in invalid state: {:?}", contract.status));
        }

        contract.status = HTLCStatus::Claimed;
        contract.preimage = Some(preimage.to_vec());

        let mut stats = self.stats.write().await;
        stats.active -= 1;
        stats.claimed += 1;

        let total = stats.claimed + stats.refunded + stats.expired;
        stats.success_rate = if total > 0 { stats.claimed as f64 / total as f64 } else { 0.0 };
        stats.avg_claim_time_secs = (Utc::now().timestamp() - contract.created_at.timestamp()) as f64;

        info!("HTLC contract claimed: id={}", contract_id);
        Ok(())
    }

    pub async fn partial_claim(&self, contract_id: &str, preimage: &[u8], amount: f64, claim_tx_hash: &str) -> Result<(), String> {
        let config = self.config.read().await;
        if !config.partial_claim_enabled {
            return Err("partial claims disabled".to_string());
        }
        drop(config);

        let mut hasher = Sha256::new();
        hasher.update(preimage);
        let hash = hasher.finalize().to_vec();

        let mut contract = self.contracts.get_mut(contract_id).ok_or("contract not found")?;
        
        if contract.hash_lock != hash {
            return Err("invalid preimage".to_string());
        }

        if contract.status != HTLCStatus::Funded && contract.status != HTLCStatus::PartiallyClaimed {
            return Err(format!("contract in invalid state: {:?}", contract.status));
        }

        if amount > contract.amount {
            return Err("claim amount exceeds contract amount".to_string());
        }

        let claim = PartialClaim {
            claim_id: format!("claim_{}", Uuid::new_v4()),
            amount,
            preimage: preimage.to_vec(),
            claimed_at: Utc::now(),
            tx_hash: claim_tx_hash.to_string(),
        };

        contract.partial_claims.push(claim);
        contract.status = HTLCStatus::PartiallyClaimed;

        info!("HTLC partial claim: id={} amount={}", contract_id, amount);
        Ok(())
    }

    pub async fn refund(&self, contract_id: &str) -> Result<(), String> {
        let now = now_timestamp();
        let mut contract = self.contracts.get_mut(contract_id).ok_or("contract not found")?;

        if now < contract.timeout {
            return Err("contract not yet expired".to_string());
        }

        contract.status = HTLCStatus::Refunded;

        let mut stats = self.stats.write().await;
        stats.active -= 1;
        stats.refunded += 1;

        info!("HTLC contract refunded: id={}", contract_id);
        Ok(())
    }

    pub async fn get_contract(&self, contract_id: &str) -> Option<HTLCContract> {
        self.contracts.get(contract_id).map(|e| e.value().clone())
    }

    pub async fn list_contracts(&self, status: Option<HTLCStatus>) -> Vec<HTLCContract> {
        match status {
            Some(s) => self.contracts.iter()
                .filter(|e| e.status == s)
                .map(|e| e.value().clone())
                .collect(),
            None => self.contracts.iter().map(|e| e.value().clone()).collect(),
        }
    }

    pub async fn get_stats(&self) -> HTLCStats {
        self.stats.read().await.clone()
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;

        if let Some(handle) = self.monitor_handle.write().await.take() {
            handle.abort();
        }
        info!("HTLC Bridge stopped");
    }
}

fn now_timestamp() -> u64 {
    Utc::now().timestamp() as u64
}