//! Building a basket through the cart commands, setting it aside and bringing
//! it back, and overriding a line's price (microstep 1.4.11, its first three
//! parts).
//!
//! The shop is `shop/mod.rs`'s, and every command is driven through the plain
//! function its Tauri wrapper calls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod shop;

use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::parked_cart::ParkedCartRepository;
use pos_domain::{
    ApprovalBinding, ApprovalHandle, ApprovalId, BusinessDate, ClockConfidence, EscalationPolicy,
    GrantSet, ProductId, Role, SaleId, SaleLineId, UnitOfMeasure, UserId, cap,
};
use rusqlite::{OptionalExtension, params};
use shop::{
    AT, BREAD, CASHIER_CODE, CASHIER_ID, CASHIER_PIN, CHEESE, CHEESE_PLU, DAY, LEAD_CODE, LEAD_ID,
    MANAGER_CODE, MANAGER_ID, OIL, OIL_BOTTLE, OIL_SIX, OWNER_CODE, OWNER_ID, SECOND_CODE,
    SECOND_ID, SHIFT_ID, TOBACCO, add_staff, at, audited, field, id, jod, label, open, product_id,
    refusal, saved, seed, shop, states, the_line,
};
use terminal_lib::commands::auth::{login_pin, logout};
use terminal_lib::commands::cart::{
    CartSnapshot, PARK_LABEL_MAX_CHARS, ParkedRef, PriceChange, add_product, add_scan, answer_age,
    current, list_parked, open_cart, override_line_price, park_cart, resume_cart,
};
use terminal_lib::commands::sale::finalize;
use terminal_lib::commands::tender::{add_cash, begin_payment};
use terminal_lib::ipc::IpcError;
use terminal_lib::ipc::context::merchant_policy;
use terminal_lib::state::Register;
use uuid::Uuid;

#[test]
fn a_cart_opens_on_the_open_shift_and_is_held_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), true);
    let register = open(dir.path());

    // No one is signed in: nothing opens.
    assert_eq!(refusal(open_cart(&register)).code, "not_signed_in");
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(current(&register).unwrap(), None);

    let opened = open_cart(&register).unwrap();
    assert!(opened.lines.is_empty());
    assert_eq!(
        (opened.currency, opened.total_minor, opened.tax_total_minor),
        ("JOD", 0, 0)
    );
    // Asking again answers with the same cart; a second press opens nothing.
    assert_eq!(open_cart(&register).unwrap(), opened);
    assert_eq!(current(&register).unwrap(), Some(opened.clone()));

    // It is on disk from its first moment, claimed, on the open shift.
    let rows = saved(&register);
    assert_eq!(rows.len(), 1);
    let (state, cart) = rows.first().unwrap();
    assert_eq!(state, "active");
    assert_eq!(cart.id, opened.sale_id);
    assert_eq!(cart.shift_id.as_uuid().as_bytes(), &SHIFT_ID);
    assert_eq!(cart.cashier_id.as_uuid().as_bytes(), &CASHIER_ID);
    assert!(!cart.is_training);
}

#[test]
fn a_register_with_no_open_shift_opens_no_cart() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), false);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(refusal(open_cart(&register)).code, "shift_not_open");
    assert!(saved(&register).is_empty());
    // And no line can be added to a cart that is not open.
    assert_eq!(
        refusal(add_product(&register, product_id(OIL), None)).code,
        "cart_not_open"
    );
}

#[test]
fn a_line_is_the_catalogues_price_and_a_refused_change_changes_nothing() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();

    // One bottle by default, at the catalogue's 1.250, named as the catalogue
    // names it, and priced under the store's own settings: prices include
    // tax, so the line's total is its price and its tax is inside it.
    let one = add_product(&register, product_id(OIL), None).unwrap();
    let line = the_line(&one);
    assert_eq!(
        (
            line.product_id,
            line.name.as_str(),
            line.qty_milli,
            line.unit_price_minor
        ),
        (Some(product_id(OIL)), "زيت زيتون", 1_000, 1_250)
    );
    assert_eq!((line.gross_minor, line.line_total_minor), (1_250, 1_250));
    assert!(line.tax_minor > 0, "a registered store charges GST");
    assert_eq!(one.total_minor, 1_250);
    assert_eq!(one.tax_total_minor, line.tax_minor);
    assert_eq!(
        one.tax_rows.iter().map(|row| row.tax_minor).sum::<i64>(),
        one.tax_total_minor
    );

    // Three more, as a quantity.
    let four = add_product(&register, product_id(OIL), Some(3_000)).unwrap();
    assert_eq!(four.lines.len(), 2);
    assert_eq!(four.subtotal_minor, 5_000);
    assert_eq!(four.total_minor, 5_000);

    // Each refusal names itself, and leaves the cart and its saved copy as
    // they were.
    register.with_db(|conn| {
        conn.execute(
            "UPDATE product SET is_active = 0 WHERE id = ?1",
            [id(BREAD).as_slice()],
        )
        .unwrap();
    });
    for (product, qty, code) in [
        (product_id(0xDEAD), None, "product_unknown"),
        (product_id(OIL), Some(1_500), "qty_not_whole"),
        (product_id(OIL), Some(0), "qty_not_positive"),
        (product_id(BREAD), None, "product_inactive"),
    ] {
        assert_eq!(
            refusal(add_product(&register, product, qty)).code,
            code,
            "{code}"
        );
    }
    assert_eq!(current(&register).unwrap(), Some(four.clone()));
    let rows = saved(&register);
    let (_, cart) = rows.first().unwrap();
    assert_eq!(cart.lines.len(), 2);
}

