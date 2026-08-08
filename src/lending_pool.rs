use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingPoolConfig {
    pub base_apr_bps: u64,
    pub collateral_ratio: f64,
    pub min_deposit: f64,
    pub min_borrow: f64,
    pub max_loan_term_days: u64,
    pub liquidation_threshold: f64,
}

impl Default for LendingPoolConfig {
    fn default() -> Self {
        Self {
            base_apr_bps: 500,
            collateral_ratio: 1.5,
            min_deposit: 100.0,
            min_borrow: 500.0,
            max_loan_term_days: 365,
            liquidation_threshold: 0.8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositPosition {
    pub id: u64,
    pub user_id: String,
    pub asset: String,
    pub amount: f64,
    pub apr_bps: u64,
    pub deposit_time: i64,
    pub accumulated_interest: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoanPosition {
    pub id: u64,
    pub user_id: String,
    pub asset: String,
    pub principal: f64,
    pub outstanding: f64,
    pub apr_bps: u64,
    pub collateral_asset: String,
    pub collateral_amount: f64,
    pub created_at: i64,
    pub due_at: i64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingPoolSnapshot {
    pub total_deposits: f64,
    pub total_loans: f64,
    pub utilization_pct: f64,
    pub active_depositors: u64,
    pub active_borrowers: u64,
    pub total_interest_earned: f64,
    pub total_interest_paid: f64,
    pub base_apr_bps: u64,
}

pub struct LendingPool {
    pub deposits: Arc<DashMap<u64, DepositPosition>>,
    pub loans: Arc<DashMap<u64, LoanPosition>>,
    config: LendingPoolConfig,
    total_deposits: AtomicU64,
    total_loans: AtomicU64,
    next_deposit_id: AtomicU64,
    next_loan_id: AtomicU64,
    total_interest_paid: AtomicU64,
    total_interest_earned: AtomicU64,
}

impl LendingPool {
    pub fn new(config: LendingPoolConfig) -> Self {
        info!("Lending Pool initialized with base APR {} bps", config.base_apr_bps);
        Self {
            deposits: Arc::new(DashMap::new()),
            loans: Arc::new(DashMap::new()),
            config,
            total_deposits: AtomicU64::new(0),
            total_loans: AtomicU64::new(0),
            next_deposit_id: AtomicU64::new(1),
            next_loan_id: AtomicU64::new(1),
            total_interest_paid: AtomicU64::new(0),
            total_interest_earned: AtomicU64::new(0),
        }
    }

    pub fn deposit(&self, user_id: String, asset: String, amount: f64) -> Result<DepositPosition, String> {
        if amount < self.config.min_deposit {
            return Err(format!("minimum deposit is {}", self.config.min_deposit));
        }
        let now = now_ms();
        let id = self.next_deposit_id.fetch_add(1, Ordering::Relaxed);
        let pos = DepositPosition {
            id,
            user_id: user_id.clone(),
            asset: asset.clone(),
            amount,
            apr_bps: self.config.base_apr_bps,
            deposit_time: now,
            accumulated_interest: 0.0,
        };
        self.deposits.insert(id, pos.clone());
        self.total_deposits.fetch_add((amount * 100.0) as u64, Ordering::Relaxed);
        info!("Deposit #{}: {} {} by {}", id, amount, asset, user_id);
        Ok(pos)
    }

    pub fn borrow(&self, user_id: String, asset: String, amount: f64, collateral_asset: String, collateral_amount: f64) -> Result<LoanPosition, String> {
        if amount < self.config.min_borrow {
            return Err(format!("minimum borrow is {}", self.config.min_borrow));
        }
        let required_collateral = amount * self.config.collateral_ratio;
        if collateral_amount < required_collateral {
            return Err(format!("need {:.2} collateral, provided {:.2}", required_collateral, collateral_amount));
        }

        let utilization = self.utilization_rate();
        let risk_premium = if utilization > 0.8 { 200 } else if utilization > 0.6 { 100 } else { 0 };
        let apr_bps = self.config.base_apr_bps + risk_premium;

        let now = now_ms();
        let id = self.next_loan_id.fetch_add(1, Ordering::Relaxed);
        let due = now + (self.config.max_loan_term_days as i64 * 86_400_000);
        let loan = LoanPosition {
            id,
            user_id,
            asset,
            principal: amount,
            outstanding: amount,
            apr_bps,
            collateral_asset: collateral_asset.clone(),
            collateral_amount,
            created_at: now,
            due_at: due,
            active: true,
        };
        self.loans.insert(id, loan.clone());
        self.total_loans.fetch_add((amount * 100.0) as u64, Ordering::Relaxed);
        info!("Loan #{}: {} {} with {} {} collateral at {} bps", id, amount, loan.asset, collateral_amount, collateral_asset, apr_bps);
        Ok(loan)
    }

    pub fn repay(&self, loan_id: u64, amount: f64) -> Result<LoanPosition, String> {
        let mut loan = self.loans.get_mut(&loan_id).ok_or("loan not found")?;
        if !loan.active {
            return Err("loan already closed".into());
        }
        let interest = loan.outstanding * (loan.apr_bps as f64 / 10000.0) * 30.0 / 365.0;
        loan.outstanding = (loan.outstanding - amount).max(0.0);
        self.total_interest_paid.fetch_add((interest * 100.0) as u64, Ordering::Relaxed);
        self.total_interest_earned.fetch_add((interest * 100.0) as u64, Ordering::Relaxed);
        if loan.outstanding <= 0.0 {
            loan.active = false;
        }
        let _ = self.total_loans.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
            Some(v.saturating_sub((amount.min(loan.principal) * 100.0) as u64))
        });
        Ok(loan.clone())
    }

    pub fn withdraw(&self, deposit_id: u64, amount: f64) -> Result<DepositPosition, String> {
        let mut dep = self.deposits.get_mut(&deposit_id).ok_or("deposit not found")?;
        if amount > dep.amount {
            return Err("insufficient deposit balance".into());
        }
        let interest = dep.amount * (dep.apr_bps as f64 / 10000.0);
        dep.accumulated_interest += interest;
        dep.amount -= amount;
        self.total_interest_earned.fetch_add((interest * 100.0) as u64, Ordering::Relaxed);
        self.total_deposits.fetch_sub((amount * 100.0) as u64, Ordering::Relaxed);
        Ok(dep.clone())
    }

    pub fn utilization_rate(&self) -> f64 {
        let total_dep = self.total_deposits.load(Ordering::Relaxed) as f64;
        let total_loan = self.total_loans.load(Ordering::Relaxed) as f64;
        if total_dep == 0.0 { return 0.0; }
        (total_loan / total_dep).min(1.0)
    }

    pub fn snapshot(&self) -> LendingPoolSnapshot {
        let total_dep = self.total_deposits.load(Ordering::Relaxed) as f64 / 100.0;
        let total_loan = self.total_loans.load(Ordering::Relaxed) as f64 / 100.0;
        LendingPoolSnapshot {
            total_deposits: total_dep,
            total_loans: total_loan,
            utilization_pct: self.utilization_rate() * 100.0,
            active_depositors: self.deposits.len() as u64,
            active_borrowers: self.loans.iter().filter(|l| l.active).count() as u64,
            total_interest_earned: self.total_interest_earned.load(Ordering::Relaxed) as f64 / 100.0,
            total_interest_paid: self.total_interest_paid.load(Ordering::Relaxed) as f64 / 100.0,
            base_apr_bps: self.config.base_apr_bps,
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}
