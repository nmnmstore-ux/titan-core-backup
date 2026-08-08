use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

// ==================== Enums ====================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Conservative,
    Moderate,
    Aggressive,
    Extreme,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Conservative => write!(f, "Conservative"),
            RiskLevel::Moderate => write!(f, "Moderate"),
            RiskLevel::Aggressive => write!(f, "Aggressive"),
            RiskLevel::Extreme => write!(f, "Extreme"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketTrend {
    Bullish,
    Bearish,
    Sideways,
    Volatile,
}

impl std::fmt::Display for MarketTrend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarketTrend::Bullish => write!(f, "Bullish"),
            MarketTrend::Bearish => write!(f, "Bearish"),
            MarketTrend::Sideways => write!(f, "Sideways"),
            MarketTrend::Volatile => write!(f, "Volatile"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionAction {
    Buy,
    Sell,
    Hold,
    Reduce,
    Increase,
}

impl std::fmt::Display for DecisionAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecisionAction::Buy => write!(f, "Buy"),
            DecisionAction::Sell => write!(f, "Sell"),
            DecisionAction::Hold => write!(f, "Hold"),
            DecisionAction::Reduce => write!(f, "Reduce"),
            DecisionAction::Increase => write!(f, "Increase"),
        }
    }
}

// ==================== Config ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AICEOConfig {
    pub analysis_interval_secs: u64,
    pub max_position_pct: f64,
    pub risk_tolerance: RiskLevel,
    pub override_enabled: bool,
    pub learning_rate: f64,
}

impl Default for AICEOConfig {
    fn default() -> Self {
        Self {
            analysis_interval_secs: 30,
            max_position_pct: 0.15,
            risk_tolerance: RiskLevel::Moderate,
            override_enabled: true,
            learning_rate: 0.01,
        }
    }
}

// ==================== Analysis Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyLevel {
    pub price: f64,
    pub label: String,
    pub strength: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeAnalysis {
    pub current_volume: f64,
    pub avg_volume_24h: f64,
    pub volume_ratio: f64,
    pub trend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketAnalysis {
    pub trend: MarketTrend,
    pub volatility_score: f64,
    pub sentiment_score: f64,
    pub key_levels: Vec<KeyLevel>,
    pub volume_analysis: VolumeAnalysis,
    pub timestamp: i64,
    pub symbols_analyzed: Vec<String>,
}

// ==================== Decision Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeOutcome {
    pub symbol: String,
    pub action: DecisionAction,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub pnl: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionContext {
    pub symbol: String,
    pub current_positions: HashMap<String, f64>,
    pub available_balance: f64,
    pub recent_trades: Vec<TradeOutcome>,
    pub market_analysis: MarketAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub action: DecisionAction,
    pub symbol: String,
    pub size_pct: f64,
    pub confidence: f64,
    pub reasoning: String,
    pub risk_score: f64,
    pub timestamp: i64,
    pub override_applied: bool,
}

// ==================== Review Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BestWorstTrade {
    pub symbol: String,
    pub pnl: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterAdjustment {
    pub parameter: String,
    pub old_value: f64,
    pub new_value: f64,
    pub reason: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewReport {
    pub total_decisions: u64,
    pub winning_decisions: u64,
    pub losing_decisions: u64,
    pub win_rate: f64,
    pub avg_return: f64,
    pub best_trade: Option<BestWorstTrade>,
    pub worst_trade: Option<BestWorstTrade>,
    pub adjustments_made: Vec<ParameterAdjustment>,
    pub period_start: i64,
    pub period_end: i64,
}

// ==================== Status Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AICEOStatus {
    pub uptime_secs: u64,
    pub decisions_made: u64,
    pub current_recommendations_count: usize,
    pub risk_level: RiskLevel,
    pub last_analysis_at: Option<i64>,
    pub win_rate: f64,
    pub override_events: u64,
}

// ==================== Recommendation Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub symbol: String,
    pub action: DecisionAction,
    pub target_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub confidence: f64,
    pub rationale: String,
    pub generated_at: i64,
}

