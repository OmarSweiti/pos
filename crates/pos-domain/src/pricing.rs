//! Manual discounts (microstep 1.4.5, `ref/domain-api.md` §6.1): what a
//! cashier asks for, the cap that bounds it, and what a discount leaves on a
//! line.
//!
//! A discount is kept as it was asked for — a share or an amount — and turned
//! into money where every other number is, when the cart is priced (1.4.9),
//! so a percentage keeps meaning the same share when the quantity changes.
//! The checks made when it is applied are exact and need no rounding rule:
//! [`exceeds_value`] and [`DiscountCap::admits`] compare against the line's
//! unrounded value, so they give the same answer under every rule.
//!
//! This module sits below `cart` (§15): it knows nothing of a cart, and
//! `cart::apply_line_discount` is the transition that uses it.

use serde::{Deserialize, Serialize};

use crate::ids::UserId;
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
        let DiscountCap::AtMost(cap) = self else {
            return true;
        };
        match kind {
            DiscountKind::Percent(p) => p.ppm() <= cap.ppm(),
            DiscountKind::Amount(a) => {
                // a / value <= cap / WHOLE, with value = price × qty / 1000:
                // a × WHOLE × 1000 <= cap × price × qty.
                let asked = i128::from(a.minor()) * i128::from(WHOLE) * MILLI_PER_UNIT;
                let value = i128::from(unit_price.minor()) * i128::from(qty.milli());
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
    match kind {
        DiscountKind::Percent(p) => p.ppm() > WHOLE,
        DiscountKind::Amount(a) => {
            i128::from(a.minor()) * MILLI_PER_UNIT
                > i128::from(unit_price.minor()) * i128::from(qty.milli())
        }
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
        match self.kind {
            DiscountKind::Percent(p) => gross.mul_percent(p, rule),
            DiscountKind::Amount(a) if a.currency() == gross.currency() => Ok(a),
            DiscountKind::Amount(a) => Err(MoneyError::CurrencyMismatch(
                gross.currency().code(),
                a.currency().code(),
            )),
        }
    }
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
}
