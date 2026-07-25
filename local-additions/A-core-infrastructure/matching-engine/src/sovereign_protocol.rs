#![allow(dead_code)]
use crate::pipeline::TradePayload;
use dashmap::{DashMap, DashSet};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Sovereign Protocol — Ghost layer for absolute control over the financial flow.
///
/// Three pillars:
/// 1. Sovereign Transaction Levy — silent protocol tax on every trade
/// 2. Prohibited Address Detection — block sanctioned/illicit addresses at match level
/// 3. Sleeper Agent Protocol — stealth monitor + freeze/seize capability
pub struct SovereignProtocol {
    // === Tax ===
    tax_rate_bps: AtomicU64,
    treasury: Arc<DashMap<String, AtomicU64>>,
    tax_collected_total: AtomicU64,
    tax_collected_by_asset: Arc<DashMap<String, u64>>,

    // === Prohibited ===
    prohibited: Arc<DashSet<String>>,
    prohibited_blocked: AtomicU64,
    prohibited_log: Arc<DashMap<String, u64>>,

    // === Sleeper ===
    sleepers: Arc<DashMap<String, SleeperState>>,
    sleeper_trades_intercepted: AtomicU64,
    sleeper_actions_taken: AtomicU64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleeperState {
    pub label: String,
    pub added_at: i64,
    pub status: SleeperStatus,
    pub total_volume: u64,
    pub trade_count: u64,
    pub last_seen_ns: i64,
    pub action: Option<SleeperAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SleeperStatus {
    Watching,
    Frozen,
    Seized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleeperAction {
    pub action_type: SleeperActionType,
    pub triggered_at: i64,
    pub amount_seized: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SleeperActionType {
    Freeze,
    Seize,
    Tax,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptedTrade {
    pub trade_id: u64,
    pub timestamp_ns: i64,
    pub pair: String,
    pub total: u64,
    pub counterparty: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SovereignProtocolSnapshot {
    pub tax_rate_bps: u64,
    pub treasury: Vec<(String, u64)>,
    pub tax_collected_total: u64,
    pub prohibited_count: usize,
    pub prohibited_blocked: u64,
    pub sleeper_count: usize,
    pub sleeper_trades_intercepted: u64,
    pub sleeper_actions_taken: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleeperDetail {
    pub address: String,
    pub state: SleeperState,
}

impl SovereignProtocol {
    pub fn new() -> Self {
        Self {
            tax_rate_bps: AtomicU64::new(0),
            treasury: Arc::new(DashMap::new()),
            tax_collected_total: AtomicU64::new(0),
            tax_collected_by_asset: Arc::new(DashMap::new()),
            prohibited: Arc::new(DashSet::new()),
            prohibited_blocked: AtomicU64::new(0),
            prohibited_log: Arc::new(DashMap::new()),
            sleepers: Arc::new(DashMap::new()),
            sleeper_trades_intercepted: AtomicU64::new(0),
            sleeper_actions_taken: AtomicU64::new(0),
        }
    }

    /// Process a batch of trades from the pipeline. Called on every batch.
    /// Returns the number of trades blocked (prohibited) and the total tax levied.
    pub fn process_batch(&self, batch: &[TradePayload]) -> ProcessBatchResult {
        let mut blocked = 0u64;
        let mut taxed_total = 0u64;

        // Extract buy/sell user IDs as addresses
        for trade in batch {
            let buy_addr = format!("user:{}", trade.buy_user_id);
            let sell_addr = format!("user:{}", trade.sell_user_id);

            // === Pillar 2: Prohibited Address Detection ===
            if self.is_prohibited(&buy_addr) || self.is_prohibited(&sell_addr) {
                blocked += 1;
                self.prohibited_blocked.fetch_add(1, Ordering::Relaxed);
                self.prohibited_log
                    .entry(trade.pair_str().to_string())
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
                continue;
            }

            // === Pillar 1: Sovereign Transaction Levy ===
            let rate = self.tax_rate_bps.load(Ordering::Relaxed);
            if rate > 0 {
                let tax = (trade.total * rate) / 10_000;
                if tax > 0 {
                    let asset = self.asset_from_pair(trade.pair_str());
                    self.treasury
                        .entry(asset.clone())
                        .or_insert_with(|| AtomicU64::new(0))
                        .fetch_add(tax, Ordering::Relaxed);
                    self.tax_collected_by_asset
                        .entry(asset)
                        .and_modify(|t| *t += tax)
                        .or_insert(tax);
                    self.tax_collected_total.fetch_add(tax, Ordering::Relaxed);
                    taxed_total += tax;
                }
            }

            // === Pillar 3: Sleeper Agent ===
            self.check_sleeper(trade, &buy_addr);
            self.check_sleeper(trade, &sell_addr);

            // If the buy side is frozen, block the trade
            if let Some(s) = self.sleepers.get(&buy_addr) {
                if s.status == SleeperStatus::Frozen || s.status == SleeperStatus::Seized {
                    blocked += 1;
                    continue;
                }
            }
            if let Some(s) = self.sleepers.get(&sell_addr) {
                if s.status == SleeperStatus::Frozen || s.status == SleeperStatus::Seized {
                    blocked += 1;
                    continue;
                }
            }
        }

        ProcessBatchResult {
            blocked,
            tax_collected: taxed_total,
            trades_processed: batch.len() as u64 - blocked,
        }
    }

    fn is_prohibited(&self, addr: &str) -> bool {
        self.prohibited.contains(addr)
    }

    fn check_sleeper(&self, trade: &TradePayload, addr: &str) {
        if let Some(mut s) = self.sleepers.get_mut(addr) {
            s.total_volume += trade.total;
            s.trade_count += 1;
            if trade.timestamp_ns > s.last_seen_ns {
                s.last_seen_ns = trade.timestamp_ns;
            }
            self.sleeper_trades_intercepted.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn asset_from_pair(&self, pair: &str) -> String {
        if let Some(idx) = pair.find('/') {
            pair[idx + 1..].to_string()
        } else {
            "UNKNOWN".to_string()
        }
    }

    // ===== Tax Controls =====

    pub fn set_tax_rate(&self, rate_bps: u64) {
        self.tax_rate_bps.store(rate_bps, Ordering::Release);
    }

    pub fn tax_rate(&self) -> u64 {
        self.tax_rate_bps.load(Ordering::Acquire)
    }

    pub fn treasury_balance(&self) -> Vec<(String, u64)> {
        self.tax_collected_by_asset
            .iter()
            .map(|e| (e.key().clone(), *e.value()))
            .collect()
    }

    pub fn tax_collected_total(&self) -> u64 {
        self.tax_collected_total.load(Ordering::Relaxed)
    }

    // ===== Prohibited Address Controls =====

    pub fn add_prohibited(&self, addr: &str) {
        self.prohibited.insert(addr.to_string());
    }

    pub fn remove_prohibited(&self, addr: &str) -> bool {
        self.prohibited.remove(addr).is_some()
    }

    pub fn list_prohibited(&self) -> Vec<(String, u64)> {
        let mut result: Vec<(String, u64)> = self
            .prohibited
            .iter()
            .map(|e| {
                let addr = e.key().clone();
                let count = self.prohibited_log.get(&addr).map(|c| *c).unwrap_or(0);
                (addr, count)
            })
            .collect();
        result.sort_by(|a, b| b.1.cmp(&a.1));
        result
    }

    pub fn is_prohibited_addr(&self, addr: &str) -> bool {
        self.prohibited.contains(addr)
    }

    pub fn prohibited_blocked_count(&self) -> u64 {
        self.prohibited_blocked.load(Ordering::Relaxed)
    }

    // ===== Sleeper Agent Controls =====

    pub fn watch_sleeper(&self, addr: &str, label: &str) {
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.sleepers.insert(
            addr.to_string(),
            SleeperState {
                label: label.to_string(),
                added_at: now,
                status: SleeperStatus::Watching,
                total_volume: 0,
                trade_count: 0,
                last_seen_ns: 0,
                action: None,
            },
        );
    }

    pub fn unwatch_sleeper(&self, addr: &str) -> bool {
        self.sleepers.remove(addr).is_some()
    }

    pub fn freeze_sleeper(&self, addr: &str) -> Result<(), String> {
        let mut s = self
            .sleepers
            .get_mut(addr)
            .ok_or_else(|| format!("sleeper not found: {}", addr))?;
        s.status = SleeperStatus::Frozen;
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        s.action = Some(SleeperAction {
            action_type: SleeperActionType::Freeze,
            triggered_at: now,
            amount_seized: 0,
        });
        self.sleeper_actions_taken.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn seize_sleeper(&self, addr: &str) -> Result<SleeperAction, String> {
        let mut s = self
            .sleepers
            .get_mut(addr)
            .ok_or_else(|| format!("sleeper not found: {}", addr))?;
        let amount = s.total_volume;
        s.status = SleeperStatus::Seized;
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let action = SleeperAction {
            action_type: SleeperActionType::Seize,
            triggered_at: now,
            amount_seized: amount,
        };
        s.action = Some(action.clone());
        // Transfer seized amount to sovereign treasury
        self.tax_collected_by_asset
            .entry("SEIZED".to_string())
            .and_modify(|t| *t += amount)
            .or_insert(amount);
        self.tax_collected_total.fetch_add(amount, Ordering::Relaxed);
        self.sleeper_actions_taken.fetch_add(1, Ordering::Relaxed);
        Ok(action)
    }

    pub fn one_time_tax_sleeper(&self, addr: &str, amount: u64) -> Result<(), String> {
        let mut s = self
            .sleepers
            .get_mut(addr)
            .ok_or_else(|| format!("sleeper not found: {}", addr))?;
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        s.action = Some(SleeperAction {
            action_type: SleeperActionType::Tax,
            triggered_at: now,
            amount_seized: amount,
        });
        self.tax_collected_by_asset
            .entry("SLEEPER_TAX".to_string())
            .and_modify(|t| *t += amount)
            .or_insert(amount);
        self.tax_collected_total.fetch_add(amount, Ordering::Relaxed);
        self.sleeper_actions_taken.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn list_sleepers(&self) -> Vec<SleeperDetail> {
        let mut result: Vec<SleeperDetail> = self
            .sleepers
            .iter()
            .map(|e| SleeperDetail {
                address: e.key().clone(),
                state: e.value().clone(),
            })
            .collect();
        result.sort_by(|a, b| b.state.trade_count.cmp(&a.state.trade_count));
        result
    }

    pub fn sleeper_stats(&self) -> (u64, u64, u64) {
        (
            self.sleeper_trades_intercepted.load(Ordering::Relaxed),
            self.sleeper_actions_taken.load(Ordering::Relaxed),
            self.sleepers.len() as u64,
        )
    }

    // ===== Snapshot =====

    pub fn snapshot(&self) -> SovereignProtocolSnapshot {
        SovereignProtocolSnapshot {
            tax_rate_bps: self.tax_rate(),
            treasury: self.treasury_balance(),
            tax_collected_total: self.tax_collected_total(),
            prohibited_count: self.prohibited.len(),
            prohibited_blocked: self.prohibited_blocked_count(),
            sleeper_count: self.sleepers.len(),
            sleeper_trades_intercepted: self.sleeper_trades_intercepted.load(Ordering::Relaxed),
            sleeper_actions_taken: self.sleeper_actions_taken.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessBatchResult {
    pub blocked: u64,
    pub tax_collected: u64,
    pub trades_processed: u64,
}
