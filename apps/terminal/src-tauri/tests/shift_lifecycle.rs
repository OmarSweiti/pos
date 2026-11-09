#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A register's shift, through the shell (microstep 1.9.5, its terminal
//! half): `shift_open`, `shift_current` and `shift_close` on a register file
//! built through `pos_db::open`, the registered migration chain, with no shift
//! open until a cashier opens one.
//!
//! The register's clock is one the suite can move, so one register can open a
//! shift, sell in it, close it and open the next. A crash is a register
//! dropped without signing out, and the next one opened on the same file.

mod shop;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use pos_domain::{Clock, ClockConfidence, SaleId, ShiftId, Timestamp, UserId};
use rusqlite::params;
use serde_json::json;
use shop::{
    CASHIER_CODE, CASHIER_ID, CASHIER_PIN, KEY, LEAD_CODE, LEAD_ID, OIL, SECOND_CODE, SECOND_ID,
    add_staff, audited, field, product_id, refusal, seed, states,
};
use terminal_lib::commands::auth::{login_pin, logout};
use terminal_lib::commands::cart::{add_product, list_parked, open_cart, resume_cart};
use terminal_lib::commands::sale::{CompletedSaleRef, finalize};
use terminal_lib::commands::shift::{
    DenominationCount, ShiftSnapshot, begin_shift, current_shift, end_shift,
};
use terminal_lib::commands::tender::{add_cash, begin_payment, in_flight, resume_journalled};
use terminal_lib::state::{Register, V7Ids};
use uuid::Uuid;

/// A clock the suite moves. Every register opened on it reads the same hands.
#[derive(Clone)]
struct Hands(Arc<AtomicI64>);

impl Hands {
    fn at(iso: &str) -> Hands {
        let hands = Hands(Arc::new(AtomicI64::new(0)));
        hands.set(iso);
        hands
    }

    fn set(&self, iso: &str) {
        let at = Timestamp::parse_iso8601(iso).unwrap();
        self.0.store(at.epoch_milliseconds(), Ordering::SeqCst);
    }
}

impl Clock for Hands {
    fn now(&self) -> Timestamp {
        Timestamp::from_epoch_milliseconds(self.0.load(Ordering::SeqCst)).unwrap()
    }
}

/// 12:00 in Amman on 5 November 2026.
const NOON: &str = "2026-11-05T09:00:00.000Z";
const DAY: &str = "2026-11-05";
const NEXT_DAY: &str = "2026-11-06";

fn register_on(dir: &Path, hands: &Hands) -> Register {
    Register::open_with(
        dir,
        KEY,
        Box::new(hands.clone()),
        Box::new(V7Ids::default()),
    )
    .unwrap()
}

/// The shop with no shift open, its clock at noon, and its cashier signed in.
fn till() -> (tempfile::TempDir, Hands, Register) {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), false);
    let hands = Hands::at(NOON);
    let register = register_on(dir.path(), &hands);
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    (dir, hands, register)
}

fn piece(denomination_minor: i64, count: u32) -> DenominationCount {
    DenominationCount {
        denomination_minor,
        count,
    }
}

/// Two ten-dinar notes, a five, four half dinars and five 100-fils coins:
/// 27.500.
fn float() -> Vec<DenominationCount> {
    vec![
        piece(10_000, 2),
        piece(5_000, 1),
        piece(500, 4),
        piece(100, 5),
    ]
}

const FLOAT_MINOR: i64 = 27_500;

/// A shift opened on `date`, which the operator confirms: the register
/// trusts no clock yet, so every opening in the shell asks for one.
fn confirmed(register: &Register, date: &str) -> ShiftSnapshot {
    begin_shift(register, register.clock_confidence(), &float(), Some(date)).unwrap()
}

/// One bottle of oil, paid for in cash and completed.
fn sell_one(register: &Register) -> CompletedSaleRef {
    open_cart(register).unwrap();
    add_product(register, product_id(OIL), None).unwrap();
    let paying = begin_payment(register).unwrap();
    add_cash(register, paying.cash_due_minor).unwrap();
    finalize(register).unwrap()
}