#[test]
fn a_scan_adds_what_its_code_names_and_names_every_other_answer() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let trusted = ClockConfidence::Trusted;

    // The bottle's own code adds one; the six-pack's outer code adds six of
    // the same product, at its catalogue price (E.78).
    add_scan(&register, OIL_BOTTLE, trusted).unwrap();
    let both = add_scan(&register, OIL_SIX, trusted).unwrap();
    let quantities: Vec<(Option<ProductId>, i64, i64)> = both
        .lines
        .iter()
        .map(|line| (line.product_id, line.qty_milli, line.unit_price_minor))
        .collect();
    assert_eq!(
        quantities,
        vec![
            (Some(product_id(OIL)), 1_000, 1_250),
            (Some(product_id(OIL)), 6_000, 1_250)
        ]
    );
    assert_eq!(both.total_minor, 8_750);

    // Every scan that adds no line says why, each its own code.
    let good = label("22", CHEESE_PLU, 416);
    let damaged: String = good
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i == 8 {
                if c == '9' { '8' } else { '9' }
            } else {
                c
            }
        })
        .collect();
    for (raw, code) in [
        ("6290000000006", "scan_unknown"),
        (damaged.as_str(), "scan_checksum_failed"),
        ("", "scan_unreadable"),
    ] {
        assert_eq!(
            refusal(add_scan(&register, raw, trusted)).code,
            code,
            "{raw:?}"
        );
    }
    assert_eq!(current(&register).unwrap(), Some(both));
}

#[test]
fn a_price_label_prices_its_line_only_from_a_trusted_clock() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let code = label("22", CHEESE_PLU, 416);

    // The register holds no trusted time yet, so its own clock confidence
    // stops every scale label, by the scale's reason, never as an unknown
    // code to quick-add. A plain barcode still sells.
    assert_eq!(register.clock_confidence(), ClockConfidence::Untrusted);
    let refused = refusal(add_scan(&register, &code, register.clock_confidence()));
    assert_eq!(refused.code, "scan_scale_refused");
    assert_eq!(
        refused.detail,
        Some("the register's clock is not trusted, so the scale's verification cannot be judged")
    );
    add_scan(&register, OIL_BOTTLE, register.clock_confidence()).unwrap();

    // From a trusted clock, the label is what is charged: one package of
    // cheese at 0.416, whatever its weight (E.80). No argument carried that
    // price; the scan handler read it from the label.
    let priced = add_scan(&register, &code, ClockConfidence::Trusted).unwrap();
    let cheese = priced
        .lines
        .iter()
        .find(|line| line.product_id == Some(product_id(CHEESE)))
        .unwrap();
    assert_eq!(
        (
            cheese.qty_milli,
            cheese.unit,
            cheese.unit_price_minor,
            cheese.line_total_minor
        ),
        (1_000, UnitOfMeasure::Package, 416, 416)
    );
    assert_eq!(priced.total_minor, 1_250 + 416);
    // Its stock is the weight its price implies, 49 g at 8.500 a kilogram,
    // not its one package: half a kilogram of cheese covers it.
    assert!(priced.stock_flags.is_empty());
}

#[test]
fn an_age_restricted_line_waits_for_an_operator_and_each_answer_is_audited() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();

    // The tobacco goes in blocked: it stays on screen, waiting for an
    // operator's answer (E.69, 1.4.3).
    let blocked = add_product(&register, product_id(TOBACCO), None).unwrap();
    let line = the_line(&blocked).clone();
    assert_eq!((line.min_age, line.age_confirmed), (Some(18), false));
    assert_eq!(blocked.awaiting_age, vec![line.line_id]);

    // Yes unblocks it, and is audited with the cart's saving.
    let confirmed = answer_age(&register, line.line_id, true).unwrap();
    assert!(confirmed.awaiting_age.is_empty());
    assert!(the_line(&confirmed).age_confirmed);
    assert_eq!(
        refusal(answer_age(&register, line.line_id, true)).code,
        "age_not_pending",
        "a second answer would be a second audit row for one decision"
    );

    // No removes the second pack, and is audited too.
    let second = add_product(&register, product_id(TOBACCO), None).unwrap();
    let waiting = second.awaiting_age.first().copied().unwrap();
    let declined = answer_age(&register, waiting, false).unwrap();
    assert_eq!(declined.lines, confirmed.lines);
    assert!(declined.awaiting_age.is_empty());

    let answers: Vec<(String, bool)> = ["age.confirm", "age.decline"]
        .into_iter()
        .flat_map(|action| {
            audited(&register, action).into_iter().map(move |payload| {
                (
                    action.to_owned(),
                    field(&payload, "confirmed").as_bool().unwrap(),
                )
            })
        })
        .collect();
    assert_eq!(
        answers,
        vec![
            ("age.confirm".to_owned(), true),
            ("age.decline".to_owned(), false)
        ]
    );
    // Every audit row went out with its envelope (I-9).
    let unenveloped: i64 = register.with_db(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM audit_log a
              WHERE a.action LIKE 'age.%'
                AND NOT EXISTS (SELECT 1 FROM fact_commit_member m WHERE m.entity_id = a.id)",
            [],
            |row| row.get(0),
        )
        .unwrap()
    });
    assert_eq!(unenveloped, 0);
    assert_eq!(
        refusal(answer_age(
            &register,
            pos_domain::SaleLineId::from_uuid(Uuid::from_u128(0xBAD)),
            true
        ))
        .code,
        "line_unknown"
    );
}

