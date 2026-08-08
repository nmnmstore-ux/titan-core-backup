use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
pub enum Chain {
    Ethereum,
    Bitcoin,
    Polygon,
    Arbitrum,
    Optimism,
    BSC,
    Solana,
    Avalanche,
    Base,
    Gnosis,
    Celo,
    Fantom,
    Linea,
    Scroll,
    Zksync,
}

impl Chain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Chain::Ethereum => "ethereum",
            Chain::Bitcoin => "bitcoin",
            Chain::Polygon => "polygon",
            Chain::Arbitrum => "arbitrum",
            Chain::Optimism => "optimism",
            Chain::BSC => "bsc",
            Chain::Solana => "solana",
            Chain::Avalanche => "avalanche",
            Chain::Base => "base",
            Chain::Gnosis => "gnosis",
            Chain::Celo => "celo",
            Chain::Fantom => "fantom",
            Chain::Linea => "linea",
            Chain::Scroll => "scroll",
            Chain::Zksync => "zksync",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Chain::Ethereum => "Ethereum",
            Chain::Bitcoin => "Bitcoin",
            Chain::Polygon => "Polygon",
            Chain::Arbitrum => "Arbitrum",
            Chain::Optimism => "Optimism",
            Chain::BSC => "BNB Smart Chain",
            Chain::Solana => "Solana",
            Chain::Avalanche => "Avalanche",
            Chain::Base => "Base",
            Chain::Gnosis => "Gnosis",
            Chain::Celo => "Celo",
            Chain::Fantom => "Fantom",
            Chain::Linea => "Linea",
            Chain::Scroll => "Scroll",
            Chain::Zksync => "zkSync",
        }
    }

    pub fn chain_id(&self) -> Option<u64> {
        match self {
            Chain::Ethereum => Some(1),
            Chain::Polygon => Some(137),
            Chain::Arbitrum => Some(42161),
            Chain::Optimism => Some(10),
            Chain::BSC => Some(56),
            Chain::Avalanche => Some(43114),
            Chain::Base => Some(8453),
            Chain::Gnosis => Some(100),
            Chain::Linea => Some(59144),
            Chain::Scroll => Some(534352),
            Chain::Zksync => Some(324),
            Chain::Bitcoin | Chain::Solana | Chain::Fantom | Chain::Celo => None,
        }
    }

    pub fn is_evm(&self) -> bool {
        !matches!(self, Chain::Bitcoin | Chain::Solana)
    }

    pub fn is_l2(&self) -> bool {
        matches!(
            self,
            Chain::Polygon | Chain::Arbitrum | Chain::Optimism | Chain::Base | Chain::Linea | Chain::Scroll | Chain::Zksync
        )
    }
}

impl Default for Chain {
    fn default() -> Self {
        Chain::Ethereum
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WalletAddress {
    pub chain: Chain,
    pub address: String,
}

impl WalletAddress {
    pub fn new(chain: Chain, address: impl Into<String>) -> Self {
        Self {
            chain,
            address: address.into(),
        }
    }

    pub fn is_valid(&self) -> bool {
        match self.chain {
            Chain::Bitcoin => {
                let addr = &self.address;
                addr.starts_with("1") || addr.starts_with("3") || addr.starts_with("bc1")
            }
            Chain::Solana => {
                self.address.len() >= 32 && self.address.len() <= 44
            }
            _ => {
                let addr = &self.address;
                addr.len() == 42 && addr.starts_with("0x")
            }
        }
    }

    pub fn as_str(&self) -> &str {
        &self.address
    }
}

impl std::fmt::Display for WalletAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.chain.as_str(), self.address)
    }
}
