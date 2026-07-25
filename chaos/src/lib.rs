use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;
pub type Address = [u8; 20];
pub type H256 = [u8; 32];
pub type U256 = u128;
#[derive(Debug, Clone)]
pub enum ChaosError {
    ExperimentFailed(String), InvalidSignature(String), TimelockActive(u64),
    NotAuthorized(String), OraclePoisoningDetected(String), RecoveryFailed(String), SerializationError(String),
}
impl std::fmt::Display for ChaosError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self { Self::ExperimentFailed(m) => write!(f,"Exp fail: {}",m), Self::InvalidSignature(m) => write!(f,"Bad sig: {}",m), Self::TimelockActive(s) => write!(f,"Timelock {}s",s), Self::NotAuthorized(m) => write!(f,"No auth: {}",m), Self::OraclePoisoningDetected(m) => write!(f,"Poison: {}",m), Self::RecoveryFailed(m) => write!(f,"Recov: {}",m), Self::SerializationError(m) => write!(f,"Ser: {}",m), }
    }
}
impl std::error::Error for ChaosError {}
impl From<serde_json::Error> for ChaosError { fn from(e: serde_json::Error) -> Self { Self::SerializationError(e.to_string()) } }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceFeedSnapshot { pub feed_id: H256, pub price: U256, pub timestamp: u64, pub source: String, pub block_number: u64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricePoisoningResult { pub feed_id: H256, pub original_price: U256, pub poisoned_price: U256, pub deviation_bps: u64, pub timestamp: u64, pub success: bool }
pub struct OraclePricePoisoning {
    feed_id: H256, normal: U256, dev_bps: u64,
    poisoned: Arc<RwLock<bool>>, cur_dev: Arc<RwLock<u64>>, hist: Arc<RwLock<VecDeque<PricePoisoningResult>>>,
}
impl OraclePricePoisoning {
    pub fn new(feed_id: H256, normal: U256, dev_bps: u64) -> Self { Self { feed_id, normal, dev_bps, poisoned: Arc::new(RwLock::new(false)), cur_dev: Arc::new(RwLock::new(0)), hist: Arc::new(RwLock::new(VecDeque::with_capacity(100))) } }
    pub async fn poison(&self, dev: Option<u64>) -> Result<PricePoisoningResult, ChaosError> {
        let d = dev.unwrap_or(self.dev_bps); if d > 5000 { return Err(ChaosError::ExperimentFailed("max 5000".into())); }
        let pp = if rand::thread_rng().gen() { self.normal + self.normal * d as u128 / 10000 } else { self.normal.saturating_sub(self.normal * d as u128 / 10000) };
        *self.poisoned.write() = true; *self.cur_dev.write() = d;
        let r = PricePoisoningResult { feed_id: self.feed_id, original_price: self.normal, poisoned_price: pp, deviation_bps: d, timestamp: Utc::now().timestamp() as u64, success: true };
        self.hist.write().push_back(r.clone()); if self.hist.read().len() > 100 { self.hist.write().pop_front(); } Ok(r)
    }
    pub async fn restore(&self) -> Result<(), ChaosError> { *self.poisoned.write() = false; *self.cur_dev.write() = 0; Ok(()) }
    pub fn is_poisoned(&self) -> bool { *self.poisoned.read() }
    pub fn deviation(&self) -> u64 { *self.cur_dev.read() }
    pub async fn detect(feeds: &[PriceFeedSnapshot], thresh: u64) -> Result<bool, ChaosError> {
        if feeds.len() < 2 { return Ok(false); }
        Ok(feeds.windows(2).filter_map(|w| if w[0].price == 0 { None } else { Some(if w[1].price > w[0].price { (w[1].price - w[0].price) * 10000 / w[0].price } else { (w[0].price - w[1].price) * 10000 / w[0].price }) }).max().unwrap_or(0) > thresh as u128)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction { FreezeAll, EmergencyWithdraw, CircuitBreakAll, ResetToCheckpoint, HaltTrading }
impl RecoveryAction {
    pub fn desc(&self) -> &'static str { match self { Self::FreezeAll => "Freeze", Self::EmergencyWithdraw => "Emergency", Self::CircuitBreakAll => "CircuitBreak", Self::ResetToCheckpoint => "Reset", Self::HaltTrading => "Halt" } }
    pub fn byte(&self) -> u8 { match self { Self::FreezeAll=>0, Self::EmergencyWithdraw=>1, Self::CircuitBreakAll=>2, Self::ResetToCheckpoint=>3, Self::HaltTrading=>4 } }
    pub fn from_byte(b: u8) -> Option<Self> { match b { 0=>Some(Self::FreezeAll),1=>Some(Self::EmergencyWithdraw),2=>Some(Self::CircuitBreakAll),3=>Some(Self::ResetToCheckpoint),4=>Some(Self::HaltTrading),_=>None } }
}
pub struct SignedRecovery { pub id: Uuid, pub action: RecoveryAction, pub sigs: Vec<[u8;64]>, pub signers: Vec<Address>, pub nonce: u64, pub ts: u64, pub timelock: u64 }
#[derive(Clone)] pub struct RecoveryResult { pub ok: bool, pub action: RecoveryAction, pub by: Vec<Address>, pub at: u64, pub effects: Vec<String> }
pub struct UnilateralRecovery {
    signers: Arc<RwLock<Vec<Address>>>, thresh: usize, cool: Duration,
    last: Arc<RwLock<Option<Instant>>>, nonces: Arc<RwLock<HashMap<Address,u64>>>, hist: Arc<RwLock<VecDeque<RecoveryResult>>>,
}
impl UnilateralRecovery {
    pub fn new(s: Address, t: usize, c: Duration) -> Self { Self { signers: Arc::new(RwLock::new(vec![s])), thresh: t, cool: c, last: Arc::new(RwLock::new(None)), nonces: Arc::new(RwLock::new(HashMap::new())), hist: Arc::new(RwLock::new(VecDeque::with_capacity(50))) } }
    pub fn hash(action: &RecoveryAction, nonce: u64, ts: u64) -> H256 {
        let mut h = Sha256::new(); h.update([action.byte(), nonce as u8, ts as u8]); let mut out=[0u8;32]; out.copy_from_slice(&h.finalize()); out
    }
    pub fn sign_hash(hash: &H256, key: &[u8;32]) -> [u8;64] { SigningKey::from_bytes(key).sign(hash).to_bytes() }
    pub fn verify_hash(hash: &H256, sig: &[u8;64], s: &Address) -> bool { let mut pk_bytes = [0u8;32]; pk_bytes[..20].copy_from_slice(s); VerifyingKey::from_bytes(&pk_bytes).map(|pk| { pk.verify(hash, &Signature::from_bytes(sig)).is_ok() }).unwrap_or(false) }
    pub async fn sign(&self, action: RecoveryAction, key: &[u8;32]) -> Result<SignedRecovery, ChaosError> {
        let n = { let mut m = self.nonces.write(); let e = m.entry(Address::default()).or_insert(0); *e += 1; *e };
        let ts = Utc::now().timestamp() as u64; let hash = Self::hash(&action, n, ts); let sig = Self::sign_hash(&hash, key);
        let mut a = [0u8;20]; a.copy_from_slice(&SigningKey::from_bytes(key).verifying_key().to_bytes()[..20]);
        Ok(SignedRecovery { id: Uuid::new_v4(), action, sigs: vec![sig], signers: vec![a], nonce: n, ts, timelock: ts+3600 })
    }
    pub async fn execute(&self, r: &SignedRecovery) -> Result<RecoveryResult, ChaosError> {
        let now = Utc::now().timestamp() as u64; if now < r.timelock { return Err(ChaosError::TimelockActive(r.timelock-now)); }
        if r.sigs.len() < self.thresh { return Err(ChaosError::NotAuthorized("need sigs".into())); }
        if let Some(l) = *self.last.read() { if l.elapsed() < self.cool { return Err(ChaosError::TimelockActive((self.cool-l.elapsed()).as_secs())); } }
        let h = Self::hash(&r.action, r.nonce, r.ts); for (s, a) in r.sigs.iter().zip(r.signers.iter()) { if !Self::verify_hash(&h, s, a) { return Err(ChaosError::InvalidSignature("bad".into())); } }
        *self.last.write() = Some(Instant::now()); let res = RecoveryResult { ok: true, action: r.action, by: r.signers.clone(), at: now, effects: vec![format!("{:?} ok", r.action)] };
        self.hist.write().push_back(res.clone()); Ok(res)
    }
    pub fn verify(&self, r: &SignedRecovery) -> Result<bool, ChaosError> { let h = Self::hash(&r.action, r.nonce, r.ts); Ok(r.sigs.iter().zip(r.signers.iter()).all(|(s,a)| Self::verify_hash(&h,s,a))) }
    pub async fn add_signer(&mut self, s: Address) -> Result<(), ChaosError> { let mut w = self.signers.write(); if w.len()>=10 { return Err(ChaosError::NotAuthorized("max".into())); } if !w.contains(&s) { w.push(s); } Ok(()) }
}
#[derive(Clone)] pub struct TimeLockedItem { pub id: Uuid, pub target: Address, pub data: Vec<U256>, pub release: u64, pub done: bool, pub created: u64 }
pub struct TimeLockResult { pub id: Uuid, pub ok: bool, pub at: u64, pub err: Option<String> }
pub struct TimeLockedTransaction { items: Arc<RwLock<Vec<TimeLockedItem>>> }
impl TimeLockedTransaction {
    pub fn new() -> Self { Self { items: Arc::new(RwLock::new(Vec::new())) } }
    pub async fn schedule(&self, t: Address, d: Vec<U256>, r: u64) -> Result<Uuid, ChaosError> {
        let now = Utc::now().timestamp() as u64; if r <= now { return Err(ChaosError::ExperimentFailed("future".into())); }
        let item = TimeLockedItem { id: Uuid::new_v4(), target: t, data: d, release: r, done: false, created: now }; let id = item.id; self.items.write().push(item); Ok(id)
    }
    pub async fn cancel(&mut self, id: Uuid) -> Result<bool, ChaosError> { let mut w = self.items.write(); if let Some(p) = w.iter().position(|i| i.id==id && !i.done) { w.remove(p); return Ok(true); } Ok(false) }
    pub async fn exec_ready(&mut self) -> Result<Vec<TimeLockResult>, ChaosError> {
        let now = Utc::now().timestamp() as u64; let mut res = Vec::new(); let mut ids = Vec::new();
        for i in self.items.read().iter() { if !i.done && now >= i.release { res.push(TimeLockResult{id:i.id,ok:true,at:now,err:None}); ids.push(i.id); } }
        for id in &ids { if let Some(i) = self.items.write().iter_mut().find(|x| x.id==*id) { i.done = true; } } self.items.write().retain(|i| !i.done); Ok(res)
    }
    pub async fn pending(&self) -> Vec<TimeLockedItem> { let now = Utc::now().timestamp() as u64; self.items.read().iter().filter(|i| !i.done && i.release > now).cloned().collect() }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChaosType { OraclePoisoning, NetworkPartition, LatencyInjection, OrderFlood, GasPriceSpike }
#[derive(Clone)] pub enum ExpStatus { Pending, Running, Done(bool), Failed(String), RolledBack }
#[derive(Clone)] pub struct ChaosExpResult { pub id: Uuid, pub name: String, pub typ: ChaosType, pub ok: bool, pub dur: Duration, pub effects: Vec<String> }
pub struct ChaosExperiment {
    id: Uuid, name: String, typ: ChaosType, params: HashMap<String,String>,
    status: Arc<RwLock<ExpStatus>>, start: Arc<RwLock<Option<Instant>>>, dur: Duration, oracle: Option<Arc<OraclePricePoisoning>>,
}
impl ChaosExperiment {
    pub fn new(name: &str, t: ChaosType, p: HashMap<String,String>, o: Option<Arc<OraclePricePoisoning>>) -> Self {
        let d = p.get("duration_secs").and_then(|s| s.parse().ok()).unwrap_or(60u64);
        Self { id: Uuid::new_v4(), name: name.into(), typ: t, params: p, status: Arc::new(RwLock::new(ExpStatus::Pending)), start: Arc::new(RwLock::new(None)), dur: Duration::from_secs(d), oracle: o }
    }
    pub async fn run(&mut self) -> Result<ChaosExpResult, ChaosError> {
        *self.status.write() = ExpStatus::Running; *self.start.write() = Some(Instant::now()); let mut effects = Vec::new();
        if let ChaosType::OraclePoisoning = self.typ { if let Some(ref o) = self.oracle { let dev = self.params.get("deviation_bps").and_then(|s| s.parse().ok()).unwrap_or(100); let r = o.poison(Some(dev)).await?; effects.push(format!("poison {}->{}", r.original_price, r.poisoned_price)); } }
        tokio::time::sleep(self.dur).await; if let Some(ref o) = self.oracle { o.restore().await?; effects.push("restored".into()); }
        let elapsed = self.start.read().map(|s| s.elapsed()).unwrap_or_default(); *self.status.write() = ExpStatus::Done(true);
        Ok(ChaosExpResult { id: self.id, name: self.name.clone(), typ: self.typ, ok: true, dur: elapsed, effects })
    }
    pub async fn rollback(&mut self) -> Result<(), ChaosError> { if let Some(ref o) = self.oracle { o.restore().await?; } *self.status.write() = ExpStatus::RolledBack; Ok(()) }
    pub fn status(&self) -> ExpStatus { match &*self.status.read() { ExpStatus::Pending => ExpStatus::Pending, ExpStatus::Running => ExpStatus::Running, ExpStatus::Done(b) => ExpStatus::Done(*b), ExpStatus::Failed(s) => ExpStatus::Failed(s.clone()), ExpStatus::RolledBack => ExpStatus::RolledBack } }
}
pub enum OracleStat { Healthy, Poisoned(u64), Unknown }
pub struct SystemHealth { pub ok: bool, pub oracle: OracleStat, pub active: usize, pub last: u64 }
#[allow(dead_code)]
pub struct ChaosEngine {
    oracle: Arc<OraclePricePoisoning>, recovery: Arc<RwLock<UnilateralRecovery>>,
    timelock: Arc<RwLock<TimeLockedTransaction>>, metrics: Arc<RwLock<u64>>,
}
impl ChaosEngine {
    pub fn new(o: OraclePricePoisoning, r: UnilateralRecovery, t: TimeLockedTransaction) -> Self { Self { oracle: Arc::new(o), recovery: Arc::new(RwLock::new(r)), timelock: Arc::new(RwLock::new(t)), metrics: Arc::new(RwLock::new(0)) } }
    pub async fn run_exp(&self, name: &str, typ: ChaosType, params: HashMap<String,String>) -> Result<ChaosExpResult, ChaosError> {
        let mut exp = ChaosExperiment::new(name, typ, params, Some(self.oracle.clone())); let r = exp.run().await?; *self.metrics.write() += 1; Ok(r)
    }
    pub async fn emergency(&self, a: RecoveryAction, k: &[u8;32]) -> Result<RecoveryResult, ChaosError> { let s = self.recovery.read().sign(a, k).await?; self.recovery.write().execute(&s).await }
    pub async fn health(&self) -> SystemHealth { SystemHealth { ok: !self.oracle.is_poisoned(), oracle: if self.oracle.is_poisoned() { OracleStat::Poisoned(self.oracle.deviation()) } else { OracleStat::Healthy }, active: 0, last: Utc::now().timestamp() as u64 } }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test] async fn test_poison() { let o = OraclePricePoisoning::new([0u8;32], 1000, 100); assert!(!o.is_poisoned()); o.poison(None).await.unwrap(); assert!(o.is_poisoned()); }
    #[tokio::test] async fn test_poison_restore() { let o = OraclePricePoisoning::new([1u8;32], 500, 200); o.poison(None).await.unwrap(); o.restore().await.unwrap(); assert!(!o.is_poisoned()); }
    #[tokio::test] async fn test_detect() { let now = Utc::now().timestamp() as u64; let f = vec![PriceFeedSnapshot{feed_id:[2u8;32],price:1000,timestamp:now-10,source:"c".into(),block_number:100}, PriceFeedSnapshot{feed_id:[2u8;32],price:2000,timestamp:now,source:"c".into(),block_number:101}]; assert!(OraclePricePoisoning::detect(&f, 50).await.unwrap()); }
    #[tokio::test] async fn test_recovery() { let r = UnilateralRecovery::new([3u8;20], 1, Duration::from_secs(60)); let s = r.sign(RecoveryAction::FreezeAll, &[4u8;32]).await.unwrap(); assert!(r.verify(&s).unwrap()); }
    #[tokio::test] async fn test_timelock() { let tl = TimeLockedTransaction::new(); let id = tl.schedule([5u8;20], vec![100], Utc::now().timestamp() as u64 + 3600).await.unwrap(); assert!(tl.pending().await.len() == 1); }
    #[tokio::test] async fn test_exp() { let o = Arc::new(OraclePricePoisoning::new([6u8;32], 1000, 50)); let mut p = HashMap::new(); p.insert("duration_secs".into(), "1".into()); p.insert("deviation_bps".into(), "100".into()); let mut e = ChaosExperiment::new("t", ChaosType::OraclePoisoning, p, Some(o)); assert!(e.run().await.unwrap().ok); }
    #[tokio::test] async fn test_engine() { let e = ChaosEngine::new(OraclePricePoisoning::new([7u8;32],1000,50), UnilateralRecovery::new([8u8;20],1,Duration::from_secs(60)), TimeLockedTransaction::new()); let mut p = HashMap::new(); p.insert("duration_secs".into(),"1".into()); assert!(e.run_exp("t",ChaosType::OraclePoisoning,p).await.unwrap().ok); }
    #[tokio::test] async fn test_health() { let e = ChaosEngine::new(OraclePricePoisoning::new([9u8;32],1000,50), UnilateralRecovery::new([10u8;20],1,Duration::from_secs(60)), TimeLockedTransaction::new()); let h = e.health().await; assert!(h.ok); }
    #[tokio::test] async fn test_recovery_exec() { let r = UnilateralRecovery::new([11u8;20], 1, Duration::from_secs(0)); let mut s = r.sign(RecoveryAction::HaltTrading, &[12u8;32]).await.unwrap(); s.timelock = 0; assert!(r.execute(&s).await.unwrap().ok); }
}
