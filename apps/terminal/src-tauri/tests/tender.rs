//! Paying for a basket through the tender commands (microstep 1.8.3, its
//! first part): payment journalled before any money is taken, cash settled at
//! the store's coin step, a tender taken back and audited, a way back to the
//! basket while nothing is taken, and the owner's two decisions of
//! 7 November 2026 on a sale left mid-payment.
//!
//! The shop is `shop/mod.rs`'s. Its store's approved policy rounds cash to the
//! nearest 5 fils. Every command is driven through the plain function its
//! Tauri wrapper calls, and every figure expected here is worked out by hand
//! from the coin step, never read back from the code under test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod shop;

use pos_db::repo::checkout_operation::{CheckoutOperation, CheckoutOperationRepository};
use pos_domain::{InFlight, SaleId, TenderId, UserId};
use shop::{
    CASHIER_CODE, CASHIER_ID, CASHIER_PIN, CHEESE, OIL, OWNER_CODE, OWNER_ID, SECOND_CODE,
    SECOND_ID, TOBACCO, add_staff, audited, field, open, product_id, refusal, seed, shop, states,
};
use terminal_lib::commands::auth::{login_pin, logout};
use terminal_lib::commands::cart::{
    CartSnapshot, add_product, answer_age, current, open_cart, park_cart, resume_cart,
};
use terminal_lib::commands::tender::{
    PaymentSnapshot, RoundingLine, add_cash, back_to_cart, begin_payment, in_flight, take_back,
};
use terminal_lib::state::Register;
use uuid::Uuid;

/// The fixture policy's coin step, in fils.
const COIN: i64 = 5;

/// `minor` at the nearest whole coin step. The step is odd, so no remainder is
/// ever exactly half of it.
fn nearest(minor: i64) -> i64 {
    let over = minor.rem_euclid(COIN);
    if over * 2 < COIN {
        minor - over
    } else {
        minor - over + COIN
    }
}

/// A basket of one bottle of oil, at 1.250.
fn one_bottle(register: &Register) -> CartSnapshot {
    open_cart(register).unwrap();
    add_product(register, product_id(OIL), None).unwrap()
}

/// A bottle of oil and 101 g of cheese, which come to no whole number of coin
/// steps, with payment begun.
fn odd_basket(register: &Register) -> PaymentSnapshot {
    open_cart(register).unwrap();
    add_product(register, product_id(OIL), None).unwrap();
    add_product(register, product_id(CHEESE), Some(101)).unwrap();
    let paying = begin_payment(register).unwrap();
    assert_ne!(
        paying.total_minor % COIN,
        0,
        "the basket must need rounding"
    );
    paying
}

/// The checkout journalled for `sale`, if any.
fn journal(register: &Register, sale: SaleId) -> Option<CheckoutOperation> {
    register.with_db(|conn| CheckoutOperationRepository::new(conn).get(sale).unwrap())
}

/// How many tenders the journal holds for `sale`, and the version it is at.
fn journalled(register: &Register, sale: SaleId) -> (usize, i64) {
    let operation = journal(register, sale).unwrap();
    (
        operation.in_flight.tendering().tenders().len(),
        operation.version,
    )
}

/// Whether every audit row written for `action` is a member of a delivery
/// envelope.
fn enveloped(register: &Register, action: &str) -> bool {
    register.with_db(|conn| {
        conn.query_row(
            "SELECT NOT EXISTS (
               SELECT 1 FROM audit_log a
                WHERE a.action = ?1
                  AND NOT EXISTS (SELECT 1 FROM fact_commit_member m
                                   WHERE m.entity = 'audit_log' AND m.entity_id = a.id))",
            [action],
            |row| row.get(0),
        )
        .unwrap()
    })
}

/// A tender id no sale holds.
fn no_such_tender() -> TenderId {
    TenderId::from_uuid(Uuid::from_u128(0x7E))
}

