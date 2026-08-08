use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::bmm_amm::BmmEngine;
use crate::fx_engine::FXEngine;
use crate::revenue_engine::RevenueEngine;
use crate::types::OrderSide;

const DEFAULT_BMM_FEE_BPS: u32 = 50;
const DEFAULT_FX_SPREAD_BPS: u32 = 5;
const DEFAULT_REVENUE_FEE_BPS: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriangularFeeConfig {
    pub bmm_fee_bps: u32,
    pub fx_spread_bps: u32,
    pub revenue_fee_bps: u32,
    pub legs_enabled: [bool; 3],
}

impl Default for TriangularFeeConfig {
    fn default() -> Self {
        Self {
            bmm_fee_bps: DEFAULT_BMM_FEE_BPS,
            fx_spread_bps: DEFAULT_FX_SPREAD_BPS,
            revenue_fee_bps: DEFAULT_REVENUE_FEE_BPS,
            legs_enabled: [true, true, true],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriangularFeeReport {
    pub trade_id: String,
    pub timestamp: u64,
    pub pair: String,
    pub from_ccy: String,
    pub to_ccy: String,
    pub input_amount: f64,
    pub bmm_fee: f64,
    pub fx_spread: f64,
    pub revenue_fee: f64,
    pub total_fees: f64,
    pub net_amount: f64,
    pub legs_enabled: [bool; 3],
    pub is_simulated: [bool; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriangularFeeStats {
    pub total_routes: u64,
    pub live_routes: u64,
    pub simulated_routes: u64,
    pub total_routed_usd: f64,
    pub total_fees_collected_usd: f64,
    pub bmm_fees_usd: f64,
    pub fx_spread_usd: f64,
    pub revenue_fees_usd: f64,
    pub projected_revenue_multiplier: f64,
}

pub struct TriangularFeeNetwork {
    bmm: Arc<BmmEngine>,
    fx: Arc<FXEngine>,
    revenue: Arc<RevenueEngine>,
    config: TriangularFeeConfig,
    total_routes: AtomicU64,
    live_routes: AtomicU64,
    simulated_routes: AtomicU64,
    total_routed_micro: AtomicU64,
    total_fees_micro: AtomicU64,
    bmm_fees_micro: AtomicU64,
    fx_spread_micro: AtomicU64,
    revenue_fees_micro: AtomicU64,
}

impl TriangularFeeNetwork {
    pub fn new(
        bmm: Arc<BmmEngine>,
        fx: Arc<FXEngine>,
        revenue: Arc<RevenueEngine>,
        config: TriangularFeeConfig,
    ) -> Self {
        Self {
            bmm,
            fx,
            revenue,
            config,
            total_routes: AtomicU64::new(0),
            live_routes: AtomicU64::new(0),
            simulated_routes: AtomicU64::new(0),
            total_routed_micro: AtomicU64::new(0),
            total_fees_micro: AtomicU64::new(0),
            bmm_fees_micro: AtomicU64::new(0),
            fx_spread_micro: AtomicU64::new(0),
            revenue_fees_micro: AtomicU64::new(0),
        }
    }

    pub fn config(&self) -> &TriangularFeeConfig {
        &self.config
    }

    fn leg_fee(&self, amount: f64, bps: u32, enabled: bool) -> f64 {
        if enabled {
            amount * bps as f64 / 10_000.0
        } else {
            0.0
        }
    }

    pub fn projected_revenue_multiplier(&self, legs: &[bool]) -> f64 {
        let bps = [self.config.bmm_fee_bps, self.config.fx_spread_bps, self.config.revenue_fee_bps];
        let baseline = bps[0] as f64;
        if baseline <= 0.0 {
            return 1.0;
        }
        let mut enabled_bps = 0.0;
        for (i, enabled) in legs.iter().enumerate() {
            if *enabled {
                if let Some(v) = bps.get(i) {
                    enabled_bps += *v as f64;
                }
            }
        }
        enabled_bps / baseline
    }

    pub fn route_trade_simulated(
        &self,
        input_usd: f64,
        pair: &str,
        from_ccy: &str,
        to_ccy: &str,
    ) -> TriangularFeeReport {
        let cfg = &self.config;
        let bmm_fee = self.leg_fee(input_usd, cfg.bmm_fee_bps, cfg.legs_enabled[0]);
        let fx_spread = self.leg_fee(input_usd, cfg.fx_spread_bps, cfg.legs_enabled[1]);
        let revenue_fee = self.leg_fee(input_usd, cfg.revenue_fee_bps, cfg.legs_enabled[2]);
        let total_fees = bmm_fee + fx_spread + revenue_fee;
        TriangularFeeReport {
            trade_id: Uuid::new_v4().to_string(),
            timestamp: now_ts(),
            pair: pair.to_string(),
            from_ccy: from_ccy.to_string(),
            to_ccy: to_ccy.to_string(),
            input_amount: input_usd,
            bmm_fee,
            fx_spread,
            revenue_fee,
            total_fees,
            net_amount: input_usd - total_fees,
            legs_enabled: cfg.legs_enabled,
            is_simulated: [true, true, true],
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn route_trade_live(
        &self,
        input_usd: f64,
        pair: &str,
        side: OrderSide,
        from_ccy: &str,
        to_ccy: &str,
        user_id: Uuid,
        participant_id: &str,
        counterparty_id: &str,
    ) -> TriangularFeeReport {
        let cfg = &self.config;
        let mut bmm_fee = 0.0;
        let mut fx_spread = 0.0;
        let mut revenue_fee = 0.0;
        let mut sim = [false, false, false];

        if cfg.legs_enabled[0] {
            let quote = self.bmm.get_quote(pair, side, input_usd).await;
            if let Some(q) = quote {
                if self.bmm.execute_swap(pair, side, input_usd, user_id).await.is_some() {
                    bmm_fee = q.fee;
                    sim[0] = false;
                } else {
                    sim[0] = true;
                }
            } else {
                sim[0] = true;
            }
            if sim[0] {
                bmm_fee = self.leg_fee(input_usd, cfg.bmm_fee_bps, true);
            }
        }

        // FX leg collects the real spread via FXEngine (seeded methodology until a live feed is wired).
        if cfg.legs_enabled[1] {
            match self.fx.quote(from_ccy.to_string(), to_ccy.to_string(), input_usd) {
                Ok(q) => {
                    if self.fx.execute(from_ccy.to_string(), to_ccy.to_string(), input_usd).is_ok() {
                        fx_spread = q.fee_cents as f64 / 100.0;
                    } else {
                        sim[1] = true;
                    }
                }
                Err(_) => sim[1] = true,
            }
            if sim[1] {
                fx_spread = self.leg_fee(input_usd, cfg.fx_spread_bps, true);
            }
        }

        if cfg.legs_enabled[2] {
            let side_str = match side {
                OrderSide::Buy => "buy",
                OrderSide::Sell => "sell",
            };
            // calculate_trade_fees records the trade and applies fee/revenue logic.
            let breakdown = self
                .revenue
                .calculate_trade_fees(participant_id, counterparty_id, pair, side_str, input_usd, 1.0, true)
                .await;
            revenue_fee = breakdown.net_revenue_usd;
        }

        let total_fees = bmm_fee + fx_spread + revenue_fee;
        let report = TriangularFeeReport {
            trade_id: Uuid::new_v4().to_string(),
            timestamp: now_ts(),
            pair: pair.to_string(),
            from_ccy: from_ccy.to_string(),
            to_ccy: to_ccy.to_string(),
            input_amount: input_usd,
            bmm_fee,
            fx_spread,
            revenue_fee,
            total_fees,
            net_amount: input_usd - total_fees,
            legs_enabled: cfg.legs_enabled,
            is_simulated: sim,
        };
        let sim_count = sim.iter().filter(|s| **s).count();
        info!(
            "Triangular route: {} {} -> {} via {} | in={:.4} fees={:.4} bmm={:.4} fx={:.4} rev={:.4} simulated_legs={}",
            pair, from_ccy, to_ccy, side_str_of(side), input_usd, total_fees, bmm_fee, fx_spread, revenue_fee, sim_count
        );
        self.record(&report);
        report
    }

    pub fn record(&self, report: &TriangularFeeReport) {
        self.total_routes.fetch_add(1, Ordering::Relaxed);
        let any_enabled_simulated = report
            .legs_enabled
            .iter()
            .zip(report.is_simulated.iter())
            .any(|(enabled, sim)| *enabled && *sim);
        if any_enabled_simulated {
            self.simulated_routes.fetch_add(1, Ordering::Relaxed);
        } else {
            self.live_routes.fetch_add(1, Ordering::Relaxed);
        }
        self.total_routed_micro.fetch_add(to_micro(report.input_amount), Ordering::Relaxed);
        self.total_fees_micro.fetch_add(to_micro(report.total_fees), Ordering::Relaxed);
        self.bmm_fees_micro.fetch_add(to_micro(report.bmm_fee), Ordering::Relaxed);
        self.fx_spread_micro.fetch_add(to_micro(report.fx_spread), Ordering::Relaxed);
        self.revenue_fees_micro.fetch_add(to_micro(report.revenue_fee), Ordering::Relaxed);
    }

    pub fn stats(&self) -> TriangularFeeStats {
        TriangularFeeStats {
            total_routes: self.total_routes.load(Ordering::Relaxed),
            live_routes: self.live_routes.load(Ordering::Relaxed),
            simulated_routes: self.simulated_routes.load(Ordering::Relaxed),
            total_routed_usd: from_micro(self.total_routed_micro.load(Ordering::Relaxed)),
            total_fees_collected_usd: from_micro(self.total_fees_micro.load(Ordering::Relaxed)),
            bmm_fees_usd: from_micro(self.bmm_fees_micro.load(Ordering::Relaxed)),
            fx_spread_usd: from_micro(self.fx_spread_micro.load(Ordering::Relaxed)),
            revenue_fees_usd: from_micro(self.revenue_fees_micro.load(Ordering::Relaxed)),
            projected_revenue_multiplier: self.projected_revenue_multiplier(&self.config.legs_enabled),
        }
    }
}

fn side_str_of(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    }
}

fn to_micro(v: f64) -> u64 {
    (v * 1_000_000.0).round() as u64
}

fn from_micro(m: u64) -> f64 {
    m as f64 / 1_000_000.0
}

fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bmm_amm::BmmConfig;
    use crate::revenue_engine::RevenueConfig;

    fn network_with(config: TriangularFeeConfig) -> TriangularFeeNetwork {
        let bmm = Arc::new(BmmEngine::new(BmmConfig::default()));
        let fx = Arc::new(FXEngine::new(5));
        let revenue = Arc::new(RevenueEngine::new(RevenueConfig::default()));
        TriangularFeeNetwork::new(bmm, fx, revenue, config)
    }

    fn default_network() -> TriangularFeeNetwork {
        network_with(TriangularFeeConfig::default())
    }

    #[test]
    fn simulated_route_computes_fees() {
        let net = default_network();
        let rep = net.route_trade_simulated(1_000_000.0, "EUR/USD", "USD", "EUR");
        assert!((rep.bmm_fee - 5000.0).abs() < 0.001);
        assert!((rep.fx_spread - 500.0).abs() < 0.001);
        assert!((rep.revenue_fee - 500.0).abs() < 0.001);
        assert!((rep.total_fees - 6000.0).abs() < 0.001);
        assert!((rep.net_amount - 994_000.0).abs() < 0.001);
        assert_eq!(rep.legs_enabled, [true, true, true]);
        assert_eq!(rep.is_simulated, [true, true, true]);
    }

    #[test]
    fn disabled_legs_charge_zero() {
        let net = network_with(TriangularFeeConfig {
            legs_enabled: [true, false, false],
            ..TriangularFeeConfig::default()
        });
        let rep = net.route_trade_simulated(100_000.0, "BTC/USDT", "USD", "BTC");
        assert!((rep.bmm_fee - 500.0).abs() < 1e-9);
        assert!((rep.fx_spread - 0.0).abs() < 1e-9);
        assert!((rep.revenue_fee - 0.0).abs() < 1e-9);
        assert!((rep.total_fees - 500.0).abs() < 1e-9);
    }

    #[test]
    fn revenue_multiplier() {
        let net = default_network();
        assert_eq!(net.projected_revenue_multiplier(&[true, false, false]), 1.0);
        let m = net.projected_revenue_multiplier(&[true, true, true]);
        assert!(m > 1.0);
        assert!((m - 1.2).abs() < 1e-9);

        let all5 = network_with(TriangularFeeConfig {
            bmm_fee_bps: 5,
            fx_spread_bps: 5,
            revenue_fee_bps: 5,
            legs_enabled: [true, true, true],
        });
        let m3 = all5.projected_revenue_multiplier(&[true, true, true]);
        assert!(m3 > 1.0);
        assert!((m3 - 3.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn live_route_with_engines() {
        let mut bcfg = BmmConfig::default();
        bcfg.initial_liquidity_usd = 10_000_000_000.0;
        let bmm = Arc::new(BmmEngine::new(bcfg));
        bmm.start().await.unwrap();
        let fx = Arc::new(FXEngine::new(5));
        let revenue = Arc::new(RevenueEngine::new(RevenueConfig::default()));
        let net = TriangularFeeNetwork::new(bmm, fx, revenue, TriangularFeeConfig::default());
        let rep = net
            .route_trade_live(
                1_000_000.0,
                "BTC/USDT",
                OrderSide::Buy,
                "USD",
                "EUR",
                Uuid::new_v4(),
                "participant_1",
                "counterparty_1",
            )
            .await;
        assert!(!rep.is_simulated[0]);
        assert!(rep.total_fees > 0.0);
        assert!(rep.bmm_fee > 0.0);
        assert!(rep.fx_spread > 0.0);
        assert!(rep.revenue_fee > 0.0);
    }

    #[tokio::test]
    async fn stats_counters_increment() {
        let net = default_network();
        let rep = net.route_trade_simulated(10_000.0, "BTC/USDT", "USD", "BTC");
        net.record(&rep);
        net.record(&rep);
        let s = net.stats();
        assert_eq!(s.total_routes, 2);
        assert!((s.total_routed_usd - 20_000.0).abs() < 0.01);
        assert!(s.total_fees_collected_usd > 0.0);
        assert!(s.bmm_fees_usd > 0.0);
        assert_eq!(s.simulated_routes, 2);
        assert_eq!(s.live_routes, 0);
    }
}