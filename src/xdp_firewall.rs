use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{RwLock, Semaphore};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error, debug};
use dashmap::DashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

const MAX_RULES: usize = 100_000;
const DEFAULT_RATE_LIMIT_PPS: u64 = 10_000;
const DEFAULT_CONNECTION_LIMIT: u32 = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XDPConfig {
    pub interface: String,
    pub rate_limit_pps: u64,
    pub connection_limit: u32,
    pub enable_ghost_drop: bool,
    pub enable_kill_switch: bool,
    pub enable_ddos_protection: bool,
    pub geo_block_enabled: bool,
    pub blocked_countries: Vec<String>,
    pub max_packet_size: u32,
    pub enable_syn_cookies: bool,
    pub enable_tls_inspection: bool,
    pub log_level: XDPLogLevel,
    pub metrics_interval_secs: u64,
}

impl Default for XDPConfig {
    fn default() -> Self {
        Self {
            interface: "eth0".to_string(),
            rate_limit_pps: DEFAULT_RATE_LIMIT_PPS,
            connection_limit: DEFAULT_CONNECTION_LIMIT,
            enable_ghost_drop: true,
            enable_kill_switch: true,
            enable_ddos_protection: true,
            geo_block_enabled: false,
            blocked_countries: vec![],
            max_packet_size: 1514,
            enable_syn_cookies: true,
            enable_tls_inspection: false,
            log_level: XDPLogLevel::Info,
            metrics_interval_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum XDPLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum XDPRuleType {
    RateLimit,
    GeoBlock,
    IpBlock,
    PortBlock,
    Protocol,
    DDoS,
    GhostDrop,
    KillSwitch,
    TLSInspection,
    PayloadInspection,
    AnomalyDetection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum XDPAction {
    Pass,
    Drop,
    Redirect(u32),
    Tx,
    Abort,
    LogOnly,
    Quarantine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XDPMatch {
    pub src_ip: Option<String>,
    pub dst_ip: Option<String>,
    pub src_port: Option<u16>,
    pub dst_port: Option<u16>,
    pub protocol: Option<String>,
    pub country: Option<String>,
    pub packet_size_gt: Option<u32>,
    pub packet_size_lt: Option<u32>,
    pub tls_sni: Option<String>,
    pub payload_regex: Option<String>,
    pub anomaly_score_gt: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XDPRule {
    pub id: u64,
    pub name: String,
    pub rule_type: XDPRuleType,
    pub action: XDPAction,
    pub priority: u32,
    pub criteria: XDPMatch,
    pub hit_count: u64,
    pub last_hit: Option<DateTime<Utc>>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XDPStats {
    pub packets_processed: u64,
    pub packets_dropped: u64,
    pub packets_passed: u64,
    pub packets_redirected: u64,
    pub packets_quarantined: u64,
    pub bytes_processed: u64,
    pub bytes_dropped: u64,
    pub active_rules: u32,
    pub rate_limit_triggers: u64,
    pub ddos_mitigations: u64,
    pub ghost_drops: u64,
    pub kill_switch_activations: u64,
    pub syn_cookie_generations: u64,
    pub tls_inspections: u64,
    pub anomaly_detections: u64,
    pub uptime_secs: u64,
    pub avg_latency_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionEntry {
    pub src_ip: String,
    pub dst_port: u16,
    pub packet_count: u64,
    pub byte_count: u64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub state: ConnectionState,
    pub anomaly_score: f64,
    pub tls_info: Option<TLSInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionState {
    New,
    Established,
    Closing,
    RateLimited,
    Blocked,
    Quarantined,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TLSInfo {
    pub version: String,
    pub cipher_suite: String,
    pub sni: Option<String>,
    pub cert_fingerprint: Option<String>,
}

pub struct EBPFXDPGhostDrop {
    config: Arc<RwLock<XDPConfig>>,
    rules: Arc<DashMap<u64, XDPRule>>,
    stats: Arc<RwLock<XDPStats>>,
    connections: Arc<DashMap<String, ConnectionEntry>>,
    kill_switch_active: Arc<RwLock<bool>>,
    running: Arc<RwLock<bool>>,
    next_rule_id: Arc<RwLock<u64>>,
    metrics_semaphore: Arc<Semaphore>,
    start_time: DateTime<Utc>,
}

impl EBPFXDPGhostDrop {
    pub fn new(config: XDPConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            rules: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(XDPStats {
                packets_processed: 0,
                packets_dropped: 0,
                packets_passed: 0,
                packets_redirected: 0,
                packets_quarantined: 0,
                bytes_processed: 0,
                bytes_dropped: 0,
                active_rules: 0,
                rate_limit_triggers: 0,
                ddos_mitigations: 0,
                ghost_drops: 0,
                kill_switch_activations: 0,
                syn_cookie_generations: 0,
                tls_inspections: 0,
                anomaly_detections: 0,
                uptime_secs: 0,
                avg_latency_ns: 0,
            })),
            connections: Arc::new(DashMap::new()),
            kill_switch_active: Arc::new(RwLock::new(false)),
            running: Arc::new(RwLock::new(false)),
            next_rule_id: Arc::new(RwLock::new(1)),
            metrics_semaphore: Arc::new(Semaphore::new(1000)),
            start_time: Utc::now(),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        let mut running = self.running.write().await;
        *running = true;

        let config = self.config.read().await;
        info!(
            "eBPF/XDP Ghost Drop started on {} — rate_limit={}pps conn_limit={} ghost_drop={} kill_switch={} tls_inspection={}",
            config.interface,
            config.rate_limit_pps,
            config.connection_limit,
            config.enable_ghost_drop,
            config.enable_kill_switch,
            config.enable_tls_inspection,
        );

        if config.enable_kill_switch {
            warn!("eBPF/XDP Kill Switch ARMED — emergency drop all enabled");
        }

        self.start_metrics_collector().await;
        Ok(())
    }

    async fn start_metrics_collector(&self) {
        let stats = self.stats.clone();
        let config = self.config.clone();
        let running = self.running.clone();
        let start_time = self.start_time;
        let metrics_interval_secs = self.config.read().await.metrics_interval_secs;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs(metrics_interval_secs)
            );
            loop {
                interval.tick().await;
                let running_guard = running.read().await;
                if !*running_guard {
                    break;
                }
                drop(running_guard);

                let mut stats_guard = stats.write().await;
                stats_guard.uptime_secs = (Utc::now() - start_time).num_seconds() as u64;
                
                let config_guard = config.read().await;
                debug!(
                    "XDP Metrics: pps={} dropped={} dropped_pct={:.2}% avg_latency={}ns",
                    stats_guard.packets_processed,
                    stats_guard.packets_dropped,
                    if stats_guard.packets_processed > 0 {
                        (stats_guard.packets_dropped as f64 / stats_guard.packets_processed as f64) * 100.0
                    } else { 0.0 },
                    stats_guard.avg_latency_ns,
                );
            }
        });
    }

    pub async fn add_rule(&self, mut rule: XDPRule) -> Result<u64, String> {
        if self.rules.len() >= MAX_RULES {
            return Err("rule table full".to_string());
        }

        let mut next_id = self.next_rule_id.write().await;
        let id = *next_id;
        *next_id += 1;
        
        rule.id = id;
        rule.created_at = Utc::now();
        rule.updated_at = Utc::now();

        self.rules.insert(id, rule);

        let mut stats = self.stats.write().await;
        stats.active_rules = self.rules.len() as u32;

        info!("eBPF/XDP rule added: id={}", id);
        Ok(id)
    }

    pub async fn remove_rule(&self, id: u64) -> Result<(), String> {
        if self.rules.remove(&id).is_none() {
            return Err(format!("rule {} not found", id));
        }

        let mut stats = self.stats.write().await;
        stats.active_rules = self.rules.len() as u32;

        info!("eBPF/XDP rule removed: id={}", id);
        Ok(())
    }

    pub async fn update_rule(&self, id: u64, mut rule: XDPRule) -> Result<(), String> {
        let mut entry = self.rules.get_mut(&id)
            .ok_or_else(|| format!("rule {} not found", id))?;
        
        rule.id = id;
        rule.updated_at = Utc::now();
        *entry = rule;

        info!("eBPF/XDP rule updated: id={}", id);
        Ok(())
    }

    pub async fn process_packet(&self, packet: &PacketInfo) -> XDPAction {
        let start = std::time::Instant::now();
        
        let _permit = self.metrics_semaphore.acquire().await.ok();
        
        let mut stats = self.stats.write().await;
        stats.packets_processed += 1;
        stats.bytes_processed += packet.size as u64;

        let kill_active = *self.kill_switch_active.read().await;
        if kill_active {
            stats.packets_dropped += 1;
            stats.bytes_dropped += packet.size as u64;
            stats.kill_switch_activations += 1;
            return XDPAction::Drop;
        }

        let config = self.config.read().await;
        if config.enable_ddos_protection && packet.size > config.max_packet_size {
            stats.packets_dropped += 1;
            stats.bytes_dropped += packet.size as u64;
            stats.ddos_mitigations += 1;
            return XDPAction::Drop;
        }

        let mut best_priority = u32::MAX;
        let mut matched_action = XDPAction::Pass;
        let mut matched_rule_id: Option<u64> = None;

        for entry in self.rules.iter() {
            let rule = entry.value();
            if !rule.enabled {
                continue;
            }
            if rule.priority < best_priority && Self::matches_rule(packet, &rule.criteria) {
                best_priority = rule.priority;
                matched_action = rule.action.clone();
                matched_rule_id = Some(rule.id);
            }
        }

        if let Some(rule_id) = matched_rule_id {
            if let Some(mut entry) = self.rules.get_mut(&rule_id) {
                entry.hit_count += 1;
                entry.last_hit = Some(Utc::now());
            }
        }

        match &matched_action {
            XDPAction::Drop => {
                stats.packets_dropped += 1;
                stats.bytes_dropped += packet.size as u64;
                stats.ghost_drops += 1;
            }
            XDPAction::Pass => {
                stats.packets_passed += 1;
            }
            XDPAction::Redirect(_) => {
                stats.packets_redirected += 1;
            }
            XDPAction::Quarantine => {
                stats.packets_quarantined += 1;
            }
            XDPAction::LogOnly => {
                stats.packets_passed += 1;
                debug!("XDP LogOnly: packet from {} allowed", packet.src_ip);
            }
            _ => {
                stats.packets_passed += 1;
            }
        }

        stats.avg_latency_ns = ((stats.avg_latency_ns * (stats.packets_processed - 1)) 
            + start.elapsed().as_nanos() as u64) / stats.packets_processed;

        matched_action
    }

    fn matches_rule(packet: &PacketInfo, criteria: &XDPMatch) -> bool {
        if let Some(ref src) = criteria.src_ip {
            if !packet.src_ip.starts_with(src.trim_end_matches('/')) {
                return false;
            }
        }
        if let Some(ref dst) = criteria.dst_ip {
            if !packet.dst_ip.starts_with(dst.trim_end_matches('/')) {
                return false;
            }
        }
        if let Some(port) = criteria.src_port {
            if packet.src_port != port {
                return false;
            }
        }
        if let Some(port) = criteria.dst_port {
            if packet.dst_port != port {
                return false;
            }
        }
        if let Some(min_size) = criteria.packet_size_gt {
            if packet.size <= min_size {
                return false;
            }
        }
        if let Some(max_size) = criteria.packet_size_lt {
            if packet.size >= max_size {
                return false;
            }
        }
        true
    }

    pub async fn activate_kill_switch(&self) {
        let mut ks = self.kill_switch_active.write().await;
        *ks = true;
        error!("eBPF/XDP KILL SWITCH ACTIVATED — all packets will be dropped");
    }

    pub async fn deactivate_kill_switch(&self) {
        let mut ks = self.kill_switch_active.write().await;
        *ks = false;
        info!("eBPF/XDP Kill Switch deactivated — traffic resumed");
    }

    pub async fn is_kill_switch_active(&self) -> bool {
        *self.kill_switch_active.read().await
    }

    pub async fn get_stats(&self) -> XDPStats {
        self.stats.read().await.clone()
    }

    pub async fn get_rules(&self) -> Vec<XDPRule> {
        self.rules.iter().map(|e| e.value().clone()).collect()
    }

    pub async fn get_rule(&self, id: u64) -> Option<XDPRule> {
        self.rules.get(&id).map(|e| e.value().clone())
    }

    pub async fn get_connections(&self) -> Vec<ConnectionEntry> {
        self.connections.iter().map(|e| e.value().clone()).collect()
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("eBPF/XDP Ghost Drop stopped");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketInfo {
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: String,
    pub size: u32,
    pub timestamp: DateTime<Utc>,
    pub tls_sni: Option<String>,
    pub payload_hash: Option<String>,
}