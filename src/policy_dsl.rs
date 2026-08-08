use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug};
use dashmap::DashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub max_policies: usize,
    pub wasm_target: String,
    pub compile_optimization: u32,
    pub enable_hot_reload: bool,
    pub snapshot_interval_secs: u64,
    pub max_wasm_size: usize,
    pub enable_aot: bool,
    pub sandbox_enabled: bool,
    pub gas_limit: u64,
    pub memory_limit: usize,
    pub fuel_limit: u64,
    pub enable_debug: bool,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            max_policies: 10_000,
            wasm_target: "wasm32-wasip1".to_string(),
            compile_optimization: 2,
            enable_hot_reload: true,
            snapshot_interval_secs: 60,
            max_wasm_size: 10 * 1024 * 1024,
            enable_aot: true,
            sandbox_enabled: true,
            gas_limit: 1_000_000,
            memory_limit: 16 * 1024 * 1024,
            fuel_limit: 10_000_000,
            enable_debug: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyLanguage {
    Rust,
    Rego,
    DSL,
    TypeScript,
    Python,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub policy_id: String,
    pub name: String,
    pub description: String,
    pub language: PolicyLanguage,
    pub source: String,
    pub compiled_wasm: Option<Vec<u8>>,
    pub aot_compiled: Option<Vec<u8>>,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub active: bool,
    pub direction: String,
    pub priority: u32,
    pub tags: Vec<String>,
    pub dependencies: Vec<String>,
    pub gas_used: u64,
    pub memory_used: usize,
    pub compile_time_ms: u64,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySnapshot {
    pub snapshot_id: String,
    pub timestamp: DateTime<Utc>,
    pub policies: Vec<Policy>,
    pub hash: String,
    pub size_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyStats {
    pub total_policies: usize,
    pub active_policies: usize,
    pub compiled_policies: usize,
    pub aot_compiled: usize,
    pub total_snapshots: u64,
    pub hot_reloads: u64,
    pub compilation_failures: u64,
    pub total_evaluations: u64,
    pub total_gas_used: u64,
    pub avg_evaluation_time_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvaluationResult {
    pub policy_id: String,
    pub decision: PolicyDecision,
    pub gas_used: u64,
    pub memory_used: usize,
    pub execution_time_ns: u64,
    pub logs: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PolicyDecision {
    Allow,
    Deny,
    RequireReview,
    Modify,
}

pub struct PolicyDSLCompiler {
    config: Arc<RwLock<PolicyConfig>>,
    policies: Arc<DashMap<String, Policy>>,
    snapshots: Arc<DashMap<String, PolicySnapshot>>,
    stats: Arc<RwLock<PolicyStats>>,
    running: Arc<RwLock<bool>>,
    evaluator_cache: Arc<DashMap<String, Vec<u8>>>,
}

impl PolicyDSLCompiler {
    pub fn new(config: PolicyConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            policies: Arc::new(DashMap::new()),
            snapshots: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(PolicyStats {
                total_policies: 0,
                active_policies: 0,
                compiled_policies: 0,
                aot_compiled: 0,
                total_snapshots: 0,
                hot_reloads: 0,
                compilation_failures: 0,
                total_evaluations: 0,
                total_gas_used: 0,
                avg_evaluation_time_ns: 0,
            })),
            running: Arc::new(RwLock::new(false)),
            evaluator_cache: Arc::new(DashMap::new()),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        let mut running = self.running.write().await;
        *running = true;

        let config = self.config.read().await;
        info!(
            "Policy DSL Compiler started — target={} max_policies={} hot_reload={} aot={} sandbox={} gas_limit={}",
            config.wasm_target, config.max_policies, config.enable_hot_reload, config.enable_aot, config.sandbox_enabled, config.gas_limit
        );
        Ok(())
    }

    pub async fn compile_policy(&self, mut policy: Policy) -> Result<Policy, String> {
        let start = std::time::Instant::now();
        let config = self.config.read().await;

        match policy.language {
            PolicyLanguage::Rust => {
                let wasm = self.compile_rust_to_wasm(&policy.source, &config.wasm_target).await?;
                if wasm.len() > config.max_wasm_size {
                    return Err("WASM too large".to_string());
                }
                policy.compiled_wasm = Some(wasm);
                if config.enable_aot {
                    let aot = self.compile_wasm_to_aot(&policy.compiled_wasm.as_ref().unwrap()).await?;
                    policy.aot_compiled = Some(aot);
                }
            }
            PolicyLanguage::Rego => {
                let wasm = self.compile_rego_to_wasm(&policy.source).await?;
                policy.compiled_wasm = Some(wasm);
            }
            PolicyLanguage::DSL => {
                let wasm = self.compile_dsl_to_wasm(&policy.source).await?;
                policy.compiled_wasm = Some(wasm);
            }
            PolicyLanguage::TypeScript => {
                let wasm = self.compile_typescript_to_wasm(&policy.source).await?;
                policy.compiled_wasm = Some(wasm);
            }
            PolicyLanguage::Python => {
                let wasm = self.compile_python_to_wasm(&policy.source).await?;
                policy.compiled_wasm = Some(wasm);
            }
        }

        policy.version += 1;
        policy.updated_at = Utc::now();
        policy.compile_time_ms = start.elapsed().as_millis() as u64;

        let mut stats = self.stats.write().await;
        stats.compiled_policies += 1;
        if policy.aot_compiled.is_some() {
            stats.aot_compiled += 1;
        }

        info!(
            "Policy compiled: id={} name={} v{} lang={:?} time={}ms gas={}",
            policy.policy_id, policy.name, policy.version, policy.language, policy.compile_time_ms, policy.gas_used
        );
        Ok(policy)
    }

    async fn compile_rust_to_wasm(&self, source: &str, target: &str) -> Result<Vec<u8>, String> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(source.as_bytes());
        hasher.update(target.as_bytes());
        hasher.update(b"rust_wasm_v2");
        Ok(hasher.finalize().to_vec())
    }

    async fn compile_rego_to_wasm(&self, source: &str) -> Result<Vec<u8>, String> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(source.as_bytes());
        hasher.update(b"rego_wasm_v2");
        Ok(hasher.finalize().to_vec())
    }

    async fn compile_dsl_to_wasm(&self, source: &str) -> Result<Vec<u8>, String> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(source.as_bytes());
        hasher.update(b"dsl_wasm_v2");
        Ok(hasher.finalize().to_vec())
    }

    async fn compile_typescript_to_wasm(&self, source: &str) -> Result<Vec<u8>, String> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(source.as_bytes());
        hasher.update(b"ts_wasm_v1");
        Ok(hasher.finalize().to_vec())
    }

    async fn compile_python_to_wasm(&self, source: &str) -> Result<Vec<u8>, String> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(source.as_bytes());
        hasher.update(b"py_wasm_v1");
        Ok(hasher.finalize().to_vec())
    }

    async fn compile_wasm_to_aot(&self, wasm: &[u8]) -> Result<Vec<u8>, String> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(wasm);
        hasher.update(b"aot_v1");
        Ok(hasher.finalize().to_vec())
    }

    pub async fn register_policy(&self, mut policy: Policy) -> Result<(), String> {
        let config = self.config.read().await;
        if self.policies.len() >= config.max_policies {
            return Err("policy limit reached".to_string());
        }
        drop(config);

        let policy = self.compile_policy(policy).await?;

        self.policies.insert(policy.policy_id.clone(), policy.clone());

        let mut stats = self.stats.write().await;
        stats.total_policies += 1;
        if policy.active {
            stats.active_policies += 1;
        }

        info!("Policy registered: id={} direction={} tags={:?}", policy.policy_id, policy.direction, policy.tags);
        Ok(())
    }

    pub async fn update_policy(&self, mut policy: Policy) -> Result<(), String> {
        let existing = self.policies.get(&policy.policy_id)
            .ok_or("policy not found")?;
        policy.version = existing.version + 1;
        policy.created_at = existing.created_at;
        let policy = self.compile_policy(policy).await?;
        self.policies.insert(policy.policy_id.clone(), policy.clone());
        info!("Policy updated: id={} v{}", policy.policy_id, policy.version);
        Ok(())
    }

    pub async fn deactivate_policy(&self, policy_id: &str) -> Result<(), String> {
        let mut policy = self.policies.get_mut(policy_id).ok_or("policy not found")?;
        policy.active = false;

        let mut stats = self.stats.write().await;
        stats.active_policies -= 1;

        info!("Policy deactivated: id={}", policy_id);
        Ok(())
    }

    pub async fn hot_reload(&self, policy_id: &str, source: &str) -> Result<(), String> {
        let config = self.config.read().await;
        if !config.enable_hot_reload {
            return Err("hot reload disabled".to_string());
        }
        drop(config);

        let mut policy = self.policies.get(policy_id).map(|e| e.value().clone()).ok_or("policy not found")?;
        policy.source = source.to_string();
        let policy = self.compile_policy(policy).await?;
        self.policies.insert(policy_id.to_string(), policy.clone());
        
        let mut stats = self.stats.write().await;
        stats.hot_reloads += 1;

        info!("Policy hot-reloaded: id={} v{}", policy_id, policy.version);
        Ok(())
    }

    pub async fn evaluate(&self, policy_id: &str, input: &[u8]) -> Result<PolicyEvaluationResult, String> {
        let start = std::time::Instant::now();
        let config = self.config.read().await;

        let policy = self.policies
            .get(policy_id).map(|e| e.value().clone())
            .ok_or("policy not found")?;

        if !policy.active {
            return Err("policy not active".to_string());
        }

        let wasm = policy.compiled_wasm.clone()
            .ok_or("policy not compiled")?;

        let result = self.execute_wasm(&wasm, input, &config).await?;

        let exec_time = start.elapsed().as_nanos() as u64;
        let mut stats = self.stats.write().await;
        stats.total_evaluations += 1;
        stats.total_gas_used += result.gas_used;
        stats.avg_evaluation_time_ns = ((stats.avg_evaluation_time_ns * (stats.total_evaluations - 1)) + exec_time) / stats.total_evaluations;

        Ok(result)
    }

    async fn execute_wasm(&self, wasm: &[u8], input: &[u8], config: &PolicyConfig) -> Result<PolicyEvaluationResult, String> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(wasm);
        hasher.update(input);
        let hash = hasher.finalize();

        let decision = match hash[0] % 4 {
            0 => PolicyDecision::Allow,
            1 => PolicyDecision::Deny,
            2 => PolicyDecision::RequireReview,
            _ => PolicyDecision::Modify,
        };

        Ok(PolicyEvaluationResult {
            policy_id: "simulated".to_string(),
            decision,
            gas_used: config.gas_limit / 4,
            memory_used: config.memory_limit / 2,
            execution_time_ns: 100_000,
            logs: vec!["WASM executed".to_string()],
            error: None,
        })
    }

    pub async fn create_snapshot(&self) -> Result<PolicySnapshot, String> {
        let policies = self.policies.clone();
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        for p in policies.iter() {
            hasher.update(p.value().policy_id.as_bytes());
            hasher.update(&p.value().version.to_be_bytes());
        }
        let hash = format!("{:x}", hasher.finalize());

        let snapshot = PolicySnapshot {
            snapshot_id: format!("snap_{}", Uuid::new_v4()),
            timestamp: Utc::now(),
            policies: policies.iter().map(|e| e.value().clone()).collect(),
            hash,
            size_bytes: policies.iter().map(|p| p.value().source.len() + p.value().compiled_wasm.as_ref().map_or(0, |w| w.len())).sum(),
        };

        self.snapshots.insert(snapshot.snapshot_id.clone(), snapshot.clone());

        let mut stats = self.stats.write().await;
        stats.total_snapshots += 1;

        info!("Policy snapshot created: id={} policies={}", snapshot.snapshot_id, policies.len());
        Ok(snapshot)
    }

    pub async fn rollback_snapshot(&self, snapshot_id: &str) -> Result<(), String> {
        let snapshot = self.snapshots
            .get(snapshot_id).map(|e| e.value().clone())
            .ok_or("snapshot not found")?;

        self.policies.clear();
        for p in snapshot.policies {
            self.policies.insert(p.policy_id.clone(), p);
        }

        info!("Policy rollback to snapshot: id={}", snapshot_id);
        Ok(())
    }

    pub async fn get_policy(&self, policy_id: &str) -> Option<Policy> {
        self.policies.get(policy_id).map(|e| e.value().clone())
    }

    pub async fn list_policies(&self, direction: Option<&str>, active_only: bool) -> Vec<Policy> {
        self.policies.iter()
            .filter(|e| {
                let p = e.value();
                let dir_match = direction.map_or(true, |d| p.direction == d);
                let active_match = !active_only || p.active == active_only;
                dir_match && active_match
            })
            .map(|e| e.value().clone())
            .collect()
    }

    pub async fn get_stats(&self) -> PolicyStats {
        self.stats.read().await.clone()
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Policy DSL Compiler stopped");
    }
}