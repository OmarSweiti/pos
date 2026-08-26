use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
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

/// A quantity in signed integer milli-units.
///
/// **One unit is 1000 milli-units (I-3).** Discrete and weighed goods share
/// this representation — two items are `2000`, and 0.347 kg is `347` — so
/// quantity arithmetic never branches on which kind of product it belongs to.
///
/// Unlike `Money`, a `Qty` has no currency or other external dimension that
/// could make two values incomparable. Its derived ordering is therefore the
/// honest numeric ordering of the underlying milli-units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Qty(i64);

impl Qty {
    pub const ZERO: Qty = Qty(0);
    pub const ONE: Qty = Qty(1_000);

    pub const fn from_milli(milli: i64) -> Qty {
        Qty(milli)
    }

    pub const fn milli(self) -> i64 {
        self.0
    }

    /// Construct a whole-unit quantity without overflowing the milli-unit
    /// representation.
    pub fn from_units(units: i64) -> Result<Qty, MoneyError> {
        units
            .checked_mul(Qty::ONE.0)
            .map(Qty::from_milli)
            .ok_or(MoneyError::Overflow)
    }

    pub fn checked_add(self, other: Qty) -> Result<Qty, MoneyError> {
        self.0
            .checked_add(other.0)
            .map(Qty::from_milli)
            .ok_or(MoneyError::Overflow)
    }

    pub fn checked_sub(self, other: Qty) -> Result<Qty, MoneyError> {
        self.0
            .checked_sub(other.0)
            .map(Qty::from_milli)
            .ok_or(MoneyError::Overflow)
    }

    pub fn is_whole_units(self) -> bool {
        self.0 % Qty::ONE.0 == 0
    }

    /// Convert exactly for decimal arithmetic. The integer mantissa and fixed
    /// scale avoid both division and any float intermediate.
    pub fn to_decimal(self) -> Decimal {
        Decimal::new(self.0, 3)
    }

    /// Render weighed quantities at the representation's fixed three-decimal
    /// precision. Whole discrete quantities omit the decimal point.
    ///
    /// `weighed` is a display hint, not permission to discard data: if a value
    /// marked discrete is fractional, this falls back to the exact three-place
    /// form instead of rounding a real quantity away. Signed quantities retain
    /// their sign, including refund and correction values.
    pub fn format(self, weighed: bool) -> String {
        if weighed || !self.is_whole_units() {
            self.to_decimal().to_string()
        } else {
            (self.0 / Qty::ONE.0).to_string()
        }
    }
}

/// How an exact intermediate value becomes a whole number of units — the tie
/// rule for tax arithmetic.
///
/// **Not a merchant preference.** The tie rule changes tax *facts*, not
/// presentation: a 13-fil 4%-inclusive line has an exact net of 12.5 fils, so
/// half-away records net 13 and tax 0 while half-even records net 12 and tax 1.
/// Two registers under one taxpayer that disagree file inconsistent returns and
/// nothing diagnoses it, so the rule belongs to a versioned jurisdiction policy
/// pinned per store — `ref/tax-jordan.md` §4 and conventions §2 — and not to a
/// settings screen offering four options.
///
/// `HalfAwayFromZero` is the provisional Jordan default. It is provisional
/// because the scale and tie rule ISTD's own validator applies are still an open
/// question owned by microstep 2.7.0, which is also why there is deliberately no
/// `Default` impl: `unwrap_or_default()` is exactly how an unapproved tax rule
/// would reach a real sale, and 1.3.4 exists to block that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoundingRule {
    /// 1.5 → 2 and −1.5 → −2. Symmetric about zero, which is what a merchant's
    /// accountant checking the till by hand expects. The provisional default.
    HalfAwayFromZero,
    /// Banker's rounding: a tie goes to the even neighbour, so 1.5 → 2 and
    /// 2.5 → 2. Offered because a jurisdiction policy may require it, and never
    /// the default here — `00-master-plan.md` §4a records that the source
    /// blueprint's banker's-rounding default was superseded.
    HalfEven,
    /// Toward negative infinity, on both sides of zero: −1.2 → −2, not −1.
    /// This is not truncation.
    Floor,
    /// Toward positive infinity, on both sides of zero: 1.2 → 2 and −1.2 → −1.
    Ceil,
}

