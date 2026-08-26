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
    #[error("cannot parse {0:?} as an amount")]
    Parse(String),
    #[error("weights sum to zero; cannot prorate")]
    ZeroWeights,
    #[error("negative weight in a proration")]
    NegativeWeight,
    #[error("rounding step must be positive, got {0}")]
    InvalidStep(i64),
    /// Carries the value as written and the decimal places that were available,
    /// because "not exact" is unactionable without both. `Percent` reports its
    /// four; `Money::format` (1.1.2b) reports the store's.
    #[error("{0} is not exact at {1} decimals")]
    NotRepresentableAtPrecision(String, u8),
    /// A conversion whose input cannot be held at all, as opposed to `Overflow`
    /// on an arithmetic step between two values that could.
    #[error("value out of representable range")]
    OutOfRange,
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

    /// Multiply a unit price by a signed quantity in milli-units.
    ///
    /// The product remains an exact `Decimal` until the one reduction through
    /// `RoundingRule::round_to_i64`. A checked decimal product turns magnitudes
    /// outside `Decimal` into `Overflow` rather than panicking.
    pub fn mul_qty(self, qty: Qty, rule: RoundingRule) -> Result<Money, MoneyError> {
        let exact_minor = Decimal::from(self.minor)
            .checked_mul(qty.to_decimal())
            .ok_or(MoneyError::Overflow)?;
        rule.round_to_i64(exact_minor)
            .map(|minor| Money::from_minor(minor, self.currency))
    }

    /// Apply a signed parts-per-million rate to this amount.
    ///
    /// `Percent::to_decimal` supplies the exact fractional rate. As with
    /// quantity multiplication, the final call is the sole rounding point.
    pub fn mul_percent(self, pct: Percent, rule: RoundingRule) -> Result<Money, MoneyError> {
        let exact_minor = Decimal::from(self.minor)
            .checked_mul(pct.to_decimal())
            .ok_or(MoneyError::Overflow)?;
        rule.round_to_i64(exact_minor)
            .map(|minor| Money::from_minor(minor, self.currency))
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

    /// Split proportionally by same-currency money values.
    ///
    /// This entry point owns only the currency check and the projection to
    /// integer minor-unit weights. All allocation mechanics live in
    /// `split_proportional_by`, so value and quantity proration cannot drift.
    pub fn split_proportional(self, weights: &[Money]) -> Result<Vec<Money>, MoneyError> {
        let integer_weights = weights
            .iter()
            .map(|weight| {
                self.ensure_same_currency(*weight)?;
                Ok(weight.minor())
            })
            .collect::<Result<Vec<_>, MoneyError>>()?;

        self.split_proportional_by(&integer_weights)
    }

    /// Split proportionally by arbitrary non-negative integer weights.
    ///
    /// Exact integer quotients form the base allocation. Remaining minor units
    /// go to the first positive weights in original index order, exactly as
    /// `split_evenly` awards its first N pieces. Inputs and outputs never move,
    /// and the primitive learns no identity. Signed totals use the same
    /// magnitude allocation and apply their sign at the end, including the
    /// full `i64::MIN` magnitude.
    pub fn split_proportional_by(self, weights: &[i64]) -> Result<Vec<Money>, MoneyError> {
        if weights.iter().any(|weight| *weight < 0) {
            return Err(MoneyError::NegativeWeight);
        }

        let total_weight = weights.iter().try_fold(0_u128, |sum, weight| {
            let weight = u128::try_from(*weight).map_err(|_| MoneyError::NegativeWeight)?;
            sum.checked_add(weight).ok_or(MoneyError::Overflow)
        })?;
        if total_weight == 0 {
            return Err(MoneyError::ZeroWeights);
        }

        let magnitude = u128::from(self.minor.unsigned_abs());
        let mut allocated = 0_u128;
        let mut shares = Vec::with_capacity(weights.len());

        for weight in weights.iter().copied() {
            let weight = u128::try_from(weight).map_err(|_| MoneyError::NegativeWeight)?;
            let numerator = magnitude.checked_mul(weight).ok_or(MoneyError::Overflow)?;
            let base = numerator / total_weight;

            allocated = allocated.checked_add(base).ok_or(MoneyError::Overflow)?;
            shares.push(base);
        }

        let mut undistributed = magnitude
            .checked_sub(allocated)
            .ok_or(MoneyError::Overflow)?;
        for (share, weight) in shares.iter_mut().zip(weights) {
            if undistributed == 0 {
                break;
            }
            if *weight > 0 {
                *share = share.checked_add(1).ok_or(MoneyError::Overflow)?;
                undistributed -= 1;
            }
        }
        if undistributed != 0 {
            return Err(MoneyError::Overflow);
        }

        shares
            .into_iter()
            .map(|share| {
                let magnitude = i128::try_from(share).map_err(|_| MoneyError::Overflow)?;
                let signed = if self.minor < 0 {
                    magnitude.checked_neg().ok_or(MoneyError::Overflow)?
                } else {
                    magnitude
                };
                let minor = i64::try_from(signed).map_err(|_| MoneyError::Overflow)?;
                Ok(Money::from_minor(minor, self.currency))
            })
            .collect()
    }

    /// Round to a positive minor-unit step using numeric-order directions.
    ///
    /// `Up` is toward positive infinity and `Down` toward negative infinity,
    /// so below zero `Up` moves toward zero while `Down` moves away from it.
    /// `Nearest` chooses the closer multiple and breaks an exact half-step tie
    /// away from zero. Which direction a cash collection or refund *selects*
    /// is policy outside this primitive.
    pub fn round_to_step(
        self,
        step_minor: i64,
        dir: RoundingDirection,
    ) -> Result<Money, MoneyError> {
        if step_minor <= 0 {
            return Err(MoneyError::InvalidStep(step_minor));
        }

        let amount = i128::from(self.minor);
        let step = i128::from(step_minor);
        let lower = amount
            .div_euclid(step)
            .checked_mul(step)
            .ok_or(MoneyError::Overflow)?;
        let lower_distance = amount.rem_euclid(step);
        let upper = if lower_distance == 0 {
            lower
        } else {
            lower.checked_add(step).ok_or(MoneyError::Overflow)?
        };
        let upper_distance = upper.checked_sub(amount).ok_or(MoneyError::Overflow)?;

        let rounded = match dir {
            RoundingDirection::Down => lower,
            RoundingDirection::Up => upper,
            RoundingDirection::Nearest => match lower_distance.cmp(&upper_distance) {
                core::cmp::Ordering::Less => lower,
                core::cmp::Ordering::Greater => upper,
                core::cmp::Ordering::Equal if amount < 0 => lower,
                core::cmp::Ordering::Equal => upper,
            },
        };
        let minor = i64::try_from(rounded).map_err(|_| MoneyError::Overflow)?;
        Ok(Money::from_minor(minor, self.currency))
    }

    /// Convert exactly from stored minor units to major-unit decimal form.
    pub fn to_decimal(self) -> Decimal {
        Decimal::new(self.minor, u32::from(self.currency.exponent()))
    }

    /// Convert a major-unit decimal into money, rounding exactly once.
    pub fn from_decimal(
        decimal: Decimal,
        currency: Currency,
        rule: RoundingRule,
    ) -> Result<Money, MoneyError> {
        let exact_minor = decimal
            .checked_mul(Decimal::from(currency.minor_per_major()))
            .ok_or(MoneyError::Overflow)?;
        rule.round_to_i64(exact_minor)
            .map(|minor| Money::from_minor(minor, currency))
    }

    /// Render a catalogue amount at `decimals`, refusing hidden minor units.
    ///
    /// More decimal places append zeros and fewer are accepted only when the
    /// omitted places are all zero. No rounding rule is accepted here, so an
    /// inexact shorter display is a named error rather than a rounded price.
    pub fn format(self, decimals: u8) -> Result<String, MoneyError> {
        let exponent = self.currency.exponent();
        if decimals >= exponent {
            let mut rendered = self.format_exact();
            if decimals > exponent {
                if exponent == 0 {
                    rendered.push('.');
                }
                rendered.extend(core::iter::repeat_n('0', usize::from(decimals - exponent)));
            }
            return Ok(rendered);
        }

        let divisor = 10_i64
            .checked_pow(u32::from(exponent - decimals))
            .ok_or(MoneyError::OutOfRange)?;
        if self.minor % divisor != 0 {
            return Err(MoneyError::NotRepresentableAtPrecision(
                self.format_exact(),
                decimals,
            ));
        }

        Ok(format_fixed(self.minor / divisor, decimals))
    }

    /// Render at the currency's own exponent, always and without a setting.
    pub fn format_exact(self) -> String {
        format_fixed(self.minor, self.currency.exponent())
    }

    /// Parse an exact major-unit amount for `currency` without rounding.
    ///
    /// The grammar is ASCII decimal: an optional leading sign, at least one
    /// integer digit, and an optional non-empty fractional part. Fewer digits
    /// than the currency exponent are padded; extra trailing zeros are accepted
    /// because they carry no extra value, while any excess non-zero precision
    /// is refused. Whitespace, grouping separators and exponent notation are
    /// not part of the grammar.
    pub fn parse(input: &str, currency: Currency) -> Result<Money, MoneyError> {
        let (negative, unsigned) = if let Some(rest) = input.strip_prefix('-') {
            (true, rest)
        } else if let Some(rest) = input.strip_prefix('+') {
            (false, rest)
        } else {
            (false, input)
        };

        if unsigned.is_empty() {
            return Err(MoneyError::Parse(input.to_owned()));
        }

        let (whole, fraction, has_decimal_point) =
            if let Some((whole, fraction)) = unsigned.split_once('.') {
                (whole, fraction, true)
            } else {
                (unsigned, "", false)
            };
        if whole.is_empty()
            || (has_decimal_point && fraction.is_empty())
            || whole.bytes().any(|byte| !byte.is_ascii_digit())
            || fraction.bytes().any(|byte| !byte.is_ascii_digit())
        {
            return Err(MoneyError::Parse(input.to_owned()));
        }

        let exponent = usize::from(currency.exponent());
        if fraction.bytes().skip(exponent).any(|digit| digit != b'0') {
            return Err(MoneyError::NotRepresentableAtPrecision(
                input.to_owned(),
                currency.exponent(),
            ));
        }

        let carried_fraction = fraction.bytes().take(exponent);
        let padding = exponent.saturating_sub(fraction.len().min(exponent));
        let digits = whole
            .bytes()
            .chain(carried_fraction)
            .chain(core::iter::repeat_n(b'0', padding));
        let limit = if negative {
            i64::MIN.unsigned_abs()
        } else {
            i64::MAX.unsigned_abs()
        };
        let mut magnitude = 0_u64;
        for digit in digits {
            magnitude = magnitude
                .checked_mul(10)
                .and_then(|value| value.checked_add(u64::from(digit - b'0')))
                .ok_or(MoneyError::OutOfRange)?;
            if magnitude > limit {
                return Err(MoneyError::OutOfRange);
            }
        }

        let minor = if negative && magnitude == i64::MIN.unsigned_abs() {
            i64::MIN
        } else {
            let magnitude = i64::try_from(magnitude).map_err(|_| MoneyError::OutOfRange)?;
            if negative { -magnitude } else { magnitude }
        };
        Ok(Money::from_minor(minor, currency))
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

/// Render a signed integer at a fixed decimal scale without negating
/// `i64::MIN` and without relying on decimal formatting to round nothing.
fn format_fixed(value: i64, decimals: u8) -> String {
    let digits = value.unsigned_abs().to_string();
    let decimals = usize::from(decimals);
    let mut rendered = String::new();

    if value < 0 {
        rendered.push('-');
    }
    if decimals == 0 {
        rendered.push_str(&digits);
        return rendered;
    }
    if digits.len() <= decimals {
        rendered.push_str("0.");
        rendered.extend(core::iter::repeat_n('0', decimals - digits.len()));
        rendered.push_str(&digits);
        return rendered;
    }

    let decimal_index = digits.len() - decimals;
    for (index, digit) in digits.chars().enumerate() {
        if index == decimal_index {
            rendered.push('.');
        }
        rendered.push(digit);
    }
    rendered
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

/// A rate in signed integer parts-per-million.
///
/// **16% is `160_000`, 4% is `40_000`, 0.5% is `5_000`** (conventions §2). The
/// source blueprint says basis points and `rate_bp`; parts-per-million
/// supersedes it (`00-master-plan.md` §4a.2), because Jordan's reduced rates
/// already include 1% and 2% and nothing guarantees the next decree lands on a
/// whole basis point. The extra factor of a hundred costs nothing and ends the
/// "we cannot represent 0.125%" conversation permanently.
///
/// Used for tax rates, discount percentages and margin floors. Like `Qty` and
/// unlike `Money`, a rate carries no external dimension that could make two
/// values incomparable, so the derived ordering is the honest numeric ordering
/// of the underlying ppm, and `ZERO` is a rate rather than an invented default.
///
/// **The two decimal projections point in opposite directions, on purpose.**
/// `to_percent_decimal` is the percentage a human reads and types — `160_000`
/// is `16` — and `from_percent_decimal` is its exact inverse. `to_decimal` is
/// the fraction the arithmetic multiplies by — `160_000` is `0.16` — because
/// `net × r` and `gross / (1 + r)` (`ref/tax-jordan.md`) need `r`, not `100 r`.
/// Collapsing the two would hide a ÷100 at every tax site, and a rate wrong by
/// two orders of magnitude is not a rate anyone notices in a code review.
///
/// **A negative rate is representable, and nothing here forbids one.**
/// `from_ppm` is `const` and infallible by specification, so a refusal written
/// here would be a comment rather than a rule. The sign restriction on a *tax*
/// rate belongs where one is built and stored — `CHECK (rate_ppm >= 0)` in
/// `ref/schema.md` and group 1.3's rate resolution — and a discount stays a
/// positive rate with the direction living at the call site, which is how
/// `Money::mul_percent` (1.1.2b) reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Percent(i64);

impl Percent {
    pub const ZERO: Percent = Percent(0);

    /// One whole percent, in ppm — and therefore the factor between the two
    /// decimal projections below. It exists once so that nothing else in the
    /// crate has a reason to spell `10_000`.
    const PPM_PER_PERCENT: i64 = 10_000;

    /// Decimal places of a *percentage* that ppm represents exactly: 1 ppm is
    /// `0.0001%`. It is what `NotRepresentableAtPrecision` reports, because the
    /// caller typed a percentage and needs to be told about that rather than
    /// about the integer underneath it.
    const PERCENT_DECIMALS: u8 = 4;

    /// Decimal places of the *fraction*: 1 ppm is `0.000001`.
    const FRACTION_DECIMALS: u8 = 6;

    pub const fn from_ppm(ppm: i64) -> Percent {
        Percent(ppm)
    }

    pub const fn ppm(self) -> i64 {
        self.0
    }

    /// Read a rate the way a decree, a settings row or a cashier writes one:
    /// as a **percentage**. `16` is 16%, and `0.5` is half a percent.
    ///
    /// The inverse is `to_percent_decimal`, not `to_decimal` — see the type's
    /// note on the two directions.
    ///
    /// Two ways to fail, and neither of them is a rounding:
    ///
    /// * A value finer than one ppm — `0.00001%` — is
    ///   `NotRepresentableAtPrecision`, carrying the value and the four decimal
    ///   places a caller may use. There is no `RoundingRule` argument here, and
    ///   I-1 puts rounding only where a rule was passed in, so quietly keeping
    ///   `0.0000%` of a rate is not on offer: a truncated rate is a mispriced
    ///   line on every sale that uses it, for as long as nobody notices.
    /// * A value beyond ±`i64` ppm is `OutOfRange`, which also covers one so
    ///   large that scaling it to ppm overflows `Decimal` itself.
    ///   `checked_mul` is what keeps that a handled error: plain `Decimal`
    ///   multiplication panics with "Multiplication overflowed", and
    ///   `Decimal::MAX` scaled to ppm does overflow.
    ///
    /// The exactness test runs first, so a value that is both imprecise and out
    /// of range reports the precision error. Either answer is a refusal and
    /// neither is a number, which is the property that matters; a caller
    /// branching on which one has a rate problem this type cannot fix.
    ///
    /// Exactness is the test, not scale: `16.000000` carries six decimal places
    /// and is exactly 16%, so it is accepted.
    pub fn from_percent_decimal(percent: Decimal) -> Result<Percent, MoneyError> {
        let ppm = percent
            .checked_mul(Decimal::from(Self::PPM_PER_PERCENT))
            .ok_or(MoneyError::OutOfRange)?;
        if ppm.fract() != Decimal::ZERO {
            return Err(MoneyError::NotRepresentableAtPrecision(
                percent.to_string(),
                Self::PERCENT_DECIMALS,
            ));
        }
        ppm.to_i64().map(Percent).ok_or(MoneyError::OutOfRange)
    }

    /// The rate as the **fraction** the arithmetic multiplies by: `160_000`
    /// becomes `0.16`. Exact, at a fixed scale of six, with no division and no
    /// float — the same integer-mantissa construction as `Qty::to_decimal`.
    pub fn to_decimal(self) -> Decimal {
        Decimal::new(self.0, u32::from(Self::FRACTION_DECIMALS))
    }

    /// The rate as the **percentage** a human reads: `160_000` becomes `16`.
    /// The exact inverse of `from_percent_decimal` over every representable
    /// rate, which is the pair `prop_percent_decimal_roundtrip` attacks.
    pub fn to_percent_decimal(self) -> Decimal {
        Decimal::new(self.0, u32::from(Self::PERCENT_DECIMALS))
    }

    /// `"16%"`, `"0.5%"`, `"0%"` — the percentage, with its trailing zeros
    /// removed and nothing else changed.
    ///
    /// The four decimal places of `to_percent_decimal` belong to the
    /// representation, not to the rate: rendered literally, every whole percent
    /// reads `"16.0000%"`. `Decimal::normalize` drops trailing zeros only, so
    /// `0.5%` and `0.0001%` keep every digit they have and nothing here rounds.
    /// A rate display that rounds is a rate display that lies — `0.0001%` shown
    /// as `0%` is a charge a merchant would swear was not being made.
    pub fn format(self) -> String {
        format!("{}%", self.to_percent_decimal().normalize())
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
/// `Money::round_to_step` lands in microstep 1.1.2b. Its mechanics use numeric
/// order: `Up` means positive infinity and `Down` negative infinity, so below
/// zero `Up` moves toward zero and `Down` away from it; `Nearest` breaks exact
/// half-step ties away from zero. Microstep 1.5.3 owns `compute_cash_rounding`,
/// the policy that applies this primitive only to a final cash tender's
/// remaining amount, using the selected direction. The direction for a cash
/// refund payout remains the separate open question in `ref/tax-jordan.md` §5;
/// these mechanics do not decide it.
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
    use core::str::FromStr;
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

    // Covers the round trip from both ends. `ppm` sweeps the ENTIRE signed i64
    // ppm space, so every rate a `Percent` can hold is generated, i64::MIN,
    // i64::MAX and zero included. `percent` builds the other end: every
    // percentage expressible at 0 through 4 decimal places — four is the finest
    // ppm holds, since 1 ppm is 0.0001% — with the mantissa bounded to ±10^14
    // so the exact ppm value it denotes stays inside i64. Deliberately
    // excluded: percentages finer than one ppm and magnitudes past ±i64 ppm.
    // Those are the constructor's two error branches, pinned by
    // `a_rate_finer_than_one_ppm_is_refused_not_rounded` and
    // `a_rate_beyond_i64_ppm_is_out_of_range`; no conservation claim can be made
    // about a value the type refuses to hold.
    fn percent_decimal_roundtrip_cases() -> impl Strategy<Value = (i64, Decimal)> {
        (
            any::<i64>(),
            -100_000_000_000_000i64..=100_000_000_000_000,
            0u32..=4,
        )
            .prop_map(|(ppm, mantissa, places)| (ppm, Decimal::new(mantissa, places)))
    }

    // Covers every rate the type can hold and nothing else: both properties
    // below are claims about all of them, so the whole i64 ppm space is exactly
    // the input space. Nothing is excluded — neither projection nor the render
    // has a failure mode to carve out.
    fn every_representable_rate() -> impl Strategy<Value = i64> {
        any::<i64>()
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

    // Covers the full signed i64 amount space, every known currency, and
    // non-empty weight vectors of length 1 through 16 containing deliberately
    // frequent zeros and positive values across the full i64 range. Negative
    // and all-zero weights are deliberately excluded because they are named
    // error branches rather than conservation cases.
    fn proportional_integer_cases() -> impl Strategy<Value = (i64, Vec<i64>, Currency)> {
        (
            any::<i64>(),
            prop::collection::vec(prop_oneof![1 => Just(0_i64), 3 => 1_i64..=i64::MAX], 1..17)
                .prop_filter("at least one weight is positive", |weights| {
                    weights.iter().any(|weight| *weight > 0)
                }),
            known_currency(),
        )
    }

    // Covers the same signed totals and non-negative, non-zero aggregate
    // weights as `proportional_integer_cases`, projected into same-currency
    // `Money` values. Mixed currencies and negative money weights are excluded
    // here because their dedicated examples pin those refusal paths.
    fn proportional_money_cases() -> impl Strategy<Value = (i64, Vec<Money>, Currency)> {
        proportional_integer_cases().prop_map(|(minor, weights, currency)| {
            let weights = weights
                .into_iter()
                .map(|weight| Money::from_minor(weight, currency))
                .collect();
            (minor, weights, currency)
        })
    }

    // Covers signed prices through one trillion minor units, zero through 64
    // whole units, every known currency and every rounding rule. Larger counts
    // and magnitudes that could overflow repeated addition are deliberately
    // excluded; explicit examples own the overflow branches.
    fn mul_qty_whole_unit_cases() -> impl Strategy<Value = (i64, u8, Currency, RoundingRule)> {
        (
            -1_000_000_000_000_i64..=1_000_000_000_000,
            0_u8..=64,
            known_currency(),
            every_rounding_rule(),
        )
    }

    // Covers the entire stored amount space in every known currency. Nothing
    // is excluded: `format_exact` and `parse` are total over every `Money`.
    fn money_exact_roundtrip_cases() -> impl Strategy<Value = (i64, Currency)> {
        (any::<i64>(), known_currency())
    }

    // Covers each cash direction and nothing else. It is separate from tax's
    // `every_rounding_rule` because the two axes must never be conflated.
    fn every_rounding_direction() -> impl Strategy<Value = RoundingDirection> {
        prop_oneof![
            Just(RoundingDirection::Nearest),
            Just(RoundingDirection::Up),
            Just(RoundingDirection::Down),
        ]
    }

    // Covers positive and negative amounts through 10^15 minor units, steps 1
    // through one million, and all three directions. Values close enough to an
    // i64 edge for the selected multiple to overflow are deliberately excluded;
    // the boundary example owns those handled errors.
    fn round_to_step_cases() -> impl Strategy<Value = (i64, i64, RoundingDirection)> {
        (
            -1_000_000_000_000_000_i64..=1_000_000_000_000_000,
            1_i64..=1_000_000,
            every_rounding_direction(),
        )
    }

    // Covers the same signed amounts and positive steps as
    // `round_to_step_cases`, restricted to `Nearest` because the half-step
    // distance bound is not a claim about directional rounding. Boundary
    // overflow remains deliberately excluded and separately tested.
    fn nearest_step_cases() -> impl Strategy<Value = (i64, i64)> {
        (
            -1_000_000_000_000_000_i64..=1_000_000_000_000_000,
            1_i64..=1_000_000,
        )
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
    fn money_decimal_arithmetic_uses_the_selected_rounding_rule() {
        // All three paths carry an exact half-minor intermediate into the one
        // shared rounding primitive. The separating vectors prove the rule is
        // actually threaded rather than a caller rounding or truncating first.
        let one_fil = Money::from_minor(1, Currency::JOD);
        let half_unit = Qty::from_milli(500);
        assert_eq!(
            one_fil.mul_qty(half_unit, RoundingRule::HalfAwayFromZero),
            Ok(one_fil)
        );
        assert_eq!(
            one_fil.mul_qty(half_unit, RoundingRule::HalfEven),
            Ok(Money::zero(Currency::JOD))
        );
        assert_eq!(
            one_fil.mul_qty(Qty::from_milli(-500), RoundingRule::HalfAwayFromZero),
            Ok(Money::from_minor(-1, Currency::JOD))
        );
        assert_eq!(
            one_fil.mul_qty(Qty::from_milli(-500), RoundingRule::Ceil),
            Ok(Money::zero(Currency::JOD))
        );

        let five_fils = Money::from_minor(5, Currency::JOD);
        let ten_percent = Percent::from_ppm(100_000);
        assert_eq!(
            five_fils.mul_percent(ten_percent, RoundingRule::HalfAwayFromZero),
            Ok(one_fil)
        );
        assert_eq!(
            five_fils.mul_percent(ten_percent, RoundingRule::HalfEven),
            Ok(Money::zero(Currency::JOD))
        );
        assert_eq!(
            five_fils.mul_percent(Percent::from_ppm(-100_000), RoundingRule::Floor),
            Ok(Money::from_minor(-1, Currency::JOD))
        );

        let one_point_2585 = Decimal::new(12_585, 4);
        assert_eq!(
            Money::from_decimal(
                one_point_2585,
                Currency::JOD,
                RoundingRule::HalfAwayFromZero,
            ),
            Ok(Money::from_minor(1_259, Currency::JOD))
        );
        assert_eq!(
            Money::from_decimal(one_point_2585, Currency::JOD, RoundingRule::HalfEven),
            Ok(Money::from_minor(1_258, Currency::JOD))
        );
    }

    #[test]
    fn money_to_decimal_uses_each_currency_exponent_exactly() {
        let jod = Money::from_minor(1_259, Currency::JOD).to_decimal();
        assert_eq!(jod, Decimal::new(1_259, 3));
        assert_eq!(jod.scale(), 3);

        let usd = Money::from_minor(1_259, Currency::USD).to_decimal();
        assert_eq!(usd, Decimal::new(1_259, 2));
        assert_eq!(usd.scale(), 2);

        assert_eq!(
            Money::from_minor(i64::MIN, Currency::JOD).to_decimal(),
            Decimal::new(i64::MIN, 3)
        );
        assert_eq!(
            Money::from_minor(i64::MAX, Currency::JOD).to_decimal(),
            Decimal::new(i64::MAX, 3)
        );
    }

    #[test]
    fn money_decimal_arithmetic_reports_every_overflow() {
        let max = Money::from_minor(i64::MAX, Currency::JOD);
        assert_eq!(
            max.mul_qty(Qty::from_units(2).unwrap(), RoundingRule::HalfAwayFromZero),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            max.mul_qty(Qty::from_milli(i64::MAX), RoundingRule::HalfAwayFromZero,),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            max.mul_percent(Percent::from_ppm(i64::MAX), RoundingRule::HalfAwayFromZero,),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            Money::from_decimal(Decimal::MAX, Currency::JOD, RoundingRule::HalfAwayFromZero,),
            Err(MoneyError::Overflow)
        );
    }

    #[test]
    fn proportional_split_distributes_remainders_by_input_index() {
        let amount = Money::from_minor(5, Currency::JOD);
        assert_eq!(
            amount.split_proportional_by(&[1, 2, 4]),
            Ok(vec![
                Money::from_minor(1, Currency::JOD),
                Money::from_minor(2, Currency::JOD),
                Money::from_minor(2, Currency::JOD),
            ])
        );

        // Original indices zero and one get the two residual fils; no product
        // identity or internal reordering enters the primitive.
        assert_eq!(
            Money::from_minor(2, Currency::JOD).split_proportional_by(&[1, 1, 1]),
            Ok(vec![
                Money::from_minor(1, Currency::JOD),
                Money::from_minor(1, Currency::JOD),
                Money::zero(Currency::JOD),
            ])
        );
        assert_eq!(
            Money::from_minor(-2, Currency::JOD).split_proportional_by(&[1, 1, 1]),
            Ok(vec![
                Money::from_minor(-1, Currency::JOD),
                Money::from_minor(-1, Currency::JOD),
                Money::zero(Currency::JOD),
            ])
        );

        // Zero weights remain zero even when they appear before the recipients
        // of residual units.
        assert_eq!(
            Money::from_minor(7, Currency::JOD).split_proportional_by(&[0, 3, 0, 1]),
            Ok(vec![
                Money::zero(Currency::JOD),
                Money::from_minor(6, Currency::JOD),
                Money::zero(Currency::JOD),
                Money::from_minor(1, Currency::JOD),
            ])
        );
    }

    #[test]
    fn proportional_split_refuses_invalid_weights_and_currency() {
        let amount = Money::from_minor(10, Currency::JOD);
        assert_eq!(
            amount.split_proportional_by(&[]),
            Err(MoneyError::ZeroWeights)
        );
        assert_eq!(
            Money::zero(Currency::JOD).split_proportional_by(&[0, 0]),
            Err(MoneyError::ZeroWeights),
            "a zero total does not create a proportion where none exists"
        );
        assert_eq!(
            amount.split_proportional_by(&[1, -1]),
            Err(MoneyError::NegativeWeight)
        );
        assert_eq!(
            amount.split_proportional(&[
                Money::from_minor(1, Currency::JOD),
                Money::from_minor(1, Currency::USD),
            ]),
            Err(MoneyError::CurrencyMismatch("JOD", "USD"))
        );
        assert_eq!(
            amount.split_proportional(&[Money::from_minor(-1, Currency::JOD)]),
            Err(MoneyError::NegativeWeight)
        );
    }

    #[test]
    fn proportional_split_handles_the_full_signed_range() {
        assert_eq!(
            Money::from_minor(i64::MIN, Currency::JOD).split_proportional_by(&[1]),
            Ok(vec![Money::from_minor(i64::MIN, Currency::JOD)])
        );
        assert_eq!(
            Money::from_minor(i64::MAX, Currency::JOD).split_proportional_by(&[1]),
            Ok(vec![Money::from_minor(i64::MAX, Currency::JOD)])
        );

        for minor in [i64::MIN, i64::MAX] {
            let pieces = Money::from_minor(minor, Currency::JOD)
                .split_proportional_by(&[i64::MAX, i64::MAX])
                .unwrap();
            assert_eq!(
                pieces
                    .iter()
                    .map(|piece| i128::from(piece.minor()))
                    .sum::<i128>(),
                i128::from(minor)
            );
        }
    }

    #[test]
    fn format_truncating_a_fil_is_refused() {
        let amount = Money::from_minor(1_259, Currency::JOD);
        assert_eq!(
            amount.format(2),
            Err(MoneyError::NotRepresentableAtPrecision(
                "1.259".to_owned(),
                2,
            ))
        );
        assert_eq!(amount.format_exact(), "1.259");

        // A three-fil cash adjustment must never be displayed as zero.
        let adjustment = Money::from_minor(3, Currency::JOD);
        assert!(matches!(
            adjustment.format(2),
            Err(MoneyError::NotRepresentableAtPrecision(..))
        ));
        assert_eq!(adjustment.format_exact(), "0.003");
    }

    #[test]
    fn exact_catalogue_precision_formats_without_hiding_value() {
        let amount = Money::from_minor(1_250, Currency::JOD);
        assert_eq!(
            amount.format(0),
            Err(MoneyError::NotRepresentableAtPrecision(
                "1.250".to_owned(),
                0
            ))
        );
        assert_eq!(amount.format(2), Ok("1.25".to_owned()));
        assert_eq!(amount.format(3), Ok("1.250".to_owned()));
        assert_eq!(amount.format(4), Ok("1.2500".to_owned()));
        assert_eq!(Money::zero(Currency::JOD).format(2), Ok("0.00".to_owned()));
        assert_eq!(
            Money::from_minor(-1_200, Currency::JOD).format(2),
            Ok("-1.20".to_owned())
        );
        assert_eq!(
            Money::from_minor(i64::MIN, Currency::JOD).format_exact(),
            "-9223372036854775.808"
        );
        assert_eq!(
            Money::from_minor(i64::MAX, Currency::JOD).format_exact(),
            "9223372036854775.807"
        );
    }

    #[test]
    fn parse_accepts_exact_decimal_forms_without_rounding() {
        let jod = Currency::JOD;
        assert_eq!(Money::parse("1", jod), Ok(Money::from_minor(1_000, jod)));
        assert_eq!(Money::parse("1.2", jod), Ok(Money::from_minor(1_200, jod)));
        assert_eq!(
            Money::parse("+1.259", jod),
            Ok(Money::from_minor(1_259, jod))
        );
        assert_eq!(Money::parse("-0.001", jod), Ok(Money::from_minor(-1, jod)));
        assert_eq!(
            Money::parse("1.259000", jod),
            Ok(Money::from_minor(1_259, jod)),
            "extra trailing zeros carry no additional precision"
        );
        assert_eq!(
            Money::parse("1.2591", jod),
            Err(MoneyError::NotRepresentableAtPrecision(
                "1.2591".to_owned(),
                3,
            ))
        );
        assert_eq!(
            Money::parse("9223372036854775.807", jod),
            Ok(Money::from_minor(i64::MAX, jod))
        );
        assert_eq!(
            Money::parse("-9223372036854775.808", jod),
            Ok(Money::from_minor(i64::MIN, jod))
        );
        assert_eq!(
            Money::parse("9223372036854775.808", jod),
            Err(MoneyError::OutOfRange)
        );
    }

    #[test]
    fn parse_rejects_non_decimal_syntax_and_lone_signs() {
        for input in [
            "", "-", "+", " 1.259", "1.259 ", "1,259", "1e3", ".5", "1.", "--1", "+-1", "1.2.3",
        ] {
            assert_eq!(
                Money::parse(input, Currency::JOD),
                Err(MoneyError::Parse(input.to_owned())),
                "{input:?} is outside the amount grammar"
            );
        }
    }

    #[test]
    fn round_to_step_defines_every_direction_below_zero() {
        let negative = Money::from_minor(-1_245, Currency::JOD);
        assert_eq!(
            negative.round_to_step(10, RoundingDirection::Nearest),
            Ok(Money::from_minor(-1_250, Currency::JOD))
        );
        assert_eq!(
            negative.round_to_step(10, RoundingDirection::Up),
            Ok(Money::from_minor(-1_240, Currency::JOD))
        );
        assert_eq!(
            negative.round_to_step(10, RoundingDirection::Down),
            Ok(Money::from_minor(-1_250, Currency::JOD))
        );

        let positive = Money::from_minor(1_245, Currency::JOD);
        assert_eq!(
            positive.round_to_step(10, RoundingDirection::Nearest),
            Ok(Money::from_minor(1_250, Currency::JOD))
        );
        assert_eq!(
            positive.round_to_step(10, RoundingDirection::Up),
            Ok(Money::from_minor(1_250, Currency::JOD))
        );
        assert_eq!(
            positive.round_to_step(10, RoundingDirection::Down),
            Ok(Money::from_minor(1_240, Currency::JOD))
        );
    }

    #[test]
    fn round_to_step_refuses_invalid_steps_and_reports_boundary_overflow() {
        let amount = Money::from_minor(7, Currency::JOD);
        assert_eq!(
            amount.round_to_step(0, RoundingDirection::Nearest),
            Err(MoneyError::InvalidStep(0))
        );
        assert_eq!(
            amount.round_to_step(-10, RoundingDirection::Nearest),
            Err(MoneyError::InvalidStep(-10))
        );

        let max = Money::from_minor(i64::MAX, Currency::JOD);
        assert_eq!(
            max.round_to_step(10, RoundingDirection::Nearest),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            max.round_to_step(10, RoundingDirection::Up),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            max.round_to_step(10, RoundingDirection::Down),
            Ok(Money::from_minor(9_223_372_036_854_775_800, Currency::JOD))
        );

        let min = Money::from_minor(i64::MIN, Currency::JOD);
        assert_eq!(
            min.round_to_step(10, RoundingDirection::Nearest),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            min.round_to_step(10, RoundingDirection::Down),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            min.round_to_step(10, RoundingDirection::Up),
            Ok(Money::from_minor(-9_223_372_036_854_775_800, Currency::JOD))
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
    fn sixteen_percent_is_160000_ppm() {
        // The rate on the front of every Jordanian receipt, and the first
        // number a reader checks. 16% is 160_000 ppm — not 1_600 basis points,
        // which is what the superseded `rate_bp` would have held.
        assert_eq!(Percent::from_ppm(160_000).ppm(), 160_000);
        assert_eq!(
            Percent::from_percent_decimal(Decimal::from(16)),
            Ok(Percent::from_ppm(160_000))
        );
        assert_eq!(Percent::from_ppm(160_000).format(), "16%");

        // The rates that made ppm rather than basis points the choice
        // (conventions §2): the reduced rates already in Jordanian law, and one
        // no whole number of basis points can hold.
        for (percentage, ppm) in [(4, 40_000), (2, 20_000), (1, 10_000), (0, 0)] {
            assert_eq!(
                Percent::from_percent_decimal(Decimal::from(percentage)),
                Ok(Percent::from_ppm(ppm))
            );
        }
        assert_eq!(
            Percent::from_percent_decimal(Decimal::new(5, 1)),
            Ok(Percent::from_ppm(5_000)),
            "0.5% is 5_000 ppm"
        );
        assert_eq!(
            Percent::from_percent_decimal(Decimal::new(125, 3)),
            Ok(Percent::from_ppm(1_250)),
            "0.125% is 1_250 ppm, and 12.5 basis points"
        );
        assert_eq!(Percent::ZERO, Percent::from_ppm(0));
    }

    #[test]
    fn percent_to_decimal_is_the_fraction_not_the_percentage() {
        // The one ambiguity in this type, pinned in both directions. A rate
        // entered as the percentage 16 multiplies as 0.16; an implementation
        // that answered 16 here would multiply every taxed line by a hundred,
        // and one that answered 0.16 from `to_percent_decimal` would print a
        // receipt claiming a sixth of a percent of tax.
        let standard = Percent::from_ppm(160_000);
        assert_eq!(standard.to_decimal(), Decimal::new(16, 2));
        assert_eq!(standard.to_percent_decimal(), Decimal::from(16));

        // Exact at a fixed scale, like `Qty::to_decimal`: no division, no float.
        assert_eq!(standard.to_decimal().scale(), 6);
        assert_eq!(standard.to_percent_decimal().scale(), 4);

        assert_eq!(Percent::from_ppm(5_000).to_decimal(), Decimal::new(5, 3));
        assert_eq!(
            Percent::from_ppm(5_000).to_percent_decimal(),
            Decimal::new(5, 1)
        );
        // One ppm: the smallest representable rate, in both forms.
        assert_eq!(Percent::from_ppm(1).to_decimal(), Decimal::new(1, 6));
        assert_eq!(
            Percent::from_ppm(1).to_percent_decimal(),
            Decimal::new(1, 4)
        );
        assert_eq!(Percent::ZERO.to_decimal(), Decimal::ZERO);
    }

    #[test]
    fn percent_format_trims_trailing_zeros_without_rounding() {
        // 160_000 ppm carries four trailing zeros the rate does not have, so a
        // literal render says "16.0000%". Trimming them is presentation;
        // rounding them away would be a different rate.
        assert_eq!(Percent::ZERO.format(), "0%");
        assert_eq!(Percent::from_ppm(1_000_000).format(), "100%");
        assert_eq!(Percent::from_ppm(160_000).format(), "16%");
        assert_eq!(Percent::from_ppm(5_000).format(), "0.5%");
        assert_eq!(Percent::from_ppm(1_250).format(), "0.125%");
        // The smallest representable rate keeps all four places. Showing it as
        // "0%" would deny a charge that is being made.
        assert_eq!(Percent::from_ppm(1).format(), "0.0001%");
        assert_eq!(Percent::from_ppm(10).format(), "0.001%");
        // Negative rates keep their sign, and both extremes stay exact.
        assert_eq!(Percent::from_ppm(-50_000).format(), "-5%");
        assert_eq!(Percent::from_ppm(-1).format(), "-0.0001%");
        assert_eq!(
            Percent::from_ppm(i64::MAX).format(),
            "922337203685477.5807%"
        );
        assert_eq!(
            Percent::from_ppm(i64::MIN).format(),
            "-922337203685477.5808%"
        );
    }

    #[test]
    fn a_rate_finer_than_one_ppm_is_refused_not_rounded() {
        // 0.00001% is a tenth of a ppm. This constructor takes no
        // `RoundingRule`, and I-1 puts rounding only where a rule was passed
        // in, so the only honest answers are the exact rate or a named error.
        assert_eq!(
            Percent::from_percent_decimal(Decimal::new(1, 5)),
            Err(MoneyError::NotRepresentableAtPrecision(
                "0.00001".to_owned(),
                4
            ))
        );
        // A long fraction is refused whole rather than rounded to the nearest
        // ppm: 0.166667% would otherwise silently become 0.1667%.
        assert_eq!(
            Percent::from_percent_decimal(Decimal::new(166_667, 6)),
            Err(MoneyError::NotRepresentableAtPrecision(
                "0.166667".to_owned(),
                4
            ))
        );

        // One ppm exactly is fine, on both sides of zero.
        assert_eq!(
            Percent::from_percent_decimal(Decimal::new(1, 4)),
            Ok(Percent::from_ppm(1))
        );
        assert_eq!(
            Percent::from_percent_decimal(Decimal::new(-1, 4)),
            Ok(Percent::from_ppm(-1))
        );

        // Exactness is the test, not scale. Trailing zeros are not precision,
        // and a `scale() <= 4` implementation refuses both of these — which is
        // exactly what a JSON number or a SQL decimal arrives looking like.
        assert_eq!(
            Percent::from_percent_decimal(Decimal::new(16_000_000, 6)),
            Ok(Percent::from_ppm(160_000)),
            "16.000000% is 16%"
        );
        assert_eq!(
            Percent::from_percent_decimal(Decimal::new(5_000, 4)),
            Ok(Percent::from_ppm(5_000)),
            "0.5000% is 0.5%"
        );
    }

    #[test]
    fn a_rate_beyond_i64_ppm_is_out_of_range() {
        // The last rate that fits and the first that does not. A saturating
        // cast here would answer with a rate nobody asked for, and a panic
        // would take the register down over a settings row.
        let one_ppm = Decimal::new(1, 4);
        let max_percentage = Decimal::new(i64::MAX, 4);
        let min_percentage = Decimal::new(i64::MIN, 4);

        assert_eq!(
            Percent::from_percent_decimal(max_percentage),
            Ok(Percent::from_ppm(i64::MAX))
        );
        assert_eq!(
            Percent::from_percent_decimal(min_percentage),
            Ok(Percent::from_ppm(i64::MIN))
        );
        assert_eq!(
            Percent::from_percent_decimal(max_percentage + one_ppm),
            Err(MoneyError::OutOfRange)
        );
        assert_eq!(
            Percent::from_percent_decimal(min_percentage - one_ppm),
            Err(MoneyError::OutOfRange)
        );

        // Decimal reaches ~7.9e28, so scaling either extreme to ppm overflows
        // the representation itself. `checked_mul` is what makes that the same
        // handled error rather than the panic `Decimal * Decimal` raises.
        assert_eq!(
            Percent::from_percent_decimal(Decimal::MAX),
            Err(MoneyError::OutOfRange)
        );
        assert_eq!(
            Percent::from_percent_decimal(Decimal::MIN),
            Err(MoneyError::OutOfRange)
        );

        // Both wrong at once: past i64 ppm AND finer than a ppm. The exactness
        // test runs first, so this is the precision error. Documented rather
        // than incidental — both answers are a refusal, which is the part a
        // caller may rely on.
        assert_eq!(
            Percent::from_percent_decimal(max_percentage + Decimal::new(1, 5)),
            Err(MoneyError::NotRepresentableAtPrecision(
                "922337203685477.58071".to_owned(),
                4
            ))
        );
    }

    #[test]
    fn a_negative_rate_is_representable_and_keeps_its_sign() {
        // `Percent` is a signed carrier, like `Qty`. `from_ppm` is const and
        // infallible by specification, so refusing a negative here would be a
        // comment rather than a rule; the sign restriction on a TAX rate lives
        // where one is built and stored — CHECK (rate_ppm >= 0) in
        // ref/schema.md, and group 1.3's rate resolution.
        let adjustment = Percent::from_ppm(-50_000);
        assert_eq!(adjustment.to_decimal(), Decimal::new(-5, 2));
        assert_eq!(adjustment.to_percent_decimal(), Decimal::from(-5));
        assert_eq!(
            Percent::from_percent_decimal(Decimal::from(-5)),
            Ok(adjustment)
        );
        assert_eq!(adjustment.format(), "-5%");
    }

    #[test]
    fn percent_order_follows_signed_ppm() {
        let mut rates = [
            Percent::from_ppm(160_000),
            Percent::from_ppm(-1),
            Percent::ZERO,
            Percent::from_ppm(-50_000),
            Percent::from_ppm(1),
        ];
        rates.sort();

        assert_eq!(
            rates,
            [
                Percent::from_ppm(-50_000),
                Percent::from_ppm(-1),
                Percent::ZERO,
                Percent::from_ppm(1),
                Percent::from_ppm(160_000),
            ]
        );
        assert!(Percent::from_ppm(-1) < Percent::ZERO);
        assert!(Percent::from_ppm(40_000) < Percent::from_ppm(160_000));
    }

    #[test]
    fn percent_serialises_as_ppm() {
        // ppm is the wire form, exactly as milli-units are for `Qty`. A rate
        // that travelled as 0.16 or as "16%" would need a parser at the other
        // end, and the two ends would eventually disagree about which one it is.
        let standard = Percent::from_ppm(160_000);
        assert_eq!(serde_json::to_string(&standard).unwrap(), "160000");
        assert_eq!(serde_json::from_str::<Percent>("160000").unwrap(), standard);
        assert_eq!(
            serde_json::from_str::<Percent>("-5000").unwrap(),
            Percent::from_ppm(-5_000)
        );
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

        /// Value-weighted proration never creates or destroys a minor unit,
        /// never changes currency, and gives every zero-valued weight zero.
        #[test]
        fn prop_split_proportional_preserves_total(
            (minor, weights, currency) in proportional_money_cases()
        ) {
            let amount = Money::from_minor(minor, currency);
            let raw_weights = weights.iter().map(|weight| weight.minor()).collect::<Vec<_>>();
            let pieces = amount.split_proportional(&weights).unwrap();

            prop_assert_eq!(pieces.len(), weights.len());
            prop_assert!(pieces.iter().all(|piece| piece.currency() == currency));
            prop_assert_eq!(
                pieces.iter().map(|piece| i128::from(piece.minor())).sum::<i128>(),
                i128::from(minor)
            );
            let zero_weights_are_zero = pieces.iter().zip(&weights).all(|(piece, weight)| {
                weight.minor() != 0 || piece.is_zero()
            });
            prop_assert!(zero_weights_are_zero);
            prop_assert_eq!(
                pieces,
                amount.split_proportional_by(&raw_weights).unwrap(),
                "the Money entry point must delegate to the integer algorithm"
            );
        }

        /// Integer-weighted proration conserves the signed total exactly over
        /// every generated weight vector, and a zero weight always gets zero.
        #[test]
        fn prop_split_proportional_by_preserves_total(
            (minor, weights, currency) in proportional_integer_cases()
        ) {
            let pieces = Money::from_minor(minor, currency)
                .split_proportional_by(&weights)
                .unwrap();

            prop_assert_eq!(pieces.len(), weights.len());
            prop_assert!(pieces.iter().all(|piece| piece.currency() == currency));
            prop_assert_eq!(
                pieces.iter().map(|piece| i128::from(piece.minor())).sum::<i128>(),
                i128::from(minor)
            );
            let zero_weights_are_zero = pieces.iter().zip(&weights).all(|(piece, weight)| {
                *weight != 0 || piece.is_zero()
            });
            prop_assert!(zero_weights_are_zero);
        }

        /// A whole-unit quantity costs exactly the same as adding that unit
        /// price once per unit, under every rounding rule.
        #[test]
        fn prop_mul_qty_whole_units_is_repeated_add(
            (minor, units, currency, rule) in mul_qty_whole_unit_cases()
        ) {
            let price = Money::from_minor(minor, currency);
            let repeated = (0..units)
                .try_fold(Money::zero(currency), |sum, _| sum.checked_add(price))
                .unwrap();
            let qty = Qty::from_units(i64::from(units)).unwrap();

            prop_assert_eq!(price.mul_qty(qty, rule), Ok(repeated));
        }

        /// Rounding to a step is idempotent: once an amount is a multiple of
        /// the step, applying the same direction cannot move it again.
        #[test]
        fn prop_round_to_step_is_idempotent(
            (minor, step, direction) in round_to_step_cases()
        ) {
            let amount = Money::from_minor(minor, Currency::JOD);
            let rounded = amount.round_to_step(step, direction).unwrap();
            prop_assert_eq!(rounded.round_to_step(step, direction), Ok(rounded));
        }

        /// Nearest step rounding never moves an amount by more than half of
        /// one step, with exact half-step ties included on both sides of zero.
        #[test]
        fn prop_round_to_step_within_half_step((minor, step) in nearest_step_cases()) {
            let amount = Money::from_minor(minor, Currency::JOD);
            let rounded = amount
                .round_to_step(step, RoundingDirection::Nearest)
                .unwrap();
            let distance = (i128::from(rounded.minor()) - i128::from(minor)).abs();

            prop_assert!(
                distance * 2 <= i128::from(step),
                "nearest moved {distance} minor units for step {step}"
            );
        }

        /// Exact rendering is a lossless representation of every stored amount
        /// at every known currency exponent, including both i64 boundaries.
        #[test]
        fn prop_format_exact_parse_roundtrip(
            (minor, currency) in money_exact_roundtrip_cases()
        ) {
            let amount = Money::from_minor(minor, currency);
            prop_assert_eq!(Money::parse(&amount.format_exact(), currency), Ok(amount));
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

        /// A rate survives the trip through its decimal percentage form, from
        /// either end: every representable ppm renders to a percentage that
        /// reads back as the same ppm, and every percentage a `Percent` can
        /// hold reads in and renders back to the same number.
        #[test]
        fn prop_percent_decimal_roundtrip((ppm, percentage) in percent_decimal_roundtrip_cases()) {
            // Percent → percentage → Percent, over every rate the type holds.
            let rate = Percent::from_ppm(ppm);
            prop_assert_eq!(
                Percent::from_percent_decimal(rate.to_percent_decimal()),
                Ok(rate)
            );

            // And back the other way, from a percentage a decree or a settings
            // row could carry. Decimal equality is numeric rather than textual:
            // `to_percent_decimal` always answers at scale 4, so 16 returns as
            // 16.0000 and the two are the same number.
            prop_assert_eq!(
                Percent::from_percent_decimal(percentage).map(Percent::to_percent_decimal),
                Ok(percentage)
            );
        }

        /// The fraction and the percentage are one rate a hundred apart, at
        /// every representable value — so arithmetic that multiplies by
        /// `to_decimal` and a receipt that prints `to_percent_decimal` can never
        /// disagree by two orders of magnitude.
        #[test]
        fn prop_percent_fraction_is_the_percentage_over_one_hundred(
            ppm in every_representable_rate()
        ) {
            let rate = Percent::from_ppm(ppm);
            prop_assert_eq!(
                rate.to_decimal() * Decimal::ONE_HUNDRED,
                rate.to_percent_decimal()
            );
        }

        /// Rendering a rate loses no digits: the string, read back as a decimal,
        /// is the exact percentage it came from. Trailing zeros are the only
        /// thing `format` is allowed to remove, because a rate display that
        /// rounds is a rate display that lies.
        #[test]
        fn prop_percent_format_loses_no_digits(ppm in every_representable_rate()) {
            let rate = Percent::from_ppm(ppm);
            let rendered = rate.format();

            let digits = rendered.strip_suffix('%');
            prop_assert!(digits.is_some(), "a rendered rate ends in a per-cent sign");
            let digits = digits.unwrap();
            prop_assert!(
                !digits.ends_with('0') || !digits.contains('.'),
                "a fractional render must not keep a trailing zero: {}",
                rendered
            );
            prop_assert_eq!(
                Decimal::from_str(digits).ok(),
                Some(rate.to_percent_decimal())
            );
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
