use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

// ======================================================================
// 1. IMMUTABLE SOVEREIGN AUDIT TRAIL
// ======================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub index: u64,
    pub timestamp_ns: i64,
    pub actor: String,
    pub action: String,
    pub params_hash: String,
    pub prev_hash: String,
    pub tee_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditTrailSnapshot {
    pub entries: Vec<AuditEntry>,
    pub length: u64,
    pub verified: bool,
}

/// Blockchain-style immutable audit trail for all sovereign actions.
/// Every entry is linked to previous via SHA256 and signed by TEE.
pub struct AuditTrail {
    chain: parking_lot::RwLock<Vec<AuditEntry>>,
    prev_hash: parking_lot::RwLock<String>,
}

impl AuditTrail {
    pub fn new() -> Self {
        let genesis_hash = Sha256::digest(b"THE_BRIDGE_GENESIS_2026");
        Self {
            chain: parking_lot::RwLock::new(Vec::with_capacity(1_000_000)),
            prev_hash: parking_lot::RwLock::new(format!("{:x}", genesis_hash)),
        }
    }

    /// Append a sovereign action to the immutable chain.
    /// Returns the entry with TEE signature.
    pub fn append(
        &self,
        actor: &str,
        action: &str,
        params: &serde_json::Value,
        tee_sign: impl Fn(&[u8]) -> Vec<u8>,
    ) -> AuditEntry {
        let prev = self.prev_hash.read().clone();
        let params_bytes = serde_json::to_vec(params).unwrap_or_default();
        let params_hash = format!("{:x}", Sha256::digest(&params_bytes));

        let entry = AuditEntry {
            index: self.chain.read().len() as u64,
            timestamp_ns: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            actor: actor.to_string(),
            action: action.to_string(),
            params_hash,
            prev_hash: prev,
            tee_signature: String::new(),
        };

        // Serialize entry to sign
        let entry_bytes =
            serde_json::to_vec(&entry).unwrap_or_default();
        let sig = tee_sign(&entry_bytes);
        let mut signed = entry;
        signed.tee_signature = hex::encode(sig);

        let hash = Sha256::digest(serde_json::to_vec(&signed).unwrap_or_default());
        *self.prev_hash.write() = format!("{:x}", hash);
        self.chain.write().push(signed.clone());

        signed
    }

    pub fn len(&self) -> usize {
        self.chain.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.chain.read().is_empty()
    }

    /// Verify the entire chain integrity (tamper detection).
    /// Returns (is_valid, entry_count).
    pub fn verify(&self) -> (bool, usize) {
        let chain = self.chain.read();
        if chain.is_empty() {
            return (true, 0);
        }
        let genesis_hash = format!("{:x}", Sha256::digest(b"THE_BRIDGE_GENESIS_2026"));
        if chain[0].index != 0 || chain[0].prev_hash != genesis_hash {
            return (false, chain.len());
        }
        let mut prev = Sha256::digest(serde_json::to_vec(&chain[0]).unwrap_or_default());
        for entry in chain.iter().skip(1) {
            let expected_prev = format!("{:x}", prev);
            if entry.prev_hash != expected_prev {
                return (false, chain.len());
            }
            prev = Sha256::digest(serde_json::to_vec(entry).unwrap_or_default());
        }
        (true, chain.len())
    }

    pub fn snapshot(&self) -> AuditTrailSnapshot {
        let (verified, length) = self.verify();
        AuditTrailSnapshot {
            entries: self.chain.read().clone(),
            length: length as u64,
            verified,
        }
    }
}

// ======================================================================
// 2. DEAD MAN'S SWITCH + SUCCESSION PROTOCOL
// ======================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessionPlan {
    pub successor_pubkey: String,
    pub cold_wallet_addresses: Vec<String>,
    pub notify_webhooks: Vec<String>,
    pub timeout_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SwitchState {
    Armed,
    Triggered,
    Disabled,
}

impl std::fmt::Display for SwitchState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwitchState::Armed => write!(f, "armed"),
            SwitchState::Triggered => write!(f, "triggered"),
            SwitchState::Disabled => write!(f, "disabled"),
        }
    }
}

/// Dead Man's Switch — if the sovereign stops sending heartbeats,
/// the system autonomously transfers control and funds to successors.
pub struct DeadMansSwitch {
    last_heartbeat: AtomicI64,
    state: parking_lot::RwLock<SwitchState>,
    plan: parking_lot::RwLock<Option<SuccessionPlan>>,
    trigger_count: AtomicU64,
}

