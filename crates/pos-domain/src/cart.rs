//! The checkout state machine (group 1.4, `ref/domain-api.md` §6).
//!
//! Blueprint §8's diagram as a Rust enum. A sale is in exactly one state and
//! each state is its own type, so a transition from the wrong state does not
//! compile. Every transition is a free function that takes a state by value
//! and returns the next one or a [`CartError`]; nothing mutates in place.
//!
//! As delivered at 1.4.1, the machine holds the states whose data exists: a
//! cart being built and a parked one. Each later state arrives with the
//! transition that reaches it (tendering, finalizing, complete and voided at
//! 1.4.8), and each later field of a cart or a line with the step that gives
//! it meaning: discounts at 1.4.5, a price override at 1.4.7, and a promotion
//! group when promotions are first captured. A field added before its rule
//! would be a value nothing checks.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::catalog::{DerivedWeight, PriceOrigin, UnitOfMeasure};
use crate::ids::{
    CustomerId, ProductId, RegisterId, SaleId, SaleLineId, ShiftId, TaxCategoryId, UserId,
};
use crate::money::{Currency, Money, MoneyError, Qty};
use crate::tax::{SupplyDestination, SupplyTaxContext, TaxError};
use crate::time::Timestamp;

/// A sale, in the one state it is in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Sale {
    /// Lines are being added, changed or removed.
    Building(Cart),
    /// Set aside by the cashier to serve someone else, and resumed later
    /// (1.4.4).
    Parked(Cart),
}

/// What a cart is opened with: the register, the shift, the cashier and the
/// moment, which the shell supplies because this crate reads no clock (I-8).
///
/// `shift_id` is not optional. A cart's business date is its shift's, and a
/// sale outside a shift has no business date at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CartContext {
    pub register_id: RegisterId,
    pub shift_id: ShiftId,
    pub cashier_id: UserId,
    pub currency: Currency,
    /// Training carts are carried from the first moment, because every report
    /// and the fiscal queue must be able to leave them out.
    pub is_training: bool,
    pub opened_at: Timestamp,
}

/// A sale being built: its lines and who and what it is for.
///
/// `Debug` is written by hand so that `buyer_name` never reaches a log line,
/// a crash report or an error's detail (`.claude/rules/security.md`). The TIN
/// stays visible, as on the receipt's buyer block, because a fiscal invoice
/// that names the wrong buyer is diagnosed by it.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Cart {
    pub id: SaleId,
    pub register_id: RegisterId,
    pub shift_id: ShiftId,
    pub cashier_id: UserId,
    pub currency: Currency,
    pub lines: Vec<CartLine>,
    pub customer_id: Option<CustomerId>,
    /// A business buyer's tax number, for a B2B fiscal invoice (master plan
    /// B.2).
    pub buyer_tin: Option<String>,
    pub buyer_name: Option<String>,
    /// Why this supply is taxed the way it is (§5). Every cart opens
    /// `Domestic`, and 1.3.3 refuses any other destination until the
    /// accountant names the zero-rated lines (`ref/merchant-decisions.md`
    /// 6.14).
    pub supply: SupplyTaxContext,
    /// Checked by every report and, later, by the fiscal queue.
    pub is_training: bool,
    pub opened_at: Timestamp,
}

impl fmt::Debug for Cart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cart")
            .field("id", &self.id)
            .field("register_id", &self.register_id)
            .field("shift_id", &self.shift_id)
            .field("cashier_id", &self.cashier_id)
            .field("currency", &self.currency)
            .field("lines", &self.lines)
            .field("customer_id", &self.customer_id)
            .field("buyer_tin", &self.buyer_tin)
            .field(
                "buyer_name",
                &self.buyer_name.as_ref().map(|_| "[redacted]"),
            )
            .field("supply", &self.supply)
            .field("is_training", &self.is_training)
            .field("opened_at", &self.opened_at)
            .finish()
    }
}

/// One line of a cart, with everything the sale must remember about it.
///
/// The name and the price are copied when the line is captured and never read
/// from the catalogue again (I-5), and `price_origin` says where that price
/// came from. A department sale has no product (§6.5, 1.4.12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartLine {
    pub id: SaleLineId,
    pub product_id: Option<ProductId>,
    pub name_snapshot: String,
    pub unit_price: Money,
    pub price_origin: PriceOrigin,
    pub qty: Qty,
    pub unit: UnitOfMeasure,
    pub is_weighed: bool,
    pub tax_category_id: TaxCategoryId,
    /// Captured with the line, so a catalogue edit while the cart is open
    /// cannot clear the age gate (1.4.3, 1.4.3b).
    pub min_age: Option<u8>,
    pub age_confirmed: bool,
    /// The weight a scale's label claimed: the stock basis, never money
    /// (§4.3).
    pub label: Option<DerivedWeight>,
}

