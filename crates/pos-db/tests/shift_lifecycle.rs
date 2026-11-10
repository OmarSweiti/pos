//! Microstep 1.9.5 — a shift opened with a counted float on a date the clock
//! or a person decided, one open per register, closed by its opener with a fact
//! of its own; and a sale only ever in the open shift, on its business date.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod sale_world;

use pos_db::DbError;
use pos_db::repo::audit::AuditRepository;
use pos_db::repo::shift::{self, ShiftRepository};
use pos_domain::{
    ApprovalBinding, AuditIntent, Authorized, BusinessDate, ClockConfidence, CompletedSale,
    Currency, DayBoundary, EscalationPolicy, GrantSet, Money, RegisterId, Role, Shift, ShiftClose,
    ShiftError, ShiftId, ShiftOpening, Timestamp, UserId, authorize, cap, count_float, open_shift,
    opening_business_date,
};
use rusqlite::params;
use sale_world::{
    AT, OTHER_REGISTER, REGISTER, SHIFT, World, jod, no_sabotage, tender, uuid, write_envelope,
};
use uuid::Uuid;

/// A second cashier, who did not open the shifts below.
const SECOND: u8 = 0x55;

fn world() -> World {
    let w = World::new();
    w.conn
        .execute(
            "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
             VALUES (?1, ?2, 'C-2', 'Second Cashier', 'placeholder-not-a-hash', ?3)",
            params![uuid(SECOND).as_bytes().as_slice(), &w.chain.org, AT],
        )
        .unwrap();
    w
}

fn instant(iso: &str) -> Timestamp {
    Timestamp::parse_iso8601(iso).unwrap()
}

/// Amman, UTC+3, with the store's default 04:00 cutover.
fn amman() -> DayBoundary {
    DayBoundary::new(180, 240).unwrap()
}

fn unbound() -> ApprovalBinding {
    ApprovalBinding {
        entity_id: Uuid::nil(),
        amount_minor: 0,
        content_hash: None,
    }
}

fn as_cashier<C: pos_domain::Capability>(who: UserId, at: Timestamp) -> Authorized<C> {
    authorize::<C>(
        who,
        &GrantSet::of_role(Role::Cashier),
        None,
        &unbound(),
        &EscalationPolicy::empty(),
        at,
    )
    .unwrap()
}

fn second() -> UserId {
    UserId::from_uuid(uuid(SECOND))
}

/// Two twenty-dinar notes and a ten: 50.000.
fn float() -> Money {
    count_float(Currency::JOD, &[(jod(20_000), 2), (jod(10_000), 1)]).unwrap()
}

