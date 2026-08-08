use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug};
use dashmap::DashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

const DEFAULT_HUGEPAGE_SIZE: usize = 2 * 1024 * 1024;
const DEFAULT_NUM_PAGES: usize = 256;
const GIB: usize = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HugePagesConfig {
    pub page_size: usize,
    pub num_pages: usize,
    pub mlock_enabled: bool,
    pub map_hugetlb: bool,
    pub transparent_hugepages: bool,
    pub defrag: String,
    pub enabled: bool,
    pub numa_aware: bool,
    pub prefault_pages: bool,
    pub reserve_at_startup: bool,
    pub allow_partial: bool,
    pub monitoring_interval_secs: u64,
}

impl Default for HugePagesConfig {
    fn default() -> Self {
        Self {
            page_size: DEFAULT_HUGEPAGE_SIZE,
            num_pages: DEFAULT_NUM_PAGES,
            mlock_enabled: true,
            map_hugetlb: true,
            transparent_hugepages: true,
            defrag: "madvise".to_string(),
            enabled: true,
            numa_aware: true,
            prefault_pages: true,
            reserve_at_startup: false,
            allow_partial: true,
            monitoring_interval_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HugePageRegion {
    pub id: u32,
    pub virtual_addr: u64,
    pub size: usize,
    pub num_pages: usize,
    pub allocated: bool,
    pub locked: bool,
    pub numa_node: Option<usize>,
    pub prefaulted: bool,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u64,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HugePagesStats {
    pub total_regions: usize,
    pub total_allocated: usize,
    pub total_locked: usize,
    pub total_bytes: usize,
    pub page_size: usize,
    pub pages_available: usize,
    pub pages_used: usize,
    pub defrag_mode: String,
    pub thp_enabled: bool,
    pub allocation_failures: u64,
    pub lock_failures: u64,
    pub prefault_failures: u64,
    pub total_prefaulted_pages: usize,
    pub numa_distribution: HashMap<usize, usize>,
    pub fragmentation_percent: f64,
}

pub struct HugePagesAllocator {
    config: Arc<RwLock<HugePagesConfig>>,
    regions: Arc<DashMap<u32, HugePageRegion>>,
    stats: Arc<RwLock<HugePagesStats>>,
    running: Arc<RwLock<bool>>,
    next_region_id: Arc<RwLock<u32>>,
    monitor_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl HugePagesAllocator {
    pub fn new(config: HugePagesConfig) -> Self {
        let total_pages = config.num_pages;
        let page_size = config.page_size;
        
        Self {
            config: Arc::new(RwLock::new(config)),
            regions: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(HugePagesStats {
                total_regions: 0,
                total_allocated: 0,
                total_locked: 0,
                total_bytes: 0,
                page_size: DEFAULT_HUGEPAGE_SIZE,
                pages_available: DEFAULT_NUM_PAGES,
                pages_used: 0,
                defrag_mode: "madvise".to_string(),
                thp_enabled: true,
                allocation_failures: 0,
                lock_failures: 0,
                prefault_failures: 0,
                total_prefaulted_pages: 0,
                numa_distribution: HashMap::new(),
                fragmentation_percent: 0.0,
            })),
            running: Arc::new(RwLock::new(false)),
            next_region_id: Arc::new(RwLock::new(1)),
            monitor_handle: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        {
            let mut running = self.running.write().await;
            *running = true;
        }

        let config = self.config.read().await;
        info!(
            "HugePages allocator started — page_size={}KB num_pages={} mlock={} numa_aware={} prefault={} thp={}",
            config.page_size / 1024, config.num_pages, config.mlock_enabled, config.numa_aware, config.prefault_pages, config.transparent_hugepages
        );

        if config.reserve_at_startup {
            self.reserve_all_pages().await?;
        }

        self.start_monitor().await;
        Ok(())
    }

    async fn reserve_all_pages(&self) -> Result<(), String> {
        let config = self.config.read().await;
        for _ in 0..config.num_pages {
            let _ = self.allocate(config.page_size).await;
        }
        Ok(())
    }

    async fn start_monitor(&self) {
        let stats = self.stats.clone();
        let regions = self.regions.clone();
        let running = self.running.clone();
        let interval = self.config.read().await.monitoring_interval_secs;

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval));
            loop {
                interval.tick().await;
                let running_guard = running.read().await;
                if !*running_guard {
                    break;
                }
                drop(running_guard);

                let regions_guard: Vec<_> = regions.iter().map(|e| e.value().clone()).collect();
                let mut numa_dist = HashMap::new();
                let mut total_prefaulted = 0;
                let mut total_pages_used = 0;

                for r in regions_guard {
                    if r.allocated {
                        total_pages_used += r.num_pages;
                        if r.prefaulted {
                            total_prefaulted += 1;
                        }
                    }
                    if let Some(node) = r.numa_node {
                        *numa_dist.entry(node).or_insert(0) += 1;
                    }
                }

                let mut stats_guard = stats.write().await;
                stats_guard.total_prefaulted_pages = total_prefaulted;
                stats_guard.pages_used = total_pages_used;
                stats_guard.numa_distribution = numa_dist;
                stats_guard.fragmentation_percent = 0.0;

                debug!(
                    "HugePages: used={} prefaulted={} fragmented={:.1}%",
                    stats_guard.pages_used, stats_guard.total_prefaulted_pages, stats_guard.fragmentation_percent
                );
            }
        });

        let mut handle_guard = self.monitor_handle.write().await;
        *handle_guard = Some(handle);
    }

    pub async fn allocate(&self, size: usize) -> Result<u32, String> {
        let config = self.config.read().await;
        if !config.enabled {
            return Err("huge pages disabled".to_string());
        }

        let pages_needed = (size + config.page_size - 1) / config.page_size;

        {
            let mut stats = self.stats.write().await;
            if stats.pages_used + pages_needed > config.num_pages {
                stats.allocation_failures += 1;
                return Err("insufficient huge pages".to_string());
            }
        }

        let mut id_guard = self.next_region_id.write().await;
        let id = *id_guard;
        *id_guard += 1;

        let numa_node = if config.numa_aware {
            Some(rand::random::<usize>() % num_cpus::get())
        } else {
            None
        };

        let region = HugePageRegion {
            id,
            virtual_addr: 0x7f00_0000_0000 + (id as u64 * config.num_pages as u64 * config.page_size as u64),
            size: pages_needed * config.page_size,
            num_pages: pages_needed,
            allocated: true,
            locked: config.mlock_enabled,
            numa_node,
            prefaulted: config.prefault_pages,
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            access_count: 0,
            purpose: "auto".to_string(),
        };

        {
            let mut stats = self.stats.write().await;
            stats.total_regions += 1;
            stats.total_allocated += 1;
            if config.mlock_enabled {
                stats.total_locked += 1;
            }
            stats.total_bytes += region.size;
            stats.pages_used += pages_needed;
            stats.pages_available -= pages_needed;
        }

        self.regions.insert(id, region.clone());

        info!(
            "HugePages allocated: id={} size={}MB pages={} numa_node={:?} locked={} prefaulted={}",
            id, region.size / (1024*1024), pages_needed, numa_node, config.mlock_enabled, config.prefault_pages
        );

        Ok(id)
    }

    pub async fn allocate_for(&self, size: usize, purpose: &str, numa_node: Option<usize>) -> Result<u32, String> {
        let config = self.config.read().await;
        if !config.enabled {
            return Err("huge pages disabled".to_string());
        }

        let pages_needed = (size + config.page_size - 1) / config.page_size;
        
        {
            let mut stats = self.stats.write().await;
            if stats.pages_used + pages_needed > config.num_pages {
                stats.allocation_failures += 1;
                return Err("insufficient huge pages".to_string());
            }
        }

        let mut id_guard = self.next_region_id.write().await;
        let id = *id_guard;
        *id_guard += 1;

        let final_numa = if config.numa_aware && numa_node.is_none() {
            Some(rand::random::<usize>() % num_cpus::get())
        } else {
            numa_node
        };

        let region = HugePageRegion {
            id,
            virtual_addr: 0x7f00_0000_0000 + (id as u64 * config.num_pages as u64 * config.page_size as u64),
            size: pages_needed * config.page_size,
            num_pages: pages_needed,
            allocated: true,
            locked: config.mlock_enabled,
            numa_node: final_numa,
            prefaulted: config.prefault_pages,
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            access_count: 0,
            purpose: purpose.to_string(),
        };

        {
            let mut stats = self.stats.write().await;
            stats.total_regions += 1;
            stats.total_allocated += 1;
            if config.mlock_enabled {
                stats.total_locked += 1;
            }
            stats.total_bytes += region.size;
            stats.pages_used += pages_needed;
            stats.pages_available -= pages_needed;
        }

        if let Some(node) = final_numa {
            let mut stats = self.stats.write().await;
            *stats.numa_distribution.entry(node).or_insert(0) += 1;
        }

        self.regions.insert(id, region.clone());

        info!(
            "HugePages allocated for '{}': id={} size={}MB pages={} numa_node={:?} locked={}",
            purpose, id, region.size / (1024*1024), pages_needed, final_numa, config.mlock_enabled
        );

        Ok(id)
    }

    pub async fn deallocate(&self, id: u32) -> Result<(), String> {
        let entry = self.regions.remove(&id).ok_or("region not found")?.1;

        let mut stats = self.stats.write().await;
        stats.total_regions -= 1;
        stats.total_allocated -= 1;
        if entry.locked {
            stats.total_locked -= 1;
        }
        stats.total_bytes -= entry.size;
        stats.pages_used -= entry.num_pages;
        stats.pages_available += entry.num_pages;

        if let Some(node) = entry.numa_node {
            let mut stats = self.stats.write().await;
            if let Some(count) = stats.numa_distribution.get_mut(&node) {
                *count = count.saturating_sub(1);
            }
        }

        info!("HugePages deallocated: id={} size={}MB", id, entry.size / (1024*1024));
        Ok(())
    }

    pub async fn access(&self, id: u32) -> Result<(), String> {
        if let Some(mut entry) = self.regions.get_mut(&id) {
            entry.last_accessed = Utc::now();
            entry.access_count += 1;
            Ok(())
        } else {
            Err("region not found".to_string())
        }
    }

    pub async fn get_region(&self, id: u32) -> Option<HugePageRegion> {
        self.regions.get(&id).map(|e| e.value().clone())
    }

    pub async fn list_regions(&self, purpose: Option<&str>) -> Vec<HugePageRegion> {
        match purpose {
            Some(p) => self.regions.iter()
                .filter(|e| e.value().purpose == p)
                .map(|e| e.value().clone())
                .collect(),
            None => self.regions.iter().map(|e| e.value().clone()).collect(),
        }
    }

    pub async fn get_stats(&self) -> HugePagesStats {
        self.stats.read().await.clone()
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn stop(&self) {
        {
            let mut running = self.running.write().await;
            *running = false;
        }

        if let Some(handle) = self.monitor_handle.write().await.take() {
            handle.abort();
        }

        info!("HugePages allocator stopped — {} regions", self.regions.len());
    }
}

use rand;
use num_cpus;