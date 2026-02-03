//! MOLT token amount representation.
//!
//! Amounts are stored as lamports (base units) internally for precision,
//! with convenient conversion to/from MOLT (decimal) representation.

use crate::error::{MoltError, Result};
use crate::LAMPORTS_PER_MOLT;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Sub};

/// An amount of MOLT tokens.
///
/// Internally stored as lamports (1 MOLT = 10^9 lamports) for precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Amount {
    lamports: u64,
}

impl Amount {
    /// Zero MOLT.
    pub const ZERO: Self = Self { lamports: 0 };

    /// Maximum amount (`u64::MAX` lamports).
    pub const MAX: Self = Self {
        lamports: u64::MAX,
    };

    /// Create an amount from lamports (base units).
    #[must_use]
    pub const fn from_lamports(lamports: u64) -> Self {
        Self { lamports }
    }

    /// Create an amount from MOLT (decimal representation).
    ///
    /// # Panics
    ///
    /// Panics if the amount is negative or too large.
    #[must_use]
    pub fn molt(amount: f64) -> Self {
        assert!(amount >= 0.0, "amount must be non-negative");
        let lamports = (amount * LAMPORTS_PER_MOLT as f64).round() as u64;
        Self { lamports }
    }

    /// Try to create an amount from MOLT.
    ///
    /// # Errors
    ///
    /// Returns error if amount is negative.
    pub fn try_molt(amount: f64) -> Result<Self> {
        if amount < 0.0 {
            return Err(MoltError::InvalidAmount {
                message: "amount must be non-negative".to_string(),
            });
        }
        Ok(Self::molt(amount))
    }

    /// Get the amount in lamports.
    #[must_use]
    pub const fn lamports(&self) -> u64 {
        self.lamports
    }

    /// Get the amount in MOLT (decimal).
    #[must_use]
    pub fn as_molt(&self) -> f64 {
        self.lamports as f64 / LAMPORTS_PER_MOLT as f64
    }

    /// Check if the amount is zero.
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.lamports == 0
    }

    /// Saturating addition.
    #[must_use]
    pub const fn saturating_add(&self, other: Self) -> Self {
        Self {
            lamports: self.lamports.saturating_add(other.lamports),
        }
    }

    /// Saturating subtraction.
    #[must_use]
    pub const fn saturating_sub(&self, other: Self) -> Self {
        Self {
            lamports: self.lamports.saturating_sub(other.lamports),
        }
    }

    /// Checked addition.
    #[must_use]
    pub const fn checked_add(&self, other: Self) -> Option<Self> {
        match self.lamports.checked_add(other.lamports) {
            Some(lamports) => Some(Self { lamports }),
            None => None,
        }
    }

    /// Checked subtraction.
    #[must_use]
    pub const fn checked_sub(&self, other: Self) -> Option<Self> {
        match self.lamports.checked_sub(other.lamports) {
            Some(lamports) => Some(Self { lamports }),
            None => None,
        }
    }
}

impl Default for Amount {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.9} MOLT", self.as_molt())
    }
}

impl Add for Amount {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            lamports: self.lamports + other.lamports,
        }
    }
}

impl Sub for Amount {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            lamports: self.lamports - other.lamports,
        }
    }
}

impl From<u64> for Amount {
    fn from(lamports: u64) -> Self {
        Self::from_lamports(lamports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_molt_to_lamports() {
        let amount = Amount::molt(1.0);
        assert_eq!(amount.lamports(), LAMPORTS_PER_MOLT);
    }

    #[test]
    fn test_lamports_to_molt() {
        let amount = Amount::from_lamports(LAMPORTS_PER_MOLT);
        assert!((amount.as_molt() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fractional_molt() {
        let amount = Amount::molt(0.5);
        assert_eq!(amount.lamports(), LAMPORTS_PER_MOLT / 2);
    }

    #[test]
    fn test_zero() {
        assert!(Amount::ZERO.is_zero());
        assert_eq!(Amount::ZERO.lamports(), 0);
    }

    #[test]
    fn test_add() {
        let a = Amount::molt(1.0);
        let b = Amount::molt(2.0);
        let c = a + b;
        assert!((c.as_molt() - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_sub() {
        let a = Amount::molt(3.0);
        let b = Amount::molt(1.0);
        let c = a - b;
        assert!((c.as_molt() - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_saturating_add() {
        let a = Amount::MAX;
        let b = Amount::molt(1.0);
        let c = a.saturating_add(b);
        assert_eq!(c, Amount::MAX);
    }

    #[test]
    fn test_saturating_sub() {
        let a = Amount::molt(1.0);
        let b = Amount::molt(2.0);
        let c = a.saturating_sub(b);
        assert!(c.is_zero());
    }

    #[test]
    fn test_display() {
        let amount = Amount::molt(1.5);
        let s = format!("{amount}");
        assert!(s.contains("1.5"));
        assert!(s.contains("MOLT"));
    }

    #[test]
    fn test_try_molt_negative() {
        let result = Amount::try_molt(-1.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_ordering() {
        let a = Amount::molt(1.0);
        let b = Amount::molt(2.0);
        assert!(a < b);
        assert!(b > a);
    }

    #[test]
    fn test_serialization() {
        let amount = Amount::molt(1.5);
        let json = serde_json::to_string(&amount).expect("serialize");
        let parsed: Amount = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(amount, parsed);
    }
}
