//! Manual discounts (microsteps 1.4.5–1.4.6, `ref/domain-api.md` §6.1–§6.2):
//! what a cashier asks for, the cap that bounds it, what a discount leaves on a
//! line or a basket, and how a basket's discount is divided among its lines.
//!
//! A discount is kept as it was asked for — a share or an amount — and turned
//! into money where every other number is, when the cart is priced (1.4.9),
//! so a percentage keeps meaning the same share when the quantity changes.
//! The checks made when it is applied are exact: [`exceeds_value`] and
//! [`DiscountCap::admits`] compare against a line's unrounded value, so they
//! give the same answer under every rule, and their basket forms compare
//! against the basket's value as priced.
//!
//! This module sits below `cart` (§15): it knows nothing of a cart, and
//! `cart::apply_line_discount` and `cart::apply_basket_discount` are the
//! transitions that use it.

use std::cmp::Reverse;

use serde::{Deserialize, Serialize};

use crate::ids::{ProductId, SaleLineId, TaxCategoryId, UserId};
use crate::money::{Money, MoneyError, Percent, Qty, RoundingRule};

/// One hundred percent, in ppm: the most any discount can take.
const WHOLE: i64 = 1_000_000;

/// Milli-units in one unit of quantity (I-3).
const MILLI_PER_UNIT: i128 = 1_000;

/// How a manual discount is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscountKind {
    /// A share of the line's value, in ppm: 5% is `50_000`.
    Percent(Percent),
    /// An exact amount off, in the cart's currency.
    Amount(Money),
}

impl DiscountKind {
    /// Whether this asks for nothing, which clears a line's discount.
    pub const fn is_zero(self) -> bool {
        match self {
            DiscountKind::Percent(p) => p.ppm() == 0,
            DiscountKind::Amount(a) => a.is_zero(),
        }
    }

    /// Whether this asks for less than nothing: a surcharge dressed as a
    /// discount, which is a price change and never this path's to make.
    pub const fn is_negative(self) -> bool {
        match self {
            DiscountKind::Percent(p) => p.ppm() < 0,
            DiscountKind::Amount(a) => a.is_negative(),
        }
    }

    /// `"5%"` or `"0.250 JOD"`, for a message a person reads.
    pub fn format(self) -> String {
        match self {
            DiscountKind::Percent(p) => p.format(),
            DiscountKind::Amount(a) => format!("{} {}", a.format_exact(), a.currency().code()),
        }
    }
}

/// The configured reason for a discount: a code from the store's list, never
/// free text, because free text is unreportable (`ref/ui-spec.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscountReason(pub String);

/// What the cashier asks for: `cart_discount_line`'s `{ kind, value, reason }`
/// (1.4.11). The amount in it is typed by a person, which is why the
/// transition that takes it is capped, approved above the cap, and audited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscountRequest {
    pub kind: DiscountKind,
    pub reason: DiscountReason,
}

/// The most a holder of `discount.manual` may give on their own: their role's
/// cap (merchant decisions 3.1–3.3), or none. Above it, a manager approves.
///
/// The shell resolves it from the actor's effective grant, never from the
/// request: a cap the cashier could send would bound nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscountCap {
    /// The grant is held without a limit.
    Unlimited,
    /// The grant is held within a role cap of this share.
    AtMost(Percent),
}

impl DiscountCap {
    /// Whether `kind`, off a line of `qty` at `unit_price`, is within this cap.
    ///
    /// A percentage is compared as a percentage. An amount is compared with
    /// the cap's share of the line's exact, unrounded value, in integers, so
    /// `0.050` off `1.000` is within 5% and `0.051` is not, whatever rule
    /// later rounds the line.
    pub fn admits(self, kind: DiscountKind, unit_price: Money, qty: Qty) -> bool {
        self.admits_exact(
            kind,
            i128::from(unit_price.minor()) * i128::from(qty.milli()),
        )
    }

