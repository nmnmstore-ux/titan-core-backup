use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for SessionId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TransactionId(Uuid);

impl TransactionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for TransactionId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for TransactionId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl fmt::Display for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CardId(Uuid);

impl CardId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for CardId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for CardId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl fmt::Display for CardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AnalysisId(Uuid);

impl AnalysisId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for AnalysisId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for AnalysisId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl fmt::Display for AnalysisId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderId(Uuid);

impl OrderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for OrderId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for OrderId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(feature = "bincode-support")]
mod bincode_impls {
    use bincode::Options;

    fn config() -> impl bincode::config::Options {
        bincode::config::DefaultOptions::new()
            .with_varint_encoding()
            .with_little_endian()
    }

    pub fn serialize_ids<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, bincode::Error> {
        config().serialize(value)
    }

    pub fn deserialize_ids<'de, T: serde::Deserialize<'de>>(
        bytes: &'de [u8],
    ) -> Result<T, bincode::Error> {
        config().deserialize(bytes)
    }
}

#[cfg(feature = "bincode-support")]
pub use bincode_impls::{deserialize_ids, serialize_ids};
