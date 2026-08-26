use serde::{Deserialize, Serialize};

/// An ISO 4217 currency, together with its minor-unit exponent.
///
/// **The exponent is data, never a constant (I-2).** JOD has three minor digits
/// — one dinar is 1000 fils — so the `100` that every payments codebase grows
/// somewhere is wrong here by a factor of ten. Carrying the exponent on the
/// currency is what makes that literal unnecessary, and therefore suspicious.
///
/// Four bytes and `Copy`. It rides on every `Money`, so size matters: three
/// bytes of ISO code plus one of exponent, with no pointer and no allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Currency {
    code: [u8; 3],
    exponent: u8,
}

/// Every currency this build knows.
///
/// `from_code` and the associated constants are the *only* ways to obtain a
/// `Currency`, and both draw from here, so no `Currency` can exist whose code
/// this table does not contain. `code()` relies on that.
const CURRENCIES: &[Currency] = &[Currency::JOD, Currency::USD, Currency::EUR];

impl Currency {
    /// The home currency. Exponent 3: 1 dinar = 1000 fils.
    pub const JOD: Currency = Currency {
        code: *b"JOD",
        exponent: 3,
    };
    pub const USD: Currency = Currency {
        code: *b"USD",
        exponent: 2,
    };
    pub const EUR: Currency = Currency {
        code: *b"EUR",
        exponent: 2,
    };

    /// Resolve an ISO 4217 alphabetic code.
    ///
    /// Case-insensitive on purpose: this parses settings rows, sync payloads and
    /// JSON written by other systems, and refusing `"jod"` would turn a cosmetic
    /// difference into a register that will not open. An unknown code is still a
    /// hard error — the engine never guesses an exponent.
    pub fn from_code(code: &str) -> Result<Currency, MoneyError> {
        CURRENCIES
            .iter()
            .find(|c| c.code.eq_ignore_ascii_case(code.as_bytes()))
            .copied()
            .ok_or_else(|| MoneyError::UnknownCurrency(code.to_owned()))
    }

    /// The ISO code, as a `&'static str`, with no allocation.
    ///
    /// Total by construction: `self.code` can only be one of the arms below,
    /// because the constants and `from_code` are the only constructors and both
    /// come from `CURRENCIES`. The final arm is therefore unreachable — and it
    /// returns `"XXX"`, which is the real ISO 4217 code for "no currency",
    /// rather than panicking. A register must not crash while formatting a
    /// receipt, and `unwrap`/`expect`/`panic!` are denied here anyway.
    ///
    /// `every_known_currency_round_trips_through_its_code` is what keeps this
    /// total: add a row to `CURRENCIES` without an arm here and it fails.
    pub fn code(self) -> &'static str {
        match &self.code {
            b"JOD" => "JOD",
            b"USD" => "USD",
            b"EUR" => "EUR",
            _ => "XXX",
        }
    }

    /// Minor digits: 3 for JOD, 2 for USD and EUR, 0 for a currency with no
    /// subunit.
    pub const fn exponent(self) -> u8 {
        self.exponent
    }

    /// Minor units in one major unit — 1000 for JOD, 100 for USD.
    ///
    /// Integer arithmetic only; `clippy::float_arithmetic` is denied workspace
    /// wide and `10f64.powi(3)` is 999.9999999999999 waiting to happen.
    /// `saturating_pow` keeps this total: `i64::pow` panics on overflow in a
    /// debug build, and while no real exponent comes close, a total function
    /// needs no argument about which inputs are reachable.
    pub const fn minor_per_major(self) -> i64 {
        10_i64.saturating_pow(self.exponent as u32)
    }
}

/// Serialised as the bare ISO string — `"JOD"`, not `{"code":[74,79,68],...}`.
///
/// Deriving this over private fields would put the minor-unit exponent on the
/// wire as a second source of truth, where a client could disagree with the
/// server about how many decimal places a dinar has. Sending only the code
/// means the exponent is resolved from `CURRENCIES` on arrival, once.
impl Serialize for Currency {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Owned rather than borrowed: a borrowed &str cannot be deserialised
        // from every format (JSON with an escape sequence needs to allocate),
        // and this is configuration-path code, not the formatting hot path.
        let code = String::deserialize(d)?;
        Currency::from_code(&code).map_err(serde::de::Error::custom)
    }
}

/// An amount in signed integer minor units, tagged with its currency.
///
/// `PartialOrd` and `Ord` are deliberately absent: comparing different
/// currencies requires an explicit, fallible check through `checked_cmp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    minor: i64,
    currency: Currency,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("arithmetic overflow")]
    Overflow,
    #[error("cannot split into zero parts")]
    ZeroParts,
    #[error("negative amount not allowed here")]
    Negative,
    #[error("currency mismatch: {0} vs {1}")]
    CurrencyMismatch(&'static str, &'static str),
    #[error("unknown currency code {0}")]
    UnknownCurrency(String),
}

