//! MOLT token amount type with fixed-point precision.
//!
//! The Amount type represents MOLT tokens with 9 decimal places of precision.
//! All arithmetic operations are overflow-safe.

use std::fmt;
use std::str::FromStr;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::MoltError;

/// Number of decimal places for MOLT precision.
pub const DECIMALS: u32 = 9;

/// One whole MOLT in nano units.
pub const NANO_PER_MOLT: u64 = 1_000_000_000;

/// Represents a MOLT token amount with fixed-point precision (9 decimals).
///
/// Internally stored as nano-MOLT (10^-9 MOLT) to avoid floating-point issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Amount(u64);

impl Amount {
    /// Zero amount constant.
    pub const ZERO: Self = Self(0);

    /// Maximum possible amount.
    pub const MAX: Self = Self(u64::MAX);

    /// Creates an Amount from nano-MOLT units.
    #[must_use]
    pub const fn from_nano(nano: u64) -> Self {
        Self(nano)
    }

    /// Creates an Amount from whole MOLT units.
    #[must_use]
    pub const fn from_molt(molt: u64) -> Self {
        Self(molt * NANO_PER_MOLT)
    }

    /// Returns the amount in nano-MOLT units.
    #[must_use]
    pub const fn as_nano(self) -> u64 {
        self.0
    }

    /// Returns the amount in whole MOLT units (truncates fractional part).
    #[must_use]
    pub const fn as_molt(self) -> u64 {
        self.0 / NANO_PER_MOLT
    }

    /// Checked addition. Returns `None` on overflow.
    #[must_use]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match self.0.checked_add(rhs.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked subtraction. Returns `None` on underflow.
    #[must_use]
    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        match self.0.checked_sub(rhs.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked multiplication by a scalar. Returns `None` on overflow.
    #[must_use]
    pub const fn checked_mul(self, rhs: u64) -> Option<Self> {
        match self.0.checked_mul(rhs) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked division by a scalar. Returns `None` if divisor is zero.
    #[must_use]
    pub const fn checked_div(self, rhs: u64) -> Option<Self> {
        match self.0.checked_div(rhs) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Returns true if this amount is zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / NANO_PER_MOLT;
        let frac = self.0 % NANO_PER_MOLT;
        write!(f, "{whole}.{frac:09} MOLT")
    }
}

impl FromStr for Amount {
    type Err = MoltError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Reject negative values
        if s.starts_with('-') {
            return Err(MoltError::InvalidAmount("negative values not allowed".into()));
        }

        let parts: Vec<&str> = s.split('.').collect();
        match parts.len() {
            1 => {
                // Whole number only
                let whole: u64 = parts[0]
                    .parse()
                    .map_err(|_| MoltError::InvalidAmount(format!("invalid number: {s}")))?;
                whole
                    .checked_mul(NANO_PER_MOLT)
                    .map(Amount)
                    .ok_or_else(|| MoltError::InvalidAmount("overflow".into()))
            }
            2 => {
                // Has decimal part
                let whole: u64 = if parts[0].is_empty() {
                    0
                } else {
                    parts[0]
                        .parse()
                        .map_err(|_| MoltError::InvalidAmount(format!("invalid whole part: {s}")))?
                };

                let frac_str = parts[1];
                if frac_str.len() > DECIMALS as usize {
                    return Err(MoltError::InvalidAmount(
                        "too many decimal places".into(),
                    ));
                }

                // Pad fractional part to 9 digits
                let padded = format!("{frac_str:0<9}");
                let frac: u64 = padded[..9]
                    .parse()
                    .map_err(|_| MoltError::InvalidAmount(format!("invalid fractional part: {s}")))?;

                let whole_nano = whole
                    .checked_mul(NANO_PER_MOLT)
                    .ok_or_else(|| MoltError::InvalidAmount("overflow".into()))?;

                whole_nano
                    .checked_add(frac)
                    .map(Amount)
                    .ok_or_else(|| MoltError::InvalidAmount("overflow".into()))
            }
            _ => Err(MoltError::InvalidAmount(format!("invalid format: {s}"))),
        }
    }
}

impl Serialize for Amount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize as a decimal string without trailing zeros
        let whole = self.0 / NANO_PER_MOLT;
        let frac = self.0 % NANO_PER_MOLT;

