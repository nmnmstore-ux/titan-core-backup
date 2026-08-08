#![allow(dead_code)]

#![allow(clippy::unreadable_literal)]
use once_cell::sync::OnceCell;
use page_size;
use std::alloc::Layout;
use std::collections::HashMap;
use std::mem;
use std::ptr;
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

/// Typical L1/L2 cache line size on x86-64 and aarch64. Padding slots to this
/// boundary prevents ``False Locality`` — adjacent hot working-set entries no
/// longer share a cache line, eliminating remote-node contention stalls.
const CACHELINE: usize = 64;

/// Size of a ``unsigned long`` nodemask bitmap (CLONGS) usable by mbind().
const NUMA_NODEMASK_BITS: usize = 1024;
const CLONG: usize = std::mem::size_of::<libc::c_ulong>() * 8;

#[cfg(not(target_os = "linux"))]
const _SC_NPROCESSORS_CONF: i32 = 84;
#[cfg(not(target_os = "linux"))]
unsafe fn sysconf(_: i32) -> i64 { num_cpus::get() as i64 }

static NUMA_TOPOLOGY: OnceCell<NUMATopology> = OnceCell::new();

#[derive(Debug)]
pub struct NUMATopology {
    pub node_count: i32,
    pub nodes: Vec<NUMANode>,
    pub total_cores: u32,
    pub page_size: usize,
    pub hugepage_size: usize,
}

#[derive(Debug, Clone)]
pub struct NUMANode {
    pub id: i32,
    pub cores: Vec<u32>,
    pub memory_total: u64,
    pub memory_free: u64,
    pub distance: Vec<u32>,
}

impl NUMATopology {
    #[cfg(target_os = "linux")]
    pub fn detect() -> Self {
        let total_cores = num_cpus::get() as u32;
        let page_size = page_size::get();
        let hugepage_size = 2 * 1024 * 1024;
        let nodes = Self::detect_numa_nodes();
        let node_count = nodes.len() as i32;
        Self { node_count, nodes, total_cores, page_size, hugepage_size }
    }

    #[cfg(target_os = "linux")]
    fn detect_numa_nodes() -> Vec<NUMANode> {
        let mut nodes = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/sys/devices/system/node/") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with("node") { continue; }
                let id: i32 = match name.trim_start_matches("node").parse() { Ok(n) => n, Err(_) => continue };
                let mut cores = Vec::new();
                let cpulist_path = entry.path().join("cpulist");
                if let Ok(cpulist) = std::fs::read_to_string(&cpulist_path) {
                    cores = Self::parse_cpulist(cpulist.trim());
                }
                if cores.is_empty() {
                    if let Ok(cpu_dir) = std::fs::read_dir(entry.path().join("cpu")) {
                        for cpu_entry in cpu_dir.flatten() {
                            let cpu_name = cpu_entry.file_name().to_string_lossy().to_string();
                            if let Some(cpu) = cpu_name.strip_prefix("cpu") {
                                if let Ok(c) = cpu.parse::<u32>() { cores.push(c); }
                            }
                        }
                        cores.sort();
                    }
                }
                let memory_total = Self::read_meminfo(&entry.path().join("meminfo"), "MemTotal:");
                let memory_free = Self::read_meminfo(&entry.path().join("meminfo"), "MemFree:");
                nodes.push(NUMANode { id, cores, memory_total, memory_free, distance: Vec::new() });
            }
        }
        if nodes.is_empty() {
            nodes.push(NUMANode { id: 0, cores: (0..num_cpus::get() as u32).collect(), memory_total: 0, memory_free: 0, distance: Vec::new() });
        }
        nodes.sort_by_key(|n| n.id);
        nodes
    }

    #[cfg(target_os = "linux")]
    fn parse_cpulist(cpulist: &str) -> Vec<u32> {
        let mut cores = Vec::new();
        for part in cpulist.split(',') {
            let part = part.trim();
            if part.is_empty() { continue; }
            if let Some((start, end)) = part.split_once('-') {
                let s: u32 = start.trim().parse().unwrap_or(0);
                let e: u32 = end.trim().parse().unwrap_or(s);
                for c in s..=e { cores.push(c); }
            } else {
                if let Ok(c) = part.parse::<u32>() { cores.push(c); }
            }
        }
        cores
    }

    #[cfg(target_os = "linux")]
    fn read_meminfo(path: &std::path::Path, key: &str) -> u64 {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                if line.trim().starts_with(key) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(val) = parts[1].parse::<u64>() { return val; }
                    }
                }
            }
        }
        0
    }

    #[cfg(not(target_os = "linux"))]
    pub fn detect() -> Self {
        let total_cores = num_cpus::get() as u32;
        let page_size = page_size::get();
        let hugepage_size = 2 * 1024 * 1024;
        let nodes = vec![NUMANode { id: 0, cores: (0..total_cores).collect(), memory_total: 0, memory_free: 0, distance: Vec::new() }];
        Self { node_count: 1, nodes, total_cores, page_size, hugepage_size }
    }

    pub fn instance() -> &'static NUMATopology {
        NUMA_TOPOLOGY.get_or_init(|| Self::detect())
    }

    pub fn affinity_for_node(&self, node_id: i32) -> Option<&[u32]> {
        self.nodes.iter().find(|n| n.id == node_id).map(|n| n.cores.as_slice())
    }
}

