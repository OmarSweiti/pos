//! The checkout journal (microstep 1.8.4, its journal half).
//!
//! Every sale here is a real cart priced by the domain under its store's own
//! pack and policy (`sale_world`), and every finalize is the transaction 1.8.3
//! will write, with the journal's removal inside it. A restart closes the
//! connection and opens the file afresh.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod sale_world;

use pos_db::DbError;
use pos_db::repo::checkout_operation::{CheckoutOperation, CheckoutOperationRepository};
use pos_domain::cart::{add_tender, begin_finalize, complete};
use pos_domain::{
    CardOpRef, FinalizeEffects, InFlight, InFlightSale, RegisterId, SaleId, Tendering, Timestamp,
};
use rusqlite::Connection;
use sale_world::{
    BUYER_NAME, BUYER_TIN, KEY, MANAGER, OTHER_REGISTER, REGISTER, World, at, day, jod,
    no_sabotage, tender, uuid,
};
use uuid::Uuid;

fn journal(world: &World) -> CheckoutOperationRepository<'_> {
    CheckoutOperationRepository::new(&world.conn)
}

/// The idempotency key of checkout attempt `n`.
fn key(n: u8) -> Uuid {
    Uuid::from_bytes([0x4B, n, 0, 0, 0, 0, 0x70, 0, 0x80, 0, 0, 0, 0, 0, 0, n])
}

/// The fixture's instant, `seconds` later.
fn later(seconds: u32) -> Timestamp {
    Timestamp::parse_iso8601(&format!("2026-08-25T10:00:{seconds:02}.000Z")).unwrap()
}

fn register() -> RegisterId {
    RegisterId::from_uuid(uuid(REGISTER))
}

/// Sale `sale`: one item at 1.000 JOD, priced and being paid for.
fn paying(world: &World, sale: u8) -> Tendering {
    let mut cart = world.cart(sale);
    cart.lines = vec![world.line(sale.wrapping_add(1), world.item, 1_000, 1_000)];
    world.tendering(cart)
}

fn in_flight(state: InFlight, key: Uuid, at: Timestamp) -> InFlightSale {
    InFlightSale {
        state,
        card_op: None,
        idempotency_key: key,
        updated_at: at,
    }
}

fn begun(world: &World, tendering: &Tendering, key: Uuid) {
    let tx = world.conn.unchecked_transaction().unwrap();
    journal(world).begin(&tx, tendering, key, at()).unwrap();
    tx.commit().unwrap();
}

/// `in_flight` recorded over `version` in a transaction of its own, which
/// commits when the write is accepted.
fn advanced(world: &World, in_flight: &InFlightSale, version: i64) -> Result<i64, DbError> {
    let tx = world.conn.unchecked_transaction().unwrap();
    let advanced = journal(world).advance(&tx, in_flight, version)?;
    tx.commit().unwrap();
    Ok(advanced)
}

/// The process dies and the register starts again: the connection is closed,
/// and the file opened afresh.
fn restart(world: &mut World) {
    let path = world._dir.path().join("sales.db");
    let dead = std::mem::replace(&mut world.conn, Connection::open_in_memory().unwrap());
    dead.close().map_err(|(_, e)| e).unwrap();
    world.conn = pos_db::open(&path, KEY).unwrap();
}

/// The sale `finalizing` completes as, with the fixture's receipt number.
fn completed(finalizing: &pos_domain::Finalizing) -> pos_domain::CompletedSale {
    complete(
        finalizing.clone(),
        FinalizeEffects {
            receipt_number: "REG01-000001".to_owned(),
            business_date: day(),
            completed_at: at(),
        },
    )
    .unwrap()
}

