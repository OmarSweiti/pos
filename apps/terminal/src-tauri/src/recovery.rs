//! Start-up recovery, and what it leaves for a till to take up (microstep
//! 1.8.4, its start-up recovery half).
//!
//! **Before the first command,** [`recover`] puts right what a crash left, in
//! the order the steps before it asked:
//! - sessions a crash left open are ended as `crash`;
//! - every basket a crash left being worked on is claimed by this process and
//!   set aside in the parked list, or dropped if it holds nothing (1.4.11);
//! - the stock cache is held to its ledger and rebuilt where it is off
//!   (1.10.3's note).
//!
//! **A sale journalled mid-payment is left where it is,** its basket claimed
//! by this process and not parked, and counted in
//! [`Recovered::sales_in_flight`] (the owner's decision of 7 November 2026).
//! Start-up completes nothing on its own, by the owner's decision of
//! 9 November 2026. A sale whose finalize had begun is not finished with no
//! one at the till: its receipt would be rendered before a printer is
//! attached, and its audit row would need an actor no one is.
//!
//! **A cashier takes each one up,** with `sale_resume_in_flight`
//! ([`crate::commands::tender::resume_journalled`]). What it becomes is the
//! domain's `resume_in_flight`, read from the journal alone:
//! - a checkout with nothing taken is abandoned, audited, and its basket is
//!   built again;
//! - one with tenders taken returns to the tender screen with every one;
//! - one whose finalize had begun is completed by `sale_finalize`, which
//!   replays it under the journal's own idempotency key
//!   (`commands::sale::finalize`).
//!
//! The tests below restart a register on the same file in every state the
//! journal persists, which is what `cargo nextest run -p terminal recovery::`
//! runs.

use pos_db::DbError;
use pos_db::repo::checkout_operation::CheckoutOperationRepository;
use pos_db::repo::parked_cart::{ParkedCartRepository, SetAside};
use pos_db::repo::stock::{CacheDrift, StockRepository};
use pos_db::repo::user::{SessionEnd, UserRepository};
use pos_domain::{RegisterId, SaleId, StoreId, Timestamp};
use rusqlite::Connection;
use uuid::Uuid;

use crate::state::StartupError;

/// What start-up recovery found and did, before the first command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovered {
    /// Sessions a crash left open, now ended with `end_reason = 'crash'`.
    pub sessions_closed: usize,
    /// Baskets a crash left being worked on, now waiting in the parked list.
    /// The session that was building each one died with the process, so none
    /// is handed to whoever signs in next without being resumed.
    pub carts_set_aside: usize,
    /// Working carts a crash left with nothing in them, dropped.
    pub empty_carts_dropped: usize,
    /// Sales a restart found journalled mid-payment. Each is left where it
    /// is, its basket claimed and not parked (the owner's decision of
    /// 7 November 2026), for a cashier to take up with
    /// `sale_resume_in_flight`; `sale_in_flight_state` names them. A journal
    /// row this build cannot read is counted too.
    pub sales_in_flight: usize,
    pub stock_cache: CacheCheck,
}

/// What holding the stock cache to its ledger found (1.10.3's note).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheCheck {
    /// The cache is what the ledger says.
    True,
    /// It was not, and was rebuilt in the same transaction. An alarm.
    Rebuilt(Vec<CacheDrift>),
    /// The ledger holds another register's events, which a register's own
    /// database never should. The cache was left as it was. An alarm.
    SpansRegisters,
}

