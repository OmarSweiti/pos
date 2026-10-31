//! The cart total recomputed over 200 lines (microstep 1.4.9, gap G-9).
//!
//! `cargo bench -p pos-domain --bench price_cart` times [`price_cart`] on the
//! cart [`two_hundred_lines`] rings up through the transitions themselves:
//! three tax bands, weighed and counted lines, a share and an amount off some
//! of them, an overridden price, and a share off the basket, which is the most
//! work a recompute does. `cargo test -p pos-domain --bench price_cart` prices
//! it once instead, which is how a gate knows the benchmark still runs.
//!
//! **This file claims no budget.** §7's limit is a p99 under 16 ms on the
//! reference register, and no reference register exists before milestone 5.8
//! (conventions §7.1–§7.2). A figure this prints on a laptop or a hosted
//! runner is indicative and is never committed as a baseline: 1.12.3 compares
//! a pull request against its base on one runner, and 5.8.2 records the first
//! baseline and proves the limit with `just bench-gate price-cart`.
#![allow(clippy::expect_used)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use pos_domain::cart::{
    self, AddLine, Cart, CartContext, PricingSettings, TaxBands, add_line, apply_basket_discount,
    apply_line_discount, override_price, price_cart,
};
use pos_domain::permissions::{
    ApprovalBinding, Authorized, Capability, EscalationPolicy, GrantSet, Role, authorize, cap,
};
use pos_domain::pricing::{
    DiscountCap, DiscountKind, DiscountReason, DiscountRequest, OverrideReason,
};
use pos_domain::tax::{PriceMode, StoreTaxProfile, TaxBase, TaxBasis, TaxComponent, TaxTreatment};
use pos_domain::{
    Currency, Money, Percent, Product, ProductId, Qty, RegisterId, RegulatedSaleForm, RoundingRule,
    SaleForm, SaleId, SaleLineId, ShiftId, TaxCategoryId, Timestamp, UnitOfMeasure, UserId,
};
use uuid::Uuid;

/// §7: "50 measured samples after warm-up". criterion's warm-up runs first
/// and is not counted among them.
const SAMPLES: usize = 50;

/// G-9's cart: 200 lines.
const LINES: u128 = 200;

const RULE: RoundingRule = RoundingRule::HalfAwayFromZero;

fn jod(minor: i64) -> Money {
    Money::from_minor(minor, Currency::JOD)
}

fn at() -> Timestamp {
    Timestamp::from_epoch_milliseconds(978_307_320_000).expect("a representable instant")
}

/// `who`'s proof of `C` under the shipped grid for `role`.
fn proof<C: Capability>(who: u128, role: Role) -> Authorized<C> {
    let unbound = ApprovalBinding {
        entity_id: Uuid::nil(),
        amount_minor: 0,
        content_hash: None,
    };
    authorize(
        UserId::from_uuid(Uuid::from_u128(who)),
        &GrantSet::of_role(role),
        None,
        &unbound,
        &EscalationPolicy::empty(),
        at(),
    )
    .expect("the shipped grid grants it")
}

fn band(n: u128) -> TaxCategoryId {
    TaxCategoryId::from_uuid(Uuid::from_u128(0x12 + n))
}

/// A synthetic pack: 16% standard, zero-rated and exempt.
fn bands() -> TaxBands {
    let gst = |treatment, ppm| {
        vec![TaxComponent {
            code: "GST".to_owned(),
            treatment,
            basis: TaxBasis::Percentage {
                rate: Percent::from_ppm(ppm),
            },
            sequence: 1,
            base: TaxBase::LineNet,
            is_inclusive_capable: true,
        }]
    };
    TaxBands::from([
        (band(0), gst(TaxTreatment::Standard, 160_000)),
        (band(1), gst(TaxTreatment::Zero, 0)),
        (band(2), gst(TaxTreatment::Exempt, 0)),
    ])
}