#[test]
fn paying_begins_journalled_and_the_basket_waits_for_it() {
    let (_dir, register) = shop();
    let basket = one_bottle(&register);
    assert!(journal(&register, basket.sale_id).is_none());

    let paying = begin_payment(&register).unwrap();
    assert_eq!(paying.sale_id, basket.sale_id);
    assert_eq!(paying.currency, "JOD");
    assert_eq!(paying.total_minor, 1_250);
    assert!(paying.tenders.is_empty());
    assert_eq!(
        (
            paying.collected_minor,
            paying.remaining_minor,
            paying.cash_due_minor
        ),
        (0, 1_250, 1_250)
    );
    assert_eq!(
        (paying.change_minor, paying.settled, paying.cash_rounding),
        (0, false, None)
    );

    // The checkout is on disk before the answer: its first version, no
    // tender, and the prices payment began at.
    let operation = journal(&register, basket.sale_id).unwrap();
    assert_eq!(operation.version, 0);
    assert!(matches!(operation.in_flight.state, InFlight::Tendering(_)));
    assert!(operation.in_flight.tendering().tenders().is_empty());
    assert_eq!(
        operation.in_flight.tendering().priced().total.minor(),
        1_250
    );
    // The basket's own row still holds it, for the finalize to consume.
    assert_eq!(
        states(&register),
        vec![("active".to_owned(), basket.sale_id)]
    );

    // While it is being paid for, nothing changes the basket, opens another,
    // sets it aside or begins paying for it again.
    for refused in [
        refusal(add_product(&register, product_id(OIL), None)),
        refusal(open_cart(&register)),
        refusal(resume_cart(&register, basket.sale_id)),
        refusal(begin_payment(&register)),
    ] {
        assert_eq!(refused.code, "sale_being_paid");
    }
    assert_eq!(refusal(park_cart(&register, None)).code, "sale_being_paid");
    // The screen still reads it, at the prices it went to payment with.
    let shown = current(&register).unwrap().unwrap();
    assert_eq!((shown.sale_id, shown.total_minor), (basket.sale_id, 1_250));
    assert_eq!(journalled(&register, basket.sale_id), (0, 0));
}

#[test]
fn cash_settles_at_the_coin_step_and_gives_change() {
    let (_dir, register) = shop();
    let paying = odd_basket(&register);
    let (sale, total) = (paying.sale_id, paying.total_minor);
    assert_eq!(paying.cash_due_minor, nearest(total));

    // A part payment is taken exactly, and cash is asked again for what is
    // left, at the coin step.
    let part = add_cash(&register, 1_000).unwrap();
    let left = total - 1_000;
    assert_eq!(part.collected_minor, 1_000);
    assert_eq!(part.remaining_minor, left);
    assert_eq!(part.cash_due_minor, nearest(left));
    assert_eq!(
        (part.change_minor, part.settled, part.cash_rounding),
        (0, false, None)
    );
    let taken = part.tenders.first().unwrap();
    assert_eq!(
        (taken.type_code.as_str(), taken.amount_minor),
        ("cash", 1_000)
    );
    assert_eq!(journalled(&register, sale), (1, 1));

    // Cash covering the exact remainder but not what the coin step asks for
    // would leave the sale neither settled nor payable by a coin.
    assert!(nearest(left) > left, "this basket rounds up");
    assert_eq!(refusal(add_cash(&register, left)).code, "tender_underpaid");
    assert_eq!(journalled(&register, sale), (1, 1));

    // Five dinars settle it at the coin step, and the rest is change.
    let paid = add_cash(&register, 5_000).unwrap();
    let asked = nearest(left);
    assert!(paid.settled);
    assert_eq!(paid.collected_minor, 6_000);
    assert_eq!((paid.remaining_minor, paid.cash_due_minor), (0, 0));
    assert_eq!(
        paid.cash_rounding,
        Some(RoundingLine {
            original_minor: left,
            rounded_minor: asked,
            adjustment_minor: asked - left,
        })
    );
    assert_eq!(paid.change_minor, 5_000 - asked);
    assert_eq!(journalled(&register, sale), (2, 2));

    // Nothing more is owed.
    assert_eq!(refusal(add_cash(&register, COIN)).code, "sale_settled");
    assert_eq!(journalled(&register, sale), (2, 2));
}

