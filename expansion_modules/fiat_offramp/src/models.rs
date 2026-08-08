use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum KycStatus {
    NotStarted,
    InProgress,
    Verified,
    Rejected,
    Expired,
}

impl KycStatus {
    pub fn is_verified(&self) -> bool {
        matches!(self, KycStatus::Verified)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            KycStatus::NotStarted => "not_started",
            KycStatus::InProgress => "in_progress",
            KycStatus::Verified => "verified",
            KycStatus::Rejected => "rejected",
            KycStatus::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRequest {
    pub user_id: String,
    pub amount: Decimal,
    pub currency: String,
    pub method: DepositMethod,
    pub destination_wallet: String,
    pub chain_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DepositMethod {
    BankTransfer,
    Card,
    Wallet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositResponse {
    pub deposit_id: String,
    pub external_reference: String,
    pub status: crate::models::TransactionStatus,
    pub net_amount: Decimal,
    pub fees: Decimal,
    pub payment_url: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalRequest {
    pub user_id: String,
    pub amount: Decimal,
    pub currency: String,
    pub method: WithdrawalMethod,
    pub source_wallet: String,
    pub chain_id: String,
    pub bank_account_id: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WithdrawalMethod {
    BankTransfer,
    VirtualCard,
    PhysicalCard,
    MobileWallet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalResponse {
    pub withdrawal_id: String,
    pub external_reference: String,
    pub status: TransactionStatus,
    pub net_amount: Decimal,
    pub fees: Decimal,
    pub tracking_number: Option<String>,
    pub estimated_delivery: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardRequest {
    pub user_id: String,
    pub wallet_address: String,
    pub chain_id: String,
    pub initial_load: Decimal,
    pub currency: String,
    pub contactless: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardResponse {
    pub card_id: String,
    pub masked_pan: String,
    pub token: String,
    pub status: crate::models::CardStatus,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardActivationRequest {
    pub card_id: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardActivationResponse {
    pub success: bool,
    pub card_id: String,
    pub status: crate::models::CardStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub id: String,
    pub user_id: String,
    pub status: TransactionStatus,
    pub transaction_type: TransactionType,
    pub amount: Decimal,
    pub currency: String,
    pub fees: Decimal,
    pub source: String,
    pub destination: String,
    pub external_reference: String,
    pub idempotency_key: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    CardLoad,
    CardSpend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionHistoryResponse {
    pub transactions: Vec<TransactionRecord>,
    pub total: usize,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub event_type: String,
    pub data: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CardStatus {
    Pending,
    Active,
    Frozen,
    Cancelled,
    Expired,
}

impl CardStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CardStatus::Pending => "pending",
            CardStatus::Active => "active",
            CardStatus::Frozen => "frozen",
            CardStatus::Cancelled => "cancelled",
            CardStatus::Expired => "expired",
        }
    }
}
