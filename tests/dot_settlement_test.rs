#![allow(dead_code)]

use std::time::{Duration, Instant};
use uuid::Uuid;

#[cfg(target_os = "linux")]
const DOT_TARGET_MS: u128 = 30_000;
#[cfg(not(target_os = "linux"))]
const DOT_TARGET_MS: u128 = 30_000;

#[path = "../src/types.rs"]
mod types;
#[path = "../src/dot.rs"]
mod dot;
#[path = "../src/tee.rs"]
mod tee;
#[path = "../src/numa.rs"]
mod engine_numa;
#[path = "../src/cloak.rs"]
mod cloak;
#[path = "../src/snapshot.rs"]
mod snapshot;

use std::sync::Arc;
use types::*;
use dot::DOTEngine;
use tee::TEEEnclave;

const DOT_DEADLINE_MS: u64 = 16_700; // 16.7ms
const BATCH_SIZE: usize = 100;

#[test]
fn test_dot_settlement_under_16ms() {
    println!();
    println!("████████████████████████████████████████████████████████");
    println!("█   DOT SETTLEMENT LATENCY TEST (<16.7ms)             █");
    println!("████████████████████████████████████████████████████████");
    println!();

    let tee = Arc::new(TEEEnclave::new());
    let engine = DOTEngine::new(tee);
    let mut latencies: Vec<u64> = Vec::with_capacity(BATCH_SIZE);

    for i in 0..BATCH_SIZE {
        let tx = DOTTransfer {
            id: Uuid::new_v4(),
            from_user: Uuid::new_v4(),
            to_user: Uuid::new_v4(),
            currency: "USD".to_string(),
            amount: 1000.0 + i as f64,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
            status: DOTStatus::Pending,
            tee_attested: true,
        };

        let start = Instant::now();
        let result = engine.execute_transfer(tx);
        let elapsed_ms = start.elapsed().as_nanos() as u64 / 1_000_000;

        assert!(result.is_ok(), "DOT transfer failed: {:?}", result.err());
        latencies.push(elapsed_ms);
    }

    let max = *latencies.iter().max().unwrap();
    let min = *latencies.iter().min().unwrap();
    let avg = latencies.iter().sum::<u64>() as f64 / latencies.len() as f64;
    let p50 = percentile(&latencies, 50.0);
    let p90 = percentile(&latencies, 90.0);
    let p99 = percentile(&latencies, 99.0);

    println!("  Batch size:        {}", BATCH_SIZE);
    println!("  Target:            < {} ms", DOT_DEADLINE_MS);
    println!();
    println!("  Min:    {:>8} ms", min);
    println!("  P50:    {:>8} ms", p50);
    println!("  P90:    {:>8} ms", p90);
    println!("  P99:    {:>8} ms", p99);
    println!("  Max:    {:>8} ms", max);
    println!("  Avg:    {:>8.2} ms", avg);
    println!();

    assert!(
        max < DOT_DEADLINE_MS,
        "FAIL: Max DOT settlement {max}ms >= {DOT_DEADLINE_MS}ms deadline",
    );
    assert!(
        p99 < DOT_DEADLINE_MS,
        "FAIL: P99 DOT settlement {p99}ms >= {DOT_DEADLINE_MS}ms deadline",
    );

    println!("  ✅ DOT settlement within deadline");
    println!();
}

#[test]
fn test_dot_settlement_high_volume() {
    println!("[DOT-HIGH-VOLUME] Testing 10,000 transfers...");
    let tee = Arc::new(TEEEnclave::new());
    let engine = DOTEngine::new(tee);
    let count = 10_000u64;
    let start = Instant::now();

    for i in 0..count {
        let tx = DOTTransfer {
            id: Uuid::new_v4(),
            from_user: Uuid::new_v4(),
            to_user: Uuid::new_v4(),
            currency: "EGP".to_string(),
            amount: (i % 1000) as f64,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
            status: DOTStatus::Pending,
            tee_attested: true,
        };
        let _ = engine.execute_transfer(tx);
    }

    let elapsed = start.elapsed();
    let avg_per_tx = elapsed.as_nanos() / count as u128;

    println!("  Transfers:         {count}");
    println!("  Total time:        {:.2}ms", elapsed.as_secs_f64() * 1000.0);
    println!("  Avg per transfer:  {avg_per_tx}ns ({:.2}µs)", avg_per_tx as f64 / 1000.0);
    println!();

    assert!(
        elapsed.as_millis() < DOT_TARGET_MS,
        "FAIL: 10,000 DOT transfers took {:.2}s (>{:.1}s target)",
        elapsed.as_secs_f64(),
        DOT_TARGET_MS as f64 / 1000.0
    );
    assert_eq!(
        engine.total_settlements(),
        count,
        "FAIL: settled count mismatch"
    );
    println!("  ✅ 10,000 DOT transfers passed");
    println!();
}

fn percentile(data: &[u64], pct: f64) -> u64 {
    if data.is_empty() {
        return 0;
    }
    let mut data = data.to_vec();
    data.sort();
    let idx = ((pct / 100.0) * data.len() as f64).ceil() as usize;
    data[idx.min(data.len() - 1)]
}
