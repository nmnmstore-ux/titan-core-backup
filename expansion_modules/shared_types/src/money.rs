use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumIter, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString, AsRefStr, EnumIter)]
pub enum Currency {
    USD,
    EUR,
    GBP,
    JPY,
    CHF,
    CAD,
    AUD,
    CNY,
    INR,
    KRW,
    USDT,
    USDC,
    DAI,
    ETH,
    BTC,
    BNB,
    SOL,
    MATIC,
    ARB,
    OP,
    AVAX,
    LINK,
    UNI,
    APE,
    SAND,
    MANA,
    AXS,
    FLOW,
    ICP,
    FIL,
    NEAR,
    DOT,
    XRP,
}

impl Currency {
    pub fn symbol(&self) -> &'static str {
        match self {
            Currency::USD => "$",
            Currency::EUR => "€",
            Currency::GBP => "£",
            Currency::JPY => "¥",
            Currency::CHF => "Fr",
            Currency::CAD => "C$",
            Currency::AUD => "AU$",
            Currency::CNY => "¥",
            Currency::INR => "₹",
            Currency::KRW => "₩",
            Currency::USDT | Currency::USDC | Currency::DAI => "$",
            Currency::ETH => "Ξ",
            Currency::BTC => "₿",
            _ => "",
        }
    }

    pub fn decimals(&self) -> u32 {
        match self {
            Currency::JPY | Currency::KRW => 0,
            Currency::USDT | Currency::USDC | Currency::DAI => 6,
            _ => 8,
        }
    }

    pub fn is_crypto(&self) -> bool {
        matches!(
            self,
            Currency::USDT | Currency::USDC | Currency::DAI | Currency::ETH
                | Currency::BTC | Currency::BNB | Currency::SOL | Currency::MATIC
                | Currency::ARB | Currency::OP | Currency::AVAX | Currency::LINK
                | Currency::UNI | Currency::APE | Currency::SAND | Currency::MANA
                | Currency::AXS | Currency::FLOW | Currency::ICP | Currency::FIL
                | Currency::NEAR | Currency::DOT | Currency::XRP
        )
    }

    pub fn is_fiat(&self) -> bool {
        !self.is_crypto()
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Currency::USD => "USD",
            Currency::EUR => "EUR",
            Currency::GBP => "GBP",
            Currency::JPY => "JPY",
            Currency::CHF => "CHF",
            Currency::CAD => "CAD",
            Currency::AUD => "AUD",
            Currency::CNY => "CNY",
            Currency::INR => "INR",
            Currency::KRW => "KRW",
            Currency::USDT => "USDT",
            Currency::USDC => "USDC",
            Currency::DAI => "DAI",
            Currency::ETH => "ETH",
            Currency::BTC => "BTC",
            Currency::BNB => "BNB",
            Currency::SOL => "SOL",
            Currency::MATIC => "MATIC",
            Currency::ARB => "ARB",
            Currency::OP => "OP",
            Currency::AVAX => "AVAX",
            Currency::LINK => "LINK",
            Currency::UNI => "UNI",
            Currency::APE => "APE",
            Currency::SAND => "SAND",
            Currency::MANA => "MANA",
            Currency::AXS => "AXS",
            Currency::FLOW => "FLOW",
            Currency::ICP => "ICP",
            Currency::FIL => "FIL",
            Currency::NEAR => "NEAR",
            Currency::DOT => "DOT",
            Currency::XRP => "XRP",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Money {
    pub amount: Decimal,
    pub currency: Currency,
}

impl Money {
    pub fn new(amount: impl Into<Decimal>, currency: Currency) -> Self {
        Self {
            amount: amount.into(),
            currency,
        }
    }

    pub fn zero(currency: Currency) -> Self {
        Self {
            amount: Decimal::ZERO,
            currency,
        }
    }

    pub fn add(&self, other: &Money) -> Option<Money> {
        if self.currency != other.currency {
            return None;
        }
        Some(Money {
            amount: self.amount + other.amount,
            currency: self.currency,
        })
    }

    pub fn subtract(&self, other: &Money) -> Option<Money> {
        if self.currency != other.currency {
            return None;
        }
        Some(Money {
            amount: self.amount - other.amount,
            currency: self.currency,
        })
    }

    pub fn is_zero(&self) -> bool {
        self.amount.is_zero()
    }

    pub fn format(&self) -> String {
        format!("{}{}", self.currency.symbol(), self.amount)
    }
}
