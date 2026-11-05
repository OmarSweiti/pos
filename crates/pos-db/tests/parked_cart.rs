//! Microstep 1.8.2b — the register's parked carts, against the exact
//! registered chain.
//!
//! A restart is a real one: the connection is closed and the file opened
//! again, and a transaction a process died inside is dropped uncommitted.
//! After every injected restart a fixture leaves exactly one current basket,
//! or exactly one completed sale.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod sale_world;

use pos_db::DbError;
use pos_db::fixtures::registered_chain::Member;
use pos_db::repo::audit::AuditRepository;
use pos_db::repo::parked_cart::{ParkedCartRepository, Resumed, SetAside};
use pos_domain::cart::{ParkedCart, park, set_qty};
use pos_domain::{
    ApprovalBinding, Authorized, CartContext, CartError, Currency, EscalationPolicy, GrantSet,
    PermissionError, Qty, RegisterId, Role, SaleId, ShiftId, Tender, TenderId, TenderState, UserId,
    authorize, cap,
};
use rusqlite::{Connection, params};
use sale_world::{AT, DAY, KEY, REGISTER, SHIFT, World, at, day, jod, no_sabotage, uuid};
use uuid::Uuid;

/// The cashier who takes over another's basket.
const SECOND_CASHIER: u8 = 0x55;
/// The owner, who runs no till and holds no `sale.resume`.
const OWNER: u8 = 0x56;

fn nonce(n: u8) -> Uuid {
    Uuid::from_bytes([0x9E, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, n])
}

fn user(world: &World, byte: u8, code: &str) -> UserId {
    world
        .conn
        .execute(
            "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
             VALUES (?1, ?2, ?3, ?3, 'placeholder-not-a-hash', ?4)",
            params![uuid(byte).as_bytes().as_slice(), &world.chain.org, code, AT],
        )
        .unwrap();
    UserId::from_uuid(uuid(byte))
}

/// The register's till, on `shift`, for `cashier`.
fn till_on(shift: u8, cashier: UserId) -> CartContext {
    CartContext {
        register_id: RegisterId::from_uuid(uuid(REGISTER)),
        shift_id: ShiftId::from_uuid(uuid(shift)),
        cashier_id: cashier,
        currency: Currency::JOD,
        is_training: false,
        opened_at: at(),
    }
}

fn till(cashier: UserId) -> CartContext {
    till_on(SHIFT, cashier)
}

fn unbound() -> ApprovalBinding {
    ApprovalBinding {
        entity_id: Uuid::nil(),
        amount_minor: 0,
        content_hash: None,
    }
}

/// `who`'s proof of `sale.resume`, as their role grants it.
fn proof(who: UserId, role: Role) -> Result<Authorized<cap::SaleResume>, PermissionError> {
    authorize::<cap::SaleResume>(
        who,
        &GrantSet::of_role(role),
        None,
        &unbound(),
        &EscalationPolicy::empty(),
        at(),
    )
}

/// A basket of one item, opened on the register's shift by its cashier.
fn basket(world: &World, sale: u8) -> ParkedCart {
    let mut cart = world.cart(sale);
    cart.lines = vec![world.line(sale.wrapping_add(1), world.item, 1_000, 1_000)];
    park(cart).unwrap()
}

fn parked_carts(world: &World) -> ParkedCartRepository<'_> {
    ParkedCartRepository::new(&world.conn)
}

fn park_committed(world: &World, parked: &ParkedCart, label: Option<&str>) {
    let tx = world.conn.unchecked_transaction().unwrap();
    parked_carts(world).park(&tx, parked, label, at()).unwrap();
    tx.commit().unwrap();
}

fn resume_committed(world: &World, id: SaleId, till: &CartContext, session: Uuid) -> Resumed {
    let tx = world.conn.unchecked_transaction().unwrap();
    let auth = proof(till.cashier_id, Role::Cashier).unwrap();
    let resumed = parked_carts(world)
        .resume(&tx, id, till, &auth, session)
        .unwrap();
    tx.commit().unwrap();
    resumed
}

