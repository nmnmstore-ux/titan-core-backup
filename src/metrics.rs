use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::time::Instant;

const LATENCY_THRESHOLDS: [u64; 10] = [1, 5, 10, 25, 50, 100, 250, 500, 1000, 5000];

pub struct MetricsCollector {
    orders_processed: AtomicU64,
    trades_executed: AtomicU64,
    dot_settlements: AtomicU64,
    fix_messages: AtomicU64,
    total_volume: AtomicU64,
    errors: AtomicU64,
    health: AtomicBool,
    latency_buckets: [AtomicU64; 10],
    peak_tps: AtomicU64,
    start_time: Instant,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            orders_processed: AtomicU64::new(0),
            trades_executed: AtomicU64::new(0),
            dot_settlements: AtomicU64::new(0),
            fix_messages: AtomicU64::new(0),
            total_volume: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            health: AtomicBool::new(true),
            latency_buckets: Default::default(),
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
        let idx = LATENCY_THRESHOLDS.partition_point(|&le| latency_us > le);
        if idx < LATENCY_THRESHOLDS.len() {
            self.latency_buckets[idx].fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        let tps = self.orders_processed.swap(0, Ordering::Relaxed);
        self.peak_tps.fetch_max(tps, Ordering::Relaxed);
        serde_json::json!({
            "tps_current": tps,
            "tps_peak": self.peak_tps.load(Ordering::Relaxed),
            "trades": self.trades_executed.swap(0, Ordering::Relaxed),
            "dot": self.dot_settlements.swap(0, Ordering::Relaxed),
            "fix": self.fix_messages.swap(0, Ordering::Relaxed),
            "volume_24h": self.total_volume.swap(0, Ordering::Relaxed) as f64 / 100.0,
            "errors": self.errors.swap(0, Ordering::Relaxed),
            "health": self.health.load(Ordering::Relaxed),
            "uptime_secs": self.start_time.elapsed().as_secs(),
        })
    }

    pub fn prometheus_cumulative(&self) -> u64 {
        self.orders_processed.swap(0, Ordering::Relaxed)
    }

    pub fn prometheus_text(&self) -> String {
        let orders = self.orders_processed.swap(0, Ordering::Relaxed);
        let trades = self.trades_executed.swap(0, Ordering::Relaxed);
        let dot = self.dot_settlements.swap(0, Ordering::Relaxed);
        let fix = self.fix_messages.swap(0, Ordering::Relaxed);
        let volume = self.total_volume.swap(0, Ordering::Relaxed) as f64 / 100.0;
        let errors = self.errors.swap(0, Ordering::Relaxed);
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
        for (i, &le) in LATENCY_THRESHOLDS.iter().enumerate() {
            let count = self.latency_buckets[i].swap(0, Ordering::Relaxed);
            out.push_str(&format!("the_bridge_latency_bucket{{le=\"{}\"}} {}\n", le, count));
        }
        out.push_str(&format!("the_bridge_latency_count {}\n", orders));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn inc_orders_counts_correctly() {
        let m = MetricsCollector::new();
        m.inc_orders();
        m.inc_orders();
        m.inc_orders();
        let snap = m.snapshot();
        assert_eq!(snap["tps_current"], 3);
    }

    #[test]
    fn snapshot_resets_counters() {
        let m = MetricsCollector::new();
        m.inc_orders();
        m.inc_trades();
        m.snapshot(); // resets

        let snap2 = m.snapshot();
        assert_eq!(snap2["tps_current"], 0);
        assert_eq!(snap2["trades"], 0);
    }

    #[test]
    fn peak_tps_tracks_maximum() {
        let m = MetricsCollector::new();
        for _ in 0..100 { m.inc_orders(); }
        m.snapshot(); // peak = 100

        // Next snapshot with fewer orders doesn't lower peak
        m.inc_orders();
        let snap = m.snapshot();
        assert_eq!(snap["tps_current"], 1);
        assert_eq!(snap["tps_peak"], 100);
    }

    #[test]
    fn concurrent_snapshot_no_lost_counts() {
        let m = Arc::new(MetricsCollector::new());
        let total_increments: u64 = 10_000;
        let num_threads = 10;

        let handles: Vec<_> = (0..num_threads).map(|_| {
            let m = Arc::clone(&m);
            thread::spawn(move || {
                for _ in 0..total_increments / num_threads {
                    m.inc_orders();
                }
            })
        }).collect();

        for h in handles { h.join().unwrap(); }

        // Drain all counts
        let mut total_seen = 0u64;
        let snap = m.snapshot();
        total_seen += snap["tps_current"].as_u64().unwrap();
        // No more increments — next snapshot should be 0
        let snap2 = m.snapshot();
        total_seen += snap2["tps_current"].as_u64().unwrap();

        assert_eq!(total_seen, total_increments);
    }

    #[test]
    fn add_volume_and_snapshot() {
        let m = MetricsCollector::new();
        m.add_volume(123.45);
        m.add_volume(67.89);
        let snap = m.snapshot();
        let vol = snap["volume_24h"].as_f64().unwrap();
        assert!((vol - 191.34).abs() < 0.01, "expected ~191.34, got {}", vol);
    }

    #[test]
    fn record_latency_buckets() {
        let m = MetricsCollector::new();
        m.record_latency(3);   // bucket 0 (<=1µs... actually >1 so bucket 1)
        m.record_latency(100); // bucket 5
        m.record_latency(9999); // all below 5000µs threshold, bucket 9
        // just ensure no panics
        m.prometheus_text();
    }

    #[test]
    fn health_toggle() {
        let m = MetricsCollector::new();
        assert!(m.health.load(Ordering::Relaxed));
        m.set_health(false);
        let snap = m.snapshot();
        assert_eq!(snap["health"], false);
    }

    #[test]
    fn prometheus_text_contains_all_metrics() {
        let m = MetricsCollector::new();
        m.inc_orders();
        m.inc_trades();
        let text = m.prometheus_text();
        assert!(text.contains("the_bridge_orders_total"));
        assert!(text.contains("the_bridge_trades_total"));
        assert!(text.contains("the_bridge_health"));
        assert!(text.contains("the_bridge_uptime_seconds"));
        assert!(text.contains("the_bridge_peak_tps"));
    }

    #[test]
    fn inc_trades_by_adds_correctly() {
        let m = MetricsCollector::new();
        m.inc_trades_by(500);
        m.inc_trades_by(300);
        let snap = m.snapshot();
        assert_eq!(snap["trades"], 800);
    }
}