#[test]
fn stock_is_screened_as_a_line_is_added() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();

    // The store allows and flags by default (C.7): bread, of which there is
    // none, is sold and flagged loudly.
    let flagged = add_product(&register, product_id(BREAD), None).unwrap();
    assert_eq!(flagged.stock_flags.len(), 1);
    let flag = flagged.stock_flags.first().unwrap();
    assert_eq!(
        (flag.product_id, flag.on_hand_milli, flag.taken_milli),
        (product_id(BREAD), 0, 1_000)
    );
    // Within stock, nothing is flagged for it.
    let oil = add_product(&register, product_id(OIL), Some(10_000)).unwrap();
    assert_eq!(oil.stock_flags.len(), 1, "only the bread");

    // Under a hard block, a line that would take a product below zero is
    // refused, and the cart is as it was.
    register.with_db(|conn| {
        conn.execute("UPDATE store SET allow_negative_stock = 0", [])
            .unwrap();
    });
    assert_eq!(
        refusal(add_product(&register, product_id(OIL), None)).code,
        "stock_would_go_negative"
    );
    let unchanged = current(&register).unwrap().unwrap();
    assert_eq!(unchanged.lines, oil.lines);
    // A basket that is already short is still shown, with its flag.
    assert_eq!(unchanged.stock_flags, oil.stock_flags);
}

#[test]
fn every_change_is_on_disk_and_a_crash_sets_the_basket_aside() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), true);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), None).unwrap();
    let basket = add_product(&register, product_id(TOBACCO), None).unwrap();

    // The saved copy is the basket the cashier is looking at.
    let rows = saved(&register);
    let (state, cart) = rows.first().unwrap();
    assert_eq!(state, "active");
    let saved_lines: Vec<_> = cart.lines.iter().map(|line| line.id).collect();
    let shown_lines: Vec<_> = basket.lines.iter().map(|line| line.line_id).collect();
    assert_eq!(saved_lines, shown_lines);

    // The process dies with nobody signing out. The next one finds the
    // basket and sets it aside in the parked list, where whoever signs in
    // can resume it; the till itself opens empty.
    drop(register);
    let restarted = open(dir.path());
    assert_eq!(restarted.recovered().carts_set_aside, 1);
    assert_eq!(restarted.recovered().empty_carts_dropped, 0);
    let parked = restarted.with_db(|conn| {
        ParkedCartRepository::new(conn)
            .parked(restarted.register(), BusinessDate::parse(DAY).unwrap())
            .unwrap()
    });
    assert_eq!(parked.len(), 1);
    let entry = parked.first().unwrap();
    assert_eq!(entry.id, basket.sale_id);
    assert_eq!(entry.cart.cart().lines.len(), 2);
    login_pin(&restarted, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(current(&restarted).unwrap(), None);

    // A cart left empty by a crash is dropped instead.
    let fresh = open_cart(&restarted).unwrap();
    drop(restarted);
    let again = open(dir.path());
    assert_eq!(again.recovered().empty_carts_dropped, 1);
    assert!(
        saved(&again)
            .iter()
            .all(|(_, cart)| cart.id != fresh.sale_id)
    );
}

#[test]
fn signing_out_sets_the_basket_aside_and_drops_an_empty_one() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let basket = add_product(&register, product_id(OIL), None).unwrap();

    logout(&register).unwrap();
    let rows = saved(&register);
    assert_eq!(rows.len(), 1);
    let (state, cart) = rows.first().unwrap();
    assert_eq!((state.as_str(), cart.id), ("parked", basket.sale_id));
    let logouts = audited(&register, "auth.logout");
    let payload = logouts.first().unwrap();
    assert_eq!(*field(payload, "cause"), "requested");
    assert_eq!(
        *field(payload, "parked_cart"),
        basket.sale_id.as_uuid().to_string()
    );

    // Signed in again, the till is empty: the basket waits to be resumed.
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(current(&register).unwrap(), None);

    // An empty basket is not kept.
    let empty = open_cart(&register).unwrap();
    logout(&register).unwrap();
    assert!(
        saved(&register)
            .iter()
            .all(|(_, cart)| cart.id != empty.sale_id)
    );
    let logouts = audited(&register, "auth.logout");
    assert_eq!(
        *field(logouts.last().unwrap(), "parked_cart"),
        serde_json::Value::Null
    );
}

#[test]
fn a_deactivated_cashier_is_signed_out_and_their_basket_set_aside() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let basket = add_product(&register, product_id(OIL), None).unwrap();

    register.with_db(|conn| {
        conn.execute(
            "UPDATE app_user SET is_active = 0 WHERE id = ?1",
            [CASHIER_ID.as_slice()],
        )
        .unwrap();
    });
    // The next command finds the account no longer live, signs it out, and
    // the basket goes to the parked list rather than with the session.
    assert_eq!(
        refusal(add_product(&register, product_id(OIL), None)).code,
        "user_inactive"
    );
    let rows = saved(&register);
    let (state, cart) = rows.first().unwrap();
    assert_eq!((state.as_str(), cart.id), ("parked", basket.sale_id));
    assert_eq!(cart.lines.len(), 1);
    let logouts = audited(&register, "auth.logout");
    let payload = logouts.first().unwrap();
    assert_eq!(*field(payload, "cause"), "user_inactive");
    assert_eq!(
        *field(payload, "parked_cart"),
        basket.sale_id.as_uuid().to_string()
    );
    assert_eq!(
        current(&register).map_err(|refused| refused.code),
        Err("not_signed_in")
    );
}

// ── 1.4.11, its second part: setting a basket aside and bringing it back ──