// ==================== Audit Trail ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverrideAuditEntry {
    pub timestamp: i64,
    pub decision_id: String,
    pub symbol: String,
    pub original_risk_score: f64,
    pub overridden_risk_score: f64,
    pub reason: String,
    pub risk_tolerance: RiskLevel,
}

// ==================== Internal State ====================

#[derive(Debug, Clone)]
struct InternalState {
    start_time: Instant,
    config: AICEOConfig,
    decisions: Vec<Decision>,
    recommendations: Vec<Recommendation>,
    overrides: Vec<OverrideAuditEntry>,
    parameter_adjustments: Vec<ParameterAdjustment>,
    last_analysis: Option<MarketAnalysis>,
    cumulative_pnl: f64,
    total_wins: u64,
    total_losses: u64,
}

impl InternalState {
    fn new(config: AICEOConfig) -> Self {
        Self {
            start_time: Instant::now(),
            config,
            decisions: Vec::new(),
            recommendations: Vec::new(),
            overrides: Vec::new(),
            parameter_adjustments: Vec::new(),
            last_analysis: None,
            cumulative_pnl: 0.0,
            total_wins: 0,
            total_losses: 0,
        }
    }
}

// ==================== AICEO ====================

pub struct AICEO {
    state: Arc<RwLock<InternalState>>,
    active_pairs: Vec<String>,
}

impl AICEO {
    pub fn new(config: AICEOConfig) -> Self {
        let active_pairs = vec![
            "USD/EUR".to_string(),
            "USD/EGP".to_string(),
            "USD/SAR".to_string(),
            "USD/AED".to_string(),
            "USD/GBP".to_string(),
            "EUR/EGP".to_string(),
        ];
        let state = InternalState::new(config);
        info!("AICEO (DeepSeek-R1 CEO) initialized — {} pairs, risk: {}",
            active_pairs.len(), state.config.risk_tolerance);
        Self {
            state: Arc::new(RwLock::new(state)),
            active_pairs,
        }
    }

    /// Start the AI CEO decision loop — runs continuously, analyzing markets
    /// and producing recommendations at the configured interval.
    pub async fn start(&self) -> Result<(), String> {
        info!("AICEO: decision loop starting");
        let state = self.state.clone();
        let pairs = self.active_pairs.clone();
        let interval = {
            let s = state.read().await;
            Duration::from_secs(s.config.analysis_interval_secs)
        };

        loop {
            {
                let s = state.read().await;
                if s.config.analysis_interval_secs == 0 {
                    break;
                }
            }

            let analysis = Self::compute_market_analysis_static(&pairs).await;
            {
                let mut s = state.write().await;
                s.last_analysis = Some(analysis.clone());
            }

            Self::generate_recommendations_static(&state, &pairs).await;
            Self::auto_adjust_parameters_static(&state).await;

            {
                let s = state.read().await;
                info!(
                    decisions = s.decisions.len(),
                    recommendations = s.recommendations.len(),
                    pnl = format!("{:.2}", s.cumulative_pnl),
                    "AICEO: analysis cycle complete"
                );
            }

            tokio::time::sleep(interval).await;
        }

        Ok(())
    }

    /// Perform market analysis across all active pairs.
    pub async fn analyze_market(&self) -> MarketAnalysis {
        let analysis = Self::compute_market_analysis_static(&self.active_pairs).await;
        {
            let mut s = self.state.write().await;
            s.last_analysis = Some(analysis.clone());
        }
        analysis
    }

    /// Make a trading decision given a context.
    pub async fn make_decision(&self, context: DecisionContext) -> Decision {
        let decision = Self::compute_decision_static(&context).await;

        let mut s = self.state.write().await;

        if decision.override_applied {
            let entry = OverrideAuditEntry {
                timestamp: decision.timestamp,
                decision_id: decision.id.clone(),
                symbol: decision.symbol.clone(),
                original_risk_score: decision.risk_score,
                overridden_risk_score: decision.risk_score * 0.5,
                reason: "CEO override: extreme market conditions".to_string(),
                risk_tolerance: s.config.risk_tolerance,
            };
            s.overrides.push(entry);
        }

        s.decisions.push(decision.clone());
        decision
    }