#[test]
fn a_removed_tender_is_audited_and_the_rest_are_taken_again() {
    let (_dir, register) = shop();
    let paying = odd_basket(&register);
    let (sale, total) = (paying.sale_id, paying.total_minor);
    add_cash(&register, 1_000).unwrap();
    let paid = add_cash(&register, 5_000).unwrap();
    let first = paid.tenders.first().unwrap().tender_id;

    // A tender the sale does not hold is refused, and nothing changes.
    assert_eq!(
        refusal(take_back(&register, no_such_tender())).code,
        "tender_unknown"
    );
    assert_eq!(journalled(&register, sale), (2, 2));
    assert!(audited(&register, "tender.remove").is_empty());

    // With the first taken back, the five dinars are taken again alone: they
    // now settle the whole sale, rounded from its total.
    let rest = take_back(&register, first).unwrap();
    assert_eq!(rest.tenders.len(), 1);
    assert_eq!(rest.tenders.first().unwrap().amount_minor, 5_000);
    assert!(rest.settled);
    assert_eq!(
        rest.cash_rounding.map(|rounding| rounding.original_minor),
        Some(total)
    );
    assert_eq!(rest.change_minor, 5_000 - nearest(total));
    assert_eq!(journalled(&register, sale), (1, 3));

    // Taking money back is audited, with its delivery envelope.
    let removed = audited(&register, "tender.remove");
    assert_eq!(removed.len(), 1);
    let payload = removed.first().unwrap();
    assert_eq!(*field(payload, "tender_id"), first.as_uuid().to_string());
    assert_eq!(*field(payload, "type_code"), "cash");
    assert_eq!(*field(payload, "amount_minor"), 1_000);
    assert_eq!(*field(payload, "remaining_minor"), 0);
    assert!(enveloped(&register, "tender.remove"));
}

#[test]
fn back_to_the_basket_only_while_nothing_is_taken() {
    let (_dir, register) = shop();
    let basket = one_bottle(&register);
    begin_payment(&register).unwrap();
    let first_key = journal(&register, basket.sale_id)
        .unwrap()
        .in_flight
        .idempotency_key;

    // Nothing taken: the basket comes back, and its checkout is abandoned.
    let again = back_to_cart(&register).unwrap();
    assert_eq!((again.sale_id, again.total_minor), (basket.sale_id, 1_250));
    assert!(journal(&register, basket.sale_id).is_none());
    let more = add_product(&register, product_id(OIL), None).unwrap();
    assert_eq!(more.total_minor, 2_500);

    // Paid for again, it is a new attempt under a fresh key.
    assert_eq!(begin_payment(&register).unwrap().total_minor, 2_500);
    let second_key = journal(&register, basket.sale_id)
        .unwrap()
        .in_flight
        .idempotency_key;
    assert_ne!(first_key, second_key);

    // With money taken there is no way back: the tender is removed first.
    let taken = add_cash(&register, 500).unwrap();
    assert_eq!(refusal(back_to_cart(&register)).code, "tender_collected");
    assert_eq!(journalled(&register, basket.sale_id), (1, 1));
    take_back(&register, taken.tenders.first().unwrap().tender_id).unwrap();
    assert_eq!(back_to_cart(&register).unwrap().total_minor, 2_500);
    assert!(journal(&register, basket.sale_id).is_none());
}

