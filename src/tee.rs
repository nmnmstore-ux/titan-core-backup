use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use parking_lot::RwLock;
use rand_core::OsRng;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

#[derive(Debug, Error, serde::Serialize)]
pub enum TEEError {
    #[error("enclave not initialized")]
    NotInitialized,
    #[error("enclave seal broken")]
    SealBroken,
    #[error("key rotation failed: {0}")]
    KeyRotationFailed(String),
    #[error("signing failed: {0}")]
    SigningFailed(String),
    #[error("verification failed")]
    VerificationFailed,
    #[error("seal export failed: {0}")]
    SealExportFailed(String),
    #[error("SGX DCAP not linked")]
    SgxNotLinked,
}

impl From<TEEError> for String {
    fn from(e: TEEError) -> String {
        e.to_string()
    }
}

pub trait HardwareEnclave: Send + Sync {
    fn sign(&self, msg: &[u8]) -> Vec<u8>;
    fn verify(&self, msg: &[u8], signature: &[u8]) -> bool;
    fn attest(&self) -> Result<String, TEEError>;
    fn public_key(&self) -> &[u8];
    fn signing_key_bytes(&self) -> [u8; 32];
    fn rotate_keys(&self) -> Result<String, TEEError>;
    fn measure(&self) -> Result<Vec<u8>, TEEError>;
    fn export_sealing_key(&self) -> Result<Vec<u8>, TEEError>;
}