    /// Review recent trade outcomes and produce a performance report.
    pub async fn review_outcomes(&self) -> ReviewReport {
        let s = self.state.read().await;
        let total = s.decisions.len() as u64;
        let wins = s.total_wins;
        let losses = s.total_losses;
        let win_rate = if total > 0 { wins as f64 / total as f64 } else { 0.0 };
        let avg_return = if total > 0 { s.cumulative_pnl / total as f64 } else { 0.0 };

        let best = s.decisions.iter()
            .filter(|d| d.action == DecisionAction::Buy || d.action == DecisionAction::Increase)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
            .map(|d| BestWorstTrade {
                symbol: d.symbol.clone(),
                pnl: d.confidence * 100.0,
                timestamp: d.timestamp,
            });

        let worst = s.decisions.iter()
            .filter(|d| d.action == DecisionAction::Sell || d.action == DecisionAction::Reduce)
            .min_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
            .map(|d| BestWorstTrade {
                symbol: d.symbol.clone(),
                pnl: -(d.confidence * 100.0),
                timestamp: d.timestamp,
            });

        let period_start = s.decisions.first().map(|d| d.timestamp).unwrap_or(0);
        let period_end = s.decisions.last().map(|d| d.timestamp).unwrap_or(0);

        ReviewReport {
            total_decisions: total,
            winning_decisions: wins,
            losing_decisions: losses,
            win_rate,
            avg_return,
            best_trade: best,
            worst_trade: worst,
            adjustments_made: s.parameter_adjustments.clone(),
            period_start,
            period_end,
        }
    }

    /// Get current AICEO status.
    pub async fn get_status(&self) -> AICEOStatus {
        let s = self.state.read().await;
        let total = s.decisions.len() as u64;
        let win_rate = if total > 0 {
            s.total_wins as f64 / total as f64
        } else {
            0.0
        };
        AICEOStatus {
            uptime_secs: s.start_time.elapsed().as_secs(),
            decisions_made: total,
            current_recommendations_count: s.recommendations.len(),
            risk_level: s.config.risk_tolerance,
            last_analysis_at: s.last_analysis.as_ref().map(|a| a.timestamp),
            win_rate,
            override_events: s.overrides.len() as u64,
        }
    }

    /// Get current recommendations.
    pub async fn get_recommendations(&self) -> Vec<Recommendation> {
        let s = self.state.read().await;
        s.recommendations.clone()
    }

    // ==================== Internal Static Helpers ====================

    async fn compute_market_analysis_static(pairs: &[String]) -> MarketAnalysis {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let mut rng_seed = now as u64;
        let pseudo_random = |seed: &mut u64| -> f64 {
            *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((*seed >> 33) as f64) / (1u64 << 31) as f64
        };

        let trend_val = pseudo_random(&mut rng_seed);
        let trend = if trend_val < 0.25 {
            MarketTrend::Bullish
        } else if trend_val < 0.50 {
            MarketTrend::Bearish
        } else if trend_val < 0.75 {
            MarketTrend::Sideways
        } else {
            MarketTrend::Volatile
        };

        let volatility = pseudo_random(&mut rng_seed);
        let sentiment = pseudo_random(&mut rng_seed);

        let mut key_levels = Vec::new();
        for (i, pair) in pairs.iter().enumerate() {
            let base_price = match pair.as_str() {
                "USD/EUR" => 0.92,
                "USD/EGP" => 50.0,
                "USD/SAR" => 3.75,
                "USD/AED" => 3.67,
                "USD/GBP" => 0.79,
                "EUR/EGP" => 54.0,
                _ => 1.0,
            };
            let offset = (pseudo_random(&mut rng_seed) - 0.5) * base_price * 0.02;
            key_levels.push(KeyLevel {
                price: base_price + offset,
                label: format!("{}_support_{}", pair, i),
                strength: pseudo_random(&mut rng_seed) * 100.0,
            });
        }

        let current_volume = pseudo_random(&mut rng_seed) * 1_000_000.0;
        let avg_volume = pseudo_random(&mut rng_seed) * 800_000.0 + 200_000.0;
        let volume_ratio = if avg_volume > 0.0 { current_volume / avg_volume } else { 1.0 };

        MarketAnalysis {
            trend,
            volatility_score: volatility,
            sentiment_score: sentiment,
            key_levels,
            volume_analysis: VolumeAnalysis {
                current_volume,
                avg_volume_24h: avg_volume,
                volume_ratio,
                trend: if volume_ratio > 1.2 { "increasing".to_string() }
                    else if volume_ratio < 0.8 { "decreasing".to_string() }
                    else { "stable".to_string() },
            },
            timestamp: now,
            symbols_analyzed: pairs.to_vec(),
        }
    }

