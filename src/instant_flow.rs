use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum RevenueSource {
    TradingFees,
    FxSpread,
    LendingInterest,
    DarkPoolFees,
    ArbitrageProfit,
    MarketMaking,
    FlashLoanFees,
}

impl RevenueSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            RevenueSource::TradingFees => "trading_fees",
            RevenueSource::FxSpread => "fx_spread",
            RevenueSource::LendingInterest => "lending_interest",
            RevenueSource::DarkPoolFees => "dark_pool_fees",
            RevenueSource::ArbitrageProfit => "arbitrage_profit",
            RevenueSource::MarketMaking => "market_making",
            RevenueSource::FlashLoanFees => "flash_loan_fees",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionRule {
    pub source: RevenueSource,
    pub destination: String,
    pub pct: f64,
    pub min_amount_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstantFlowConfig {
    pub auto_compound_pct: f64,
    pub reserve_target_usd: f64,
    pub reserve_max_pct: f64,
    pub distribution_rules: Vec<DistributionRule>,
}

impl Default for InstantFlowConfig {
    fn default() -> Self {
        Self {
            auto_compound_pct: 0.30,
            reserve_target_usd: 5_000_000.0,
            reserve_max_pct: 0.15,
            distribution_rules: vec![
                DistributionRule {
                    source: RevenueSource::TradingFees,
                    destination: "treasury_mainnet".to_string(),
                    pct: 0.40,
                    min_amount_usd: 100.0,
                },
                DistributionRule {
                    source: RevenueSource::TradingFees,
                    destination: "reserve_fund".to_string(),
                    pct: 0.15,
                    min_amount_usd: 50.0,
                },
                DistributionRule {
                    source: RevenueSource::FxSpread,
                    destination: "treasury_fx".to_string(),
                    pct: 0.50,
                    min_amount_usd: 200.0,
                },
                DistributionRule {
                    source: RevenueSource::LendingInterest,
                    destination: "treasury_lending".to_string(),
                    pct: 0.35,
                    min_amount_usd: 100.0,
                },
                DistributionRule {
                    source: RevenueSource::DarkPoolFees,
                    destination: "treasury_darkpool".to_string(),
                    pct: 0.30,
                    min_amount_usd: 500.0,
                },
                DistributionRule {
                    source: RevenueSource::ArbitrageProfit,
                    destination: "treasury_arb".to_string(),
                    pct: 0.45,
                    min_amount_usd: 50.0,
                },
                DistributionRule {
                    source: RevenueSource::MarketMaking,
                    destination: "treasury_mm".to_string(),
                    pct: 0.40,
                    min_amount_usd: 100.0,
                },
                DistributionRule {
                    source: RevenueSource::FlashLoanFees,
                    destination: "treasury_flash".to_string(),
                    pct: 0.50,
                    min_amount_usd: 25.0,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Distribution {
    pub source: RevenueSource,
    pub destination: String,
    pub amount: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueDashboard {
    pub total_revenue_24h: f64,
    pub by_source: HashMap<String, f64>,
    pub reserve_balance: f64,
    pub pending_distributions: Vec<Distribution>,
    pub compounding_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceBucket {
    pub total: f64,
    pub count: u64,
}

struct InnerState {
    config: InstantFlowConfig,
    buckets: HashMap<RevenueSource, SourceBucket>,
    pending: Vec<Distribution>,
    reserve_balance: f64,
    total_revenue_24h: f64,
    recent_entries: Vec<(RevenueSource, f64, i64)>,
}

pub struct RevenueRouter {
    state: Arc<RwLock<InnerState>>,
}

impl RevenueRouter {
    pub fn new(config: InstantFlowConfig) -> Self {
        let mut buckets = HashMap::new();
        buckets.insert(RevenueSource::TradingFees, SourceBucket { total: 0.0, count: 0 });
        buckets.insert(RevenueSource::FxSpread, SourceBucket { total: 0.0, count: 0 });
        buckets.insert(RevenueSource::LendingInterest, SourceBucket { total: 0.0, count: 0 });
        buckets.insert(RevenueSource::DarkPoolFees, SourceBucket { total: 0.0, count: 0 });
        buckets.insert(RevenueSource::ArbitrageProfit, SourceBucket { total: 0.0, count: 0 });
        buckets.insert(RevenueSource::MarketMaking, SourceBucket { total: 0.0, count: 0 });
        buckets.insert(RevenueSource::FlashLoanFees, SourceBucket { total: 0.0, count: 0 });

        info!(
            auto_compound_pct = config.auto_compound_pct,
            reserve_target = config.reserve_target_usd,
            rule_count = config.distribution_rules.len(),
            "Instant-Flow revenue router initialized"
        );

        Self {
            state: Arc::new(RwLock::new(InnerState {
                config,
                buckets,
                pending: Vec::new(),
                reserve_balance: 0.0,
                total_revenue_24h: 0.0,
                recent_entries: Vec::new(),
            })),
        }
    }

    pub async fn start(self: &Arc<Self>) -> Result<(), String> {
        let router = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                router.purge_old_entries().await;
                router.try_fill_reserve().await;
            }
        });
        info!("Instant-Flow routing loop started (10s tick)");
        Ok(())
    }

    pub async fn record_revenue(&self, source: RevenueSource, amount: f64) {
        let now = chrono::Utc::now().timestamp_millis();

        let mut state = self.state.write().await;

        state.total_revenue_24h += amount;

        if let Some(bucket) = state.buckets.get_mut(&source) {
            bucket.total += amount;
            bucket.count += 1;
        }

        state.recent_entries.push((source.clone(), amount, now));

        let reserve_cap = state.config.reserve_target_usd;
        let reserve_pct = state.config.reserve_max_pct;
        let reserve_share = (amount * reserve_pct).min(reserve_cap - state.reserve_balance).max(0.0);

        if reserve_share > 0.0 {
            state.reserve_balance += reserve_share;
            let distributable = amount - reserve_share;
            self.generate_distributions(&mut state, &source, distributable);
        } else {
            self.generate_distributions(&mut state, &source, amount);
        }

        let compound_amount = amount * state.config.auto_compound_pct;
        info!(
            source = %source.as_str(),
            amount,
            reserve_share,
            compound_amount,
            pending_count = state.pending.len(),
            total_24h = state.total_revenue_24h,
            "revenue recorded"
        );
    }

    fn generate_distributions(&self, state: &mut InnerState, source: &RevenueSource, distributable: f64) {
        let rules: Vec<_> = state.config.distribution_rules.iter()
            .filter(|r| r.source == *source)
            .cloned()
            .collect();

        for rule in rules {
            let amount = distributable * rule.pct;
            if amount >= rule.min_amount_usd {
                state.pending.push(Distribution {
                    source: source.clone(),
                    destination: rule.destination.clone(),
                    amount,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                });
            }
        }

        let compound_share = distributable * state.config.auto_compound_pct;
        if compound_share > 1.0 {
            state.pending.push(Distribution {
                source: source.clone(),
                destination: "auto_compound_vault".to_string(),
                amount: compound_share,
                timestamp: chrono::Utc::now().timestamp_millis(),
            });
        }
    }

    async fn try_fill_reserve(&self) {
        let mut state = self.state.write().await;
        if state.reserve_balance < state.config.reserve_target_usd {
            let deficit = state.config.reserve_target_usd - state.reserve_balance;
            let fill_amount = deficit.min(state.total_revenue_24h * 0.01);
            if fill_amount > 0.0 {
                state.reserve_balance += fill_amount;
                state.pending.push(Distribution {
                    source: RevenueSource::TradingFees,
                    destination: "reserve_fund".to_string(),
                    amount: fill_amount,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                });
            }
        }
    }

    async fn purge_old_entries(&self) {
        let cutoff = chrono::Utc::now().timestamp_millis() - 24 * 3600 * 1000;
        let mut state = self.state.write().await;
        state.recent_entries.retain(|(_, _, ts)| *ts > cutoff);

        state.total_revenue_24h = state.recent_entries.iter()
            .map(|(_, amt, _)| amt)
            .sum();
    }

    pub async fn get_dashboard(&self) -> RevenueDashboard {
        let state = self.state.read().await;

        let mut by_source: HashMap<String, f64> = HashMap::new();
        for (source, bucket) in &state.buckets {
            by_source.insert(source.as_str().to_string(), bucket.total);
        }

        RevenueDashboard {
            total_revenue_24h: state.total_revenue_24h,
            by_source,
            reserve_balance: state.reserve_balance,
            pending_distributions: state.pending.clone(),
            compounding_rate: state.config.auto_compound_pct,
        }
    }

    pub async fn distribute(&self) -> Vec<Distribution> {
        let mut state = self.state.write().await;
        let pending = std::mem::take(&mut state.pending);

        if pending.is_empty() {
            return vec![];
        }

        let total: f64 = pending.iter().map(|d| d.amount).sum();
        info!(
            distribution_count = pending.len(),
            total_amount = total,
            "executing distributions"
        );

        pending
    }

    pub async fn get_config(&self) -> InstantFlowConfig {
        self.state.read().await.config.clone()
    }

    pub async fn update_config(&self, config: InstantFlowConfig) {
        let mut state = self.state.write().await;
        state.config = config;
        info!("Instant-Flow config updated");
    }

    pub async fn get_pending_distributions(&self) -> Vec<Distribution> {
        self.state.read().await.pending.clone()
    }

    pub async fn get_reserve_balance(&self) -> f64 {
        self.state.read().await.reserve_balance
    }

    pub async fn get_source_buckets(&self) -> HashMap<String, SourceBucket> {
        let state = self.state.read().await;
        state.buckets.iter()
            .map(|(k, v)| (k.as_str().to_string(), v.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_revenue() {
        let router = RevenueRouter::new(InstantFlowConfig::default());
        router.record_revenue(RevenueSource::TradingFees, 10_000.0).await;
        let dash = router.get_dashboard().await;
        assert!((dash.total_revenue_24h - 10_000.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_distribute() {
        let router = RevenueRouter::new(InstantFlowConfig::default());
        router.record_revenue(RevenueSource::TradingFees, 50_000.0).await;
        let dists = router.distribute().await;
        assert!(!dists.is_empty());
        let total: f64 = dists.iter().map(|d| d.amount).sum();
        assert!(total > 0.0);
    }

    #[tokio::test]
    async fn test_reserve_accumulates() {
        let router = RevenueRouter::new(InstantFlowConfig::default());
        router.record_revenue(RevenueSource::TradingFees, 100_000.0).await;
        let reserve = router.get_reserve_balance().await;
        assert!(reserve > 0.0);
    }
}
