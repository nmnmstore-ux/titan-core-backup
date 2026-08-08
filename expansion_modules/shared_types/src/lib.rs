pub mod auth;
pub mod errors;
pub mod ids;
pub mod market;
pub mod money;
pub mod status;
pub mod wallet;

pub use auth::{AuthClaims, Permissions, Role, SessionInfo};
pub use errors::BridgeError;
pub use ids::{AnalysisId, CardId, OrderId, SessionId, TransactionId};
pub use market::{LiquidityData, PoolId, SlippageReport, TradingMode, VenueInfo, VenueKind};
pub use money::{Currency, Money};
pub use status::{CardStatus, KycStatus, OrderStatus, SessionStatus, TxStatus};
pub use wallet::{Chain, WalletAddress};

pub const MODULE_NAME: &str = "shared_types";
pub const SCHEMA_VERSION: &str = "1.0.0";

pub type SharedResult<T> = std::result::Result<T, BridgeError>;
