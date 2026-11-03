//! The stock ledger's writer and its projection (microstep 1.10.2,
//! `ref/schema.md` §0006).
//!
//! Stock is a ledger (I-6): on-hand is `SUM(qty_delta_milli)` over the
//! append-only `stock_ledger`, and `stock_cache` is that sum projected, with
//! the WAC beside it. `0006`'s `stock_ledger_projects_cache` trigger updates the
//! cache in the same statement as the insert, so the ledger row and its
//! projection cannot be committed apart. A partial cache update is not a
//! performance problem: it is a false on-hand figure shown to the cashier.
//!
//! An event is written in two steps, the way every fact with a delivery
//! envelope is (I-9, and the order 1.8.3's finalize follows):
//!
//! 1. **[`StockRepository::prepare`]** reads, through the caller's transaction,
//!    the position each product is in and this register's ledger head, and
//!    asks `pos_domain`'s [`StockEvent::capture`] for the event: the cost basis
//!    it captures and the position it leaves. Several movements prepare as one
//!    batch, each starting from the position the one before it left, so a sale
//!    with two lines of the same product records two consecutive events.
//!    [`PreparedStockEvent::payload`] is what the event travels as.
//! 2. The caller writes the envelope naming every prepared event with exactly
//!    that payload, then **[`StockRepository::append`]** inserts the events in
//!    the order they were prepared. It refuses an event its envelope does not
//!    describe, so the row and what travels cannot differ, and `0006`'s
//!    triggers check the chain, the quantity step and the cost basis again in
//!    the storage engine.
//!
//! A [`PreparedStockEvent`] can only be made by `prepare`. Its cost basis is the
//! domain's answer, and no caller can change it between the two steps.
//!
//! [`StockRepository::rebuild_cache`] replays a store's ledger into its cache;
//! [`StockRepository::on_hand`] and [`StockRepository::negative_stock`] read
//! the cache.
//!
//! Before a basket is tendered, [`StockRepository::screen_basket`] reads the
//! store's [`NegativeStockPolicy`] and the cache, and asks the domain which
//! products the basket would leave below zero (microstep 1.10.4). By default
//! that is a flag and the sale goes ahead; under the store's hard block it is a
//! refusal, before any money is taken. Finalize never screens: once a customer
//! has paid, the sale stands and its stock event records the position it left.
//!
//! The cache is a cache, so it is checked against the ledger rather than
//! trusted (microstep 1.10.3). [`StockRepository::verify_cache`] compares every
//! column of every row with a full replay and names each product on which they
//! differ; [`StockRepository::reconcile_cache`] is the check I-6 asks of startup
//! and of a periodic sweep, which rebuilds on any difference in the same
//! transaction as the comparison. The replay is one query, shared by the
//! rebuild and the comparison, so what is checked is what a rebuild writes.

use std::collections::{BTreeMap, BTreeSet};

use pos_domain::{
    Currency, Money, NegativeStockPolicy, NewStockEvent, ProductId, Qty, RegisterId, Shortfall,
    StockEvent, StockPosition, StockSource, StoreId,
};
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::Value;

use crate::DbError;

/// The entity name a stock event's envelope member carries, as
/// `ref/schema.md`'s fact-table list spells it.
pub const ENTITY: &str = "stock_ledger";

/// A stock event ready to be appended: the domain's event, plus the facts that
/// place it in this register's ledger.
///
/// Only [`StockRepository::prepare`] makes one, and its fields are private, so
/// the cost basis it carries is the one the domain captured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedStockEvent {
    register: RegisterId,
    event_seq: i64,
    qty_step_milli: i64,
    event: StockEvent,
}

impl PreparedStockEvent {
    /// The event, with its captured cost basis and the position it leaves.
    #[must_use]
    pub const fn event(&self) -> &StockEvent {
        &self.event
    }

    /// The register whose ledger the event extends.
    #[must_use]
    pub const fn register(&self) -> RegisterId {
        self.register
    }

    /// The event's place in that register's ledger: one more than the event
    /// before it.
    #[must_use]
    pub const fn event_seq(&self) -> i64 {
        self.event_seq
    }

    /// The payload this event travels as (`ref/sync-protocol.md` §3, "The
    /// payload a `stock_ledger` row travels as"): its own columns in the
    /// canonical form every fact uses — sorted keys, no whitespace, ids as
    /// lower-case hyphenated UUIDs, instants as ISO-8601 UTC with milliseconds,
    /// flags as `0` or `1`, quantities and money as integers.
    ///
    /// The caller writes the envelope with it, and [`StockRepository::append`]
    /// refuses an envelope that carries anything else.
    #[must_use]
    pub fn payload(&self) -> String {
        let e = &self.event;
        let (ref_kind, ref_id, line_id) = match e.source {
            Some(StockSource::SaleLine { sale_id, line_id }) => (
                Value::from("sale"),
                Value::from(sale_id.as_uuid().hyphenated().to_string()),
                Value::from(line_id.as_uuid().hyphenated().to_string()),
            ),
            None => (Value::Null, Value::Null, Value::Null),
        };
        let fields: BTreeMap<&str, Value> = BTreeMap::from([
            (
                "actor_id",
                e.actor_id.map_or(Value::Null, |actor| {
                    Value::from(actor.as_uuid().hyphenated().to_string())
                }),
            ),
            ("business_date", Value::from(e.business_date.to_iso())),
            ("event_seq", Value::from(self.event_seq)),
            ("id", Value::from(e.id.as_uuid().hyphenated().to_string())),
            (
                "is_cost_estimated",
                Value::from(i64::from(e.is_cost_estimated)),
            ),
            (
                "is_wac_estimated",
                Value::from(i64::from(e.after.is_wac_estimated)),
            ),
            (
                "is_weight_derived",
                Value::from(i64::from(e.is_weight_derived)),
            ),
            ("kind", Value::from(e.kind.as_str())),
            // 1.10.5's adjustments fill these two. The columns exist, so the
            // payload carries them from the first event and keeps its shape.
            ("note", Value::Null),
            ("occurred_at", Value::from(e.occurred_at.to_iso8601())),
            ("on_hand_after_milli", Value::from(e.after.on_hand.milli())),
            (
                "product_id",
                Value::from(e.product_id.as_uuid().hyphenated().to_string()),
            ),
            ("qty_delta_milli", Value::from(e.qty_delta.milli())),
            ("qty_step_milli", Value::from(self.qty_step_milli)),
            ("reason_code", Value::Null),
            ("ref_id", ref_id),
            ("ref_kind", ref_kind),
            (
                "register_id",
                Value::from(self.register.as_uuid().hyphenated().to_string()),
            ),
            ("source_sale_line_id", line_id),
            (
                "store_id",
                Value::from(e.store_id.as_uuid().hyphenated().to_string()),
            ),
            (
                "unit_cost_minor",
                e.unit_cost
                    .map_or(Value::Null, |cost| Value::from(cost.minor())),
            ),
            ("wac_after_minor", Value::from(e.after.wac.minor())),
        ]);
        // Inserted in key order from the `BTreeMap`, so the object keeps that
        // order whether `serde_json` sorts its maps or preserves insertion;
        // and rendering a `Value` has no failure to hide.
        Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
        .to_string()
    }
}

/// `stock_cache`'s columns, in the order the table declares them: what a
/// rebuild writes, what the replay selects, and what [`CacheDrift::Differs`]
/// names.
const CACHE_COLUMNS: [&str; 10] = [
    "product_id",
    "store_id",
    "on_hand_milli",
    "wac_minor",
    "is_wac_estimated",
    "last_event_register_id",
    "last_event_seq",
    "last_event_id",
    "event_count",
    "last_event_at",
];