pub struct TEEEnclave {
    initialized: AtomicBool,
    sealed: AtomicBool,
    signing_key: RwLock<SigningKey>,
    verifying_key: RwLock<VerifyingKey>,
    key_hash: RwLock<String>,
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
            signing_key: RwLock::new(signing_key),
            verifying_key: RwLock::new(verifying_key),
            key_hash: RwLock::new(key_hash),
            hardware_mode: AtomicBool::new(false),
        }
    }

    pub fn attest_report(&self) -> String {
        let vk = self.verifying_key.read();
        let hw_tag = if self.hardware_mode.load(Ordering::Relaxed) { "DCAP" } else { "SIM" };
        format!("TEE_ED25519_{}_ACTIVE_{}", hw_tag, &vk.as_bytes()[..4].iter().map(|b| format!("{:02x}", b)).collect::<String>())
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
        let sk = self.signing_key.read();
        sk.sign(msg).to_bytes().to_vec()
    }

    fn verify(&self, msg: &[u8], signature: &[u8]) -> bool {
        let sig = match Signature::from_slice(signature) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let vk = self.verifying_key.read();
        vk.verify(msg, &sig).is_ok()
    }

    fn attest(&self) -> Result<String, TEEError> {
        if !self.initialized.load(Ordering::Relaxed) {
            return Err(TEEError::NotInitialized);
        }
        let sk = self.signing_key.read();
        let vk = self.verifying_key.read();
        let msg = format!("THE_BRIDGE_TEE_ATTESTATION_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
        let signature = sk.sign(msg.as_bytes());
        let hw_tag = if self.hardware_mode.load(Ordering::Relaxed) { "SGX_DCAP" } else { "SIM" };
        Ok(format!(
            "THE_BRIDGE_TEE_{}_ATTESTATION_{}_{}",
            hw_tag,
            hex::encode(&vk.as_bytes()[..8]),
            hex::encode(signature.to_bytes())
        ))
    }

    fn public_key(&self) -> &[u8] {
        // Safety: read lock is held for the duration of this call.
        // The returned slice borrows from the RwLock guard, which is leaked here.
        // This is acceptable because the key is never deallocated in practice,
        // but for full safety we should return a copy. Using into_owned pattern:
        Box::leak(self.verifying_key.read().as_bytes().to_vec().into_boxed_slice())
    }

    fn signing_key_bytes(&self) -> [u8; 32] {
        let sk = self.signing_key.read();
        sk.to_bytes()
    }

    fn rotate_keys(&self) -> Result<String, TEEError> {
        if !self.initialized.load(Ordering::Relaxed) {
            return Err(TEEError::NotInitialized);
        }
        let old_hash = self.key_hash.read().clone();
        let mut csprng = OsRng;
        let new_signing = SigningKey::generate(&mut csprng);
        let new_verifying = new_signing.verifying_key();
        let new_hash = hex::encode(new_verifying.as_bytes());
        *self.signing_key.write() = new_signing;
        *self.verifying_key.write() = new_verifying;
        *self.key_hash.write() = new_hash.clone();
        Ok(format!("KEYS_ROTATED_ED25519_{}_{}", &old_hash[..16], &new_hash[..16]))
    }

    fn measure(&self) -> Result<Vec<u8>, TEEError> {
        let vk = self.verifying_key.read();
        let kh = self.key_hash.read();
        let mut m = vk.as_bytes().to_vec();
        m.extend_from_slice(kh.as_bytes());
        Ok(m)
    }

    fn export_sealing_key(&self) -> Result<Vec<u8>, TEEError> {
        if !self.sealed.load(Ordering::Relaxed) {
            return Err(TEEError::SealBroken);
        }
        let sk = self.signing_key.read();
        Ok(bincode::serialize(&sk.to_bytes().to_vec())
            .map_err(|e| TEEError::SealExportFailed(e.to_string()))?)
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn rotate_keys_changes_verifying_key() {
        let enclave = TEEEnclave::new();
        let pk_before = enclave.signing_key_bytes();

        let result = enclave.rotate_keys();
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert!(msg.starts_with("KEYS_ROTATED_ED25519_"));

        let pk_after = enclave.signing_key_bytes();
        assert_ne!(pk_before, pk_after);
    }

    #[test]
    fn rotate_keys_changes_key_hash() {
        let enclave = TEEEnclave::new();
        let hash_before = enclave.key_hash.read().clone();

        enclave.rotate_keys().unwrap();
        let hash_after = enclave.key_hash.read().clone();

        assert_ne!(hash_before, hash_after);
    }

    #[test]
    fn sign_verify_works_before_and_after_rotation() {
        let enclave = TEEEnclave::new();
        let msg = b"test message for signing";

        let sig_before = enclave.sign(msg);
        assert!(enclave.verify(msg, &sig_before));

        enclave.rotate_keys().unwrap();

        let sig_after = enclave.sign(msg);
        assert!(enclave.verify(msg, &sig_after));

        // Old signature should fail with new key
        assert!(!enclave.verify(msg, &sig_before));
    }

    #[test]
    fn rotate_keys_fails_when_not_initialized() {
        let enclave = TEEEnclave::new();
        enclave.initialized.store(false, Ordering::Relaxed);

        let result = enclave.rotate_keys();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "enclave not initialized");
    }

    #[test]
    fn attest_works_and_returns_formatted_string() {
        let enclave = TEEEnclave::new();
        let attestation = enclave.attest().unwrap();
        assert!(attestation.starts_with("THE_BRIDGE_TEE_SIM_ATTESTATION_"));
    }

    #[test]
    fn verify_rejects_garbage_signature() {
        let enclave = TEEEnclave::new();
        let msg = b"hello";
        assert!(!enclave.verify(msg, &[0u8; 64]));
        assert!(!enclave.verify(msg, &[]));
    }

    #[test]
    fn export_sealing_key_returns_bytes() {
        let enclave = TEEEnclave::new();
        let key = enclave.export_sealing_key().unwrap();
        assert!(!key.is_empty());
    }

    #[test]
    fn export_sealing_key_fails_when_seal_broken() {
        let enclave = TEEEnclave::new();
        enclave.sealed.store(false, Ordering::Relaxed);
        assert!(enclave.export_sealing_key().is_err());
    }
}

impl HardwareEnclave for SgxDcapEnclave {
    fn sign(&self, _msg: &[u8]) -> Vec<u8> {
        Vec::new()
    }
    fn verify(&self, _msg: &[u8], _signature: &[u8]) -> bool {
        false
    }
    fn attest(&self) -> Result<String, TEEError> {
        Err(TEEError::SgxNotLinked)
    }
    fn public_key(&self) -> &[u8] {
        &[]
    }
    fn signing_key_bytes(&self) -> [u8; 32] {
        [0u8; 32]
    }
    fn rotate_keys(&self) -> Result<String, TEEError> {
        Err(TEEError::SgxNotLinked)
    }
    fn measure(&self) -> Result<Vec<u8>, TEEError> {
        Err(TEEError::SgxNotLinked)
    }
    fn export_sealing_key(&self) -> Result<Vec<u8>, TEEError> {
        Err(TEEError::SgxNotLinked)
    }
}
