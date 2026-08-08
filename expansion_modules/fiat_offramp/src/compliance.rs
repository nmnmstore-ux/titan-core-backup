use std::collections::HashMap;

use crate::{FiatConfig, FiatError, Result};

#[derive(Debug, Clone)]
pub struct ComplianceCheck {
    pub user_id: String,
    pub kyc_verified: bool,
    pub kyc_status: crate::models::KycStatus,
    pub risk_score: f64,
    pub sanctioned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KycCheck {
    pub verified: bool,
    pub status: KycLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KycLevel {
    Basic,
    Advanced,
    Premium,
}

#[derive(Debug, Clone)]
pub struct TransactionLimit {
    pub daily_remaining: rust_decimal::Decimal,
    pub monthly_remaining: rust_decimal::Decimal,
    pub tier: KycLevel,
}

pub struct ComplianceChecker {
    config: FiatConfig,
    kyc_store: std::sync::Arc<tokio::sync::RwLock<HashMap<String, KycCheck>>>,
    limits: std::sync::Arc<tokio::sync::RwLock<HashMap<String, TransactionLimit>>>,
}

impl ComplianceChecker {
    pub fn new(config: FiatConfig) -> Self {
        Self {
            config,
            kyc_store: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            limits: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    pub async fn check_kyc(&self, user_id: &str) -> Result<KycCheck> {
        let store = self.kyc_store.read().await;
        let check = store
            .get(user_id)
            .copied()
            .unwrap_or(KycCheck {
                verified: false,
                status: KycLevel::Basic,
            });

        if self.config.kyc_required && !check.verified {
            return Err(FiatError::KycNotVerified);
        }

        Ok(check)
    }

    pub async fn set_kyc_status(&self, user_id: &str, verified: bool, level: KycLevel) {
        self.kyc_store.write().await.insert(
            user_id.to_string(),
            KycCheck {
                verified,
                status: level,
            },
        );
    }

    pub async fn check_transaction_limits(
        &self,
        user_id: &str,
        amount: rust_decimal::Decimal,
        _currency: &str,
    ) -> Result<TransactionLimit> {
        let limits = self.limits.read().await;
        let limit = limits.get(user_id).cloned().unwrap_or(TransactionLimit {
            daily_remaining: rust_decimal::Decimal::ZERO,
            monthly_remaining: rust_decimal::Decimal::ZERO,
            tier: KycLevel::Basic,
        });

        drop(limits);

        if amount > limit.daily_remaining {
            return Err(FiatError::LimitExceeded);
        }

        if amount > limit.monthly_remaining {
            return Err(FiatError::LimitExceeded);
        }

        Ok(limit)
    }

    pub async fn check_sanctions(&self, wallet_address: &str) -> Result<bool> {
        let sanctioned_addresses = [
            "0x0000000000000000000000000000000000000000",
            "0x000000000000000000000000000000000000dEaD",
        ];

        for sanctioned in &sanctioned_addresses {
            if wallet_address.to_lowercase() == sanctioned.to_lowercase() {
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub async fn full_check(
        &self,
        user_id: &str,
        wallet_address: &str,
        amount: rust_decimal::Decimal,
        currency: &str,
    ) -> Result<ComplianceCheck> {
        if self.config.kyc_required {
            self.check_kyc(user_id).await?;
        }

        let sanctioned = self.check_sanctions(wallet_address).await?;
        if sanctioned {
            return Err(FiatError::Sanctioned);
        }

        let _limits = self.check_transaction_limits(user_id, amount, currency).await?;

        Ok(ComplianceCheck {
            user_id: user_id.to_string(),
            kyc_verified: true,
            kyc_status: crate::models::KycStatus::Verified,
            risk_score: 0.0,
            sanctioned,
        })
    }
}