/// A full replay of one store's ledger (`?1`): one row per product the ledger
/// moved there, in [`CACHE_COLUMNS`]' order.
///
/// On-hand is the sum of the events, which is I-6's definition rather than a
/// copy of the last event's projection. The WAC, its estimate flag and the
/// watermark come from each product's last event — the highest `event_seq`,
/// the register's own sequence and never a clock (I-7).
const REPLAY: &str = "SELECT t.product_id, t.store_id, t.on_hand, e.wac_after_minor,
        e.is_wac_estimated, e.register_id, e.event_seq, e.id, t.events, e.occurred_at
   FROM (SELECT product_id, store_id, SUM(qty_delta_milli) AS on_hand,
                COUNT(*) AS events, MAX(event_seq) AS last_seq
           FROM stock_ledger
          WHERE store_id = ?1
          GROUP BY product_id, store_id) t
   JOIN stock_ledger e
     ON e.product_id = t.product_id AND e.store_id = t.store_id
    AND e.event_seq = t.last_seq";

/// How one product's row in a store's stock cache departs from a full replay
/// of the ledger. It names products and columns, never a figure, so an alarm
/// can carry it as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheDrift {
    /// The ledger moved the product in the store, and the cache holds no row
    /// for it, so its on-hand reads as zero.
    Missing(ProductId),
    /// The cache holds a row for a product no event in the store's ledger
    /// moved.
    Unexplained(ProductId),
    /// The row differs from the replay in these columns, named as
    /// `stock_cache` names them and in its order.
    Differs {
        product_id: ProductId,
        columns: Vec<&'static str>,
    },
}

/// A product the cache holds below zero in one store (master plan C.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegativeStockRow {
    pub product_id: ProductId,
    pub on_hand: Qty,
}

/// Writes stock events and reads their projection.
pub struct StockRepository<'c> {
    conn: &'c Connection,
}