    /// Whether `kind`, off a basket worth `value` once each line's own
    /// discount is taken off, is within this cap (1.4.6). Each discount is held
    /// to the cap on its own, so a line's discount and the basket's can both
    /// apply (the owner's decision, 29 October 2026).
    pub fn admits_basket(self, kind: DiscountKind, value: Money) -> bool {
        self.admits_exact(kind, i128::from(value.minor()) * MILLI_PER_UNIT)
    }

    /// The cap against a value in thousandths of a minor unit, which is what a
    /// unit price times a milli-quantity is, exactly.
    fn admits_exact(self, kind: DiscountKind, value: i128) -> bool {
        let DiscountCap::AtMost(cap) = self else {
            return true;
        };
        match kind {
            DiscountKind::Percent(p) => p.ppm() <= cap.ppm(),
            DiscountKind::Amount(a) => {
                // a / value <= cap / WHOLE, with value in thousandths:
                // a × WHOLE × 1000 <= cap × value.
                let asked = i128::from(a.minor()) * i128::from(WHOLE) * MILLI_PER_UNIT;
                match value.checked_mul(i128::from(cap.ppm())) {
                    Some(allowed) => asked <= allowed,
                    // Out of i128's range, the allowance is either far above
                    // any i64 amount or far below it: its sign decides.
                    None => (value > 0) == (cap.ppm() > 0),
                }
            }
        }
    }

    /// The cap, for an audit payload: its ppm, or `None` when unlimited.
    pub const fn ppm(self) -> Option<i64> {
        match self {
            DiscountCap::Unlimited => None,
            DiscountCap::AtMost(cap) => Some(cap.ppm()),
        }
    }

    /// `"5%"` or `"no cap"`, for a message a person reads.
    pub fn format(self) -> String {
        match self {
            DiscountCap::Unlimited => "no cap".to_owned(),
            DiscountCap::AtMost(cap) => cap.format(),
        }
    }
}

/// Whether `kind` would take more than a line of `qty` at `unit_price` is
/// worth, which would make the line negative (E.19).
///
/// Exact, like [`DiscountCap::admits`]: an amount is compared with the line's
/// unrounded value, and a whole amount no greater than that is no greater than
/// the value rounded by any rule, so the answer holds once the line is priced.
/// A share above 100% exceeds every line, and no share exceeds a line worth
/// nothing.
pub fn exceeds_value(kind: DiscountKind, unit_price: Money, qty: Qty) -> bool {
    exceeds_exact(
        kind,
        i128::from(unit_price.minor()) * i128::from(qty.milli()),
    )
}

/// Whether `kind` would take more than a basket worth `value` (1.4.6).
pub fn exceeds_basket(kind: DiscountKind, value: Money) -> bool {
    exceeds_exact(kind, i128::from(value.minor()) * MILLI_PER_UNIT)
}

/// The value check against thousandths of a minor unit.
fn exceeds_exact(kind: DiscountKind, value: i128) -> bool {
    match kind {
        DiscountKind::Percent(p) => p.ppm() > WHOLE,
        DiscountKind::Amount(a) => i128::from(a.minor()) * MILLI_PER_UNIT > value,
    }
}

/// The money `kind` takes off `value`: a share rounded once by `rule`, or the
/// amount itself.
fn amount_off(kind: DiscountKind, value: Money, rule: RoundingRule) -> Result<Money, MoneyError> {
    match kind {
        DiscountKind::Percent(p) => value.mul_percent(p, rule),
        DiscountKind::Amount(a) if a.currency() == value.currency() => Ok(a),
        DiscountKind::Amount(a) => Err(MoneyError::CurrencyMismatch(
            value.currency().code(),
            a.currency().code(),
        )),
    }
}

/// A manual discount on one line, kept as it was asked for, with who gave it
/// and who approved it above their cap.
///
/// A line carries at most one: a second replaces the first, so two discounts
/// each within the cap cannot add up past it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineDiscount {
    pub kind: DiscountKind,
    pub reason: DiscountReason,
    pub by: UserId,
    pub approver: Option<UserId>,
}

