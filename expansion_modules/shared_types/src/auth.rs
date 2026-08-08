use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum Role {
    Admin,
    User,
    Merchant,
    Compliance,
    System,
}

impl Role {
    pub fn default_permissions(&self) -> Permissions {
        match self {
            Role::Admin => Permissions::all(),
            Role::User => Permissions::READ | Permissions::WRITE | Permissions::TRADE | Permissions::WITHDRAW,
            Role::Merchant => Permissions::READ | Permissions::WRITE | Permissions::TRADE,
            Role::Compliance => Permissions::READ | Permissions::VIEW_COMPLIANCE,
            Role::System => Permissions::all(),
        }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
    pub struct Permissions: u32 {
        const READ          = 1 << 0;
        const WRITE         = 1 << 1;
        const TRADE         = 1 << 2;
        const WITHDRAW      = 1 << 3;
        const ADMIN         = 1 << 4;
        const MANAGE_USERS  = 1 << 5;
        const MANAGE_CARDS  = 1 << 6;
        const VIEW_COMPLIANCE = 1 << 7;
    }
}

impl Permissions {
    pub fn has(self, perm: Permissions) -> bool {
        self.contains(perm)
    }

    pub fn combine(self, other: Permissions) -> Permissions {
        self | other
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    pub sub: String,
    pub role: Role,
    pub permissions: Permissions,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub user_id: String,
    pub role: Role,
    pub permissions: Permissions,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}