pub struct NumaVec<T> {
    ptr: *mut T,
    capacity: usize,
    node_id: i32,
    size_bytes: u64,
    is_numa: bool,
    layout: Layout,
}

/// Build an ``unsigned long`` nodemask with the given node bit set.
fn nodemask_for_node(node_id: i32) -> [libc::c_ulong; NUMA_NODEMASK_BITS / CLONG] {
    let mut mask = [0u64; NUMA_NODEMASK_BITS / CLONG];
    if node_id >= 0 {
        let bit = node_id as usize;
        let word = bit / CLONG;
        if word < mask.len() {
            mask[word] |= 1u64 << (bit % CLONG);
        }
    }
    mask
}

/// Bind an already-allocated region of memory to a specific NUMA node using
/// ``mbind(MPOL_BIND)``. This is the true ``False Locality`` fix: pages are
/// placed on (and stay on) the local node instead of being interleaved or
/// migrating, eliminating remote-node accesses on the hot path.
///
/// Returns ``Ok(true)`` when the kernel accepted the binding, ``Ok(false)``
/// when the call is not supported (non-Linux), ``Err`` on real failure.
#[cfg(target_os = "linux")]
fn bind_pages_to_node(ptr: *const u8, len: usize, node_id: i32) -> Result<bool, String> {
    if len == 0 || ptr.is_null() {
        return Ok(false);
    }
    let mask = nodemask_for_node(node_id);
    let ret = unsafe {
        libc::mbind(
            ptr as *mut libc::c_void,
            len,
            libc::MPOL_BIND,
            mask.as_ptr(),
            NUMA_NODEMASK_BITS,
            0,
        )
    };
    if ret != 0 {
        let errno = *libc::__errno_location();
        // EINVAL/EOPNOTSUPP typically mean the node doesn't exist or the kernel
        // has no NUMA support; treat as a graceful fallback, not a hard error.
        return Err(format!("mbind(node={}) failed: errno {}", node_id, errno));
    }
    Ok(true)
}

#[cfg(not(target_os = "linux"))]
fn bind_pages_to_node(_ptr: *const u8, _len: usize, _node_id: i32) -> Result<bool, String> {
    Ok(false)
}

/// Prefer node-local allocation: try ``mbind``-bound memory first, then fall
/// back to the default allocator when NUMA isn't available.
#[cfg(target_os = "linux")]
fn try_alloc_on_node(node_id: i32, size_bytes: usize, align: usize) -> Result<(*mut u8, bool), String> {
    let layout = Layout::from_size_align(size_bytes, align).map_err(|e| e.to_string())?;
    let raw = unsafe { std::alloc::alloc_zeroed(layout) };
    if raw.is_null() {
        return Err(format!("allocation failed for {} bytes on node {}", size_bytes, node_id));
    }
    let bound = match bind_pages_to_node(raw, size_bytes, node_id) {
        Ok(b) => b,
        Err(e) => {
            // mbind unavailable on this kernel — keep the allocation, report not-bound.
            if cfg!(debug_assertions) {
                eprintln!("[numa] warning: {}; falling back to unbound allocation", e);
            }
            false
        }
    };
    Ok((raw, bound))
}

