use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MoneyError {
    #[error("currency must be a three-letter ISO-style code, got {0:?}")]
    InvalidCurrency(String),
    #[error("money arithmetic requires the same currency ({left} != {right})")]
    CurrencyMismatch { left: String, right: String },
    #[error("money arithmetic overflow")]
    Overflow,
}

/// A normalized three-letter currency code. It deliberately does not assert
/// that the code is in a compiled-in list, so the core stays forward-compatible.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CurrencyCode(String);

impl CurrencyCode {
    pub fn new(value: impl AsRef<str>) -> Result<Self, MoneyError> {
        let normalized = value.as_ref().trim().to_ascii_uppercase();
        if normalized.len() == 3 && normalized.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            Ok(Self(normalized))
        } else {
            Err(MoneyError::InvalidCurrency(value.as_ref().to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for CurrencyCode {
    type Error = MoneyError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<CurrencyCode> for String {
    fn from(value: CurrencyCode) -> Self {
        value.0
    }
}

/// Money is always stored in the currency's minor unit. No floating point
/// values cross the financial core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount_minor: i64,
    pub currency: CurrencyCode,
}

impl Money {
    pub fn new(amount_minor: i64, currency: CurrencyCode) -> Self {
        Self {
            amount_minor,
            currency,
        }
    }

    pub fn checked_add(&self, other: &Self) -> Result<Self, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch {
                left: self.currency.to_string(),
                right: other.currency.to_string(),
            });
        }
        let amount_minor = self
            .amount_minor
            .checked_add(other.amount_minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Self::new(amount_minor, self.currency.clone()))
    }
}
