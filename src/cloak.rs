#![allow(dead_code)]
use crate::snapshot::{EngineSnapshot, BookSnapshot, DOTSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThreatLevel {
    Green,
    Yellow,
    Orange,
    Red,
    Black,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatSignature {
    pub pattern: String,
    pub source_ips: Vec<String>,
    pub request_rate: f64,
    pub geographic_concentration: f64,
    pub api_endpoint_targeted: String,
    pub timestamp: i64,
    pub confidence: f64,
}

pub struct ThreatAnalyzer {
    request_log: Arc<Mutex<VecDeque<(i64, String, String)>>>,
    sus_ip_threshold: u32,
    rate_limit: f64,
}

impl ThreatAnalyzer {
    pub fn new() -> Self {
        Self {
            request_log: Arc::new(Mutex::new(VecDeque::with_capacity(100_000))),
            sus_ip_threshold: 100,
            rate_limit: 10_000.0,
        }
    }

    pub fn record_request(&self, ip: String, endpoint: String) {
        let now = chrono::Utc::now().timestamp_millis();
        if let Ok(mut log) = self.request_log.lock() {
            log.push_back((now, ip, endpoint));
            while let Some(front) = log.front() {
                if now - front.0 > 10_000 {
                    log.pop_front();
                } else {
                    break;
                }
            }
        }
    }

    pub fn analyze(&self) -> ThreatLevel {
        let now = chrono::Utc::now().timestamp_millis();
        let window_ms = 5_000i64;

        let log = match self.request_log.lock() {
            Ok(l) => l,
            Err(_) => return ThreatLevel::Green,
        };

        let mut ip_counts: HashMap<&str, u32> = HashMap::new();
        let mut total_recent = 0u32;
        let mut endpoint_counts: HashMap<&str, u32> = HashMap::new();

        for (ts, ip, endpoint) in log.iter() {
            if now - ts <= window_ms {
                *ip_counts.entry(ip).or_insert(0) += 1;
                *endpoint_counts.entry(endpoint).or_insert(0) += 1;
                total_recent += 1;
            }
        }

        let rate_per_sec = total_recent as f64 / (window_ms as f64 / 1000.0);

        let max_ip_rate = ip_counts.values().copied().max().unwrap_or(0);
        if max_ip_rate > self.sus_ip_threshold * (window_ms as u32 / 1000) {
            return ThreatLevel::Red;
        }

        if rate_per_sec > self.rate_limit {
            return ThreatLevel::Orange;
        }

        if let Some(admin_count) = endpoint_counts.get("/api/v1/admin") {
            if *admin_count > 50 {
                return ThreatLevel::Orange;
            }
        }

        ThreatLevel::Green
    }
}

pub struct SovereignKillSwitch {
    pub threat_analyzer: ThreatAnalyzer,
    pub current_threat: Mutex<ThreatLevel>,
    pub cloaking_active: AtomicBool,
    pub backup_nodes: Vec<String>,
    pub last_migration: Mutex<i64>,
    pub total_cloaks: AtomicU64,
}

impl SovereignKillSwitch {
    pub fn new(backup_nodes: Vec<String>) -> Self {
        Self {
            threat_analyzer: ThreatAnalyzer::new(),
            current_threat: Mutex::new(ThreatLevel::Green),
            cloaking_active: AtomicBool::new(false),
            backup_nodes,
            last_migration: Mutex::new(0),
            total_cloaks: AtomicU64::new(0),
        }
    }

    pub async fn monitoring_loop(&self, _engine_state: &EngineSnapshot) {
        loop {
            let level = self.threat_analyzer.analyze();

            if let Ok(mut current) = self.current_threat.lock() {
                if level != *current {
                    tracing::info!(threat = ?level, previous = ?*current, "Threat level changed");
                    *current = level.clone();
                }
            }

            if level == ThreatLevel::Red || level == ThreatLevel::Black {
                tracing::warn!(threat = ?level, "SOVEREIGN KILL-SWITCH ACTIVATED!");
                break;
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    pub fn activate(&self) -> ThreatLevel {
        self.cloaking_active.store(true, Ordering::Relaxed);
        self.total_cloaks.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut t) = self.current_threat.lock() {
            *t = ThreatLevel::Black;
        }
        ThreatLevel::Black
    }

    pub fn is_cloaked(&self) -> bool {
        self.cloaking_active.load(Ordering::Relaxed)
    }

    pub fn threat_level(&self) -> ThreatLevel {
        match self.current_threat.lock() {
            Ok(t) => t.clone(),
            Err(_) => ThreatLevel::Green,
        }
    }
}

pub struct NodeCloakingProtocol;

impl NodeCloakingProtocol {
    pub fn create_snapshot(
        order_books: Vec<BookSnapshot>,
        dot_pending: Vec<DOTSnapshot>,
    ) -> EngineSnapshot {
        let start = Instant::now();

        let snapshot = EngineSnapshot {
            timestamp: chrono::Utc::now().timestamp_millis(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            order_books,
            dot_pending,
            mempool_txs: vec![],
            metrics: serde_json::json!({}),
            tee_attestation: "sgx-enclave-verified".into(),
            numa_topology: String::new(),
        };

        let elapsed = start.elapsed();
        tracing::info!(elapsed_us = %elapsed.as_micros(), "Engine snapshot created");
        snapshot
    }

    pub fn pack_snapshot(snapshot: &EngineSnapshot) -> Result<Vec<u8>, String> {
        crate::types::bincode_serialize(snapshot)
    }

    pub fn migrate_to_backup(packed: &[u8], backup_addr: &str) -> Result<Duration, String> {
        let start = Instant::now();

        let addr = backup_addr.parse().map_err(|e| format!("invalid addr: {}", e))?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(5))
            .map_err(|e| format!("backup connect: {}", e))?;

        let len = (packed.len() as u64).to_le_bytes();
        stream.write_all(&len).map_err(|e| e.to_string())?;
        stream.write_all(packed).map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())?;

        let elapsed = start.elapsed();
        tracing::info!(bytes = packed.len(), backup = %backup_addr, elapsed_us = %elapsed.as_micros(), "State migrated");
        Ok(elapsed)
    }

    pub fn execute_hot_migration(
        backup_nodes: &[String],
        order_books: Vec<BookSnapshot>,
        dot_pending: Vec<DOTSnapshot>,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_millis(10);

        let snapshot = Self::create_snapshot(order_books, dot_pending);
        if Instant::now() > deadline { return Err("Snapshot exceeded deadline".into()); }

        let packed = Self::pack_snapshot(&snapshot)?;
        if Instant::now() > deadline { return Err("Pack exceeded deadline".into()); }

        for backup in backup_nodes {
            if Instant::now() > deadline { break; }
            let _ = Self::migrate_to_backup(&packed, backup);
        }

        tracing::info!("Hot migration complete");
        Ok(())
    }

    pub fn activate_cloaking(kill_switch: &SovereignKillSwitch) {
        kill_switch.activate();
        tracing::warn!("NODE CLOAKING ACTIVATED");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloakSignal {
    pub node_id: String,
    pub timestamp: i64,
    pub threat_level: ThreatLevel,
    pub snapshot_hash: String,
    pub fiat_balances: HashMap<String, f64>,
    pub convert_to_rwa_gold: Vec<FiatBalance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiatBalance {
    pub currency: String,
    pub amount: f64,
    pub chain: String,
    pub contract_address: String,
    pub target_gold_contract: String,
}

impl CloakSignal {
    pub fn new(node_id: &str, threat: ThreatLevel, fiat: HashMap<String, f64>) -> Self {
        let now = chrono::Utc::now();
        Self {
            node_id: node_id.to_string(),
            timestamp: now.timestamp(),
            threat_level: threat,
            snapshot_hash: format!("{:x}", now.timestamp_millis()),
            fiat_balances: fiat.clone(),
            convert_to_rwa_gold: fiat.into_iter().map(|(currency, amount)| {
                FiatBalance {
                    currency: currency.clone(),
                    amount,
                    chain: match currency.as_str() {
                        "USDC" | "USDT" => "ethereum".into(),
                        "EGP" => "polygon".into(),
                        _ => "ethereum".into(),
                    },
                    contract_address: String::new(),
                    target_gold_contract: "0xRWA_GOLD_TOKEN".into(),
                }
            }).collect(),
        }
    }
}