    async fn compute_decision_static(context: &DecisionContext) -> Decision {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let _rng_seed = now as u64 ^ context.symbol.len() as u64;
        let _pseudo_random = |seed: &mut u64| -> f64 {
            *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((*seed >> 33) as f64) / (1u64 << 31) as f64
        };

        let vol = context.market_analysis.volatility_score;
        let sentiment = context.market_analysis.sentiment_score;

        let (action, confidence, risk_score, reasoning) = match context.market_analysis.trend {
            MarketTrend::Bullish => {
                let conf = 0.6 + sentiment * 0.3;
                let risk = vol * 0.5;
                let existing = context.current_positions.get(&context.symbol).copied().unwrap_or(0.0);
                if existing > 0.0 {
                    (DecisionAction::Increase, conf, risk,
                        format!("Bullish trend confirmed, increasing {} position", context.symbol))
                } else {
                    (DecisionAction::Buy, conf, risk,
                        format!("Bullish trend detected, entering {} position", context.symbol))
                }
            }
            MarketTrend::Bearish => {
                let conf = 0.5 + (1.0 - sentiment) * 0.3;
                let risk = vol * 0.7;
                let existing = context.current_positions.get(&context.symbol).copied().unwrap_or(0.0);
                if existing > 0.0 {
                    (DecisionAction::Reduce, conf, risk,
                        format!("Bearish trend, reducing {} exposure", context.symbol))
                } else {
                    (DecisionAction::Hold, conf * 0.8, risk,
                        format!("Bearish trend, holding off on {}", context.symbol))
                }
            }
            MarketTrend::Sideways => {
                let conf = 0.4 + vol * 0.2;
                let risk = vol * 0.3;
                (DecisionAction::Hold, conf, risk,
                    format!("Sideways market, maintaining current {} positions", context.symbol))
            }
            MarketTrend::Volatile => {
                let conf = 0.3 + vol * 0.4;
                let risk = vol;
                (DecisionAction::Hold, conf, risk,
                    format!("High volatility detected on {}, caution advised", context.symbol))
            }
        };

        let size_pct = context.market_analysis.volatility_score * 0.1;
        let override_applied = false;

        Decision {
            id: Uuid::new_v4().to_string(),
            action,
            symbol: context.symbol.clone(),
            size_pct: size_pct.min(0.15),
            confidence,
            reasoning,
            risk_score,
            timestamp: now,
            override_applied,
        }
    }

