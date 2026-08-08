use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VenueKind {
    Cex,
    Dex,
    Aggregator,
    Bridge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueQuote {
    pub venue_id: String,
    pub venue_type: VenueKind,
    pub bid_price: Decimal,
    pub ask_price: Decimal,
    pub bid_size: Decimal,
    pub ask_size: Decimal,
    pub latency_ms: u64,
    pub taker_fee_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolSnapshot {
    pub pool_id: String,
    pub chain: String,
    pub token_a: String,
    pub token_b: String,
    pub reserves_a: Decimal,
    pub reserves_b: Decimal,
    pub fee_tier: u32,
    pub tvl: Decimal,
    pub volume_24h: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainLiquidity {
    pub chain: String,
    pub total_tvl: Decimal,
    pub pools: Vec<PoolSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityFinding {
    pub pool_id: String,
    pub chain: String,
    pub severity: AnomalySeverity,
    pub score: f64,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalySeverity {
    Info,
    Alert,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityReport {
    pub report_id: String,
    pub symbol: String,
    pub target_size: Decimal,
    pub liquidity_score: f64,
    pub available_depth: Decimal,
    pub spread_bps: f64,
    pub imbalance_ratio: f64,
    pub verdict: LiquidityVerdict,
    pub findings: Vec<LiquidityFinding>,
    pub venue_quotes: Vec<VenueQuote>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiquidityVerdict {
    Abundant,
    Adequate,
    Thin,
    Critical,
}

impl LiquidityVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            LiquidityVerdict::Abundant => "abundant",
            LiquidityVerdict::Adequate => "adequate",
            LiquidityVerdict::Thin => "thin",
            LiquidityVerdict::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlippageAnomaly {
    pub venue: String,
    pub symbol: String,
    pub observed_bps: f64,
    pub tolerance_bps: f64,
    pub severity: AnomalySeverity,
    pub projected_price: Decimal,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlippageReport {
    pub scan_id: String,
    pub symbol: String,
    pub breaches: Vec<SlippageAnomaly>,
    pub total_breaches: usize,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeDrivers {
    pub liquidity_score: f64,
    pub slippage_pressure: f64,
    pub market_volatility: f64,
    pub cross_chain_flows: f64,
    pub risk_budget_used: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeSignal {
    pub mode: TradingMode,
    pub confidence: f64,
    pub rationale: String,
    pub drivers: ModeDrivers,
    pub timestamp: DateTime<Utc>,
    pub next_review: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingMode {
    Aggressive,
    Normal,
    Conservative,
    Defensive,
    Halted,
}

impl TradingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradingMode::Aggressive => "aggressive",
            TradingMode::Normal => "normal",
            TradingMode::Conservative => "conservative",
            TradingMode::Defensive => "defensive",
            TradingMode::Halted => "halted",
        }
    }
}

impl std::fmt::Display for TradingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSnapshot {
    pub symbol: String,
    pub venues: Vec<VenueQuote>,
    pub pools: Vec<PoolSnapshot>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    pub mode_review_interval_secs: u64,
    pub slippage_tolerance_bps: f64,
    pub min_liquidity_score: f64,
    pub cross_chain_monitoring: bool,
    pub chains: Vec<String>,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            mode_review_interval_secs: 30,
            slippage_tolerance_bps: 50.0,
            min_liquidity_score: 0.7,
            cross_chain_monitoring: true,
            chains: vec![
                "ethereum".to_string(),
                "polygon".to_string(),
                "arbitrum".to_string(),
                "optimism".to_string(),
            ],
        }
    }
}

pub struct Analyzer {
    config: AnalyzerConfig,
}

impl Analyzer {
    pub fn new(config: AnalyzerConfig) -> Self {
        Self { config }
    }

    pub async fn analyze_liquidity(
        &self,
        symbol: &str,
        target_size: Decimal,
        markets: &[MarketSnapshot],
    ) -> LiquidityReport {
        let report_id = uuid::Uuid::new_v4().to_string();
        let venue_quotes: Vec<VenueQuote> = markets
            .iter()
            .flat_map(|m| m.venues.iter().cloned())
            .collect();

        let liquidity_score = self.calculate_liquidity_score(target_size, &venue_quotes);
        let available_depth = self.calculate_available_depth(target_size, &venue_quotes);
        let spread_bps = self.calculate_avg_spread(&venue_quotes);
        let imbalance = self.calculate_imbalance(&venue_quotes);
        let verdict = self.determine_verdict(liquidity_score);
        let findings = self.detect_findings(markets, liquidity_score);

        LiquidityReport {
            report_id,
            symbol: symbol.to_string(),
            target_size,
            liquidity_score,
            available_depth,
            spread_bps,
            imbalance_ratio: imbalance,
            verdict,
            findings,
            venue_quotes,
            timestamp: Utc::now(),
        }
    }

    pub async fn detect_slippage(
        &self,
        symbol: &str,
        order_size: Decimal,
        markets: &[MarketSnapshot],
        tolerance_bps: f64,
    ) -> SlippageReport {
        let scan_id = uuid::Uuid::new_v4().to_string();
        let mut breaches = Vec::new();

        for market in markets {
            for quote in &market.venues {
                let projected_price = if quote.ask_price > Decimal::ZERO {
                    quote.ask_price
                        + (quote.ask_price * Decimal::from_f64(tolerance_bps / 10000.0).unwrap_or_default())
                } else {
                    quote.ask_price
                };

                let observed_slippage = if !quote.ask_size.is_zero() && quote.ask_size < order_size {
                    ((order_size - quote.ask_size) / quote.ask_size) * Decimal::from(10000)
                } else {
                    Decimal::ZERO
                };

                let observed_bps = observed_slippage.try_into().unwrap_or(0.0);

                if observed_bps > tolerance_bps {
                    let severity = if observed_bps > tolerance_bps * 3.0 {
                        AnomalySeverity::Critical
                    } else if observed_bps > tolerance_bps * 2.0 {
                        AnomalySeverity::Alert
                    } else {
                        AnomalySeverity::Info
                    };

                    breaches.push(SlippageAnomaly {
                        venue: quote.venue_id.clone(),
                        symbol: symbol.to_string(),
                        observed_bps,
                        tolerance_bps,
                        severity,
                        projected_price,
                        timestamp: market.timestamp,
                    });
                }
            }
        }

        SlippageReport {
            scan_id,
            symbol: symbol.to_string(),
            total_breaches: breaches.len(),
            breaches,
            timestamp: Utc::now(),
        }
    }

    pub async fn recommend_mode(
        &self,
        liquidity: &LiquidityReport,
        slippage: &SlippageReport,
    ) -> ModeSignal {
        let liquidity_score = liquidity.liquidity_score;
        let slippage_pressure = slippage.breaches.iter().map(|b| b.observed_bps).sum::<f64>() / 100.0;
        let has_critical = slippage
            .breaches
            .iter()
            .any(|b| b.severity == AnomalySeverity::Critical);

        let mode = if has_critical || liquidity_score < 0.3 {
            TradingMode::Defensive
        } else if liquidity_score < 0.5 || slippage_pressure > 0.3 {
            TradingMode::Conservative
        } else if liquidity_score > 0.8 && slippage_pressure < 0.1 {
            TradingMode::Aggressive
        } else {
            TradingMode::Normal
        };

        let confidence = if has_critical {
            0.9
        } else {
            liquidity_score * 0.5 + (1.0 - slippage_pressure.min(1.0)) * 0.5
        };

        let rationale = if has_critical {
            "Critical slippage detected or severely thin liquidity".to_string()
        } else {
            format!(
                "Liquidity score: {:.2}, Slippage pressure: {:.2}",
                liquidity_score, slippage_pressure
            )
        };

        let now = Utc::now();
        ModeSignal {
            mode,
            confidence,
            rationale,
            drivers: ModeDrivers {
                liquidity_score,
                slippage_pressure,
                market_volatility: 0.0,
                cross_chain_flows: 0.0,
                risk_budget_used: liquidity_score,
            },
            timestamp: now,
            next_review: now + chrono::Duration::seconds(self.config.mode_review_interval_secs as i64),
        }
    }

    fn calculate_liquidity_score(&self, target_size: Decimal, quotes: &[VenueQuote]) -> f64 {
        let total_available: Decimal = quotes.iter().map(|q| q.ask_size).sum();
        if total_available.is_zero() {
            return 0.0_f64;
        }
        let ratio = target_size / total_available;
        let covered: f64 = ratio.max(Decimal::ZERO).try_into().unwrap_or(1.0_f64);
        (1.0_f64 - covered).max(0.0_f64)
    }

    fn calculate_available_depth(&self, target_size: Decimal, quotes: &[VenueQuote]) -> Decimal {
        let total: Decimal = quotes.iter().map(|q| q.ask_size).sum();
        target_size.min(total)
    }

    fn calculate_avg_spread(&self, quotes: &[VenueQuote]) -> f64 {
        let total_spread: f64 = quotes
            .iter()
            .filter(|q| q.bid_price > Decimal::ZERO)
            .map(|q| {
                let bps = ((q.ask_price - q.bid_price) / q.bid_price) * Decimal::from(10000);
                bps.try_into().unwrap_or(0.0)
            })
            .sum();
        if quotes.is_empty() {
            0.0
        } else {
            total_spread / quotes.len() as f64
        }
    }

    fn calculate_imbalance(&self, quotes: &[VenueQuote]) -> f64 {
        let total_bid: Decimal = quotes.iter().map(|q| q.bid_size).sum();
        let total_ask: Decimal = quotes.iter().map(|q| q.ask_size).sum();
        let total = total_bid + total_ask;
        if total.is_zero() {
            0.0
        } else {
            let imbalance = (total_bid - total_ask) / total;
            let val: f64 = imbalance.try_into().unwrap_or(0.0);
            val.abs()
        }
    }

    fn determine_verdict(&self, score: f64) -> LiquidityVerdict {
        match score {
            s if s >= 0.8 => LiquidityVerdict::Abundant,
            s if s >= 0.5 => LiquidityVerdict::Adequate,
            s if s >= 0.3 => LiquidityVerdict::Thin,
            _ => LiquidityVerdict::Critical,
        }
    }

    fn detect_findings(
        &self,
        _markets: &[MarketSnapshot],
        liquidity_score: f64,
    ) -> Vec<LiquidityFinding> {
        let mut findings = Vec::new();
        if liquidity_score < 0.5 {
            findings.push(LiquidityFinding {
                pool_id: "all".to_string(),
                chain: "multi".to_string(),
                severity: if liquidity_score < 0.3 {
                    AnomalySeverity::Critical
                } else {
                    AnomalySeverity::Alert
                },
                score: liquidity_score,
                message: "Low aggregate liquidity across venues".to_string(),
            });
        }
        findings
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AnalyzerError {
    #[error("internal error: {0}")]
    Internal(String),
}