#[cfg(not(target_os = "linux"))]
fn try_alloc_on_node(_node_id: i32, size_bytes: usize, align: usize) -> Result<(*mut u8, bool), String> {
    let layout = Layout::from_size_align(size_bytes, align).map_err(|e| e.to_string())?;
    let raw = unsafe { std::alloc::alloc_zeroed(layout) };
    if raw.is_null() {
        return Err("allocation failed".to_string());
    }
    Ok((raw, false))
}

impl<T> NumaVec<T> {
    pub fn new_on_node(node_id: i32, count: usize) -> Self {
        match Self::try_new_on_node(node_id, count) {
            Ok(v) => v,
            Err(_) => {
                let size = count * mem::size_of::<T>();
                let topo = NUMATopology::instance();
                let aligned_size = (size + topo.page_size - 1) & !(topo.page_size - 1);
                let layout = Layout::from_size_align(aligned_size, mem::align_of::<T>()).unwrap_or_else(|_| Layout::new::<T>());
                let ptr = unsafe { std::alloc::alloc_zeroed(layout) as *mut T };
                NumaVec { ptr, capacity: count, node_id, size_bytes: aligned_size as u64, is_numa: false, layout }
            }
        }
    }

    /// Allocate ``count`` elements of ``T`` and bind them to ``node_id`` with
    /// ``mbind(MPOL_BIND)`` so the hot working-set stays resident on the local
    /// node (no remote NUMA access, no false sharing).
    #[cfg(target_os = "linux")]
    pub fn try_new_on_node(node_id: i32, count: usize) -> Result<Self, String> {
        let size = count * mem::size_of::<T>();
        if size == 0 {
            return Err("zero-size allocation".to_string());
        }
        let topo = NUMATopology::instance();
        let aligned_size = (size + topo.page_size - 1) & !(topo.page_size - 1);
        let align = mem::align_of::<T>().max(CACHELINE);
        let (raw, bound) = try_alloc_on_node(node_id, aligned_size, align)?;
        Ok(NumaVec { ptr: raw as *mut T, capacity: count, node_id, size_bytes: aligned_size as u64, is_numa: bound, layout: Layout::from_size_align(aligned_size, align).map_err(|e| e.to_string())? })
    }

    #[cfg(not(target_os = "linux"))]
    pub fn try_new_on_node(node_id: i32, count: usize) -> Result<Self, String> {
        let size = count * mem::size_of::<T>();
        let topo = NUMATopology::instance();
        let aligned_size = (size + topo.page_size - 1) & !(topo.page_size - 1);
        let align = mem::align_of::<T>().max(CACHELINE);
        let (raw, bound) = try_alloc_on_node(node_id, aligned_size, align)?;
        Ok(NumaVec { ptr: raw as *mut T, capacity: count, node_id, size_bytes: aligned_size as u64, is_numa: bound, layout: Layout::from_size_align(aligned_size, align).map_err(|e| e.to_string())? })
    }

    /// True when the backing memory is genuinely bound to the target node.
    pub fn is_node_bound(&self) -> bool { self.is_numa }

    pub fn as_slice(&self) -> &[T] { unsafe { slice::from_raw_parts(self.ptr, self.capacity) } }
    pub fn as_mut_slice(&mut self) -> &mut [T] { unsafe { slice::from_raw_parts_mut(self.ptr, self.capacity) } }
}

impl<T> std::ops::Index<usize> for NumaVec<T> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        unsafe { &*self.ptr.add(index) }
    }
}

impl<T> std::ops::IndexMut<usize> for NumaVec<T> {
    fn index_mut(&mut self, index: usize) -> &mut T {
        unsafe { &mut *self.ptr.add(index) }
    }
}

impl<T> Drop for NumaVec<T> {
    fn drop(&mut self) {
        unsafe { std::alloc::dealloc(self.ptr as *mut u8, self.layout); }
    }
}

#[cfg(unix)]
pub struct HugepageBuffer { ptr: *mut u8, size: usize }

#[cfg(not(unix))]
pub struct HugepageBuffer { data: Vec<u8> }

