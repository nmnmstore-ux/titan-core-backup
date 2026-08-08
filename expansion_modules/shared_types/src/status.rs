use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum TxStatus {
    Pending,
    Confirmed,
    Failed,
    Cancelled,
    Reverted,
}

impl TxStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TxStatus::Confirmed | TxStatus::Failed | TxStatus::Cancelled | TxStatus::Reverted
        )
    }

    pub fn is_successful(&self) -> bool {
        matches!(self, TxStatus::Confirmed)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TxStatus::Pending => "pending",
            TxStatus::Confirmed => "confirmed",
            TxStatus::Failed => "failed",
            TxStatus::Cancelled => "cancelled",
            TxStatus::Reverted => "reverted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum KycStatus {
    NotStarted,
    InProgress,
    Verified,
    Rejected,
    Expired,
}

impl KycStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            KycStatus::Verified | KycStatus::Rejected | KycStatus::Expired
        )
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum CardStatus {
    Pending,
    Active,
    Frozen,
    Cancelled,
    Expired,
}

impl CardStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, CardStatus::Cancelled | CardStatus::Expired)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, CardStatus::Active)
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum OrderStatus {
    Open,
    Filled,
    PartiallyFilled,
    Cancelled,
    Expired,
}

impl OrderStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderStatus::Filled | OrderStatus::Cancelled | OrderStatus::Expired
        )
    }

    pub fn is_successful(&self) -> bool {
        matches!(self, OrderStatus::Filled)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Open => "open",
            OrderStatus::Filled => "filled",
            OrderStatus::PartiallyFilled => "partially_filled",
            OrderStatus::Cancelled => "cancelled",
            OrderStatus::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum SessionStatus {
    Active,
    Expired,
    Revoked,
}

impl SessionStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, SessionStatus::Expired | SessionStatus::Revoked)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Active => "active",
            SessionStatus::Expired => "expired",
            SessionStatus::Revoked => "revoked",
        }
    }
}
