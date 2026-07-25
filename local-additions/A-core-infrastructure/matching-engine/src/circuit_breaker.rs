use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Maximum price move percentage before trigger (e.g. 10.0 = 10%)
    pub max_move_percent: f64,
    /// Time window in seconds for the price move check
    pub window_secs: u64,
    /// Enable auto-switch to BatchAuction (Level 1)
    pub level1_enabled: bool,
    /// Enable auto-pause trading for this pair (Level 2)
    pub level2_enabled: bool,
    /// Enable auto-activate Kill Shield (Level 3)
    pub level3_enabled: bool,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            max_move_percent: 10.0,
            window_secs: 5,
            level1_enabled: true,
            level2_enabled: true,
            level3_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CircuitState {
    Normal,
    /// Continuous → BatchAuction (slow down)
    Level1,
    /// Trading paused for this pair
    Level2,
    /// Kill Shield activated
    Level3,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Normal => write!(f, "normal"),
            CircuitState::Level1 => write!(f, "level1_batch"),
            CircuitState::Level2 => write!(f, "level2_paused"),
            CircuitState::Level3 => write!(f, "level3_kill"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitEvent {
    pub timestamp_ns: i64,
    pub pair: String,
    pub from_state: String,
    pub to_state: String,
    pub price_move_percent: f64,
    pub trigger_price: f64,
    pub reference_price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CircuitAction {
    SwitchToBatch { window_ns: u64, jitter_range_micros: u64 },
    PauseTrading,
    ActivateKillShield,
}

pub struct CircuitBreaker {
    configs: DashMap<String, CircuitBreakerConfig>,
    price_history: DashMap<String, VecDeque<(Instant, f64)>>,
    state: DashMap<String, CircuitState>,
    events: Arc<Mutex<VecDeque<CircuitEvent>>>,
    total_triggers: AtomicU64,
    max_history: usize,
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self {
            configs: DashMap::new(),
            price_history: DashMap::new(),
            state: DashMap::new(),
            events: Arc::new(Mutex::new(VecDeque::with_capacity(10_000))),
            total_triggers: AtomicU64::new(0),
            max_history: 1000,
        }
    }

    /// Register a trading pair with default config or custom config
    pub fn register_pair(&self, pair: &str) {
        let key = pair.to_uppercase();
        if !self.configs.contains_key(&key) {
            self.configs.insert(key.clone(), CircuitBreakerConfig::default());
            self.state.insert(key.clone(), CircuitState::Normal);
            self.price_history.insert(key, VecDeque::with_capacity(self.max_history));
        }
    }

    pub fn register_pair_with_config(&self, pair: &str, config: CircuitBreakerConfig) {
        let key = pair.to_uppercase();
        self.configs.insert(key.clone(), config);
        self.state.insert(key.clone(), CircuitState::Normal);
        self.price_history.insert(key, VecDeque::with_capacity(self.max_history));
    }

    /// Record a trade price — called from the matching pipeline after each trade
    pub fn record_trade(&self, pair: &str, price: f64) -> Option<CircuitAction> {
        let key = pair.to_uppercase();

        // Skip if market is already paused or killed
        if let Some(s) = self.state.get(&key) {
            if *s == CircuitState::Level2 || *s == CircuitState::Level3 {
                return None;
            }
        }

        // Record price
        if let Some(mut hist) = self.price_history.get_mut(&key) {
            hist.push_back((Instant::now(), price));
            while hist.len() > self.max_history {
                hist.pop_front();
            }
        }

        self.check(&key)
    }

    /// Check if a circuit breaker action is needed based on recent price history
    fn check(&self, key: &str) -> Option<CircuitAction> {
        let config = match self.configs.get(key) {
            Some(c) => c.clone(),
            None => return None,
        };

        let hist = match self.price_history.get(key) {
            Some(h) => h,
            None => return None,
        };

        if hist.len() < 2 {
            return None;
        }

        let now = Instant::now();
        let cutoff = now - Duration::from_secs(config.window_secs);

        // Find earliest price within the window
        let reference_price = match hist.front() {
            Some((t, p)) if *t >= cutoff => *p,
            Some((_, p)) => {
                // Find the first entry within the window
                let mut ref_p = *p;
                for (t, p) in hist.iter() {
                    if *t >= cutoff {
                        ref_p = *p;
                        break;
                    }
                    ref_p = *p;
                }
                ref_p
            }
            None => return None,
        };

        let latest_price = match hist.back() {
            Some((_, p)) => *p,
            None => return None,
        };

        if reference_price == 0.0 {
            return None;
        }

        let move_pct = ((latest_price - reference_price) / reference_price).abs() * 100.0;
        if move_pct < config.max_move_percent {
            return None;
        }

        // Determine current state and escalate
        let current_state = self.state.get(key).map(|s| s.clone()).unwrap_or(CircuitState::Normal);
        let (action, new_state) = match &current_state {
            CircuitState::Normal if config.level1_enabled => {
                (Some(CircuitAction::SwitchToBatch { window_ns: 2000, jitter_range_micros: 200 }), CircuitState::Level1)
            }
            CircuitState::Level1 | CircuitState::Normal if config.level2_enabled => {
                (Some(CircuitAction::PauseTrading), CircuitState::Level2)
            }
            CircuitState::Level2 | CircuitState::Level1 if config.level3_enabled => {
                (Some(CircuitAction::ActivateKillShield), CircuitState::Level3)
            }
            _ => (None, current_state.clone()),
        };

        if action.is_some() {
            self.state.insert(key.to_string(), new_state.clone());
            self.total_triggers.fetch_add(1, Ordering::Relaxed);

            let event = CircuitEvent {
                timestamp_ns: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                pair: key.to_string(),
                from_state: current_state.to_string(),
                to_state: new_state.to_string(),
                price_move_percent: move_pct,
                trigger_price: latest_price,
                reference_price,
            };
            if let Ok(mut evts) = self.events.lock() {
                evts.push_back(event);
                while evts.len() > 10_000 {
                    evts.pop_front();
                }
            }
        }

        action
    }

    /// Trigger Level 1 manually — switch to BatchAuction
    pub fn trigger_level1(&self, pair: &str) -> bool {
        let key = pair.to_uppercase();
        if let Some(mut s) = self.state.get_mut(&key) {
            *s = CircuitState::Level1;
            true
        } else {
            false
        }
    }

    /// Trigger Level 2 manually — pause trading for this pair
    pub fn trigger_level2(&self, pair: &str) -> bool {
        let key = pair.to_uppercase();
        if let Some(mut s) = self.state.get_mut(&key) {
            *s = CircuitState::Level2;
            true
        } else {
            false
        }
    }

    /// Trigger Level 3 manually — activate Kill Shield
    pub fn trigger_level3(&self, pair: &str) -> bool {
        let key = pair.to_uppercase();
        if let Some(mut s) = self.state.get_mut(&key) {
            *s = CircuitState::Level3;
            true
        } else {
            false
        }
    }

    /// Reset a pair back to Normal
    pub fn reset(&self, pair: &str) -> bool {
        let key = pair.to_uppercase();
        if let Some(mut s) = self.state.get_mut(&key) {
            *s = CircuitState::Normal;
            // Clear price history to avoid immediate re-trigger
            if let Some(mut h) = self.price_history.get_mut(&key) {
                h.clear();
            }
            true
        } else {
            false
        }
    }

    /// Check if a pair is paused (Level 2+)
    pub fn is_paused(&self, pair: &str) -> bool {
        let key = pair.to_uppercase();
        match self.state.get(&key) {
            Some(s) => *s == CircuitState::Level2 || *s == CircuitState::Level3,
            None => false,
        }
    }

    /// Get config for a pair
    pub fn get_config(&self, pair: &str) -> Option<CircuitBreakerConfig> {
        self.configs.get(&pair.to_uppercase()).map(|c| c.clone())
    }

    /// Set config for a pair
    pub fn set_config(&self, pair: &str, config: CircuitBreakerConfig) {
        self.configs.insert(pair.to_uppercase(), config);
    }

    /// Get current state for a pair
    pub fn get_state(&self, pair: &str) -> Option<CircuitState> {
        self.state.get(&pair.to_uppercase()).map(|s| s.clone())
    }

    /// Get all pair states
    pub fn all_states(&self) -> Vec<(String, CircuitState, CircuitBreakerConfig)> {
        let mut result = Vec::new();
        for entry in self.state.iter() {
            let key = entry.key().clone();
            let state = entry.value().clone();
            let config = self.configs.get(&key).map(|c| c.clone()).unwrap_or_default();
            result.push((key, state, config));
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    /// Get recent events
    pub fn recent_events(&self, limit: usize) -> Vec<CircuitEvent> {
        match self.events.lock() {
            Ok(evts) => evts.iter().rev().take(limit).cloned().collect(),
            Err(_) => vec![],
        }
    }

    pub fn total_triggers(&self) -> u64 {
        self.total_triggers.load(Ordering::Relaxed)
    }
}

pub const DEFAULT_PAIRS: &[&str] = &["USD/EUR", "USD/EGP", "USD/SAR", "USD/AED", "USD/GBP", "EUR/EGP"];
