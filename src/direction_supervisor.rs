use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorConfig {
    pub max_restart_attempts: u32,
    pub restart_cooldown_secs: u64,
    pub health_check_interval_secs: u64,
    pub crash_threshold: u32,
    pub enable_auto_restart: bool,
    pub enable_isolation: bool,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            max_restart_attempts: 5,
            restart_cooldown_secs: 60,
            health_check_interval_secs: 10,
            crash_threshold: 3,
            enable_auto_restart: true,
            enable_isolation: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectionProcess {
    pub direction_id: String,
    pub name: String,
    pub status: ProcessStatus,
    pub pid: Option<u32>,
    pub restart_count: u32,
    pub last_restart: Option<i64>,
    pub last_health_check: Option<i64>,
    pub last_crash: Option<i64>,
    pub uptime_secs: u64,
    pub memory_usage_mb: f64,
    pub cpu_usage_pct: f64,
    pub error_log: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProcessStatus {
    Running,
    Stopped,
    Crashed,
    Restarting,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorStats {
    pub total_directions: usize,
    pub running: usize,
    pub stopped: usize,
    pub crashed: usize,
    pub total_restarts: u64,
    pub total_crashes: u64,
    pub avg_uptime_secs: f64,
    pub isolation_events: u64,
}

pub struct DirectionSupervisor {
    config: Arc<RwLock<SupervisorConfig>>,
    processes: Arc<RwLock<HashMap<String, DirectionProcess>>>,
    stats: Arc<RwLock<SupervisorStats>>,
    running: Arc<RwLock<bool>>,
}

impl DirectionSupervisor {
    pub fn new(config: SupervisorConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            processes: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(SupervisorStats {
                total_directions: 0,
                running: 0,
                stopped: 0,
                crashed: 0,
                total_restarts: 0,
                total_crashes: 0,
                avg_uptime_secs: 0.0,
                isolation_events: 0,
            })),
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        {
            let mut running = self.running.write().await;
            *running = true;
        }
        let config = self.config.read().await;
        info!(
            "Direction Supervisor started — max_restarts={} cooldown={}s isolation={}",
            config.max_restart_attempts, config.restart_cooldown_secs, config.enable_isolation
        );
        Ok(())
    }

    pub async fn register_direction(&self, id: &str, name: &str) -> Result<(), String> {
        let process = DirectionProcess {
            direction_id: id.to_string(),
            name: name.to_string(),
            status: ProcessStatus::Running,
            pid: None,
            restart_count: 0,
            last_restart: None,
            last_health_check: Some(chrono::Utc::now().timestamp_millis()),
            last_crash: None,
            uptime_secs: 0,
            memory_usage_mb: 0.0,
            cpu_usage_pct: 0.0,
            error_log: Vec::new(),
        };

        {
            let mut processes = self.processes.write().await;
            processes.insert(id.to_string(), process);
        }

        let mut stats = self.stats.write().await;
        stats.total_directions += 1;
        stats.running += 1;

        info!("Direction registered: id={} name={}", id, name);
        Ok(())
    }

    pub async fn report_crash(&self, direction_id: &str, error: &str) -> Result<(), String> {
        let config = self.config.read().await;
        let mut processes = self.processes.write().await;
        let process = processes.get_mut(direction_id).ok_or("direction not found")?;

        process.status = ProcessStatus::Crashed;
        process.last_crash = Some(chrono::Utc::now().timestamp_millis());
        process.restart_count += 1;
        process.error_log.push(error.to_string());
        if process.error_log.len() > 100 {
            process.error_log.remove(0);
        }

        let mut stats = self.stats.write().await;
        stats.total_crashes += 1;
        stats.running -= 1;
        stats.crashed += 1;

        error!(
            "Direction CRASHED: id={} error={} restarts={}/{}",
            direction_id, error, process.restart_count, config.max_restart_attempts
        );

        if config.enable_auto_restart && process.restart_count <= config.max_restart_attempts {
            process.status = ProcessStatus::Restarting;
            stats.crashed -= 1;

            info!(
                "Direction auto-restarting: id={} attempt={}/{}",
                direction_id, process.restart_count, config.max_restart_attempts
            );

            let processes_clone = self.processes.clone();
            let id = direction_id.to_string();
            let cooldown = config.restart_cooldown_secs;
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(cooldown)).await;
                let mut processes = processes_clone.write().await;
                if let Some(p) = processes.get_mut(&id) {
                    p.status = ProcessStatus::Running;
                    p.last_restart = Some(chrono::Utc::now().timestamp_millis());
                }
            });

            let mut stats = self.stats.write().await;
            stats.total_restarts += 1;
        } else if process.restart_count > config.max_restart_attempts {
            process.status = ProcessStatus::Stopped;
            stats.crashed -= 1;
            stats.stopped += 1;

            if config.enable_isolation {
                stats.isolation_events += 1;
                warn!(
                    "Direction ISOLATED: id={} — exceeded max restarts",
                    direction_id
                );
            }
        }

        Ok(())
    }

    pub async fn health_check(&self, direction_id: &str, healthy: bool, memory_mb: f64, cpu_pct: f64) -> Result<(), String> {
        let mut processes = self.processes.write().await;
        let process = processes.get_mut(direction_id).ok_or("direction not found")?;

        process.last_health_check = Some(chrono::Utc::now().timestamp_millis());
        process.memory_usage_mb = memory_mb;
        process.cpu_usage_pct = cpu_pct;

        if healthy && process.status == ProcessStatus::Degraded {
            process.status = ProcessStatus::Running;
            let mut stats = self.stats.write().await;
            stats.running += 1;
            stats.stopped -= 1;
        } else if !healthy && process.status == ProcessStatus::Running {
            process.status = ProcessStatus::Degraded;
            let mut stats = self.stats.write().await;
            stats.running -= 1;
            stats.stopped += 1;
        }

        Ok(())
    }

    pub async fn stop_direction(&self, direction_id: &str) -> Result<(), String> {
        let mut processes = self.processes.write().await;
        let process = processes.get_mut(direction_id).ok_or("direction not found")?;
        process.status = ProcessStatus::Stopped;

        let mut stats = self.stats.write().await;
        stats.running -= 1;
        stats.stopped += 1;

        info!("Direction stopped: id={}", direction_id);
        Ok(())
    }

    pub async fn get_process(&self, direction_id: &str) -> Option<DirectionProcess> {
        let processes = self.processes.read().await;
        processes.get(direction_id).cloned()
    }

    pub async fn list_processes(&self) -> Vec<DirectionProcess> {
        let processes = self.processes.read().await;
        processes.values().cloned().collect()
    }

    pub async fn get_stats(&self) -> SupervisorStats {
        self.stats.read().await.clone()
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Direction Supervisor stopped");
    }
}