#[test]
fn sign_out_parks_an_unpaid_basket_and_is_refused_once_money_is_taken() {
    let (_dir, register) = shop();
    let basket = one_bottle(&register);
    begin_payment(&register).unwrap();

    // Nothing taken: the sale goes back to its basket, which is parked.
    logout(&register).unwrap();
    assert!(journal(&register, basket.sale_id).is_none());
    assert_eq!(
        states(&register),
        vec![("parked".to_owned(), basket.sale_id)]
    );
    let signed_out = audited(&register, "auth.logout");
    let payload = signed_out.first().unwrap();
    assert_eq!(
        *field(payload, "parked_cart"),
        basket.sale_id.as_uuid().to_string()
    );
    assert!(field(payload, "sale_in_flight").is_null());

    // Resumed and paid for in part, the sale keeps its cashier at the till.
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    resume_cart(&register, basket.sale_id).unwrap();
    begin_payment(&register).unwrap();
    let taken = add_cash(&register, 500).unwrap();
    assert_eq!(refusal(logout(&register)).code, "tender_collected");
    assert_eq!(current(&register).unwrap().unwrap().sale_id, basket.sale_id);
    assert_eq!(journalled(&register, basket.sale_id), (1, 1));
    assert_eq!(audited(&register, "auth.logout").len(), 1);

    // Once the tender is taken back, sign-out sets the basket aside again.
    take_back(&register, taken.tenders.first().unwrap().tender_id).unwrap();
    logout(&register).unwrap();
    assert!(journal(&register, basket.sale_id).is_none());
    assert_eq!(
        states(&register),
        vec![("parked".to_owned(), basket.sale_id)]
    );
}

#[test]
fn a_sale_left_mid_payment_stays_journalled_for_recovery() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), true);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    let left = one_bottle(&register);
    begin_payment(&register).unwrap();
    add_cash(&register, 500).unwrap();

    // The cashier's account stops being live. The next command signs them
    // out, and the sale, with money taken, is left journalled, not parked.
    register.with_db(|conn| {
        conn.execute(
            "UPDATE app_user SET is_active = 0 WHERE id = ?1",
            [CASHIER_ID.as_slice()],
        )
        .unwrap();
    });
    assert_eq!(refusal(current(&register)).code, "user_inactive");
    let forced = audited(&register, "auth.logout");
    let payload = forced.first().unwrap();
    assert_eq!(*field(payload, "cause"), "user_inactive");
    assert!(field(payload, "parked_cart").is_null());
    assert_eq!(
        *field(payload, "sale_in_flight"),
        left.sale_id.as_uuid().to_string()
    );
    assert_eq!(states(&register), vec![("active".to_owned(), left.sale_id)]);
    assert_eq!(journalled(&register, left.sale_id), (1, 1));

    // Another cashier sees it named, on no till.
    add_staff(
        &register,
        SECOND_ID,
        SECOND_CODE,
        "Synthetic Second Cashier",
        "cashier",
    );
    login_pin(&register, SECOND_CODE, CASHIER_PIN).unwrap();
    let named = in_flight(&register).unwrap();
    assert_eq!(named.len(), 1);
    let summary = named.first().unwrap();
    assert_eq!(
        (summary.sale_id, summary.state, summary.on_this_till),
        (left.sale_id, "tendering", false)
    );
    assert_eq!(
        summary.cashier_id,
        UserId::from_uuid(Uuid::from_bytes(CASHIER_ID))
    );
    assert_eq!(
        (
            summary.total_minor,
            summary.collected_minor,
            summary.remaining_minor,
            summary.tender_count,
            summary.card_outstanding,
        ),
        (1_250, 500, 750, 1, false)
    );

    // The second cashier begins paying for a basket of their own, which this
    // till is paying for; then the process dies.
    let own = one_bottle(&register);
    begin_payment(&register).unwrap();
    let both = in_flight(&register).unwrap();
    assert_eq!(both.len(), 2);
    assert!(
        both.iter()
            .any(|sale| sale.sale_id == own.sale_id && sale.on_this_till)
    );
    drop(register);

    // A restart leaves both where they are, for the recovery half: neither
    // basket is parked, and both journals stand.
    let restarted = open(dir.path());
    assert_eq!(restarted.recovered().sales_in_flight, 2);
    assert_eq!(restarted.recovered().carts_set_aside, 0);
    assert_eq!(restarted.recovered().empty_carts_dropped, 0);
    let mut rows = states(&restarted);
    rows.sort_by_key(|(_, sale)| sale.as_uuid());
    let mut expected = vec![
        ("active".to_owned(), left.sale_id),
        ("active".to_owned(), own.sale_id),
    ];
    expected.sort_by_key(|(_, sale)| sale.as_uuid());
    assert_eq!(rows, expected);
    assert_eq!(journalled(&restarted, left.sale_id), (1, 1));
    assert_eq!(journalled(&restarted, own.sale_id), (0, 0));
    // The till starts empty, and both are named, on no till.
    login_pin(&restarted, SECOND_CODE, CASHIER_PIN).unwrap();
    assert!(current(&restarted).unwrap().is_none());
    let named = in_flight(&restarted).unwrap();
    assert_eq!(named.len(), 2);
    assert!(named.iter().all(|sale| !sale.on_this_till));
}

