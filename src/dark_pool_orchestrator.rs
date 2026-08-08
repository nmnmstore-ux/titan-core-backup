use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DarkPoolConfig {
    pub mempool_interval_ms: u64,
    pub fba_interval_ms: u64,
    pub batch_auction_window_ms: u64,
    pub max_brokers: usize,
    pub min_brokers_for_batch: usize,
    pub zk_proof_threshold: f64,
    pub oracle_endpoint: String,
    pub compliance_endpoint: String,
    pub governance_address: String,
    pub enabled_features: Vec<String>,
}

impl Default for DarkPoolConfig {
    fn default() -> Self {
        Self {
            mempool_interval_ms: 100,
            fba_interval_ms: 100,
            batch_auction_window_ms: 5000,
            max_brokers: 10,
            min_brokers_for_batch: 3,
            zk_proof_threshold: 0.95,
            oracle_endpoint: "http://oracle.swiftbridge.io".to_string(),
            compliance_endpoint: "http://compliance.swiftbridge.io".to_string(),
            governance_address: "0x1234567890123456789012345678901234567890".to_string(),
            enabled_features: vec![
                "batch_auction".to_string(),
                "ghost_protocol".to_string(),
                "oracles".to_string(),
                "governance".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrchestratorState {
    pub running: bool,
    pub started_at: u64,
    pub last_mempool_flush: u64,
    pub last_fba_run: u64,
    pub last_block_build: u64,
    pub blocks_built: u64,
    pub total_mev_revenue_wads: u64,
    pub total_orders_collected: u64,
    pub total_trades_matched: u64,
}

pub struct DarkPoolManager {
    config: DarkPoolConfig,
    state: OrchestratorState,
    running: AtomicU8,
    started_at_ns: AtomicU64,
    last_mempool_flush_ns: AtomicU64,
    last_fba_run_ns: AtomicU64,
    last_block_build_ns: AtomicU64,
    blocks_built: AtomicU64,
    mev_revenue_wads: AtomicU64,
    orders_collected: AtomicU64,
    trades_matched: AtomicU64,
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

impl DarkPoolManager {
    pub fn new() -> Self {
        Self {
            config: DarkPoolConfig::default(),
            state: OrchestratorState::default(),
            running: AtomicU8::new(0),
            started_at_ns: AtomicU64::new(0),
            last_mempool_flush_ns: AtomicU64::new(0),
            last_fba_run_ns: AtomicU64::new(0),
            last_block_build_ns: AtomicU64::new(0),
            blocks_built: AtomicU64::new(0),
            mev_revenue_wads: AtomicU64::new(0),
            orders_collected: AtomicU64::new(0),
            trades_matched: AtomicU64::new(0),
        }
    }

    pub fn new_with_config(config: DarkPoolConfig) -> Self {
        Self {
            config,
            state: OrchestratorState::default(),
            running: AtomicU8::new(0),
            started_at_ns: AtomicU64::new(0),
            last_mempool_flush_ns: AtomicU64::new(0),
            last_fba_run_ns: AtomicU64::new(0),
            last_block_build_ns: AtomicU64::new(0),
            blocks_built: AtomicU64::new(0),
            mev_revenue_wads: AtomicU64::new(0),
            orders_collected: AtomicU64::new(0),
            trades_matched: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed) == 1
    }

    pub fn start(&self) -> Result<(), String> {
        if self
            .running
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let ts = now_ns();
            self.started_at_ns.store(ts, Ordering::Relaxed);
            self.last_mempool_flush_ns.store(ts, Ordering::Relaxed);
            self.last_fba_run_ns.store(ts, Ordering::Relaxed);
            Ok(())
        } else {
            Err("orchestrator already running".into())
        }
    }

    pub fn stop(&self) -> Result<(), String> {
        self.running
            .compare_exchange(1, 0, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| ())
            .map_err(|_| "orchestrator not running".into())
    }

    pub fn config(&self) -> &DarkPoolConfig {
        &self.config
    }

    pub fn set_config(&mut self, cfg: DarkPoolConfig) {
        self.config = cfg;
    }

    pub fn should_flush_mempool(&self, interval_ms: u64) -> bool {
        let last = self.last_mempool_flush_ns.load(Ordering::Relaxed);
        let elapsed_ms = (now_ns() - last) / 1_000_000;
        elapsed_ms >= interval_ms
    }

    pub fn should_run_fba(&self, interval_ms: u64) -> bool {
        let last = self.last_fba_run_ns.load(Ordering::Relaxed);
        let elapsed_ms = (now_ns() - last) / 1_000_000;
        elapsed_ms >= interval_ms
    }

    pub fn should_build_block(&self, window_ms: u64) -> bool {
        let last = self.last_block_build_ns.load(Ordering::Relaxed);
        let elapsed_ms = (now_ns() - last) / 1_000_000;
        elapsed_ms >= window_ms
    }

    pub fn flush_mempool(&self) -> u64 {
        let ts = now_ns();
        self.last_mempool_flush_ns.store(ts, Ordering::Relaxed);
        ts
    }

    pub fn run_fba(&self, matched: u64, revenue_wads: u64) -> u64 {
        let ts = now_ns();
        self.last_fba_run_ns.store(ts, Ordering::Relaxed);
        self.trades_matched
            .fetch_add(matched, Ordering::Relaxed);
        self.mev_revenue_wads
            .fetch_add(revenue_wads, Ordering::Relaxed);
        ts
    }

    pub fn build_mev_block(
        &self,
        collected_orders: u64,
        revenue_wads: u64,
    ) -> BlockBuildResult {
        if !self.is_running() {
            return BlockBuildResult {
                success: false,
                error: Some("orchestrator not running".into()),
                ..Default::default()
            };
        }
        let ts = now_ns();
        self.last_block_build_ns.store(ts, Ordering::Relaxed);
        self.blocks_built.fetch_add(1, Ordering::Relaxed);
        self.orders_collected
            .fetch_add(collected_orders, Ordering::Relaxed);
        self.mev_revenue_wads
            .fetch_add(revenue_wads, Ordering::Relaxed);
        BlockBuildResult {
            success: true,
            error: None,
            block_number: self.blocks_built.load(Ordering::Relaxed),
            collected_orders,
            mev_revenue_wads: revenue_wads,
            timestamp_ns: ts,
        }
    }

    pub fn seal_and_publish(&self, block_result: BlockBuildResult) -> PublishResult {
        if !block_result.success {
            return PublishResult {
                published: false,
                error: block_result.error,
                block_number: 0,
            };
        }
        let bn = block_result.block_number;
        PublishResult {
            published: true,
            error: None,
            block_number: bn,
        }
    }

    pub fn mev_revenue_wads(&self) -> u64 {
        self.mev_revenue_wads.load(Ordering::Relaxed)
    }

    pub fn state_snapshot(&self) -> OrchestratorState {
        let ts = now_ns();
        let started = self.started_at_ns.load(Ordering::Relaxed);
        OrchestratorState {
            running: self.is_running(),
            started_at: started,
            last_mempool_flush: self.last_mempool_flush_ns.load(Ordering::Relaxed),
            last_fba_run: self.last_fba_run_ns.load(Ordering::Relaxed),
            last_block_build: self.last_block_build_ns.load(Ordering::Relaxed),
            blocks_built: self.blocks_built.load(Ordering::Relaxed),
            total_mev_revenue_wads: self.mev_revenue_wads.load(Ordering::Relaxed),
            total_orders_collected: self.orders_collected.load(Ordering::Relaxed),
            total_trades_matched: self.trades_matched.load(Ordering::Relaxed),
        }
    }
}

impl Default for DarkPoolManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct BlockBuildResult {
    pub success: bool,
    pub error: Option<String>,
    pub block_number: u64,
    pub collected_orders: u64,
    pub mev_revenue_wads: u64,
    pub timestamp_ns: u64,
}

#[derive(Debug, Clone)]
pub struct PublishResult {
    pub published: bool,
    pub error: Option<String>,
    pub block_number: u64,
}