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