/// The process dies and the register starts again: the connection is closed,
/// and the file opened afresh.
fn restart(world: &mut World) {
    let path = world._dir.path().join("sales.db");
    let dead = std::mem::replace(&mut world.conn, Connection::open_in_memory().unwrap());
    dead.close().map_err(|(_, e)| e).unwrap();
    world.conn = pos_db::open(&path, KEY).unwrap();
}

/// Every row the table holds, as `(state, session)`.
fn rows(conn: &Connection) -> Vec<(String, Option<Vec<u8>>)> {
    conn.prepare("SELECT state, session_nonce FROM parked_cart ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn active(session: Uuid) -> (String, Option<Vec<u8>>) {
    ("active".to_owned(), Some(session.as_bytes().to_vec()))
}

fn parked_row() -> (String, Option<Vec<u8>>) {
    ("parked".to_owned(), None)
}

/// E.3, 1.8.2b: an app killed with carts parked comes back with them, each
/// exactly as it was set aside, under the label it was given.
#[test]
fn parked_carts_survive_restart() {
    let mut world = World::new();
    let table_four = basket(&world, 0x40);
    let unlabelled = basket(&world, 0x48);
    park_committed(&world, &table_four, Some("table 4"));
    park_committed(&world, &unlabelled, None);

    restart(&mut world);

    let listed = parked_carts(&world)
        .parked(RegisterId::from_uuid(uuid(REGISTER)), day())
        .unwrap();
    let summary: Vec<(SaleId, Option<&str>, UserId)> = listed
        .iter()
        .map(|e| (e.id, e.label.as_deref(), e.parked_by))
        .collect();
    assert_eq!(
        summary,
        [
            (table_four.cart().id, Some("table 4"), world.cashier()),
            (unlabelled.cart().id, None, world.cashier()),
        ]
    );
    assert_eq!(listed.first().unwrap().cart, table_four);
    assert!(
        listed
            .iter()
            .all(|e| e.expires_on == day() && e.parked_at == at())
    );

    let resumed = resume_committed(
        &world,
        table_four.cart().id,
        &till(world.cashier()),
        nonce(1),
    );
    assert_eq!(
        &resumed.cart,
        table_four.cart(),
        "the basket, exactly as it was parked"
    );
    assert_eq!(
        parked_carts(&world)
            .parked(RegisterId::from_uuid(uuid(REGISTER)), day())
            .unwrap()
            .len(),
        1,
        "the other customer's basket is still waiting"
    );
}

/// A resume claims its row in the same transaction that returns the basket.
/// A process that dies before that transaction commits leaves the cart parked,
/// as if nobody had touched it. Once committed, the cart is one session's, and
/// a second claim is refused rather than handing out a second copy.
#[test]
fn resume_claims_the_working_row_atomically() {
    let mut world = World::new();
    let parked = basket(&world, 0x40);
    let id = parked.cart().id;
    park_committed(&world, &parked, Some("table 4"));

    // The restart lands before the resume commits.
    {
        let tx = world.conn.unchecked_transaction().unwrap();
        let auth = proof(world.cashier(), Role::Cashier).unwrap();
        let resumed = parked_carts(&world)
            .resume(&tx, id, &till(world.cashier()), &auth, nonce(1))
            .unwrap();
        assert_eq!(resumed.audit, None, "the cashier's own basket");
        drop(tx);
    }
    restart(&mut world);
    assert_eq!(
        rows(&world.conn),
        [parked_row()],
        "still parked, still one basket"
    );

    let resumed = resume_committed(&world, id, &till(world.cashier()), nonce(2));
    assert_eq!(resumed.audit, None);
    assert_eq!(rows(&world.conn), [active(nonce(2))]);
    assert!(
        parked_carts(&world)
            .parked(RegisterId::from_uuid(uuid(REGISTER)), day())
            .unwrap()
            .is_empty(),
        "a basket being worked on is not offered to anyone else"
    );

    let tx = world.conn.unchecked_transaction().unwrap();
    let auth = proof(world.cashier(), Role::Cashier).unwrap();
    let second = parked_carts(&world).resume(&tx, id, &till(world.cashier()), &auth, nonce(3));
    assert!(
        matches!(second, Err(DbError::ParkedCartClaimed(claimed)) if claimed == id),
        "{second:?}"
    );
    drop(tx);
    assert_eq!(
        rows(&world.conn),
        [active(nonce(2))],
        "exactly one current basket"
    );

    // Nor is the basket being worked on set aside as a second row.
    let tx = world.conn.unchecked_transaction().unwrap();
    let again = parked_carts(&world).park(&tx, &parked, None, at());
    assert!(
        matches!(again, Err(DbError::ParkedCartAlreadyParked(p)) if p == id),
        "{again:?}"
    );
    drop(tx);
}

/// The process dies after a resume committed and before the till heard about
/// it. The register starts again with the basket still claimed, hands the
/// claim to its new session, and the dead session's nonce can no longer write.
/// A change made to the basket afterwards survives the next restart too.
#[test]
fn process_death_after_resume_commit_restores_the_active_cart() {
    let mut world = World::new();
    let parked = basket(&world, 0x40);
    let id = parked.cart().id;
    park_committed(&world, &parked, Some("table 4"));
    let resumed = resume_committed(&world, id, &till(world.cashier()), nonce(1));
    let register = RegisterId::from_uuid(uuid(REGISTER));

    restart(&mut world);

    let tx = world.conn.unchecked_transaction().unwrap();
    let restored = parked_carts(&world)
        .reclaim_after_restart(&tx, register, nonce(2))
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(
        restored,
        std::slice::from_ref(&resumed.cart),
        "the basket the till was working on"
    );
    assert_eq!(rows(&world.conn), [active(nonce(2))]);

    // The dead session's claim died with it.
    let line = resumed.cart.lines.first().unwrap().id;
    let changed = set_qty(resumed.cart, line, Qty::from_milli(3_000)).unwrap();
    let tx = world.conn.unchecked_transaction().unwrap();
    let stale = parked_carts(&world).save_active(&tx, nonce(1), &changed);
    assert!(
        matches!(stale, Err(DbError::ParkedCartNotClaimed(c)) if c == id),
        "{stale:?}"
    );
    parked_carts(&world)
        .save_active(&tx, nonce(2), &changed)
        .unwrap();
    tx.commit().unwrap();

    // The restart lands after an active-cart mutation.
    restart(&mut world);
    let tx = world.conn.unchecked_transaction().unwrap();
    let restored = parked_carts(&world)
        .reclaim_after_restart(&tx, register, nonce(3))
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(
        restored,
        std::slice::from_ref(&changed),
        "the change survived the restart"
    );
    assert_eq!(
        rows(&world.conn),
        [active(nonce(3))],
        "exactly one current basket"
    );

    // Set aside again, it is offered to the till once more.
    let tx = world.conn.unchecked_transaction().unwrap();
    parked_carts(&world)
        .repark(
            &tx,
            nonce(3),
            &park(changed).unwrap(),
            Some("table 4, again"),
            at(),
        )
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(rows(&world.conn), [parked_row()]);
    let listed = parked_carts(&world).parked(register, day()).unwrap();
    assert_eq!(
        listed.first().map(|e| e.label.as_deref()),
        Some(Some("table 4, again"))
    );
}

/// The owner's decision on #363: whoever holds `sale.resume` at the register
/// may resume any parked cart, and taking over another cashier's basket is
/// audited in the same transaction, naming who parked it. The owner holds no
/// `sale.resume`, so has no proof to resume with at all.
#[test]
fn another_actor_cannot_resume_without_the_required_grant() {
    let mut world = World::new();
    let second = user(&world, SECOND_CASHIER, "C-2");
    let owner = user(&world, OWNER, "OWN-1");
    let theirs = basket(&world, 0x40);
    let own = basket(&world, 0x48);
    park_committed(&world, &theirs, None);
    park_committed(&world, &own, None);

    // Without the grant there is no proof, and no resume can be called.
    assert!(proof(owner, Role::Owner).is_err());

    // A proof that is not the till's own cashier's is refused.
    let tx = world.conn.unchecked_transaction().unwrap();
    let firsts = proof(world.cashier(), Role::Cashier).unwrap();
    let refused =
        parked_carts(&world).resume(&tx, theirs.cart().id, &till(second), &firsts, nonce(1));
    assert!(
        matches!(
            refused,
            Err(DbError::ResumeRefused(
                CartError::ResumedByAnotherThanTheTill(..)
            ))
        ),
        "{refused:?}"
    );
    drop(tx);

    // The second cashier takes over the first's basket, and the handover is
    // audited with its envelope in the same transaction.
    let tx = world.conn.unchecked_transaction().unwrap();
    let auth = proof(second, Role::Cashier).unwrap();
    let resumed = parked_carts(&world)
        .resume(&tx, theirs.cart().id, &till(second), &auth, nonce(2))
        .unwrap();
    assert_eq!(resumed.cart.cashier_id, second);
    let intent = resumed.audit.expect("another cashier's basket is audited");
    assert_eq!(intent.action, "sale.resume");
    assert_eq!(
        intent
            .payload
            .get("parked_by")
            .and_then(|v| v.as_str())
            .unwrap(),
        world.cashier().as_uuid().to_string()
    );
    let audit_id = Uuid::from_bytes([0xAD; 16]);
    sale_world::write_envelope(
        &world.conn,
        &tx,
        0xE3,
        &[(
            "audit_log".to_owned(),
            *audit_id.as_bytes(),
            "{}".to_owned(),
        )],
    );
    AuditRepository::new(&world.conn)
        .append(
            &tx,
            RegisterId::from_uuid(uuid(REGISTER)),
            audit_id,
            &intent,
        )
        .unwrap();
    tx.commit().unwrap();
    let audited: (Vec<u8>, String) = world
        .conn
        .query_row(
            "SELECT actor_id, action FROM audit_log WHERE entity = 'sale' AND entity_id = ?1",
            [theirs.cart().id.as_uuid().as_bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        audited,
        (
            second.as_uuid().as_bytes().to_vec(),
            "sale.resume".to_owned()
        )
    );

    // A cashier coming back to their own customer leaves no record.
    let mine = resume_committed(&world, own.cart().id, &till(world.cashier()), nonce(3));
    assert_eq!(mine.audit, None);

    // On disk too, each basket is now the cashier's who resumed it.
    restart(&mut world);
    let tx = world.conn.unchecked_transaction().unwrap();
    let restored = parked_carts(&world)
        .reclaim_after_restart(&tx, RegisterId::from_uuid(uuid(REGISTER)), nonce(4))
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(
        restored
            .iter()
            .map(|c| (c.id, c.cashier_id))
            .collect::<Vec<_>>(),
        [(theirs.cart().id, second), (own.cart().id, world.cashier())]
    );
}

/// I-4's edge, from the other side: the working cart a sale came from is
/// removed only inside the transaction that completes the sale. A process that
/// dies during finalize takes the removal with it, and the register starts
/// again with the basket still there to try again; a finalize that commits
/// leaves one completed sale and no basket.
#[test]
fn finalize_consumes_the_working_cart_with_the_sale() {
    let mut world = World::new();
    let parked = basket(&world, 0x60);
    let id = parked.cart().id;
    let register = RegisterId::from_uuid(uuid(REGISTER));
    park_committed(&world, &parked, None);
    let resumed = resume_committed(&world, id, &till(world.cashier()), nonce(1));
    let card = Tender {
        id: TenderId::from_uuid(uuid(0x62)),
        type_code: "card".to_owned(),
        amount: jod(1_000),
        psp_ref: None,
        masked_pan: None,
        scheme: None,
        state: TenderState::Collected,
    };
    let sale = world.complete(resumed.cart.clone(), vec![card], 0, "REG01-000001");

    // The process dies during finalize: the completion is refused, here for
    // want of its receipt, the whole transaction is gone, and the register
    // starts again.
    let failed = world.finalize_with(&sale, 0xE1, no_sabotage, true, |tx| {
        parked_carts(&world).consume_on_finalize(tx, id, nonce(1))
    });
    assert!(failed.is_err());
    restart(&mut world);
    assert_eq!(world.sales().by_id(id).unwrap(), None, "no sale");
    let tx = world.conn.unchecked_transaction().unwrap();
    let restored = parked_carts(&world)
        .reclaim_after_restart(&tx, register, nonce(2))
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(
        restored,
        std::slice::from_ref(&resumed.cart),
        "the basket is still there"
    );
    assert_eq!(rows(&world.conn), [active(nonce(2))]);

    // The dead session's finalize completes nothing.
    let unclaimed = world.finalize_with(&sale, 0xE2, no_sabotage, false, |tx| {
        parked_carts(&world).consume_on_finalize(tx, id, nonce(1))
    });
    assert!(
        matches!(unclaimed, Err(DbError::ParkedCartNotClaimed(c)) if c == id),
        "{unclaimed:?}"
    );
    assert_eq!(world.sales().by_id(id).unwrap(), None);

    world
        .finalize_with(&sale, 0xE4, no_sabotage, false, |tx| {
            parked_carts(&world).consume_on_finalize(tx, id, nonce(2))
        })
        .unwrap();
    assert!(
        rows(&world.conn).is_empty(),
        "no basket beside the sale it became"
    );
    assert!(
        world.sales().by_id(id).unwrap().is_some(),
        "exactly one completed sale"
    );
}

/// Master plan C.2: a parked cart expires at the end of the business day it
/// was parked in. On a later business day it is no longer offered and cannot
/// be resumed; it is listed among the expired carts for the end-of-day report.
#[test]
fn a_cart_parked_on_an_earlier_business_day_has_expired() {
    let world = World::new();
    let parked = basket(&world, 0x40);
    let id = parked.cart().id;
    park_committed(&world, &parked, Some("table 4"));

    // The day's shift closes, and the next business day's opens.
    let close = [0xC5; 16];
    world.chain.write_envelope(
        &world.conn,
        &[0xE5; 16],
        &[Member::new(&[0xC6; 16], "shift_close_event", &close)],
    );
    world
        .conn
        .execute(
            "INSERT INTO shift_close_event (id, shift_id, sync_commit_id, closed_by, closed_at)
             VALUES (?1, ?2, ?3, ?4, '2026-08-25T23:00:00.000Z')",
            params![
                close.as_slice(),
                uuid(SHIFT).as_bytes().as_slice(),
                [0xE5_u8; 16].as_slice(),
                &world.chain.cashier
            ],
        )
        .unwrap();
    const NEXT_SHIFT: u8 = 0x57;
    world.chain.open_shift(
        &world.conn,
        &[NEXT_SHIFT; 16],
        &[REGISTER; 16],
        "2026-08-26",
    );
    let tomorrow = day().succ();
    let register = RegisterId::from_uuid(uuid(REGISTER));

    let tx = world.conn.unchecked_transaction().unwrap();
    let auth = proof(world.cashier(), Role::Cashier).unwrap();
    let refused = parked_carts(&world).resume(
        &tx,
        id,
        &till_on(NEXT_SHIFT, world.cashier()),
        &auth,
        nonce(1),
    );
    assert!(
        matches!(&refused, Err(DbError::ParkedCartExpired { id: e, expires_on }) if *e == id && expires_on == DAY),
        "{refused:?}"
    );
    drop(tx);

    assert!(
        parked_carts(&world)
            .parked(register, tomorrow)
            .unwrap()
            .is_empty()
    );
    let expired = parked_carts(&world).expired(register, tomorrow).unwrap();
    assert_eq!(
        expired
            .iter()
            .map(|e| (e.id, e.label.as_deref()))
            .collect::<Vec<_>>(),
        [(id, Some("table 4"))]
    );
    assert_eq!(
        rows(&world.conn),
        [parked_row()],
        "kept for the report, not deleted"
    );
}

/// A stored form this build cannot read, or one holding another sale's cart,
/// is refused by name rather than resumed as a guess, and the refusal never
/// quotes the stored form, which can carry a buyer's name.
#[test]
fn a_stored_cart_this_build_cannot_read_is_refused_without_quoting_it() {
    let world = World::new();
    let one = basket(&world, 0x40);
    let other = basket(&world, 0x48);
    park_committed(&world, &one, None);
    park_committed(&world, &other, None);
    let resume = |id: SaleId| {
        let tx = world.conn.unchecked_transaction().unwrap();
        let auth = proof(world.cashier(), Role::Cashier).unwrap();
        parked_carts(&world).resume(&tx, id, &till(world.cashier()), &auth, nonce(1))
    };

    world
        .conn
        .execute(
            "UPDATE parked_cart SET snapshot = (SELECT snapshot FROM parked_cart WHERE id = ?1)
              WHERE id = ?2",
            params![
                other.cart().id.as_uuid().as_bytes().as_slice(),
                one.cart().id.as_uuid().as_bytes().as_slice()
            ],
        )
        .unwrap();
    let swapped = resume(one.cart().id);
    assert!(
        matches!(&swapped, Err(DbError::StoredParkedCartInvalid { reason }) if reason.contains("another sale's cart")),
        "{swapped:?}"
    );

    world
        .conn
        .execute(
            "UPDATE parked_cart SET snapshot = '{\"buyer_name\": \"Synthetic Buyer'
              WHERE id = ?1",
            [other.cart().id.as_uuid().as_bytes().as_slice()],
        )
        .unwrap();
    let cut = resume(other.cart().id);
    assert!(
        matches!(&cut, Err(DbError::StoredParkedCartInvalid { reason }) if reason.contains("cut short")),
        "{cut:?}"
    );
    let shown = format!("{cut:?} {}", cut.as_ref().unwrap_err());
    assert!(
        !shown.contains("Synthetic Buyer"),
        "the stored form is never quoted"
    );
    assert_eq!(
        rows(&world.conn),
        [parked_row(), parked_row()],
        "left as they were found"
    );
}

/// A cart is held from the moment the till opens it, so a basket never parked
/// is still on disk after a restart, claimed, with every change saved
/// (1.4.11, the owner's decision of 5 November 2026). It is held once, and
/// only on a shift the register holds.
#[test]
fn a_working_cart_is_held_from_the_moment_it_opens() {
    let mut world = World::new();
    let session = nonce(1);
    let opened = world.cart(0x40);

    let tx = world.conn.unchecked_transaction().unwrap();
    parked_carts(&world).hold(&tx, &opened, session).unwrap();
    tx.commit().unwrap();
    assert_eq!(rows(&world.conn), vec![active(session)]);
    let (parked_at, expires_on): (String, String) = world
        .conn
        .query_row(
            "SELECT parked_at, expires_on FROM parked_cart WHERE id = ?1",
            [opened.id.as_uuid().as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (parked_at, expires_on),
        (opened.opened_at.to_iso8601(), DAY.to_owned()),
        "a cart never parked began when it was opened, and expires with its business day"
    );

    // A line added is saved over the held snapshot.
    let mut changed = opened.clone();
    changed.lines = vec![world.line(0x41, world.item, 1_000, 2_000)];
    let tx = world.conn.unchecked_transaction().unwrap();
    parked_carts(&world)
        .save_active(&tx, session, &changed)
        .unwrap();
    tx.commit().unwrap();

    // The process dies; the next one finds the basket, as it was saved.
    restart(&mut world);
    let tx = world.conn.unchecked_transaction().unwrap();
    let found = parked_carts(&world)
        .reclaim_after_restart(&tx, RegisterId::from_uuid(uuid(REGISTER)), nonce(2))
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(found, vec![changed.clone()]);
    assert_eq!(rows(&world.conn), vec![active(nonce(2))]);

    // A cart is held once, and only on a shift this register holds.
    let tx = world.conn.unchecked_transaction().unwrap();
    assert!(matches!(
        parked_carts(&world).hold(&tx, &opened, session),
        Err(DbError::ParkedCartAlreadyParked(id)) if id == opened.id
    ));
    let mut elsewhere = world.cart(0x42);
    elsewhere.shift_id = ShiftId::from_uuid(uuid(0x7F));
    assert!(matches!(
        parked_carts(&world).hold(&tx, &elsewhere, session),
        Err(DbError::ParkedCartShiftUnknown(shift)) if shift == elsewhere.shift_id
    ));
}

/// Only a basket with nothing in it is dropped, and only by the session that
/// claims it. Whether it is empty is read from disk: a caller holding a stale,
/// empty copy cannot drop a basket whose saved snapshot has lines.
#[test]
fn only_an_empty_working_cart_is_dropped() {
    let world = World::new();
    let session = nonce(1);
    let opened = world.cart(0x40);
    let tx = world.conn.unchecked_transaction().unwrap();
    parked_carts(&world).hold(&tx, &opened, session).unwrap();
    let mut changed = opened.clone();
    changed.lines = vec![world.line(0x41, world.item, 1_000, 1_000)];
    parked_carts(&world)
        .save_active(&tx, session, &changed)
        .unwrap();
    tx.commit().unwrap();

    let tx = world.conn.unchecked_transaction().unwrap();
    assert!(matches!(
        parked_carts(&world).discard(&tx, session, opened.id),
        Err(DbError::ParkedCartNotEmpty(id)) if id == opened.id
    ));
    assert!(matches!(
        parked_carts(&world).discard(&tx, nonce(9), opened.id),
        Err(DbError::ParkedCartNotClaimed(id)) if id == opened.id
    ));
    drop(tx);
    assert_eq!(rows(&world.conn), vec![active(session)]);

    // Emptied again, it is dropped, and nothing of it is left.
    let tx = world.conn.unchecked_transaction().unwrap();
    parked_carts(&world)
        .save_active(&tx, session, &opened)
        .unwrap();
    parked_carts(&world)
        .discard(&tx, session, opened.id)
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(rows(&world.conn), Vec::new());
}

/// A working cart set aside goes to the parked list if it holds a line,
/// unlabelled, unclaimed and under its cashier, and is dropped if it holds
/// none. Only the session that claims it can set it aside, and a stale, empty
/// copy never drops the basket saved on disk.
#[test]
fn a_working_cart_is_set_aside_parked_or_dropped() {
    let world = World::new();
    let session = nonce(1);
    let mut basket = world.cart(0x50);
    let empty = world.cart(0x52);
    let tx = world.conn.unchecked_transaction().unwrap();
    parked_carts(&world).hold(&tx, &basket, session).unwrap();
    parked_carts(&world).hold(&tx, &empty, session).unwrap();
    basket.lines = vec![world.line(0x51, world.item, 1_000, 1_000)];
    parked_carts(&world)
        .save_active(&tx, session, &basket)
        .unwrap();
    tx.commit().unwrap();

    let tx = world.conn.unchecked_transaction().unwrap();
    let mut stale = basket.clone();
    stale.lines.clear();
    assert!(matches!(
        parked_carts(&world).set_aside(&tx, session, &stale, at()),
        Err(DbError::ParkedCartNotEmpty(id)) if id == basket.id
    ));
    assert!(matches!(
        parked_carts(&world).set_aside(&tx, nonce(9), &basket, at()),
        Err(DbError::ParkedCartNotClaimed(id)) if id == basket.id
    ));
    drop(tx);
    assert_eq!(rows(&world.conn), vec![active(session), active(session)]);

    let tx = world.conn.unchecked_transaction().unwrap();
    assert_eq!(
        parked_carts(&world)
            .set_aside(&tx, session, &basket, at())
            .unwrap(),
        SetAside::Parked
    );
    assert_eq!(
        parked_carts(&world)
            .set_aside(&tx, session, &empty, at())
            .unwrap(),
        SetAside::Dropped
    );
    tx.commit().unwrap();
    assert_eq!(rows(&world.conn), vec![parked_row()]);
    let listed = parked_carts(&world)
        .parked(RegisterId::from_uuid(uuid(REGISTER)), day())
        .unwrap();
    let entry = listed.first().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (entry.id, entry.label.as_deref(), entry.parked_by, entry.parked_at),
        (basket.id, None, basket.cashier_id, at())
    );
    assert_eq!(entry.cart.cart(), &basket);
}