#[test]
fn a_parked_basket_leaves_the_till_free_and_comes_back_as_it_was() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), Some(2_000)).unwrap();
    let basket = add_product(&register, product_id(BREAD), None).unwrap();

    // Parked under a label, the basket leaves the till, and the till is free
    // for the next customer.
    let parked = park_cart(&register, Some("  Blue bag  ")).unwrap();
    assert_eq!(
        parked,
        ParkedRef {
            parked_id: basket.sale_id,
            label: Some("Blue bag".to_owned()),
            parked_by: UserId::from_uuid(Uuid::from_bytes(CASHIER_ID)),
            parked_by_name: "Synthetic Cashier".to_owned(),
            parked_at: at(),
            line_count: 2,
            currency: "JOD",
            total_minor: basket.total_minor,
        }
    );
    assert_eq!(current(&register).unwrap(), None);
    assert_eq!(
        states(&register),
        vec![("parked".to_owned(), basket.sale_id)]
    );
    assert_eq!(list_parked(&register).unwrap(), vec![parked]);

    // A resume onto an empty cart drops it. The basket comes back exactly as
    // it was shown, and a cashier resuming their own writes nothing.
    let empty = open_cart(&register).unwrap();
    assert_eq!(resume_cart(&register, basket.sale_id).unwrap(), basket);
    assert_eq!(current(&register).unwrap(), Some(basket.clone()));
    assert_eq!(
        states(&register),
        vec![("active".to_owned(), basket.sale_id)]
    );
    assert!(list_parked(&register).unwrap().is_empty());
    assert!(
        saved(&register)
            .iter()
            .all(|(_, cart)| cart.id != empty.sale_id)
    );

    // A resume onto a basket with lines parks that basket, in the same step.
    park_cart(&register, None).unwrap();
    open_cart(&register).unwrap();
    let other = add_product(&register, product_id(OIL), None).unwrap();
    assert_eq!(resume_cart(&register, basket.sale_id).unwrap(), basket);
    let listed = list_parked(&register).unwrap();
    assert_eq!(listed.len(), 1);
    let waiting = listed.first().unwrap();
    assert_eq!(
        (
            waiting.parked_id,
            waiting.label.as_deref(),
            waiting.line_count,
            waiting.total_minor
        ),
        (other.sale_id, None, 1, other.total_minor)
    );
    assert!(audited(&register, "sale.resume").is_empty());
}

#[test]
fn parking_names_what_it_refuses_and_changes_nothing() {
    let (_dir, register) = shop();
    assert_eq!(refusal(park_cart(&register, None)).code, "cart_not_open");
    let empty = open_cart(&register).unwrap();
    assert_eq!(refusal(park_cart(&register, None)).code, "nothing_to_park");
    assert_eq!(current(&register).unwrap(), Some(empty));

    let basket = add_product(&register, product_id(OIL), None).unwrap();
    let too_long = "ب".repeat(PARK_LABEL_MAX_CHARS + 1);
    for label in [too_long.as_str(), "Blue\nbag", "Blue\u{7}bag"] {
        assert_eq!(
            refusal(park_cart(&register, Some(label))).code,
            "park_label_invalid"
        );
    }
    assert_eq!(current(&register).unwrap(), Some(basket.clone()));
    assert_eq!(
        states(&register),
        vec![("active".to_owned(), basket.sale_id)]
    );

    // At the limit a label is taken whole, counted in characters in any
    // script; a blank one is no label.
    let longest = "ب".repeat(PARK_LABEL_MAX_CHARS);
    assert_eq!(
        park_cart(&register, Some(&longest)).unwrap().label,
        Some(longest)
    );
    open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), None).unwrap();
    assert_eq!(park_cart(&register, Some("   ")).unwrap().label, None);
}

#[test]
fn another_cashiers_basket_is_resumed_on_the_record() {
    let (_dir, register) = shop();
    add_staff(
        &register,
        SECOND_ID,
        SECOND_CODE,
        "Synthetic Second Cashier",
        "cashier",
    );
    open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), None).unwrap();
    let basket = add_product(&register, product_id(TOBACCO), None).unwrap();
    assert_eq!(basket.awaiting_age.len(), 1);
    // The first cashier signs out, which sets the basket aside.
    logout(&register).unwrap();

    login_pin(&register, SECOND_CODE, CASHIER_PIN).unwrap();
    let listed = list_parked(&register).unwrap();
    assert_eq!(listed.len(), 1);
    let waiting = listed.first().unwrap();
    assert_eq!(
        (
            waiting.parked_id,
            waiting.parked_by_name.as_str(),
            waiting.line_count
        ),
        (basket.sale_id, "Synthetic Cashier", 2)
    );

    // The second cashier takes it over: the basket comes back as it was, its
    // age gate still waiting, and is theirs from now on.
    assert_eq!(resume_cart(&register, basket.sale_id).unwrap(), basket);
    let rows = saved(&register);
    let (state, cart) = rows.first().unwrap();
    assert_eq!(
        (state.as_str(), cart.cashier_id),
        ("active", UserId::from_uuid(Uuid::from_bytes(SECOND_ID)))
    );

    // The handover is on the record, naming who parked it and what it held,
    // and went out with its envelope (I-9).
    let takeovers = audited(&register, "sale.resume");
    assert_eq!(takeovers.len(), 1);
    let payload = takeovers.first().unwrap();
    assert_eq!(
        *field(payload, "parked_by"),
        Uuid::from_bytes(CASHIER_ID).to_string()
    );
    assert_eq!(*field(payload, "line_count"), 2);
    let (actor, entity_id, enveloped): (Vec<u8>, Vec<u8>, i64) = register.with_db(|conn| {
        conn.query_row(
            "SELECT a.actor_id, a.entity_id,
                    EXISTS (SELECT 1 FROM fact_commit_member m WHERE m.entity_id = a.id)
               FROM audit_log a WHERE a.action = 'sale.resume'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
    });
    assert_eq!(actor, SECOND_ID.to_vec());
    assert_eq!(entity_id, basket.sale_id.as_uuid().as_bytes().to_vec());
    assert_eq!(enveloped, 1);
}