/// A checkout is journalled as its payment begins, before any tender, and
/// reads back exactly as it was written: its sale, its key, its moment, and
/// the cart's own register, shift and cashier in the row.
#[test]
fn a_checkout_is_journalled_as_payment_begins_and_read_back_whole() {
    let world = World::new();
    let paying = paying(&world, 0x70);
    let sale = paying.cart().id;
    begun(&world, &paying, key(1));

    let journalled = journal(&world).get(sale).unwrap().unwrap();
    assert_eq!(
        journalled,
        CheckoutOperation {
            in_flight: in_flight(InFlight::Tendering(paying.clone()), key(1), at()),
            version: 0,
            created_at: at(),
        }
    );
    assert_eq!(
        journal(&world).in_flight(register()).unwrap(),
        vec![journalled]
    );
    assert!(
        journal(&world)
            .in_flight(RegisterId::from_uuid(uuid(OTHER_REGISTER)))
            .unwrap()
            .is_empty(),
        "another register's journal is its own"
    );
    let row: (Vec<u8>, Vec<u8>, Vec<u8>, String) = world
        .conn
        .query_row(
            "SELECT register_id, shift_id, actor_id, state FROM checkout_operation",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    let cart = paying.cart();
    assert_eq!(
        row,
        (
            cart.register_id.as_uuid().as_bytes().to_vec(),
            cart.shift_id.as_uuid().as_bytes().to_vec(),
            cart.cashier_id.as_uuid().as_bytes().to_vec(),
            "tendering".to_owned(),
        )
    );
}

/// Each tender taken advances the journal, and a restart finds the checkout
/// exactly as far as it got. A writer holding an older copy is refused, so a
/// stale copy never overwrites a newer step.
#[test]
fn each_step_advances_the_journal_and_survives_a_restart() {
    let mut world = World::new();
    let paying = paying(&world, 0x72);
    let sale = paying.cart().id;
    begun(&world, &paying, key(2));

    let part_paid = add_tender(paying.clone(), tender(0x74, "cash", 400)).unwrap();
    let version = advanced(
        &world,
        &in_flight(InFlight::Tendering(part_paid.clone()), key(2), later(5)),
        0,
    )
    .unwrap();
    assert_eq!(version, 1);

    let stale = add_tender(paying, tender(0x75, "cash", 300)).unwrap();
    let refused = advanced(
        &world,
        &in_flight(InFlight::Tendering(stale), key(2), later(6)),
        0,
    );
    assert!(
        matches!(
            refused,
            Err(DbError::CheckoutStale { sale: s, expected: 0, found: 1 }) if s == sale
        ),
        "{refused:?}"
    );

    restart(&mut world);
    let journalled = journal(&world).get(sale).unwrap().unwrap();
    assert_eq!(
        journalled,
        CheckoutOperation {
            in_flight: in_flight(InFlight::Tendering(part_paid), key(2), later(5)),
            version: 1,
            created_at: at(),
        }
    );
}

/// Once its finalize begins, a checkout takes no more tenders and cannot be
/// abandoned. A finalize that fails takes nothing with it, across a restart:
/// no sale, and the journal still there to replay under the same key. The
/// finalize that commits removes it in its own transaction.
#[test]
fn once_finalizing_only_the_finalize_removes_it() {
    let mut world = World::new();
    let paying = paying(&world, 0x76);
    let sale = paying.cart().id;
    begun(&world, &paying, key(3));
    let paid = add_tender(paying, tender(0x78, "cash", 1_000)).unwrap();
    let finalizing = begin_finalize(paid.clone()).unwrap();
    let version = advanced(
        &world,
        &in_flight(InFlight::Finalizing(finalizing.clone()), key(3), later(10)),
        0,
    )
    .unwrap();

    let another_tender = advanced(
        &world,
        &in_flight(InFlight::Tendering(paid), key(3), later(11)),
        version,
    );
    assert!(
        matches!(another_tender, Err(DbError::CheckoutFinalizing(s)) if s == sale),
        "{another_tender:?}"
    );
    let tx = world.conn.unchecked_transaction().unwrap();
    let abandoned = journal(&world).abandon(&tx, sale, key(3));
    assert!(
        matches!(abandoned, Err(DbError::CheckoutFinalizing(s)) if s == sale),
        "{abandoned:?}"
    );
    let other_attempt = journal(&world).remove_on_finalize(&tx, sale, key(9));
    assert!(
        matches!(other_attempt, Err(DbError::CheckoutKeyMismatch(s)) if s == sale),
        "{other_attempt:?}"
    );
    drop(tx);

    // The journal's removal runs, and then the finalize is refused, here for
    // want of its receipt: the whole transaction is gone, the removal with it,
    // and the register starts again.
    let completed = completed(&finalizing);
    let mut removed = false;
    let failed = world.finalize_with(&completed, 0xE1, no_sabotage, true, |tx| {
        let removal = CheckoutOperationRepository::new(tx).remove_on_finalize(tx, sale, key(3));
        removed = removal.is_ok();
        removal
    });
    assert!(removed, "the removal ran inside the finalize");
    assert!(failed.is_err(), "the finalize was refused after it");
    restart(&mut world);
    assert_eq!(world.sales().by_id(sale).unwrap(), None, "no sale");
    let still = journal(&world).get(sale).unwrap().unwrap();
    assert_eq!(still.in_flight.state, InFlight::Finalizing(finalizing));
    assert_eq!(still.in_flight.idempotency_key, key(3));

    world
        .finalize_with(&completed, 0xE2, no_sabotage, false, |tx| {
            CheckoutOperationRepository::new(tx).remove_on_finalize(tx, sale, key(3))
        })
        .unwrap();
    assert_eq!(journal(&world).get(sale).unwrap(), None, "no journal");
    assert!(world.sales().by_id(sale).unwrap().is_some(), "one sale");
}

/// A finalize whose checkout never began finalizing is refused, and the
/// refusal fails the finalize: the sale is not written and the journal stays.
#[test]
fn a_checkout_still_being_paid_for_cannot_be_finalized_away() {
    let world = World::new();
    let paying = paying(&world, 0x7A);
    let sale = paying.cart().id;
    begun(&world, &paying, key(4));
    let paid = add_tender(paying, tender(0x7C, "cash", 1_000)).unwrap();
    let completed = completed(&begin_finalize(paid).unwrap());

    let refused = world.finalize_with(&completed, 0xE3, no_sabotage, false, |tx| {
        CheckoutOperationRepository::new(tx).remove_on_finalize(tx, sale, key(4))
    });
    assert!(
        matches!(refused, Err(DbError::CheckoutNotFinalizing(s)) if s == sale),
        "{refused:?}"
    );
    assert_eq!(world.sales().by_id(sale).unwrap(), None);
    assert!(journal(&world).get(sale).unwrap().is_some());
}

/// A sale's checkout is journalled once, and a key belongs to one attempt.
#[test]
fn a_checkout_is_journalled_once_and_its_key_is_not_reused() {
    let world = World::new();
    let first = paying(&world, 0x80);
    let second = paying(&world, 0x82);
    begun(&world, &first, key(5));

    let tx = world.conn.unchecked_transaction().unwrap();
    let again = journal(&world).begin(&tx, &first, key(6), at());
    assert!(
        matches!(again, Err(DbError::CheckoutAlreadyJournalled(s)) if s == first.cart().id),
        "{again:?}"
    );
    let reused = journal(&world).begin(&tx, &second, key(5), at());
    assert!(
        matches!(reused, Err(DbError::CheckoutKeyReused(s)) if s == second.cart().id),
        "{reused:?}"
    );
}

/// A checkout still being paid for can be given up. What was journalled comes
/// back for the void's or the recovery's audit, the row is gone, and the sale
/// can begin a new attempt under a new key.
#[test]
fn an_abandoned_checkout_is_returned_for_its_audit_and_can_begin_again() {
    let world = World::new();
    let paying = paying(&world, 0x84);
    let sale = paying.cart().id;
    begun(&world, &paying, key(7));
    let part_paid = add_tender(paying.clone(), tender(0x86, "cash", 400)).unwrap();
    advanced(
        &world,
        &in_flight(InFlight::Tendering(part_paid.clone()), key(7), later(20)),
        0,
    )
    .unwrap();

    let tx = world.conn.unchecked_transaction().unwrap();
    let gone = journal(&world).abandon(&tx, sale, key(7)).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        gone,
        CheckoutOperation {
            in_flight: in_flight(InFlight::Tendering(part_paid), key(7), later(20)),
            version: 1,
            created_at: at(),
        }
    );
    assert_eq!(journal(&world).get(sale).unwrap(), None);

    begun(&world, &paying, key(8));
    assert_eq!(
        journal(&world)
            .get(sale)
            .unwrap()
            .unwrap()
            .in_flight
            .idempotency_key,
        key(8)
    );
}

