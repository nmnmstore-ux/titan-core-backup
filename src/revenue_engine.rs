use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueConfig {
    pub maker_fee_bps: u32,
    pub taker_fee_bps: u32,
    pub volume_tier_discounts: Vec<VolumeTier>,
    pub rebate_program: RebateConfig,
    pub data_licensing_fee_usd_monthly: u64,
    pub premium_tier_fee_monthly: u64,
    pub cross_venue_fee_bps: u32,
    pub mei_sharing_bps: u32,
}

impl Default for RevenueConfig {
    fn default() -> Self {
        Self {
            maker_fee_bps: 2,
            taker_fee_bps: 5,
            volume_tier_discounts: vec![
                VolumeTier { min_monthly_volume_usd: 0, max_monthly_volume_usd: 10_000_000, maker_discount_bps: 0, taker_discount_bps: 0, rebate_bps: 0 },
                VolumeTier { min_monthly_volume_usd: 10_000_000, max_monthly_volume_usd: 100_000_000, maker_discount_bps: 1, taker_discount_bps: 2, rebate_bps: 1 },
                VolumeTier { min_monthly_volume_usd: 100_000_000, max_monthly_volume_usd: 1_000_000_000, maker_discount_bps: 2, taker_discount_bps: 3, rebate_bps: 2 },
                VolumeTier { min_monthly_volume_usd: 1_000_000_000, max_monthly_volume_usd: u64::MAX, maker_discount_bps: 3, taker_discount_bps: 4, rebate_bps: 3 },
            ],
            rebate_program: RebateConfig {
                enabled: true,
                base_rebate_bps: 1,
                volume_multiplier: 1.5,
                loyalty_bonus_bps: 1,
                min_monthly_volume_usd: 5_000_000,
            },
            data_licensing_fee_usd_monthly: 50_000,
            premium_tier_fee_monthly: 25_000,
            cross_venue_fee_bps: 3,
            mei_sharing_bps: 50,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeTier {
    pub min_monthly_volume_usd: u64,
    pub max_monthly_volume_usd: u64,
    pub maker_discount_bps: u32,
    pub taker_discount_bps: u32,
    pub rebate_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebateConfig {
    pub enabled: bool,
    pub base_rebate_bps: u32,
    pub volume_multiplier: f64,
    pub loyalty_bonus_bps: u32,
    pub min_monthly_volume_usd: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantRevenueProfile {
    pub participant_id: String,
    pub tier: ParticipantTier,
    pub monthly_volume_usd: u64,
    pub total_fees_paid_usd: u64,
    pub total_rebates_earned_usd: u64,
    pub data_license_active: bool,
    pub premium_tier_active: bool,
    pub joined_at: u64,
    pub last_activity: u64,
    pub referral_code: String,
    pub referred_by: Option<String>,
    pub referral_earnings_usd: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParticipantTier {
    Standard,
    Professional,
    Institutional,
    Prime,
    Sovereign,
}

impl ParticipantTier {
    pub fn from_volume(volume_usd: u64) -> Self {
        match volume_usd {
            0..=10_000_000 => ParticipantTier::Standard,
            10_000_001..=100_000_000 => ParticipantTier::Professional,
            100_000_001..=1_000_000_000 => ParticipantTier::Institutional,
            1_000_000_001..=10_000_000_000 => ParticipantTier::Prime,
            _ => ParticipantTier::Sovereign,
        }
    }

    pub fn fee_multiplier(&self) -> f64 {
        match self {
            ParticipantTier::Standard => 1.0,
            ParticipantTier::Professional => 0.8,
            ParticipantTier::Institutional => 0.6,
            ParticipantTier::Prime => 0.4,
            ParticipantTier::Sovereign => 0.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRevenueBreakdown {
    pub trade_id: String,
    pub timestamp: u64,
    pub participant_id: String,
    pub counterparty_id: String,
    pub symbol: String,
    pub side: String,
    pub quantity: f64,
    pub price: f64,
    pub notional_usd: f64,
    pub maker_fee_usd: f64,
    pub taker_fee_usd: f64,
    pub rebate_usd: f64,
    pub mei_captured_usd: f64,
    pub mei_shared_usd: f64,
    pub net_revenue_usd: f64,
    pub tier_at_trade: ParticipantTier,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RevenueMetrics {
    pub total_revenue_usd: u64,
    pub trading_fees_usd: u64,
    pub rebates_paid_usd: u64,
    pub data_licensing_usd: u64,
    pub premium_tiers_usd: u64,
    pub cross_venue_usd: u64,
    pub mei_captured_usd: u64,
    pub mei_shared_usd: u64,
    pub active_participants: u32,
    pub paying_data_licensees: u32,
    pub premium_subscribers: u32,
    pub revenue_per_participant_usd: f64,
    pub take_rate_bps: f64,
}

pub struct RevenueEngine {
    config: RevenueConfig,
    profiles: Arc<RwLock<HashMap<String, ParticipantRevenueProfile>>>,
    trade_history: Arc<RwLock<Vec<TradeRevenueBreakdown>>>,
    metrics: Arc<RwLock<RevenueMetrics>>,
    referral_tree: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl RevenueEngine {
    pub fn new(config: RevenueConfig) -> Self {
        Self {
            config,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            trade_history: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(RevenueMetrics::default())),
            referral_tree: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get_config(&self) -> &RevenueConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: RevenueConfig) {
        self.config = config;
    }

    pub fn get_profiles(&self) -> &Arc<RwLock<HashMap<String, ParticipantRevenueProfile>>> {
        &self.profiles
    }

    pub fn get_referral_tree(&self) -> &Arc<RwLock<HashMap<String, Vec<String>>>> {
        &self.referral_tree
    }

    pub async fn register_participant(&self, participant_id: String, referred_by: Option<String>) -> ParticipantRevenueProfile {
        let referral_code = format!("REF_{}", Uuid::new_v4().to_string()[..8].to_uppercase());
        let profile = ParticipantRevenueProfile {
            participant_id: participant_id.clone(),
            tier: ParticipantTier::Standard,
            monthly_volume_usd: 0,
            total_fees_paid_usd: 0,
            total_rebates_earned_usd: 0,
            data_license_active: false,
            premium_tier_active: false,
            joined_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            last_activity: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            referral_code: referral_code.clone(),
            referred_by: referred_by.clone(),
            referral_earnings_usd: 0,
        };

        if let Some(ref_id) = &referred_by {
            let mut tree = self.referral_tree.write().await;
            tree.entry(ref_id.clone()).or_default().push(participant_id.clone());
            
            if let Some(ref_profile) = self.profiles.write().await.get_mut(ref_id) {
                ref_profile.referral_earnings_usd += 1000;
            }
        }

        self.profiles.write().await.insert(participant_id.clone(), profile.clone());
        info!("Registered participant {} with referral code {}", participant_id, referral_code);
        profile
    }

    pub async fn calculate_trade_fees(
        &self,
        participant_id: &str,
        counterparty_id: &str,
        symbol: &str,
        side: &str,
        quantity: f64,
        price: f64,
        is_maker: bool,
    ) -> TradeRevenueBreakdown {
        let notional_usd = quantity * price;
        let profile = self.profiles.read().await.get(participant_id).cloned();
        let tier = profile.as_ref().map(|p| p.tier.clone()).unwrap_or(ParticipantTier::Standard);
        let multiplier = tier.fee_multiplier();

        let (base_fee_bps, discount_bps) = if is_maker {
            (self.config.maker_fee_bps, self.get_volume_discount(profile.as_ref(), true))
        } else {
            (self.config.taker_fee_bps, self.get_volume_discount(profile.as_ref(), false))
        };

        let effective_fee_bps = (base_fee_bps as f64 * multiplier - discount_bps as f64).max(0.0) as u32;
        let fee_usd = notional_usd * effective_fee_bps as f64 / 10000.0;

        let rebate_usd = if self.config.rebate_program.enabled && !is_maker {
            if let Some(profile) = profile.as_ref() {
                if profile.monthly_volume_usd >= self.config.rebate_program.min_monthly_volume_usd {
                    let rebate_bps = self.config.rebate_program.base_rebate_bps as f64 * 
                        self.config.rebate_program.volume_multiplier *
                        (1.0 + profile.total_fees_paid_usd as f64 / 1_000_000.0).ln().max(1.0);
                    notional_usd * (rebate_bps as u32) as f64 / 10000.0
                } else { 0.0 }
            } else { 0.0 }
        } else { 0.0 };

        let mei_captured = notional_usd * 0.0001;
        let mei_shared = mei_captured * self.config.mei_sharing_bps as f64 / 10000.0;

        let net_revenue = fee_usd - rebate_usd + mei_captured - mei_shared;

        let breakdown = TradeRevenueBreakdown {
            trade_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            participant_id: participant_id.to_string(),
            counterparty_id: counterparty_id.to_string(),
            symbol: symbol.to_string(),
            side: side.to_string(),
            quantity,
            price,
            notional_usd,
            maker_fee_usd: if is_maker { fee_usd } else { 0.0 },
            taker_fee_usd: if !is_maker { fee_usd } else { 0.0 },
            rebate_usd,
            mei_captured_usd: mei_captured,
            mei_shared_usd: mei_shared,
            net_revenue_usd: net_revenue,
            tier_at_trade: tier,
        };

        self.record_trade(breakdown.clone()).await;
        breakdown
    }

    fn get_volume_discount(&self, profile: Option<&ParticipantRevenueProfile>, is_maker: bool) -> u32 {
        let volume = profile.map(|p| p.monthly_volume_usd).unwrap_or(0);
        for tier in &self.config.volume_tier_discounts {
            if volume >= tier.min_monthly_volume_usd && volume < tier.max_monthly_volume_usd {
                return if is_maker { tier.maker_discount_bps } else { tier.taker_discount_bps };
            }
        }
        0
    }

    async fn record_trade(&self, breakdown: TradeRevenueBreakdown) {
        let mut history = self.trade_history.write().await;
        history.push(breakdown.clone());
        if history.len() > 1_000_000 {
            history.drain(0..100_000);
        }

        let mut metrics = self.metrics.write().await;
        metrics.total_revenue_usd += breakdown.net_revenue_usd as u64;
        metrics.trading_fees_usd += (breakdown.maker_fee_usd + breakdown.taker_fee_usd) as u64;
        metrics.rebates_paid_usd += breakdown.rebate_usd as u64;
        metrics.mei_captured_usd += breakdown.mei_captured_usd as u64;
        metrics.mei_shared_usd += breakdown.mei_shared_usd as u64;

        if let Some(profile) = self.profiles.write().await.get_mut(&breakdown.participant_id) {
            profile.monthly_volume_usd += breakdown.notional_usd as u64;
            profile.total_fees_paid_usd += (breakdown.maker_fee_usd + breakdown.taker_fee_usd) as u64;
            profile.total_rebates_earned_usd += breakdown.rebate_usd as u64;
            profile.last_activity = breakdown.timestamp;
            profile.tier = ParticipantTier::from_volume(profile.monthly_volume_usd);
        }
    }

    pub async fn activate_data_license(&self, participant_id: &str) -> Result<(), String> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles.get_mut(participant_id).ok_or("Participant not found")?;
        profile.data_license_active = true;
        let mut metrics = self.metrics.write().await;
        metrics.data_licensing_usd += self.config.data_licensing_fee_usd_monthly;
        metrics.paying_data_licensees += 1;
        Ok(())
    }

    pub async fn activate_premium_tier(&self, participant_id: &str) -> Result<(), String> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles.get_mut(participant_id).ok_or("Participant not found")?;
        profile.premium_tier_active = true;
        let mut metrics = self.metrics.write().await;
        metrics.premium_tiers_usd += self.config.premium_tier_fee_monthly;
        metrics.premium_subscribers += 1;
        Ok(())
    }

    pub async fn get_metrics(&self) -> RevenueMetrics {
        let mut metrics = self.metrics.read().await.clone();
        let profiles = self.profiles.read().await;
        metrics.active_participants = profiles.len() as u32;
        if metrics.active_participants > 0 {
            metrics.revenue_per_participant_usd = metrics.total_revenue_usd as f64 / metrics.active_participants as f64;
        }
        let total_volume: u64 = profiles.values().map(|p| p.monthly_volume_usd).sum();
        if total_volume > 0 {
            metrics.take_rate_bps = (metrics.trading_fees_usd as f64 / total_volume as f64) * 10000.0;
        }
        metrics
    }

    pub async fn get_participant_profile(&self, participant_id: &str) -> Option<ParticipantRevenueProfile> {
        self.profiles.read().await.get(participant_id).cloned()
    }

    pub async fn monthly_reset(&self) {
        let mut profiles = self.profiles.write().await;
        for profile in profiles.values_mut() {
            profile.monthly_volume_usd = 0;
        }
        info!("Monthly volume reset completed for {} participants", profiles.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_revenue_engine_basic() {
        let engine = RevenueEngine::new(RevenueConfig::default());
        let profile = engine.register_participant("test_broker_1".to_string(), None).await;
        assert_eq!(profile.tier, ParticipantTier::Standard);
        
        let breakdown = engine.calculate_trade_fees(
            "test_broker_1", "counterparty_1", "EUR/USD", "buy", 1_000_000.0, 1.08, false
        ).await;
        
        assert!(breakdown.net_revenue_usd > 0.0);
        assert_eq!(breakdown.tier_at_trade, ParticipantTier::Standard);
    }

    #[tokio::test]
    async fn test_volume_tier_progression() {
        let engine = RevenueEngine::new(RevenueConfig::default());
        engine.register_participant("whale".to_string(), None).await;
        
        for _ in 0..100 {
            engine.calculate_trade_fees("whale", "cp", "EUR/USD", "buy", 10_000_000.0, 1.08, false).await;
        }
        
        let profile = engine.get_participant_profile("whale").await.unwrap();
        assert!(profile.tier >= ParticipantTier::Professional);
    }
}