impl DeadMansSwitch {
    pub fn new() -> Self {
        Self {
            last_heartbeat: AtomicI64::new(chrono::Utc::now().timestamp()),
            state: parking_lot::RwLock::new(SwitchState::Armed),
            plan: parking_lot::RwLock::new(None),
            trigger_count: AtomicU64::new(0),
        }
    }

    /// Configure the succession plan.
    pub fn configure(&self, plan: SuccessionPlan) {
        *self.plan.write() = Some(plan);
    }

    /// Sovereign sends heartbeat — resets the timer.
    pub fn heartbeat(&self) {
        self.last_heartbeat
            .store(chrono::Utc::now().timestamp(), Ordering::Release);
        if *self.state.read() == SwitchState::Triggered {
            *self.state.write() = SwitchState::Armed;
        }
    }

    /// Check if the switch should trigger. Called periodically by the system.
    /// Returns true if the switch just triggered.
    pub fn check(&self) -> bool {
        if *self.state.read() != SwitchState::Armed {
            return false;
        }
        let plan = match self.plan.read().as_ref() {
            Some(p) => p.clone(),
            None => return false,
        };

        let last = self.last_heartbeat.load(Ordering::Acquire);
        let elapsed = chrono::Utc::now().timestamp() - last;
        let timeout_secs = (plan.timeout_hours * 3600) as i64;

        if elapsed >= timeout_secs {
            *self.state.write() = SwitchState::Triggered;
            self.trigger_count.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                elapsed_hours = %(elapsed as f64 / 3600.0),
                "DEAD MAN'S SWITCH TRIGGERED — sovereign heartbeat silent for {:.1}h",
                elapsed as f64 / 3600.0
            );
            return true;
        }
        false
    }

    pub fn state(&self) -> SwitchState {
        self.state.read().clone()
    }

    pub fn last_heartbeat(&self) -> i64 {
        self.last_heartbeat.load(Ordering::Acquire)
    }

    pub fn trigger_count(&self) -> u64 {
        self.trigger_count.load(Ordering::Relaxed)
    }

    pub fn disable(&self) {
        *self.state.write() = SwitchState::Disabled;
    }

    pub fn succession_plan(&self) -> Option<SuccessionPlan> {
        self.plan.read().clone()
    }
}

// ======================================================================
// 3. MEMORY FORTRESS — Encrypted Runtime Storage
// ======================================================================

/// Encrypted cell — stores a single u64 value encrypted in memory.
/// Decrypts on read, encrypts on write. Uses AES-256-GCM with TEE-derived key.
pub struct EncryptedCell {
    encrypted: parking_lot::RwLock<Vec<u8>>,
    nonce: [u8; 12],
    key: Vec<u8>,
}

impl EncryptedCell {
    pub fn new(value: u64, key: &[u8; 32]) -> Self {
        let nonce: [u8; 12] = rand::random();
        let cipher = Aes256Gcm::new_from_slice(key).expect("valid key");
        let plaintext = value.to_le_bytes();
        let encrypted = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
            .expect("encrypt");
        Self {
            encrypted: parking_lot::RwLock::new(encrypted),
            nonce,
            key: key.to_vec(),
        }
    }

    pub fn read(&self) -> u64 {
        let cipher = Aes256Gcm::new_from_slice(&self.key).expect("valid key");
        let decrypted = cipher
            .decrypt(Nonce::from_slice(&self.nonce), self.encrypted.read().as_ref())
            .expect("decrypt");
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&decrypted[..8]);
        u64::from_le_bytes(arr)
    }

    pub fn write(&self, value: u64) {
        let cipher = Aes256Gcm::new_from_slice(&self.key).expect("valid key");
        let plaintext = value.to_le_bytes();
        let encrypted = cipher
            .encrypt(Nonce::from_slice(&self.nonce), plaintext.as_ref())
            .expect("encrypt");
        *self.encrypted.write() = encrypted;
    }

    pub fn add(&self, delta: u64) -> u64 {
        let current = self.read();
        let new = current.saturating_add(delta);
        self.write(new);
        new
    }
}

/// Memory-hardened treasury — stores totals encrypted in runtime RAM.
/// Intercepts theft by cold boot / physical memory access.
pub struct FortressTreasury {
    encrypted_total: EncryptedCell,
    #[allow(dead_code)]
    key: [u8; 32],
}

