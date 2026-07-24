use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendableAsset {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub total_available: f64,
    pub total_lent: f64,
    pub fee_bps_per_day: u64,
    pub min_lend: f64,
    pub max_lend: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendOffer {
    pub id: u64,
    pub lender_id: String,
    pub asset_id: String,
    pub quantity: f64,
    pub fee_bps: u64,
    pub duration_days: u64,
    pub created_at: i64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorrowAgreement {
    pub id: u64,
    pub borrower_id: String,
    pub lender_id: String,
    pub asset_id: String,
    pub quantity: f64,
    pub fee_bps: u64,
    pub collateral: f64,
    pub daily_fee: f64,
    pub started_at: i64,
    pub due_at: i64,
    pub returned: bool,
    pub total_fees_paid: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritiesLendingSnapshot {
    pub total_assets_lendable: u64,
    pub total_lent_value: f64,
    pub active_loans: u64,
    pub total_fees_collected: f64,
    pub avg_fee_bps: u64,
}

pub struct SecuritiesLending {
    assets: Arc<DashMap<String, LendableAsset>>,
    offers: Arc<DashMap<u64, LendOffer>>,
    agreements: Arc<DashMap<u64, BorrowAgreement>>,
    next_offer_id: AtomicU64,
    next_agreement_id: AtomicU64,
    total_fees_collected: AtomicU64,
}

impl SecuritiesLending {
    pub fn new() -> Self {
        let engine = Self {
            assets: Arc::new(DashMap::new()),
            offers: Arc::new(DashMap::new()),
            agreements: Arc::new(DashMap::new()),
            next_offer_id: AtomicU64::new(1),
            next_agreement_id: AtomicU64::new(1),
            total_fees_collected: AtomicU64::new(0),
        };
        engine.seed_assets();
        info!("Securities Lending initialized with {} assets", engine.assets.len());
        engine
    }

    fn seed_assets(&self) {
        let default_assets = vec![
            LendableAsset { id: "AAPL".into(), name: "Apple Inc.".into(), symbol: "AAPL".into(), total_available: 100_000.0, total_lent: 0.0, fee_bps_per_day: 5, min_lend: 100.0, max_lend: 10_000.0 },
            LendableAsset { id: "GOOGL".into(), name: "Alphabet Inc.".into(), symbol: "GOOGL".into(), total_available: 80_000.0, total_lent: 0.0, fee_bps_per_day: 4, min_lend: 50.0, max_lend: 5_000.0 },
            LendableAsset { id: "TSLA".into(), name: "Tesla Inc.".into(), symbol: "TSLA".into(), total_available: 50_000.0, total_lent: 0.0, fee_bps_per_day: 8, min_lend: 50.0, max_lend: 2_000.0 },
            LendableAsset { id: "BTC".into(), name: "Bitcoin".into(), symbol: "BTC".into(), total_available: 500.0, total_lent: 0.0, fee_bps_per_day: 3, min_lend: 0.1, max_lend: 50.0 },
            LendableAsset { id: "ETH".into(), name: "Ethereum".into(), symbol: "ETH".into(), total_available: 5_000.0, total_lent: 0.0, fee_bps_per_day: 4, min_lend: 1.0, max_lend: 500.0 },
            LendableAsset { id: "USB".into(), name: "Unified Swift-Bridge".into(), symbol: "USB".into(), total_available: 1_000_000.0, total_lent: 0.0, fee_bps_per_day: 2, min_lend: 100.0, max_lend: 100_000.0 },
            LendableAsset { id: "GOLD".into(), name: "Gold Token (RWA)".into(), symbol: "GOLD".into(), total_available: 10_000.0, total_lent: 0.0, fee_bps_per_day: 1, min_lend: 0.1, max_lend: 1_000.0 },
        ];
        for asset in default_assets {
            self.assets.insert(asset.id.clone(), asset);
        }
    }

    pub fn lend(&self, lender_id: String, asset_id: String, quantity: f64, fee_bps: u64, duration_days: u64) -> Result<LendOffer, String> {
        let asset = self.assets.get(&asset_id).ok_or("asset not found")?;
        if quantity < asset.min_lend {
            return Err(format!("minimum lend is {}", asset.min_lend));
        }
        if quantity > asset.max_lend {
            return Err(format!("maximum lend is {}", asset.max_lend));
        }
        if quantity > asset.total_available - asset.total_lent {
            return Err("insufficient available quantity".into());
        }
        drop(asset);

        let id = self.next_offer_id.fetch_add(1, Ordering::Relaxed);
        let offer = LendOffer {
            id,
            lender_id,
            asset_id: asset_id.clone(),
            quantity,
            fee_bps: if fee_bps == 0 { self.assets.get(&asset_id).map_or(5, |a| a.fee_bps_per_day) } else { fee_bps },
            duration_days,
            created_at: now_ms(),
            active: true,
        };
        if let Some(mut a) = self.assets.get_mut(&asset_id) {
            a.total_lent += quantity;
        }
        self.offers.insert(id, offer.clone());
        info!("Lend Offer #{}: {} {} at {} bps/day", id, quantity, asset_id, offer.fee_bps);
        Ok(offer)
    }

    pub fn borrow(&self, borrower_id: String, offer_id: u64, collateral: f64) -> Result<BorrowAgreement, String> {
        let offer = self.offers.get(&offer_id).ok_or("offer not found")?;
        if !offer.active {
            return Err("offer no longer active".into());
        }
        let required_collateral = offer.quantity * 1.1;
        if collateral < required_collateral {
            return Err(format!("need {:.2} collateral, provided {:.2}", required_collateral, collateral));
        }

        let now = now_ms();
        let daily_fee = offer.quantity * (offer.fee_bps as f64 / 10000.0);
        let id = self.next_agreement_id.fetch_add(1, Ordering::Relaxed);

        let agreement = BorrowAgreement {
            id,
            borrower_id,
            lender_id: offer.lender_id.clone(),
            asset_id: offer.asset_id.clone(),
            quantity: offer.quantity,
            fee_bps: offer.fee_bps,
            collateral,
            daily_fee,
            started_at: now,
            due_at: now + (offer.duration_days as i64 * 86_400_000),
            returned: false,
            total_fees_paid: 0.0,
        };

        if let Some(mut o) = self.offers.get_mut(&offer_id) {
            o.active = false;
        }

        self.agreements.insert(id, agreement.clone());
        info!("Borrow #{}: {} {} for {} days, {:.2}/day fee", id, offer.quantity, offer.asset_id, offer.duration_days, daily_fee);
        Ok(agreement)
    }

    pub fn return_asset(&self, agreement_id: u64) -> Result<BorrowAgreement, String> {
        let mut agreement = self.agreements.get_mut(&agreement_id).ok_or("agreement not found")?;
        if agreement.returned {
            return Err("already returned".into());
        }
        let days_active = ((now_ms() - agreement.started_at) as f64 / 86_400_000.0).max(1.0);
        let total_fee = agreement.daily_fee * days_active;

        agreement.total_fees_paid = total_fee;
        agreement.returned = true;
        self.total_fees_collected.fetch_add((total_fee * 100.0) as u64, Ordering::Relaxed);

        if let Some(mut asset) = self.assets.get_mut(&agreement.asset_id) {
            asset.total_lent = (asset.total_lent - agreement.quantity).max(0.0);
        }

        info!("Return #{}: {} {} returned, {:.2} fees collected", agreement_id, agreement.quantity, agreement.asset_id, total_fee);
        Ok(agreement.clone())
    }

    pub fn list_assets(&self) -> Vec<LendableAsset> {
        self.assets.iter().map(|a| a.clone()).collect()
    }

    pub fn list_offers(&self) -> Vec<LendOffer> {
        self.offers.iter().map(|o| o.clone()).collect()
    }

    pub fn list_active_loans(&self) -> Vec<BorrowAgreement> {
        self.agreements.iter().filter(|a| !a.returned).map(|a| a.clone()).collect()
    }

    pub fn snapshot(&self) -> SecuritiesLendingSnapshot {
        SecuritiesLendingSnapshot {
            total_assets_lendable: self.assets.len() as u64,
            total_lent_value: self.assets.iter().map(|a| a.total_lent).sum(),
            active_loans: self.agreements.iter().filter(|a| !a.returned).count() as u64,
            total_fees_collected: self.total_fees_collected.load(Ordering::Relaxed) as f64 / 100.0,
            avg_fee_bps: self.assets.iter().map(|a| a.fee_bps_per_day).sum::<u64>() / self.assets.len().max(1) as u64,
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}