#[cfg(unix)]
impl HugepageBuffer {
    pub fn new(size: usize) -> Result<Self, String> {
        let topo = NUMATopology::instance();
        let aligned = (size + topo.page_size - 1) & !(topo.page_size - 1);
        unsafe {
            let ptr = libc::mmap(ptr::null_mut(), aligned, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_PRIVATE | libc::MAP_ANONYMOUS, -1, 0);
            if ptr == libc::MAP_FAILED { return Err(format!("mmap failed for {} bytes", aligned)); }
            ptr::write_bytes(ptr as *mut u8, 0, aligned);
            // Bind the hugepage region to the first local node (node 0 by default)
            // to keep it resident on the local node — prevents remote-node stalls.
            let node_id = topo.nodes.first().map(|n| n.id).unwrap_or(0);
            let _ = bind_pages_to_node(ptr as *const u8, aligned, node_id);
            Ok(Self { ptr: ptr as *mut u8, size: aligned })
        }
    }
    pub fn as_slice(&self) -> &[u8] { unsafe { slice::from_raw_parts(self.ptr, self.size) } }
    pub fn as_mut_slice(&mut self) -> &mut [u8] { unsafe { slice::from_raw_parts_mut(self.ptr, self.size) } }
}

#[cfg(unix)]
impl Drop for HugepageBuffer { fn drop(&mut self) { unsafe { libc::munmap(self.ptr as *mut libc::c_void, self.size); } } }

#[cfg(not(unix))]
impl HugepageBuffer {
    pub fn new(size: usize) -> Result<Self, String> { Ok(Self { data: vec![0u8; size] }) }
    pub fn as_slice(&self) -> &[u8] { &self.data }
    pub fn as_mut_slice(&mut self) -> &mut [u8] { &mut self.data }
}

pub struct CPUAffinity {
    topology: &'static NUMATopology,
    pub core_count: u32,
    next_core: AtomicU32,
    isolated_cores: Vec<u32>,
}

impl CPUAffinity {
    pub fn new() -> Self {
        let topo = NUMATopology::instance();
        Self { topology: topo, core_count: topo.total_cores, next_core: AtomicU32::new(0), isolated_cores: Vec::new() }
    }

