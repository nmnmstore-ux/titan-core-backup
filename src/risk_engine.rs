use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub max_position_usd: f64,
    pub max_position_pct_nav: f64,
    pub max_leverage: f64,
    pub max_drawdown_pct: f64,
    pub var_confidence: f64,
    pub var_horizon_days: u32,
    pub stress_test_scenarios: Vec<StressScenario>,
    pub margin_call_threshold_pct: f64,
    pub liquidation_threshold_pct: f64,
    pub max_daily_loss_usd: f64,
    pub max_order_size_usd: f64,
    pub max_open_orders: u32,
    pub concentration_limit_pct: f64,
    pub sector_limit_pct: f64,
    pub counterparty_limit_usd: f64,
    pub real_time_monitoring: bool,
    pub pre_trade_checks: bool,
    pub post_trade_checks: bool,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_position_usd: 100_000_000.0,
            max_position_pct_nav: 0.1,
            max_leverage: 10.0,
            max_drawdown_pct: 0.15,
            var_confidence: 0.99,
            var_horizon_days: 1,
            stress_test_scenarios: vec![
                StressScenario {
                    name: "2008_Financial_Crisis".to_string(),
                    equity_shock: -0.5,
                    fx_shock: 0.3,
                    rates_shock: 0.02,
                    credit_spread_shock: 0.05,
                    volatility_multiplier: 3.0,
                    correlation_breakdown: 0.8,
                },
                StressScenario {
                    name: "COVID_Crash_2020".to_string(),
                    equity_shock: -0.35,
                    fx_shock: 0.15,
                    rates_shock: -0.01,
                    credit_spread_shock: 0.03,
                    volatility_multiplier: 4.0,
                    correlation_breakdown: 0.9,
                },
                StressScenario {
                    name: "Flash_Crash".to_string(),
                    equity_shock: -0.1,
                    fx_shock: 0.05,
                    rates_shock: 0.0,
                    credit_spread_shock: 0.01,
                    volatility_multiplier: 10.0,
                    correlation_breakdown: 0.5,
                },
                StressScenario {
                    name: "Crypto_Winter".to_string(),
                    equity_shock: -0.2,
                    fx_shock: 0.05,
                    rates_shock: 0.0,
                    credit_spread_shock: 0.0,
                    volatility_multiplier: 5.0,
                    correlation_breakdown: 0.3,
                },
            ],
            margin_call_threshold_pct: 0.8,
            liquidation_threshold_pct: 0.5,
            max_daily_loss_usd: 10_000_000.0,
            max_order_size_usd: 50_000_000.0,
            max_open_orders: 1000,
            concentration_limit_pct: 0.25,
            sector_limit_pct: 0.4,
            counterparty_limit_usd: 50_000_000.0,
            real_time_monitoring: true,
            pre_trade_checks: true,
            post_trade_checks: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressScenario {
    pub name: String,
    pub equity_shock: f64,
    pub fx_shock: f64,
    pub rates_shock: f64,
    pub credit_spread_shock: f64,
    pub volatility_multiplier: f64,
    pub correlation_breakdown: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantRiskProfile {
    pub participant_id: String,
    pub nav_usd: f64,
    pub total_exposure_usd: f64,
    pub leverage: f64,
    pub margin_used_usd: f64,
    pub margin_available_usd: f64,
    pub margin_call_level: f64,
    pub liquidation_level: f64,
    pub current_drawdown_pct: f64,
    pub max_drawdown_pct: f64,
    pub daily_pnl_usd: f64,
    pub daily_loss_limit_usd: f64,
    pub var_1d_99_usd: f64,
    pub var_10d_99_usd: f64,
    pub expected_shortfall_usd: f64,
    pub positions: HashMap<String, PositionRisk>,
    pub open_orders_count: u32,
    pub risk_score: f64,
    pub risk_tier: RiskTier,
    pub alerts: Vec<RiskAlert>,
    pub last_updated: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionRisk {
    pub symbol: String,
    pub quantity: f64,
    pub market_value_usd: f64,
    pub unrealized_pnl_usd: f64,
    pub realized_pnl_usd: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub weight_pct: f64,
    pub beta: f64,
    pub volatility: f64,
    pub var_contribution: f64,
    pub sector: String,
    pub currency: String,
    pub liquidity_score: f64,
    pub concentration_risk: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskTier {
    Low,
    Medium,
    High,
    Critical,
    Liquidation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAlert {
    pub alert_id: String,
    pub participant_id: String,
    pub alert_type: RiskAlertType,
    pub severity: RiskTier,
    pub message: String,
    pub threshold: f64,
    pub current_value: f64,
    pub triggered_at: u64,
    pub acknowledged: bool,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskAlertType {
    PositionLimit,
    LeverageLimit,
    DrawdownLimit,
    DailyLossLimit,
    VaRLimit,
    ConcentrationRisk,
    SectorConcentration,
    CounterpartyExposure,
    MarginCall,
    LiquidationRisk,
    OrderSizeLimit,
    OpenOrdersLimit,
    LiquidityRisk,
    CorrelationRisk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreTradeCheckResult {
    pub allowed: bool,
    pub checks: Vec<PreTradeCheck>,
    pub rejection_reason: Option<String>,
    pub warnings: Vec<String>,
    pub required_margin_usd: f64,
    pub estimated_margin_impact_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreTradeCheck {
    pub check_name: String,
    pub passed: bool,
    pub current_value: f64,
    pub limit: f64,
    pub severity: RiskTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostTradeAnalysis {
    pub trade_id: String,
    pub participant_id: String,
    pub symbol: String,
    pub side: String,
    pub quantity: f64,
    pub price: f64,
    pub impact_on_var: f64,
    pub impact_on_drawdown: f64,
    pub impact_on_leverage: f64,
    pub new_risk_tier: RiskTier,
    pub new_alerts: Vec<RiskAlert>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RiskMetrics {
    pub total_participants: u32,
    pub participants_at_risk: u32,
    pub total_margin_used_usd: f64,
    pub total_margin_available_usd: f64,
    pub avg_leverage: f64,
    pub max_leverage: f64,
    pub total_var_usd: f64,
    pub total_exposure_usd: f64,
    pub margin_calls_pending: u32,
    pub liquidations_pending: u32,
    pub daily_loss_limit_breaches: u32,
    pub concentration_breaches: u32,
}

pub struct RiskEngine {
    config: RiskConfig,
    profiles: Arc<RwLock<HashMap<String, ParticipantRiskProfile>>>,
    market_data: Arc<RwLock<HashMap<String, MarketData>>>,
    correlation_matrix: Arc<RwLock<HashMap<String, HashMap<String, f64>>>>,
    volatility_cache: Arc<RwLock<HashMap<String, f64>>>,
    alerts: Arc<RwLock<Vec<RiskAlert>>>,
    metrics: Arc<RwLock<RiskMetrics>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketData {
    pub symbol: String,
    pub price: f64,
    pub bid: f64,
    pub ask: f64,
    pub volume_24h: f64,
    pub volatility: f64,
    pub beta: f64,
    pub sector: String,
    pub currency: String,
    pub last_updated: u64,
}

impl RiskEngine {
    pub fn new(config: RiskConfig) -> Self {
        Self {
            config,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            market_data: Arc::new(RwLock::new(HashMap::new())),
            correlation_matrix: Arc::new(RwLock::new(HashMap::new())),
            volatility_cache: Arc::new(RwLock::new(HashMap::new())),
            alerts: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(RiskMetrics::default())),
        }
    }

    pub async fn initialize(&self) -> Result<(), String> {
        self.load_market_data().await?;
        self.start_monitoring_loop().await;
        info!("Risk engine initialized");
        Ok(())
    }

    async fn load_market_data(&self) -> Result<(), String> {
        let mut data = self.market_data.write().await;
        let symbols = vec!["BTC/USD", "ETH/USD", "EUR/USD", "GBP/USD", "USD/JPY", "XAU/USD", "SPX", "NDX"];
        
        for symbol in symbols {
            data.insert(symbol.to_string(), MarketData {
                symbol: symbol.to_string(),
                price: match symbol {
                    "BTC/USD" => 50000.0,
                    "ETH/USD" => 3000.0,
                    "EUR/USD" => 1.08,
                    "GBP/USD" => 1.25,
                    "USD/JPY" => 150.0,
                    "XAU/USD" => 2000.0,
                    "SPX" => 4500.0,
                    "NDX" => 15000.0,
                    _ => 100.0,
                },
                bid: 0.0,
                ask: 0.0,
                volume_24h: 1_000_000_000.0,
                volatility: match symbol {
                    "BTC/USD" => 0.8,
                    "ETH/USD" => 1.0,
                    "EUR/USD" => 0.1,
                    _ => 0.2,
                },
                beta: match symbol {
                    "BTC/USD" => 1.5,
                    "ETH/USD" => 1.8,
                    "SPX" => 1.0,
                    "NDX" => 1.2,
                    _ => 0.5,
                },
                sector: match symbol {
                    s if s.contains("BTC") || s.contains("ETH") => "Crypto".to_string(),
                    s if s.contains("EUR") || s.contains("GBP") || s.contains("JPY") => "FX".to_string(),
                    s if s.contains("SPX") || s.contains("NDX") => "Equity".to_string(),
                    s if s.contains("XAU") => "Commodity".to_string(),
                    _ => "Other".to_string(),
                },
                currency: "USD".to_string(),
                last_updated: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            });
        }
        
        self.build_correlation_matrix().await;
        Ok(())
    }

    async fn build_correlation_matrix(&self) {
        let mut matrix = self.correlation_matrix.write().await;
        let symbols = vec!["BTC/USD", "ETH/USD", "EUR/USD", "GBP/USD", "USD/JPY", "XAU/USD", "SPX", "NDX"];
        
        for (i, s1) in symbols.iter().enumerate() {
            for (j, s2) in symbols.iter().enumerate() {
                let corr = if i == j { 1.0 } else {
                    match (*s1, *s2) {
                        ("BTC/USD", "ETH/USD") | ("ETH/USD", "BTC/USD") => 0.85,
                        ("EUR/USD", "GBP/USD") | ("GBP/USD", "EUR/USD") => 0.9,
                        ("SPX", "NDX") | ("NDX", "SPX") => 0.95,
                        ("BTC/USD", "SPX") | ("SPX", "BTC/USD") => 0.3,
                        ("XAU/USD", "USD/JPY") | ("USD/JPY", "XAU/USD") => -0.4,
                        _ => 0.1,
                    }
                };
                matrix.entry(s1.to_string()).or_default().insert(s2.to_string(), corr);
            }
        }
    }

    async fn start_monitoring_loop(&self) {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                if engine.config.real_time_monitoring {
                    if let Err(e) = engine.monitor_all_participants().await {
                        warn!("Risk monitoring error: {}", e);
                    }
                }
            }
        });
    }

    pub async fn register_participant(&self, participant_id: String, initial_nav: f64) -> ParticipantRiskProfile {
        let profile = ParticipantRiskProfile {
            participant_id: participant_id.clone(),
            nav_usd: initial_nav,
            total_exposure_usd: 0.0,
            leverage: 1.0,
            margin_used_usd: 0.0,
            margin_available_usd: initial_nav,
            margin_call_level: initial_nav * self.config.margin_call_threshold_pct,
            liquidation_level: initial_nav * self.config.liquidation_threshold_pct,
            current_drawdown_pct: 0.0,
            max_drawdown_pct: 0.0,
            daily_pnl_usd: 0.0,
            daily_loss_limit_usd: self.config.max_daily_loss_usd,
            var_1d_99_usd: 0.0,
            var_10d_99_usd: 0.0,
            expected_shortfall_usd: 0.0,
            positions: HashMap::new(),
            open_orders_count: 0,
            risk_score: 0.1,
            risk_tier: RiskTier::Low,
            alerts: vec![],
            last_updated: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        };
        
        self.profiles.write().await.insert(participant_id.clone(), profile.clone());
        info!("Registered participant {} for risk management", participant_id);
        profile
    }

    pub async fn pre_trade_check(
        &self,
        participant_id: &str,
        symbol: &str,
        _side: &str,
        quantity: f64,
        price: f64,
    ) -> Result<PreTradeCheckResult, String> {
        let profile = self.profiles.read().await.get(participant_id).cloned()
            .ok_or("Participant not found")?;
        
        let notional = quantity * price;
        let mut checks = Vec::new();
        let mut warnings = Vec::new();
        let mut allowed = true;
        
        let order_size_check = PreTradeCheck {
            check_name: "Order Size Limit".to_string(),
            passed: notional <= self.config.max_order_size_usd,
            current_value: notional,
            limit: self.config.max_order_size_usd,
            severity: RiskTier::High,
        };
        if !order_size_check.passed { allowed = false; }
        checks.push(order_size_check);
        
        let projected_exposure = profile.total_exposure_usd + notional;
        let position_limit_check = PreTradeCheck {
            check_name: "Position Limit".to_string(),
            passed: projected_exposure <= self.config.max_position_usd,
            current_value: projected_exposure,
            limit: self.config.max_position_usd,
            severity: RiskTier::High,
        };
        if !position_limit_check.passed { allowed = false; }
        checks.push(position_limit_check);
        
        let projected_leverage = projected_exposure / profile.nav_usd.max(1.0);
        let leverage_check = PreTradeCheck {
            check_name: "Leverage Limit".to_string(),
            passed: projected_leverage <= self.config.max_leverage,
            current_value: projected_leverage,
            limit: self.config.max_leverage,
            severity: RiskTier::Critical,
        };
        if !leverage_check.passed { allowed = false; }
        checks.push(leverage_check);
        
        let daily_loss_check = PreTradeCheck {
            check_name: "Daily Loss Limit".to_string(),
            passed: profile.daily_pnl_usd > -self.config.max_daily_loss_usd,
            current_value: -profile.daily_pnl_usd,
            limit: self.config.max_daily_loss_usd,
            severity: RiskTier::Critical,
        };
        if !daily_loss_check.passed { allowed = false; }
        checks.push(daily_loss_check);
        
        let open_orders_check = PreTradeCheck {
            check_name: "Open Orders Limit".to_string(),
            passed: profile.open_orders_count < self.config.max_open_orders,
            current_value: profile.open_orders_count as f64,
            limit: self.config.max_open_orders as f64,
            severity: RiskTier::Medium,
        };
        if !open_orders_check.passed { allowed = false; }
        checks.push(open_orders_check);
        
        let symbol_position = profile.positions.get(symbol);
        let symbol_exposure = symbol_position.map(|p| p.market_value_usd).unwrap_or(0.0);
        let projected_symbol_exposure = symbol_exposure + notional;
        let concentration_check = PreTradeCheck {
            check_name: "Concentration Limit".to_string(),
            passed: projected_symbol_exposure / profile.nav_usd.max(1.0) <= self.config.concentration_limit_pct,
            current_value: projected_symbol_exposure / profile.nav_usd.max(1.0),
            limit: self.config.concentration_limit_pct,
            severity: RiskTier::High,
        };
        if !concentration_check.passed { allowed = false; }
        checks.push(concentration_check);
        
        let sector = self.market_data.read().await.get(symbol).map(|d| d.sector.clone()).unwrap_or("Other".to_string());
        let sector_map: HashMap<String, String> = {
            let md = self.market_data.read().await;
            profile.positions.keys().map(|s| {
                let sec = md.get(s).map(|d| d.sector.clone()).unwrap_or_default();
                (s.clone(), sec)
            }).collect()
        };
        let sector_exposure: f64 = profile.positions.iter()
            .filter(|(s, _)| sector_map.get(*s).map(|d| d.as_str()) == Some(&sector))
            .map(|(_, p)| p.market_value_usd)
            .sum();
        let projected_sector_exposure = sector_exposure + notional;
        let sector_check = PreTradeCheck {
            check_name: "Sector Concentration".to_string(),
            passed: projected_sector_exposure / profile.nav_usd.max(1.0) <= self.config.sector_limit_pct,
            current_value: projected_sector_exposure / profile.nav_usd.max(1.0),
            limit: self.config.sector_limit_pct,
            severity: RiskTier::Medium,
        };
        if !sector_check.passed { allowed = false; }
        checks.push(sector_check);
        
        let required_margin = notional / self.config.max_leverage;
        let estimated_margin_impact = required_margin / profile.margin_available_usd.max(1.0);
        
        if profile.leverage > self.config.max_leverage * 0.9 {
            warnings.push(format!("Leverage at {:.1}% of limit", profile.leverage / self.config.max_leverage * 100.0));
        }
        if profile.current_drawdown_pct > self.config.max_drawdown_pct * 0.8 {
            warnings.push(format!("Drawdown at {:.1}% of limit", profile.current_drawdown_pct / self.config.max_drawdown_pct * 100.0));
        }
        
        Ok(PreTradeCheckResult {
            allowed,
            checks,
            rejection_reason: if !allowed { Some("Pre-trade risk checks failed".to_string()) } else { None },
            warnings,
            required_margin_usd: required_margin,
            estimated_margin_impact_pct: estimated_margin_impact * 100.0,
        })
    }

    pub async fn post_trade_update(
        &self,
        participant_id: &str,
        symbol: &str,
        side: &str,
        quantity: f64,
        price: f64,
        trade_id: String,
    ) -> Result<PostTradeAnalysis, String> {
        let mut profile = self.profiles.write().await.get_mut(participant_id).cloned()
            .ok_or("Participant not found")?;
        
        let notional = quantity * price;
        let market_data = self.market_data.read().await.get(symbol).cloned();
        
        let position = profile.positions.entry(symbol.to_string()).or_insert(PositionRisk {
            symbol: symbol.to_string(),
            quantity: 0.0,
            market_value_usd: 0.0,
            unrealized_pnl_usd: 0.0,
            realized_pnl_usd: 0.0,
            entry_price: price,
            current_price: price,
            weight_pct: 0.0,
            beta: market_data.as_ref().map(|d| d.beta).unwrap_or(1.0),
            volatility: market_data.as_ref().map(|d| d.volatility).unwrap_or(0.2),
            var_contribution: 0.0,
            sector: market_data.as_ref().map(|d| d.sector.clone()).unwrap_or("Other".to_string()),
            currency: "USD".to_string(),
            liquidity_score: 0.8,
            concentration_risk: 0.0,
        });
        
        let old_quantity = position.quantity;
        let _old_value = position.market_value_usd;
        
        match side {
            "buy" => {
                position.quantity += quantity;
                position.market_value_usd += notional;
                position.entry_price = ((old_quantity * position.entry_price) + notional) / position.quantity.max(0.0001);
            }
            "sell" => {
                position.quantity -= quantity;
                position.market_value_usd -= notional;
                let realized = (price - position.entry_price) * quantity;
                position.realized_pnl_usd += realized;
                profile.daily_pnl_usd += realized;
            }
            _ => {}
        }
        
        position.current_price = price;
        position.unrealized_pnl_usd = (price - position.entry_price) * position.quantity;
        
        if position.quantity.abs() < 0.0001 {
            profile.positions.remove(symbol);
        }
        
        self.recalculate_risk_metrics(&mut profile).await;
        self.check_risk_limits(&mut profile).await;
        
        self.profiles.write().await.insert(participant_id.to_string(), profile.clone());
        
        let new_alerts = profile.alerts.iter().filter(|a| a.triggered_at > SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() - 60).cloned().collect();
        
        Ok(PostTradeAnalysis {
            trade_id,
            participant_id: participant_id.to_string(),
            symbol: symbol.to_string(),
            side: side.to_string(),
            quantity,
            price,
            impact_on_var: self.calculate_var_impact(&profile, symbol, notional).await,
            impact_on_drawdown: profile.current_drawdown_pct,
            impact_on_leverage: profile.leverage,
            new_risk_tier: profile.risk_tier.clone(),
            new_alerts,
            recommendations: self.generate_recommendations(&profile).await,
        })
    }

    async fn recalculate_risk_metrics(&self, profile: &mut ParticipantRiskProfile) {
        profile.total_exposure_usd = profile.positions.values().map(|p| p.market_value_usd.abs()).sum();
        profile.leverage = if profile.nav_usd > 0.0 { profile.total_exposure_usd / profile.nav_usd } else { 0.0 };
        profile.margin_used_usd = profile.total_exposure_usd / self.config.max_leverage;
        profile.margin_available_usd = profile.nav_usd - profile.margin_used_usd;
        
        let total_pnl: f64 = profile.positions.values().map(|p| p.unrealized_pnl_usd + p.realized_pnl_usd).sum();
        let peak_nav = profile.nav_usd + profile.max_drawdown_pct * profile.nav_usd;
        profile.current_drawdown_pct = if peak_nav > 0.0 { (peak_nav - (profile.nav_usd + total_pnl)) / peak_nav } else { 0.0 };
        if profile.current_drawdown_pct > profile.max_drawdown_pct {
            profile.max_drawdown_pct = profile.current_drawdown_pct;
        }
        
        profile.var_1d_99_usd = self.calculate_portfolio_var(profile, 1, self.config.var_confidence).await;
        profile.var_10d_99_usd = profile.var_1d_99_usd * (10.0_f64).sqrt();
        profile.expected_shortfall_usd = profile.var_1d_99_usd * 1.3;
        
        profile.risk_score = self.calculate_risk_score(profile);
        profile.risk_tier = self.determine_risk_tier(profile.risk_score);
        profile.last_updated = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    }

    async fn calculate_portfolio_var(&self, profile: &ParticipantRiskProfile, horizon_days: u32, confidence: f64) -> f64 {
        let mut portfolio_variance = 0.0;
        let mut weights = HashMap::new();
        let mut volatilities = HashMap::new();
        
        for (symbol, pos) in &profile.positions {
            weights.insert(symbol.clone(), pos.market_value_usd.abs() / profile.total_exposure_usd.max(1.0));
            volatilities.insert(symbol.clone(), pos.volatility);
        }
        
        for (s1, w1) in &weights {
            for (s2, w2) in &weights {
                let corr = self.correlation_matrix.read().await
                    .get(s1).and_then(|m| m.get(s2)).copied().unwrap_or(0.0);
                let vol1 = volatilities.get(s1).copied().unwrap_or(0.2);
                let vol2 = volatilities.get(s2).copied().unwrap_or(0.2);
                portfolio_variance += w1 * w2 * corr * vol1 * vol2;
            }
        }
        
        let portfolio_vol = portfolio_variance.sqrt();
        let z_score = match confidence {
            c if c >= 0.99 => 2.33,
            c if c >= 0.95 => 1.65,
            _ => 1.28,
        };
        
        profile.total_exposure_usd * portfolio_vol * z_score * (horizon_days as f64).sqrt()
    }

    fn calculate_risk_score(&self, profile: &ParticipantRiskProfile) -> f64 {
        let mut score = 0.0;
        
        score += (profile.leverage / self.config.max_leverage).min(1.0) * 0.3;
        score += (profile.current_drawdown_pct / self.config.max_drawdown_pct).min(1.0) * 0.25;
        score += (-profile.daily_pnl_usd / self.config.max_daily_loss_usd).max(0.0).min(1.0) * 0.2;
        score += (profile.var_1d_99_usd / profile.nav_usd.max(1.0)).min(1.0) * 0.15;
        
        let max_concentration = profile.positions.values()
            .map(|p| p.market_value_usd.abs() / profile.nav_usd.max(1.0))
            .fold(0.0, f64::max);
        score += (max_concentration / self.config.concentration_limit_pct).min(1.0) * 0.1;
        
        score.min(1.0)
    }

    fn determine_risk_tier(&self, score: f64) -> RiskTier {
        if score >= 0.9 { RiskTier::Liquidation }
        else if score >= 0.7 { RiskTier::Critical }
        else if score >= 0.5 { RiskTier::High }
        else if score >= 0.3 { RiskTier::Medium }
        else { RiskTier::Low }
    }

    async fn check_risk_limits(&self, profile: &mut ParticipantRiskProfile) {
        let mut new_alerts = Vec::new();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        
        if profile.leverage > self.config.max_leverage {
            new_alerts.push(RiskAlert {
                alert_id: Uuid::new_v4().to_string(),
                participant_id: profile.participant_id.clone(),
                alert_type: RiskAlertType::LeverageLimit,
                severity: RiskTier::Critical,
                message: format!("Leverage {:.2}x exceeds limit {:.2}x", profile.leverage, self.config.max_leverage),
                threshold: self.config.max_leverage,
                current_value: profile.leverage,
                triggered_at: now,
                acknowledged: false,
                resolved: false,
            });
        }
        
        if profile.current_drawdown_pct > self.config.max_drawdown_pct {
            new_alerts.push(RiskAlert {
                alert_id: Uuid::new_v4().to_string(),
                participant_id: profile.participant_id.clone(),
                alert_type: RiskAlertType::DrawdownLimit,
                severity: RiskTier::Critical,
                message: format!("Drawdown {:.2}% exceeds limit {:.2}%", profile.current_drawdown_pct * 100.0, self.config.max_drawdown_pct * 100.0),
                threshold: self.config.max_drawdown_pct,
                current_value: profile.current_drawdown_pct,
                triggered_at: now,
                acknowledged: false,
                resolved: false,
            });
        }
        
        if -profile.daily_pnl_usd > self.config.max_daily_loss_usd {
            new_alerts.push(RiskAlert {
                alert_id: Uuid::new_v4().to_string(),
                participant_id: profile.participant_id.clone(),
                alert_type: RiskAlertType::DailyLossLimit,
                severity: RiskTier::Critical,
                message: format!("Daily loss ${:.0} exceeds limit ${:.0}", -profile.daily_pnl_usd, self.config.max_daily_loss_usd),
                threshold: self.config.max_daily_loss_usd,
                current_value: -profile.daily_pnl_usd,
                triggered_at: now,
                acknowledged: false,
                resolved: false,
            });
        }
        
        for (symbol, pos) in &profile.positions {
            let concentration = pos.market_value_usd.abs() / profile.nav_usd.max(1.0);
            if concentration > self.config.concentration_limit_pct {
                new_alerts.push(RiskAlert {
                    alert_id: Uuid::new_v4().to_string(),
                    participant_id: profile.participant_id.clone(),
                    alert_type: RiskAlertType::ConcentrationRisk,
                    severity: RiskTier::High,
                    message: format!("Position {} concentration {:.2}% exceeds limit {:.2}%", symbol, concentration * 100.0, self.config.concentration_limit_pct * 100.0),
                    threshold: self.config.concentration_limit_pct,
                    current_value: concentration,
                    triggered_at: now,
                    acknowledged: false,
                    resolved: false,
                });
            }
        }
        
        if profile.margin_available_usd <= profile.margin_call_level {
            new_alerts.push(RiskAlert {
                alert_id: Uuid::new_v4().to_string(),
                participant_id: profile.participant_id.clone(),
                alert_type: RiskAlertType::MarginCall,
                severity: RiskTier::High,
                message: "Margin call triggered".to_string(),
                threshold: profile.margin_call_level,
                current_value: profile.margin_available_usd,
                triggered_at: now,
                acknowledged: false,
                resolved: false,
            });
        }
        
        if profile.margin_available_usd <= profile.liquidation_level {
            new_alerts.push(RiskAlert {
                alert_id: Uuid::new_v4().to_string(),
                participant_id: profile.participant_id.clone(),
                alert_type: RiskAlertType::LiquidationRisk,
                severity: RiskTier::Liquidation,
                message: "Liquidation risk - immediate action required".to_string(),
                threshold: profile.liquidation_level,
                current_value: profile.margin_available_usd,
                triggered_at: now,
                acknowledged: false,
                resolved: false,
            });
        }
        
        profile.alerts.extend(new_alerts.clone());
        
        let mut all_alerts = self.alerts.write().await;
        all_alerts.extend(new_alerts);
    }

    async fn calculate_var_impact(&self, profile: &ParticipantRiskProfile, symbol: &str, notional: f64) -> f64 {
        let current_var = profile.var_1d_99_usd;
        let position_weight = notional / profile.total_exposure_usd.max(1.0);
        let symbol_vol = self.volatility_cache.read().await.get(symbol).copied().unwrap_or(0.2);
        current_var * position_weight * symbol_vol
    }

    async fn generate_recommendations(&self, profile: &ParticipantRiskProfile) -> Vec<String> {
        let mut recs = Vec::new();
        
        if profile.leverage > self.config.max_leverage * 0.8 {
            recs.push("Consider reducing position sizes to lower leverage".to_string());
        }
        if profile.current_drawdown_pct > self.config.max_drawdown_pct * 0.7 {
            recs.push("Implement stop-losses to limit further drawdown".to_string());
        }
        
        let max_concentration = profile.positions.values()
            .map(|p| p.market_value_usd.abs() / profile.nav_usd.max(1.0))
            .fold(0.0, f64::max);
        if max_concentration > self.config.concentration_limit_pct * 0.8 {
            recs.push("Diversify positions to reduce concentration risk".to_string());
        }
        
        if profile.open_orders_count as f64 > self.config.max_open_orders as f64 * 0.8 {
            recs.push("Review and cancel stale orders".to_string());
        }
        
        recs
    }

    async fn monitor_all_participants(&self) -> Result<(), String> {
        let mut profiles = self.profiles.write().await;
        
        for profile in profiles.values_mut() {
            for (symbol, position) in &mut profile.positions {
                if let Some(market) = self.market_data.read().await.get(symbol) {
                    position.current_price = market.price;
                    position.unrealized_pnl_usd = (market.price - position.entry_price) * position.quantity;
                    position.market_value_usd = market.price * position.quantity.abs();
                    position.volatility = market.volatility;
                }
            }
            
            self.recalculate_risk_metrics(profile).await;
            self.check_risk_limits(profile).await;
        }
        
        self.update_metrics().await;
        Ok(())
    }

    async fn update_metrics(&self) {
        let profiles = self.profiles.read().await;
        let mut metrics = self.metrics.write().await;
        
        metrics.total_participants = profiles.len() as u32;
        metrics.participants_at_risk = profiles.values().filter(|p| p.risk_tier >= RiskTier::High).count() as u32;
        metrics.total_margin_used_usd = profiles.values().map(|p| p.margin_used_usd).sum();
        metrics.total_margin_available_usd = profiles.values().map(|p| p.margin_available_usd).sum();
        metrics.avg_leverage = if !profiles.is_empty() {
            profiles.values().map(|p| p.leverage).sum::<f64>() / profiles.len() as f64
        } else { 0.0 };
        metrics.max_leverage = profiles.values().map(|p| p.leverage).fold(0.0, f64::max);
        metrics.total_var_usd = profiles.values().map(|p| p.var_1d_99_usd).sum();
        metrics.total_exposure_usd = profiles.values().map(|p| p.total_exposure_usd).sum();
        metrics.margin_calls_pending = profiles.values().filter(|p| p.alerts.iter().any(|a| a.alert_type == RiskAlertType::MarginCall && !a.resolved)).count() as u32;
        metrics.liquidations_pending = profiles.values().filter(|p| p.alerts.iter().any(|a| a.alert_type == RiskAlertType::LiquidationRisk && !a.resolved)).count() as u32;
    }

    pub async fn run_stress_test(&self, participant_id: &str) -> Result<HashMap<String, f64>, String> {
        let profile = self.profiles.read().await.get(participant_id).cloned()
            .ok_or("Participant not found")?;
        
        let mut results = HashMap::new();
        
        for scenario in &self.config.stress_test_scenarios {
            let mut stressed_nav = profile.nav_usd;
            let mut stressed_positions = profile.positions.clone();
            
            for (_symbol, pos) in &mut stressed_positions {
                let sector = &pos.sector;
                let shock = match sector.as_str() {
                    "Crypto" => scenario.equity_shock * scenario.volatility_multiplier,
                    "Equity" => scenario.equity_shock,
                    "FX" => scenario.fx_shock,
                    "Commodity" => scenario.equity_shock * 0.5,
                    _ => scenario.equity_shock * 0.3,
                };
                
                pos.current_price *= 1.0 + shock;
                pos.market_value_usd = pos.current_price * pos.quantity.abs();
                pos.unrealized_pnl_usd = (pos.current_price - pos.entry_price) * pos.quantity;
                stressed_nav += pos.unrealized_pnl_usd;
            }
            
            let stressed_leverage = stressed_positions.values().map(|p| p.market_value_usd.abs()).sum::<f64>() / stressed_nav.max(1.0);
            let stressed_drawdown = (profile.nav_usd - stressed_nav) / profile.nav_usd.max(1.0);
            
            results.insert(format!("{}_nav", scenario.name), stressed_nav);
            results.insert(format!("{}_leverage", scenario.name), stressed_leverage);
            results.insert(format!("{}_drawdown", scenario.name), stressed_drawdown);
            results.insert(format!("{}_survives", scenario.name), if stressed_leverage < self.config.max_leverage && stressed_drawdown < self.config.max_drawdown_pct { 1.0 } else { 0.0 });
        }
        
        Ok(results)
    }

    pub async fn get_profile(&self, participant_id: &str) -> Option<ParticipantRiskProfile> {
        self.profiles.read().await.get(participant_id).cloned()
    }

    pub async fn get_alerts(&self, participant_id: Option<&str>, unresolved_only: bool) -> Vec<RiskAlert> {
        self.alerts.read().await.iter()
            .filter(|a| participant_id.map_or(true, |p| a.participant_id == p))
            .filter(|a| !unresolved_only || !a.resolved)
            .cloned()
            .collect()
    }

    pub async fn acknowledge_alert(&self, alert_id: &str) -> Result<(), String> {
        let mut alerts = self.alerts.write().await;
        if let Some(alert) = alerts.iter_mut().find(|a| a.alert_id == alert_id) {
            alert.acknowledged = true;
            Ok(())
        } else { Err("Alert not found".to_string()) }
    }

    pub async fn get_metrics(&self) -> RiskMetrics {
        self.metrics.read().await.clone()
    }
}

impl Clone for RiskEngine {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            profiles: self.profiles.clone(),
            market_data: self.market_data.clone(),
            correlation_matrix: self.correlation_matrix.clone(),
            volatility_cache: self.volatility_cache.clone(),
            alerts: self.alerts.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_risk_engine() {
        let engine = RiskEngine::new(RiskConfig::default());
        engine.initialize().await.unwrap();
        
        engine.register_participant("trader_1".to_string(), 1_000_000.0).await;
        
        let check = engine.pre_trade_check("trader_1", "BTC/USD", "buy", 1.0, 50000.0).await.unwrap();
        assert!(check.allowed);
        
        let analysis = engine.post_trade_update("trader_1", "BTC/USD", "buy", 1.0, 50000.0, "trade_1".to_string()).await.unwrap();
        assert_eq!(analysis.new_risk_tier, RiskTier::Low);
    }
}