/// Start-up recovery, before the first command.
///
/// Sessions and carts first, in one transaction. Every basket a crash left
/// being worked on is claimed and set aside in the parked list, where whoever
/// signs in can resume it, and one with nothing in it is dropped: the session
/// building it died with the process (1.4.11). A basket whose sale is
/// journalled mid-payment is claimed and left where it is, for a cashier to
/// take up from its journal (the owner's decisions of 7 and 9 November 2026):
/// parked, it could never be paid for again. Then the stock cache, in a
/// transaction of its own, because a cache that spans registers is an alarm
/// that leaves the cache as it was, not a reason to undo the rest.
pub(crate) fn recover(
    conn: &mut Connection,
    register: RegisterId,
    store: StoreId,
    nonce: Uuid,
    at: Timestamp,
) -> Result<Recovered, StartupError> {
    let failed = |error: DbError| StartupError::Recovery(error);
    let sqlite = |error: rusqlite::Error| StartupError::Recovery(error.into());

    let tx = conn.transaction().map_err(sqlite)?;
    let sessions_closed = UserRepository::new(&tx)
        .end_open_sessions(&tx, register, SessionEnd::Crash, at)
        .map_err(failed)?;
    let carts = ParkedCartRepository::new(&tx);
    let journal = CheckoutOperationRepository::new(&tx);
    let (mut carts_set_aside, mut empty_carts_dropped, mut sales_in_flight) = (0, 0, 0);
    for cart in carts
        .reclaim_after_restart(&tx, register, nonce)
        .map_err(failed)?
    {
        if journalled(&journal, cart.id).map_err(failed)? {
            sales_in_flight += 1;
            continue;
        }
        match carts.set_aside(&tx, nonce, &cart, at).map_err(failed)? {
            SetAside::Parked => carts_set_aside += 1,
            SetAside::Dropped => empty_carts_dropped += 1,
        }
    }
    tx.commit().map_err(sqlite)?;

    let tx = conn.transaction().map_err(sqlite)?;
    let checked = StockRepository::new(&tx).reconcile_cache(&tx, store);
    let stock_cache = match checked {
        Ok(drift) => {
            tx.commit().map_err(sqlite)?;
            if drift.is_empty() {
                CacheCheck::True
            } else {
                tracing::warn!(
                    drift = ?drift,
                    "start-up found the stock cache off its ledger and rebuilt it"
                );
                CacheCheck::Rebuilt(drift)
            }
        }
        Err(DbError::StockReplayAcrossRegisters) => {
            drop(tx);
            tracing::warn!(
                "start-up found another register's stock events; the cache is unchanged"
            );
            CacheCheck::SpansRegisters
        }
        Err(other) => return Err(failed(other)),
    };

    if sessions_closed > 0 {
        tracing::warn!(
            sessions_closed,
            "start-up closed sessions a crash left open"
        );
    }
    if carts_set_aside > 0 {
        tracing::warn!(
            carts_set_aside,
            "start-up set aside baskets a crash left being worked on"
        );
    }
    if sales_in_flight > 0 {
        tracing::warn!(
            sales_in_flight,
            "start-up left sales a crash interrupted mid-payment for recovery"
        );
    }
    Ok(Recovered {
        sessions_closed,
        carts_set_aside,
        empty_carts_dropped,
        sales_in_flight,
        stock_cache,
    })
}

/// Whether `sale` is journalled mid-payment. A row this build cannot read is
/// a sale in payment all the same: its basket is left where it is rather than
/// parked beside a journal that still names it.
fn journalled(journal: &CheckoutOperationRepository<'_>, sale: SaleId) -> Result<bool, DbError> {
    match journal.get(sale) {
        Ok(found) => Ok(found.is_some()),
        Err(DbError::StoredCheckoutInvalid { .. }) => Ok(true),
        Err(other) => Err(other),
    }
}

#[cfg(test)]
mod tests {
    //! 1.8.4's named tests that run in Phase 1, and `sale_resume_in_flight`'s
    //! own. `an_interrupted_tendering_is_recovered_and_status_queried` and
    //! `a_card_approval_before_a_power_cut_is_found_and_attached` moved to
    //! 2.1.3 by the owner's decision of 9 November 2026: no Phase-1 tender
    //! carries a card operation.
    //!
    //! A power cut is a register dropped where it stands, with a finalize
    //! stopped at a write point by the fault seam so its transaction never
    //! commits; the restart is a new register opened on the same file, as
    //! `run` opens it. The shop is the integration suites' (`tests/shop`):
    //! oil at 1.250 and 101 g of cheese at 8.500 a kilogram come to 2.108,
    //! which the store's coin step asks of cash as 2.110. Every figure is
    //! worked out by hand.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::path::Path;

    use pos_db::repo::sale::SaleRepository;
    use pos_domain::cart::BASKET_ENTITY;
    use pos_domain::{InFlight, TenderId, UserId};
    use pos_test_support::io_proptest_config;
    use proptest::prelude::*;
    use rusqlite::{OptionalExtension, params};

    use super::*;
    use crate::commands::auth::login_pin;
    use crate::commands::cart::{add_product, current, open_cart};
    use crate::commands::sale::{
        FinalizeFaults, FinalizeWritePoint, Injected, finalize, finalize_with,
    };
    use crate::commands::tender::{
        ABANDON_ACTION, ResumedSale, add_cash, begin_payment, in_flight, resume_journalled,
    };
    use crate::shop::{
        CASHIER_CODE, CASHIER_ID, CASHIER_PIN, CHEESE, KEY, OIL, OWNER_CODE, OWNER_ID, SECOND_CODE,
        SECOND_ID, add_staff, audited, field, open, product_id, refusal, seed,
    };
    use crate::state::{DATABASE_FILE, Register, Working};

    /// What the basket costs, and what the coin step asks of cash for it.
    const TOTAL: i64 = 2_108;
    const CASH_DUE: i64 = 2_110;