/// Every write names the checkout it means: one with no journal, or under
/// another attempt's key, is refused by name.
#[test]
fn a_checkout_not_journalled_or_under_another_key_is_refused() {
    let world = World::new();
    let paying = paying(&world, 0x88);
    let sale = paying.cart().id;
    let step = in_flight(InFlight::Tendering(paying.clone()), key(10), later(30));

    let none = advanced(&world, &step, 0);
    assert!(
        matches!(none, Err(DbError::CheckoutNotJournalled(s)) if s == sale),
        "{none:?}"
    );

    begun(&world, &paying, key(11));
    let other = advanced(&world, &step, 0);
    assert!(
        matches!(other, Err(DbError::CheckoutKeyMismatch(s)) if s == sale),
        "{other:?}"
    );
    let tx = world.conn.unchecked_transaction().unwrap();
    let abandoned = journal(&world).abandon(&tx, sale, key(10));
    assert!(
        matches!(abandoned, Err(DbError::CheckoutKeyMismatch(s)) if s == sale),
        "{abandoned:?}"
    );
}

/// The card terminal is Phase 2's, and the table cannot hold a card
/// operation's amount or start time. One is refused by name rather than half
/// stored, and a row that records one is not read as a checkout without it.
#[test]
fn a_card_operation_is_refused_until_the_terminal_exists() {
    let world = World::new();
    let paying = paying(&world, 0x8A);
    let sale = paying.cart().id;
    begun(&world, &paying, key(12));

    let mut with_card = in_flight(InFlight::Tendering(paying), key(12), later(40));
    with_card.card_op = Some(CardOpRef {
        sale_ref: "SYNTH-TERMINAL-REF-1".to_owned(),
        amount: jod(1_000),
        started_at: later(39),
    });
    let refused = advanced(&world, &with_card, 0);
    assert!(
        matches!(refused, Err(DbError::CheckoutCardOpNotJournalled(s)) if s == sale),
        "{refused:?}"
    );

    world
        .conn
        .execute(
            "UPDATE checkout_operation SET terminal_sale_ref = 'SYNTH-TERMINAL-REF-1'",
            [],
        )
        .unwrap();
    let unread = journal(&world).get(sale);
    assert!(
        matches!(&unread, Err(DbError::StoredCheckoutInvalid { reason })
            if reason.contains("card operation")),
        "{unread:?}"
    );
}

