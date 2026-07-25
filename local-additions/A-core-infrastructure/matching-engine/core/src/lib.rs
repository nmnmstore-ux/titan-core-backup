use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub mod types;

pub use types::*;
pub use types::{AAVE_V3_POOL, UNISWAP_V3_FACTORY, MIN_TICK, MAX_TICK, MIN_SQRT_RATIO, MAX_SQRT_RATIO};

pub mod error;
pub use error::*;

pub mod prelude {
    pub use crate::types::*;
    pub use crate::CoreError;
    pub use chrono::Utc;
    pub use serde::{Deserialize, Serialize};
    pub use std::collections::HashMap;
    pub use std::sync::Arc;
    pub use std::time::Duration;
    pub use uuid::Uuid;
}

pub type Result<T> = std::result::Result<T, CoreError>;
