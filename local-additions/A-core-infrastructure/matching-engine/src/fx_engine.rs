use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostroAccount {
    pub id: String,
    pub bank_name: String,
    pub country: String,
    pub currency: String,
    pub balance: f64,
    pub max_balance: f64,
    pub reserve_ratio: f64,
    pub active: bool,
}

impl NostroAccount {
    pub fn default_accounts() -> Vec<Self> {
        vec![
            NostroAccount { id: "JPM_USD".into(), bank_name: "JPMorgan Chase".into(), country: "US".into(), currency: "USD".into(), balance: 50_000_000.0, max_balance: 100_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "CITI_USD".into(), bank_name: "Citibank".into(), country: "US".into(), currency: "USD".into(), balance: 45_000_000.0, max_balance: 90_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "BOFA_USD".into(), bank_name: "Bank of America".into(), country: "US".into(), currency: "USD".into(), balance: 40_000_000.0, max_balance: 80_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "BARC_GBP".into(), bank_name: "Barclays".into(), country: "GB".into(), currency: "GBP".into(), balance: 30_000_000.0, max_balance: 60_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "HSBC_GBP".into(), bank_name: "HSBC".into(), country: "GB".into(), currency: "GBP".into(), balance: 35_000_000.0, max_balance: 70_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "DB_EUR".into(), bank_name: "Deutsche Bank".into(), country: "DE".into(), currency: "EUR".into(), balance: 40_000_000.0, max_balance: 80_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "BNP_EUR".into(), bank_name: "BNP Paribas".into(), country: "FR".into(), currency: "EUR".into(), balance: 35_000_000.0, max_balance: 70_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "SABB_SAR".into(), bank_name: "SABB".into(), country: "SA".into(), currency: "SAR".into(), balance: 15_000_000.0, max_balance: 30_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "NBAD_AED".into(), bank_name: "NBAD".into(), country: "AE".into(), currency: "AED".into(), balance: 18_000_000.0, max_balance: 36_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "CIB_EGP".into(), bank_name: "CIB".into(), country: "EG".into(), currency: "EGP".into(), balance: 10_000_000.0, max_balance: 20_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "HBL_PKR".into(), bank_name: "Habib Bank".into(), country: "PK".into(), currency: "PKR".into(), balance: 5_000_000.0, max_balance: 10_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "SBI_INR".into(), bank_name: "State Bank of India".into(), country: "IN".into(), currency: "INR".into(), balance: 15_000_000.0, max_balance: 30_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "GTB_NGN".into(), bank_name: "GTBank".into(), country: "NG".into(), currency: "NGN".into(), balance: 5_000_000.0, max_balance: 10_000_000.0, reserve_ratio: 1.1, active: true },
            NostroAccount { id: "KCB_KES".into(), bank_name: "KCB Bank".into(), country: "KE".into(), currency: "KES".into(), balance: 3_000_000.0, max_balance: 6_000_000.0, reserve_ratio: 1.1, active: true },
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FXRate {
    pub from: String,
    pub to: String,
    pub bid: f64,
    pub ask: f64,
    pub mid: f64,
    pub spread_bps: u64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FXQuote {
    pub from: String,
    pub to: String,
    pub amount: f64,
    pub converted_amount: f64,
    pub rate: f64,
    pub fee_cents: u64,
    pub total_cost: f64,
    pub valid_until: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FXTrade {
    pub id: u64,
    pub from_account: String,
    pub to_account: String,
    pub from_currency: String,
    pub to_currency: String,
    pub amount: f64,
    pub rate: f64,
    pub converted: f64,
    pub spread_cents: u64,
    pub fee_cents: u64,
    pub timestamp: i64,
}

pub struct FXEngine {
    accounts: Arc<DashMap<String, NostroAccount>>,
    rates: Arc<DashMap<String, FXRate>>,
    trades: Arc<DashMap<u64, FXTrade>>,
    spread_bps: AtomicU64,
    total_volume: AtomicU64,
    total_fees: AtomicU64,
    total_trades: AtomicU64,
    next_trade_id: AtomicU64,
}

impl FXEngine {
    pub fn new(spread_bps: u64) -> Self {
        let engine = Self {
            accounts: Arc::new(DashMap::new()),
            rates: Arc::new(DashMap::new()),
            trades: Arc::new(DashMap::new()),
            spread_bps: AtomicU64::new(spread_bps),
            total_volume: AtomicU64::new(0),
            total_fees: AtomicU64::new(0),
            total_trades: AtomicU64::new(0),
            next_trade_id: AtomicU64::new(1),
        };

        for acc in NostroAccount::default_accounts() {
            engine.accounts.insert(acc.id.clone(), acc);
        }
        engine.seed_rates();
        info!("FX Engine initialized with {} Nostro accounts", engine.accounts.len());
        engine
    }

    fn seed_rates(&self) {
        let base_rates: Vec<(&str, f64)> = vec![
            ("USD", 1.0), ("EUR", 1.08), ("GBP", 1.26), ("SAR", 0.27),
            ("AED", 0.27), ("EGP", 0.021), ("PKR", 0.0036), ("INR", 0.012),
            ("NGN", 0.00067), ("KES", 0.0075),
        ];
        let spread_bps = self.spread_bps.load(Ordering::Relaxed);
        let spread = spread_bps as f64 / 10000.0;
        for (from, from_rate) in &base_rates {
            for (to, to_rate) in &base_rates {
                if from == to { continue; }
                let mid = from_rate / to_rate;
                let key = format!("{}/{}", from, to);
                self.rates.insert(key.clone(), FXRate {
                    from: from.to_string(),
                    to: to.to_string(),
                    bid: mid * (1.0 - spread / 2.0),
                    ask: mid * (1.0 + spread / 2.0),
                    mid,
                    spread_bps,
                    updated_at: now_ms(),
                });
            }
        }
    }

    pub fn quote(&self, from: String, to: String, amount: f64) -> Result<FXQuote, String> {
        let key = format!("{}/{}", from, to);
        let rate = self.rates.get(&key).ok_or("rate not found")?;
        let fee_bps = self.spread_bps.load(Ordering::Relaxed) as f64;
        let fee_cents = ((amount * fee_bps / 10000.0) * 100.0) as u64;
        let converted_amount = amount * rate.ask;
        Ok(FXQuote {
            from, to, amount,
            converted_amount,
            rate: rate.ask,
            fee_cents,
            total_cost: amount + (fee_cents as f64 / 100.0),
            valid_until: now_ms() + 10_000,
        })
    }

    pub fn execute(
        &self,
        from_currency: String,
        to_currency: String,
        amount: f64,
    ) -> Result<(FXTrade, FXQuote), String> {
        let from_account = self.accounts.iter()
            .find(|a| a.currency == from_currency && a.active)
            .ok_or("no active account for source currency")?;
        let to_account = self.accounts.iter()
            .find(|a| a.currency == to_currency && a.active)
            .ok_or("no active account for target currency")?;

        if from_account.balance < amount {
            return Err(format!("insufficient balance in {}: have {}, need {}", from_account.id, from_account.balance, amount));
        }

        let from_id = from_account.id.clone();
        let to_id = to_account.id.clone();
        drop(from_account);
        drop(to_account);

        let quote = self.quote(from_currency.clone(), to_currency.clone(), amount)?;
        let converted = amount * quote.rate;

        {
            let mut src = self.accounts.get_mut(&from_id).ok_or("source lost")?;
            src.balance -= amount;
            if src.balance < 0.0 { src.balance = 0.0; }
        }
        {
            let mut dst = self.accounts.get_mut(&to_id).ok_or("dest lost")?;
            dst.balance += converted;
        }

        let trade_id = self.next_trade_id.fetch_add(1, Ordering::Relaxed);
        let trade = FXTrade {
            id: trade_id,
            from_account: from_id,
            to_account: to_id,
            from_currency,
            to_currency,
            amount,
            rate: quote.rate,
            converted,
            spread_cents: ((amount * quote.rate - amount * quote.rate) * 100.0) as u64,
            fee_cents: quote.fee_cents,
            timestamp: now_ms(),
        };

        self.total_volume.fetch_add((amount * 100.0) as u64, Ordering::Relaxed);
        self.total_fees.fetch_add(quote.fee_cents, Ordering::Relaxed);
        self.total_trades.fetch_add(1, Ordering::Relaxed);
        self.trades.insert(trade_id, trade.clone());

        info!("FX Trade #{}: {} {} → {} {} @ rate {}", trade_id, amount, trade.from_currency, converted, trade.to_currency, quote.rate);
        Ok((trade, quote))
    }

    pub fn get_rate(&self, from: &str, to: &str) -> Option<FXRate> {
        self.rates.get(&format!("{}/{}", from, to)).map(|r| r.clone())
    }

    pub fn get_account_balance(&self, id: &str) -> Option<f64> {
        self.accounts.get(id).map(|a| a.balance)
    }

    pub fn total_balance_by_currency(&self, currency: &str) -> f64 {
        self.accounts.iter()
            .filter(|a| a.currency == currency)
            .map(|a| a.balance)
            .sum()
    }

    pub fn snapshot(&self) -> FXSnapshot {
        FXSnapshot {
            accounts: self.accounts.iter().map(|a| a.clone()).collect(),
            total_volume_usd: self.total_volume.load(Ordering::Relaxed) as f64 / 100.0,
            total_fees_usd: self.total_fees.load(Ordering::Relaxed) as f64 / 100.0,
            total_trades: self.total_trades.load(Ordering::Relaxed),
            spread_bps: self.spread_bps.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FXSnapshot {
    pub accounts: Vec<NostroAccount>,
    pub total_volume_usd: f64,
    pub total_fees_usd: f64,
    pub total_trades: u64,
    pub spread_bps: u64,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}
