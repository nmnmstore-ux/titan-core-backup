use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use dashmap::DashMap;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;
use x25519_dalek::{EphemeralSecret, PublicKey};

const SOVEREIGN_SALT: &[u8] = b"THE-BRIDGE-SOVEREIGN-2026";
const SOVEREIGN_INFO: &[u8] = b"the-bridge-sovereign-identity";
const NONCE_SIZE: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct RegulatorKeypair {
    pub public_key_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SovereignIdentity {
    pub tenant_id: Uuid,
    pub encrypted_blob: Vec<u8>,
    pub ephemeral_pubkey_hex: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SovereignIdentityRequest {
    pub tenant_id: Uuid,
    pub legal_name: String,
    pub lei: String,
    pub jurisdiction: String,
}

pub struct SovereignIdentityStore {
    identities: DashMap<Uuid, SovereignIdentity>,
    regulator_public: PublicKey,
}

impl SovereignIdentityStore {
    pub fn new(regulator_public_hex: &str) -> Result<Self, String> {
        let pub_bytes = hex::decode(regulator_public_hex)
            .map_err(|_| "invalid regulator public key hex".to_string())?;
        let arr: [u8; 32] = pub_bytes.try_into()
            .map_err(|_| "regulator public key must be 32 bytes".to_string())?;
        Ok(Self {
            identities: DashMap::new(),
            regulator_public: PublicKey::from(arr),
        })
    }

    pub fn encrypt_identity(
        &self,
        request: &SovereignIdentityRequest,
    ) -> Result<SovereignIdentity, String> {
        let ephemeral_secret = EphemeralSecret::random_from_rng(rand_core::OsRng);
        let ephemeral_public = PublicKey::from(&ephemeral_secret);
        let shared = ephemeral_secret.diffie_hellman(&self.regulator_public);
        let shared_bytes = shared.as_bytes();

        let aes_key = hkdf_extract_expand(shared_bytes, SOVEREIGN_SALT, SOVEREIGN_INFO)?;

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        let key = Key::<Aes256Gcm>::from_slice(&aes_key);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext_json = serde_json::json!({
            "legal_name": request.legal_name,
            "lei": request.lei,
            "jurisdiction": request.jurisdiction,
            "tenant_id": request.tenant_id,
        });
        let plaintext = serde_json::to_vec(&plaintext_json)
            .map_err(|e| format!("serialization error: {}", e))?;

        let encrypted = cipher.encrypt(nonce, plaintext.as_ref())
            .map_err(|e| format!("encryption failed: {}", e))?;

        let mut ciphertext = Vec::with_capacity(NONCE_SIZE + encrypted.len());
        ciphertext.extend_from_slice(&nonce_bytes);
        ciphertext.extend_from_slice(&encrypted);

        let identity = SovereignIdentity {
            tenant_id: request.tenant_id,
            encrypted_blob: ciphertext,
            ephemeral_pubkey_hex: hex::encode(ephemeral_public.as_bytes()),
            created_at: chrono::Utc::now().timestamp_millis(),
        };

        self.identities.insert(request.tenant_id, identity.clone());
        Ok(identity)
    }

    pub fn get_encrypted(&self, tenant_id: &Uuid) -> Option<SovereignIdentity> {
        self.identities.get(tenant_id).map(|r| r.clone())
    }

    pub fn decrypt_identity(
        &self,
        identity: &SovereignIdentity,
        regulator_secret_hex: &str,
    ) -> Result<serde_json::Value, String> {
        let secret_bytes = hex::decode(regulator_secret_hex)
            .map_err(|_| "invalid regulator secret hex".to_string())?;
        let arr: [u8; 32] = secret_bytes.try_into()
            .map_err(|_| "regulator secret must be 32 bytes".to_string())?;

        let ephemeral_pub_bytes = hex::decode(&identity.ephemeral_pubkey_hex)
            .map_err(|_| "invalid ephemeral pubkey hex".to_string())?;
        let pub_arr: [u8; 32] = ephemeral_pub_bytes.try_into()
            .map_err(|_| "ephemeral pubkey must be 32 bytes".to_string())?;

        let shared_secret = x25519_dalek::x25519(arr, pub_arr);

        let aes_key = hkdf_extract_expand(&shared_secret, SOVEREIGN_SALT, SOVEREIGN_INFO)?;

        if identity.encrypted_blob.len() < NONCE_SIZE {
            return Err("ciphertext too short".to_string());
        }
        let nonce_bytes = &identity.encrypted_blob[..NONCE_SIZE];
        let ciphertext = &identity.encrypted_blob[NONCE_SIZE..];

        let key = Key::<Aes256Gcm>::from_slice(&aes_key);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher.decrypt(nonce, ciphertext)
            .map_err(|e| format!("decryption failed: {}", e))?;

        serde_json::from_slice(&plaintext)
            .map_err(|e| format!("deserialization failed: {}", e))
    }

    pub fn identity_count(&self) -> usize {
        self.identities.len()
    }
}

fn hkdf_extract_expand(ikm: &[u8; 32], salt: &[u8], info: &[u8]) -> Result<[u8; 32], String> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(salt)
        .map_err(|e| format!("HMAC salt key error: {}", e))?;
    mac.update(ikm);
    let prk = mac.finalize().into_bytes();

    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&prk)
        .map_err(|e| format!("HMAC prk key error: {}", e))?;
    mac.update(info);
    mac.update(&[0x01]);
    let output = mac.finalize().into_bytes();
    let mut okm = [0u8; 32];
    okm.copy_from_slice(&output[..32]);
    Ok(okm)
}

pub fn generate_regulator_keypair_hex() -> (String, String) {
    let mut secret_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret_bytes);
    let public = x25519_dalek::x25519(secret_bytes, x25519_dalek::X25519_BASEPOINT_BYTES);
    (hex::encode(secret_bytes), hex::encode(public))
}