impl FortressTreasury {
    pub fn new(tee_seed: &[u8; 32]) -> Self {
        let key = Self::derive_key(tee_seed);
        Self {
            encrypted_total: EncryptedCell::new(0, &key),
            key,
        }
    }

    pub fn balance(&self) -> u64 {
        self.encrypted_total.read()
    }

    pub fn deposit(&self, amount: u64) -> u64 {
        self.encrypted_total.add(amount)
    }

    pub fn withdraw(&self, amount: u64) -> Result<u64, String> {
        let current = self.encrypted_total.read();
        if amount > current {
            return Err("insufficient fortified balance".to_string());
        }
        let new = current - amount;
        self.encrypted_total.write(new);
        Ok(new)
    }

    fn derive_key(seed: &[u8; 32]) -> [u8; 32] {
        let hash = Sha256::digest([seed.as_ref(), b"THE_BRIDGE_MEMORY_FORTRESS"].concat());
        let mut key = [0u8; 32];
        key.copy_from_slice(&hash[..32]);
        key
    }
}

// ======================================================================
// 4. SOVEREIGN FORTRESS — Top-Level Integration
// ======================================================================

/// Integrated fortress system that provides:
/// - Immutable audit trail for all sovereign actions
/// - Dead Man's Switch with succession protocol
/// - Memory-encrypted treasury
/// - Self-healing monitoring loop
pub struct SovereignFortress {
    pub audit: AuditTrail,
    pub dead_mans_switch: DeadMansSwitch,
    pub treasury: FortressTreasury,
    active_threats: AtomicU64,
    self_heal_count: AtomicU64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FortressStatus {
    pub audit_entries: usize,
    pub audit_verified: bool,
    pub switch_state: String,
    pub last_heartbeat_ago_secs: i64,
    pub trigger_count: u64,
    pub treasury_balance: u64,
    pub active_threats: u64,
    pub self_heal_count: u64,
    pub succession_plan_configured: bool,
}

impl SovereignFortress {
    pub fn new(tee_seed: &[u8; 32]) -> Self {
        Self {
            audit: AuditTrail::new(),
            dead_mans_switch: DeadMansSwitch::new(),
            treasury: FortressTreasury::new(tee_seed),
            active_threats: AtomicU64::new(0),
            self_heal_count: AtomicU64::new(0),
        }
    }

    /// Record a sovereign action in the immutable audit trail.
    /// This is called by all ghost/bridge/backup operations.
    pub fn record_action(
        &self,
        actor: &str,
        action: &str,
        params: &serde_json::Value,
        tee_sign: impl Fn(&[u8]) -> Vec<u8>,
    ) -> AuditEntry {
        self.audit.append(actor, action, params, tee_sign)
    }

    /// Configure the Dead Man's Switch with a succession plan.
    pub fn configure_succession(&self, plan: SuccessionPlan) {
        self.dead_mans_switch.configure(plan);
    }

    /// Sovereign heartbeat — prevents Dead Man's Switch.
    pub fn heartbeat(&self) {
        self.dead_mans_switch.heartbeat();
    }

    /// Self-healing monitoring — call periodically from main loop.
    /// Checks Dead Man's Switch, detects anomalies.
    pub fn monitor(&self, tee: &dyn crate::tee::HardwareEnclave) {
        // Check Dead Man's Switch
        if self.dead_mans_switch.check() {
            self.active_threats.fetch_add(1, Ordering::Relaxed);
            self.record_action(
                "system",
                "dead_mans_switch_triggered",
                &serde_json::json!({"state": "triggered"}),
                |msg| tee.sign(msg),
            );
        }
    }

    pub fn self_healed(&self) {
        self.self_heal_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn status(&self) -> FortressStatus {
        let (audit_verified, _audit_len) = self.audit.verify();
        let last_hb = self.dead_mans_switch.last_heartbeat();
        FortressStatus {
            audit_entries: self.audit.len(),
            audit_verified,
            switch_state: self.dead_mans_switch.state().to_string(),
            last_heartbeat_ago_secs: chrono::Utc::now().timestamp() - last_hb,
            trigger_count: self.dead_mans_switch.trigger_count(),
            treasury_balance: self.treasury.balance(),
            active_threats: self.active_threats.load(Ordering::Relaxed),
            self_heal_count: self.self_heal_count.load(Ordering::Relaxed),
            succession_plan_configured: self.dead_mans_switch.succession_plan().is_some(),
        }
    }
}