impl RoundingRule {
    /// **The** rounding point (I-1): one exact `Decimal` in, one whole `i64`
    /// out.
    ///
    /// Every consumer performs this same last step — `mul_qty`, `mul_percent`
    /// and `from_decimal` all reduce an exact `rust_decimal` intermediate to
    /// integer units — so it exists once. "Rounds once" is only a meaningful
    /// claim if there is exactly one place to round in; four callers each
    /// spelling their own conversion is how two of them come to disagree by a
    /// fil on the one document where they must not.
    ///
    /// The caller passes the value already expressed in the units it wants
    /// back: minor units for money, milli-units for a quantity. The name says
    /// `i64` rather than `minor` for that reason — this primitive carries no
    /// currency and must not imply one.
    ///
    /// A result outside `i64` is `MoneyError::Overflow`, never a panic and never
    /// a saturating cast. A saturated total is a wrong price wearing the shape
    /// of a right one, and rounding itself can be what leaves the range.
    pub fn round_to_i64(self, value: Decimal) -> Result<i64, MoneyError> {
        value
            .round_dp_with_strategy(0, self.strategy())
            .to_i64()
            .ok_or(MoneyError::Overflow)
    }

    /// The `rust_decimal` strategy each rule *is*.
    ///
    /// Hand-rolling four roundings would be four chances to get a tie or a sign
    /// wrong, and a wrong one misprices every line in the system. Delegating
    /// leaves the mapping as the only thing that can be wrong here, which is
    /// why `each_rounding_rule_maps_to_its_own_decimal_strategy` pins all four
    /// against vectors that separate them from each other *and* from the three
    /// strategies none of them may map to.
    const fn strategy(self) -> RoundingStrategy {
        match self {
            RoundingRule::HalfAwayFromZero => RoundingStrategy::MidpointAwayFromZero,
            RoundingRule::HalfEven => RoundingStrategy::MidpointNearestEven,
            RoundingRule::Floor => RoundingStrategy::ToNegativeInfinity,
            RoundingRule::Ceil => RoundingStrategy::ToPositiveInfinity,
        }
    }
}