/// A completed sale's business date and its shift, as stored.
fn dated(register: &Register, sale: SaleId) -> (String, ShiftId) {
    register.with_db(|conn| {
        conn.query_row(
            "SELECT business_date, shift_id FROM sale WHERE id = ?1",
            [sale.as_uuid().as_bytes().as_slice()],
            |row| {
                let shift: Vec<u8> = row.get(1)?;
                Ok((
                    row.get(0)?,
                    ShiftId::from_uuid(Uuid::from_slice(&shift).unwrap()),
                ))
            },
        )
        .unwrap()
    })
}

/// The opening fact's row, column for column.
fn opening_row(register: &Register, shift: ShiftId) -> (String, Vec<u8>, String, i64) {
    register.with_db(|conn| {
        conn.query_row(
            "SELECT business_date, opened_by, opened_at, float_minor FROM shift WHERE id = ?1",
            [shift.as_uuid().as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap()
    })
}

/// How many close facts name `shift`.
fn closes_of(register: &Register, shift: ShiftId) -> i64 {
    register.with_db(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM shift_close_event WHERE shift_id = ?1",
            [shift.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap()
    })
}

/// The close fact naming `shift`: its id, and the commit it says it travels
/// in.
fn close_of(register: &Register, shift: ShiftId) -> (Uuid, Uuid) {
    register.with_db(|conn| {
        conn.query_row(
            "SELECT id, sync_commit_id FROM shift_close_event WHERE shift_id = ?1",
            [shift.as_uuid().as_bytes().as_slice()],
            |row| {
                let (id, commit): (Vec<u8>, Vec<u8>) = (row.get(0)?, row.get(1)?);
                Ok((
                    Uuid::from_slice(&id).unwrap(),
                    Uuid::from_slice(&commit).unwrap(),
                ))
            },
        )
        .unwrap()
    })
}

/// Every member of the envelope that carries `entity` `id`, as `(entity,
/// the audit row's action when it is one)`, and that envelope's id.
fn envelope_of(
    register: &Register,
    entity: &str,
    id: Uuid,
) -> (Uuid, Vec<(String, Option<String>)>) {
    register.with_db(|conn| {
        let commit: Vec<u8> = conn
            .query_row(
                "SELECT commit_id FROM fact_commit_member WHERE entity = ?1 AND entity_id = ?2",
                params![entity, id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        let members = conn
            .prepare(
                "SELECT m.entity, a.action
                   FROM fact_commit_member m
                   LEFT JOIN audit_log a ON m.entity = 'audit_log' AND a.id = m.entity_id
                  WHERE m.commit_id = ?1
                  ORDER BY m.rowid",
            )
            .unwrap()
            .query_map([&commit], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        (Uuid::from_slice(&commit).unwrap(), members)
    })
}

/// How many shifts the register has opened, and how many audit rows have
/// been written.
fn written(register: &Register) -> (i64, i64) {
    register.with_db(|conn| {
        conn.query_row(
            "SELECT (SELECT COUNT(*) FROM shift), (SELECT COUNT(*) FROM audit_log)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
    })
}

fn user(id: [u8; 16]) -> UserId {
    UserId::from_uuid(Uuid::from_bytes(id))
}

fn sale_text(sale: SaleId) -> serde_json::Value {
    json!(sale.as_uuid().to_string())
}

/// 1.9.5's `Done when`, through the shell: a shift opens with a counted
/// float, a sale completes in it on its business date, the shift closes with
/// a fact of its own, the opening untouched, and the next shift opens on the
/// same register. Each opening and the close travel in one envelope with
/// their audit row.
#[test]
fn a_shift_opens_sells_closes_and_the_next_opens_on_the_same_register() {
    let (_dir, hands, register) = till();
    assert_eq!(current_shift(&register).unwrap(), None);
    assert_eq!(refusal(open_cart(&register)).code, "shift_not_open");

    let first = confirmed(&register, DAY);
    assert_eq!(first.business_date, DAY);
    assert_eq!(first.opened_by, user(CASHIER_ID));
    assert_eq!(first.opened_at, Timestamp::parse_iso8601(NOON).unwrap());
    assert_eq!((first.currency, first.float_minor), ("JOD", FLOAT_MINOR));
    assert_eq!(current_shift(&register).unwrap(), Some(first.clone()));
    let opened = opening_row(&register, first.shift_id);

    let sale = sell_one(&register);
    assert_eq!(sale.receipt_number, "REG01-000001");
    assert_eq!(
        dated(&register, sale.sale_id),
        (DAY.to_owned(), first.shift_id)
    );

    hands.set("2026-11-05T18:00:00.000Z");
    let closed = end_shift(&register, first.shift_id).unwrap();
    assert_eq!(closed.shift_id, first.shift_id);
    assert_eq!(closed.business_date, DAY);
    assert_eq!(
        closed.closed_at,
        Timestamp::parse_iso8601("2026-11-05T18:00:00.000Z").unwrap()
    );
    assert_eq!(closed.parked_cart, None, "the till was empty");
    assert_eq!(closes_of(&register, first.shift_id), 1);
    assert_eq!(
        opening_row(&register, first.shift_id),
        opened,
        "closing appends a fact and never changes the opening"
    );
    assert_eq!(current_shift(&register).unwrap(), None);
    assert_eq!(refusal(open_cart(&register)).code, "shift_not_open");

    hands.set("2026-11-06T06:00:00.000Z");
    let second = confirmed(&register, NEXT_DAY);
    assert_ne!(second.shift_id, first.shift_id);
    assert_eq!(second.business_date, NEXT_DAY);
    assert_eq!(current_shift(&register).unwrap(), Some(second.clone()));
    let next = sell_one(&register);
    assert_eq!(next.receipt_number, "REG01-000002");
    assert_eq!(
        dated(&register, next.sale_id),
        (NEXT_DAY.to_owned(), second.shift_id)
    );

    // Each fact travels with its audit row and nothing else; the close names
    // the commit it travels in.
    for shift in [first.shift_id, second.shift_id] {
        let (_, members) = envelope_of(&register, "shift", shift.as_uuid());
        assert_eq!(
            members,
            vec![
                ("shift".to_owned(), None),
                ("audit_log".to_owned(), Some("shift.open".to_owned())),
            ]
        );
    }
    let (close, commit) = close_of(&register, first.shift_id);
    assert_eq!(close, closed.close_id);
    let (carried_in, members) = envelope_of(&register, "shift_close_event", close);
    assert_eq!(carried_in, commit);
    assert_eq!(
        members,
        vec![
            ("shift_close_event".to_owned(), None),
            ("audit_log".to_owned(), Some("shift.close".to_owned())),
        ]
    );
    let closes = audited(&register, "shift.close");
    assert_eq!(closes.len(), 1);
    let close_row = closes.first().unwrap();
    assert_eq!(field(close_row, "business_date"), &json!(DAY));
    assert_eq!(
        field(close_row, "shift_id"),
        &json!(first.shift_id.as_uuid().to_string())
    );
    assert_eq!(field(close_row, "parked_cart"), &json!(null));
    assert_eq!(audited(&register, "shift.open").len(), 2);
}

/// The register trusts no clock yet, so every opening in the shell needs the
/// operator to confirm its business date. None is refused, and so is a date
/// that is not one. A confirmed date is the shift's, whatever the clock
/// reads, and its audit row says the operator chose it under an untrusted
/// clock.
#[test]
fn an_untrusted_clock_opens_a_shift_only_on_a_date_the_operator_confirms() {
    let (_dir, _hands, register) = till();
    assert_eq!(register.clock_confidence(), ClockConfidence::Untrusted);
    let untrusted = register.clock_confidence();
    let before = written(&register);

    let none = refusal(begin_shift(&register, untrusted, &float(), None));
    assert_eq!(none.code, "business_date_required");
    for not_a_date in ["05/11/2026", "2026-11-31", ""] {
        let refused = refusal(begin_shift(
            &register,
            untrusted,
            &float(),
            Some(not_a_date),
        ));
        assert_eq!(refused.code, "business_date_invalid", "{not_a_date:?}");
    }
    assert_eq!(
        written(&register),
        before,
        "a refused opening writes no shift and no audit row"
    );

    // The clock reads the 5th in Amman; the operator says the 4th.
    let opened = begin_shift(&register, untrusted, &float(), Some("2026-11-04")).unwrap();
    assert_eq!(opened.business_date, "2026-11-04");
    let rows = audited(&register, "shift.open");
    let row = rows.first().unwrap();
    assert_eq!(field(row, "business_date"), &json!("2026-11-04"));
    assert_eq!(
        field(row, "business_date_source"),
        &json!("operator_confirmed")
    );
    assert_eq!(field(row, "clock_confidence"), &json!("untrusted"));
    assert_eq!(field(row, "float_minor"), &json!(FLOAT_MINOR));
    let reason: Option<String> = register.with_db(|conn| {
        conn.query_row(
            "SELECT reason FROM audit_log WHERE action = 'shift.open'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    });
    assert_eq!(
        reason.as_deref(),
        Some("business date confirmed by the operator")
    );
}

/// Under a trusted clock the shell dates the shift itself, from the instant
/// and the store's own zone and cutover as its row holds them, and a date the
/// caller supplies is refused, even the right one. A zone the time-zone
/// database does not know dates nothing.
#[test]
fn a_trusted_clock_dates_the_shift_by_the_stores_own_day() {
    let (_dir, hands, register) = till();
    let trusted = ClockConfidence::Trusted;
    let open_and_close = |expected: &str| {
        let opened = begin_shift(&register, trusted, &float(), None).unwrap();
        assert_eq!(opened.business_date, expected);
        end_shift(&register, opened.shift_id).unwrap();
    };
    let store = |assignments: &str| {
        register.with_db(|conn| {
            conn.execute(&format!("UPDATE store SET {assignments}"), [])
                .unwrap()
        });
    };

    // 03:30 in Amman, before the 04:00 cutover: still the 4th's trading day.
    hands.set("2026-11-05T00:30:00.000Z");
    let supplied = refusal(begin_shift(
        &register,
        trusted,
        &float(),
        Some("2026-11-04"),
    ));
    assert_eq!(supplied.code, "business_date_not_accepted");
    open_and_close("2026-11-04");
    let rows = audited(&register, "shift.open");
    let row = rows.first().unwrap();
    assert_eq!(field(row, "business_date_source"), &json!("derived"));
    assert_eq!(field(row, "clock_confidence"), &json!("trusted"));

    // A store whose day turns at midnight is on the 5th by then.
    store("day_cutover_minutes = 0");
    open_and_close(DAY);

    // 15:00 UTC is 05:00 on the 6th at UTC+14, past its 04:00 cutover.
    hands.set("2026-11-05T15:00:00.000Z");
    store("time_zone = 'Pacific/Kiritimati', day_cutover_minutes = 240");
    open_and_close(NEXT_DAY);

    store("time_zone = 'Mars/Olympus_Mons'");
    let unknown = refusal(begin_shift(&register, trusted, &float(), None));
    assert_eq!(unknown.code, "store_day_unreadable");
}

/// A float is counted in the dinar's own pieces, and only the total is the
/// shift's. A piece the dinar does not have is refused by name, and nothing
/// is written. A drawer may open empty.
#[test]
fn the_float_is_counted_in_the_dinars_own_pieces() {
    let (_dir, hands, register) = till();
    let untrusted = register.clock_confidence();
    let before = written(&register);
    for not_a_piece in [piece(300, 1), piece(-500, 1), piece(0, 3)] {
        let counted = [piece(10_000, 1), not_a_piece];
        let refused = refusal(begin_shift(&register, untrusted, &counted, Some(DAY)));
        assert_eq!(refused.code, "float_not_a_denomination");
    }
    assert_eq!(written(&register), before, "no shift and no audit row");

    let every_piece = [
        piece(50_000, 1),
        piece(20_000, 1),
        piece(10_000, 1),
        piece(5_000, 1),
        piece(1_000, 1),
        piece(500, 1),
        piece(250, 1),
        piece(100, 1),
        piece(50, 1),
        piece(25, 1),
        piece(10, 1),
    ];
    let opened = begin_shift(&register, untrusted, &every_piece, Some(DAY)).unwrap();
    assert_eq!(opened.float_minor, 86_935);
    end_shift(&register, opened.shift_id).unwrap();

    hands.set("2026-11-05T10:00:00.000Z");
    let empty = begin_shift(&register, untrusted, &[], Some(DAY)).unwrap();
    assert_eq!(empty.float_minor, 0);
}

/// A register holds one open shift. A second opening is refused by name,
/// whoever asks, and writes nothing.
#[test]
fn a_register_holds_one_open_shift() {
    let (_dir, _hands, register) = till();
    let open = confirmed(&register, DAY);
    let written_once = written(&register);
    let again = refusal(begin_shift(
        &register,
        register.clock_confidence(),
        &float(),
        Some(DAY),
    ));
    assert_eq!(again.code, "shift_already_open");

    add_staff(
        &register,
        SECOND_ID,
        SECOND_CODE,
        "Synthetic Second",
        "cashier",
    );
    logout(&register).unwrap();
    login_pin(&register, SECOND_CODE, CASHIER_PIN).unwrap();
    let theirs = refusal(begin_shift(
        &register,
        register.clock_confidence(),
        &float(),
        Some(DAY),
    ));
    assert_eq!(theirs.code, "shift_already_open");
    assert_eq!(written(&register).0, written_once.0);
    assert_eq!(current_shift(&register).unwrap(), Some(open));
}

/// Only the person who opened a shift closes it here, and with no approval:
/// another cashier and a shift lead are each refused, though both hold
/// `shift.close`. A shift that is not the register's open one is refused, and
/// a closed one cannot close again. The opener's close needs nothing more.
#[test]
fn only_the_opener_closes_the_shift_and_without_approval() {
    let (_dir, _hands, register) = till();
    let open = confirmed(&register, DAY);
    add_staff(
        &register,
        SECOND_ID,
        SECOND_CODE,
        "Synthetic Second",
        "cashier",
    );
    add_staff(
        &register,
        LEAD_ID,
        LEAD_CODE,
        "Synthetic Lead",
        "shift_lead",
    );

    for code in [SECOND_CODE, LEAD_CODE] {
        logout(&register).unwrap();
        login_pin(&register, code, CASHIER_PIN).unwrap();
        let refused = refusal(end_shift(&register, open.shift_id));
        assert_eq!(refused.code, "shift_not_yours", "{code}");
    }
    assert_eq!(closes_of(&register, open.shift_id), 0);

    logout(&register).unwrap();
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    let elsewhere = ShiftId::from_uuid(Uuid::from_u128(0xE15E));
    assert_eq!(
        refusal(end_shift(&register, elsewhere)).code,
        "shift_not_current"
    );
    end_shift(&register, open.shift_id).unwrap();
    assert_eq!(
        refusal(end_shift(&register, open.shift_id)).code,
        "shift_not_open"
    );
    assert_eq!(closes_of(&register, open.shift_id), 1);
}

/// A sale with money taken keeps its shift open, because once the shift
/// closed it could never complete. The refused close leaves the till paying,
/// and once the sale completes the shift closes.
#[test]
fn a_sale_being_paid_for_keeps_its_shift_open() {
    let (_dir, _hands, register) = till();
    let open = confirmed(&register, DAY);
    open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), None).unwrap();
    let paying = begin_payment(&register).unwrap();
    add_cash(&register, 1_000).unwrap();

    let refused = refusal(end_shift(&register, open.shift_id));
    assert_eq!(refused.code, "sale_in_flight");
    assert_eq!(closes_of(&register, open.shift_id), 0);
    assert_eq!(audited(&register, "shift.close").len(), 0);

    add_cash(&register, paying.cash_due_minor - 1_000).unwrap();
    finalize(&register).unwrap();
    end_shift(&register, open.shift_id).unwrap();
}

/// A sale a crash left mid-payment keeps its shift open too, on the next
/// process as on the one that took the money, until a cashier takes it up and
/// completes it.
#[test]
fn a_sale_a_crash_left_mid_payment_keeps_its_shift_open() {
    let (dir, hands, register) = till();
    let open = confirmed(&register, DAY);
    open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), None).unwrap();
    let paying = begin_payment(&register).unwrap();
    add_cash(&register, 1_000).unwrap();
    drop(register);

    let register = register_on(dir.path(), &hands);
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(in_flight(&register).unwrap().len(), 1);
    let refused = refusal(end_shift(&register, open.shift_id));
    assert_eq!(refused.code, "sale_in_flight");

    resume_journalled(&register, paying.sale_id).unwrap();
    add_cash(&register, paying.cash_due_minor - 1_000).unwrap();
    finalize(&register).unwrap();
    end_shift(&register, open.shift_id).unwrap();
}

/// Closing sets the till's basket aside, as a sign-out does: parked with its
/// lines, named in the close's audit row, and resumed in the next shift of
/// the same trading day. A basket being paid for with nothing taken goes back
/// to being a basket first, its checkout abandoned; one with no line is
/// dropped.
#[test]
fn closing_sets_the_tills_basket_aside_for_the_next_shift() {
    let (_dir, hands, register) = till();

    // A basket being built.
    let first = confirmed(&register, DAY);
    let basket = open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), None).unwrap();
    let closed = end_shift(&register, first.shift_id).unwrap();
    assert_eq!(closed.parked_cart, Some(basket.sale_id));
    assert_eq!(
        states(&register),
        vec![("parked".to_owned(), basket.sale_id)]
    );
    let rows = audited(&register, "shift.close");
    assert_eq!(
        field(rows.first().unwrap(), "parked_cart"),
        &json!(basket.sale_id.as_uuid().to_string())
    );

    // The next shift of the same day resumes it, and sells it.
    hands.set("2026-11-05T13:00:00.000Z");
    let second = confirmed(&register, DAY);
    let listed = list_parked(&register).unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|parked| parked.parked_id)
            .collect::<Vec<_>>(),
        vec![basket.sale_id]
    );
    resume_cart(&register, basket.sale_id).unwrap();
    let paying = begin_payment(&register).unwrap();
    add_cash(&register, paying.cash_due_minor).unwrap();
    let sold = finalize(&register).unwrap();
    assert_eq!(
        dated(&register, sold.sale_id),
        (DAY.to_owned(), second.shift_id)
    );

    // A basket being paid for, with nothing taken yet.
    let unpaid = open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), None).unwrap();
    begin_payment(&register).unwrap();
    let closed = end_shift(&register, second.shift_id).unwrap();
    assert_eq!(closed.parked_cart, Some(unpaid.sale_id));
    assert!(
        in_flight(&register).unwrap().is_empty(),
        "its checkout is abandoned"
    );
    let rows = audited(&register, "shift.close");
    assert_eq!(
        field(rows.get(1).unwrap(), "parked_cart"),
        &sale_text(unpaid.sale_id)
    );

    // A basket with no line is dropped, and the row says nothing was parked.
    hands.set("2026-11-05T15:00:00.000Z");
    let third = confirmed(&register, DAY);
    open_cart(&register).unwrap();
    let closed = end_shift(&register, third.shift_id).unwrap();
    assert_eq!(closed.parked_cart, None);
    let rows = audited(&register, "shift.close");
    assert_eq!(field(rows.get(2).unwrap(), "parked_cart"), &json!(null));
    assert_eq!(
        states(&register),
        vec![("parked".to_owned(), unpaid.sale_id)],
        "the sold basket is gone, the unpaid one parked, the empty one dropped"
    );
}

/// The lock screen names whose shift is open before anyone signs in, so
/// `shift_current` needs no session. Every other shift command does.
#[test]
fn shift_current_answers_before_anyone_signs_in() {
    let (_dir, _hands, register) = till();
    assert_eq!(current_shift(&register).unwrap(), None);
    let open = confirmed(&register, DAY);
    logout(&register).unwrap();
    assert_eq!(current_shift(&register).unwrap(), Some(open.clone()));

    let untrusted = register.clock_confidence();
    assert_eq!(
        refusal(begin_shift(&register, untrusted, &float(), Some(DAY))).code,
        "not_signed_in"
    );
    assert_eq!(
        refusal(end_shift(&register, open.shift_id)).code,
        "not_signed_in"
    );
}
