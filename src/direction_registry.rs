use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    pub max_directions: usize,
    pub enable_hot_load: bool,
    pub enable_lifecycle: bool,
    pub health_check_interval_secs: u64,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            max_directions: 32,
            enable_hot_load: true,
            enable_lifecycle: true,
            health_check_interval_secs: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssetClass {
    Equities,
    Crypto,
    Bonds,
    FX,
    Derivatives,
    Commodities,
    RealEstate,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Direction {
    pub direction_id: String,
    pub name: String,
    pub asset_class: AssetClass,
    pub status: DirectionStatus,
    pub version: String,
    pub config: HashMap<String, serde_json::Value>,
    pub wasm_module: Option<Vec<u8>>,
    pub created_at: i64,
    pub updated_at: i64,
    pub load_count: u64,
    pub last_loaded: Option<i64>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DirectionStatus {
    Registered,
    Loading,
    Active,
    Paused,
    Unloading,
    Unloaded,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectionSnapshot {
    pub snapshot_id: String,
    pub timestamp: i64,
    pub directions: Vec<Direction>,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    pub total_directions: usize,
    pub active: usize,
    pub paused: usize,
    pub error: usize,
    pub total_loads: u64,
    pub total_unloads: u64,
    pub hot_reloads: u64,
    pub total_snapshots: u64,
}

pub struct DirectionRegistry {
    config: Arc<RwLock<RegistryConfig>>,
    directions: Arc<RwLock<HashMap<String, Direction>>>,
    snapshots: Arc<RwLock<Vec<DirectionSnapshot>>>,
    stats: Arc<RwLock<RegistryStats>>,
    running: Arc<RwLock<bool>>,
}

impl DirectionRegistry {
    pub fn new(config: RegistryConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            directions: Arc::new(RwLock::new(HashMap::new())),
            snapshots: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(RegistryStats {
                total_directions: 0,
                active: 0,
                paused: 0,
                error: 0,
                total_loads: 0,
                total_unloads: 0,
                hot_reloads: 0,
                total_snapshots: 0,
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
            "Direction Registry started — max={} hot_load={} lifecycle={}",
            config.max_directions, config.enable_hot_load, config.enable_lifecycle
        );
        Ok(())
    }

    pub async fn register(&self, direction: Direction) -> Result<(), String> {
        let config = self.config.read().await;
        {
            let directions = self.directions.read().await;
            if directions.len() >= config.max_directions {
                return Err("direction limit reached".to_string());
            }
        }

        {
            let mut directions = self.directions.write().await;
            directions.insert(direction.direction_id.clone(), direction.clone());
        }

        let mut stats = self.stats.write().await;
        stats.total_directions += 1;

        info!(
            "Direction registered: id={} name={} asset_class={:?}",
            direction.direction_id, direction.name, direction.asset_class
        );
        Ok(())
    }

    pub async fn load_direction(&self, direction_id: &str) -> Result<(), String> {
        let config = self.config.read().await;
        let mut directions = self.directions.write().await;
        let direction = directions.get_mut(direction_id).ok_or("direction not found")?;

        if direction.status == DirectionStatus::Active {
            return Err("direction already active".to_string());
        }

        direction.status = DirectionStatus::Loading;
        drop(directions);

        if config.enable_hot_load {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let mut directions = self.directions.write().await;
        let direction = directions.get_mut(direction_id).unwrap();
        direction.status = DirectionStatus::Active;
        direction.load_count += 1;
        direction.last_loaded = Some(chrono::Utc::now().timestamp_millis());

        let mut stats = self.stats.write().await;
        stats.active += 1;
        stats.total_loads += 1;

        info!("Direction loaded: id={}", direction_id);
        Ok(())
    }

    pub async fn unload_direction(&self, direction_id: &str) -> Result<(), String> {
        let mut directions = self.directions.write().await;
        let direction = directions.get_mut(direction_id).ok_or("direction not found")?;

        direction.status = DirectionStatus::Unloading;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        direction.status = DirectionStatus::Unloaded;

        let mut stats = self.stats.write().await;
        stats.active -= 1;
        stats.total_unloads += 1;

        info!("Direction unloaded: id={}", direction_id);
        Ok(())
    }

    pub async fn hot_reload(&self, direction_id: &str, wasm_module: Vec<u8>) -> Result<(), String> {
        let config = self.config.read().await;
        if !config.enable_hot_load {
            return Err("hot reload disabled".to_string());
        }

        let mut directions = self.directions.write().await;
        let direction = directions.get_mut(direction_id).ok_or("direction not found")?;

        direction.wasm_module = Some(wasm_module);
        direction.version = format!("{}.{}", direction.version, "hot");
        direction.updated_at = chrono::Utc::now().timestamp_millis();

        let mut stats = self.stats.write().await;
        stats.hot_reloads += 1;

        info!("Direction hot-reloaded: id={}", direction_id);
        Ok(())
    }

    pub async fn create_snapshot(&self) -> Result<DirectionSnapshot, String> {
        let directions = self.directions.read().await;
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        for d in directions.values() {
            hasher.update(d.direction_id.as_bytes());
            hasher.update(d.version.as_bytes());
        }
        let hash = format!("{:x}", hasher.finalize());

        let snapshot = DirectionSnapshot {
            snapshot_id: format!("dsnap_{}", uuid::Uuid::new_v4()),
            timestamp: chrono::Utc::now().timestamp_millis(),
            directions: directions.values().cloned().collect(),
            hash,
        };

        let mut snapshots = self.snapshots.write().await;
        snapshots.push(snapshot.clone());

        let mut stats = self.stats.write().await;
        stats.total_snapshots += 1;

        info!("Direction snapshot created: id={}", snapshot.snapshot_id);
        Ok(snapshot)
    }

    pub async fn get_direction(&self, direction_id: &str) -> Option<Direction> {
        let directions = self.directions.read().await;
        directions.get(direction_id).cloned()
    }

    pub async fn list_directions(&self, asset_class: Option<&AssetClass>) -> Vec<Direction> {
        let directions = self.directions.read().await;
        match asset_class {
            Some(ac) => directions.values().filter(|d| std::mem::discriminant(&d.asset_class) == std::mem::discriminant(ac)).cloned().collect(),
            None => directions.values().cloned().collect(),
        }
    }

    pub async fn get_stats(&self) -> RegistryStats {
        self.stats.read().await.clone()
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Direction Registry stopped");
    }
}