/// Which way a *cash* settlement amount moves to reach a payable coin step.
///
/// A different axis from `RoundingRule`, and the distinction is load-bearing:
/// tax rounding decides immutable line facts, while cash rounding is a signed
/// tender-level adjustment that leaves those facts alone
/// (`ref/tax-jordan.md` §5).
///
/// It carries no primitive yet, deliberately. `Money::round_to_step` is
/// microstep 1.5.3, and what `Up` and `Down` mean below zero — toward the
/// infinities, or away from and toward zero — is decided there, next to the
/// still-open question of which direction a cash *refund payout* takes so a
/// drawer cannot retain an unrecorded remainder. A primitive written here would
/// answer that question by accident, in a microstep nobody reviewed for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoundingDirection {
    Nearest,
    Up,
    Down,
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

    // Every rounding rule, listed once. It drives `every_rounding_rule()`, the
    // overflow sweep and the strategy-distinctness check, so
    // `the_rule_table_lists_every_variant_exactly_once` is what stops a fifth
    // variant from slipping past all three at the same time.
    const ALL_RULES: [RoundingRule; 4] = [
        RoundingRule::HalfAwayFromZero,
        RoundingRule::HalfEven,
        RoundingRule::Floor,
        RoundingRule::Ceil,
    ];

    // A rounding vector, built from integer parts. `1.5_f64` in a money test is
    // the exact defect I-1 exists to prevent, and `clippy::float_arithmetic`
    // does not see a bare literal, so every fractional value in this file goes
    // through here.
    fn tenths(value: i64) -> Decimal {
        Decimal::new(value, 1)
    }

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

    // Covers every ordered pair across the full signed i64 milli-unit space,
    // including zero, refunds, and pairs whose sum overflows. It deliberately
    // excludes nothing: overflow must be a named error, while every
    // representable sum must subtract back to its original left operand.
    fn qty_add_sub_cases() -> impl Strategy<Value = (i64, i64)> {
        (any::<i64>(), any::<i64>())
    }

    // Covers every rounding rule and nothing else. It reads `ALL_RULES` rather
    // than restating the variants, so a fifth rule reaches the properties below
    // the moment it reaches that list.
    fn every_rounding_rule() -> impl Strategy<Value = RoundingRule> {
        proptest::sample::select(ALL_RULES.to_vec())
    }

    // Covers the ENTIRE i64 range — every value the primitive can legally
    // return — against every rule. Fractional inputs are deliberately excluded:
    // this property is about the values that must not move at all.
    fn whole_value_cases() -> impl Strategy<Value = (i64, RoundingRule)> {
        (any::<i64>(), every_rounding_rule())
    }

    // Covers values carrying zero through three decimal places — the finest
    // scale a JOD amount or a milli-unit quantity can hold — over ±10^15 units,
    // against every rule. Magnitudes near the i64 boundary are deliberately
    // excluded because overflow is its own error branch, not a distance claim.
    fn fractional_cases() -> impl Strategy<Value = (Decimal, RoundingRule)> {
        (
            -1_000_000_000_000_000i64..=1_000_000_000_000_000,
            0u32..=3,
            every_rounding_rule(),
        )
            .prop_map(|(mantissa, scale, rule)| (Decimal::new(mantissa, scale), rule))
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
    fn weighed_formats_three_decimals() {
        assert_eq!(Qty::from_milli(347).format(true), "0.347");
        assert_eq!(Qty::from_units(2).unwrap().format(true), "2.000");
        assert_eq!(Qty::ZERO.format(true), "0.000");
    }

    #[test]
    fn whole_units_format_without_decimals() {
        assert_eq!(Qty::ONE.milli(), 1_000);
        assert_eq!(Qty::ONE.format(false), "1");
        assert_eq!(Qty::from_units(2).unwrap().format(false), "2");
        assert_eq!(Qty::ZERO.format(false), "0");
    }

    #[test]
    fn discrete_format_preserves_fractional_quantities() {
        // The product-kind flag cannot make a real quantity disappear. A
        // fractional value on a discrete product is anomalous but still exact.
        assert_eq!(Qty::from_milli(347).format(false), "0.347");
        assert_eq!(Qty::from_milli(1_001).format(false), "1.001");
        assert_eq!(Qty::from_milli(-347).format(false), "-0.347");
        assert_eq!(Qty::from_milli(-1_001).format(false), "-1.001");
    }

    #[test]
    fn negative_quantities_preserve_their_sign() {
        let whole = Qty::from_units(-2).unwrap();
        let fractional = Qty::from_milli(-347);

        assert_eq!(whole.format(false), "-2");
        assert_eq!(whole.format(true), "-2.000");
        assert_eq!(fractional.format(false), "-0.347");
        assert_eq!(fractional.format(true), "-0.347");
    }

    #[test]
    fn from_units_reports_overflow() {
        let max_units = i64::MAX / Qty::ONE.milli();
        let min_units = i64::MIN / Qty::ONE.milli();

        assert_eq!(
            Qty::from_units(max_units),
            Ok(Qty::from_milli(max_units * Qty::ONE.milli()))
        );
        assert_eq!(
            Qty::from_units(min_units),
            Ok(Qty::from_milli(min_units * Qty::ONE.milli()))
        );
        assert_eq!(Qty::from_units(max_units + 1), Err(MoneyError::Overflow));
        assert_eq!(Qty::from_units(min_units - 1), Err(MoneyError::Overflow));
    }

    #[test]
    fn qty_checked_arithmetic_reports_overflow() {
        let one_milli = Qty::from_milli(1);
        let negative_one_milli = Qty::from_milli(-1);

        assert_eq!(
            Qty::from_milli(i64::MAX).checked_add(one_milli),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            Qty::from_milli(i64::MIN).checked_add(negative_one_milli),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            Qty::from_milli(i64::MAX).checked_sub(negative_one_milli),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            Qty::from_milli(i64::MIN).checked_sub(one_milli),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            Qty::ZERO.checked_sub(Qty::ONE),
            Ok(Qty::from_units(-1).unwrap())
        );
    }

    #[test]
    fn whole_unit_detection_handles_sign_and_fraction_boundaries() {
        for milli in [-2_000, -1_000, 0, 1_000, 2_000] {
            assert!(Qty::from_milli(milli).is_whole_units());
        }
        for milli in [i64::MIN, -1_001, -999, -1, 1, 999, 1_001, i64::MAX] {
            assert!(!Qty::from_milli(milli).is_whole_units());
        }
    }

    #[test]
    fn qty_to_decimal_is_exact() {
        for milli in [i64::MIN, -1_001, -347, 0, 347, 1_001, i64::MAX] {
            let decimal = Qty::from_milli(milli).to_decimal();
            assert_eq!(decimal, Decimal::new(milli, 3));
            assert_eq!(decimal.scale(), 3);
        }
        assert_eq!(
            Qty::from_milli(i64::MIN).to_decimal().to_string(),
            "-9223372036854775.808"
        );
        assert_eq!(
            Qty::from_milli(i64::MAX).to_decimal().to_string(),
            "9223372036854775.807"
        );
    }

    #[test]
    fn qty_order_follows_signed_milli_units() {
        let mut quantities = [
            Qty::from_milli(347),
            Qty::ONE,
            Qty::from_milli(-347),
            Qty::ZERO,
            Qty::from_units(-1).unwrap(),
        ];
        quantities.sort();

        assert_eq!(
            quantities,
            [
                Qty::from_units(-1).unwrap(),
                Qty::from_milli(-347),
                Qty::ZERO,
                Qty::from_milli(347),
                Qty::ONE,
            ]
        );
        assert!(Qty::from_milli(-1) < Qty::ZERO);
    }

    #[test]
    fn qty_serialises_as_milli_units() {
        let qty = Qty::from_milli(-347);
        assert_eq!(serde_json::to_string(&qty).unwrap(), "-347");
        assert_eq!(serde_json::from_str::<Qty>("-347").unwrap(), qty);
    }

    #[test]
    fn the_rule_table_lists_every_variant_exactly_once() {
        // ALL_RULES drives the generator and every table below, so a rule that
        // never reaches it would be untested behind green tests. The exhaustive
        // match is the compiler's half of the proof — add a variant and this
        // stops compiling; the pairwise check is the other half, because a list
        // that names one rule twice still matches exhaustively.
        for rule in ALL_RULES {
            match rule {
                RoundingRule::HalfAwayFromZero
                | RoundingRule::HalfEven
                | RoundingRule::Floor
                | RoundingRule::Ceil => {}
            }
        }
        for (index, left) in ALL_RULES.iter().enumerate() {
            for right in ALL_RULES.iter().skip(index + 1) {
                assert_ne!(left, right, "ALL_RULES names {left:?} twice");
            }
        }
    }

    #[test]
    fn half_away_from_zero_rounds_1_5_to_2_and_neg_1_5_to_neg_2() {
        // The provisional Jordan default, and the only rule symmetric about
        // zero: a merchant's accountant checking the till by hand sends 1.5 up
        // and -1.5 down, landing the same distance from zero either way.
        let rule = RoundingRule::HalfAwayFromZero;
        assert_eq!(rule.round_to_i64(tenths(15)), Ok(2));
        assert_eq!(rule.round_to_i64(tenths(-15)), Ok(-2));
        // 2.5 is where it parts company with banker's rounding.
        assert_eq!(rule.round_to_i64(tenths(25)), Ok(3));
        assert_eq!(rule.round_to_i64(tenths(-25)), Ok(-3));
        // Away from a tie it is ordinary nearest-value rounding.
        assert_eq!(rule.round_to_i64(tenths(14)), Ok(1));
        assert_eq!(rule.round_to_i64(tenths(-14)), Ok(-1));
    }

    #[test]
    fn half_even_rounds_1_5_and_2_5_both_to_2() {
        // Banker's rounding: a tie goes to the even neighbour, so 1.5 rises and
        // 2.5 falls. Present because a jurisdiction policy may require it, and
        // deliberately not the default.
        let rule = RoundingRule::HalfEven;
        assert_eq!(rule.round_to_i64(tenths(15)), Ok(2));
        assert_eq!(rule.round_to_i64(tenths(25)), Ok(2));
        // Below zero it still seeks the even neighbour rather than fleeing zero.
        assert_eq!(rule.round_to_i64(tenths(-15)), Ok(-2));
        assert_eq!(rule.round_to_i64(tenths(-25)), Ok(-2));
    }

    #[test]
    fn floor_and_ceil_round_toward_their_own_infinity_below_zero() {
        // Below zero is where a plausible-looking implementation is usually
        // wrong. Floor is not truncation: -1.2 floors to -2, while truncating
        // toward zero answers -1 and quietly moves a fil to whoever the sign
        // belongs to.
        assert_eq!(RoundingRule::Floor.round_to_i64(tenths(-12)), Ok(-2));
        assert_eq!(RoundingRule::Ceil.round_to_i64(tenths(-12)), Ok(-1));
        assert_eq!(RoundingRule::Floor.round_to_i64(tenths(12)), Ok(1));
        assert_eq!(RoundingRule::Ceil.round_to_i64(tenths(12)), Ok(2));
        // Neither rule has a tie case: to them a half is just another fraction.
        assert_eq!(RoundingRule::Floor.round_to_i64(tenths(-15)), Ok(-2));
        assert_eq!(RoundingRule::Ceil.round_to_i64(tenths(-15)), Ok(-1));
        assert_eq!(RoundingRule::Floor.round_to_i64(tenths(15)), Ok(1));
        assert_eq!(RoundingRule::Ceil.round_to_i64(tenths(15)), Ok(2));
        // A whole value is left alone even by the two directional rules.
        assert_eq!(RoundingRule::Floor.round_to_i64(tenths(-20)), Ok(-2));
        assert_eq!(RoundingRule::Ceil.round_to_i64(tenths(-20)), Ok(-2));
    }

    #[test]
    fn each_rounding_rule_maps_to_its_own_decimal_strategy() {
        // The mapping onto `rust_decimal::RoundingStrategy` is the only thing
        // that can be wrong in `round_to_i64`, and a pasted match arm is the
        // cheapest way to get it wrong. These eight vectors separate the four
        // rules from each other AND from the three strategies none of them may
        // map to — MidpointTowardZero, ToZero and AwayFromZero each answer this
        // table differently from all four rows below.
        const VECTORS: [i64; 8] = [15, 25, -15, -25, 12, -12, 18, -18];
        let answers = |rule: RoundingRule| {
            VECTORS
                .iter()
                .map(|&value| rule.round_to_i64(tenths(value)))
                .collect::<Vec<_>>()
        };

        assert_eq!(
            answers(RoundingRule::HalfAwayFromZero),
            [Ok(2), Ok(3), Ok(-2), Ok(-3), Ok(1), Ok(-1), Ok(2), Ok(-2)]
        );
        assert_eq!(
            answers(RoundingRule::HalfEven),
            [Ok(2), Ok(2), Ok(-2), Ok(-2), Ok(1), Ok(-1), Ok(2), Ok(-2)]
        );
        assert_eq!(
            answers(RoundingRule::Floor),
            [Ok(1), Ok(2), Ok(-2), Ok(-3), Ok(1), Ok(-2), Ok(1), Ok(-2)]
        );
        assert_eq!(
            answers(RoundingRule::Ceil),
            [Ok(2), Ok(3), Ok(-1), Ok(-2), Ok(2), Ok(-1), Ok(2), Ok(-1)]
        );

        // And no two rules are the same strategy under another name.
        let tables: Vec<Vec<Result<i64, MoneyError>>> =
            ALL_RULES.iter().copied().map(answers).collect();
        for (index, left) in tables.iter().enumerate() {
            for right in tables.iter().skip(index + 1) {
                assert_ne!(left, right, "two rules answer identically everywhere");
            }
        }
    }

    #[test]
    fn a_value_outside_i64_is_an_error_not_a_saturating_cast() {
        // Decimal reaches ~7.9e28 and i64 stops at ~9.2e18. A saturating cast
        // here would turn an unrepresentable amount into a plausible one, and a
        // panic would lose the sale; both are worse than a handled error.
        for rule in ALL_RULES {
            assert_eq!(rule.round_to_i64(Decimal::MAX), Err(MoneyError::Overflow));
            assert_eq!(rule.round_to_i64(Decimal::MIN), Err(MoneyError::Overflow));
            // The last representable value at each end still converts.
            assert_eq!(rule.round_to_i64(Decimal::from(i64::MAX)), Ok(i64::MAX));
            assert_eq!(rule.round_to_i64(Decimal::from(i64::MIN)), Ok(i64::MIN));
            // One step past each end does not.
            assert_eq!(
                rule.round_to_i64(Decimal::from(i64::MAX) + Decimal::ONE),
                Err(MoneyError::Overflow)
            );
            assert_eq!(
                rule.round_to_i64(Decimal::from(i64::MIN) - Decimal::ONE),
                Err(MoneyError::Overflow)
            );
        }

        // Rounding itself can be what leaves the range: i64::MAX + 0.5 is an
        // exact Decimal, and only the rules that move it upward overflow.
        let just_over = Decimal::from(i64::MAX) + tenths(5);
        assert_eq!(
            RoundingRule::HalfAwayFromZero.round_to_i64(just_over),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            RoundingRule::HalfEven.round_to_i64(just_over),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            RoundingRule::Ceil.round_to_i64(just_over),
            Err(MoneyError::Overflow)
        );
        assert_eq!(RoundingRule::Floor.round_to_i64(just_over), Ok(i64::MAX));
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

        /// Quantity arithmetic is reversible at milli-unit precision: adding
        /// a signed quantity and then removing that same quantity returns the
        /// exact starting value whenever the sum is representable.
        #[test]
        fn prop_qty_add_sub_roundtrip((left_milli, right_milli) in qty_add_sub_cases()) {
            let left = Qty::from_milli(left_milli);
            let right = Qty::from_milli(right_milli);

            match left_milli.checked_add(right_milli) {
                Some(expected_sum) => {
                    let sum = left.checked_add(right).unwrap();
                    prop_assert_eq!(sum.milli(), expected_sum);
                    prop_assert_eq!(sum.checked_sub(right).unwrap(), left);
                }
                None => prop_assert_eq!(left.checked_add(right), Err(MoneyError::Overflow)),
            }
        }

        /// Rounding a value that is already whole leaves it exactly where it
        /// was, under every rule — including the two that always travel in one
        /// direction. A rule that moves an exact amount charges a fil for the
        /// arithmetic.
        #[test]
        fn prop_rounding_a_whole_value_is_the_identity((units, rule) in whole_value_cases()) {
            prop_assert_eq!(rule.round_to_i64(Decimal::from(units)), Ok(units));
        }

        /// Rounding reaches a neighbouring whole number and never further: the
        /// correction it applies is always smaller than one whole unit,
        /// whichever rule asked for it. A larger correction means a decimal
        /// place went missing on the way in.
        #[test]
        fn prop_rounding_moves_less_than_one_whole_unit((value, rule) in fractional_cases()) {
            let rounded = rule.round_to_i64(value).unwrap();
            prop_assert!((Decimal::from(rounded) - value).abs() < Decimal::ONE);
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
