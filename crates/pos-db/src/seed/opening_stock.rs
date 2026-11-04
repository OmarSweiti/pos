//! Opening stock: what a store counted on the shelf before its first sale,
//! posted as ledger events (microstep 1.10.5).
//!
//! Every path that increases stock otherwise arrives in Phase 4, so without
//! this a register's first sale takes every product below zero and the
//! negative-stock flag is a false positive the merchant learns to ignore before
//! it ever means anything (`ref/ipc-contract.md`, the stock commands).
//!
//! This is a provisioning path, not the `stock_adjust` command, and needs no
//! approval. It posts through the same append path as every event: each row
//! becomes an `adjust` event with reason `opening_stock`, prepared from the
//! position the cache holds, under one delivery envelope, in the caller's
//! transaction. `stock_cache` is never written here; `0006`'s trigger projects
//! each event as it is appended (I-6).
//!
//! The file a merchant provides has exactly three columns: `product_id`,
//! `qty_milli` and `unit_cost_minor`, the last empty when no cost is known. A
//! missing cost is stored as `NULL` and marked estimated, never as zero,
//! because a zero would make the margin look known when it is not.

use pos_domain::{
    BusinessDate, Currency, IdSource, IncomingCost, Money, NewStockEvent, ProductId, Qty,
    RegisterId, StockAdjustmentReason, StockEventId, StockEventKind, StockSource, StoreId,
    Timestamp, UserId,
};
use rusqlite::{OptionalExtension, Transaction};
use uuid::Uuid;

use crate::DbError;
use crate::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use crate::repo::stock::{self, PreparedStockEvent, StockRepository};

/// The header an opening-stock file must start with, exactly.
pub const HEADER: &str = "product_id,qty_milli,unit_cost_minor";

/// One product's counted stock, and what it cost when that is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpeningStockRow {
    pub product_id: ProductId,
    pub qty_milli: i64,
    pub unit_cost_minor: Option<i64>,
}

/// Where and when an opening load is posted, and the ids it needs. Ids and
/// instants are arguments (conventions §5).
pub struct OpeningStockLoad<'a> {
    pub register: RegisterId,
    pub store: StoreId,
    /// The person who provisioned it, when one did.
    pub actor: Option<UserId>,
    pub occurred_at: Timestamp,
    pub business_date: BusinessDate,
    /// The one envelope every row's event travels in.
    pub envelope: CommitEnvelope<'a>,
    /// Each event's id and each manifest member's change id.
    pub ids: &'a dyn IdSource,
}

/// Read an opening-stock file: [`HEADER`], then one row per line.
///
/// # Errors
///
/// [`DbError::OpeningStockFileInvalid`] naming the first line that is not a
/// row of exactly three fields, a product id, an integer quantity and an
/// optional integer cost. The message never quotes the line.
pub fn parse_csv(text: &str) -> Result<Vec<OpeningStockRow>, DbError> {
    let mut lines = text
        .lines()
        .map(|line| line.strip_suffix('\r').unwrap_or(line));
    if lines.next() != Some(HEADER) {
        return Err(DbError::OpeningStockFileInvalid {
            line: 1,
            reason: "the header is not product_id,qty_milli,unit_cost_minor",
        });
    }
    let mut rows = Vec::new();
    for (index, line) in lines.enumerate() {
        let number = index.saturating_add(2);
        if line.is_empty() {
            continue;
        }
        let invalid = |reason| DbError::OpeningStockFileInvalid {
            line: number,
            reason,
        };
        let fields: Vec<&str> = line.split(',').collect();
        let [product, qty, cost] = fields.as_slice() else {
            return Err(invalid("a row has exactly three fields"));
        };
        let product = Uuid::parse_str(product).map_err(|_| invalid("product_id is not a UUID"))?;
        let qty_milli = qty
            .parse::<i64>()
            .map_err(|_| invalid("qty_milli is not an integer"))?;
        let unit_cost_minor = match *cost {
            "" => None,
            cost => Some(
                cost.parse::<i64>()
                    .map_err(|_| invalid("unit_cost_minor is not an integer"))?,
            ),
        };
        rows.push(OpeningStockRow {
            product_id: ProductId::from_uuid(product),
            qty_milli,
            unit_cost_minor,
        });
    }
    Ok(rows)
}

/// Post `rows` as opening stock, inside the caller's transaction, and return
/// how many events were appended.
///
/// Each row is an `adjust` event with reason `opening_stock`: an observed cost
/// when the row has one, an unknown one when it has none. The rows are
/// prepared in order, each from the position the one before it left, so two
/// rows of one product are two consecutive events. All of them travel in the
/// one envelope `load.envelope` describes.
///
/// # Errors
///
/// [`DbError::StockRefused`] for a row that is not stock arriving (a quantity
/// that is not above zero) or a cost the ledger cannot record against the
/// position, [`DbError::StockStoreUnknown`], [`DbError::StockProductUnknown`]
/// and [`DbError::StockQtyOffStep`] as for any stock event, and the errors of
/// the envelope writer. On any of them the caller's rollback leaves nothing
/// posted.
pub fn load_opening_stock(
    tx: &Transaction<'_>,
    load: &OpeningStockLoad<'_>,
    rows: impl IntoIterator<Item = OpeningStockRow>,
) -> Result<usize, DbError> {
    let currency = store_currency(tx, load.store)?;
    let movements: Vec<NewStockEvent> = rows
        .into_iter()
        .map(|row| NewStockEvent {
            id: StockEventId::from_uuid(load.ids.next()),
            product_id: row.product_id,
            store_id: load.store,
            kind: StockEventKind::Adjust,
            qty_delta: Qty::from_milli(row.qty_milli),
            incoming_cost: Some(row.unit_cost_minor.map_or(IncomingCost::Unknown, |minor| {
                IncomingCost::Observed(Money::from_minor(minor, currency))
            })),
            is_weight_derived: false,
            source: Some(StockSource::Adjustment {
                reason: StockAdjustmentReason::OpeningStock,
                note: None,
            }),
            actor_id: load.actor,
            occurred_at: load.occurred_at,
            business_date: load.business_date,
        })
        .collect();
    if movements.is_empty() {
        return Ok(0);
    }
    let stock = StockRepository::new(tx);
    let prepared = stock.prepare(tx, load.register, &movements)?;
    let ids: Vec<[u8; 16]> = prepared
        .iter()
        .map(|p| *p.event().id.as_uuid().as_bytes())
        .collect();
    let changes: Vec<[u8; 16]> = prepared
        .iter()
        .map(|_| *load.ids.next().as_bytes())
        .collect();
    let payloads: Vec<String> = prepared.iter().map(PreparedStockEvent::payload).collect();
    let members: Vec<FactMember<'_>> = ids
        .iter()
        .zip(&changes)
        .zip(&payloads)
        .map(|((entity_id, change_id), payload)| FactMember {
            change_id,
            entity: stock::ENTITY,
            entity_id,
            payload,
        })
        .collect();
    OutboxRepository::new(tx).write_commit(tx, &load.envelope, &members)?;
    stock.append(tx, &prepared)?;
    Ok(prepared.len())
}

fn store_currency(tx: &Transaction<'_>, store: StoreId) -> Result<Currency, DbError> {
    let code: Option<String> = tx
        .query_row(
            "SELECT currency FROM store WHERE id = ?1",
            [store.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    let code = code.ok_or(DbError::StockStoreUnknown(store))?;
    Currency::from_code(&code).map_err(|_| DbError::StoredStockRecordInvalid {
        reason: format!("store {store} has currency {code:?}"),
    })
}