/// #397: a basket taken over from another cashier is paid for and completes.
/// The takeover's row names the basket, `parked_cart`, under the sale's id: a
/// row naming the `sale` would have to travel in the sale's own commit, which
/// a row written before the finalize cannot, and the completion would be
/// refused for ever.
#[test]
fn a_basket_taken_over_from_another_cashier_still_completes() {
    let (_dir, register) = shop();
    add_staff(
        &register,
        SECOND_ID,
        SECOND_CODE,
        "Synthetic Second Cashier",
        "cashier",
    );
    open_cart(&register).unwrap();
    let basket = add_product(&register, product_id(OIL), None).unwrap();
    logout(&register).unwrap();
    login_pin(&register, SECOND_CODE, CASHIER_PIN).unwrap();
    resume_cart(&register, basket.sale_id).unwrap();
    begin_payment(&register).unwrap();
    assert!(add_cash(&register, 1_250).unwrap().settled);

    let done = finalize(&register).unwrap();
    assert_eq!(done.sale_id, basket.sale_id);
    assert_eq!(done.receipt_number, "REG01-000001");
    let (entity, entity_id): (String, Vec<u8>) = register.with_db(|conn| {
        conn.query_row(
            "SELECT entity, entity_id FROM audit_log WHERE action = 'sale.resume'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
    });
    assert_eq!(entity, "parked_cart");
    assert_eq!(entity_id, basket.sale_id.as_uuid().as_bytes().to_vec());
}

#[test]
fn a_refused_resume_leaves_both_baskets_where_they_were() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let parked = add_product(&register, product_id(OIL), None).unwrap();
    park_cart(&register, None).unwrap();
    open_cart(&register).unwrap();
    let working = add_product(&register, product_id(BREAD), None).unwrap();
    let before = saved(&register);

    // No such basket.
    let unknown = SaleId::from_uuid(Uuid::from_u128(0xBAD));
    assert_eq!(
        refusal(resume_cart(&register, unknown)).code,
        "parked_cart_unknown"
    );
    // A basket another till is working on.
    let claim = |state: &str, nonce: Option<[u8; 16]>| {
        register.with_db(|conn| {
            conn.execute(
                "UPDATE parked_cart SET state = ?1, session_nonce = ?2 WHERE id = ?3",
                params![
                    state,
                    nonce.map(|n| n.to_vec()),
                    parked.sale_id.as_uuid().as_bytes().as_slice()
                ],
            )
            .unwrap();
        });
    };
    claim("active", Some([0xEE; 16]));
    assert_eq!(
        refusal(resume_cart(&register, parked.sale_id)).code,
        "parked_cart_claimed"
    );
    claim("parked", None);
    // A basket parked on an earlier business day has expired, and is no
    // longer offered.
    register.with_db(|conn| {
        conn.execute(
            "UPDATE parked_cart SET expires_on = '2026-11-04' WHERE id = ?1",
            [parked.sale_id.as_uuid().as_bytes().as_slice()],
        )
        .unwrap();
    });
    assert_eq!(
        refusal(resume_cart(&register, parked.sale_id)).code,
        "parked_cart_expired"
    );
    assert!(list_parked(&register).unwrap().is_empty());

    // Each refusal left the till's basket on the till, unparked, and the
    // parked one where it was.
    assert_eq!(current(&register).unwrap(), Some(working));
    let after = saved(&register);
    assert_eq!(after.len(), before.len());
    for ((state, cart), (was_state, was)) in after.iter().zip(&before) {
        assert_eq!(
            (state, cart.id, &cart.lines),
            (was_state, was.id, &was.lines)
        );
    }
    assert!(audited(&register, "sale.resume").is_empty());
}

#[test]
fn parking_and_resuming_need_a_shift_and_a_role_that_sells() {
    // Outside a shift nothing can be listed or resumed.
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), false);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(refusal(list_parked(&register)).code, "shift_not_open");
    let anything = SaleId::from_uuid(Uuid::from_u128(0xBAD));
    assert_eq!(
        refusal(resume_cart(&register, anything)).code,
        "shift_not_open"
    );

    // An owner sells nothing, so parks and resumes nothing (§8.2's grid).
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let basket = add_product(&register, product_id(OIL), None).unwrap();
    logout(&register).unwrap();
    add_staff(&register, OWNER_ID, OWNER_CODE, "Synthetic Owner", "owner");
    login_pin(&register, OWNER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(
        refusal(park_cart(&register, None)).code,
        "permission_denied"
    );
    assert_eq!(refusal(list_parked(&register)).code, "permission_denied");
    assert_eq!(
        refusal(resume_cart(&register, basket.sale_id)).code,
        "permission_denied"
    );
    assert_eq!(
        states(&register),
        vec![("parked".to_owned(), basket.sale_id)]
    );
}

#[test]
fn a_resumed_basket_changed_and_crashed_comes_back_as_it_was_left() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), true);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    open_cart(&register).unwrap();
    let basket = add_product(&register, product_id(OIL), None).unwrap();
    park_cart(&register, Some("Blue bag")).unwrap();
    resume_cart(&register, basket.sale_id).unwrap();
    // A change to the resumed basket is saved as any other.
    let changed = add_product(&register, product_id(BREAD), None).unwrap();

    // The process dies with nobody signing out. The next start-up sets the
    // basket aside again, under its label, with the line added after it was
    // resumed.
    drop(register);
    let restarted = open(dir.path());
    assert_eq!(restarted.recovered().carts_set_aside, 1);
    login_pin(&restarted, CASHIER_CODE, CASHIER_PIN).unwrap();
    let listed = list_parked(&restarted).unwrap();
    assert_eq!(listed.len(), 1);
    let waiting = listed.first().unwrap();
    assert_eq!(
        (
            waiting.parked_id,
            waiting.label.as_deref(),
            waiting.line_count,
            waiting.total_minor
        ),
        (basket.sale_id, Some("Blue bag"), 2, changed.total_minor)
    );
    assert_eq!(resume_cart(&restarted, basket.sale_id).unwrap(), changed);
}

// ── The price override ──────────────────────────────────────────────────────

