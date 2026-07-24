#![allow(dead_code)]
use crossbeam::queue::ArrayQueue;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle, available_parallelism};
use std::time::Instant;

use crate::numa::CPUAffinity;

const PIN_DRAIN_CORE: u32 = 1;
const PIN_WORKER_BASE: u32 = 2;

const RING_CAPACITY: usize = 1 << 21;
const STAGE_CAPACITY: usize = 1 << 18;
const BATCH_MAX: usize = 10_000;
const BATCH_TIME_US: u64 = 100_000;
const BURST_WINDOW: u64 = 50_000;

#[derive(Clone, Debug)]
pub struct TradePayload {
    pub trade_id: u64,
    pub buy_order_id: u64,
    pub sell_order_id: u64,
    pub pair: [u8; 14],
    pub pair_len: u8,
    pub price: u64,
    pub quantity: u64,
    pub total: u64,
    pub buy_user_id: u64,
    pub sell_user_id: u64,
    pub timestamp_ns: i64,
    pub seq: u64,
    pub track: u8,
}

impl TradePayload {
    pub fn pair_str(&self) -> &str {
        let valid = &self.pair[..self.pair_len as usize];
        core::str::from_utf8(valid).unwrap_or("UNKNOWN")
    }
}

struct Slot {
    seq: AtomicU64,
    data: std::cell::UnsafeCell<std::mem::MaybeUninit<TradePayload>>,
}

unsafe impl Send for Slot {}
unsafe impl Sync for Slot {}

pub struct Disruptor {
    slots: Box<[Slot]>,
    capacity: usize,
    mask: usize,
    producer_seq: AtomicU64,
    consumer_gate: AtomicU64,
    commit_cursor: AtomicU64,
}

unsafe impl Send for Disruptor {}
unsafe impl Sync for Disruptor {}

impl Disruptor {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two();
        let slots: Vec<Slot> = (0..cap)
            .map(|_| Slot {
                seq: AtomicU64::new(0),
                data: std::cell::UnsafeCell::new(std::mem::MaybeUninit::uninit()),
            })
            .collect();
        Disruptor {
            slots: slots.into_boxed_slice(),
            capacity: cap,
            mask: cap - 1,
            producer_seq: AtomicU64::new(1),
            consumer_gate: AtomicU64::new(1),
            commit_cursor: AtomicU64::new(0),
        }
    }

    /// Single-producer push. Called only by the drain thread.
    pub fn push_sp(&self, payload: &TradePayload) -> Result<(), ()> {
        let seq = self.producer_seq.fetch_add(1, Ordering::AcqRel);
        let idx = seq as usize & self.mask;
        let slot = &self.slots[idx];
        unsafe {
            let dst = (*slot.data.get()).as_mut_ptr();
            core::ptr::copy_nonoverlapping(payload, dst, 1);
        }
        slot.seq.store(seq, Ordering::Release);
        Ok(())
    }

    /// Multi-consumer safe batch claim.
    pub fn claim_batch(&self, max_size: usize) -> Option<(u64, usize)> {
        let producer = self.producer_seq.load(Ordering::Acquire);
        loop {
            let start = self.consumer_gate.load(Ordering::Acquire);
            if start >= producer {
                return None;
            }
            let available = (producer - start) as usize;
            let batch_size = available.min(max_size);
            let new_gate = start + batch_size as u64;
            match self.consumer_gate.compare_exchange_weak(
                start, new_gate, Ordering::AcqRel, Ordering::Acquire,
            ) {
                Ok(_) => return Some((start, batch_size)),
                Err(_) => continue,
            }
        }
    }

    pub fn read_at(&self, seq: u64) -> TradePayload {
        let idx = seq as usize & self.mask;
        let slot = &self.slots[idx];
        while slot.seq.load(Ordering::Acquire) < seq {
            std::hint::spin_loop();
        }
        unsafe { (*slot.data.get()).as_ptr().read() }
    }

    pub fn commit_cursor(&self) -> u64 { self.commit_cursor.load(Ordering::Acquire) }
    pub fn set_commit_cursor(&self, seq: u64) { self.commit_cursor.store(seq, Ordering::Release); }
    pub fn producer_seq_val(&self) -> u64 { self.producer_seq.load(Ordering::Acquire) }
    pub fn consumed(&self) -> u64 { self.consumer_gate.load(Ordering::Acquire) }
    pub fn pending(&self) -> usize {
        self.producer_seq.load(Ordering::Acquire)
            .saturating_sub(self.consumer_gate.load(Ordering::Acquire)) as usize
    }
}

pub struct AdaptiveBatcher {
    last_flush: Instant,
    batch_time_us: u64,
    batch_max: usize,
    burst_window: u64,
    in_burst: bool,
}

impl AdaptiveBatcher {
    pub fn new() -> Self {
        AdaptiveBatcher {
            last_flush: Instant::now(),
            batch_time_us: BATCH_TIME_US,
            batch_max: BATCH_MAX,
            burst_window: BURST_WINDOW,
            in_burst: false,
        }
    }