    /// Where a power cut stops a checkout: each state the journal persists,
    /// and the moment after the commit.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Cut {
        /// Payment begun, nothing taken.
        Begun,
        /// 1.000 taken of 2.110.
        PartPaid,
        /// Settled by 1.000 and 5.000, before the finalize.
        Settled,
        /// The finalize stopped just before this write point.
        Finalize(FinalizeWritePoint),
        /// Committed, before the printer and the drawer.
        Committed,
    }

    /// A power cut just before `0`: the finalize stops there, and its
    /// transaction never commits.
    struct CutAt(FinalizeWritePoint);

    impl FinalizeFaults for CutAt {
        fn before(&self, point: FinalizeWritePoint, _conn: &Connection) -> Result<(), Injected> {
            if point == self.0 {
                return Err(Injected(point));
            }
            Ok(())
        }
    }

    /// What the finalize's own transaction holds for `sale` the moment before
    /// it commits, and then a power cut there.
    struct LastMoment {
        sale: SaleId,
        seen: RefCell<Option<(bool, bool)>>,
    }

    impl FinalizeFaults for LastMoment {
        fn before(&self, point: FinalizeWritePoint, conn: &Connection) -> Result<(), Injected> {
            if point != FinalizeWritePoint::Commit {
                return Ok(());
            }
            let journalled = CheckoutOperationRepository::new(conn)
                .get(self.sale)
                .unwrap()
                .is_some();
            let complete = SaleRepository::new(conn)
                .by_id(self.sale)
                .unwrap()
                .is_some();
            *self.seen.borrow_mut() = Some((journalled, complete));
            Err(Injected(point))
        }
    }

    /// A register file in `dir` with a second cashier, its first cashier
    /// signed in.
    fn shop_in(dir: &Path) -> Register {
        seed(dir, true);
        let register = open(dir);
        add_staff(
            &register,
            SECOND_ID,
            SECOND_CODE,
            "Synthetic Second Cashier",
            "cashier",
        );
        login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
        register
    }

    /// The process dies where it stands, and a new one opens the same file
    /// and signs `code` in.
    fn restart(dir: &Path, register: Register, code: &str) -> Register {
        drop(register);
        let register = open(dir);
        login_pin(&register, code, CASHIER_PIN).unwrap();
        register
    }

    /// The basket, its payment begun.
    fn basket(register: &Register) -> SaleId {
        open_cart(register).unwrap();
        add_product(register, product_id(OIL), None).unwrap();
        add_product(register, product_id(CHEESE), Some(101)).unwrap();
        let paying = begin_payment(register).unwrap();
        assert_eq!(paying.total_minor, TOTAL);
        paying.sale_id
    }

    /// The checkout taken as far as `cut`.
    fn run_until(register: &Register, cut: Cut) -> SaleId {
        let sale = basket(register);
        if cut == Cut::Begun {
            return sale;
        }
        add_cash(register, 1_000).unwrap();
        if cut == Cut::PartPaid {
            return sale;
        }
        assert!(add_cash(register, 5_000).unwrap().settled);
        match cut {
            Cut::Finalize(point) => {
                let failed = refusal(finalize_with(register, &CutAt(point)));
                assert_eq!(failed.code, "storage_failed");
            }
            Cut::Committed => {
                finalize(register).unwrap();
            }
            _ => {}
        }
        sale
    }

    /// The journal's row for `sale`: its state, key and version.
    fn journal(register: &Register, sale: SaleId) -> Option<(&'static str, Uuid, i64)> {
        register.with_db(|conn| {
            CheckoutOperationRepository::new(conn)
                .get(sale)
                .unwrap()
                .map(|operation| {
                    let state = match operation.in_flight.state {
                        InFlight::Tendering(_) => "tendering",
                        InFlight::Finalizing(_) => "finalizing",
                    };
                    (
                        state,
                        operation.in_flight.idempotency_key,
                        operation.version,
                    )
                })
        })
    }

    fn completed(register: &Register, sale: SaleId) -> bool {
        register.with_db(|conn| SaleRepository::new(conn).by_id(sale).unwrap().is_some())
    }

    /// Each stock event `sale` posted, as `(product, quantity)`, by product.
    fn stock_events(register: &Register, sale: SaleId) -> Vec<(Uuid, i64)> {
        register.with_db(|conn| {
            let mut events: Vec<(Uuid, i64)> = conn
                .prepare(
                    "SELECT product_id, qty_delta_milli FROM stock_ledger
                      WHERE kind = 'sale' AND ref_id = ?1",
                )
                .unwrap()
                .query_map([sale.as_uuid().as_bytes().as_slice()], |row| {
                    Ok((
                        Uuid::from_slice(&row.get::<_, Vec<u8>>(0)?).unwrap(),
                        row.get(1)?,
                    ))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            events.sort();
            events
        })
    }

    /// What the cache holds on hand of product `n`.
    fn on_hand(register: &Register, n: u128) -> i64 {
        register.with_db(|conn| {
            conn.query_row(
                "SELECT on_hand_milli FROM stock_cache WHERE product_id = ?1",
                [product_id(n).as_uuid().as_bytes().as_slice()],
                |row| row.get(0),
            )
            .unwrap()
        })
    }

    /// Every row a sale's finalize and its delivery write, by table, and the
    /// envelope's members by entity: what one finalize adds, counted.
    fn tally(register: &Register) -> BTreeMap<String, i64> {
        const TABLES: [&str; 11] = [
            "sale",
            "sale_line",
            "sale_tender",
            "tender_status_event",
            "stock_ledger",
            "receipt_artifact",
            "print_job",
            "audit_log",
            "sync_commit",
            "fact_commit_member",
            "sync_outbox",
        ];
        register.with_db(|conn| {
            let mut counts: BTreeMap<String, i64> = TABLES
                .iter()
                .map(|table| {
                    let n = conn
                        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                            row.get(0)
                        })
                        .unwrap();
                    ((*table).to_owned(), n)
                })
                .collect();
            let members = conn
                .prepare("SELECT entity, count(*) FROM fact_commit_member GROUP BY entity")
                .unwrap()
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            for (entity, n) in members {
                counts.insert(format!("member {entity}"), n);
            }
            counts
        })
    }

    /// The delivery rows of each member of the one commit that carries
    /// `sale`, and how many commits carry it.
    fn deliveries(register: &Register, sale: SaleId) -> (i64, Vec<i64>) {
        register.with_db(|conn| {
            let id = sale.as_uuid().as_bytes().to_vec();
            let commits: i64 = conn
                .query_row(
                    "SELECT count(DISTINCT commit_id) FROM fact_commit_member
                      WHERE entity = 'sale' AND entity_id = ?1",
                    [&id],
                    |row| row.get(0),
                )
                .unwrap();
            let rows = conn
                .prepare(
                    "SELECT (SELECT count(*) FROM sync_outbox o WHERE o.change_id = m.change_id)
                       FROM fact_commit_member m
                      WHERE m.commit_id = (SELECT commit_id FROM fact_commit_member
                                            WHERE entity = 'sale' AND entity_id = ?1)",
                )
                .unwrap()
                .query_map([&id], |row| row.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (commits, rows)
        })
    }

    /// The tenders of the completed `sale`, as `(id, amount)`, by id.
    fn sale_tenders(register: &Register, sale: SaleId) -> Vec<(Uuid, i64)> {
        register.with_db(|conn| {
            let mut tenders: Vec<(Uuid, i64)> = conn
                .prepare("SELECT id, amount_minor FROM sale_tender WHERE sale_id = ?1")
                .unwrap()
                .query_map([sale.as_uuid().as_bytes().as_slice()], |row| {
                    Ok((
                        Uuid::from_slice(&row.get::<_, Vec<u8>>(0)?).unwrap(),
                        row.get(1)?,
                    ))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            tenders.sort();
            tenders
        })
    }

    /// The entity and id each audit row under `action` names, in order.
    fn named(register: &Register, action: &str) -> Vec<(String, Uuid)> {
        register.with_db(|conn| {
            conn.prepare("SELECT entity, entity_id FROM audit_log WHERE action = ?1 ORDER BY rowid")
                .unwrap()
                .query_map([action], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
                })
                .unwrap()
                .map(|row| {
                    let (entity, id) = row.unwrap();
                    (entity, Uuid::from_slice(&id).unwrap())
                })
                .collect()
        })
    }

    fn cashier() -> UserId {
        UserId::from_uuid(Uuid::from_bytes(CASHIER_ID))
    }

    fn second() -> UserId {
        UserId::from_uuid(Uuid::from_bytes(SECOND_ID))
    }

    /// Every committed state holds the journal's row or the completed sale,
    /// never both and never neither. A power cut before any write point leaves
    /// the row and no sale, before the restart and after it; the row goes in
    /// the transaction that completes the sale, which the last moment before
    /// its commit shows; and a restart after the commit finds nothing in
    /// flight.
    #[test]
    fn a_checkout_operation_row_never_outlives_its_commit() {
        for &point in FinalizeWritePoint::ALL {
            let dir = tempfile::tempdir().unwrap();
            let register = shop_in(dir.path());
            let sale = run_until(&register, Cut::Finalize(point));
            assert!(journal(&register, sale).is_some(), "at {point:?}");
            assert!(!completed(&register, sale), "at {point:?}");
            let register = restart(dir.path(), register, CASHIER_CODE);
            assert_eq!(register.recovered().sales_in_flight, 1, "at {point:?}");
            assert!(journal(&register, sale).is_some(), "at {point:?}");
            assert!(!completed(&register, sale), "at {point:?}");
            resume_journalled(&register, sale).unwrap();
            finalize(&register).unwrap();
            assert_eq!(journal(&register, sale), None, "at {point:?}");
            assert!(completed(&register, sale), "at {point:?}");
        }

        // Inside the finalize's transaction, the moment before it commits, the
        // row is already gone and the sale complete: the two commit together.
        // A power cut there lands neither.
        let dir = tempfile::tempdir().unwrap();
        let register = shop_in(dir.path());
        let sale = run_until(&register, Cut::Settled);
        let last = LastMoment {
            sale,
            seen: RefCell::new(None),
        };
        assert!(finalize_with(&register, &last).is_err());
        assert_eq!(*last.seen.borrow(), Some((false, true)));
        let register = restart(dir.path(), register, CASHIER_CODE);
        assert_eq!(
            journal(&register, sale).map(|(state, _, _)| state),
            Some("finalizing")
        );
        assert!(!completed(&register, sale));

        // After the commit, a restart finds the sale complete and nothing
        // journalled.
        let dir = tempfile::tempdir().unwrap();
        let register = shop_in(dir.path());
        let sale = run_until(&register, Cut::Committed);
        let register = restart(dir.path(), register, CASHIER_CODE);
        assert_eq!(register.recovered().sales_in_flight, 0);
        assert_eq!(journal(&register, sale), None);
        assert!(completed(&register, sale));
        assert!(in_flight(&register).unwrap().is_empty());
    }

    /// A finalize a power cut stopped is taken up under the journal's own
    /// idempotency key and version, and completed without the journal being
    /// advanced again, even when a second power cut stops the replay too.
    #[test]
    fn finalize_replays_under_the_same_idempotency_key() {
        let dir = tempfile::tempdir().unwrap();
        let register = shop_in(dir.path());
        let sale = run_until(&register, Cut::Finalize(FinalizeWritePoint::SaleRows));
        let (state, key, version) = journal(&register, sale).unwrap();
        assert_eq!(state, "finalizing");

        let register = restart(dir.path(), register, CASHIER_CODE);
        let ResumedSale::Finalizing { payment } = resume_journalled(&register, sale).unwrap()
        else {
            panic!("a finalize begun resumes as one");
        };
        assert!(payment.settled);
        assert_eq!(
            payment
                .tenders
                .iter()
                .map(|tender| tender.amount_minor)
                .collect::<Vec<_>>(),
            vec![1_000, 5_000]
        );
        match register.lock_till().working.as_ref() {
            Some(Working::Paying(paying)) => {
                assert_eq!((paying.idempotency_key, paying.version), (key, version));
            }
            other => panic!("the till holds {other:?}"),
        }

        // A second power cut, during the replay: the journal is where it was.
        assert!(finalize_with(&register, &CutAt(FinalizeWritePoint::Commit)).is_err());
        assert_eq!(journal(&register, sale), Some(("finalizing", key, version)));

        let register = restart(dir.path(), register, CASHIER_CODE);
        resume_journalled(&register, sale).unwrap();
        let done = finalize(&register).unwrap();
        assert_eq!(done.receipt_number, "REG01-000001");
        assert_eq!((done.total_minor, done.change_minor), (CASH_DUE, 3_890));
        assert!(completed(&register, sale));
        assert_eq!(journal(&register, sale), None);
    }

    /// Whichever write point a power cut stops the finalize at, the sale,
    /// taken up after the restart and completed, posts each line's stock event
    /// once: a bottle of oil and 101 g of cheese leave the shelf, and the
    /// cache holds what its ledger says.
    #[test]
    fn interrupted_finalize_resumes_without_double_stock_event() {
        let mut expected = vec![
            (product_id(OIL).as_uuid(), -1_000),
            (product_id(CHEESE).as_uuid(), -101),
        ];
        expected.sort();
        for &point in FinalizeWritePoint::ALL {
            let dir = tempfile::tempdir().unwrap();
            let register = shop_in(dir.path());
            let sale = run_until(&register, Cut::Finalize(point));
            assert!(stock_events(&register, sale).is_empty(), "at {point:?}");
            let register = restart(dir.path(), register, CASHIER_CODE);
            resume_journalled(&register, sale).unwrap();
            finalize(&register).unwrap();
            assert_eq!(stock_events(&register, sale), expected, "at {point:?}");
            assert_eq!(on_hand(&register, OIL), 9_000, "at {point:?}");
            assert_eq!(on_hand(&register, CHEESE), 399, "at {point:?}");
            let drift = register.with_db(|conn| {
                let tx = conn.transaction().unwrap();
                StockRepository::new(&tx)
                    .verify_cache(&tx, register.store())
                    .unwrap()
            });
            assert!(drift.is_empty(), "at {point:?}: {drift:?}");
        }
    }

    /// Whichever write point a power cut stops the finalize at, the sale,
    /// taken up after the restart and completed, travels in one commit, each
    /// of whose members has exactly one delivery row. And the register holds
    /// exactly what the same sale holds when its finalize was never cut: the
    /// stopped attempt left no commit, member or delivery row behind.
    #[test]
    fn interrupted_finalize_resumes_without_double_outbox_row() {
        let reference = {
            let dir = tempfile::tempdir().unwrap();
            let register = shop_in(dir.path());
            let sale = run_until(&register, Cut::Settled);
            let register = restart(dir.path(), register, CASHIER_CODE);
            resume_journalled(&register, sale).unwrap();
            finalize(&register).unwrap();
            tally(&register)
        };
        for &point in FinalizeWritePoint::ALL {
            let dir = tempfile::tempdir().unwrap();
            let register = shop_in(dir.path());
            let sale = run_until(&register, Cut::Finalize(point));
            let register = restart(dir.path(), register, CASHIER_CODE);
            resume_journalled(&register, sale).unwrap();
            finalize(&register).unwrap();
            let (commits, rows) = deliveries(&register, sale);
            assert_eq!(commits, 1, "at {point:?}");
            assert!(!rows.is_empty(), "at {point:?}");
            assert!(rows.iter().all(|&n| n == 1), "at {point:?}: {rows:?}");
            assert_eq!(tally(&register), reference, "at {point:?}");
        }
    }

    /// A checkout with nothing taken is abandoned when it is taken up, and
    /// audited, and its basket comes back to the till being built, to be paid
    /// for afresh.
    #[test]
    fn a_checkout_with_nothing_taken_comes_back_to_its_basket() {
        let dir = tempfile::tempdir().unwrap();
        let register = shop_in(dir.path());
        let sale = run_until(&register, Cut::Begun);
        let register = restart(dir.path(), register, CASHIER_CODE);
        assert_eq!(register.recovered().sales_in_flight, 1);

        let ResumedSale::Building { cart } = resume_journalled(&register, sale).unwrap() else {
            panic!("a checkout with nothing taken comes back to its basket");
        };
        assert_eq!((cart.sale_id, cart.lines.len()), (sale, 2));
        assert_eq!(cart.total_minor, TOTAL);
        assert_eq!(journal(&register, sale), None);
        let abandoned = audited(&register, ABANDON_ACTION);
        assert_eq!(abandoned.len(), 1);
        let row = abandoned.first().unwrap();
        assert_eq!(field(row, "reason"), "nothing_collected");
        assert_eq!(*field(row, "total_minor"), TOTAL);
        assert_eq!(
            field(row, "cashier_id"),
            &serde_json::Value::String(cashier().as_uuid().to_string())
        );
        assert!(audited(&register, "sale.resume").is_empty(), "their own");

        let paying = begin_payment(&register).unwrap();
        assert_eq!(paying.sale_id, sale);
        add_cash(&register, CASH_DUE).unwrap();
        let done = finalize(&register).unwrap();
        assert_eq!(
            (done.receipt_number.as_str(), done.change_minor),
            ("REG01-000001", 0)
        );
    }

    /// A sale with money taken comes back to the tender screen with every
    /// tender, and is asked only for what remains.
    #[test]
    fn a_part_paid_sale_resumes_with_every_tender() {
        let dir = tempfile::tempdir().unwrap();
        let register = shop_in(dir.path());
        let sale = run_until(&register, Cut::PartPaid);
        let register = restart(dir.path(), register, CASHIER_CODE);
        let ResumedSale::Tendering { payment } = resume_journalled(&register, sale).unwrap() else {
            panic!("a part-paid sale resumes at the tender screen");
        };
        assert_eq!(
            (
                payment.collected_minor,
                payment.remaining_minor,
                payment.cash_due_minor
            ),
            (1_000, 1_108, 1_110)
        );
        assert_eq!(payment.tenders.len(), 1);
        assert!(audited(&register, "sale.resume").is_empty(), "their own");
        assert!(add_cash(&register, 1_110).unwrap().settled);
        let done = finalize(&register).unwrap();
        assert_eq!((done.total_minor, done.change_minor), (CASH_DUE, 0));
        assert_eq!(sale_tenders(&register, sale).len(), 2);
    }

    /// Another cashier may take up a sale a crash left: with money taken it
    /// stays the first cashier's, whose tenders it holds, and the takeover is
    /// audited; with nothing taken its basket becomes theirs, as a parked one
    /// would, audited the same way. Either way each row names the basket, and
    /// the sale still completes.
    #[test]
    fn another_cashiers_sale_is_taken_over_and_audited() {
        let dir = tempfile::tempdir().unwrap();
        let register = shop_in(dir.path());
        let sale = run_until(&register, Cut::PartPaid);
        let register = restart(dir.path(), register, SECOND_CODE);
        resume_journalled(&register, sale).unwrap();
        let taken = audited(&register, "sale.resume");
        assert_eq!(taken.len(), 1);
        let row = taken.first().unwrap();
        assert_eq!(
            field(row, "cashier_id"),
            &serde_json::Value::String(cashier().as_uuid().to_string())
        );
        assert_eq!(field(row, "stage"), "tendering");
        assert_eq!(*field(row, "tender_count"), 1);
        assert_eq!(*field(row, "collected_minor"), 1_000);
        assert_eq!(
            named(&register, "sale.resume"),
            vec![(BASKET_ENTITY.to_owned(), sale.as_uuid())],
            "on the basket, so the finalize can complete it (#397)"
        );
        add_cash(&register, 1_110).unwrap();
        finalize(&register).unwrap();
        let cashier_of_sale: Vec<u8> = register.with_db(|conn| {
            conn.query_row(
                "SELECT cashier_id FROM sale WHERE id = ?1",
                [sale.as_uuid().as_bytes().as_slice()],
                |row| row.get(0),
            )
            .unwrap()
        });
        assert_eq!(cashier_of_sale, CASHIER_ID.to_vec(), "it stays theirs");
        let finalized: Vec<Vec<u8>> = register.with_db(|conn| {
            conn.prepare("SELECT actor_id FROM audit_log WHERE action = 'sale.finalize'")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect()
        });
        assert_eq!(
            finalized,
            vec![SECOND_ID.to_vec()],
            "finalized by the second"
        );

        let dir = tempfile::tempdir().unwrap();
        let register = shop_in(dir.path());
        let sale = run_until(&register, Cut::Begun);
        let register = restart(dir.path(), register, SECOND_CODE);
        resume_journalled(&register, sale).unwrap();
        assert_eq!(audited(&register, ABANDON_ACTION).len(), 1);
        let taken = audited(&register, "sale.resume");
        assert_eq!(taken.len(), 1);
        assert_eq!(*field(taken.first().unwrap(), "line_count"), 2);
        let on_till = register
            .lock_till()
            .working
            .as_ref()
            .map(|w| w.cart().cashier_id);
        assert_eq!(on_till, Some(second()), "the basket is theirs now");
        for action in [ABANDON_ACTION, "sale.resume"] {
            assert_eq!(
                named(&register, action),
                vec![(BASKET_ENTITY.to_owned(), sale.as_uuid())],
                "{action} names the basket"
            );
        }
        begin_payment(&register).unwrap();
        add_cash(&register, CASH_DUE).unwrap();
        finalize(&register).unwrap();
        assert!(completed(&register, sale), "and it still completes");
        assert_eq!(journal(&register, sale), None);
    }

    /// The till's own basket is set aside before a sale is taken up, and a
    /// resume that cannot be made changes nothing: no such sale journalled
    /// here, a till already paying, a journal row this build cannot read, and
    /// a role that may not resume.
    #[test]
    fn taking_up_a_sale_sets_the_till_aside_and_refuses_what_it_cannot_take() {
        let dir = tempfile::tempdir().unwrap();
        let register = shop_in(dir.path());
        let sale = run_until(&register, Cut::PartPaid);
        let register = restart(dir.path(), register, CASHIER_CODE);

        let nothing = SaleId::from_uuid(Uuid::from_u128(0xDEAD));
        assert_eq!(
            refusal(resume_journalled(&register, nothing)).code,
            "sale_not_in_flight"
        );

        // A basket being built goes to the parked list first.
        let building = open_cart(&register).unwrap();
        add_product(&register, product_id(OIL), None).unwrap();
        resume_journalled(&register, sale).unwrap();
        let parked: Option<String> = register.with_db(|conn| {
            conn.query_row(
                "SELECT state FROM parked_cart WHERE id = ?1",
                [building.sale_id.as_uuid().as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .unwrap()
        });
        assert_eq!(parked.as_deref(), Some("parked"));
        assert_eq!(current(&register).unwrap().unwrap().sale_id, sale);

        // A till paying for a sale takes up no other, nor this one again.
        assert_eq!(
            refusal(resume_journalled(&register, sale)).code,
            "sale_being_paid"
        );
        let journalled = journal(&register, sale);
        drop(register);

        // A journal row this build cannot read is named, not resumed.
        let conn = pos_db::open(&dir.path().join(DATABASE_FILE), KEY).unwrap();
        conn.execute(
            "UPDATE checkout_operation SET priced_snapshot = '{}' WHERE sale_id = ?1",
            params![sale.as_uuid().as_bytes().as_slice()],
        )
        .unwrap();
        drop(conn);
        let register = open(dir.path());
        login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
        assert_eq!(
            refusal(resume_journalled(&register, sale)).code,
            "checkout_unreadable"
        );
        assert!(register.lock_till().working.is_none());
        assert!(journalled.is_some());

        // An owner sells nothing, so resumes nothing.
        let dir = tempfile::tempdir().unwrap();
        let register = shop_in(dir.path());
        let sale = run_until(&register, Cut::PartPaid);
        drop(register);
        let register = open(dir.path());
        add_staff(&register, OWNER_ID, OWNER_CODE, "Synthetic Owner", "owner");
        login_pin(&register, OWNER_CODE, CASHIER_PIN).unwrap();
        assert_eq!(
            refusal(resume_journalled(&register, sale)).code,
            "permission_denied"
        );
        assert!(journal(&register, sale).is_some());
    }

    /// Where a generated checkout is left when the till's memory is lost.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Left {
        /// Wherever its cash left it, with or without a tender taken.
        AsPaid,
        /// Settled, before the finalize.
        Settled,
        /// The finalize begun and stopped. Every write point from the receipt
        /// number on leaves the file exactly as this one does
        /// (`finalize_is_atomic_under_injected_failure`), and this one renders
        /// no receipt.
        Finalizing,
    }

    thread_local! {
        /// The register every case of the property sells at: one file, one
        /// cashier signed in, a new basket each case.
        static TILL: RefCell<Option<(tempfile::TempDir, Register)>> = const { RefCell::new(None) };
    }

    fn with_till<T>(work: impl FnOnce(&Register) -> T) -> T {
        TILL.with(|till| {
            let mut till = till.borrow_mut();
            let (_, register) = till.get_or_insert_with(|| {
                let dir = tempfile::tempdir().unwrap();
                let register = shop_in(dir.path());
                (dir, register)
            });
            work(register)
        })
    }

    /// The process's memory lost: the till holds nothing, and what was
    /// committed is all there is.
    fn forget(register: &Register) {
        *register.lock_till().working = None;
    }

    proptest! {
        #![proptest_config(io_proptest_config())]

        /// 1.8.4: however a checkout was paid for, and wherever it stood when
        /// the till's memory was lost, taking it up from its journal never
        /// asks for money twice. It carries exactly the tenders taken, under
        /// the journal's own key and version; it is asked only for what they
        /// left; a finalize begun takes no further tender; and only a checkout
        /// with nothing taken goes back to its basket.
        ///
        /// A real restart reopens the file and signs in again, which the
        /// example tests above do at every persisted state; here the till's
        /// memory is dropped instead, so 256 checkouts fit in the gate.
        #[test]
        fn prop_resume_never_produces_a_second_authorisation(
            // Cash of this many fils: a part payment, the final one, or one
            // the till refuses, which changes nothing.
            cash in proptest::collection::vec(0i64..=2_200, 0..=3),
            left in prop_oneof![Just(Left::AsPaid), Just(Left::Settled), Just(Left::Finalizing)],
        ) {
            with_till(|register| -> Result<(), TestCaseError> {
                forget(register);
                let sale = basket(register);
                let mut paid = None;
                for &amount in &cash {
                    if let Ok(answer) = add_cash(register, amount) {
                        paid = Some(answer);
                    }
                }
                if left != Left::AsPaid && !paid.as_ref().is_some_and(|answer| answer.settled) {
                    let due = paid.as_ref().map_or(CASH_DUE, |answer| answer.cash_due_minor);
                    paid = Some(add_cash(register, due).unwrap());
                }
                if left == Left::Finalizing {
                    let cut = CutAt(FinalizeWritePoint::ReceiptNumber);
                    prop_assert!(finalize_with(register, &cut).is_err());
                }
                let taken: Vec<(TenderId, i64)> = paid
                    .as_ref()
                    .map(|answer| {
                        answer.tenders.iter().map(|t| (t.tender_id, t.amount_minor)).collect()
                    })
                    .unwrap_or_default();
                let collected: i64 = taken.iter().map(|(_, amount)| amount).sum();
                let (state, key, version) = journal(register, sale).unwrap();

                forget(register);
                let resumed = resume_journalled(register, sale).unwrap();
                let finalizing = matches!(resumed, ResumedSale::Finalizing { .. });
                match resumed {
                    ResumedSale::Building { cart } => {
                        prop_assert!(taken.is_empty(), "a basket came back with money taken");
                        prop_assert_eq!(left, Left::AsPaid);
                        prop_assert_eq!(cart.sale_id, sale);
                        prop_assert_eq!(journal(register, sale), None);
                    }
                    ResumedSale::Tendering { payment } | ResumedSale::Finalizing { payment } => {
                        prop_assert!(!taken.is_empty());
                        prop_assert_eq!(finalizing, left == Left::Finalizing);
                        prop_assert_eq!(finalizing, state == "finalizing");
                        let carried: Vec<(TenderId, i64)> = payment
                            .tenders
                            .iter()
                            .map(|t| (t.tender_id, t.amount_minor))
                            .collect();
                        prop_assert_eq!(&carried, &taken, "the tenders carried are the ones taken");
                        prop_assert_eq!(payment.collected_minor, collected);
                        if payment.settled {
                            prop_assert_eq!(payment.remaining_minor, 0);
                            prop_assert_eq!(
                                refusal(add_cash(register, 1)).code,
                                "sale_settled",
                                "a settled sale takes no more money"
                            );
                        } else {
                            prop_assert_eq!(payment.remaining_minor, TOTAL - collected);
                        }
                        match register.lock_till().working.as_ref() {
                            Some(Working::Paying(paying)) => {
                                prop_assert_eq!((paying.idempotency_key, paying.version), (key, version));
                            }
                            other => {
                                return Err(TestCaseError::fail(format!("the till holds {other:?}")));
                            }
                        }
                        prop_assert_eq!(journal(register, sale), Some((state, key, version)));
                    }
                }
                Ok(())
            })?;
        }
    }
}