#[test]
fn paying_names_what_it_refuses_and_changes_nothing() {
    let (_dir, register) = shop();
    // No basket on the till.
    assert_eq!(refusal(begin_payment(&register)).code, "cart_not_open");
    assert_eq!(refusal(add_cash(&register, 1_000)).code, "cart_not_open");

    // An empty basket is no sale, and payment for it has not begun.
    let empty = open_cart(&register).unwrap();
    assert_eq!(refusal(begin_payment(&register)).code, "nothing_to_tender");
    for refused in [
        refusal(add_cash(&register, 1_000)),
        refusal(take_back(&register, no_such_tender())),
    ] {
        assert_eq!(refused.code, "payment_not_begun");
    }
    assert_eq!(refusal(back_to_cart(&register)).code, "payment_not_begun");

    // A line whose buyer's age is unconfirmed holds payment back.
    let aged = add_product(&register, product_id(TOBACCO), None).unwrap();
    assert_eq!(refusal(begin_payment(&register)).code, "age_unconfirmed");
    assert!(journal(&register, empty.sale_id).is_none());
    answer_age(&register, *aged.awaiting_age.first().unwrap(), true).unwrap();
    let paying = begin_payment(&register).unwrap();
    assert_eq!(paying.total_minor, 2_500);

    // Nothing, less than nothing, and more than can be counted.
    for amount in [0, -COIN] {
        assert_eq!(
            refusal(add_cash(&register, amount)).code,
            "tender_not_positive"
        );
    }
    add_cash(&register, 1_000).unwrap();
    assert_eq!(
        refusal(add_cash(&register, i64::MAX)).code,
        "tender_amount_invalid"
    );

    // A register that takes no cash takes none.
    register.with_db(|conn| {
        conn.execute(
            "UPDATE tender_type SET is_active = 0 WHERE code = 'cash'",
            [],
        )
        .unwrap();
    });
    assert_eq!(
        refusal(add_cash(&register, 1_500)).code,
        "tender_type_disabled"
    );
    // None of it changed the sale: one tender, journalled once.
    assert_eq!(journalled(&register, paying.sale_id), (1, 1));
}

#[test]
fn paying_needs_a_role_that_sells() {
    let (_dir, register) = shop();
    let basket = one_bottle(&register);
    logout(&register).unwrap();
    add_staff(&register, OWNER_ID, OWNER_CODE, "Synthetic Owner", "owner");
    login_pin(&register, OWNER_CODE, CASHIER_PIN).unwrap();
    for refused in [
        refusal(begin_payment(&register)),
        refusal(add_cash(&register, 500)),
        refusal(take_back(&register, no_such_tender())),
    ] {
        assert_eq!(refused.code, "permission_denied");
    }
    assert_eq!(refusal(back_to_cart(&register)).code, "permission_denied");
    assert_eq!(refusal(in_flight(&register)).code, "permission_denied");
    assert!(journal(&register, basket.sale_id).is_none());
}
