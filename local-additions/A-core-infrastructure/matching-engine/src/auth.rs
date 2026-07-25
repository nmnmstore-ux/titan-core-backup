use crate::cloud::tenant::{Tenant, Tier};
use crate::cloud::ApiKeyManager;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RegistrationStep {
    EmailSent,
    EmailVerified,
    KycSubmitted,
    TierSelected,
    Completed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Registration {
    pub id: Uuid,
    pub email: String,
    pub step: RegistrationStep,
    pub verify_token: String,
    pub tenant_id: Option<Uuid>,
    pub created_at: i64,
}

#[derive(Debug)]
pub struct AuthGateway {
    registrations: RwLock<HashMap<String, Registration>>,
    tokens: RwLock<HashMap<String, String>>,
}

impl AuthGateway {
    pub fn new() -> Self {
        AuthGateway {
            registrations: RwLock::new(HashMap::new()),
            tokens: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, email: &str) -> Result<Registration, String> {
        let trimmed = email.trim().to_lowercase();
        if !trimmed.contains('@') || !trimmed.contains('.') {
            return Err("invalid email address".into());
        }
        {
            let regs = self.registrations.read();
            if let Some(existing) = regs.get(&trimmed) {
                if existing.step == RegistrationStep::Completed {
                    return Err("email already registered as active tenant".into());
                }
            }
        }
        let verify_token = hex::encode(&Uuid::new_v4().as_bytes()[..8]);
        let registration = Registration {
            id: Uuid::new_v4(),
            email: trimmed.clone(),
            step: RegistrationStep::EmailSent,
            verify_token: verify_token.clone(),
            tenant_id: None,
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        {
            let mut regs = self.registrations.write();
            regs.insert(trimmed.clone(), registration.clone());
        }
        {
            let mut toks = self.tokens.write();
            toks.insert(verify_token, trimmed);
        }
        Ok(registration)
    }

    pub fn verify_email(&self, token: &str) -> Result<Registration, String> {
        let email: String = {
            let toks = self.tokens.read();
            toks.get(token).cloned().ok_or("invalid or expired token")?
        };
        let mut regs = self.registrations.write();
        let reg = regs.get_mut(&email).ok_or("registration not found")?;
        if reg.step != RegistrationStep::EmailSent {
            return Err("email already verified".into());
        }
        reg.step = RegistrationStep::EmailVerified;
        Ok(reg.clone())
    }

    pub fn submit_kyc(&self, email: &str, _lei: &str, _jurisdiction: &str) -> Result<Registration, String> {
        let mut regs = self.registrations.write();
        let reg = regs.get_mut(email).ok_or("registration not found")?;
        if reg.step != RegistrationStep::EmailVerified {
            return Err("email not verified yet".into());
        }
        reg.step = RegistrationStep::KycSubmitted;
        Ok(reg.clone())
    }

    pub fn select_tier(&self, email: &str, tier: &Tier, api_keys: &ApiKeyManager) -> Result<(Registration, String), String> {
        let mut regs = self.registrations.write();
        let reg = regs.get_mut(email).ok_or("registration not found")?;
        if reg.step != RegistrationStep::KycSubmitted {
            return Err("KYC not submitted yet".into());
        }
        let tenant = Tenant::new(email.to_string(), email.to_string(), tier.clone());
        let (_, full_key) = api_keys.create_key(tenant.id).map_err(|e| e)?;
        let tid = tenant.id;
        reg.step = RegistrationStep::Completed;
        reg.tenant_id = Some(tid);
        let clone = reg.clone();
        Ok((clone, full_key))
    }

    pub fn get_registration(&self, email: &str) -> Option<Registration> {
        let regs = self.registrations.read();
        regs.get(email).cloned()
    }

    pub fn cleanup(&self, email: &str) {
        let mut regs = self.registrations.write();
        regs.remove(email);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SignupSession {
    pub email: String,
    pub step: RegistrationStep,
    pub tenant_id: Option<Uuid>,
    pub api_key: Option<String>,
}