impl Money {
    pub const fn from_minor(minor: i64, currency: Currency) -> Self {
        Self { minor, currency }
    }

    pub fn zero(currency: Currency) -> Self {
        Self::from_minor(0, currency)
    }

    pub const fn minor(self) -> i64 {
        self.minor
    }

    pub const fn currency(self) -> Currency {
        self.currency
    }

    pub const fn is_zero(self) -> bool {
        self.minor == 0
    }

    pub const fn is_negative(self) -> bool {
        self.minor < 0
    }

    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        self.ensure_same_currency(other)?;
        self.minor
            .checked_add(other.minor)
            .map(|minor| Money::from_minor(minor, self.currency))
            .ok_or(MoneyError::Overflow)
    }

    pub fn checked_sub(self, other: Money) -> Result<Money, MoneyError> {
        self.ensure_same_currency(other)?;
        self.minor
            .checked_sub(other.minor)
            .map(|minor| Money::from_minor(minor, self.currency))
            .ok_or(MoneyError::Overflow)
    }

    pub fn checked_neg(self) -> Result<Money, MoneyError> {
        self.minor
            .checked_neg()
            .map(|minor| Money::from_minor(minor, self.currency))
            .ok_or(MoneyError::Overflow)
    }

    pub fn sum<I: IntoIterator<Item = Money>>(
        iter: I,
        currency: Currency,
    ) -> Result<Money, MoneyError> {
        iter.into_iter()
            .try_fold(Money::zero(currency), Money::checked_add)
    }

    pub fn checked_cmp(self, other: Money) -> Result<core::cmp::Ordering, MoneyError> {
        self.ensure_same_currency(other)?;
        Ok(self.minor.cmp(&other.minor))
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
            .map(|i| Money::from_minor(base + i64::from(i < remainder), self.currency))
            .collect())
    }

    fn ensure_same_currency(self, other: Money) -> Result<(), MoneyError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(MoneyError::CurrencyMismatch(
                self.currency.code(),
                other.currency.code(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;

    // These exact wire fixtures are review tripwires. Money gained its currency
    // in microstep 1.1.2a; changing either fixture remains an intentional,
    // reviewed act rather than an incidental serde refactor.
    const GOLDEN_CURRENCY_JSON: &str = r#""JOD""#;
    const GOLDEN_MONEY_JSON: &str = r#"{"minor":1250,"currency":"JOD"}"#;

    // Covers every known currency. It deliberately excludes no currency that
    // callers can construct through the public API.
    fn known_currency() -> impl Strategy<Value = Currency> {
        prop_oneof![
            Just(Currency::JOD),
            Just(Currency::USD),
            Just(Currency::EUR),
        ]
    }

    // Covers non-negative amounts through one trillion minor units, every
    // split size from 1 through 64, and every known currency. Negative amounts
    // and zero parts are excluded because they are explicit error branches,
    // not conservation cases.
    fn split_cases() -> impl Strategy<Value = (i64, u32, Currency)> {
        (0i64..=1_000_000_000_000, 1u32..=64, known_currency())
    }

    // Covers positive, zero, and negative operands through one trillion minor
    // units in every known currency. Values near i64 overflow are deliberately
    // excluded because this property proves the algebraic round-trip, while
    // overflow remains its own error branch.
    fn add_sub_cases() -> impl Strategy<Value = (i64, i64, Currency)> {
        (
            -1_000_000_000_000i64..=1_000_000_000_000,
            -1_000_000_000_000i64..=1_000_000_000_000,
            known_currency(),
        )
    }

    // Covers the full i64 amount range and every ordered pair of distinct
    // known currencies. Same-currency pairs are deliberately excluded because
    // this property proves that a mismatch is always refused.
    fn mixed_currency_cases() -> impl Strategy<Value = (i64, i64, Currency, Currency)> {
        (
            any::<i64>(),
            any::<i64>(),
            prop_oneof![
                Just((Currency::JOD, Currency::USD)),
                Just((Currency::JOD, Currency::EUR)),
                Just((Currency::USD, Currency::JOD)),
                Just((Currency::USD, Currency::EUR)),
                Just((Currency::EUR, Currency::JOD)),
                Just((Currency::EUR, Currency::USD)),
            ],
        )
            .prop_map(|(left, right, (left_currency, right_currency))| {
                (left, right, left_currency, right_currency)
            })
    }

    #[test]
    fn jod_exponent_is_three() {
        // I-2, and the whole reason this type exists: one dinar is 1000 fils,
        // so a hardcoded 100 would misprice every amount by a factor of ten.
        assert_eq!(Currency::JOD.exponent(), 3);
        assert_eq!(Currency::JOD.minor_per_major(), 1000);
        assert_eq!(Currency::USD.minor_per_major(), 100);
    }

    #[test]
    fn unknown_currency_code_errors() {
        // The engine never guesses an exponent. Two decimals would be a
        // plausible, wrong answer for a currency it has never heard of.
        assert_eq!(
            Currency::from_code("ZZZ"),
            Err(MoneyError::UnknownCurrency("ZZZ".to_owned()))
        );
        assert_eq!(
            Currency::from_code(""),
            Err(MoneyError::UnknownCurrency(String::new()))
        );
        // A four-letter code must not match a three-byte prefix.
        assert!(Currency::from_code("JODX").is_err());
        assert!(Currency::from_code("JO").is_err());
    }

    #[test]
    fn currency_codes_are_case_insensitive() {
        // Settings rows and third-party JSON are not disciplined about case,
        // and a register that will not open over `"jod"` is a bad trade.
        assert_eq!(Currency::from_code("jod"), Ok(Currency::JOD));
        assert_eq!(Currency::from_code("JoD"), Ok(Currency::JOD));
    }

    #[test]
    fn every_known_currency_round_trips_through_its_code() {
        // This is what keeps `code()` total. Its final arm returns "XXX" — ISO
        // 4217 for "no currency" — instead of panicking, so a missing match arm
        // would otherwise be invisible until it reached a receipt. Add a row to
        // CURRENCIES without an arm in `code()` and this fails.
        for c in CURRENCIES {
            assert_ne!(c.code(), "XXX", "{c:?} has no arm in Currency::code()");
            assert_eq!(Currency::from_code(c.code()), Ok(*c));
            assert_eq!(c.code().len(), 3, "an ISO 4217 alphabetic code is 3 chars");
        }
    }

    #[test]
    fn currency_stays_four_bytes() {
        // It rides on every Money. If this grows, that cost is paid on every
        // line of every sale, so a change here should be deliberate.
        assert_eq!(core::mem::size_of::<Currency>(), 4);
    }

    #[test]
    fn currency_serialises_as_its_iso_code() {
        assert_eq!(
            serde_json::to_string(&Currency::JOD).unwrap(),
            GOLDEN_CURRENCY_JSON
        );
        assert_eq!(
            serde_json::from_str::<Currency>(GOLDEN_CURRENCY_JSON).unwrap(),
            Currency::JOD
        );
        // Escapes force serde down the owned-String path; it must still work.
        assert_eq!(
            serde_json::from_str::<Currency>("\"\\u004aOD\"").unwrap(),
            Currency::JOD
        );
    }

    #[test]
    fn unknown_currency_code_is_a_deserialisation_error() {
        // An unknown code is a deserialisation error, not a default.
        assert!(serde_json::from_str::<Currency>("\"ZZZ\"").is_err());
        // Case folding must not rescue an unknown code either.
        assert!(serde_json::from_str::<Currency>("\"zzz\"").is_err());
        // A wrong JSON type takes a different serde path than a wrong string:
        // `Cow<str>` refuses it before `from_code` is ever consulted, and a
        // number is exactly what a client sending a raw exponent would emit.
        assert!(serde_json::from_str::<Currency>("3").is_err());
        assert!(serde_json::from_str::<Currency>("null").is_err());
        assert!(
            serde_json::from_str::<Currency>(r#"{"code":"JOD","exponent":3}"#).is_err(),
            "the derived struct form must never deserialise"
        );
    }

    #[test]
    fn the_exponent_never_appears_on_the_wire() {
        // Not {"code":[74,79,68],"exponent":3}: the ISO string is the only
        // source of truth sent over the wire.
        for currency in CURRENCIES {
            assert_eq!(
                serde_json::to_value(currency).unwrap(),
                serde_json::Value::String(currency.code().to_owned())
            );
        }
    }

    #[test]
    fn golden_money_json_is_stable() {
        assert_eq!(
            serde_json::to_string(&Currency::JOD).unwrap(),
            GOLDEN_CURRENCY_JSON
        );
        assert_eq!(
            serde_json::to_string(&Money::from_minor(1250, Currency::JOD)).unwrap(),
            GOLDEN_MONEY_JSON
        );
    }

    #[test]
    fn split_examples() {
        let parts = Money::from_minor(100, Currency::JOD)
            .split_evenly(3)
            .unwrap();
        assert_eq!(
            parts,
            vec![
                Money::from_minor(34, Currency::JOD),
                Money::from_minor(33, Currency::JOD),
                Money::from_minor(33, Currency::JOD)
            ]
        );
        assert_eq!(Money::zero(Currency::JOD).split_evenly(5).unwrap().len(), 5);
        assert_eq!(
            Money::from_minor(10, Currency::JOD).split_evenly(0),
            Err(MoneyError::ZeroParts)
        );
    }

    #[test]
    fn money_core_operations_preserve_currency() {
        // A zero, a negation, and a sum must keep the currency the caller
        // supplied; no operation may invent a default currency.
        let zero = Money::zero(Currency::EUR);
        assert!(zero.is_zero());
        assert!(!zero.is_negative());
        assert_eq!(zero.currency(), Currency::EUR);

        let negative = Money::from_minor(-7, Currency::EUR);
        assert!(negative.is_negative());
        assert_eq!(
            negative.checked_neg(),
            Ok(Money::from_minor(7, Currency::EUR))
        );
        assert_eq!(
            Money::from_minor(i64::MIN, Currency::EUR).checked_neg(),
            Err(MoneyError::Overflow)
        );

        assert_eq!(Money::sum([], Currency::EUR), Ok(zero));
        assert_eq!(
            Money::sum(
                [
                    Money::from_minor(4, Currency::EUR),
                    Money::from_minor(6, Currency::EUR),
                ],
                Currency::EUR,
            ),
            Ok(Money::from_minor(10, Currency::EUR))
        );
    }

    #[test]
    fn mixed_currency_comparison_is_refused() {
        // A JOD amount and a USD amount have no meaningful ordering until a
        // caller performs an explicit conversion, so comparison must fail.
        let jod = Money::from_minor(1, Currency::JOD);
        let usd = Money::from_minor(1, Currency::USD);
        assert_eq!(
            jod.checked_cmp(usd),
            Err(MoneyError::CurrencyMismatch("JOD", "USD"))
        );
        assert_eq!(
            Money::from_minor(1, Currency::JOD).checked_cmp(Money::from_minor(2, Currency::JOD)),
            Ok(core::cmp::Ordering::Less)
        );
    }

    proptest! {
        // One shared configuration for every property in this crate: 4,096 cases,
        // the repository's recorded seed, and a minimized failing case persisted
        // under crates/pos-domain/proptest-regressions/money.txt to be committed.
        // `PROPTEST_CASES` raises the count and can never lower it, which is what
        // makes the scheduled PROPTEST_CASES=100000 lane mean what it says.
        // Owned by microstep 1.1.0; conventions §5.1 is the rule.
        #![proptest_config(domain_proptest_config())]

        /// Splitting a tender never changes its total or its currency, and no
        /// two pieces differ by more than one minor unit.
        #[test]
        fn prop_split_preserves_total((minor, parts, currency) in split_cases()) {
            let m = Money::from_minor(minor, currency);
            let pieces = m.split_evenly(parts).unwrap();
            prop_assert_eq!(pieces.len(), parts as usize);
            prop_assert!(pieces.iter().all(|piece| piece.currency() == currency));

            let sum: i64 = pieces.iter().map(|p| p.minor()).sum();
            prop_assert_eq!(sum, minor);

            let min = pieces.iter().map(|p| p.minor()).min().unwrap();
            let max = pieces.iter().map(|p| p.minor()).max().unwrap();
            prop_assert!(max - min <= 1, "pieces must differ by at most one minor unit");
        }

        /// Addition round-trips: (a + b) - b == a whenever a + b doesn't overflow.
        #[test]
        fn prop_add_sub_roundtrip((a, b, currency) in add_sub_cases()) {
            let (ma, mb) = (
                Money::from_minor(a, currency),
                Money::from_minor(b, currency),
            );
            if let Ok(sum) = ma.checked_add(mb) {
                prop_assert_eq!(sum.checked_sub(mb).unwrap(), ma);
            }
        }

        /// Mixed-currency arithmetic and comparison always return a mismatch;
        /// they never silently relabel or combine the minor-unit integers.
        #[test]
        fn prop_currency_mismatch_never_silently_coerces(
            (left_minor, right_minor, left_currency, right_currency) in mixed_currency_cases()
        ) {
            let left = Money::from_minor(left_minor, left_currency);
            let right = Money::from_minor(right_minor, right_currency);

            prop_assert_eq!(
                left.checked_add(right),
                Err(MoneyError::CurrencyMismatch(
                    left_currency.code(),
                    right_currency.code(),
                ))
            );
            prop_assert_eq!(
                left.checked_sub(right),
                Err(MoneyError::CurrencyMismatch(
                    left_currency.code(),
                    right_currency.code(),
                ))
            );
            prop_assert_eq!(
                left.checked_cmp(right),
                Err(MoneyError::CurrencyMismatch(
                    left_currency.code(),
                    right_currency.code(),
                ))
            );
            prop_assert_eq!(
                Money::sum([left, right], left_currency),
                Err(MoneyError::CurrencyMismatch(
                    left_currency.code(),
                    right_currency.code(),
                ))
            );
        }
    }
}