/// A journal row this build cannot read is refused, naming the sale and what
/// is wrong, and never quoting the snapshot, which carries the buyer's name.
#[test]
fn a_journal_row_this_build_cannot_read_is_refused_without_quoting_it() {
    let world = World::new();
    let mut cart = world.cart(0x8C);
    cart.lines = vec![world.line(0x8D, world.item, 1_000, 1_000)];
    cart.buyer_tin = Some(BUYER_TIN.to_owned());
    cart.buyer_name = Some(BUYER_NAME.to_owned());
    let paying = world.tendering(cart);
    let sale = paying.cart().id;
    begun(&world, &paying, key(13));
    let manager = uuid(MANAGER).as_bytes().to_vec();
    let other_sale = SaleId::from_uuid(uuid(0x8E));

    for (damage, says) in [
        (
            "UPDATE checkout_operation SET priced_snapshot = substr(priced_snapshot, 1, 60)",
            "cut short",
        ),
        (
            "UPDATE checkout_operation SET priced_snapshot = json_object('buyer_name', 'Synthetic Trading Est.')",
            "not a sale this build reads",
        ),
        (
            "UPDATE checkout_operation SET actor_id = ?1",
            "actor_id is not its cart's",
        ),
        (
            "UPDATE checkout_operation SET sale_id = ?2",
            "sale_id is not its cart's",
        ),
        (
            "UPDATE checkout_operation SET idempotency_key = 'not-a-key'",
            "is not a UUID",
        ),
        (
            "UPDATE checkout_operation SET terminal_state = 'unknown'",
            "card operation",
        ),
    ] {
        let tx = world.conn.unchecked_transaction().unwrap();
        let mut statement = tx.prepare(damage).unwrap();
        let wanted = statement.parameter_count();
        let values: [&dyn rusqlite::ToSql; 2] =
            [&manager, &other_sale.as_uuid().as_bytes().to_vec()];
        statement
            .execute(rusqlite::params_from_iter(values.into_iter().take(wanted)))
            .unwrap();
        drop(statement);

        let read = CheckoutOperationRepository::new(&tx).in_flight(register());
        let Err(DbError::StoredCheckoutInvalid { reason }) = &read else {
            panic!("{damage}: {read:?}");
        };
        assert!(reason.contains(says), "{damage}: {reason}");
        let shown = read.unwrap_err().to_string();
        assert!(
            !shown.contains(BUYER_NAME) && !shown.contains(BUYER_TIN),
            "{damage}: the refusal quotes the snapshot"
        );
        drop(tx);
    }
    assert!(
        journal(&world).get(sale).unwrap().is_some(),
        "every damage was rolled back"
    );
}