impl<'c> StockRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Prepare `movements`, in the order they will be appended, as `register`'s
    /// next ledger events.
    ///
    /// Reads go through the caller's transaction, so the positions, the ledger
    /// head and the events that extend them are one consistent write. Nothing
    /// is written.
    ///
    /// # Errors
    ///
    /// [`DbError::StockStoreUnknown`] and [`DbError::StockProductUnknown`] for
    /// a store or product the database does not hold,
    /// [`DbError::StockQtyOffStep`] for a quantity that is not a whole number of
    /// the product's steps, [`DbError::StockRefused`] for what the domain
    /// refuses, and [`DbError::StoredStockRecordInvalid`] for a stored value
    /// this build cannot read.
    pub fn prepare(
        &self,
        tx: &Transaction<'_>,
        register: RegisterId,
        movements: &[NewStockEvent],
    ) -> Result<Vec<PreparedStockEvent>, DbError> {
        let mut event_seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(event_seq), 0) FROM stock_ledger WHERE register_id = ?1",
            [register.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )?;
        // The position each product is in, as this batch leaves it.
        let mut positions: BTreeMap<(ProductId, StoreId), StockPosition> = BTreeMap::new();
        let mut prepared = Vec::with_capacity(movements.len());
        for movement in movements {
            let qty_step_milli = product_step(tx, movement.product_id)?;
            let qty = movement.qty_delta.milli();
            if qty.checked_rem(qty_step_milli) != Some(0) {
                return Err(DbError::StockQtyOffStep {
                    qty_milli: qty,
                    step_milli: qty_step_milli,
                });
            }
            let key = (movement.product_id, movement.store_id);
            let before = match positions.get(&key) {
                Some(position) => *position,
                None => cached_position(tx, movement.product_id, movement.store_id)?,
            };
            let event = StockEvent::capture(movement, &before)?;
            positions.insert(key, event.after);
            event_seq = event_seq.checked_add(1).ok_or_else(|| {
                invalid("the register's stock ledger head is at the integer limit")
            })?;
            prepared.push(PreparedStockEvent {
                register,
                event_seq,
                qty_step_milli,
                event,
            });
        }
        Ok(prepared)
    }

    /// Append `events`, inside the caller's transaction, after the envelope
    /// that names them. They must be appended in the order they were prepared.
    ///
    /// # Errors
    ///
    /// [`DbError::StockEnvelopeMissing`] when no ready envelope names an event,
    /// [`DbError::StockEnvelopeMismatch`] when the one that does carries a
    /// different payload, and [`DbError::Sqlite`] for what `0006` refuses —
    /// an event out of sequence, or a projection the cache has moved past.
    pub fn append(
        &self,
        tx: &Transaction<'_>,
        events: &[PreparedStockEvent],
    ) -> Result<(), DbError> {
        for prepared in events {
            let e = &prepared.event;
            let id = e.id.as_uuid();
            let travelling: Option<String> = tx
                .query_row(
                    "SELECT m.payload
                       FROM fact_commit_member m
                       JOIN sync_commit_ready ready ON ready.id = m.commit_id
                      WHERE m.entity = ?1 AND m.entity_id = ?2",
                    params![ENTITY, id.as_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()?;
            match travelling {
                None => return Err(DbError::StockEnvelopeMissing),
                Some(payload) if payload != prepared.payload() => {
                    return Err(DbError::StockEnvelopeMismatch);
                }
                Some(_) => {}
            }
            let (ref_kind, ref_id, line_id) = match e.source {
                Some(StockSource::SaleLine { sale_id, line_id }) => (
                    Some("sale"),
                    Some(sale_id.as_uuid().as_bytes().to_vec()),
                    Some(line_id.as_uuid().as_bytes().to_vec()),
                ),
                None => (None, None, None),
            };
            tx.execute(
                "INSERT INTO stock_ledger
                   (id, register_id, event_seq, product_id, store_id, qty_delta_milli,
                    qty_step_milli, kind, ref_kind, ref_id, source_sale_line_id,
                    unit_cost_minor, is_cost_estimated, is_weight_derived,
                    on_hand_after_milli, wac_after_minor, is_wac_estimated,
                    actor_id, occurred_at, business_date)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                         ?15, ?16, ?17, ?18, ?19, ?20)",
                params![
                    id.as_bytes().as_slice(),
                    prepared.register.as_uuid().as_bytes().as_slice(),
                    prepared.event_seq,
                    e.product_id.as_uuid().as_bytes().as_slice(),
                    e.store_id.as_uuid().as_bytes().as_slice(),
                    e.qty_delta.milli(),
                    prepared.qty_step_milli,
                    e.kind.as_str(),
                    ref_kind,
                    ref_id,
                    line_id,
                    e.unit_cost.map(Money::minor),
                    e.is_cost_estimated,
                    e.is_weight_derived,
                    e.after.on_hand.milli(),
                    e.after.wac.minor(),
                    e.after.is_wac_estimated,
                    e.actor_id.map(|actor| actor.as_uuid().as_bytes().to_vec()),
                    e.occurred_at.to_iso8601(),
                    e.business_date.to_iso(),
                ],
            )?;
        }
        Ok(())
    }

    /// A product's on-hand in a store, from the cache: zero for one the store
    /// has never stocked.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] if the read fails.
    pub fn on_hand(&self, product: ProductId, store: StoreId) -> Result<Qty, DbError> {
        let milli: Option<i64> = self
            .conn
            .query_row(
                "SELECT on_hand_milli FROM stock_cache WHERE product_id = ?1 AND store_id = ?2",
                params![
                    product.as_uuid().as_bytes().as_slice(),
                    store.as_uuid().as_bytes().as_slice()
                ],
                |row| row.get(0),
            )
            .optional()?;
        Ok(Qty::from_milli(milli.unwrap_or(0)))
    }

    /// The store's negative-stock policy: `store.allow_negative_stock`, 1 to
    /// allow and flag (the default, merchant decision 4.1) and 0 for the hard
    /// block.
    ///
    /// # Errors
    ///
    /// [`DbError::StockStoreUnknown`] for a store the database does not hold,
    /// and [`DbError::StoredStockRecordInvalid`] for any other stored value:
    /// `0003` gives the column no `CHECK`, and a policy read as a guess would
    /// either refuse sales the merchant allowed or allow ones they blocked.
    pub fn negative_stock_policy(&self, store: StoreId) -> Result<NegativeStockPolicy, DbError> {
        let allow: Option<i64> = self
            .conn
            .query_row(
                "SELECT allow_negative_stock FROM store WHERE id = ?1",
                [store.as_uuid().as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        match allow {
            None => Err(DbError::StockStoreUnknown(store)),
            Some(1) => Ok(NegativeStockPolicy::AllowAndFlag),
            Some(0) => Ok(NegativeStockPolicy::HardBlock),
            Some(other) => Err(invalid(&format!(
                "store {store} has allow_negative_stock {other}"
            ))),
        }
    }

    /// Screen what a basket takes from `store`'s stock, before it is tendered,
    /// and return the flag: every product it would leave below zero, in product
    /// order.
    ///
    /// `taken` is per line, as the stock each line will move — a scale label's
    /// line moves the weight its price implies, not its one package. A service
    /// moves no stock and is never screened. On-hand is read from the cache,
    /// which 1.10.3's check keeps true. Nothing is written.
    ///
    /// The shell screens when a line is added or its quantity changes, and
    /// again when tender begins, so a hard block refuses the line before any
    /// money is taken. Finalize never screens: by then the customer has paid,
    /// and a sale that took stock below zero stands and is flagged (E.12).
    ///
    /// # Errors
    ///
    /// [`DbError::StockRefused`] with [`pos_domain::StockError::WouldGoNegative`]
    /// under the store's hard block when the flag is not empty;
    /// [`DbError::StockStoreUnknown`] and [`DbError::StockProductUnknown`] for a
    /// store or product the database does not hold; and the errors of
    /// [`Self::negative_stock_policy`].
    pub fn screen_basket(
        &self,
        store: StoreId,
        taken: &[(ProductId, Qty)],
    ) -> Result<Vec<Shortfall>, DbError> {
        let policy = self.negative_stock_policy(store)?;
        let mut stocked: BTreeMap<ProductId, Qty> = BTreeMap::new();
        let mut services: BTreeSet<ProductId> = BTreeSet::new();
        for &(product, _) in taken {
            if stocked.contains_key(&product) || services.contains(&product) {
                continue;
            }
            let is_service: Option<bool> = self
                .conn
                .query_row(
                    "SELECT is_service FROM product WHERE id = ?1",
                    [product.as_uuid().as_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()?;
            match is_service {
                None => return Err(DbError::StockProductUnknown(product)),
                Some(true) => {
                    services.insert(product);
                }
                Some(false) => {
                    stocked.insert(product, self.on_hand(product, store)?);
                }
            }
        }
        let goods: Vec<(ProductId, Qty)> = taken
            .iter()
            .copied()
            .filter(|(product, _)| !services.contains(product))
            .collect();
        Ok(policy.screen(&stocked, &goods)?)
    }

    /// Replay `store`'s ledger into its cache, inside the caller's transaction,
    /// and return how many cache rows it wrote. The replay is [`REPLAY`].
    ///
    /// # Errors
    ///
    /// [`DbError::StockReplayAcrossRegisters`] when the store's ledger holds
    /// more than one register's events: `stock_ledger` travels up only, so a
    /// register's database holds its own, and no owned sequence orders two
    /// registers' events against each other. [`DbError::Sqlite`] if the replay
    /// fails, in which case the caller's rollback keeps the old cache.
    pub fn rebuild_cache(&self, tx: &Transaction<'_>, store: StoreId) -> Result<u64, DbError> {
        let store_id = store.as_uuid();
        let store_bytes = store_id.as_bytes().as_slice();
        one_register(tx, store_bytes)?;
        tx.execute("DELETE FROM stock_cache WHERE store_id = ?1", [store_bytes])?;
        let written = tx.execute(
            &format!(
                "INSERT INTO stock_cache ({}) {REPLAY}",
                CACHE_COLUMNS.join(", ")
            ),
            [store_bytes],
        )?;
        Ok(written as u64)
    }

    /// Compare `store`'s cache with a full replay of its ledger, and return
    /// each product on which they differ, in product order: nothing when the
    /// cache is true. Nothing is written.
    ///
    /// Both are read through the caller's transaction, so they are read at one
    /// moment. Every column is compared, the watermark included: the event
    /// count and the last event's register, sequence, id and instant, beside
    /// on-hand and the WAC. A cache that missed an update, was restored from an
    /// older copy or was written to by hand differs in at least one of them.
    ///
    /// # Errors
    ///
    /// [`DbError::StockReplayAcrossRegisters`] as for [`Self::rebuild_cache`],
    /// whose replay this is; [`DbError::IdWidthInvalid`] for a product id that
    /// is not sixteen bytes; and [`DbError::Sqlite`] if a read fails.
    pub fn verify_cache(
        &self,
        tx: &Transaction<'_>,
        store: StoreId,
    ) -> Result<Vec<CacheDrift>, DbError> {
        let store_id = store.as_uuid();
        let store_bytes = store_id.as_bytes().as_slice();
        one_register(tx, store_bytes)?;
        let replayed = rows_by_product(tx, REPLAY, store_bytes)?;
        let cached = rows_by_product(
            tx,
            &format!(
                "SELECT {} FROM stock_cache WHERE store_id = ?1",
                CACHE_COLUMNS.join(", ")
            ),
            store_bytes,
        )?;
        let products: BTreeSet<&Vec<u8>> = replayed.keys().chain(cached.keys()).collect();
        let mut drift = Vec::new();
        for product in products {
            match (cached.get(product), replayed.get(product)) {
                (None, Some(_)) => {
                    drift.push(CacheDrift::Missing(product_id(product, "stock_ledger")?));
                }
                (Some(_), None) => {
                    drift.push(CacheDrift::Unexplained(product_id(product, "stock_cache")?));
                }
                (Some(cache), Some(replay)) => {
                    let columns: Vec<&'static str> = CACHE_COLUMNS
                        .iter()
                        .zip(cache.iter().zip(replay))
                        .filter(|(_, (held, replayed))| held != replayed)
                        .map(|(column, _)| *column)
                        .collect();
                    if !columns.is_empty() {
                        drift.push(CacheDrift::Differs {
                            product_id: product_id(product, "stock_cache")?,
                            columns,
                        });
                    }
                }
                (None, None) => {}
            }
        }
        Ok(drift)
    }

    /// Verify `store`'s cache and, when it differs from the ledger at all,
    /// rebuild it, inside the caller's transaction. Returns the difference
    /// found, which is what an alarm reports; a true cache is left exactly as
    /// it was, with nothing written.
    ///
    /// This is the check I-6 asks of startup and of a periodic sweep. The
    /// rebuild shares the comparison's transaction, so no event can land
    /// between what was compared and what is rewritten; and one that fails
    /// part-way is undone by the caller's rollback, which leaves the old cache
    /// whole rather than half replaced.
    ///
    /// # Errors
    ///
    /// As [`Self::verify_cache`] and [`Self::rebuild_cache`].
    pub fn reconcile_cache(
        &self,
        tx: &Transaction<'_>,
        store: StoreId,
    ) -> Result<Vec<CacheDrift>, DbError> {
        let drift = self.verify_cache(tx, store)?;
        if !drift.is_empty() {
            self.rebuild_cache(tx, store)?;
        }
        Ok(drift)
    }

    /// The products the cache holds below zero in `store`, by product id.
    ///
    /// # Errors
    ///
    /// [`DbError::IdWidthInvalid`] for a stored id that is not sixteen bytes,
    /// and [`DbError::Sqlite`] if the read fails.
    pub fn negative_stock(&self, store: StoreId) -> Result<Vec<NegativeStockRow>, DbError> {
        let mut statement = self.conn.prepare(
            "SELECT product_id, on_hand_milli FROM stock_cache
              WHERE store_id = ?1 AND on_hand_milli < 0
              ORDER BY product_id",
        )?;
        let rows = statement.query_map([store.as_uuid().as_bytes().as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.map(|row| {
            let (product, on_hand) = row?;
            let product: [u8; 16] =
                product
                    .try_into()
                    .map_err(|bytes: Vec<u8>| DbError::IdWidthInvalid {
                        table: "stock_cache",
                        column: "product_id",
                        found: bytes.len(),
                    })?;
            Ok(NegativeStockRow {
                product_id: ProductId::from_uuid(uuid::Uuid::from_bytes(product)),
                on_hand: Qty::from_milli(on_hand),
            })
        })
        .collect()
    }
}

/// The product's quantity step, which every event's quantity must be a whole
/// number of.
fn product_step(tx: &Transaction<'_>, product: ProductId) -> Result<i64, DbError> {
    let step: Option<i64> = tx
        .query_row(
            "SELECT qty_step_milli FROM product WHERE id = ?1",
            [product.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    match step {
        None => Err(DbError::StockProductUnknown(product)),
        Some(step) if step <= 0 => Err(invalid(&format!(
            "product {product} has quantity step {step}"
        ))),
        Some(step) => Ok(step),
    }
}

/// The position the cache holds for a product in a store, valued in the
/// store's currency — or an unstocked position when it holds none, which is
/// what `0006`'s projection trigger assumes too.
fn cached_position(
    tx: &Transaction<'_>,
    product: ProductId,
    store: StoreId,
) -> Result<StockPosition, DbError> {
    let code: Option<String> = tx
        .query_row(
            "SELECT currency FROM store WHERE id = ?1",
            [store.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    let code = code.ok_or(DbError::StockStoreUnknown(store))?;
    let currency = Currency::from_code(&code)
        .map_err(|_| invalid(&format!("store {store} has currency {code:?}")))?;
    let cached: Option<(i64, i64, bool)> = tx
        .query_row(
            "SELECT on_hand_milli, wac_minor, is_wac_estimated FROM stock_cache
              WHERE product_id = ?1 AND store_id = ?2",
            params![
                product.as_uuid().as_bytes().as_slice(),
                store.as_uuid().as_bytes().as_slice()
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    Ok(match cached {
        None => StockPosition::unstocked(currency),
        Some((on_hand, wac, is_wac_estimated)) => StockPosition {
            on_hand: Qty::from_milli(on_hand),
            wac: Money::from_minor(wac, currency),
            is_wac_estimated,
        },
    })
}

/// Refuse a replay of a store whose ledger holds more than one register's
/// events, which no owned sequence orders against each other.
fn one_register(tx: &Transaction<'_>, store: &[u8]) -> Result<(), DbError> {
    let registers: i64 = tx.query_row(
        "SELECT COUNT(DISTINCT register_id) FROM stock_ledger WHERE store_id = ?1",
        [store],
        |row| row.get(0),
    )?;
    if registers > 1 {
        return Err(DbError::StockReplayAcrossRegisters);
    }
    Ok(())
}

/// The rows `sql` returns for one store, each whole, keyed by its first
/// column: the product.
fn rows_by_product(
    tx: &Transaction<'_>,
    sql: &str,
    store: &[u8],
) -> Result<BTreeMap<Vec<u8>, Vec<SqlValue>>, DbError> {
    let mut statement = tx.prepare(sql)?;
    let rows = statement.query_map([store], |row| {
        let columns = (0..CACHE_COLUMNS.len())
            .map(|index| row.get::<_, SqlValue>(index))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((row.get::<_, Vec<u8>>(0)?, columns))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn product_id(bytes: &[u8], table: &'static str) -> Result<ProductId, DbError> {
    let id: [u8; 16] = bytes.try_into().map_err(|_| DbError::IdWidthInvalid {
        table,
        column: "product_id",
        found: bytes.len(),
    })?;
    Ok(ProductId::from_uuid(uuid::Uuid::from_bytes(id)))
}

fn invalid(reason: &str) -> DbError {
    DbError::StoredStockRecordInvalid {
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use pos_domain::{
        AuditIntent, BusinessDate, IncomingCost, SaleId, SaleLineId, StockError, StockEventId,
        StockEventKind, Timestamp, UserId,
    };
    use rusqlite::params;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::fixtures::registered_chain::{self, Checkout, RegisteredChain};
    use crate::repo::audit::AuditRepository;
    use crate::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};

    const KEY: &str = "stock-repository-test-key";
    const AT: &str = "2026-10-25T09:00:00.000Z";
    const REGISTER: u128 = 0x01;
    const OTHER_REGISTER: u128 = 0x02;
    const COLA: u128 = 0x0a;
    const RICE: u128 = 0x0b;
    const DELIVERY: u128 = 0x0c;

    struct Till {
        _dir: tempfile::TempDir,
        conn: Connection,
        chain: RegisteredChain,
    }

    fn bytes(n: u128) -> Vec<u8> {
        Uuid::from_u128(n).as_bytes().to_vec()
    }

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, Currency::JOD)
    }

    fn register(n: u128) -> RegisterId {
        RegisterId::from_uuid(Uuid::from_u128(n))
    }

    fn product(n: u128) -> ProductId {
        ProductId::from_uuid(Uuid::from_u128(n))
    }

    /// A registered store with one register, a cola and a bag of rice, both
    /// sold by the unit (a step of 1000 milli-units), and nothing in stock.
    fn till() -> Till {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open(&dir.path().join("register.db"), KEY).unwrap();
        let chain = RegisteredChain::seed(&conn);
        chain.add_register(&conn, &bytes(REGISTER), "REG01");
        for (id, sku) in [(COLA, "COLA"), (RICE, "RICE")] {
            conn.execute(
                "INSERT INTO product (id, sku, name, price_minor, currency, is_active,
                                      tax_category_id, unit, qty_step_milli, is_weighed)
                 VALUES (?1, ?2, ?2, 450, 'JOD', 1, ?3, 'each', 1000, 0)",
                params![bytes(id), sku, &chain.tax_category],
            )
            .unwrap();
        }
        Till {
            _dir: dir,
            conn,
            chain,
        }
    }

    fn store(t: &Till) -> StoreId {
        StoreId::from_uuid(Uuid::from_slice(&t.chain.store).unwrap())
    }

    fn movement(
        t: &Till,
        id: u128,
        what: u128,
        kind: StockEventKind,
        qty_milli: i64,
        cost: Option<IncomingCost>,
    ) -> NewStockEvent {
        NewStockEvent {
            id: StockEventId::from_uuid(Uuid::from_u128(id)),
            product_id: product(what),
            store_id: store(t),
            kind,
            qty_delta: Qty::from_milli(qty_milli),
            incoming_cost: cost,
            is_weight_derived: false,
            source: None,
            actor_id: None,
            occurred_at: Timestamp::parse_iso8601(AT).unwrap(),
            business_date: BusinessDate::new(2026, 10, 25).unwrap(),
        }
    }

    /// The caller's half of a stock write, the order 1.8.3 and 1.10.5 follow:
    /// prepare, write one envelope naming every event with its payload — and
    /// the audit row, when there is one — then the audit row, then the events.
    fn write(
        t: &Till,
        tx: &Transaction<'_>,
        commit: u128,
        at: RegisterId,
        movements: &[NewStockEvent],
        audited: bool,
    ) -> Result<Vec<PreparedStockEvent>, DbError> {
        let stock = StockRepository::new(&t.conn);
        let prepared = stock.prepare(tx, at, movements)?;
        let payloads: Vec<String> = prepared.iter().map(PreparedStockEvent::payload).collect();
        let ids: Vec<[u8; 16]> = prepared
            .iter()
            .map(|p| *p.event().id.as_uuid().as_bytes())
            .collect();
        let changes: Vec<[u8; 16]> = (0..=prepared.len())
            .map(|index| *Uuid::from_u128((commit << 16) + index as u128).as_bytes())
            .collect();
        let audit_id = Uuid::from_u128((commit << 16) + 0xA0);
        let audit_bytes = *audit_id.as_bytes();

        let mut members: Vec<FactMember<'_>> = ids
            .iter()
            .zip(&changes)
            .zip(&payloads)
            .map(|((entity_id, change_id), payload)| FactMember {
                change_id,
                entity: ENTITY,
                entity_id,
                payload,
            })
            .collect();
        if audited {
            members.push(FactMember {
                change_id: changes.last().unwrap(),
                entity: "audit_log",
                entity_id: &audit_bytes,
                payload: "{}",
            });
        }
        let commit_id = *Uuid::from_u128(commit).as_bytes();
        OutboxRepository::new(&t.conn).write_commit(
            tx,
            &CommitEnvelope {
                commit_id: &commit_id,
                protocol_version: 1,
                schema_version: crate::SCHEMA_VERSION,
                producer_version: "stock-repository-test",
                created_at: AT,
            },
            &members,
        )?;
        if audited {
            let intent = AuditIntent {
                actor: UserId::from_uuid(Uuid::from_slice(&t.chain.cashier).unwrap()),
                approver: None,
                approval: None,
                action: "stock.adjust",
                entity: ENTITY,
                entity_id: Uuid::from_bytes(*ids.first().unwrap()),
                reason: None,
                payload: json!({ "events": ids.len() }),
                at: Timestamp::parse_iso8601(AT).unwrap(),
            };
            AuditRepository::new(&t.conn).append(tx, at, audit_id, &intent)?;
        }
        stock.append(tx, &prepared)?;
        Ok(prepared)
    }

    fn record(
        t: &Till,
        commit: u128,
        movements: &[NewStockEvent],
    ) -> Result<Vec<PreparedStockEvent>, DbError> {
        let tx = t.conn.unchecked_transaction().unwrap();
        let prepared = write(t, &tx, commit, register(REGISTER), movements, false)?;
        tx.commit()?;
        Ok(prepared)
    }

    /// Every row the stock transaction touches, as text: the counts of the
    /// envelope, audit and ledger tables, and every cache row whole.
    fn snapshot(t: &Till) -> Vec<String> {
        let mut rows: Vec<String> = [
            "sync_commit",
            "fact_commit_member",
            "sync_outbox",
            "audit_log",
            "stock_ledger",
        ]
        .iter()
        .map(|table| {
            let n: i64 = t
                .conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            format!("{table}: {n}")
        })
        .collect();
        let mut cache = t
            .conn
            .prepare(
                "SELECT hex(product_id), on_hand_milli, wac_minor, is_wac_estimated,
                        last_event_seq, hex(last_event_id), event_count, last_event_at
                   FROM stock_cache ORDER BY product_id",
            )
            .unwrap();
        rows.extend(
            cache
                .query_map([], |r| {
                    Ok(format!(
                        "cache {}: on hand {}, wac {}, estimated {}, last #{} {}, {} events at {}",
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, bool>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, i64>(6)?,
                        r.get::<_, String>(7)?,
                    ))
                })
                .unwrap()
                .map(Result::unwrap),
        );
        rows
    }

    /// Every write a stock transaction makes, so a fault can be injected in
    /// front of each one.
    #[derive(Debug, Clone, Copy)]
    enum WritePoint {
        Commit,
        Member,
        Delivery,
        Audit,
        Event,
        CacheInsert,
        CacheUpdate,
    }

    impl WritePoint {
        const ALL: [WritePoint; 7] = [
            WritePoint::Commit,
            WritePoint::Member,
            WritePoint::Delivery,
            WritePoint::Audit,
            WritePoint::Event,
            WritePoint::CacheInsert,
            WritePoint::CacheUpdate,
        ];

        /// A temporary trigger that refuses this write: the seam is the storage
        /// engine itself, so nothing in the repository knows it is under test.
        fn fault(self) -> String {
            let (operation, table) = match self {
                WritePoint::Commit => ("INSERT", "sync_commit"),
                WritePoint::Member => ("INSERT", "fact_commit_member"),
                WritePoint::Delivery => ("INSERT", "sync_outbox"),
                WritePoint::Audit => ("INSERT", "audit_log"),
                WritePoint::Event => ("INSERT", "stock_ledger"),
                WritePoint::CacheInsert => ("INSERT", "stock_cache"),
                WritePoint::CacheUpdate => ("UPDATE", "stock_cache"),
            };
            format!(
                "CREATE TEMP TRIGGER injected_fault BEFORE {operation} ON {table}
                 BEGIN SELECT RAISE(ABORT, 'injected fault'); END;"
            )
        }
    }

    #[test]
    fn stock_append_projects_cache_and_manifest_atomically() {
        let t = till();
        // Twelve colas arrive at 0.380, then two leave in the same transaction:
        // one batch, two consecutive events on one product.
        let opening = [
            movement(
                &t,
                0x100,
                COLA,
                StockEventKind::Adjust,
                12_000,
                Some(IncomingCost::Observed(jod(380))),
            ),
            movement(&t, 0x101, COLA, StockEventKind::Waste, -2_000, None),
        ];
        let tx = t.conn.unchecked_transaction().unwrap();
        let prepared = write(&t, &tx, 0x10, register(REGISTER), &opening, true).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            prepared
                .iter()
                .map(PreparedStockEvent::event_seq)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(
            StockRepository::new(&t.conn)
                .on_hand(product(COLA), store(&t))
                .unwrap(),
            Qty::from_milli(10_000)
        );
        let baseline = snapshot(&t);
        assert_eq!(
            baseline.get(..5).unwrap(),
            [
                "sync_commit: 1",
                "fact_commit_member: 3",
                "sync_outbox: 3",
                "audit_log: 1",
                "stock_ledger: 2"
            ]
        );

        // The next transaction moves both a stocked product (the cache row is
        // updated) and an unstocked one (a cache row is inserted), with its
        // audit row. A fault in front of any one of its writes leaves every
        // table exactly as it was.
        let next = [
            movement(&t, 0x200, COLA, StockEventKind::Waste, -1_000, None),
            movement(
                &t,
                0x201,
                RICE,
                StockEventKind::Adjust,
                5_000,
                Some(IncomingCost::Observed(jod(900))),
            ),
        ];
        for point in WritePoint::ALL {
            t.conn.execute_batch(&point.fault()).unwrap();
            let tx = t.conn.unchecked_transaction().unwrap();
            let error = write(&t, &tx, 0x20, register(REGISTER), &next, true)
                .expect_err("the injected fault stops the transaction");
            assert!(
                error.to_string().contains("injected fault"),
                "{point:?} failed for another reason: {error}"
            );
            drop(tx);
            t.conn
                .execute_batch("DROP TRIGGER temp.injected_fault;")
                .unwrap();
            assert_eq!(snapshot(&t), baseline, "a fault before the {point:?} write");
        }

        // And without a fault, the same transaction commits whole.
        let tx = t.conn.unchecked_transaction().unwrap();
        write(&t, &tx, 0x20, register(REGISTER), &next, true).unwrap();
        tx.commit().unwrap();
        let after = snapshot(&t);
        assert_eq!(
            after.get(..5).unwrap(),
            [
                "sync_commit: 2",
                "fact_commit_member: 6",
                "sync_outbox: 6",
                "audit_log: 2",
                "stock_ledger: 4"
            ]
        );
        let stock = StockRepository::new(&t.conn);
        assert_eq!(
            stock.on_hand(product(COLA), store(&t)).unwrap(),
            Qty::from_milli(9_000)
        );
        assert_eq!(
            stock.on_hand(product(RICE), store(&t)).unwrap(),
            Qty::from_milli(5_000)
        );
    }

    #[test]
    fn stock_event_captures_unit_cost_and_estimate_flag() {
        let t = till();
        let cost = |event: u128| -> (Option<i64>, bool, i64, bool) {
            t.conn
                .query_row(
                    "SELECT unit_cost_minor, is_cost_estimated, wac_after_minor, is_wac_estimated
                       FROM stock_ledger WHERE id = ?1",
                    [bytes(event)],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .unwrap()
        };

        // An observed cost is captured as observed, and becomes the WAC.
        record(
            &t,
            0x10,
            &[movement(
                &t,
                0x100,
                COLA,
                StockEventKind::Adjust,
                10_000,
                Some(IncomingCost::Observed(jod(380))),
            )],
        )
        .unwrap();
        assert_eq!(cost(0x100), (Some(380), false, 380, false));

        // A sale captures the basis in force — on the sale's own row.
        t.conn
            .execute(
                "INSERT INTO sale (id, receipt_number, register_id, store_id, status,
                                   subtotal_minor, tax_minor, total_minor, currency,
                                   business_date, completed_at)
                 VALUES (?1, 'REG01-000001', ?2, ?3, 'parked', 900, 0, 900, 'JOD',
                         '2026-10-25', ?4)",
                params![bytes(0x500), bytes(REGISTER), &t.chain.store, AT],
            )
            .unwrap();
        t.conn
            .execute(
                "INSERT INTO sale_line (id, sale_id, product_id, qty_milli, qty_step_milli,
                                        unit_price_minor, tax_minor, total_minor, line_no,
                                        name_snapshot, net_minor, tax_category_id, is_weighed)
                 VALUES (?1, ?2, ?3, 2000, 1000, 450, 0, 900, 1, 'Cola', 900, ?4, 0)",
                params![
                    bytes(0x501),
                    bytes(0x500),
                    bytes(COLA),
                    &t.chain.tax_category
                ],
            )
            .unwrap();
        let sale = NewStockEvent {
            source: Some(StockSource::SaleLine {
                sale_id: SaleId::from_uuid(Uuid::from_u128(0x500)),
                line_id: SaleLineId::from_uuid(Uuid::from_u128(0x501)),
            }),
            ..movement(&t, 0x101, COLA, StockEventKind::Sale, -2_000, None)
        };
        record(&t, 0x11, &[sale]).unwrap();
        assert_eq!(cost(0x101), (Some(380), false, 380, false));

        // A projected cost at the same WAC marks the WAC estimated from then on
        // — and the sale before it keeps the observed basis it captured.
        record(
            &t,
            0x12,
            &[movement(
                &t,
                0x102,
                COLA,
                StockEventKind::CountCorrection,
                1_000,
                Some(IncomingCost::Projected(jod(380))),
            )],
        )
        .unwrap();
        assert_eq!(cost(0x102), (Some(380), true, 380, true));
        assert_eq!(cost(0x101), (Some(380), false, 380, false));

        // No cost at all is NULL, never zero, and estimated.
        record(
            &t,
            0x13,
            &[movement(
                &t,
                0x103,
                RICE,
                StockEventKind::Adjust,
                3_000,
                Some(IncomingCost::Unknown),
            )],
        )
        .unwrap();
        assert_eq!(cost(0x103), (None, true, 0, true));
    }

    #[test]
    fn stock_event_payload_is_its_row_in_canonical_form() {
        let t = till();
        let prepared = record(
            &t,
            0x10,
            &[movement(
                &t,
                0x100,
                COLA,
                StockEventKind::Adjust,
                12_000,
                Some(IncomingCost::Observed(jod(380))),
            )],
        )
        .unwrap();
        let store = Uuid::from_slice(&t.chain.store).unwrap().hyphenated();
        // `ref/sync-protocol.md` §3's shape, key for key: sorted, no whitespace,
        // flags as 0/1, and the two 1.10.5 columns present as null.
        assert_eq!(
            prepared.first().unwrap().payload(),
            format!(
                "{{\"actor_id\":null,\"business_date\":\"2026-10-25\",\"event_seq\":1,\
                 \"id\":\"00000000-0000-0000-0000-000000000100\",\"is_cost_estimated\":0,\
                 \"is_wac_estimated\":0,\"is_weight_derived\":0,\"kind\":\"adjust\",\"note\":null,\
                 \"occurred_at\":\"2026-10-25T09:00:00.000Z\",\"on_hand_after_milli\":12000,\
                 \"product_id\":\"00000000-0000-0000-0000-00000000000a\",\"qty_delta_milli\":12000,\
                 \"qty_step_milli\":1000,\"reason_code\":null,\"ref_id\":null,\"ref_kind\":null,\
                 \"register_id\":\"00000000-0000-0000-0000-000000000001\",\
                 \"source_sale_line_id\":null,\"store_id\":\"{store}\",\"unit_cost_minor\":380,\
                 \"wac_after_minor\":380}}"
            )
        );
        // And it is what travelled: the manifest holds these exact bytes.
        let travelled: String = t
            .conn
            .query_row(
                "SELECT payload FROM fact_commit_member WHERE entity = 'stock_ledger'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(travelled, prepared.first().unwrap().payload());
    }

    #[test]
    fn stock_append_refuses_an_event_its_envelope_does_not_describe() {
        let t = till();
        let stock = StockRepository::new(&t.conn);
        let arrival = [movement(
            &t,
            0x100,
            COLA,
            StockEventKind::Adjust,
            4_000,
            Some(IncomingCost::Observed(jod(380))),
        )];

        let tx = t.conn.unchecked_transaction().unwrap();
        let prepared = stock.prepare(&tx, register(REGISTER), &arrival).unwrap();
        assert!(matches!(
            stock.append(&tx, &prepared),
            Err(DbError::StockEnvelopeMissing)
        ));
        drop(tx);

        // An envelope for the same event that says something else.
        let tx = t.conn.unchecked_transaction().unwrap();
        let prepared = stock.prepare(&tx, register(REGISTER), &arrival).unwrap();
        let other = stock
            .prepare(
                &tx,
                register(REGISTER),
                &[movement(
                    &t,
                    0x100,
                    COLA,
                    StockEventKind::Adjust,
                    5_000,
                    Some(IncomingCost::Observed(jod(380))),
                )],
            )
            .unwrap();
        let payload = other.first().unwrap().payload();
        let id = *Uuid::from_u128(0x100).as_bytes();
        let change = *Uuid::from_u128(0x1000).as_bytes();
        OutboxRepository::new(&t.conn)
            .write_commit(
                &tx,
                &CommitEnvelope {
                    commit_id: Uuid::from_u128(0x10).as_bytes(),
                    protocol_version: 1,
                    schema_version: crate::SCHEMA_VERSION,
                    producer_version: "stock-repository-test",
                    created_at: AT,
                },
                &[FactMember {
                    change_id: &change,
                    entity: ENTITY,
                    entity_id: &id,
                    payload: &payload,
                }],
            )
            .unwrap();
        assert!(matches!(
            stock.append(&tx, &prepared),
            Err(DbError::StockEnvelopeMismatch)
        ));
    }

    #[test]
    fn stock_prepare_refuses_what_the_ledger_cannot_record() {
        let t = till();
        record(
            &t,
            0x10,
            &[movement(
                &t,
                0x100,
                COLA,
                StockEventKind::Adjust,
                10_000,
                Some(IncomingCost::Observed(jod(380))),
            )],
        )
        .unwrap();
        let stock = StockRepository::new(&t.conn);
        let tx = t.conn.unchecked_transaction().unwrap();
        let refused = |movement: NewStockEvent| {
            stock
                .prepare(&tx, register(REGISTER), &[movement])
                .unwrap_err()
        };

        assert!(matches!(
            refused(movement(&t, 0x101, 0xFF, StockEventKind::Waste, -1_000, None)),
            DbError::StockProductUnknown(p) if p == product(0xFF)
        ));
        assert!(matches!(
            refused(NewStockEvent {
                store_id: StoreId::from_uuid(Uuid::from_u128(0xEE)),
                ..movement(&t, 0x101, COLA, StockEventKind::Waste, -1_000, None)
            }),
            DbError::StockStoreUnknown(_)
        ));
        assert!(matches!(
            refused(movement(
                &t,
                0x101,
                COLA,
                StockEventKind::Waste,
                -1_500,
                None
            )),
            DbError::StockQtyOffStep {
                qty_milli: -1_500,
                step_milli: 1_000
            }
        ));
        // A different cost into positive stock needs the weighted average.
        assert!(matches!(
            refused(movement(
                &t,
                0x101,
                COLA,
                StockEventKind::Receive,
                1_000,
                Some(IncomingCost::Observed(jod(500)))
            )),
            DbError::StockRefused(StockError::WacBlendNotSupported { .. })
        ));
    }

    #[test]
    fn stock_rebuild_cache_replays_the_ledger() {
        let t = till();
        record(
            &t,
            0x10,
            &[
                movement(
                    &t,
                    0x100,
                    COLA,
                    StockEventKind::Adjust,
                    10_000,
                    Some(IncomingCost::Observed(jod(380))),
                ),
                movement(&t, 0x101, COLA, StockEventKind::Waste, -3_000, None),
                movement(&t, 0x102, RICE, StockEventKind::Waste, -2_000, None),
            ],
        )
        .unwrap();
        let projected = snapshot(&t);

        t.conn
            .execute(
                "UPDATE stock_cache SET on_hand_milli = 999, wac_minor = 1, event_count = 0",
                [],
            )
            .unwrap();
        assert_ne!(snapshot(&t), projected);

        let tx = t.conn.unchecked_transaction().unwrap();
        let written = StockRepository::new(&t.conn)
            .rebuild_cache(&tx, store(&t))
            .unwrap();
        tx.commit().unwrap();
        assert_eq!(written, 2);
        assert_eq!(snapshot(&t), projected);
    }

    #[test]
    fn stock_rebuild_refuses_two_registers_events() {
        let t = till();
        t.chain
            .add_register(&t.conn, &bytes(OTHER_REGISTER), "REG02");
        record(
            &t,
            0x10,
            &[movement(
                &t,
                0x100,
                COLA,
                StockEventKind::Waste,
                -1_000,
                None,
            )],
        )
        .unwrap();
        let tx = t.conn.unchecked_transaction().unwrap();
        write(
            &t,
            &tx,
            0x11,
            register(OTHER_REGISTER),
            &[movement(
                &t,
                0x101,
                RICE,
                StockEventKind::Waste,
                -1_000,
                None,
            )],
            false,
        )
        .unwrap();
        tx.commit().unwrap();

        let tx = t.conn.unchecked_transaction().unwrap();
        assert!(matches!(
            StockRepository::new(&t.conn).rebuild_cache(&tx, store(&t)),
            Err(DbError::StockReplayAcrossRegisters)
        ));
    }

    #[test]
    fn negative_stock_lists_what_the_cache_holds_below_zero() {
        let t = till();
        record(
            &t,
            0x10,
            &[
                movement(&t, 0x100, RICE, StockEventKind::Waste, -2_000, None),
                movement(
                    &t,
                    0x101,
                    COLA,
                    StockEventKind::Adjust,
                    1_000,
                    Some(IncomingCost::Unknown),
                ),
            ],
        )
        .unwrap();
        let stock = StockRepository::new(&t.conn);
        assert_eq!(
            stock.negative_stock(store(&t)).unwrap(),
            [NegativeStockRow {
                product_id: product(RICE),
                on_hand: Qty::from_milli(-2_000)
            }]
        );
        assert_eq!(
            stock.on_hand(product(RICE), store(&t)).unwrap(),
            Qty::from_milli(-2_000)
        );
        assert_eq!(stock.on_hand(product(0xFF), store(&t)).unwrap(), Qty::ZERO);
    }

    /// One unit of `what` sold and completed on register `at`, through the
    /// registered chain's checkout: the sale, its line and its cash tender,
    /// then the fixture's envelope, the line's stock event at the cache's cost
    /// basis, the receipt, the audit row and the completion. Nothing screens
    /// it, as nothing does once a customer has paid.
    fn sell(t: &Till, slot: u8, at: u128, what: u128) -> Vec<u8> {
        let base = u128::from(slot) << 8;
        let (sale, line, tender) = (bytes(base | 1), bytes(base | 2), bytes(base | 3));
        t.conn
            .execute(
                "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                                   tax_minor, total_minor, currency, business_date, completed_at)
                 VALUES (?1, ?2, ?3, 'parked', 2500, 400, 2900, 'JOD', '2026-08-25', ?4)",
                params![
                    &sale,
                    format!("SYN-{slot:06}"),
                    bytes(at),
                    registered_chain::AT
                ],
            )
            .unwrap();
        t.conn
            .execute(
                "INSERT INTO sale_line
                   (id, sale_id, product_id, qty_milli, unit_price_minor, tax_minor,
                    total_minor, line_no, name_snapshot, net_minor, tax_category_id)
                 VALUES (?1, ?2, ?3, 1000, 2500, 400, 2900, 1, 'Synthetic', 2500, ?4)",
                params![&line, &sale, bytes(what), &t.chain.tax_category],
            )
            .unwrap();
        t.conn
            .execute(
                "INSERT INTO sale_tender (id, sale_id, method, amount_minor, change_minor)
                 VALUES (?1, ?2, 'cash', 2900, 0)",
                params![&tender, &sale],
            )
            .unwrap();
        Checkout::new(slot, &sale, &bytes(at), &[&line], &[&tender]).seal(&t.conn, &t.chain);
        sale
    }

    /// The sale's status, and the position its stock event left.
    fn sold_at(t: &Till, sale: &[u8]) -> (String, i64) {
        t.conn
            .query_row(
                "SELECT s.status, e.on_hand_after_milli
                   FROM sale s JOIN stock_ledger e ON e.ref_id = s.id
                  WHERE s.id = ?1",
                [sale],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }

    fn hard_block(t: &Till) {
        t.conn
            .execute(
                "UPDATE store SET allow_negative_stock = 0 WHERE id = ?1",
                [&t.chain.store],
            )
            .unwrap();
    }

    fn one(what: u128, milli: i64) -> (ProductId, Qty) {
        (product(what), Qty::from_milli(milli))
    }

    /// Merchant decision 4.1 and master plan C.7: by default a basket that
    /// takes more than is on hand is flagged, not refused. Its sale completes,
    /// its stock event records the position it left, and the negative-stock
    /// report lists the product until stock arrives.
    #[test]
    fn negative_stock_allowed_by_default_and_flagged() {
        let t = till();
        let stock = StockRepository::new(&t.conn);
        assert_eq!(
            stock.negative_stock_policy(store(&t)).unwrap(),
            NegativeStockPolicy::AllowAndFlag
        );
        // Two colas arrive; a basket rings up three in two lines, and a bag of
        // rice nobody has stocked.
        record(
            &t,
            0x10,
            &[movement(
                &t,
                0x100,
                COLA,
                StockEventKind::Adjust,
                2_000,
                Some(IncomingCost::Observed(jod(380))),
            )],
        )
        .unwrap();
        let basket = [one(COLA, 1_000), one(RICE, 1_000), one(COLA, 2_000)];
        assert_eq!(
            stock.screen_basket(store(&t), &basket).unwrap(),
            [
                Shortfall {
                    product_id: product(COLA),
                    on_hand: Qty::from_milli(2_000),
                    taken: Qty::from_milli(3_000),
                },
                Shortfall {
                    product_id: product(RICE),
                    on_hand: Qty::ZERO,
                    taken: Qty::from_milli(1_000),
                },
            ]
        );

        // The sales complete, one unit each, and the third takes cola below zero.
        for slot in [0x11, 0x12] {
            assert_eq!(sold_at(&t, &sell(&t, slot, REGISTER, COLA)).0, "completed");
        }
        let third = sell(&t, 0x13, REGISTER, COLA);
        assert_eq!(sold_at(&t, &third), ("completed".to_owned(), -1_000));
        let rice = sell(&t, 0x14, REGISTER, RICE);
        assert_eq!(sold_at(&t, &rice), ("completed".to_owned(), -1_000));
        assert_eq!(
            stock.negative_stock(store(&t)).unwrap(),
            [
                NegativeStockRow {
                    product_id: product(COLA),
                    on_hand: Qty::from_milli(-1_000),
                },
                NegativeStockRow {
                    product_id: product(RICE),
                    on_hand: Qty::from_milli(-1_000),
                },
            ]
        );
        let tx = t.conn.unchecked_transaction().unwrap();
        assert_eq!(stock.verify_cache(&tx, store(&t)).unwrap(), []);
    }

    /// A tightly run store's hard block refuses the line that would take stock
    /// below zero, before any money is taken. The last unit still sells, a
    /// service is never screened, and a screen writes nothing. Finalize never
    /// screens: a sale already paid for completes even under the block.
    #[test]
    fn hard_block_setting_refuses_the_line() {
        let t = till();
        hard_block(&t);
        t.conn
            .execute(
                "INSERT INTO product (id, sku, name, price_minor, currency, is_active,
                                      tax_category_id, unit, qty_step_milli, is_weighed,
                                      is_service)
                 VALUES (?1, 'DELIVERY', 'DELIVERY', 1000, 'JOD', 1, ?2, 'each', 1000, 0, 1)",
                params![bytes(DELIVERY), &t.chain.tax_category],
            )
            .unwrap();
        record(
            &t,
            0x10,
            &[movement(
                &t,
                0x100,
                COLA,
                StockEventKind::Adjust,
                1_000,
                Some(IncomingCost::Observed(jod(380))),
            )],
        )
        .unwrap();
        let stock = StockRepository::new(&t.conn);
        assert_eq!(
            stock.negative_stock_policy(store(&t)).unwrap(),
            NegativeStockPolicy::HardBlock
        );
        let changes = t.conn.total_changes();

        assert_eq!(
            stock.screen_basket(store(&t), &[one(COLA, 1_000)]).unwrap(),
            [],
            "the last unit leaves zero, which is not below it"
        );
        let refused = stock.screen_basket(store(&t), &[one(COLA, 1_000), one(COLA, 1_000)]);
        assert!(
            matches!(
                &refused,
                Err(DbError::StockRefused(StockError::WouldGoNegative(s)))
                    if s == &[Shortfall {
                        product_id: product(COLA),
                        on_hand: Qty::from_milli(1_000),
                        taken: Qty::from_milli(2_000),
                    }]
            ),
            "{refused:?}"
        );
        assert_eq!(
            stock
                .screen_basket(store(&t), &[one(DELIVERY, 5_000), one(COLA, 1_000)])
                .unwrap(),
            []
        );
        assert!(matches!(
            stock.screen_basket(store(&t), &[one(0xFF, 1_000)]),
            Err(DbError::StockProductUnknown(p)) if p == product(0xFF)
        ));
        let elsewhere = StoreId::from_uuid(Uuid::from_u128(0xEE));
        assert!(matches!(
            stock.screen_basket(elsewhere, &[one(COLA, 1_000)]),
            Err(DbError::StockStoreUnknown(s)) if s == elsewhere
        ));
        assert_eq!(t.conn.total_changes(), changes, "a screen writes nothing");

        // Two sales already paid for: the second takes cola below zero and
        // completes regardless.
        sell(&t, 0x11, REGISTER, COLA);
        assert_eq!(
            sold_at(&t, &sell(&t, 0x12, REGISTER, COLA)),
            ("completed".to_owned(), -1_000)
        );
        assert_eq!(
            stock.on_hand(product(COLA), store(&t)).unwrap(),
            Qty::from_milli(-1_000)
        );

        // A stored value that is neither is refused by name, never guessed.
        t.conn
            .execute(
                "UPDATE store SET allow_negative_stock = 2 WHERE id = ?1",
                [&t.chain.store],
            )
            .unwrap();
        assert!(matches!(
            stock.negative_stock_policy(store(&t)),
            Err(DbError::StoredStockRecordInvalid { .. })
        ));
        assert!(matches!(
            stock.screen_basket(store(&t), &[one(COLA, 1_000)]),
            Err(DbError::StoredStockRecordInvalid { .. })
        ));
    }

    /// E.12, Phase 1's half. Two registers each hold the last unit in their own
    /// ledger and sell it while offline. Neither can see the other's sale, so
    /// neither refuses, not even under the hard block, which can only screen
    /// what its own register knows. Both sales stand: inventory is a ledger,
    /// not a lock. The store's stock going negative and flagged where the two
    /// meet is Phase 3's `both_sales_of_the_last_unit_stand_and_stock_goes_negative_flagged`.
    #[test]
    fn two_offline_registers_selling_the_last_unit_both_succeed() {
        for (at, code, slot) in [(REGISTER, "REG01", 0x20), (OTHER_REGISTER, "REG02", 0x21)] {
            let t = till();
            if at != REGISTER {
                t.chain.add_register(&t.conn, &bytes(at), code);
            }
            hard_block(&t);
            let tx = t.conn.unchecked_transaction().unwrap();
            write(
                &t,
                &tx,
                0x10,
                register(at),
                &[movement(
                    &t,
                    0x100,
                    COLA,
                    StockEventKind::Adjust,
                    1_000,
                    Some(IncomingCost::Observed(jod(380))),
                )],
                false,
            )
            .unwrap();
            tx.commit().unwrap();
            let stock = StockRepository::new(&t.conn);
            assert_eq!(
                stock.screen_basket(store(&t), &[one(COLA, 1_000)]).unwrap(),
                [],
                "{code} holds the last unit"
            );
            let sale = sell(&t, slot, at, COLA);
            assert_eq!(sold_at(&t, &sale), ("completed".to_owned(), 0), "{code}");
            assert_eq!(stock.negative_stock(store(&t)).unwrap(), [], "{code}");
        }
    }
}