/// `shift_open`, as its command will run: the date decided, the float
/// counted, the opening and its audit row under one envelope.
#[allow(clippy::too_many_arguments)]
fn open(
    w: &World,
    register: u8,
    id: u8,
    who: UserId,
    at: &str,
    confidence: ClockConfidence,
    supplied: Option<&str>,
) -> Result<(Shift, AuditIntent), DbError> {
    let at = instant(at);
    let date = opening_business_date(
        confidence,
        at,
        amman(),
        supplied.map(|d| BusinessDate::parse(d).unwrap()),
    )
    .unwrap();
    let (opened, audit) = open_shift(
        &as_cashier::<cap::ShiftOpen>(who, at),
        &ShiftOpening {
            id: ShiftId::from_uuid(uuid(id)),
            register_id: RegisterId::from_uuid(uuid(register)),
            store_id: w.store(),
            float: float(),
            date,
            confidence,
            opened_at: at,
        },
    );
    let audit_id = Uuid::from_bytes([id, 0xAD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    let tx = w.conn.unchecked_transaction().unwrap();
    write_envelope(
        &w.conn,
        &tx,
        id,
        &[
            (
                shift::ENTITY.to_owned(),
                *opened.id.as_uuid().as_bytes(),
                shift::opening_payload(&opened),
            ),
            (
                "audit_log".to_owned(),
                *audit_id.as_bytes(),
                "{}".to_owned(),
            ),
        ],
    );
    ShiftRepository::new(&w.conn).open(&tx, &opened)?;
    AuditRepository::new(&w.conn).append(&tx, opened.register_id, audit_id, &audit)?;
    tx.commit().unwrap();
    Ok((opened, audit))
}

/// `shift_close`, as its command will run: the close and its audit row under
/// one envelope, in commit `commit`.
fn close_as(
    w: &World,
    close: &ShiftClose,
    audit: &AuditIntent,
    register: RegisterId,
    commit: u8,
) -> Result<(), DbError> {
    let audit_id = Uuid::from_bytes([commit, 0xAD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]);
    let commit_id = Uuid::from_bytes([commit; 16]);
    let tx = w.conn.unchecked_transaction().unwrap();
    write_envelope(
        &w.conn,
        &tx,
        commit,
        &[
            (
                shift::CLOSE_ENTITY.to_owned(),
                *close.id.as_bytes(),
                shift::close_payload(close, commit_id),
            ),
            (
                "audit_log".to_owned(),
                *audit_id.as_bytes(),
                "{}".to_owned(),
            ),
        ],
    );
    ShiftRepository::new(&w.conn).close(&tx, close, commit_id)?;
    AuditRepository::new(&w.conn).append(&tx, register, audit_id, audit)?;
    tx.commit().unwrap();
    Ok(())
}

fn close(w: &World, shift: &Shift, who: UserId, at: &str, commit: u8) -> Result<(), DbError> {
    let at = instant(at);
    let (close, audit) = shift
        .close(
            &as_cashier::<cap::ShiftClose>(who, at),
            Uuid::from_bytes([commit, 0xC1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
            at,
        )
        .unwrap();
    close_as(w, &close, &audit, shift.register_id, commit)
}

fn audit_of(w: &World, action: &str) -> (Vec<u8>, Option<String>, serde_json::Value) {
    w.conn
        .query_row(
            "SELECT actor_id, reason, payload FROM audit_log WHERE action = ?1 ORDER BY seq DESC LIMIT 1",
            [action],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    serde_json::from_str(&r.get::<_, String>(2)?).unwrap(),
                ))
            },
        )
        .unwrap()
}

fn field(payload: &serde_json::Value, key: &str) -> serde_json::Value {
    payload.get(key).cloned().unwrap()
}

#[test]
fn shift_open_records_float_and_business_date() {
    let w = world();
    // 01:30 in Amman on the 26th is before the 04:00 cutover: the 25th's trading day.
    let (opened, _) = open(
        &w,
        OTHER_REGISTER,
        0x60,
        w.cashier(),
        "2026-08-25T22:30:00.000Z",
        ClockConfidence::Trusted,
        None,
    )
    .unwrap();
    let stored: (String, i64, Vec<u8>, String) = w
        .conn
        .query_row(
            "SELECT business_date, float_minor, opened_by, opened_at FROM shift WHERE id = ?1",
            [uuid(0x60).as_bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        stored,
        (
            "2026-08-25".to_owned(),
            50_000,
            w.cashier().as_uuid().as_bytes().to_vec(),
            "2026-08-25T22:30:00.000Z".to_owned()
        )
    );
    let shifts = ShiftRepository::new(&w.conn);
    assert_eq!(
        shifts
            .current(RegisterId::from_uuid(uuid(OTHER_REGISTER)))
            .unwrap(),
        Some(opened.clone())
    );
    assert_eq!(shifts.by_id(opened.id).unwrap(), Some(opened));
    let (actor, reason, payload) = audit_of(&w, "shift.open");
    assert_eq!(actor, w.cashier().as_uuid().as_bytes().to_vec());
    assert_eq!(reason, None);
    assert_eq!(field(&payload, "business_date_source"), "derived");
    assert_eq!(field(&payload, "float_minor"), 50_000);
}

#[test]
fn a_second_open_shift_on_a_register_is_refused_by_name() {
    let w = world();
    let open_now = |id| {
        open(
            &w,
            OTHER_REGISTER,
            id,
            w.cashier(),
            "2026-08-25T06:00:00.000Z",
            ClockConfidence::Trusted,
            None,
        )
    };
    open_now(0x60).unwrap();
    assert!(matches!(
        open_now(0x61),
        Err(DbError::ShiftAlreadyOpen { open, .. }) if open == ShiftId::from_uuid(uuid(0x60))
    ));
    // The refused attempt's envelope was written in its transaction and went
    // with it; nothing of the second shift stays.
    assert_eq!(
        w.conn
            .query_row(
                "SELECT count(*) FROM sync_commit WHERE id = ?1",
                [[0x61_u8; 16].as_slice()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        w.conn
            .query_row(
                "SELECT count(*) FROM shift WHERE id = ?1",
                [uuid(0x61).as_bytes().as_slice()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn untrusted_clock_requires_and_audits_a_confirmed_business_date() {
    let w = world();
    // No date can be derived, so none is: the opening waits for a person.
    for confidence in [
        ClockConfidence::Untrusted,
        ClockConfidence::Suspect { skew_ms: 7_200_000 },
    ] {
        assert_eq!(
            opening_business_date(
                confidence,
                instant("2026-08-25T06:00:00.000Z"),
                amman(),
                None
            ),
            Err(ShiftError::ConfirmedDateRequired),
            "{confidence:?}"
        );
    }

    let (opened, _) = open(
        &w,
        OTHER_REGISTER,
        0x60,
        w.cashier(),
        "2026-08-25T06:00:00.000Z",
        ClockConfidence::Untrusted,
        Some("2026-08-25"),
    )
    .unwrap();
    assert_eq!(
        opened.business_date,
        BusinessDate::parse("2026-08-25").unwrap()
    );
    let (_, reason, payload) = audit_of(&w, "shift.open");
    assert_eq!(
        reason.as_deref(),
        Some("business date confirmed by the operator")
    );
    assert_eq!(
        field(&payload, "business_date_source"),
        "operator_confirmed"
    );
    assert_eq!(field(&payload, "clock_confidence"), "untrusted");
    assert_eq!(field(&payload, "business_date"), "2026-08-25");
}

#[test]
fn a_user_can_close_the_shift_they_opened_without_approval() {
    let w = world();
    let register = RegisterId::from_uuid(uuid(OTHER_REGISTER));
    let (opened, _) = open(
        &w,
        OTHER_REGISTER,
        0x60,
        w.cashier(),
        "2026-08-25T06:00:00.000Z",
        ClockConfidence::Trusted,
        None,
    )
    .unwrap();
    close(&w, &opened, w.cashier(), "2026-08-25T18:00:00.000Z", 0x70).unwrap();

    let shifts = ShiftRepository::new(&w.conn);
    assert_eq!(shifts.current(register).unwrap(), None);
    assert!(!shifts.is_open(opened.id).unwrap());
    let (actor, reason, payload) = audit_of(&w, "shift.close");
    assert_eq!(actor, w.cashier().as_uuid().as_bytes().to_vec());
    assert_eq!(reason, None);
    assert_eq!(field(&payload, "business_date"), "2026-08-25");
    assert_eq!(
        w.conn
            .query_row("SELECT count(*) FROM approval_consumption", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0,
        "closing one's own shift spends no approval"
    );

    // The next shift opens on the same register.
    let (next, _) = open(
        &w,
        OTHER_REGISTER,
        0x61,
        w.cashier(),
        "2026-08-26T06:00:00.000Z",
        ClockConfidence::Trusted,
        None,
    )
    .unwrap();
    assert_eq!(
        next.business_date,
        BusinessDate::parse("2026-08-26").unwrap()
    );
    assert_eq!(shifts.current(register).unwrap(), Some(next));
}

#[test]
fn a_user_cannot_close_another_users_shift() {
    let w = world();
    let (opened, _) = open(
        &w,
        OTHER_REGISTER,
        0x60,
        w.cashier(),
        "2026-08-25T06:00:00.000Z",
        ClockConfidence::Trusted,
        None,
    )
    .unwrap();
    let at = instant("2026-08-25T18:00:00.000Z");

    // The domain refuses the second cashier.
    assert_eq!(
        opened.close(
            &as_cashier::<cap::ShiftClose>(second(), at),
            Uuid::from_u128(0xC0),
            at
        ),
        Err(ShiftError::NotTheOpener {
            opened_by: w.cashier(),
            actor: second()
        })
    );
    // And the repository refuses it again, should a caller skip the domain.
    let forged = ShiftClose {
        id: Uuid::from_u128(0xC0),
        shift_id: opened.id,
        closed_by: second(),
        closed_at: at,
    };
    let (_, audit) = opened
        .close(
            &as_cashier::<cap::ShiftClose>(w.cashier(), at),
            forged.id,
            at,
        )
        .unwrap();
    assert!(matches!(
        close_as(&w, &forged, &audit, opened.register_id, 0x70),
        Err(DbError::ShiftClosedByAnother { opened_by, closed_by })
            if opened_by == w.cashier() && closed_by == second()
    ));
    assert!(ShiftRepository::new(&w.conn).is_open(opened.id).unwrap());
}

#[test]
fn shift_close_appends_an_event_without_mutating_the_open_fact() {
    let w = world();
    let (opened, _) = open(
        &w,
        OTHER_REGISTER,
        0x60,
        w.cashier(),
        "2026-08-25T06:00:00.000Z",
        ClockConfidence::Trusted,
        None,
    )
    .unwrap();
    let row = |w: &World| -> Vec<rusqlite::types::Value> {
        let mut statement = w.conn.prepare("SELECT * FROM shift WHERE id = ?1").unwrap();
        let width = statement.column_count();
        statement
            .query_row([uuid(0x60).as_bytes().as_slice()], |r| {
                (0..width).map(|i| r.get(i)).collect()
            })
            .unwrap()
    };
    let before = row(&w);
    close(&w, &opened, w.cashier(), "2026-08-25T18:00:00.000Z", 0x70).unwrap();
    assert_eq!(row(&w), before, "the opening fact is never written again");

    let (state, event, closed_at): (String, Vec<u8>, String) = w
        .conn
        .query_row(
            "SELECT ss.state, ss.last_close_event_id, e.closed_at
               FROM shift_state ss JOIN shift_close_event e ON e.id = ss.last_close_event_id
              WHERE ss.shift_id = ?1",
            [uuid(0x60).as_bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(state, "closed");
    assert_eq!(event.first(), Some(&0x70));
    assert_eq!(closed_at, "2026-08-25T18:00:00.000Z");
    assert!(matches!(
        close(&w, &opened, w.cashier(), "2026-08-25T19:00:00.000Z", 0x71),
        Err(DbError::ShiftNotOpen(id)) if id == opened.id
    ));
}

/// A plain one-item sale in `shift`, on the fixture's business date.
fn sale_in(w: &World, shift: ShiftId, sale: u8, receipt: &str) -> CompletedSale {
    let mut cart = w.cart(sale);
    cart.shift_id = shift;
    cart.lines = vec![w.line(sale.wrapping_add(1), w.item, 1_000, 1_000)];
    w.complete(
        cart,
        vec![tender(sale.wrapping_add(2), "card", 1_000)],
        0,
        receipt,
    )
}

fn fixture_shift(w: &World) -> Shift {
    ShiftRepository::new(&w.conn)
        .by_id(ShiftId::from_uuid(uuid(SHIFT)))
        .unwrap()
        .unwrap()
}

#[test]
fn sale_without_an_open_shift_is_refused() {
    let w = world();
    // The register's shift is closed, by the cashier who opened it.
    close(
        &w,
        &fixture_shift(&w),
        w.cashier(),
        "2026-08-25T18:00:00.000Z",
        0x70,
    )
    .unwrap();
    let refused = w.finalize(&w.plain(0x20, "REG01-000001"), 0x02, no_sabotage, false);
    assert!(
        matches!(&refused, Err(DbError::Sqlite(e))
            if e.to_string() == "a completed sale requires the open shift for its register, store and business date"),
        "{refused:?}"
    );
    assert_eq!(
        w.conn
            .query_row("SELECT count(*) FROM sale", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn a_sale_uses_its_shifts_business_date() {
    let w = world();
    close(
        &w,
        &fixture_shift(&w),
        w.cashier(),
        "2026-08-25T10:30:00.000Z",
        0x70,
    )
    .unwrap();

    // A shift on the 25th takes a sale dated the 25th.
    let (on_the_25th, _) = open(
        &w,
        REGISTER,
        0x60,
        w.cashier(),
        "2026-08-25T11:00:00.000Z",
        ClockConfidence::Trusted,
        None,
    )
    .unwrap();
    w.finalize(
        &sale_in(&w, on_the_25th.id, 0x20, "REG01-000001"),
        0x02,
        no_sabotage,
        false,
    )
    .unwrap();
    close(
        &w,
        &on_the_25th,
        w.cashier(),
        "2026-08-25T23:00:00.000Z",
        0x71,
    )
    .unwrap();

    // The next shift opens after the cutover, on the 26th; a sale dated the
    // 25th cannot be rung up in it.
    let (on_the_26th, _) = open(
        &w,
        REGISTER,
        0x61,
        w.cashier(),
        "2026-08-26T06:00:00.000Z",
        ClockConfidence::Trusted,
        None,
    )
    .unwrap();
    assert_eq!(
        on_the_26th.business_date,
        BusinessDate::parse("2026-08-26").unwrap()
    );
    let refused = w.finalize(
        &sale_in(&w, on_the_26th.id, 0x30, "REG01-000002"),
        0x03,
        no_sabotage,
        false,
    );
    assert!(
        matches!(&refused, Err(DbError::Sqlite(e))
            if e.to_string() == "a completed sale requires the open shift for its register, store and business date"),
        "{refused:?}"
    );
    let dates: Vec<String> = w
        .conn
        .prepare("SELECT business_date FROM sale ORDER BY receipt_number")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(dates, ["2026-08-25"]);
}

#[test]
fn an_opening_and_a_close_travel_as_their_rows() {
    let w = world();
    let (opened, _) = open(
        &w,
        OTHER_REGISTER,
        0x60,
        w.cashier(),
        "2026-08-25T06:00:00.000Z",
        ClockConfidence::Trusted,
        None,
    )
    .unwrap();
    let register = opened.register_id.as_uuid().hyphenated();
    let store = opened.store_id.as_uuid().hyphenated();
    let cashier = w.cashier().as_uuid().hyphenated();
    let id = opened.id.as_uuid().hyphenated();
    assert_eq!(
        shift::opening_payload(&opened),
        format!(
            "{{\"business_date\":\"2026-08-25\",\"float_minor\":50000,\"id\":\"{id}\",\
             \"opened_at\":\"2026-08-25T06:00:00.000Z\",\"opened_by\":\"{cashier}\",\
             \"register_id\":\"{register}\",\"store_id\":\"{store}\"}}"
        )
    );
    // The close travels with the commit it is in.
    close(&w, &opened, w.cashier(), "2026-08-25T18:00:00.000Z", 0x70).unwrap();
    let close_id = Uuid::from_bytes([0x70, 0xC1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    let commit = Uuid::from_bytes([0x70; 16]);
    let carried: String = w
        .conn
        .query_row(
            "SELECT payload FROM fact_commit_member WHERE entity = 'shift_close_event'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        carried,
        format!(
            "{{\"closed_at\":\"2026-08-25T18:00:00.000Z\",\"closed_by\":\"{cashier}\",\
             \"id\":\"{}\",\"shift_id\":\"{id}\",\"sync_commit_id\":\"{}\"}}",
            close_id.hyphenated(),
            commit.hyphenated()
        )
    );

    // A shift with no envelope, one whose envelope carries anything else, and
    // one whose float is in another currency than its store's: each refused by
    // name, and nothing written.
    let other = Shift {
        id: ShiftId::from_uuid(uuid(0x61)),
        ..opened
    };
    let tx = w.conn.unchecked_transaction().unwrap();
    assert!(matches!(
        ShiftRepository::new(&w.conn).open(&tx, &other),
        Err(DbError::ShiftEnvelopeMissing { entity: "shift" })
    ));
    let dollars = Shift {
        float: Money::from_minor(5_000, Currency::USD),
        ..other.clone()
    };
    assert!(matches!(
        ShiftRepository::new(&w.conn).open(&tx, &dollars),
        Err(DbError::ShiftFloatCurrencyMismatch)
    ));
    write_envelope(
        &w.conn,
        &tx,
        0x61,
        &[(
            shift::ENTITY.to_owned(),
            *other.id.as_uuid().as_bytes(),
            "{}".to_owned(),
        )],
    );
    assert!(matches!(
        ShiftRepository::new(&w.conn).open(&tx, &other),
        Err(DbError::ShiftEnvelopeMismatch { entity: "shift" })
    ));
    drop(tx);
    assert_eq!(
        ShiftRepository::new(&w.conn)
            .current(opened.register_id)
            .unwrap(),
        None
    );
}

/// The owner's decision of 10 November 2026: a register's business days never
/// run backwards. A shift dated earlier than the register's last is refused by
/// name and leaves nothing behind; the same day again is a second shift that
/// day; another register's days are its own.
#[test]
fn a_shift_is_never_dated_before_the_registers_last() {
    let w = world();
    let untrusted = |id, at, date| {
        open(
            &w,
            OTHER_REGISTER,
            id,
            w.cashier(),
            at,
            ClockConfidence::Untrusted,
            Some(date),
        )
    };
    // Another register's days are its own: the fixture register has a shift
    // open on 2026-08-25, and this one opens a day earlier.
    let (eve, _) = untrusted(0x5E, "2026-08-24T06:00:00.000Z", "2026-08-24").unwrap();
    close(&w, &eve, w.cashier(), "2026-08-24T18:00:00.000Z", 0x6E).unwrap();

    let (monday, _) = untrusted(0x60, "2026-11-09T06:00:00.000Z", "2026-11-09").unwrap();
    close(&w, &monday, w.cashier(), "2026-11-09T18:00:00.000Z", 0x70).unwrap();

    // Tuesday morning, the operator types Monday's eve by mistake.
    assert!(matches!(
        untrusted(0x61, "2026-11-10T06:00:00.000Z", "2026-11-08"),
        Err(DbError::ShiftDateBeforePrevious { previous, asked, .. })
            if previous == BusinessDate::parse("2026-11-09").unwrap()
                && asked == BusinessDate::parse("2026-11-08").unwrap()
    ));
    for (table, id) in [
        ("sync_commit", [0x61_u8; 16]),
        ("shift", *uuid(0x61).as_bytes()),
    ] {
        assert_eq!(
            w.conn
                .query_row(
                    &format!("SELECT count(*) FROM {table} WHERE id = ?1"),
                    [id.as_slice()],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0,
            "the refused opening left nothing in {table}"
        );
    }

    // A second shift on Monday is a second shift that day.
    let (again, _) = untrusted(0x62, "2026-11-09T19:00:00.000Z", "2026-11-09").unwrap();
    close(&w, &again, w.cashier(), "2026-11-09T22:00:00.000Z", 0x71).unwrap();
    // Tuesday, typed right.
    untrusted(0x63, "2026-11-10T06:00:00.000Z", "2026-11-10").unwrap();
}
