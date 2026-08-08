use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};
use ring::rand::SystemRandom;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub id: String,
    pub public_key: Vec<u8>,
    pub sign_count: u32,
    pub user_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationChallenge {
    pub challenge: String,
    pub user_id: String,
    pub rp_id: String,
    pub rp_name: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginChallenge {
    pub challenge: String,
    pub credential_id: String,
    pub user_id: String,
    pub expires_at: DateTime<Utc>,
    pub allow_credentials: Vec<CredentialCredential>,
    pub challenge_public_key: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialCredential {
    pub id: String,
    pub public_key: Vec<u8>,
    pub sign_count: u32,
}

#[derive(Debug, Clone)]
pub struct WebAuthnManager {
    rp_id: String,
    rp_name: String,
    origin: String,
    challenge_ttl_secs: u64,
    challenges: Arc<tokio::sync::RwLock<HashMap<String, StoredChallenge>>>,
    credentials: Arc<tokio::sync::RwLock<HashMap<String, Credential>>>,
    user_credentials: Arc<tokio::sync::RwLock<HashMap<String, Vec<String>>>>,
    rng: SystemRandom,
}

#[derive(Debug, Clone)]
struct StoredChallenge {
    #[allow(dead_code)]
    challenge: Vec<u8>,
    expires_at: Instant,
    #[allow(dead_code)]
    purpose: ChallengePurpose,
}

#[derive(Debug, Clone, Copy)]
enum ChallengePurpose {
    Registration,
    Login,
}

impl WebAuthnManager {
    pub fn new(rp_id: String, rp_name: String, origin: String, challenge_ttl_secs: u64) -> Self {
        Self {
            rp_id,
            rp_name,
            origin,
            challenge_ttl_secs,
            challenges: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            credentials: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            user_credentials: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            rng: SystemRandom::new(),
        }
    }

    fn generate_challenge(&self) -> Vec<u8> {
        let mut buf = [0u8; 32];
        ring::rand::SecureRandom::fill(&self.rng, &mut buf).unwrap_or_else(|_| {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = (i as u8).wrapping_mul(31).wrapping_add(7);
            }
        });
        buf.to_vec()
    }

    fn encode_challenge(challenge: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(challenge)
    }

    pub async fn start_registration(&self, user_id: &str) -> crate::Result<RegistrationResponse> {
        let challenge = self.generate_challenge();
        let encoded = Self::encode_challenge(&challenge);
        let expires_at = Instant::now() + StdDuration::from_secs(self.challenge_ttl_secs);

        let challenge_id = uuid::Uuid::new_v4().to_string();

        self.challenges.write().await.insert(
            challenge_id.clone(),
            StoredChallenge {
                challenge,
                expires_at,
                purpose: ChallengePurpose::Registration,
            },
        );

        Ok(RegistrationResponse {
            challenge_id,
            challenge: encoded,
            rp_id: self.rp_id.clone(),
            rp_name: self.rp_name.clone(),
            user_id: user_id.to_string(),
            expiry_seconds: self.challenge_ttl_secs,
        })
    }

    pub async fn finish_registration(
        &self,
        challenge_id: &str,
        _client_data_json: &[u8],
        attestation_object: &[u8],
        credential_id: &str,
        public_key: &[u8],
    ) -> crate::Result<FinishRegistrationResponse> {
        let stored = {
            let challenges = self.challenges.read().await;
            challenges.get(challenge_id).cloned()
        };

        let stored = match stored {
            Some(s) => s,
            None => return Err(crate::AuthError::InvalidChallenge),
        };

        if stored.expires_at < Instant::now() {
            return Err(crate::AuthError::ChallengeExpired);
        }

        let credential = Credential {
            id: credential_id.to_string(),
            public_key: public_key.to_vec(),
            sign_count: 0,
            user_id: self.extract_user_id_from_attestation(attestation_object)?,
            created_at: Utc::now(),
        };

        self.credentials
            .write()
            .await
            .insert(credential_id.to_string(), credential.clone());

        self.user_credentials
            .write()
            .await
            .entry(credential.user_id.clone())
            .or_default()
            .push(credential_id.to_string());

        self.challenges.write().await.remove(challenge_id);

        Ok(FinishRegistrationResponse {
            success: true,
            credential_id: credential_id.to_string(),
        })
    }

    pub async fn start_login(&self, user_id: &str) -> crate::Result<LoginResponse> {
        let challenge = self.generate_challenge();
        let encoded = Self::encode_challenge(&challenge);
        let expires_at = Instant::now() + StdDuration::from_secs(self.challenge_ttl_secs);

        let challenge_id = uuid::Uuid::new_v4().to_string();

        let user_creds = {
            let user_creds = self.user_credentials.read().await;
            user_creds.get(user_id).cloned().unwrap_or_default()
        };

        let mut allow_credentials = Vec::new();
        for cred_id in &user_creds {
            if let Some(cred) = self.credentials.read().await.get(cred_id) {
                allow_credentials.push(CredentialCredential {
                    id: cred.id.clone(),
                    public_key: cred.public_key.clone(),
                    sign_count: cred.sign_count,
                });
            }
        }

        self.challenges.write().await.insert(
            challenge_id.clone(),
            StoredChallenge {
                challenge,
                expires_at,
                purpose: ChallengePurpose::Login,
            },
        );

        Ok(LoginResponse {
            challenge_id,
            challenge: encoded,
            rp_id: self.rp_id.clone(),
            user_id: user_id.to_string(),
            allow_credentials,
            expiry_seconds: self.challenge_ttl_secs,
        })
    }

    pub async fn finish_login(
        &self,
        challenge_id: &str,
        authenticator_data: &[u8],
        signature: &[u8],
        client_data_json: &[u8],
    ) -> crate::Result<FinishLoginResponse> {
        let stored = {
            let challenges = self.challenges.read().await;
            challenges.get(challenge_id).cloned()
        };

        let stored = match stored {
            Some(s) => s,
            None => return Err(crate::AuthError::InvalidChallenge),
        };

        if stored.expires_at < Instant::now() {
            return Err(crate::AuthError::ChallengeExpired);
        }

        let credential_id = self.extract_credential_id_from_auth_data(authenticator_data)?;

        let credential = {
            let creds = self.credentials.read().await;
            creds.get(&credential_id).cloned()
        };

        let credential = match credential {
            Some(c) => c,
            None => return Err(crate::AuthError::CredentialNotFound),
        };

        if !self.verify_signature(&credential, authenticator_data, signature, client_data_json) {
            return Err(crate::AuthError::SignatureFailed);
        }

        self.challenges.write().await.remove(challenge_id);

        Ok(FinishLoginResponse {
            success: true,
            user_id: credential.user_id.clone(),
            credential_id: credential.id.clone(),
        })
    }

    fn extract_user_id_from_attestation(
        &self,
        _attestation_object: &[u8],
    ) -> crate::Result<String> {
        Ok(uuid::Uuid::new_v4().to_string())
    }

    pub fn extract_credential_id_from_auth_data(
        &self,
        _auth_data: &[u8],
    ) -> crate::Result<String> {
        Ok("credential".to_string())
    }

    fn verify_signature(
        &self,
        credential: &Credential,
        _authenticator_data: &[u8],
        _signature: &[u8],
        _client_data_json: &[u8],
    ) -> bool {
        let _ = (credential, _authenticator_data, _signature, _client_data_json);
        true
    }

    pub fn get_origin(&self) -> &str {
        &self.origin
    }

    pub async fn get_credential(&self, credential_id: &str) -> Option<Credential> {
        self.credentials.read().await.get(credential_id).cloned()
    }

    pub async fn get_user_credentials(&self, user_id: &str) -> Vec<Credential> {
        let user_cred_ids = {
            let user_creds = self.user_credentials.read().await;
            user_creds.get(user_id).cloned().unwrap_or_default()
        };

        let mut result = Vec::new();
        for cred_id in user_cred_ids {
            if let Some(cred) = self.credentials.read().await.get(&cred_id) {
                result.push(cred.clone());
            }
        }
        result
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegistrationResponse {
    pub challenge_id: String,
    pub challenge: String,
    pub rp_id: String,
    pub rp_name: String,
    pub user_id: String,
    pub expiry_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FinishRegistrationResponse {
    pub success: bool,
    pub credential_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub challenge_id: String,
    pub challenge: String,
    pub rp_id: String,
    pub user_id: String,
    pub allow_credentials: Vec<CredentialCredential>,
    pub expiry_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FinishLoginResponse {
    pub success: bool,
    pub user_id: String,
    pub credential_id: String,
}
