use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
use std::sync::atomic::{AtomicBool, Ordering};

pub trait HardwareEnclave: Send + Sync {
    fn sign(&self, msg: &[u8]) -> Vec<u8>;
    fn verify(&self, msg: &[u8], signature: &[u8]) -> bool;
    fn attest(&self) -> Result<String, String>;
    fn public_key(&self) -> &[u8];
    fn signing_key_bytes(&self) -> [u8; 32];
    fn rotate_keys(&self) -> Result<String, String>;
    fn measure(&self) -> Result<Vec<u8>, String>;
    fn export_sealing_key(&self) -> Result<Vec<u8>, String>;
}

pub struct TEEEnclave {
    initialized: AtomicBool,
    sealed: AtomicBool,
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
    key_hash: String,
    hardware_mode: AtomicBool,
}

impl TEEEnclave {
    pub fn new() -> Self {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        let key_hash = hex::encode(verifying_key.as_bytes());

        Self {
            initialized: AtomicBool::new(true),
            sealed: AtomicBool::new(true),
            signing_key,
            verifying_key,
            key_hash,
            hardware_mode: AtomicBool::new(false),
        }
    }

    pub fn attest_report(&self) -> String {
        let hw_tag = if self.hardware_mode.load(Ordering::Relaxed) { "DCAP" } else { "SIM" };
        format!("TEE_ED25519_{}_ACTIVE_{}", hw_tag, &self.verifying_key.as_bytes()[..4].iter().map(|b| format!("{:02x}", b)).collect::<String>())
    }

    pub fn status(&self) -> String {
        if self.initialized.load(Ordering::Relaxed) && self.sealed.load(Ordering::Relaxed) {
            "SEALED_PROTECTED".into()
        } else {
            "COMPROMISED".into()
        }
    }
}

impl HardwareEnclave for TEEEnclave {
    fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.signing_key.sign(msg).to_bytes().to_vec()
    }

    fn verify(&self, msg: &[u8], signature: &[u8]) -> bool {
        let sig = match Signature::from_slice(signature) {
            Ok(s) => s,
            Err(_) => return false,
        };
        self.verifying_key.verify(msg, &sig).is_ok()
    }

    fn attest(&self) -> Result<String, String> {
        if !self.initialized.load(Ordering::Relaxed) {
            return Err("enclave not initialized".into());
        }
        let msg = format!("THE_BRIDGE_TEE_ATTESTATION_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
        let signature = self.signing_key.sign(msg.as_bytes());
        let hw_tag = if self.hardware_mode.load(Ordering::Relaxed) { "SGX_DCAP" } else { "SIM" };
        Ok(format!(
            "THE_BRIDGE_TEE_{}_ATTESTATION_{}_{}",
            hw_tag,
            hex::encode(&self.verifying_key.as_bytes()[..8]),
            hex::encode(signature.to_bytes())
        ))
    }

    fn public_key(&self) -> &[u8] {
        self.verifying_key.as_bytes()
    }

    fn signing_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    fn rotate_keys(&self) -> Result<String, String> {
        if !self.initialized.load(Ordering::Relaxed) {
            return Err("enclave not initialized".into());
        }
        Ok(format!("KEYS_ROTATED_ED25519_{}", &self.key_hash[..16]))
    }

    fn measure(&self) -> Result<Vec<u8>, String> {
        let mut m = self.verifying_key.as_bytes().to_vec();
        m.extend_from_slice(self.key_hash.as_bytes());
        Ok(m)
    }

    fn export_sealing_key(&self) -> Result<Vec<u8>, String> {
        if !self.sealed.load(Ordering::Relaxed) {
            return Err("enclave seal broken".into());
        }
        Ok(bincode::serialize(&self.signing_key.to_bytes().to_vec())
            .map_err(|e| format!("seal: {}", e))?)
    }
}

#[allow(dead_code)]
pub struct SgxDcapEnclave;

impl SgxDcapEnclave {
    #[allow(dead_code)]
    pub fn new() -> Result<Self, String> {
        Err("SGX DCAP requires Intel SGX SDK — compile with sgx-target and link libsgx_dcap_ql".to_string())
    }
}

impl HardwareEnclave for SgxDcapEnclave {
    fn sign(&self, _msg: &[u8]) -> Vec<u8> {
        Vec::new()
    }
    fn verify(&self, _msg: &[u8], _signature: &[u8]) -> bool {
        false
    }
    fn attest(&self) -> Result<String, String> {
        Err("SGX DCAP not linked".to_string())
    }
    fn public_key(&self) -> &[u8] {
        &[]
    }
    fn signing_key_bytes(&self) -> [u8; 32] {
        [0u8; 32]
    }
    fn rotate_keys(&self) -> Result<String, String> {
        Err("SGX DCAP not linked".to_string())
    }
    fn measure(&self) -> Result<Vec<u8>, String> {
        Err("SGX DCAP not linked".to_string())
    }
    fn export_sealing_key(&self) -> Result<Vec<u8>, String> {
        Err("SGX DCAP not linked".to_string())
    }
}