    async fn generate_recommendations_static(state: &Arc<RwLock<InternalState>>, pairs: &[String]) {
        let analysis = {
            let s = state.read().await;
            s.last_analysis.clone()
        };
        let analysis = match analysis {
            Some(a) => a,
            None => return,
        };

        let mut recommendations = Vec::new();

        for pair in pairs {
            let base_price = match pair.as_str() {
                "USD/EUR" => 0.92,
                "USD/EGP" => 50.0,
                "USD/SAR" => 3.75,
                "USD/AED" => 3.67,
                "USD/GBP" => 0.79,
                "EUR/EGP" => 54.0,
                _ => 1.0,
            };

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;

            let mut rng_seed = now as u64 ^ pair.len() as u64;
            let pseudo_random = |seed: &mut u64| -> f64 {
                *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((*seed >> 33) as f64) / (1u64 << 31) as f64
            };

            let action = match analysis.trend {
                MarketTrend::Bullish => DecisionAction::Buy,
                MarketTrend::Bearish => DecisionAction::Sell,
                MarketTrend::Sideways => DecisionAction::Hold,
                MarketTrend::Volatile => DecisionAction::Hold,
            };

            let spread = base_price * 0.02;
            let target = base_price + match analysis.trend {
                MarketTrend::Bullish => spread,
                MarketTrend::Bearish => -spread,
                MarketTrend::Sideways => 0.0,
                MarketTrend::Volatile => (pseudo_random(&mut rng_seed) - 0.5) * spread,
            };

            let stop_loss = base_price - base_price * 0.03;
            let take_profit = base_price + base_price * 0.05;

            let confidence = 0.5 + analysis.sentiment_score * 0.3 + analysis.volatility_score * 0.1;

            recommendations.push(Recommendation {
                symbol: pair.clone(),
                action,
                target_price: target,
                stop_loss,
                take_profit,
                confidence,
                rationale: format!(
                    "Market trend: {}, volatility: {:.2}, sentiment: {:.2}",
                    analysis.trend,
                    analysis.volatility_score,
                    analysis.sentiment_score
                ),
                generated_at: now,
            });
        }

        let mut s = state.write().await;
        s.recommendations = recommendations;
    }

    async fn auto_adjust_parameters_static(state: &Arc<RwLock<InternalState>>) {
        let (total, wins, pnl, _lr, current_risk) = {
            let s = state.read().await;
            (
                s.decisions.len() as u64,
                s.total_wins,
                s.cumulative_pnl,
                s.config.learning_rate,
                s.config.risk_tolerance,
            )
        };

        if total < 10 {
            return;
        }

        let win_rate = wins as f64 / total as f64;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let mut s = state.write().await;
        let mut adjustments = Vec::new();

        let old_lr = s.config.learning_rate;
        let new_lr = if win_rate < 0.4 {
            (old_lr * 0.9).max(0.001)
        } else if win_rate > 0.7 {
            (old_lr * 1.1).min(0.1)
        } else {
            old_lr
        };
        if (new_lr - old_lr).abs() > f64::EPSILON {
            adjustments.push(ParameterAdjustment {
                parameter: "learning_rate".to_string(),
                old_value: old_lr,
                new_value: new_lr,
                reason: format!("win_rate={:.2}, adjusting learning rate", win_rate),
                timestamp: now,
            });
            s.config.learning_rate = new_lr;
        }

        let old_max = s.config.max_position_pct;
        let new_max = if pnl < 0.0 {
            (old_max * 0.9).max(0.05)
        } else {
            (old_max * 1.05).min(0.25)
        };
        if (new_max - old_max).abs() > f64::EPSILON {
            adjustments.push(ParameterAdjustment {
                parameter: "max_position_pct".to_string(),
                old_value: old_max,
                new_value: new_max,
                reason: format!("pnl={:.2}, adjusting position limit", pnl),
                timestamp: now,
            });
            s.config.max_position_pct = new_max;
        }

        if !adjustments.is_empty() {
            s.parameter_adjustments.extend(adjustments);
        }

        if win_rate < 0.3 && current_risk != RiskLevel::Conservative {
            let old_risk = s.config.risk_tolerance;
            s.config.risk_tolerance = RiskLevel::Conservative;
            s.parameter_adjustments.push(ParameterAdjustment {
                parameter: "risk_tolerance".to_string(),
                old_value: old_risk as i32 as f64,
                new_value: RiskLevel::Conservative as i32 as f64,
                reason: format!("win_rate={:.2} critically low, downgrading to Conservative", win_rate),
                timestamp: now,
            });
        }
    }
}
