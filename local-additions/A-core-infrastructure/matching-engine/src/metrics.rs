use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use dashmap::DashMap;

pub struct MetricsCollector {
    orders_processed: AtomicU64,
    trades_executed: AtomicU64,
    dot_settlements: AtomicU64,
    fix_messages: AtomicU64,
    total_volume: AtomicU64,
    errors: AtomicU64,
    health: AtomicBool,
    latency_buckets: Arc<DashMap<u64, AtomicU64>>,
    peak_tps: AtomicU64,
    start_time: Instant,
}

impl MetricsCollector {
    pub fn new() -> Self {
        let buckets = Arc::new(DashMap::new());
        for le in [1, 5, 10, 25, 50, 100, 250, 500, 1000, 5000] {
            buckets.insert(le, AtomicU64::new(0));
        }
        Self {
            orders_processed: AtomicU64::new(0),
            trades_executed: AtomicU64::new(0),
            dot_settlements: AtomicU64::new(0),
            fix_messages: AtomicU64::new(0),
            total_volume: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            health: AtomicBool::new(true),
            latency_buckets: buckets,
            peak_tps: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn inc_orders(&self) { self.orders_processed.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_trades(&self) { self.trades_executed.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_trades_by(&self, n: u64) { self.trades_executed.fetch_add(n, Ordering::Relaxed); }
    pub fn inc_dot(&self) { self.dot_settlements.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_fix(&self) { self.fix_messages.fetch_add(1, Ordering::Relaxed); }
    pub fn add_volume(&self, vol: f64) { self.total_volume.fetch_add((vol * 100.0) as u64, Ordering::Relaxed); }
    pub fn inc_errors(&self) { self.errors.fetch_add(1, Ordering::Relaxed); }
    pub fn set_health(&self, ok: bool) { self.health.store(ok, Ordering::Relaxed); }

    pub fn record_latency(&self, latency_us: u64) {
        for entry in self.latency_buckets.iter() {
            if latency_us <= *entry.key() {
                entry.value().fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        let tps = self.orders_processed.swap(0, Ordering::Relaxed);
        let last_peak = self.peak_tps.load(Ordering::Relaxed);
        if tps > last_peak {
            self.peak_tps.store(tps, Ordering::Relaxed);
        }
        serde_json::json!({
            "tps_current": tps,
            "tps_peak": self.peak_tps.load(Ordering::Relaxed),
            "trades": self.trades_executed.load(Ordering::Relaxed),
            "dot": self.dot_settlements.load(Ordering::Relaxed),
            "fix": self.fix_messages.load(Ordering::Relaxed),
            "volume_24h": self.total_volume.load(Ordering::Relaxed) as f64 / 100.0,
            "errors": self.errors.load(Ordering::Relaxed),
            "health": self.health.load(Ordering::Relaxed),
            "uptime_secs": self.start_time.elapsed().as_secs(),
        })
    }

    pub fn prometheus_cumulative(&self) -> u64 {
        self.orders_processed.load(Ordering::Relaxed)
    }

    pub fn prometheus_text(&self) -> String {
        let orders = self.orders_processed.load(Ordering::Relaxed);
        let trades = self.trades_executed.load(Ordering::Relaxed);
        let dot = self.dot_settlements.load(Ordering::Relaxed);
        let fix = self.fix_messages.load(Ordering::Relaxed);
        let volume = self.total_volume.load(Ordering::Relaxed) as f64 / 100.0;
        let errors = self.errors.load(Ordering::Relaxed);
        let health = self.health.load(Ordering::Relaxed) as u64;
        let uptime = self.start_time.elapsed().as_secs();

        let mut out = String::with_capacity(2048);
        out.push_str("# HELP the_bridge_orders_total Total orders processed\n");
        out.push_str("# TYPE the_bridge_orders_total counter\n");
        out.push_str(&format!("the_bridge_orders_total {}\n", orders));
        out.push_str("# HELP the_bridge_trades_total Total trades executed\n");
        out.push_str("# TYPE the_bridge_trades_total counter\n");
        out.push_str(&format!("the_bridge_trades_total {}\n", trades));
        out.push_str("# HELP the_bridge_dot_settlements DOT settlements\n");
        out.push_str("# TYPE the_bridge_dot_settlements counter\n");
        out.push_str(&format!("the_bridge_dot_settlements {}\n", dot));
        out.push_str("# HELP the_bridge_fix_messages FIX messages processed\n");
        out.push_str("# TYPE the_bridge_fix_messages counter\n");
        out.push_str(&format!("the_bridge_fix_messages {}\n", fix));
        out.push_str("# HELP the_bridge_volume_24h Total volume 24h\n");
        out.push_str("# TYPE the_bridge_volume_24h gauge\n");
        out.push_str(&format!("the_bridge_volume_24h {}\n", volume));
        out.push_str("# HELP the_bridge_errors_total Total errors\n");
        out.push_str("# TYPE the_bridge_errors_total counter\n");
        out.push_str(&format!("the_bridge_errors_total {}\n", errors));
        out.push_str("# HELP the_bridge_health Engine health (1=ok, 0=halted)\n");
        out.push_str("# TYPE the_bridge_health gauge\n");
        out.push_str(&format!("the_bridge_health {}\n", health));
        out.push_str("# HELP the_bridge_uptime_seconds Uptime in seconds\n");
        out.push_str("# TYPE the_bridge_uptime_seconds gauge\n");
        out.push_str(&format!("the_bridge_uptime_seconds {}\n", uptime));
        out.push_str("# HELP the_bridge_peak_tps Highest TPS recorded\n");
        out.push_str("# TYPE the_bridge_peak_tps gauge\n");
        out.push_str(&format!("the_bridge_peak_tps {}\n", self.peak_tps.load(Ordering::Relaxed)));
        out.push_str("# HELP the_bridge_latency Latency histogram buckets (µs)\n");
        out.push_str("# TYPE the_bridge_latency histogram\n");
        for entry in self.latency_buckets.iter() {
            let count = entry.value().load(Ordering::Relaxed);
            out.push_str(&format!("the_bridge_latency_bucket{{le=\"{}\"}} {}\n", entry.key(), count));
        }
        out.push_str(&format!("the_bridge_latency_count {}\n", orders));
        out
    }
}