impl LineDiscount {
    /// The money this discount takes off a line worth `gross`, the line's
    /// value already priced under `rule`: a share rounded once by `rule`, or
    /// the amount itself.
    ///
    /// # Errors
    ///
    /// [`MoneyError::CurrencyMismatch`] for an amount in another currency than
    /// the line's, and [`MoneyError::Overflow`] from the multiplication.
    pub fn amount_off(&self, gross: Money, rule: RoundingRule) -> Result<Money, MoneyError> {
        amount_off(self.kind, gross, rule)
    }
}

/// A manual discount on the whole basket, kept as it was asked for (1.4.6).
///
/// A cart carries at most one, which another replaces and a zero clears. A
/// share keeps its share as the basket changes. An amount off is cleared by
/// any change that lowers the basket's value, so it never takes more than the
/// basket is worth, nor a larger share than the cap admitted (the owner's
/// decision, 29 October 2026).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasketDiscount {
    pub kind: DiscountKind,
    pub reason: DiscountReason,
    pub by: UserId,
    pub approver: Option<UserId>,
}

impl BasketDiscount {
    /// The money this discount takes off a basket worth `value`, priced under
    /// `rule`, as [`LineDiscount::amount_off`] does for a line.
    ///
    /// # Errors
    ///
    /// As [`LineDiscount::amount_off`].
    pub fn amount_off(&self, value: Money, rule: RoundingRule) -> Result<Money, MoneyError> {
        amount_off(self.kind, value, rule)
    }
}

/// Where a line's share of a discount came from: `sale_line_discount.source`
/// (`ref/schema.md`). Promotions and loyalty join when they are first captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscountSource {
    ManualLine,
    ManualBasket,
}

/// One line's share of one discount, in money (§6.2).
///
/// Campaign-cost reporting and the fiscal document's line allowance both read
/// it, and a basket discount that has not been divided into these cannot
/// become a fiscal document at all. The percentage a discount was asked as
/// travels as provenance only: it is never turned back into money, and never
/// decides whether a correct basket may be sold (correction C-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscountAttribution {
    pub line_id: SaleLineId,
    pub source: DiscountSource,
    pub amount: Money,
    pub percent: Option<Percent>,
    pub reason: DiscountReason,
    pub by: UserId,
    pub approver: Option<UserId>,
}

/// A line as proration sees it: what it is worth and what it is (1.4.6).
///
/// Its id rides along so a caller can pair each share with its line, and is
/// never read: two cashiers who scan the same items in another order, and so
/// mint other ids in another order, are charged the same (§6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProrationLine {
    pub line_id: SaleLineId,
    /// The line's value once its own discount is taken off: its share's weight.
    pub value: Money,
    pub tax_category_id: TaxCategoryId,
    pub product_id: Option<ProductId>,
    pub unit_price: Money,
    pub qty: Qty,
}

/// What a line is, for proration's tie-break: its tax category, product, unit
/// price in minor units and quantity in milli-units.
type Content = (TaxCategoryId, Option<ProductId>, i64, i64);

impl ProrationLine {
    /// The tie-break: what the line is, never where it sits or its id.
    fn content(&self) -> Content {
        (
            self.tax_category_id,
            self.product_id,
            self.unit_price.minor(),
            self.qty.milli(),
        )
    }
}

/// Why a discount could not be divided among the lines.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PricingError {
    /// The discount and the lines' total value, rendered: more than the lines
    /// are worth would leave one of them negative (E.19).
    #[error("{0} is more than the lines are worth together, {1}")]
    ExceedsTotal(String, String),
    #[error(transparent)]
    Money(#[from] MoneyError),
}