/// A new, empty cart for sale `id`, being built.
///
/// It sells to no named customer or business buyer, and its supply is an
/// ordinary domestic one.
#[must_use]
pub fn open(id: SaleId, ctx: &CartContext) -> Cart {
    Cart {
        id,
        register_id: ctx.register_id,
        shift_id: ctx.shift_id,
        cashier_id: ctx.cashier_id,
        currency: ctx.currency,
        lines: Vec::new(),
        customer_id: None,
        buyer_tin: None,
        buyer_name: None,
        supply: SupplyTaxContext {
            destination: SupplyDestination::Domestic,
            reason: None,
            evidence_ref: None,
        },
        is_training: ctx.is_training,
        opened_at: ctx.opened_at,
    }
}

/// Every refusal a cart transition can return (`ref/domain-api.md` §6.3).
///
/// Each carries the data its message needs, so the shell maps a variant to
/// the code the screen renders from and never parses a message. Amounts in
/// a message are carried as their exact rendering.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CartError {
    #[error("no line {0}")]
    LineNotFound(SaleLineId),
    /// I-4: nothing changes a completed sale.
    #[error("cannot modify a completed sale")]
    SaleIsComplete,
    #[error("cannot go back to building: {0} tender(s) collected")]
    TenderCollected(usize),
    #[error("quantity must be positive")]
    NonPositiveQty,
    #[error("{0} is sold in whole units; {1} milli is not one")]
    FractionalQtyForDiscrete(ProductId, i64),
    #[error("a caller-supplied price needs the department-sale capability")]
    DepartmentPriceOutsideDepartmentSale,
    #[error("department sale of {0} exceeds the cap {1}")]
    DepartmentAmountAboveCap(String, String),
    /// E.19.
    #[error("negative line amounts are not allowed")]
    NegativeLine,
    #[error("discount {0} exceeds line value")]
    DiscountExceedsLine(String),
    #[error("price {0} is below the floor {1}")]
    BelowPriceFloor(String, String),
    /// E.71: a regulated ceiling is a hard block.
    #[error("price {0} exceeds the regulated ceiling {1}")]
    AboveMaxPrice(String, String),
    /// E.69.
    #[error("age confirmation required for {0}")]
    AgeConfirmationRequired(ProductId),
    /// E.38.
    #[error("product {0} is not sellable")]
    ProductInactive(ProductId),
    #[error("collected {0} is less than due {1}")]
    Underpaid(String, String),
    #[error("overtender is only allowed for cash")]
    OvertenderNotCash,
    #[error(transparent)]
    Tax(#[from] TaxError),
    #[error(transparent)]
    Money(#[from] MoneyError),
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::catalog::DerivedWeight;
    use uuid::Uuid;

    // Wire-format tripwires, not business data: every id, amount and name is
    // visibly synthetic.
    const GOLDEN_CART_JSON: &str = r#"{"id":"00000000-0000-0000-0000-000000000001","register_id":"00000000-0000-0000-0000-000000000002","shift_id":"00000000-0000-0000-0000-000000000003","cashier_id":"00000000-0000-0000-0000-000000000004","currency":"JOD","lines":[{"id":"00000000-0000-0000-0000-000000000010","product_id":"00000000-0000-0000-0000-000000000011","name_snapshot":"SYNTH item","unit_price":{"minor":1250,"currency":"JOD"},"price_origin":{"catalog":{"product_id":"00000000-0000-0000-0000-000000000011"}},"qty":1500,"unit":"kilogram","is_weighed":true,"tax_category_id":"00000000-0000-0000-0000-000000000012","min_age":18,"age_confirmed":false,"label":{"qty":1500,"catalog_unit_price":{"minor":1250,"currency":"JOD"}}}],"customer_id":null,"buyer_tin":"000000000","buyer_name":"SYNTH buyer","supply":{"destination":"Domestic","reason":null,"evidence_ref":null},"is_training":true,"opened_at":978307200000}"#;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn context(is_training: bool) -> CartContext {
        CartContext {
            register_id: RegisterId::from_uuid(id(2)),
            shift_id: ShiftId::from_uuid(id(3)),
            cashier_id: UserId::from_uuid(id(4)),
            currency: Currency::JOD,
            is_training,
            opened_at: Timestamp::from_epoch_milliseconds(978_307_200_000).unwrap(),
        }
    }

    fn line() -> CartLine {
        CartLine {
            id: SaleLineId::from_uuid(id(0x10)),
            product_id: Some(ProductId::from_uuid(id(0x11))),
            name_snapshot: "SYNTH item".to_owned(),
            unit_price: Money::from_minor(1_250, Currency::JOD),
            price_origin: PriceOrigin::Catalog {
                product_id: ProductId::from_uuid(id(0x11)),
            },
            qty: Qty::from_milli(1_500),
            unit: UnitOfMeasure::Kilogram,
            is_weighed: true,
            tax_category_id: TaxCategoryId::from_uuid(id(0x12)),
            min_age: Some(18),
            age_confirmed: false,
            label: Some(DerivedWeight {
                qty: Qty::from_milli(1_500),
                catalog_unit_price: Money::from_minor(1_250, Currency::JOD),
            }),
        }
    }

    fn cart() -> Cart {
        let mut cart = open(SaleId::from_uuid(id(1)), &context(true));
        cart.lines.push(line());
        cart.buyer_tin = Some("000000000".to_owned());
        cart.buyer_name = Some("SYNTH buyer".to_owned());
        cart
    }

    #[test]
    fn a_new_cart_is_empty_domestic_and_its_context_is_copied() {
        let ctx = context(false);
        let cart = open(SaleId::from_uuid(id(1)), &ctx);
        assert_eq!(cart.id, SaleId::from_uuid(id(1)));
        assert_eq!(
            (cart.register_id, cart.shift_id, cart.cashier_id),
            (ctx.register_id, ctx.shift_id, ctx.cashier_id)
        );
        assert_eq!(cart.currency, Currency::JOD);
        assert_eq!(cart.opened_at, ctx.opened_at);
        assert!(cart.lines.is_empty());
        assert_eq!(
            (cart.customer_id, cart.buyer_tin, cart.buyer_name),
            (None, None, None)
        );
        assert_eq!(
            cart.supply,
            SupplyTaxContext {
                destination: SupplyDestination::Domestic,
                reason: None,
                evidence_ref: None,
            }
        );
        assert!(!cart.is_training);
    }

    #[test]
    fn a_training_cart_says_so_from_its_first_moment() {
        assert!(open(SaleId::from_uuid(id(1)), &context(true)).is_training);
        assert!(!open(SaleId::from_uuid(id(1)), &context(false)).is_training);
    }

    #[test]
    fn golden_cart_json_is_stable() {
        assert_eq!(serde_json::to_string(&cart()).unwrap(), GOLDEN_CART_JSON);
        let back: Cart = serde_json::from_str(GOLDEN_CART_JSON).unwrap();
        assert_eq!(back, cart());
    }

    #[test]
    fn a_sale_round_trips_in_either_state() {
        for sale in [Sale::Building(cart()), Sale::Parked(cart())] {
            let json = serde_json::to_string(&sale).unwrap();
            assert_eq!(serde_json::from_str::<Sale>(&json).unwrap(), sale);
        }
    }

    #[test]
    fn buyer_name_never_reaches_debug_output() {
        let cart = cart();
        for rendered in [
            format!("{cart:?}"),
            format!("{cart:#?}"),
            format!("{:?}", Sale::Parked(cart.clone())),
        ] {
            assert!(!rendered.contains("SYNTH buyer"), "{rendered}");
            assert!(rendered.contains("[redacted]"));
            // The TIN stays visible, as on the receipt's buyer block.
            assert!(rendered.contains("000000000"));
        }
        // A cart with no buyer shows no redaction marker.
        let anonymous = open(SaleId::from_uuid(id(1)), &context(false));
        assert!(!format!("{anonymous:?}").contains("[redacted]"));
    }

    #[test]
    fn cart_errors_carry_their_data() {
        let product = ProductId::from_uuid(id(0x11));
        let line = SaleLineId::from_uuid(id(0x10));
        let cases = [
            (CartError::LineNotFound(line), format!("no line {line}")),
            (
                CartError::TenderCollected(2),
                "cannot go back to building: 2 tender(s) collected".to_owned(),
            ),
            (
                CartError::FractionalQtyForDiscrete(product, 1_500),
                format!("{product} is sold in whole units; 1500 milli is not one"),
            ),
            (
                CartError::DepartmentAmountAboveCap("12.000".to_owned(), "10.000".to_owned()),
                "department sale of 12.000 exceeds the cap 10.000".to_owned(),
            ),
            (
                CartError::BelowPriceFloor("0.900".to_owned(), "1.000".to_owned()),
                "price 0.900 is below the floor 1.000".to_owned(),
            ),
            (
                CartError::AboveMaxPrice("1.200".to_owned(), "1.100".to_owned()),
                "price 1.200 exceeds the regulated ceiling 1.100".to_owned(),
            ),
            (
                CartError::AgeConfirmationRequired(product),
                format!("age confirmation required for {product}"),
            ),
            (
                CartError::ProductInactive(product),
                format!("product {product} is not sellable"),
            ),
            (
                CartError::Underpaid("4.000".to_owned(), "5.000".to_owned()),
                "collected 4.000 is less than due 5.000".to_owned(),
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn tax_and_money_errors_pass_through_unchanged() {
        assert_eq!(
            CartError::from(TaxError::NoComponents).to_string(),
            TaxError::NoComponents.to_string()
        );
        let money = MoneyError::Overflow;
        assert_eq!(CartError::from(money).to_string(), "arithmetic overflow");
    }
}
