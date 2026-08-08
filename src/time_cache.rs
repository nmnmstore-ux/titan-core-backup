use std::sync::atomic::{AtomicI64, Ordering};
use std::thread;
use std::time::Duration;

static CACHED_MS: AtomicI64 = AtomicI64::new(0);
static CACHED_NS: AtomicI64 = AtomicI64::new(0);
static MONO_COUNTER: AtomicI64 = AtomicI64::new(0);
static INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const CACHE_INTERVAL_US: u64 = 100;

pub fn init_timestamp_cache() {
    if INITIALIZED.swap(true, Ordering::Relaxed) {
        return;
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let now_ns = now_ms * 1_000_000;

    CACHED_MS.store(now_ms, Ordering::Relaxed);
    CACHED_NS.store(now_ns, Ordering::Relaxed);

    thread::spawn(|| loop {
        thread::sleep(Duration::from_micros(CACHE_INTERVAL_US));
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let now_ns = now_ms * 1_000_000;
        CACHED_MS.store(now_ms, Ordering::Relaxed);
        CACHED_NS.store(now_ns, Ordering::Relaxed);
    });
}

pub fn fast_now_ms() -> i64 {
    if !INITIALIZED.load(Ordering::Relaxed) {
        init_timestamp_cache();
    }
    CACHED_MS.load(Ordering::Relaxed)
}

pub fn fast_now_ns() -> i64 {
    if !INITIALIZED.load(Ordering::Relaxed) {
        init_timestamp_cache();
    }
    CACHED_NS.load(Ordering::Relaxed)
}

pub fn fast_now_monotonic() -> i64 {
    if !INITIALIZED.load(Ordering::Relaxed) {
        init_timestamp_cache();
    }
    let base = CACHED_NS.load(Ordering::Relaxed);
    let diff = MONO_COUNTER.fetch_add(1, Ordering::Relaxed);
    base + diff
}
