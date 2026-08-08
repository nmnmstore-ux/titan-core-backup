use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VampireConfig {
    pub reinvest_pct: f64,
    pub strategy_allocations: HashMap<String, f64>,
    pub hunger_threshold_volatility: f64,
    pub compound_interval_secs: u64,
    pub min_profit_to_absorb: f64,
}

impl Default for VampireConfig {
    fn default() -> Self {
        let mut strategy_allocations = HashMap::new();
        strategy_allocations.insert("arbitrage".to_string(), 0.30);
        strategy_allocations.insert("market_making".to_string(), 0.25);
        strategy_allocations.insert("lending".to_string(), 0.25);
        strategy_allocations.insert("dark_pool".to_string(), 0.20);

        Self {
            reinvest_pct: 0.70,
            strategy_allocations,
            hunger_threshold_volatility: 0.05,
            compound_interval_secs: 60,
            min_profit_to_absorb: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reinvestment {
    pub strategy: String,
    pub amount: f64,
    pub expected_roi: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VampireStatus {
    pub total_absorbed: f64,
    pub total_reinvested: f64,
    pub active_strategies: usize,
    pub hunger_mode: bool,
    pub current_treasury: f64,
    pub roi_by_strategy: HashMap<String, f64>,
    pub uptime_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Treasury {
    pub balance: f64,
    pub growth_rate_24h: f64,
    pub growth_rate_7d: f64,
    pub projected_30d: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyMetrics {
    pub name: String,
    pub total_invested: f64,
    pub total_returned: f64,
    pub roi: f64,
    pub investment_count: u64,
    pub last_invested_at: i64,
}

struct Inner {
    config: VampireConfig,
    treasury_balance: f64,
    total_absorbed: f64,
    total_reinvested: f64,
    hunger_mode: bool,
    strategy_metrics: HashMap<String, StrategyMetrics>,
    absorption_history: Vec<(i64, f64, String)>,
    reinvestment_history: Vec<Reinvestment>,
    start_time: Instant,
    compound_interval_secs: u64,
}

pub struct VampireCore {
    inner: Arc<RwLock<Inner>>,
}

impl VampireCore {
    pub fn new(config: VampireConfig) -> Self {
        let interval = config.compound_interval_secs;
        let mut strategy_metrics = HashMap::new();
        for name in config.strategy_allocations.keys() {
            strategy_metrics.insert(
                name.clone(),
                StrategyMetrics {
                    name: name.clone(),
                    total_invested: 0.0,
                    total_returned: 0.0,
                    roi: 0.0,
                    investment_count: 0,
                    last_invested_at: 0,
                },
            );
        }

        info!(
            reinvest_pct = config.reinvest_pct,
            strategies = config.strategy_allocations.len(),
            interval_secs = interval,
            "VampireCore initialized"
        );

        Self {
            inner: Arc::new(RwLock::new(Inner {
                config,
                treasury_balance: 0.0,
                total_absorbed: 0.0,
                total_reinvested: 0.0,
                hunger_mode: false,
                strategy_metrics,
                absorption_history: Vec::new(),
                reinvestment_history: Vec::new(),
                start_time: Instant::now(),
                compound_interval_secs: interval,
            })),
        }
    }

    pub async fn start(self: &Arc<Self>) -> Result<(), String> {
        let core = self.clone();
        let interval = {
            let inner = core.inner.read().await;
            inner.compound_interval_secs
        };

        info!(interval_secs = interval, "VampireCore: starting reinvestment loop");

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(interval)).await;

                let reinvestments = core.reinvest().await;
                if !reinvestments.is_empty() {
                    info!(count = reinvestments.len(), "VampireCore: reinvestment cycle completed");
                }

                core.maybe_update_hunger_mode().await;
            }
        });

        info!("VampireCore: background loop spawned");
        Ok(())
    }

    pub async fn absorb_profit(&self, source: &str, amount: f64) {
        let mut inner = self.inner.write().await;

        if amount < inner.config.min_profit_to_absorb {
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let reinvestable = amount * inner.config.reinvest_pct;
        let to_treasury = amount - reinvestable;

        inner.total_absorbed += amount;
        inner.treasury_balance += to_treasury;
        inner.absorption_history.push((now, amount, source.to_string()));

        if inner.absorption_history.len() > 10000 {
            let drain_count = inner.absorption_history.len() - 10000;
            inner.absorption_history.drain(..drain_count);
        }

        info!(
            source = source,
            amount = amount,
            reinvestable = reinvestable,
            treasury = to_treasury,
            total_absorbed = inner.total_absorbed,
            treasury_balance = inner.treasury_balance,
            "VampireCore: profit absorbed"
        );
    }

    pub async fn reinvest(&self) -> Vec<Reinvestment> {
        let mut inner = self.inner.write().await;
        let mut reinvestments = Vec::new();

        let available = inner.total_absorbed * inner.config.reinvest_pct;
        if available <= 0.0 {
            return reinvestments;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let alloc_total: f64 = inner.config.strategy_allocations.values().sum();
        if alloc_total <= 0.0 {
            return reinvestments;
        }

        let allocations: Vec<(String, f64)> = inner.config.strategy_allocations.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();

        for (strategy, allocation) in allocations {
            let normalized_alloc = allocation / alloc_total;
            let amount = available * normalized_alloc;

            if amount <= 0.0 {
                continue;
            }

            let expected_roi = inner
                .strategy_metrics
                .get(&strategy)
                .map(|m| {
                    if m.investment_count > 0 {
                        m.roi + 0.01
                    } else {
                        0.05
                    }
                })
                .unwrap_or(0.05);

            let reinvestment = Reinvestment {
                strategy: strategy.clone(),
                amount,
                expected_roi,
                timestamp: now,
            };

            if let Some(metrics) = inner.strategy_metrics.get_mut(&strategy) {
                metrics.total_invested += amount;
                metrics.investment_count += 1;
                metrics.last_invested_at = now;

                let simulated_return = amount * (1.0 + expected_roi * 0.01);
                metrics.total_returned += simulated_return;
                if metrics.total_invested > 0.0 {
                    metrics.roi =
                        ((metrics.total_returned - metrics.total_invested) / metrics.total_invested)
                            * 100.0;
                }
            }

            inner.total_reinvested += amount;
            inner.reinvestment_history.push(reinvestment.clone());

            if inner.reinvestment_history.len() > 10000 {
                let drain_count = inner.reinvestment_history.len() - 10000;
                inner.reinvestment_history.drain(..drain_count);
            }

            reinvestments.push(reinvestment);
        }

        reinvestments
    }

    pub async fn get_status(&self) -> VampireStatus {
        let inner = self.inner.read().await;

        let roi_by_strategy: HashMap<String, f64> = inner
            .strategy_metrics
            .iter()
            .map(|(name, m)| (name.clone(), m.roi))
            .collect();

        VampireStatus {
            total_absorbed: inner.total_absorbed,
            total_reinvested: inner.total_reinvested,
            active_strategies: inner.strategy_metrics.len(),
            hunger_mode: inner.hunger_mode,
            current_treasury: inner.treasury_balance,
            roi_by_strategy,
            uptime_secs: inner.start_time.elapsed().as_secs(),
        }
    }

    pub async fn get_treasury(&self) -> Treasury {
        let inner = self.inner.read().await;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let day_ago = now - 86400;
        let week_ago = now - 604800;

        let recent_sum: f64 = inner
            .absorption_history
            .iter()
            .filter(|(ts, _, _)| *ts > day_ago * 1000)
            .map(|(_, amt, _)| *amt)
            .sum();

        let weekly_sum: f64 = inner
            .absorption_history
            .iter()
            .filter(|(ts, _, _)| *ts > week_ago * 1000)
            .map(|(_, amt, _)| *amt)
            .sum();

        let growth_rate_24h = if inner.treasury_balance > 0.0 {
            recent_sum / inner.treasury_balance
        } else {
            0.0
        };

        let growth_rate_7d = if inner.treasury_balance > 0.0 {
            weekly_sum / inner.treasury_balance
        } else {
            0.0
        };

        let avg_daily_growth = if inner.absorption_history.len() > 1 {
            let total_days = inner.start_time.elapsed().as_secs().max(1) as f64 / 86400.0;
            inner.total_absorbed / total_days
        } else {
            0.0
        };

        let projected_30d = inner.treasury_balance + (avg_daily_growth * 30.0);

        Treasury {
            balance: inner.treasury_balance,
            growth_rate_24h,
            growth_rate_7d,
            projected_30d,
        }
    }

    async fn maybe_update_hunger_mode(&self) {
        let mut inner = self.inner.write().await;

        let recent_count = {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;

            let window = 300_000;
            inner
                .absorption_history
                .iter()
                .filter(|(ts, _, _)| now - *ts < window)
                .count()
        };

        let was_hunger = inner.hunger_mode;
        inner.hunger_mode = recent_count as f64 > inner.config.hunger_threshold_volatility * 100.0;

        if inner.hunger_mode != was_hunger {
            if inner.hunger_mode {
                info!(
                    recent_absorptions = recent_count,
                    threshold = inner.config.hunger_threshold_volatility,
                    "VampireCore: HUNGER MODE ACTIVATED"
                );
            } else {
                info!("VampireCore: hunger mode deactivated");
            }
        }
    }

    pub async fn update_config(&self, config: VampireConfig) {
        let mut inner = self.inner.write().await;
        info!(
            reinvest_pct = config.reinvest_pct,
            interval = config.compound_interval_secs,
            "VampireCore: config updated"
        );
        inner.compound_interval_secs = config.compound_interval_secs;
        inner.config = config;
    }

    pub async fn get_config(&self) -> VampireConfig {
        let inner = self.inner.read().await;
        inner.config.clone()
    }
}
