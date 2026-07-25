use crate::cloud::tenant::TenantManager;
use dashmap::DashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ScalingConfig {
    pub max_engines_per_host: u32,
    pub scale_up_threshold: u64,
    pub scale_down_threshold: u64,
    pub cooldown_secs: u64,
    pub min_engines: u32,
    pub max_engines: u32,
}

impl Default for ScalingConfig {
    fn default() -> Self {
        Self {
            max_engines_per_host: 8,
            scale_up_threshold: 50_000,
            scale_down_threshold: 5_000,
            cooldown_secs: 120,
            min_engines: 2,
            max_engines: 100,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HostNode {
    pub id: String,
    pub address: String,
    pub engine_count: u64,
    pub available: bool,
    pub dedicated_cores: Vec<u32>,
    pub total_cores: u32,
}

impl HostNode {
    pub fn new(id: String, address: String) -> Self {
        Self {
            id,
            address,
            engine_count: 0,
            available: true,
            dedicated_cores: Vec::new(),
            total_cores: num_cpus::get() as u32,
        }
    }
}

pub struct CloudOrchestrator {
    pub tenants: TenantManager,
    pub hosts: DashMap<String, HostNode>,
    pub scaling_config: Mutex<ScalingConfig>,
    pub total_engines: AtomicU64,
    pub active_tenants: AtomicU64,
    pub running: Arc<AtomicBool>,
    next_port: AtomicU64,
}

impl CloudOrchestrator {
    pub fn new(config: ScalingConfig) -> Self {
        let orch = Self {
            tenants: TenantManager::new(),
            hosts: DashMap::new(),
            scaling_config: Mutex::new(config),
            total_engines: AtomicU64::new(0),
            active_tenants: AtomicU64::new(0),
            running: Arc::new(AtomicBool::new(true)),
            next_port: AtomicU64::new(5000),
        };
        orch.register_host("host-1".into(), "127.0.0.1".into());
        orch
    }

    pub fn register_host(&self, id: String, address: String) {
        let node = HostNode::new(id.clone(), address);
        self.hosts.insert(id, node);
    }

    pub fn provision_engine(&self, tenant_id: &Uuid) -> Result<String, String> {
        let tier = self.tenants.get_tenant(tenant_id)
            .map(|t| t.tier.clone())
            .unwrap_or(crate::cloud::tenant::Tier::Free);

        let core_count = if tier == crate::cloud::tenant::Tier::Enterprise { 2 } else { 0 };

        let best_host = self.find_best_host(core_count)?;
        let port = self.next_port.fetch_add(1, Ordering::Relaxed) as u16 + 1;

        if let Some(mut host) = self.hosts.get_mut(&best_host) {
            host.engine_count += 1;
        }

        let endpoint = format!("{}:{}", best_host, port);
        self.tenants.assign_engine(tenant_id, best_host.clone(), port)?;
        self.total_engines.fetch_add(1, Ordering::Relaxed);
        self.active_tenants.fetch_add(1, Ordering::Relaxed);
        tracing::info!(tenant = %tenant_id, endpoint = %endpoint, cores = core_count, "Engine provisioned");
        Ok(endpoint)
    }

    pub fn drain_engine(&self, tenant_id: &Uuid) -> Result<(), String> {
        let host = {
            let tenant = self.tenants.get_tenant(tenant_id).ok_or("tenant not found")?;
            tenant.engine.as_ref().map(|e| e.host.clone())
        };
        if let Some(host_id) = host {
            if let Some(mut h) = self.hosts.get_mut(&host_id) {
                if h.engine_count > 0 {
                    h.engine_count -= 1;
                }
            }
        }
        if let Some(mut tenant) = self.tenants.get_tenant_mut(tenant_id) {
            tenant.engine = None;
        }
        self.total_engines.fetch_sub(1, Ordering::Relaxed);
        self.active_tenants.fetch_sub(1, Ordering::Relaxed);
        tracing::info!(tenant = %tenant_id, "Engine drained");
        Ok(())
    }

    fn max_engines_per_host(&self) -> u32 {
        self.scaling_config.lock().map_or(8, |cfg| cfg.max_engines_per_host)
    }

    fn find_best_host(&self, _needed_cores: u32) -> Result<String, String> {
        let max = self.max_engines_per_host();
        let mut best: Option<(String, u64)> = None;
        for entry in self.hosts.iter() {
            let count = entry.engine_count;
            if count < max as u64 {
                if best.as_ref().map_or(true, |(_, c)| count < *c) {
                    best = Some((entry.key().clone(), count));
                }
            }
        }
        best.map(|(h, _)| h).ok_or_else(|| "no hosts available".into())
    }

    pub fn calculate_scaling_decision(&self) -> ScalingDecision {
        let total_orders: u64 = self.tenants.tenants.iter()
            .map(|t| t.usage.total_orders)
            .sum();
        let current = self.total_engines.load(Ordering::Relaxed);
        let cfg = self.scaling_config.lock().unwrap_or_else(|e| e.into_inner());

        let (su, sd, min) = (cfg.scale_up_threshold, cfg.scale_down_threshold, cfg.min_engines);
        drop(cfg);

        if total_orders > su * current.max(1) {
            ScalingDecision::ScaleUp(1)
        } else if total_orders < sd * current.max(1)
            && current > min as u64
        {
            ScalingDecision::ScaleDown(1)
        } else {
            ScalingDecision::Stable
        }
    }

    pub fn cloud_status(&self) -> CloudStatus {
        CloudStatus {
            total_tenants: self.tenants.tenant_count() as u64,
            active_tenants: self.active_tenants.load(Ordering::Relaxed),
            total_engines: self.total_engines.load(Ordering::Relaxed),
            available_hosts: self.hosts.len() as u64,
        }
    }

    pub fn start_monitoring_loop(self: &Arc<Self>) {
        let orch = self.clone();
        let running = self.running.clone();
        std::thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                let decision = orch.calculate_scaling_decision();
                match decision {
                    ScalingDecision::ScaleUp(n) => {
                        tracing::info!(count = n, "Scaling UP");
                    }
                    ScalingDecision::ScaleDown(n) => {
                        tracing::info!(count = n, "Scaling DOWN");
                    }
                    ScalingDecision::Stable => {}
                }
                let cooldown = orch.scaling_config.lock().map_or(120, |cfg| cfg.cooldown_secs);
                std::thread::sleep(Duration::from_secs(cooldown));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_monitoring_loop_does_not_require_tokio_runtime() {
        let orchestrator = Arc::new(CloudOrchestrator::new(ScalingConfig::default()));
        orchestrator.running.store(false, Ordering::Relaxed);
        orchestrator.start_monitoring_loop();
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum ScalingDecision {
    ScaleUp(u32),
    ScaleDown(u32),
    Stable,
}

impl serde::Serialize for HostNode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("HostNode", 6)?;
        s.serialize_field("id", &self.id)?;
        s.serialize_field("address", &self.address)?;
        s.serialize_field("engine_count", &self.engine_count)?;
        s.serialize_field("available", &self.available)?;
        s.serialize_field("dedicated_cores", &self.dedicated_cores)?;
        s.serialize_field("total_cores", &self.total_cores)?;
        s.end()
    }
}

impl serde::Serialize for ScalingConfig {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("ScalingConfig", 6)?;
        s.serialize_field("max_engines_per_host", &self.max_engines_per_host)?;
        s.serialize_field("scale_up_threshold", &self.scale_up_threshold)?;
        s.serialize_field("scale_down_threshold", &self.scale_down_threshold)?;
        s.serialize_field("cooldown_secs", &self.cooldown_secs)?;
        s.serialize_field("min_engines", &self.min_engines)?;
        s.serialize_field("max_engines", &self.max_engines)?;
        s.end()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CloudStatus {
    pub total_tenants: u64,
    pub active_tenants: u64,
    pub total_engines: u64,
    pub available_hosts: u64,
}