/// `amount` divided among `lines` in proportion to their values, to the fil:
/// the shares come back in `lines`' order and sum to `amount` exactly.
///
/// Largest remainder: each line takes its exact share rounded down, and the
/// fils left over go one each to the lines whose exact shares were cut the
/// most. Where two lines were cut equally, the line that sorts first by
/// `(tax_category_id, product_id, unit_price_minor, qty_milli)`, then by its
/// value, takes it — never the one scanned first, and never the one whose id
/// sorts first. On a basket of several tax rates a fil of discount moves tax,
/// so a tie broken by position would charge two cashiers differently for the
/// same items (§6.2). The value is in the key because two lines of one product
/// at one price and quantity can be worth different amounts, their own
/// discounts differing, and still be cut equally. Only lines alike in all of
/// it, which are interchangeable, keep the order given.
///
/// A line worth nothing takes nothing. [`Money::split_proportional_by`] gives
/// its leftover fils to the first lines it is handed, so it is handed the
/// lines in this order.
///
/// # Errors
///
/// - [`PricingError::ExceedsTotal`]: more than the lines are worth together.
/// - [`PricingError::Money`]: a negative amount ([`MoneyError::Negative`]), a
///   line worth less than nothing ([`MoneyError::NegativeWeight`]), or a value
///   in another currency than the amount's.
pub fn prorate(amount: Money, lines: &[ProrationLine]) -> Result<Vec<Money>, PricingError> {
    let currency = amount.currency();
    if amount.is_negative() {
        return Err(MoneyError::Negative.into());
    }
    let mut total: i128 = 0;
    for line in lines {
        if line.value.currency() != currency {
            return Err(MoneyError::CurrencyMismatch(
                currency.code(),
                line.value.currency().code(),
            )
            .into());
        }
        if line.value.is_negative() {
            return Err(MoneyError::NegativeWeight.into());
        }
        total += i128::from(line.value.minor());
    }
    if i128::from(amount.minor()) > total {
        let worth = i64::try_from(total).map_or_else(
            |_| total.to_string(),
            |t| Money::from_minor(t, currency).format_exact(),
        );
        return Err(PricingError::ExceedsTotal(amount.format_exact(), worth));
    }
    if amount.is_zero() {
        return Ok(vec![Money::zero(currency); lines.len()]);
    }
    // Every value is non-negative and the amount is positive, so these are
    // magnitudes; amount × value fits u128, and total is at least amount.
    let (magnitude, total) = (
        u128::from(amount.minor().unsigned_abs()),
        total.unsigned_abs(),
    );
    let mut ranked: Vec<_> = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let weight = line.value.minor();
            let cut = magnitude * u128::from(weight.unsigned_abs()) % total;
            (Reverse(cut), line.content(), weight, index)
        })
        .collect();
    ranked.sort_unstable();
    let weights: Vec<i64> = ranked.iter().map(|(_, _, weight, _)| *weight).collect();
    let shares = amount.split_proportional_by(&weights)?;
    let mut placed: Vec<(usize, Money)> = ranked
        .into_iter()
        .zip(shares)
        .map(|((.., index), share)| (index, share))
        .collect();
    placed.sort_unstable_by_key(|(index, _)| *index);
    Ok(placed.into_iter().map(|(_, share)| share).collect())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::money::Currency;
    use uuid::Uuid;

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, Currency::JOD)
    }

    /// 0.333 kg at 1.999 a kilogram: exactly 0.665667, which no rule leaves
    /// unrounded, so the checks are seen to read the exact value.
    fn weighed() -> (Money, Qty) {
        (jod(1_999), Qty::from_milli(333))
    }

    #[test]
    fn the_cap_and_the_line_s_value_are_exact_at_their_boundaries() {
        let (price, qty) = weighed();
        let five = DiscountCap::AtMost(Percent::from_ppm(50_000));
        // 5% of 665.667 fils is 33.28 fils: 33 is within it, 34 is not.
        assert!(five.admits(DiscountKind::Amount(jod(33)), price, qty));
        assert!(!five.admits(DiscountKind::Amount(jod(34)), price, qty));
        // A share is compared as a share, to the ppm.
        assert!(five.admits(DiscountKind::Percent(Percent::from_ppm(50_000)), price, qty));
        assert!(!five.admits(DiscountKind::Percent(Percent::from_ppm(50_001)), price, qty));
        assert!(DiscountCap::Unlimited.admits(DiscountKind::Amount(jod(665)), price, qty));
        // 665 fils is within the line's exact value and 666 is not, although
        // the line rounds half away from zero to 0.666: an amount no greater
        // than the exact value is no greater than the value under any rule,
        // and 100% is how the whole of it is given.
        assert!(!exceeds_value(DiscountKind::Amount(jod(665)), price, qty));
        assert!(exceeds_value(DiscountKind::Amount(jod(666)), price, qty));
        assert!(!exceeds_value(
            DiscountKind::Percent(Percent::from_ppm(1_000_000)),
            price,
            qty
        ));
        assert!(exceeds_value(
            DiscountKind::Percent(Percent::from_ppm(1_000_001)),
            price,
            qty
        ));
    }

    #[test]
    fn a_cap_holds_at_the_ends_of_the_amount_range() {
        // A line worth more than i128 can hold times the cap: its sign decides.
        let (price, qty) = (jod(i64::MAX), Qty::from_milli(i64::MAX));
        let asked = DiscountKind::Amount(jod(i64::MAX));
        assert!(DiscountCap::AtMost(Percent::from_ppm(1_000)).admits(asked, price, qty));
        assert!(!DiscountCap::AtMost(Percent::from_ppm(-1_000)).admits(asked, price, qty));
    }

    #[test]
    fn a_share_off_is_rounded_once_by_the_store_s_rule() {
        let discount = LineDiscount {
            kind: DiscountKind::Percent(Percent::from_ppm(50_000)),
            reason: DiscountReason("loyal_customer".to_owned()),
            by: UserId::from_uuid(Uuid::from_u128(4)),
            approver: None,
        };
        // 5% of 0.666 is 33.3 fils.
        assert_eq!(
            discount.amount_off(jod(666), RoundingRule::HalfAwayFromZero),
            Ok(jod(33))
        );
        assert_eq!(
            discount.amount_off(jod(666), RoundingRule::Ceil),
            Ok(jod(34))
        );
        let amount = LineDiscount {
            kind: DiscountKind::Amount(Money::from_minor(10, Currency::USD)),
            ..discount
        };
        assert_eq!(
            amount.amount_off(jod(666), RoundingRule::Floor),
            Err(MoneyError::CurrencyMismatch("JOD", "USD"))
        );
    }

    #[test]
    fn a_line_discount_s_wire_form_is_pinned() {
        let discount = LineDiscount {
            kind: DiscountKind::Amount(jod(125)),
            reason: DiscountReason("loyal_customer".to_owned()),
            by: UserId::from_uuid(Uuid::from_u128(4)),
            approver: Some(UserId::from_uuid(Uuid::from_u128(5))),
        };
        let json = serde_json::to_string(&discount).unwrap();
        assert_eq!(
            json,
            r#"{"kind":{"amount":{"minor":125,"currency":"JOD"}},"reason":"loyal_customer","by":"00000000-0000-0000-0000-000000000004","approver":"00000000-0000-0000-0000-000000000005"}"#
        );
        assert_eq!(
            serde_json::from_str::<LineDiscount>(&json).unwrap(),
            discount
        );
        let share = serde_json::to_string(&DiscountKind::Percent(Percent::from_ppm(50_000)));
        assert_eq!(share.unwrap(), r#"{"percent":50000}"#);
    }

    // ── basket proration (1.4.6) ────────────────────────────────────────

    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;
    use proptest::sample::Index;

    fn line(n: u128, tax: u128, value: i64) -> ProrationLine {
        ProrationLine {
            line_id: SaleLineId::from_uuid(Uuid::from_u128(0x1000 + n)),
            value: jod(value),
            tax_category_id: TaxCategoryId::from_uuid(Uuid::from_u128(tax)),
            product_id: Some(ProductId::from_uuid(Uuid::from_u128(0x20 + tax))),
            unit_price: jod(value),
            qty: Qty::ONE,
        }
    }

    fn minors(shares: &[Money]) -> Vec<i64> {
        shares.iter().map(|m| m.minor()).collect()
    }

    #[test]
    fn a_tie_goes_by_what_the_line_is_not_where_it_sits() {
        // 0.100 over three lines worth the same: 33.33 fils each, so one fil is
        // left, and all three were cut equally. It goes to the line whose tax
        // category sorts first, wherever it was scanned and whatever its id.
        let (a, b, c) = (line(1, 3, 500), line(2, 1, 500), line(3, 2, 500));
        assert_eq!(
            minors(&prorate(jod(100), &[a, b, c]).unwrap()),
            vec![33, 34, 33]
        );
        assert_eq!(
            minors(&prorate(jod(100), &[b, c, a]).unwrap()),
            vec![34, 33, 33]
        );
        // Where the cuts differ, the larger one takes the fil, whatever sorts
        // first: 0.100 over 0.300 and 0.600 is 33.3 and 66.6.
        let (small, large) = (line(1, 1, 300), line(2, 2, 600));
        assert_eq!(
            minors(&prorate(jod(100), &[small, large]).unwrap()),
            vec![33, 67]
        );
        // Two lines of one product at one price, worth the same — two teas with
        // 1.250 off beside one tea — differ only in quantity, and that decides:
        // the smaller quantity sorts first and takes the fil, in either order.
        let two = ProrationLine {
            qty: Qty::from_milli(2_000),
            ..line(1, 1, 1_250)
        };
        let one = ProrationLine {
            qty: Qty::from_milli(1_000),
            ..line(2, 1, 1_250)
        };
        assert_eq!(minors(&prorate(jod(1), &[two, one]).unwrap()), vec![0, 1]);
        assert_eq!(minors(&prorate(jod(1), &[one, two]).unwrap()), vec![1, 0]);
    }

    #[test]
    fn proration_refuses_what_it_cannot_divide() {
        let lines = [line(1, 1, 500), line(2, 2, 700)];
        assert_eq!(
            prorate(jod(1_201), &lines),
            Err(PricingError::ExceedsTotal(
                "1.201".to_owned(),
                "1.200".to_owned()
            ))
        );
        assert_eq!(
            minors(&prorate(jod(1_200), &lines).unwrap()),
            vec![500, 700]
        );
        assert_eq!(prorate(jod(-1), &lines), Err(MoneyError::Negative.into()));
        assert_eq!(
            prorate(jod(1), &[line(1, 1, -1)]),
            Err(MoneyError::NegativeWeight.into())
        );
        let dollars = ProrationLine {
            value: Money::from_minor(500, Currency::USD),
            ..line(1, 1, 500)
        };
        assert_eq!(
            prorate(jod(1), &[dollars]),
            Err(MoneyError::CurrencyMismatch("JOD", "USD").into())
        );
        // Nothing divides into nothing, even among lines worth nothing.
        assert_eq!(minors(&prorate(jod(0), &[line(1, 1, 0)]).unwrap()), vec![0]);
        assert_eq!(minors(&prorate(jod(0), &[]).unwrap()), Vec::<i64>::new());
    }

    /// Up to four kinds of line and up to four values, and a basket of up to
    /// twelve lines each drawn from both, so baskets repeat what they hold,
    /// hold lines alike but for what they are worth, and cut equally often.
    fn baskets() -> impl Strategy<Value = Vec<ProrationLine>> {
        let kind = (
            1u128..=3,
            proptest::option::of(1u128..=3),
            0i64..=5_000,
            1i64..=5_000,
        );
        (
            proptest::collection::vec(kind, 1..=4),
            // Mostly a few fils, where unequal values are cut equally often;
            // sometimes thousands of dinars.
            proptest::collection::vec(prop_oneof![3 => 0i64..=12, 1 => 0i64..=10_000_000], 1..=4),
            proptest::collection::vec((any::<Index>(), any::<Index>()), 1..=12),
        )
            .prop_map(|(kinds, values, picks)| {
                picks
                    .iter()
                    .enumerate()
                    .map(|(n, (what, worth))| {
                        let (tax, product, price, milli) = *what.get(&kinds);
                        let value = *worth.get(&values);
                        ProrationLine {
                            line_id: SaleLineId::from_uuid(Uuid::from_u128(0x1000 + n as u128)),
                            value: jod(value),
                            tax_category_id: TaxCategoryId::from_uuid(Uuid::from_u128(tax)),
                            product_id: product
                                .map(|p| ProductId::from_uuid(Uuid::from_u128(0x20 + p))),
                            unit_price: jod(price),
                            qty: Qty::from_milli(milli),
                        }
                    })
                    .collect()
            })
    }

    /// A basket and any discount it can carry.
    fn discounted_baskets() -> impl Strategy<Value = (Vec<ProrationLine>, i64)> {
        baskets().prop_flat_map(|lines| {
            let total: i64 = lines.iter().map(|l| l.value.minor()).sum();
            (Just(lines), 0..=total)
        })
    }

    /// Each line's content and value beside its share, sorted: what a basket
    /// was charged, whatever order it was scanned in.
    fn charged(lines: &[ProrationLine], shares: &[Money]) -> Vec<(Content, i64, i64)> {
        let mut rows: Vec<_> = lines
            .iter()
            .zip(shares)
            .map(|(l, s)| (l.content(), l.value.minor(), s.minor()))
            .collect();
        rows.sort_unstable();
        rows
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// A basket discount divided among the lines creates and destroys no
        /// fil: the shares, and the recap that repeats their sum, are exactly
        /// the discount. Each line takes its exact share rounded down or up,
        /// never more than it is worth, and the fils left over go to the lines
        /// cut the most — between two cut equally, to the one that sorts first
        /// by what it is and then by what it is worth.
        #[test]
        fn prop_basket_discount_prorates_to_the_fil((lines, amount) in discounted_baskets()) {
            let shares = prorate(jod(amount), &lines).unwrap();
            prop_assert_eq!(shares.len(), lines.len());
            let recap = Money::sum(shares.iter().copied(), Currency::JOD).unwrap();
            prop_assert_eq!(recap, jod(amount));
            let total: i128 = lines.iter().map(|l| i128::from(l.value.minor())).sum();
            let cut = |l: &ProrationLine| i128::from(amount) * i128::from(l.value.minor()) % total.max(1);
            for (l, s) in lines.iter().zip(&shares) {
                let floor = i128::from(amount) * i128::from(l.value.minor()) / total.max(1);
                let share = i128::from(s.minor());
                prop_assert!(share == floor || share == floor + 1, "{share} from {floor}");
                prop_assert!(s.minor() <= l.value.minor());
            }
            for (i, (li, si)) in lines.iter().zip(&shares).enumerate() {
                for (j, (lj, sj)) in lines.iter().zip(&shares).enumerate() {
                    let up_i = i128::from(si.minor()) * total > i128::from(amount) * i128::from(li.value.minor());
                    let up_j = i128::from(sj.minor()) * total > i128::from(amount) * i128::from(lj.value.minor());
                    if up_i && !up_j && lj.value.minor() > 0 {
                        let (ci, cj) = (cut(li), cut(lj));
                        prop_assert!(ci > cj || (ci == cj && (li.content(), li.value.minor(), i) < (lj.content(), lj.value.minor(), j)),
                            "line {i} took the fil over line {j}");
                    }
                }
            }
        }

        /// The same basket scanned in another order is charged the same: each
        /// line, by what it is and what it is worth, takes the same share.
        #[test]
        fn prop_proration_is_invariant_under_line_reordering(
            ((lines, amount), shuffled) in discounted_baskets()
                .prop_flat_map(|(lines, amount)| {
                    let reordered = Just(lines.clone()).prop_shuffle();
                    (Just((lines, amount)), reordered)
                })
        ) {
            let before = prorate(jod(amount), &lines).unwrap();
            let after = prorate(jod(amount), &shuffled).unwrap();
            prop_assert_eq!(charged(&lines, &before), charged(&shuffled, &after));
        }

        /// The ids a basket's lines were given decide nothing: with any other
        /// ids, in any order of their own, every line takes the same share.
        #[test]
        fn prop_proration_is_invariant_under_fresh_line_ids(
            (lines, amount) in discounted_baskets(),
            fresh in proptest::collection::vec(any::<u128>(), 12),
        ) {
            let renamed: Vec<_> = lines
                .iter()
                .zip(&fresh)
                .map(|(l, id)| ProrationLine { line_id: SaleLineId::from_uuid(Uuid::from_u128(*id)), ..*l })
                .collect();
            prop_assert_eq!(
                prorate(jod(amount), &lines).unwrap(),
                prorate(jod(amount), &renamed).unwrap()
            );
        }
    }
}