        let s = if frac == 0 {
            format!("{whole}")
        } else {
            // Remove trailing zeros from fractional part
            let frac_str = format!("{frac:09}");
            let trimmed = frac_str.trim_end_matches('0');
            format!("{whole}.{trimmed}")
        };

        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_from_nano_returns_correct_value() {
        let amount = Amount::from_nano(1_000_000_000);
        assert_eq!(amount.as_nano(), 1_000_000_000);
    }

    #[test]
    fn amount_zero_is_zero() {
        assert_eq!(Amount::ZERO.as_nano(), 0);
    }

    #[test]
    fn amount_from_molt_converts_correctly() {
        let amount = Amount::from_molt(5);
        assert_eq!(amount.as_nano(), 5_000_000_000);
    }

    #[test]
    fn amount_as_molt_converts_correctly() {
        let amount = Amount::from_nano(2_500_000_000);
        assert_eq!(amount.as_molt(), 2); // Truncates fractional part
    }

    #[test]
    fn checked_add_succeeds_when_no_overflow() {
        let a = Amount::from_molt(5);
        let b = Amount::from_molt(3);
        let result = a.checked_add(b);
        assert_eq!(result, Some(Amount::from_molt(8)));
    }

    #[test]
    fn checked_add_returns_none_on_overflow() {
        let a = Amount::MAX;
        let b = Amount::from_nano(1);
        assert_eq!(a.checked_add(b), None);
    }

    #[test]
    fn checked_sub_succeeds_when_no_underflow() {
        let a = Amount::from_molt(10);
        let b = Amount::from_molt(3);
        let result = a.checked_sub(b);
        assert_eq!(result, Some(Amount::from_molt(7)));
    }

    #[test]
    fn checked_sub_returns_none_on_underflow() {
        let a = Amount::from_molt(1);
        let b = Amount::from_molt(5);
        assert_eq!(a.checked_sub(b), None);
    }

    #[test]
    fn checked_mul_succeeds_when_no_overflow() {
        let a = Amount::from_molt(5);
        let result = a.checked_mul(3);
        assert_eq!(result, Some(Amount::from_molt(15)));
    }

    #[test]
    fn checked_mul_returns_none_on_overflow() {
        let a = Amount::MAX;
        assert_eq!(a.checked_mul(2), None);
    }

    #[test]
    fn checked_div_succeeds_when_divisor_nonzero() {
        let a = Amount::from_molt(10);
        let result = a.checked_div(2);
        assert_eq!(result, Some(Amount::from_molt(5)));
    }

    #[test]
    fn checked_div_returns_none_on_divide_by_zero() {
        let a = Amount::from_molt(10);
        assert_eq!(a.checked_div(0), None);
    }

    #[test]
    fn display_formats_correctly() {
        // 1.5 MOLT = 1_500_000_000 nano
        let amount = Amount::from_nano(1_500_000_000);
        assert_eq!(format!("{amount}"), "1.500000000 MOLT");
    }

    #[test]
    fn display_zero() {
        assert_eq!(format!("{}", Amount::ZERO), "0.000000000 MOLT");
    }

    #[test]
    fn from_str_parses_correctly() {
        let amount: Amount = "1.5".parse().unwrap();
        assert_eq!(amount.as_nano(), 1_500_000_000);
    }

    #[test]
    fn from_str_parses_whole_number() {
        let amount: Amount = "42".parse().unwrap();
        assert_eq!(amount.as_nano(), 42_000_000_000);
    }

    #[test]
    fn from_str_parses_nano_precision() {
        let amount: Amount = "0.000000001".parse().unwrap();
        assert_eq!(amount.as_nano(), 1);
    }

    #[test]
    fn from_str_rejects_invalid() {
        assert!("abc".parse::<Amount>().is_err());
        assert!("-1.0".parse::<Amount>().is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let original = Amount::from_nano(12_345_678_900);
        let json = serde_json::to_string(&original).unwrap();
        let restored: Amount = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn serde_deserializes_from_string() {
        let json = r#""2.5""#;
        let amount: Amount = serde_json::from_str(json).unwrap();
        assert_eq!(amount.as_nano(), 2_500_000_000);
    }

    #[test]
    fn serde_serializes_to_string() {
        let amount = Amount::from_nano(1_234_567_890);
        let json = serde_json::to_string(&amount).unwrap();
        assert_eq!(json, r#""1.23456789""#);
    }
}