    pub fn should_flush(&mut self, pending: usize) -> bool {
        let burst = pending >= self.burst_window as usize;
        self.in_burst = burst;
        if burst { return pending >= self.batch_max; }
        if self.last_flush.elapsed().as_micros() as u64 >= self.batch_time_us {
            self.last_flush = Instant::now();
            return true;
        }
        false
    }

    pub fn is_burst(&self) -> bool { self.in_burst }
}

pub struct Sequencer {
    next_commit: AtomicU64,
}

impl Sequencer {
    pub fn new() -> Self { Sequencer { next_commit: AtomicU64::new(1) } }

    pub fn wait_and_advance(&self, batch_start: u64, batch_end: u64) {
        loop {
            let current = self.next_commit.load(Ordering::Acquire);
            if current == batch_start {
                self.next_commit.store(batch_end, Ordering::Release);
                return;
            }
            std::hint::spin_loop();
        }
    }

    pub fn cursor(&self) -> u64 { self.next_commit.load(Ordering::Acquire) }
}

/// Lock-free multi-producer staging queue + single-producer Disruptor + background workers.
/// Matching engine → push() → ArrayQueue (lock-free MPSC) → drain thread → Disruptor (SP) → workers.
pub struct DualPipeline {
    staging: Arc<ArrayQueue<TradePayload>>,
    disruptor: Arc<Disruptor>,
    batcher: parking_lot::Mutex<AdaptiveBatcher>,
    sequencer: Arc<Sequencer>,
    running: Arc<AtomicU64>,
    worker_count: usize,
}

impl DualPipeline {
    pub fn new(worker_count: usize) -> Self {
        let count = if worker_count == 0 {
            available_parallelism().map(|p| p.get()).unwrap_or(4).saturating_sub(1).max(2)
        } else {
            worker_count
        };
        DualPipeline {
            staging: Arc::new(ArrayQueue::new(STAGE_CAPACITY)),
            disruptor: Arc::new(Disruptor::new(RING_CAPACITY)),
            batcher: parking_lot::Mutex::new(AdaptiveBatcher::new()),
            sequencer: Arc::new(Sequencer::new()),
            running: Arc::new(AtomicU64::new(1)),
            worker_count: count,
        }
    }

    /// Start drain thread + worker threads. Returns all JoinHandles.
    pub fn start(
        &self,
        private_handler: Arc<dyn Fn(&[TradePayload]) -> Result<(), String> + Send + Sync>,
        sovereign_handler: Arc<dyn Fn(&[TradePayload]) -> Result<(), String> + Send + Sync>,
    ) -> Result<Vec<JoinHandle<()>>, String> {
        let mut handles = Vec::new();
        let disruptor = self.disruptor.clone();
        let running = self.running.clone();

        // Drain thread: staging → Disruptor (single producer, pinned to core 1)
        let d = disruptor.clone();
        let r = running.clone();
        let staging_arc = self.staging.clone();
        handles.push(
            thread::Builder::new()
                .name("pipeline-drain".to_string())
                .spawn(move || {
                    let _ = CPUAffinity::pin_to_core(PIN_DRAIN_CORE);
                    while r.load(Ordering::Acquire) != 0 {
                        match staging_arc.pop() {
                            Some(payload) => {
                                let _ = d.push_sp(&payload);
                            }
                            None => {
                                std::thread::yield_now();
                            }
                        }
                    }
                })
                .map_err(|e| format!("pipeline drain thread: {}", e))?,
        );

        // Worker threads: Disruptor → handlers → sequencer (pinned to cores 2+)
        let sequencer = self.sequencer.clone();
        for id in 0..self.worker_count {
            let d = disruptor.clone();
            let s = sequencer.clone();
            let r = running.clone();
            let priv_h = private_handler.clone();
            let sov_h = sovereign_handler.clone();

            handles.push(
                thread::Builder::new()
                    .name(format!("pipeline-wkr-{}", id))
                    .spawn(move || {
                        let _ = CPUAffinity::pin_to_core(PIN_WORKER_BASE + id as u32);
                        let mut batch = Vec::with_capacity(BATCH_MAX);
                        while r.load(Ordering::Acquire) != 0 {
                            let (start, count) = match d.claim_batch(BATCH_MAX) {
                                Some(b) => b,
                                None => { std::thread::yield_now(); continue; }
                            };
                            batch.clear();
                            for i in 0..count {
                                batch.push(d.read_at(start + i as u64));
                            }
                            let batch_end = start + count as u64;
                            let _ = priv_h(&batch);
                            let _ = sov_h(&batch);
                            s.wait_and_advance(start, batch_end);
                            d.set_commit_cursor(batch_end);
                        }
                        })
                        .map_err(|e| format!("pipeline worker {}: {}", id, e))?,
            );
        }
        Ok(handles)
    }

    /// Lock-free multi-producer push. Never blocks matching engine.
    pub fn push(&self, payload: TradePayload) -> Result<(), TradePayload> {
        self.staging.push(payload)
    }

    pub fn shutdown(&self) { self.running.store(0, Ordering::Release); }
    pub fn is_burst(&self) -> bool { self.batcher.lock().is_burst() }
    pub fn disruptor(&self) -> &Arc<Disruptor> { &self.disruptor }
    pub fn worker_count(&self) -> usize { self.worker_count }
}