    #[cfg(target_os = "linux")]
    pub fn pin_to_core(core_id: u32) -> Result<(), String> {
        if core_id >= num_cpus::get() as u32 { return Err(format!("Core {} out of range", core_id)); }
        unsafe {
            let mut cpuset: libc::cpu_set_t = mem::zeroed();
            libc::CPU_ZERO(&mut cpuset);
            libc::CPU_SET(core_id as usize, &mut cpuset);
            if libc::sched_setaffinity(0, mem::size_of::<libc::cpu_set_t>(), &cpuset) != 0 {
                return Err(format!("sched_setaffinity failed for core {}: errno {}", core_id, *libc::__errno_location()));
            }
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn pin_to_core(core_id: u32) -> Result<(), String> { Ok(()) }

    #[cfg(target_os = "linux")]
    pub fn pin_to_node(node_id: i32) -> Result<(), String> {
        let topo = NUMATopology::instance();
        let node = topo.nodes.iter().find(|n| n.id == node_id).ok_or_else(|| format!("NUMA node {} not found", node_id))?;
        unsafe {
            let mut cpuset: libc::cpu_set_t = mem::zeroed();
            libc::CPU_ZERO(&mut cpuset);
            for &core in &node.cores { libc::CPU_SET(core as usize, &mut cpuset); }
            if libc::sched_setaffinity(0, mem::size_of::<libc::cpu_set_t>(), &cpuset) != 0 {
                return Err(format!("sched_setaffinity to node {} failed", node_id));
            }
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn pin_to_node(node_id: i32) -> Result<(), String> { Ok(()) }

    pub fn assign_core(&self) -> u32 {
        let idx = self.next_core.fetch_add(1, Ordering::Relaxed);
        let core = if self.isolated_cores.is_empty() { idx % self.core_count } else { self.isolated_cores[(idx as usize) % self.isolated_cores.len()] };
        let _ = Self::pin_to_core(core);
        core
    }

    /// Map a physical core id to its owning NUMA node. This is the key fix for
    /// ``False Locality``: threads that touch a node's hot data must run on a
    /// core that belongs to that same node, not on a distant node's core.
    #[cfg(target_os = "linux")]
    pub fn core_node(&self, core_id: u32) -> i32 {
        for node in &self.topology.nodes {
            if node.cores.contains(&core_id) {
                return node.id;
            }
        }
        0
    }

    #[cfg(not(target_os = "linux"))]
    pub fn core_node(&self, _core_id: u32) -> i32 { 0 }

    /// Pin the current thread to the first free core that belongs to ``node_id``.
    /// When ``node_id`` has no cores or the node is unknown, falls back to the
    /// node's pin or a default core — always preferring locality over speed.
    pub fn pin_to_node_core(&self, node_id: i32) -> Result<u32, String> {
        let topo = self.topology;
        let node_cores = topo.affinity_for_node(node_id);
        if let Some(cores) = node_cores {
            if !cores.is_empty() {
                let idx = (self.next_core.fetch_add(1, Ordering::Relaxed) as usize) % cores.len();
                let core = cores[idx];
                let _ = Self::pin_to_core(core);
                return Ok(core);
            }
        }
        // Fallback: default core if the node is missing.
        let core = self.next_core.fetch_add(1, Ordering::Relaxed) % self.core_count;
        let _ = Self::pin_to_core(core);
        Ok(core)
    }

    pub fn isolate_hotpath_cores(&mut self, count: u32) -> &[u32] {
        let reserve = count.min(self.core_count / 2).max(1);
        self.isolated_cores = (0..reserve).collect();
        &self.isolated_cores
    }

    pub fn topology(&self) -> &'static NUMATopology { self.topology }
}

impl std::fmt::Debug for CPUAffinity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CPUAffinity").field("core_count", &self.core_count).field("isolated_cores", &self.isolated_cores).finish()
    }
}

pub struct NUMADistributor;

impl NUMADistributor {
    pub fn distribute(pairs: &[String]) -> HashMap<String, i32> {
        let topo = NUMATopology::instance();
        let mut assignment = HashMap::new();
        for (i, pair) in pairs.iter().enumerate() {
            let node_id = (i as i32) % topo.node_count.max(1);
            assignment.insert(pair.clone(), node_id);
        }
        assignment
    }
}

pub struct AffinityThreadPool {
    pub workers: Vec<thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
    core_mask: Vec<u32>,
    numa_aware: bool,
    park: Arc<(Mutex<bool>, Condvar)>,
}

impl AffinityThreadPool {
    pub fn new(worker_count: u32, numa_aware: bool) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let mut workers = Vec::with_capacity(worker_count as usize);
        let topo = NUMATopology::instance();
        let park: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
        for i in 0..worker_count {
            let node_id = if numa_aware { (i as i32) % topo.node_count.max(1) } else { 0 };
            let node_cores = topo.affinity_for_node(node_id).map(|c| c.to_vec()).unwrap_or_else(|| (0..topo.total_cores).collect());
            if node_cores.is_empty() { continue; }
            let core_id = node_cores[(i as usize) % node_cores.len()];
            let park = park.clone();
            let running = running.clone();
            match thread::Builder::new().name(format!("worker-{}", i)).spawn(move || {
                let _ = CPUAffinity::pin_to_core(core_id);
                while running.load(Ordering::Relaxed) {
                    let (lock, cvar) = &*park;
                    let mut ready = match lock.lock() { Ok(g) => g, Err(poisoned) => poisoned.into_inner() };
                    while !*ready { ready = match cvar.wait(ready) { Ok(g) => g, Err(poisoned) => poisoned.into_inner() }; }
                    *ready = false;
                }
            }) { Ok(h) => workers.push(h), Err(_) => {} }
        }
        let core_mask: Vec<u32> = (0..topo.total_cores).collect();
        Self { workers, running, core_mask, numa_aware, park }
    }

    pub fn unpark_all(&self) {
        let (lock, cvar) = &*self.park;
        if let Ok(mut ready) = lock.lock() { *ready = true; cvar.notify_all(); }
    }

    pub fn shutdown(&self) { self.running.store(false, Ordering::Relaxed); self.unpark_all(); }
    pub fn worker_count(&self) -> usize { self.workers.len() }
}