/// A shop whose shift lead is signed in, with no cart open yet. The shift
/// lead holds `price.override`; the cashier does not.
fn lead_shop() -> (tempfile::TempDir, Register) {
    let (dir, register) = shop();
    logout(&register).unwrap();
    add_staff(
        &register,
        LEAD_ID,
        LEAD_CODE,
        "Synthetic Shift Lead",
        "shift_lead",
    );
    login_pin(&register, LEAD_CODE, CASHIER_PIN).unwrap();
    (dir, register)
}

/// `cart_override_price` as its Tauri wrapper calls it: under the register's
/// own policy, with no approval.
fn over(
    register: &Register,
    line_id: SaleLineId,
    unit_price_minor: i64,
    reason: &str,
) -> Result<CartSnapshot, IpcError> {
    override_line_price(
        register,
        &PriceChange {
            line_id,
            unit_price_minor,
            reason,
            approval: None,
        },
        &merchant_policy(),
    )
}

/// One `price.override` row as the audit chain holds it.
#[derive(Debug)]
struct OverrideRow {
    reason: Option<String>,
    payload: serde_json::Value,
    approver: Option<Vec<u8>>,
    approval: Option<Vec<u8>>,
    enveloped: bool,
}

/// Every `price.override` row, oldest first.
fn overrides(register: &Register) -> Vec<OverrideRow> {
    register.with_db(|conn| {
        conn.prepare(
            "SELECT a.reason, a.payload, a.approver_id, a.approval_handle_id,
                    EXISTS (SELECT 1 FROM fact_commit_member m WHERE m.entity_id = a.id)
               FROM audit_log a WHERE a.action = 'price.override' ORDER BY a.seq",
        )
        .unwrap()
        .query_map([], |row| {
            Ok(OverrideRow {
                reason: row.get(0)?,
                payload: serde_json::from_str(&row.get::<_, String>(1)?).unwrap(),
                approver: row.get(2)?,
                approval: row.get(3)?,
                enveloped: row.get(4)?,
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
    })
}

/// The store's own list of override reasons, as a merchant would set it.
fn set_override_reasons(register: &Register, value_json: &str) {
    register.with_db(|conn| {
        conn.execute(
            "INSERT INTO setting (scope, scope_id, key, value_json)
             VALUES ('store', ?1, 'price.override_reasons', ?2)
             ON CONFLICT (scope, scope_id, key) DO UPDATE SET value_json = excluded.value_json",
            params![register.store().as_uuid().as_bytes().as_slice(), value_json],
        )
        .unwrap();
    });
}

#[test]
fn an_override_charges_its_price_for_a_listed_reason_and_is_audited() {
    let (_dir, register) = lead_shop();
    open_cart(&register).unwrap();
    let line = the_line(&add_product(&register, product_id(OIL), Some(2_000)).unwrap()).line_id;

    // Two damaged bottles at 1.000 each. The line keeps the 1.250 it was
    // captured at beside the price it now charges (I-5).
    let lowered = over(&register, line, 1_000, "damaged").unwrap();
    let oil = the_line(&lowered);
    assert_eq!(
        (
            oil.unit_price_minor,
            oil.captured_price_minor,
            oil.override_reason.as_deref(),
            oil.line_total_minor
        ),
        (1_000, 1_250, Some("damaged"), 2_000)
    );
    assert_eq!(lowered.total_minor, 2_000);
    // It is on disk as answered, and the till shows the same.
    let (state, saved_cart) = saved(&register).into_iter().next().unwrap();
    let saved_line = saved_cart.lines.first().unwrap();
    assert_eq!(
        (state.as_str(), saved_line.price(), saved_line.unit_price),
        ("active", jod(1_000), jod(1_250))
    );
    assert_eq!(current(&register).unwrap(), Some(lowered));

    // Audited, with its envelope: why, from what to what, and the floor it
    // was held to, the oil's cost of 0.100. No second person was asked.
    let rows = overrides(&register);
    let row = rows.first().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (
            row.reason.as_deref(),
            &row.approver,
            &row.approval,
            row.enveloped
        ),
        (Some("damaged"), &None, &None, true)
    );
    for (name, value) in [
        ("amount_minor", serde_json::json!(1_000)),
        ("captured_minor", serde_json::json!(1_250)),
        ("previous_minor", serde_json::json!(1_250)),
        ("floor_minor", serde_json::json!(100)),
        ("qty_milli", serde_json::json!(2_000)),
        ("displayed_price", serde_json::json!(false)),
    ] {
        assert_eq!(field(&row.payload, name), &value, "{name}");
    }

    // Back at the captured price the override is gone, and that is audited
    // too. Raising a price is held to no floor.
    let restored = over(&register, line, 1_250, "price_match").unwrap();
    let oil = the_line(&restored);
    assert_eq!(
        (oil.unit_price_minor, oil.override_reason.as_deref()),
        (1_250, None)
    );
    let rows = overrides(&register);
    let row = rows.last().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(row.reason.as_deref(), Some("price_match"));
    assert_eq!(
        field(&row.payload, "previous_minor"),
        &serde_json::json!(1_000)
    );
    assert_eq!(field(&row.payload, "floor_minor"), &serde_json::Value::Null);
}

#[test]
fn an_override_names_what_it_refuses_and_changes_nothing() {
    let (_dir, register) = lead_shop();
    let line = SaleLineId::from_uuid(Uuid::from_u128(0x11e));
    assert_eq!(
        refusal(over(&register, line, 1_000, "damaged")).code,
        "cart_not_open"
    );

    // The oil has a ministry ceiling of 1.500 and a cost of 0.100. The bread
    // has no recorded cost at all, and the cheese is on a price label.
    register.with_db(|conn| {
        conn.execute(
            "UPDATE product SET max_price_minor = 1500 WHERE id = ?1",
            [id(OIL).as_slice()],
        )
        .unwrap();
    });
    open_cart(&register).unwrap();
    let oil = the_line(&add_product(&register, product_id(OIL), None).unwrap()).line_id;
    add_product(&register, product_id(BREAD), None).unwrap();
    let basket = add_scan(
        &register,
        &label("22", CHEESE_PLU, 416),
        ClockConfidence::Trusted,
    )
    .unwrap();
    let line_of = |n: u128| {
        basket
            .lines
            .iter()
            .find(|line| line.product_id == Some(product_id(n)))
            .unwrap()
            .line_id
    };
    let (bread, cheese) = (line_of(BREAD), line_of(CHEESE));
    let before = saved(&register);

    for (line, price, reason, code) in [
        (line, 1_000, "damaged", "line_unknown"),
        (oil, 1_000, "damaged box", "override_reason_unknown"),
        (oil, 1_000, "loyal_customer", "override_reason_unknown"),
        (oil, 1_000, "", "override_reason_unknown"),
        (oil, 1_501, "price_match", "price_above_ceiling"),
        (oil, 1_501, "displayed_price", "price_above_ceiling"),
        (oil, 99, "damaged", "price_below_floor"),
        (oil, -1, "damaged", "price_negative"),
        (oil, 1_250, "displayed_price", "displayed_price_not_lower"),
        (oil, 1_300, "displayed_price", "displayed_price_not_lower"),
        (bread, 300, "damaged", "price_no_recorded_cost"),
        (
            cheese,
            400,
            "displayed_price",
            "price_label_not_overridable",
        ),
    ] {
        assert_eq!(
            refusal(over(&register, line, price, reason)).code,
            code,
            "{price} for {reason:?}"
        );
    }
    // A list the store cannot have meant is refused by name, never read as
    // the default.
    set_override_reasons(&register, "not json");
    let unreadable = refusal(over(&register, oil, 1_000, "damaged"));
    assert_eq!(
        (unreadable.code, unreadable.detail),
        (
            "setting_invalid",
            Some("the store's list of override reasons cannot be read")
        )
    );

    // Nothing was written, and the till holds what the file holds.
    assert_eq!(saved(&register), before);
    assert_eq!(current(&register).unwrap(), Some(basket));
    assert!(overrides(&register).is_empty());
}

#[test]
fn the_stores_list_decides_the_reasons_and_the_shelf_price_is_always_one() {
    let (_dir, register) = lead_shop();
    set_override_reasons(&register, r#"["loyalty_2026"]"#);
    open_cart(&register).unwrap();
    let oil = the_line(&add_product(&register, product_id(OIL), None).unwrap()).line_id;
    let bread = add_product(&register, product_id(BREAD), None)
        .unwrap()
        .lines
        .iter()
        .find(|line| line.product_id == Some(product_id(BREAD)))
        .unwrap()
        .line_id;

    // The store's list replaces the default, so `damaged` is not a reason
    // here, and its own code is.
    assert_eq!(
        refusal(over(&register, oil, 1_000, "damaged")).code,
        "override_reason_unknown"
    );
    over(&register, oil, 1_000, "loyalty_2026").unwrap();
    // A product with no recorded cost may be raised, never lowered.
    over(&register, bread, 400, "loyalty_2026").unwrap();

    // The shelf showed 0.050, below the oil's cost: the customer pays it
    // (J.3, E.70). No store's list can leave the shelf's price out, and the
    // floor does not hold it. Its label reprint has no queue before `0014`,
    // so the audit row is the record.
    let shelf = over(&register, oil, 50, "displayed_price").unwrap();
    let line = shelf.lines.iter().find(|line| line.line_id == oil).unwrap();
    assert_eq!(
        (line.unit_price_minor, line.override_reason.as_deref()),
        (50, Some("displayed_price"))
    );
    let rows = overrides(&register);
    let row = rows.last().unwrap();
    assert_eq!(row.reason.as_deref(), Some("displayed_price"));
    for (name, value) in [
        ("displayed_price", serde_json::json!(true)),
        ("floor_minor", serde_json::Value::Null),
        ("previous_minor", serde_json::json!(1_000)),
    ] {
        assert_eq!(field(&row.payload, name), &value, "{name}");
    }
    assert_eq!(rows.len(), 3);
}

#[test]
fn only_a_role_holding_price_override_may_override() {
    // A cashier does not hold `price.override` (§8.2's grid).
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let line = the_line(&add_product(&register, product_id(OIL), None).unwrap()).line_id;
    let before = saved(&register);
    assert_eq!(
        refusal(over(&register, line, 1_000, "damaged")).code,
        "permission_denied"
    );
    assert_eq!(saved(&register), before);
    assert!(overrides(&register).is_empty());

    // A manager does. The cashier's basket is set aside at sign-out, and the
    // manager overrides on their own basket.
    logout(&register).unwrap();
    add_staff(
        &register,
        MANAGER_ID,
        MANAGER_CODE,
        "Synthetic Manager",
        "manager",
    );
    login_pin(&register, MANAGER_CODE, CASHIER_PIN).unwrap();
    open_cart(&register).unwrap();
    let line = the_line(&add_product(&register, product_id(OIL), None).unwrap()).line_id;
    assert_eq!(
        the_line(&over(&register, line, 1_000, "damaged").unwrap()).unit_price_minor,
        1_000
    );
    assert_eq!(overrides(&register).len(), 1);
}

/// A manager's approval of the shift lead's override of `line` to
/// `amount_minor` for `reason`, stored as the approval screen will store
/// one, behind its envelope.
fn override_approval(
    register: &Register,
    line: SaleLineId,
    amount_minor: i64,
    reason: &str,
) -> ApprovalId {
    let id = ApprovalId::from_uuid(register.next_id());
    let binding = ApprovalBinding {
        entity_id: line.as_uuid(),
        amount_minor,
        content_hash: None,
    };
    let manager = pos_domain::authorize::<cap::PriceOverride>(
        UserId::from_uuid(Uuid::from_bytes(MANAGER_ID)),
        &GrantSet::of_role(Role::Manager),
        None,
        &binding,
        &EscalationPolicy::empty(),
        at(),
    )
    .unwrap();
    // The nonce is derived from a fresh id, never written as a literal.
    let nonce = *register.next_id().as_bytes();
    let handle = ApprovalHandle::issue::<cap::PriceOverride>(
        id,
        UserId::from_uuid(Uuid::from_bytes(LEAD_ID)),
        &manager,
        &binding,
        reason.to_owned(),
        at(),
        60_000,
        nonce,
    )
    .unwrap();
    register.with_db(|conn| {
        let tx = conn.transaction().unwrap();
        let (change, commit) = (register.next_id(), register.next_id());
        OutboxRepository::new(&tx)
            .write_commit(
                &tx,
                &CommitEnvelope {
                    commit_id: commit.as_bytes(),
                    protocol_version: 1,
                    schema_version: pos_db::SCHEMA_VERSION,
                    producer_version: "cart-suite",
                    created_at: AT,
                },
                &[FactMember {
                    change_id: change.as_bytes(),
                    entity: "approval_handle",
                    entity_id: id.as_uuid().as_bytes(),
                    payload: "{}",
                }],
            )
            .unwrap();
        ApprovalRepository::new(&tx).insert(&tx, &handle).unwrap();
        tx.commit().unwrap();
    });
    id
}

/// The consumption that spent `approval`, as `(its effect, its audit row,
/// whether its envelope holds it)`.
fn consumption(register: &Register, approval: ApprovalId) -> Option<(Vec<u8>, Vec<u8>, bool)> {
    register.with_db(|conn| {
        conn.query_row(
            "SELECT c.effect_id, c.audit_log_id,
                    EXISTS (SELECT 1 FROM fact_commit_member m
                             WHERE m.entity = 'approval_consumption' AND m.entity_id = c.handle_id)
               FROM approval_consumption c WHERE c.handle_id = ?1",
            [approval.as_uuid().as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .unwrap()
    })
}

#[test]
fn an_escalated_override_spends_one_bound_approval_with_its_audit_row() {
    let (_dir, register) = lead_shop();
    add_staff(
        &register,
        MANAGER_ID,
        MANAGER_CODE,
        "Synthetic Manager",
        "manager",
    );
    open_cart(&register).unwrap();
    let line = the_line(&add_product(&register, product_id(OIL), None).unwrap()).line_id;
    let escalating = EscalationPolicy::empty().with_escalation::<cap::PriceOverride>();
    let with =
        |price: i64, reason: &str, approval: Option<ApprovalId>, policy: &EscalationPolicy| {
            override_line_price(
                &register,
                &PriceChange {
                    line_id: line,
                    unit_price_minor: price,
                    reason,
                    approval,
                },
                policy,
            )
        };
    let before = saved(&register);

    // Where the merchant's policy escalates `price.override`, the shift lead
    // needs a manager's approval of this line at this price.
    assert_eq!(
        refusal(with(1_000, "damaged", None, &escalating)).code,
        "approval_required"
    );
    // One for another price, or given for another reason, is refused, and
    // stays unspent. The reason is held by the schema's trigger as the
    // approval is spent, so the transaction that would have saved the cart
    // and written the audit row takes all of it back.
    let other_price = override_approval(&register, line, 900, "damaged");
    let mismatch = refusal(with(1_000, "damaged", Some(other_price), &escalating));
    assert_eq!(
        (mismatch.code, mismatch.detail),
        (
            "approval_mismatch",
            Some("the approval is for another amount")
        )
    );
    let other_reason = override_approval(&register, line, 1_000, "price_match");
    let mismatch = refusal(with(1_000, "damaged", Some(other_reason), &escalating));
    assert_eq!(
        (mismatch.code, mismatch.detail),
        (
            "approval_mismatch",
            Some("the approval was given for another reason")
        )
    );
    assert_eq!(saved(&register), before);
    assert!(overrides(&register).is_empty());
    assert_eq!(consumption(&register, other_price), None);
    assert_eq!(consumption(&register, other_reason), None);

    // Bound to this line, this price and this reason, it passes. The audit
    // row names the manager and the approval, and the consumption names the
    // line as the effect and that row, inside the same envelope.
    let bound = override_approval(&register, line, 1_000, "damaged");
    let answer = with(1_000, "damaged", Some(bound), &escalating).unwrap();
    assert_eq!(the_line(&answer).unit_price_minor, 1_000);
    let rows = overrides(&register);
    let row = rows.first().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (row.approver.clone(), row.approval.clone(), row.enveloped),
        (
            Some(MANAGER_ID.to_vec()),
            Some(bound.as_uuid().as_bytes().to_vec()),
            true
        )
    );
    let audit_id: Vec<u8> = register.with_db(|conn| {
        conn.query_row(
            "SELECT id FROM audit_log WHERE action = 'price.override'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    });
    assert_eq!(
        consumption(&register, bound),
        Some((line.as_uuid().as_bytes().to_vec(), audit_id, true))
    );

    // It is spent: the same approval cannot carry a second override.
    assert_eq!(
        refusal(with(1_000, "damaged", Some(bound), &escalating)).code,
        "approval_used"
    );
    // And where the policy escalates nothing, an approval offered is not
    // used, so it stays unspent for nothing.
    let unasked = override_approval(&register, line, 1_100, "damaged");
    let alone = with(1_100, "damaged", Some(unasked), &merchant_policy()).unwrap();
    assert_eq!(the_line(&alone).unit_price_minor, 1_100);
    assert_eq!(overrides(&register).last().unwrap().approval, None);
    assert_eq!(consumption(&register, unasked), None);
}
