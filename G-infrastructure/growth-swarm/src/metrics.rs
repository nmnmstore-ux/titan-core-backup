// ============================================================
// Vampire Metrics — إحصائيات حية لمعدلات الاختراق
// ============================================================

use std::sync::atomic::{AtomicU64, Ordering};

pub struct VampireMetrics {
    pub pools_scanned: AtomicU64,
    pub targets_qualified: AtomicU64,
    pub pitches_formulated: AtomicU64,
    pub pitches_onchain: AtomicU64,
    pub migrations_detected: AtomicU64,
    pub volume_migrated: AtomicU64,
}

impl VampireMetrics {
    pub fn new() -> Self {
        Self {
            pools_scanned: AtomicU64::new(0),
            targets_qualified: AtomicU64::new(0),
            pitches_formulated: AtomicU64::new(0),
            pitches_onchain: AtomicU64::new(0),
            migrations_detected: AtomicU64::new(0),
            volume_migrated: AtomicU64::new(0),
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "pools_scanned": self.pools_scanned.load(Ordering::Relaxed),
            "targets_qualified": self.targets_qualified.load(Ordering::Relaxed),
            "pitches_formulated": self.pitches_formulated.load(Ordering::Relaxed),
            "pitches_onchain": self.pitches_onchain.load(Ordering::Relaxed),
            "migrations_detected": self.migrations_detected.load(Ordering::Relaxed),
            "volume_migrated_usd": self.volume_migrated.load(Ordering::Relaxed),
            "hit_rate": if self.pitches_onchain.load(Ordering::Relaxed) > 0 {
                format!("{:.1}%",
                    self.migrations_detected.load(Ordering::Relaxed) as f64
                    / self.pitches_onchain.load(Ordering::Relaxed) as f64 * 100.0)
            } else { "0%".into() },
        })
    }
}
