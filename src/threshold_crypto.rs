use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use parking_lot::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DKGPublicParams {
    pub threshold: usize,
    pub total_validators: usize,
    pub generator: Vec<u8>,
    pub prime: Vec<u8>,
    pub validator_pubkeys: Vec<Vec<u8>>,
    pub group_pubkey: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSecretShare {
    pub validator_id: usize,
    pub secret_share: Vec<u8>,
    pub public_poly: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedOrder {
    pub order_id: String,
    pub ciphertext: Vec<u8>,
    pub ephemeral_pubkey: Vec<u8>,
    pub commitment: Vec<u8>,
    pub proof: ZKProof,
    pub timestamp_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZKProof {
    pub challenge: Vec<u8>,
    pub response: Vec<u8>,
    pub commitment: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptionShare {
    pub validator_id: usize,
    pub share: Vec<u8>,
    pub proof: ZKProof,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDecryptionResult {
    pub orders: Vec<DecryptedOrder>,
    pub decryption_proof: ZKProof,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptedOrder {
    pub order_id: String,
    pub user_id: String,
    pub pair: String,
    pub side: String,
    pub price: u64,
    pub quantity: u64,
    pub track: u8,
    pub nonce: u64,
}

pub struct ThresholdCrypto {
    params: DKGPublicParams,
    secret_share: Mutex<Option<ValidatorSecretShare>>,
}

impl ThresholdCrypto {
    pub fn new(threshold: usize, total_validators: usize) -> Result<Self, String> {
        let generator = vec![2];
        let prime = hex::decode("ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca237327ffffffffffffffff")
            .map_err(|e| format!("DHPrime hex decode: {}", e))?;

        let mut rng = rand::thread_rng();
        let mut validator_pubkeys = Vec::new();
        for _ in 0..total_validators {
            let pk: [u8; 32] = rng.gen();
            validator_pubkeys.push(pk.to_vec());
        }

        let group_pubkey = Self::compute_group_pubkey(&validator_pubkeys);

        let params = DKGPublicParams {
            threshold,
            total_validators,
            generator,
            prime,
            validator_pubkeys,
            group_pubkey,
        };

        Ok(Self { params, secret_share: Mutex::new(None) })
    }

    fn compute_group_pubkey(pubkeys: &[Vec<u8>]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        for pk in pubkeys {
            hasher.update(pk);
        }
        hasher.finalize().to_vec()
    }

    pub fn run_dkg(&self, validator_id: usize) -> ValidatorSecretShare {
        let mut rng = rand::thread_rng();
        let secret_share: [u8; 32] = rng.gen();
        let mut public_poly = Vec::new();
        for _ in 0..self.params.threshold {
            let coeff: [u8; 32] = rng.gen();
            public_poly.push(coeff.to_vec());
        }

        let share = ValidatorSecretShare {
            validator_id,
            secret_share: secret_share.to_vec(),
            public_poly,
        };
        *self.secret_share.lock() = Some(share.clone());
        share
    }

    pub fn get_secret_share(&self) -> Option<ValidatorSecretShare> {
        self.secret_share.lock().as_ref().cloned()
    }

    pub fn get_params(&self) -> &DKGPublicParams {
        &self.params
    }

    pub fn encrypt_order(&self, order: &DecryptedOrder) -> Result<EncryptedOrder, String> {
        let mut rng = rand::thread_rng();
        let ephemeral: [u8; 32] = rng.gen();

        let plaintext = serde_json::to_vec(order)
            .map_err(|e| format!("encrypt_order serialize: {}", e))?;
        let shared_secret = Self::ecdh(&ephemeral, &self.params.group_pubkey);

        let mut ciphertext = Vec::new();
        for (i, byte) in plaintext.iter().enumerate() {
            ciphertext.push(byte ^ shared_secret[i % 32]);
        }

        let commitment = Self::commit(&ciphertext, &ephemeral);
        let proof = Self::generate_zk_proof(&ciphertext, &ephemeral, &commitment, &plaintext, &shared_secret);

        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        Ok(EncryptedOrder {
            order_id: order.order_id.clone(),
            ciphertext,
            ephemeral_pubkey: ephemeral.to_vec(),
            commitment,
            proof,
            timestamp_ns,
        })
    }

    pub fn verify_encrypted_order(&self, encrypted: &EncryptedOrder) -> bool {
        let expected_commitment = Self::commit(&encrypted.ciphertext, &encrypted.ephemeral_pubkey);
        expected_commitment == encrypted.commitment
    }

    pub fn create_decryption_share(&self, encrypted: &EncryptedOrder) -> Option<DecryptionShare> {
        let binding = self.secret_share.lock();
        let share = binding.as_ref()?;
        let shared_secret = Self::ecdh(&share.secret_share, &encrypted.ephemeral_pubkey);

        let mut plaintext = Vec::new();
        for (i, byte) in encrypted.ciphertext.iter().enumerate() {
            plaintext.push(byte ^ shared_secret[i % 32]);
        }

        let proof = Self::generate_zk_proof(&encrypted.ciphertext, &encrypted.ephemeral_pubkey, &encrypted.commitment, &plaintext, &shared_secret);

        Some(DecryptionShare {
            validator_id: share.validator_id,
            share: plaintext,
            proof,
        })
    }

    fn ecdh(privkey: &[u8], pubkey: &[u8]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(privkey);
        hasher.update(pubkey);
        hasher.finalize().to_vec()
    }

    fn commit(ciphertext: &[u8], ephemeral: &[u8]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(ciphertext);
        hasher.update(ephemeral);
        hasher.finalize().to_vec()
    }

    fn generate_zk_proof(ciphertext: &[u8], ephemeral: &[u8], commitment: &[u8], plaintext: &[u8], secret: &[u8]) -> ZKProof {
        let mut hasher = Sha256::new();
        hasher.update(ciphertext);
        hasher.update(ephemeral);
        hasher.update(commitment);
        let challenge = hasher.finalize().to_vec();

        let mut response = Vec::new();
        for (i, byte) in plaintext.iter().enumerate() {
            let mut computed = *byte;
            for j in 0..32 {
                let byte_idx = j / 8;
                let bit_idx = j % 8;
                if (challenge[byte_idx] >> bit_idx) & 1 == 1 {
                    computed ^= secret[j % 32];
                }
            }
            response.push(computed);
        }

        let mut commitment_bytes = Vec::new();
        commitment_bytes.extend_from_slice(secret);
        commitment_bytes.extend_from_slice(ephemeral);
        commitment_bytes.extend_from_slice(&challenge);

        ZKProof {
            challenge,
            response,
            commitment: commitment_bytes,
        }
    }

    pub fn combine_decryption_shares(
        &self,
        encrypted: &EncryptedOrder,
        shares: &[DecryptionShare],
    ) -> Option<BatchDecryptionResult> {
        if shares.len() < self.params.threshold {
            return None;
        }

        let mut combined = vec![0u8; encrypted.ciphertext.len()];
        for share in shares.iter().take(self.params.threshold) {
            for (i, byte) in share.share.iter().enumerate() {
                combined[i] ^= *byte;
            }
        }

        let mut orders = Vec::new();
        let mut offset = 0;
        while offset < combined.len() {
            if let Ok(order) = serde_json::from_slice::<DecryptedOrder>(&combined[offset..]) {
                orders.push(order);
                offset += 1;
            } else {
                break;
            }
        }

        let decryption_proof = Self::generate_zk_proof(
            &encrypted.ciphertext,
            &encrypted.ephemeral_pubkey,
            &encrypted.commitment,
            &shares[0].share,
            &shares[0].share, // Using share as dummy secret for proof
        );

        Some(BatchDecryptionResult {
            orders,
            decryption_proof,
        })
    }
}