/// Line `n`'s product: every third weighed, priced from 0.250 to 9.750, in
/// one of the three bands. Synthetic names, ids and prices.
fn product(n: u128) -> Product {
    let weighed = n.is_multiple_of(3);
    let unit = if weighed {
        UnitOfMeasure::Kilogram
    } else {
        UnitOfMeasure::Each
    };
    Product {
        id: ProductId::from_uuid(Uuid::from_u128(0x1_0000 + n)),
        sku: format!("SYNTH-{n}"),
        name_ar: format!("صنف اصطناعي {n}"),
        name_en: None,
        category_id: None,
        tax_category_id: band(n % 3),
        unit,
        unit_price: jod(250 + i64::try_from(n % 39).expect("small") * 250),
        is_weighed: weighed,
        is_service: false,
        is_active: true,
        min_age: None,
        max_price: None,
        reorder_point: None,
        regulated_sale_form: RegulatedSaleForm::Unregulated(if weighed {
            SaleForm::Bulk
        } else {
            SaleForm::SealedPack
        }),
    }
}

fn discount(kind: DiscountKind) -> DiscountRequest {
    DiscountRequest {
        kind,
        reason: DiscountReason("loyal_customer".to_owned()),
    }
}

/// The 200-line cart, rung up as a till would ring it.
fn two_hundred_lines() -> Cart {
    let manager = proof::<cap::DiscountManual>(0x55, Role::Manager);
    let lead = proof::<cap::PriceOverride>(0x56, Role::ShiftLead);
    let mut cart = cart::open(
        SaleId::from_uuid(Uuid::from_u128(1)),
        &CartContext {
            register_id: RegisterId::from_uuid(Uuid::from_u128(2)),
            shift_id: ShiftId::from_uuid(Uuid::from_u128(3)),
            cashier_id: UserId::from_uuid(Uuid::from_u128(4)),
            currency: Currency::JOD,
            is_training: false,
            opened_at: at(),
        },
    );
    for n in 1..=LINES {
        let item = product(n);
        let qty = if item.is_weighed {
            Qty::from_milli(137 + i64::try_from(n).expect("small") * 7)
        } else {
            Qty::from_milli(1_000 * (1 + i64::try_from(n % 4).expect("small")))
        };
        let line = SaleLineId::from_uuid(Uuid::from_u128(0x2_0000 + n));
        let request = AddLine::from_product(line, &item, qty).expect("an active product");
        cart = add_line(cart, request).expect("a line the cart takes");
        if n.is_multiple_of(17) {
            cart = override_price(
                cart,
                line,
                item.unit_price,
                OverrideReason::Code("catalogue_error".to_owned()),
                Some(jod(0)),
                &lead,
            )
            .expect("the catalogue's own price is never refused")
            .0;
        }
        let kind = match n % 5 {
            0 => Some(DiscountKind::Percent(Percent::from_ppm(50_000))),
            1 if !item.is_weighed => Some(DiscountKind::Amount(jod(10))),
            _ => None,
        };
        if let Some(kind) = kind {
            cart = apply_line_discount(
                cart,
                line,
                discount(kind),
                DiscountCap::Unlimited,
                RULE,
                &manager,
                None,
            )
            .expect("a discount within the line")
            .0;
        }
    }
    apply_basket_discount(
        cart,
        discount(DiscountKind::Percent(Percent::from_ppm(30_000))),
        DiscountCap::Unlimited,
        RULE,
        &manager,
        None,
    )
    .expect("a share off the basket")
    .0
}

fn recompute(c: &mut Criterion) {
    let cart = two_hundred_lines();
    let bands = bands();
    let settings = PricingSettings {
        price_mode: PriceMode::Inclusive,
        profile: StoreTaxProfile::Standard,
        rule: RULE,
    };
    let priced = price_cart(&cart, &bands, &settings).expect("the fixture prices");
    assert_eq!(priced.lines.len(), 200, "G-9's cart is 200 lines");
    assert!(priced.total.minor() > 0 && priced.discount_total.minor() > 0);

    let mut group = c.benchmark_group("price_cart");
    group.sample_size(SAMPLES);
    group.bench_function("200_lines", |b| {
        b.iter(|| price_cart(black_box(&cart), &bands, &settings).expect("the fixture prices"));
    });
    group.finish();
}

criterion_group!(benches, recompute);
criterion_main!(benches);
