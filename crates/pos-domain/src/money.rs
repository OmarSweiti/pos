use serde::{Deserialize, Serialize};

/// An amount in integer minor units (cents / fils / pence).
/// Currency is tracked at the document level, not per amount, in Phase 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Money {
    minor: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("arithmetic overflow")]
    Overflow,
    #[error("cannot split into zero parts")]
    ZeroParts,
    #[error("negative amount not allowed here")]
    Negative,
}

impl Money {
    pub const ZERO: Money = Money { minor: 0 };

    pub const fn from_minor(minor: i64) -> Self {
        Self { minor }
    }

    pub const fn minor(self) -> i64 {
        self.minor
    }

    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        self.minor
            .checked_add(other.minor)
            .map(Money::from_minor)
            .ok_or(MoneyError::Overflow)
    }

    pub fn checked_sub(self, other: Money) -> Result<Money, MoneyError> {
        self.minor
            .checked_sub(other.minor)
            .map(Money::from_minor)
            .ok_or(MoneyError::Overflow)
    }

    /// Split a non-negative amount into `parts` pieces that differ by at most
    /// one minor unit and sum EXACTLY to the original (largest-remainder).
    /// This is the primitive under split tenders and per-line proration.
    pub fn split_evenly(self, parts: u32) -> Result<Vec<Money>, MoneyError> {
        if parts == 0 {
            return Err(MoneyError::ZeroParts);
        }
        if self.minor < 0 {
            return Err(MoneyError::Negative);
        }
        let parts_i = i64::from(parts);
        let base = self.minor / parts_i;
        let remainder = self.minor % parts_i;
        Ok((0..parts_i)
            .map(|i| Money::from_minor(base + i64::from(i < remainder)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use proptest::prelude::*;

    #[test]
    fn split_examples() {
        let parts = Money::from_minor(100).split_evenly(3).unwrap();
        assert_eq!(
            parts,
            vec![
                Money::from_minor(34),
                Money::from_minor(33),
                Money::from_minor(33)
            ]
        );
        assert_eq!(Money::from_minor(0).split_evenly(5).unwrap().len(), 5);
        assert_eq!(
            Money::from_minor(10).split_evenly(0),
            Err(MoneyError::ZeroParts)
        );
    }

    proptest! {
        /// Blueprint §8: "splitting a tender never changes the total."
        #[test]
        fn prop_split_preserves_total(minor in 0i64..=1_000_000_000_000, parts in 1u32..=64) {
            let m = Money::from_minor(minor);
            let pieces = m.split_evenly(parts).unwrap();
            prop_assert_eq!(pieces.len(), parts as usize);

            let sum: i64 = pieces.iter().map(|p| p.minor()).sum();
            prop_assert_eq!(sum, minor);

            let min = pieces.iter().map(|p| p.minor()).min().unwrap();
            let max = pieces.iter().map(|p| p.minor()).max().unwrap();
            prop_assert!(max - min <= 1, "pieces must differ by at most one minor unit");
        }

        /// Addition round-trips: (a + b) - b == a whenever a + b doesn't overflow.
        #[test]
        fn prop_add_sub_roundtrip(a in -1_000_000_000_000i64..=1_000_000_000_000,
                             b in -1_000_000_000_000i64..=1_000_000_000_000) {
            let (ma, mb) = (Money::from_minor(a), Money::from_minor(b));
            if let Ok(sum) = ma.checked_add(mb) {
                prop_assert_eq!(sum.checked_sub(mb).unwrap(), ma);
            }
        }
    }
}
