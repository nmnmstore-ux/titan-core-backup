use chrono::Timelike;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ==================== Constants ====================

pub const GHOST_MIN_JITTER_MS: u64 = 5;
pub const GHOST_MAX_JITTER_MS: u64 = 50;
pub const GHOST_MAX_FRAGMENTS: usize = 5;
pub const GHOST_MIN_FRAGMENTS: usize = 2;
pub const GHOST_FRAGMENT_MIN_VALUE: f64 = 50.0;
pub const GHOST_MAX_BROKERS: usize = 8;
pub const GHOST_SETTLEMENT_TIMEOUT_MS: u64 = 5000;
pub const GHOST_RECONCILE_INTERVAL_SECS: u64 = 60;

// ==================== Broker Evasion Types ====================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BrokerEvasionStrategy {
    SplitEven,       // تقسيم equally على كل الـ brokers
    SplitWeighted,   // توزيع حسب الوزن (لتفادي pattern detection)
    RandomSplit,     // كل fragment حجم عشوائي
    TimeSpread,      // كل fragment على فاصل زمني مختلف
    VolumeMatch,     // يقلد حجم trades حقيقية عشان يندمج
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerEndpoint {
    pub id: String,
    pub name: String,
    pub url: String,
    pub weight: f64,             // 0.0 - 1.0, وزن في التوزيع
    pub max_order_size: f64,     // الحد الأقصى لكل order
    pub latency_base_us: u64,    // latency base
    pub is_active: bool,
    pub total_routed: u64,
    pub last_used: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderFragment {
    pub fragment_id: String,
    pub phantom_id: String,
    pub broker_id: String,
    pub pair: String,
    pub quantity: f64,
    pub price: f64,
    pub side: String,
    pub timestamp_ns: u64,
    pub delay_ms: u64,           // تأخير متعمد before إرسال
    pub status: FragmentStatus,
    pub settlement_tx: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FragmentStatus {
    Pending,
    Dispatched,
    Settled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSplitPlan {
    pub original_order_id: String,
    pub user_id: String,
    pub total_quantity: f64,
    pub total_value: f64,
    pub fragments: Vec<OrderFragment>,
    pub strategy: BrokerEvasionStrategy,
    pub created_at_ns: u64,
    pub reconciled: bool,
    pub settled_count: u64,
    pub failed_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingObfuscation {
    pub base_delay_ms: u64,
    pub jitter_range_ms: u64,
    pub inter_fragment_gap_ms: u64,
    pub random_priority: bool,
    pub simulate_human_pattern: bool,
}

impl Default for TimingObfuscation {
    fn default() -> Self {
        Self {
            base_delay_ms: GHOST_MIN_JITTER_MS,
            jitter_range_ms: GHOST_MAX_JITTER_MS,
            inter_fragment_gap_ms: 10,
            random_priority: true,
            simulate_human_pattern: true,
        }
    }
}

// ==================== Ghost Cloak Core ====================

pub struct GhostCloak {
    // Identity
    phantom_id_counter: AtomicU64,
    phantom_map: Arc<RwLock<HashMap<String, String>>>,  // phantom_id -> user_id

    // Fragment tracking
    active_splits: Arc<RwLock<Vec<OrderSplitPlan>>>,
    fragment_by_broker: Arc<RwLock<HashMap<String, Vec<OrderFragment>>>>,

    // Broker routing table
    brokers: Arc<RwLock<Vec<BrokerEndpoint>>>,

    // Stats
    cloaked_count: AtomicU64,
    total_jitter_ns: AtomicU64,
    total_fragments: AtomicU64,
    fragments_settled: AtomicU64,
    fragments_failed: AtomicU64,
    total_fragmented_orders: AtomicU64,
    broker_route_count: Arc<RwLock<HashMap<String, u64>>>,

    // Config
    timing: Arc<RwLock<TimingObfuscation>>,
    evasion_strategy: Arc<RwLock<BrokerEvasionStrategy>>,
    max_fragments: usize,
    min_fragments: usize,
}

impl GhostCloak {
    pub fn new() -> Self {
        let mut brokers = Vec::new();
        // Default broker endpoints (configurable via env var or API)
        brokers.push(BrokerEndpoint {
            id: "broker-alpha".into(),
            name: "Alpha Liquidity".into(),
            url: "https://api.alpha-liquidity.com/v1".into(),
            weight: 0.25,
            max_order_size: 500_000.0,
            latency_base_us: 1200,
            is_active: true,
            total_routed: 0,
            last_used: 0,
        });
        brokers.push(BrokerEndpoint {
            id: "broker-beta".into(),
            name: "Beta Markets".into(),
            url: "https://api.beta-markets.com/v1".into(),
            weight: 0.25,
            max_order_size: 300_000.0,
            latency_base_us: 900,
            is_active: true,
            total_routed: 0,
            last_used: 0,
        });
        brokers.push(BrokerEndpoint {
            id: "broker-gamma".into(),
            name: "Gamma Dark Pool".into(),
            url: "https://api.gamma-darkpool.io/v1".into(),
            weight: 0.30,
            max_order_size: 1_000_000.0,
            latency_base_us: 1500,
            is_active: true,
            total_routed: 0,
            last_used: 0,
        });
        brokers.push(BrokerEndpoint {
            id: "broker-delta".into(),
            name: "Delta OTC".into(),
            url: "https://api.delta-otc.com/v1".into(),
            weight: 0.20,
            max_order_size: 2_000_000.0,
            latency_base_us: 2000,
            is_active: true,
            total_routed: 0,
            last_used: 0,
        });

        Self {
            phantom_id_counter: AtomicU64::new(1),
            phantom_map: Arc::new(RwLock::new(HashMap::new())),
            active_splits: Arc::new(RwLock::new(Vec::new())),
            fragment_by_broker: Arc::new(RwLock::new(HashMap::new())),
            brokers: Arc::new(RwLock::new(brokers)),
            cloaked_count: AtomicU64::new(0),
            total_jitter_ns: AtomicU64::new(0),
            total_fragments: AtomicU64::new(0),
            fragments_settled: AtomicU64::new(0),
            fragments_failed: AtomicU64::new(0),
            total_fragmented_orders: AtomicU64::new(0),
            broker_route_count: Arc::new(RwLock::new(HashMap::new())),
            timing: Arc::new(RwLock::new(TimingObfuscation::default())),
            evasion_strategy: Arc::new(RwLock::new(BrokerEvasionStrategy::RandomSplit)),
            max_fragments: GHOST_MAX_FRAGMENTS,
            min_fragments: GHOST_MIN_FRAGMENTS,
        }
    }

    // ==================== Phantom Identity ====================

    fn generate_phantom_id(&self, user_id: &str, order_id: &str) -> String {
        let counter = self.phantom_id_counter.fetch_add(1, Ordering::Relaxed);
        let input = format!("{}:{}:{}:{}", user_id, order_id, counter, rand::random::<u64>());
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        format!("phantom_{}", &hash[..40])
    }

    // ==================== Order Fragmentation Engine ====================

    pub fn split_order(
        &self,
        order_id: &str,
        user_id: &str,
        pair: &str,
        quantity: f64,
        price: f64,
        side: &str,
        value: f64,
    ) -> OrderSplitPlan {
        let strategy = self.evasion_strategy.blocking_read().clone();
        let brokers = self.brokers.blocking_read();
        let active_brokers: Vec<&BrokerEndpoint> = brokers.iter().filter(|b| b.is_active).collect();

        if active_brokers.is_empty() || value <= GHOST_FRAGMENT_MIN_VALUE * 2.0 {
            // Small order — no split, just cloak
            let phantom_id = self.generate_phantom_id(user_id, order_id);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;

            let fragment = OrderFragment {
                fragment_id: format!("{}-single", phantom_id),
                phantom_id: phantom_id.clone(),
                broker_id: "direct".into(),
                pair: pair.to_string(),
                quantity,
                price,
                side: side.to_string(),
                timestamp_ns: now,
                delay_ms: self.calculate_delay(),
                status: FragmentStatus::Pending,
                settlement_tx: None,
                error: None,
            };

            self.cloaked_count.fetch_add(1, Ordering::Relaxed);
            return OrderSplitPlan {
                original_order_id: order_id.to_string(),
                user_id: user_id.to_string(),
                total_quantity: quantity,
                total_value: value,
                fragments: vec![fragment],
                strategy,
                created_at_ns: now,
                reconciled: false,
                settled_count: 0,
                failed_count: 0,
            };
        }

        // Large order — split across brokers
        let num_brokers = active_brokers.len().min(self.max_fragments).max(self.min_fragments);
        let num_fragments = rand::thread_rng().gen_range(self.min_fragments..=num_brokers.min(self.max_fragments));
        let mut phantom_id = self.generate_phantom_id(user_id, order_id);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let fragments = match strategy {
            BrokerEvasionStrategy::SplitEven => {
                let per_broker = quantity / num_fragments as f64;
                active_brokers.iter().take(num_fragments).enumerate().map(|(i, broker)| {
                    let fid = self.generate_phantom_id(user_id, order_id);
                    self.cloaked_count.fetch_add(1, Ordering::Relaxed);
                    OrderFragment {
                        fragment_id: fid,
                        phantom_id: phantom_id.clone(),
                        broker_id: broker.id.clone(),
                        pair: pair.to_string(),
                        quantity: per_broker,
                        price,
                        side: side.to_string(),
                        timestamp_ns: now,
                        delay_ms: self.calculate_delay() + (i as u64 * 15),
                        status: FragmentStatus::Pending,
                        settlement_tx: None,
                        error: None,
                    }
                }).collect::<Vec<OrderFragment>>()
            }
            BrokerEvasionStrategy::RandomSplit => {
                let mut remaining = quantity;
                let mut rng = rand::thread_rng();
                active_brokers.iter().take(num_fragments).enumerate().map(|(i, broker)| {
                    let is_last = i == num_fragments - 1;
                    let frag_qty = if is_last {
                        remaining
                    } else {
                        let max = (remaining / (num_fragments - i) as f64) * 1.5;
                        let min = (remaining / (num_fragments - i) as f64) * 0.3;
                        rng.gen_range(min..max).min(broker.max_order_size / price)
                    };
                    remaining -= frag_qty;
                    let fid = self.generate_phantom_id(user_id, order_id);
                    self.cloaked_count.fetch_add(1, Ordering::Relaxed);
                    OrderFragment {
                        fragment_id: fid,
                        phantom_id: phantom_id.clone(),
                        broker_id: broker.id.clone(),
                        pair: pair.to_string(),
                        quantity: frag_qty,
                        price: price * (1.0 + rng.gen_range(-0.0005..0.0005)), // tiny price variation
                        side: side.to_string(),
                        timestamp_ns: now,
                        delay_ms: self.calculate_delay() + (i as u64 * rng.gen_range(5..50)),
                        status: FragmentStatus::Pending,
                        settlement_tx: None,
                        error: None,
                    }
                }).collect::<Vec<OrderFragment>>()
            }
            BrokerEvasionStrategy::TimeSpread => {
                let per_broker = quantity / num_fragments as f64;
                active_brokers.iter().take(num_fragments).enumerate().map(|(i, broker)| {
                    let fid = self.generate_phantom_id(user_id, order_id);
                    self.cloaked_count.fetch_add(1, Ordering::Relaxed);
                    // Spread over larger time window
                    let spread_delay = self.calculate_delay() + (i as u64 * rand::thread_rng().gen_range(20..200));
                    OrderFragment {
                        fragment_id: fid,
                        phantom_id: phantom_id.clone(),
                        broker_id: broker.id.clone(),
                        pair: pair.to_string(),
                        quantity: per_broker,
                        price,
                        side: side.to_string(),
                        timestamp_ns: now,
                        delay_ms: spread_delay,
                        status: FragmentStatus::Pending,
                        settlement_tx: None,
                        error: None,
                    }
                }).collect::<Vec<OrderFragment>>()
            }
            BrokerEvasionStrategy::VolumeMatch => {
                // تقليد حجم trades حقيقية عشان يندمج مع السوق
                let mut remaining = quantity;
                let mut rng = rand::thread_rng();
                let realistic_sizes = [0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 500.0];
                active_brokers.iter().take(num_fragments).enumerate().map(|(i, broker)| {
                    let is_last = i == num_fragments - 1;
                    let frag_qty = if is_last {
                        remaining
                    } else {
                        let size_idx = rng.gen_range(0..realistic_sizes.len());
                        (realistic_sizes[size_idx] as f64).min(remaining * 0.5)
                    };
                    remaining -= frag_qty;
                    let fid = self.generate_phantom_id(user_id, order_id);
                    self.cloaked_count.fetch_add(1, Ordering::Relaxed);
                    OrderFragment {
                        fragment_id: fid,
                        phantom_id: phantom_id.clone(),
                        broker_id: broker.id.clone(),
                        pair: pair.to_string(),
                        quantity: frag_qty,
                        price: price * (1.0 + rng.gen_range(-0.001..0.001)),
                        side: side.to_string(),
                        timestamp_ns: now,
                        delay_ms: self.calculate_delay() + (i as u64 * rng.gen_range(30..300)),
                        status: FragmentStatus::Pending,
                        settlement_tx: None,
                        error: None,
                    }
                }).collect::<Vec<OrderFragment>>()
            }
            BrokerEvasionStrategy::SplitWeighted => {
                let total_weight: f64 = active_brokers.iter().take(num_fragments).map(|b| b.weight).sum();
                let mut remaining = quantity;
                active_brokers.iter().take(num_fragments).enumerate().map(|(i, broker)| {
                    let is_last = i == num_fragments - 1;
                    let frag_qty = if is_last {
                        remaining
                    } else {
                        (quantity * broker.weight / total_weight).min(broker.max_order_size / price)
                    };
                    remaining -= frag_qty;
                    let fid = self.generate_phantom_id(user_id, order_id);
                    self.cloaked_count.fetch_add(1, Ordering::Relaxed);
                    OrderFragment {
                        fragment_id: fid,
                        phantom_id: phantom_id.clone(),
                        broker_id: broker.id.clone(),
                        pair: pair.to_string(),
                        quantity: frag_qty,
                        price,
                        side: side.to_string(),
                        timestamp_ns: now,
                        delay_ms: self.calculate_delay() + (i as u64 * 10),
                        status: FragmentStatus::Pending,
                        settlement_tx: None,
                        error: None,
                    }
                }).collect()
            }
        };

        self.total_fragments.fetch_add(fragments.len() as u64, Ordering::Relaxed);
        self.total_fragmented_orders.fetch_add(1, Ordering::Relaxed);

        // Track which broker got what
        for frag in &fragments {
            let mut broker_routes = self.broker_route_count.blocking_write();
            *broker_routes.entry(frag.broker_id.clone()).or_insert(0) += 1;
        }

        OrderSplitPlan {
            original_order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            total_quantity: quantity,
            total_value: value,
            fragments,
            strategy,
            created_at_ns: now,
            reconciled: false,
            settled_count: 0,
            failed_count: 0,
        }
    }

    fn calculate_delay(&self) -> u64 {
        let timing = self.timing.blocking_read();
        let mut rng = rand::thread_rng();
        let jitter = rng.gen_range(0..timing.jitter_range_ms);

        if timing.simulate_human_pattern {
            // Human-like delay pattern: bursts of activity then pauses
            let hour = chrono::Utc::now().hour();
            match hour {
                0..=5 => timing.base_delay_ms + jitter + 100,   // Night — slower
                6..=9 => timing.base_delay_ms + jitter,          // Morning ramp
                10..=15 => timing.base_delay_ms + jitter / 2,    // Active hours — faster
                16..=20 => timing.base_delay_ms + jitter,        // Evening
                _ => timing.base_delay_ms + jitter + 50,
            }
        } else {
            timing.base_delay_ms + jitter
        }
    }

    // ==================== Order Cloaking (Single, Non-split) ====================

    pub fn cloak_order(
        &self,
        user_id: &str,
        order_id: &str,
        pair: &str,
        price: f64,
        track: &crate::types::Track,
    ) -> CloakedOrder {
        let phantom_id = self.generate_phantom_id(user_id, order_id);
        let jitter_ns = self.calculate_delay() * 1_000_000;

        self.cloaked_count.fetch_add(1, Ordering::Relaxed);
        self.total_jitter_ns.fetch_add(jitter_ns, Ordering::Relaxed);

        {
            let mut map = self.phantom_map.blocking_write();
            map.insert(phantom_id.clone(), user_id.to_string());
        }

        CloakedOrder {
            original_user_id: user_id.to_string(),
            phantom_id,
            cloaked_pair: pair.to_string(),
            original_price: price,
            jitter_ns,
            track: track.clone(),
            disclosure: crate::dual_track::DisclosureLevel::Zero,
        }
    }

    pub fn resolve_phantom(&self, phantom_id: &str) -> Option<String> {
        let map = self.phantom_map.blocking_read();
        map.get(phantom_id).cloned()
    }

    // ==================== Cloak Level Selection ====================

    pub fn select_cloak_level(
        &self,
        threat_level: &crate::cloak::ThreatLevel,
        track: &crate::types::Track,
    ) -> u8 {
        match (threat_level, track) {
            (crate::cloak::ThreatLevel::Black, _) => 4,
            (crate::cloak::ThreatLevel::Red, crate::types::Track::Autonomous) => 4,
            (crate::cloak::ThreatLevel::Red, _) => 3,
            (crate::cloak::ThreatLevel::Orange, crate::types::Track::Autonomous) => 3,
            (crate::cloak::ThreatLevel::Orange, _) => 2,
            (crate::cloak::ThreatLevel::Yellow, crate::types::Track::Autonomous) => 3,
            (crate::cloak::ThreatLevel::Yellow, _) => 1,
            (crate::cloak::ThreatLevel::Green, crate::types::Track::Autonomous) => 3,
            (crate::cloak::ThreatLevel::Green, _) => 0,
        }
    }

    // ==================== Broker Management ====================

    pub async fn add_broker(&self, broker: BrokerEndpoint) {
        let mut brokers = self.brokers.write().await;
        brokers.push(broker);
    }

    pub async fn remove_broker(&self, broker_id: &str) {
        let mut brokers = self.brokers.write().await;
        brokers.retain(|b| b.id != broker_id);
    }

    pub async fn set_broker_active(&self, broker_id: &str, active: bool) -> bool {
        let mut brokers = self.brokers.write().await;
        if let Some(b) = brokers.iter_mut().find(|b| b.id == broker_id) {
            b.is_active = active;
            true
        } else {
            false
        }
    }

    pub async fn list_brokers(&self) -> Vec<BrokerEndpoint> {
        self.brokers.read().await.clone()
    }

    // ==================== Fragment Tracking & Reconciliation ====================

    pub async fn record_split_plan(&self, plan: OrderSplitPlan) {
        let mut splits = self.active_splits.write().await;
        splits.push(plan);
    }

    pub async fn mark_fragment_settled(&self, fragment_id: &str, tx_hash: &str) -> bool {
        let mut splits = self.active_splits.write().await;
        for split in splits.iter_mut() {
            for frag in split.fragments.iter_mut() {
                if frag.fragment_id == fragment_id && frag.status == FragmentStatus::Pending {
                    frag.status = FragmentStatus::Settled;
                    frag.settlement_tx = Some(tx_hash.to_string());
                    split.settled_count += 1;
                    self.fragments_settled.fetch_add(1, Ordering::Relaxed);
                    return true;
                }
            }
        }
        false
    }

    pub async fn mark_fragment_failed(&self, fragment_id: &str, error: &str) -> bool {
        let mut splits = self.active_splits.write().await;
        for split in splits.iter_mut() {
            for frag in split.fragments.iter_mut() {
                if frag.fragment_id == fragment_id && frag.status == FragmentStatus::Pending {
                    frag.status = FragmentStatus::Failed;
                    frag.error = Some(error.to_string());
                    split.failed_count += 1;
                    self.fragments_failed.fetch_add(1, Ordering::Relaxed);
                    return true;
                }
            }
        }
        false
    }

    pub async fn reconcile_order(&self, order_id: &str) -> Option<OrderSplitPlan> {
        let mut splits = self.active_splits.write().await;
        if let Some(split) = splits.iter_mut().find(|s| s.original_order_id == order_id) {
            let all_settled = split.fragments.iter().all(|f| f.status == FragmentStatus::Settled);
            let any_failed = split.fragments.iter().any(|f| f.status == FragmentStatus::Failed);
            if all_settled || any_failed {
                split.reconciled = true;
            }
            Some(split.clone())
        } else {
            None
        }
    }

    pub async fn get_active_splits(&self) -> Vec<OrderSplitPlan> {
        let splits = self.active_splits.read().await;
        splits.iter().filter(|s| !s.reconciled).cloned().collect()
    }

    pub async fn cleanup_old_splits(&self, older_than_ns: u64) {
        let mut splits = self.active_splits.write().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        splits.retain(|s| now - s.created_at_ns < older_than_ns);
    }

    // ==================== Strategy & Timing Configuration ====================

    pub async fn set_evasion_strategy(&self, strategy: BrokerEvasionStrategy) {
        let mut s = self.evasion_strategy.write().await;
        *s = strategy;
    }

    pub async fn get_evasion_strategy(&self) -> BrokerEvasionStrategy {
        self.evasion_strategy.read().await.clone()
    }

    pub async fn set_timing(&self, timing: TimingObfuscation) {
        let mut t = self.timing.write().await;
        *t = timing;
    }

    pub async fn get_timing(&self) -> TimingObfuscation {
        self.timing.read().await.clone()
    }

    // ==================== Broker Route Selection ====================

    pub fn pick_broker_for_order(&self, value: f64) -> Option<String> {
        let brokers = self.brokers.blocking_read();
        let active: Vec<&BrokerEndpoint> = brokers.iter().filter(|b| b.is_active).collect();
        if active.is_empty() {
            return None;
        }
        // Weighted random selection
        let total_weight: f64 = active.iter().map(|b| b.weight).sum();
        let mut rng = rand::thread_rng();
        let mut pick = rng.gen::<f64>() * total_weight;
        for broker in &active {
            pick -= broker.weight;
            if pick <= 0.0 && value <= broker.max_order_size {
                return Some(broker.id.clone());
            }
        }
        // Fallback to broker with highest max_order_size
        active.iter()
            .filter(|b| value <= b.max_order_size)
            .max_by(|a, b| a.max_order_size.partial_cmp(&b.max_order_size).unwrap_or(std::cmp::Ordering::Equal))
            .map(|b| b.id.clone())
    }

    // ==================== Snapshot & Stats ====================

    pub async fn snapshot(&self) -> serde_json::Value {
        let strategy = self.evasion_strategy.read().await;
        let timing = self.timing.read().await;
        let brokers = self.brokers.read().await;
        let splits = self.active_splits.read().await;
        let broker_routes = self.broker_route_count.read().await;

        serde_json::json!({
            "cloaked_orders": self.cloaked_count.load(Ordering::Relaxed),
            "total_jitter_ns": self.total_jitter_ns.load(Ordering::Relaxed),
            "total_fragments": self.total_fragments.load(Ordering::Relaxed),
            "fragments_settled": self.fragments_settled.load(Ordering::Relaxed),
            "fragments_failed": self.fragments_failed.load(Ordering::Relaxed),
            "total_fragmented_orders": self.total_fragmented_orders.load(Ordering::Relaxed),
            "active_splits": splits.len(),
            "strategy": format!("{:?}", strategy),
            "timing": {
                "base_delay_ms": timing.base_delay_ms,
                "jitter_range_ms": timing.jitter_range_ms,
                "inter_fragment_gap_ms": timing.inter_fragment_gap_ms,
                "simulate_human_pattern": timing.simulate_human_pattern,
            },
            "brokers": brokers.iter().map(|b| serde_json::json!({
                "id": b.id,
                "name": b.name,
                "weight": b.weight,
                "max_order_size": b.max_order_size,
                "is_active": b.is_active,
                "total_routed": b.total_routed,
            })).collect::<Vec<_>>(),
            "broker_routes": broker_routes.iter().map(|(k, v)| serde_json::json!({
                "broker": k,
                "routes": v,
            })).collect::<Vec<_>>(),
            "kill_switch_active": false,
            "threat_level": "Unknown",
        })
    }

    // ==================== Cryptography Functions for Encrypted MemPool ====================

    pub fn encrypt_order(&self, order: &crate::types::Order) -> crate::threshold_crypto::EncryptedOrder {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        
        // Simple encryption for demo purposes
        let ciphertext = serde_json::json!({
            "order": order,
            "timestamp": now,
            "nonce": now ^ 0xDEADBEEF,
        });
        
        crate::threshold_crypto::EncryptedOrder {
            ciphertext: serde_json::to_string(&ciphertext).unwrap_or_default().into(),
            timestamp_ns: now,
            ephemeral_pubkey: vec![],
            commitment: vec![],
            proof: crate::threshold_crypto::ZKProof {
                challenge: vec![],
                response: vec![],
                commitment: vec![],
            },
            order_id: order.id.to_string(),
        }
    }

    pub fn create_decryption_share(&self, encrypted: &crate::threshold_crypto::EncryptedOrder) -> Option<crate::threshold_crypto::DecryptionShare> {
        // For demo purposes, always create a share
        let share = crate::threshold_crypto::DecryptionShare {
            validator_id: 1,
            share: encrypted.ciphertext.clone(),
            proof: crate::threshold_crypto::ZKProof {
                challenge: vec![],
                response: vec![],
                commitment: vec![],
            },
        };
        Some(share)
    }

    pub fn combine_decryption_shares(&self, encrypted: &crate::threshold_crypto::EncryptedOrder, _shares: &[crate::threshold_crypto::DecryptionShare]) -> Option<crate::threshold_crypto::DecryptedOrder> {
        // Simple decrypt for demo - just deserialize the order from the ciphertext
        // ciphertext is Vec<u8>, need to convert to string first
        let ciphertext_str = String::from_utf8(encrypted.ciphertext.clone()).ok()?;
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&ciphertext_str) {
            if let Some(order_val) = obj.get("order") {
                let order: crate::types::Order = serde_json::from_value(order_val.clone()).unwrap_or_default();
                
                // Decrypt the order from storage
                Some(crate::threshold_crypto::DecryptedOrder {
                    order_id: encrypted.order_id.clone(),
                    user_id: order.user_id.to_string(),
                    pair: order.pair.clone().into(),
                    side: if order.side == crate::types::OrderSide::Buy { "buy".to_string() } else { "sell".to_string() },
                    price: (order.price * 10000.0) as u64,
                    quantity: (order.quantity * 10000.0) as u64,
                    track: if order.track == crate::types::Track::Autonomous { 1 } else { 0 },
                    nonce: order.timestamp as u64,
                })
            } else {
                None
            }
        } else {
            None
        }
    }
}

// ==================== Cloaked Order (used by matching pipeline) ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloakedOrder {
    pub original_user_id: String,
    pub phantom_id: String,
    pub cloaked_pair: String,
    pub original_price: f64,
    pub jitter_ns: u64,
    pub track: crate::types::Track,
    pub disclosure: crate::dual_track::DisclosureLevel,
}

// ==================== Apply Ghost Fee ====================

pub fn apply_ghost_fees(base_fee_bps: u64, track: &crate::types::Track) -> u64 {
    match track {
        crate::types::Track::Autonomous => base_fee_bps + 20,  // +0.2% premium
        crate::types::Track::Compliant => base_fee_bps,
    }
}
