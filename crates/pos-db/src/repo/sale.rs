//! The completed sale's writer and its reads (microstep 1.8.2, `ref/schema.md`
//! §0003, §0005 and §0009).
//!
//! **A completed sale is never updated and never deleted (I-4).** This module
//! has no method that does either, not a private one, and
//! `tests/ui/completed_sale_update.rs` proves none can be called. A correction
//! is a new document, a refund or a credit note, never an edit.
//!
//! A sale is written the way every fact with a delivery envelope is, and the
//! way 1.8.3's finalize writes it (I-9):
//!
//! 1. **[`SaleRepository::prepare`]** turns a [`CompletedSale`] into the rows
//!    that hold it, through the caller's transaction: the header, its supply
//!    context, each line with its tax components and discount attributions,
//!    the receipt's tax summary, each tender, and each tender's first status
//!    event. It reads what the sale does not carry: its register's store, that
//!    store's tax computation policy, and each component's place in the store's
//!    pack. The ids its child rows need come from the caller's [`IdSource`].
//!    Before it answers, it reads the sale back out of those rows, exactly as
//!    [`SaleRepository::by_id`] does, and refuses a sale that would not come
//!    back as it went in. Nothing is written.
//! 2. The caller writes the envelope naming every one of
//!    [`PreparedSale::members`], with exactly those payloads, beside the stock,
//!    audit and receipt facts of the same finalize.
//! 3. **[`SaleRepository::insert_complete`]** checks the envelope carries each
//!    member exactly, writes the rows, hands the transaction to the caller for
//!    the facts that need the sale's rows to exist (its stock events, its
//!    receipt and print job, its audit row, any approval consumption), and then
//!    completes the sale.
//!
//! **Why completion is the last write.** `0005` and `0006` refuse a completed
//! sale until its receipt and print job, stock facts, tender events, audit row
//! and complete manifest exist, and each of those references the sale's own
//! rows or is checked against them. So the header is written `parked`, the
//! transaction's other facts are written against it, and the one transition
//! `0005` names, `parked` to `completed`, is the last write. It updates a row
//! this same call inserted, in the same transaction, which nothing else has
//! seen. No method takes a sale id and changes its row, and `0003`'s triggers
//! refuse any update of a completed one.
//!
//! **One description of each row.** Each row is a map from column to value,
//! built once from the sale. The `INSERT` binds it, the payload that travels
//! is its canonical form, and the reads decode the same columns, so the row
//! written, the fact that travels and the sale read back cannot drift apart.
//! The payload follows the canonical form every fact uses (`ref/sync-protocol.md`
//! §2): sorted keys, no whitespace, ids as lower-case hyphenated UUIDs, instants
//! as ISO-8601 UTC with milliseconds, flags as `0` or `1`, quantities and money
//! as integers.
//!
//! **What it does not store.** A department line has no product, and
//! `sale_line.product_id` is NOT NULL until #330 is decided, so a sale with one
//! is refused by name. So is a tax component charged by amount or on another
//! component, which no approved pack can hold yet (1.3.5), and a tax summary
//! row with an amount per unit.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use pos_domain::{
    ApprovalId, BasketDiscount, BusinessDate, Cart, CartLine, CashRounding, CompletedSale,
    ComponentTax, Currency, CustomerId, DerivedWeight, DiscountAttribution, DiscountKind,
    DiscountReason, DiscountSource, IdSource, LineDiscount, LineTax, Money, OverrideReason,
    Percent, PriceMode, PriceOrigin, PriceOverride, PricedCart, PricedLine, ProductId, Qty,
    RegisterId, SaleId, SaleLineId, ShiftId, StoreId, StoredCompletedSale, SupplyDestination,
    SupplyTaxContext, TaxBasis, TaxCategoryId, TaxSummaryRow, Tender, TenderId, TenderState,
    Timestamp, UserId, ZeroRatingReason, summarize_tax,
};
use rusqlite::types::{ToSqlOutput, Value};
use rusqlite::{Connection, OptionalExtension, ToSql, Transaction, params_from_iter};
use serde_json::{Map, Value as Json};
use uuid::Uuid;

use crate::DbError;
use crate::repo::outbox::FactMember;
use crate::repo::product::{unit_from, unit_token};
use crate::tax_pack::{TaxPackRepository, treatment_from_token, treatment_token};

// ── The entities, as `ref/schema.md`'s fact-table marker spells them ───────

pub const SALE: &str = "sale";
pub const SALE_SUPPLY_TAX_CONTEXT: &str = "sale_supply_tax_context";
pub const SALE_LINE: &str = "sale_line";
pub const SALE_LINE_TAX: &str = "sale_line_tax";
pub const SALE_LINE_DISCOUNT: &str = "sale_line_discount";
pub const SALE_TAX_SUMMARY: &str = "sale_tax_summary";
pub const SALE_TENDER: &str = "sale_tender";
pub const TENDER_STATUS_EVENT: &str = "tender_status_event";

// ── The columns this repository writes and reads, table by table ──────────

const SALE_COLUMNS: [&str; 34] = [
    "id",
    "receipt_number",
    "register_id",
    "status",
    "subtotal_minor",
    "tax_minor",
    "total_minor",
    "currency",
    "ref_sale_id",
    "business_date",
    "completed_at",
    "store_id",
    "shift_id",
    "cashier_id",
    "customer_id",
    "doc_type",
    "buyer_id_scheme",
    "buyer_id_value",
    "buyer_name",
    "is_training",
    "discount_minor",
    "rounding_adj_minor",
    "tax_computation_policy_id",
    "sync_commit_id",
    "origin_device",
    "opened_at",
    "price_mode",
    "cash_rounding_original_minor",
    "basket_discount_kind",
    "basket_discount_ppm",
    "basket_discount_amount_minor",
    "basket_discount_reason",
    "basket_discount_by",
    "basket_discount_approver",
];

const SUPPLY_COLUMNS: [&str; 7] = [
    "sale_id",
    "destination_code",
    "zero_tax_reason_code",
    "eligible_entity_authority",
    "evidence_ref",
    "evidence_hash",
    "captured_at",
];

const LINE_COLUMNS: [&str; 33] = [
    "id",
    "sale_id",
    "product_id",
    "qty_milli",
    "qty_step_milli",
    "unit_price_minor",
    "discount_minor",
    "tax_minor",
    "total_minor",
    "line_no",
    "name_snapshot",
    "net_minor",
    "tax_category_id",
    "is_weighed",
    "unit",
    "price_origin",
    "label_rule_prefix",
    "label_raw_code",
    "max_price_minor",
    "min_age",
    "is_age_confirmed",
    "label_qty_milli",
    "label_unit_price_minor",
    "line_discount_kind",
    "line_discount_ppm",
    "line_discount_amount_minor",
    "line_discount_reason",
    "line_discount_by",
    "line_discount_approver",
    "override_price_minor",
    "override_reason",
    "override_by",
    "override_approver",
];

const LINE_TAX_COLUMNS: [&str; 14] = [
    "id",
    "sale_line_id",
    "component_code",
    "treatment",
    "calculation_kind",
    "rate_ppm",
    "fixed_amount_minor",
    "fixed_currency",
    "fixed_basis_qty_milli",
    "calculation_order",
    "base_kind",
    "taxable_base_minor",
    "taxable_qty_milli",
    "tax_minor",
];

const LINE_DISCOUNT_COLUMNS: [&str; 7] = [
    "id",
    "sale_line_id",
    "source",
    "authorized_by",
    "reason",
    "amount_minor",
    "percent_ppm",
];

const SUMMARY_COLUMNS: [&str; 16] = [
    "id",
    "sale_id",
    "component_code",
    "treatment",
    "calculation_kind",
    "rate_ppm",
    "fixed_amount_minor",
    "fixed_currency",
    "fixed_basis_qty_milli",
    "calculation_order",
    "base_kind",
    "taxable_base_minor",
    "taxable_qty_milli",
    "net_minor",
    "tax_minor",
    "gross_minor",
];

const TENDER_COLUMNS: [&str; 7] = [
    "id",
    "sale_id",
    "method",
    "amount_minor",
    "psp_ref",
    "change_minor",
    "tender_no",
];

const EVENT_COLUMNS: [&str; 10] = [
    "id",
    "tender_id",
    "sync_commit_id",
    "event_no",
    "state",
    "psp_ref",
    "masked_pan",
    "scheme",
    "reason_code",
    "occurred_at",
];

/// The scheme a business buyer's tax number is recorded under: the
/// reconstructed `TN` candidate `ref/fiscal-jofotara.md` lists, PROVISIONAL
/// until 2.7.0 pins the official package. The cart holds a tax number and
/// nothing else, so it is the only scheme this repository writes or reads.
const BUYER_TAX_NUMBER: &str = "TN";

// ── One value, as a row holds it ───────────────────────────────────────────

/// A column's value, in the four shapes these tables use. No `REAL`: no
/// column here holds one, and money never does (I-1).
#[derive(Clone, PartialEq, Eq)]
enum Cell {
    Null,
    Int(i64),
    Text(String),
    Id(Uuid),
}

impl Cell {
    fn of_id(id: Uuid) -> Cell {
        Cell::Id(id)
    }

    fn of_opt_id(id: Option<Uuid>) -> Cell {
        id.map_or(Cell::Null, Cell::Id)
    }

    fn of_text(text: &str) -> Cell {
        Cell::Text(text.to_owned())
    }

    fn of_opt_text(text: Option<&str>) -> Cell {
        text.map_or(Cell::Null, Cell::of_text)
    }

    fn of_opt_int(value: Option<i64>) -> Cell {
        value.map_or(Cell::Null, Cell::Int)
    }

    fn of_flag(value: bool) -> Cell {
        Cell::Int(i64::from(value))
    }

    /// The canonical form this value travels in.
    fn json(&self) -> Json {
        match self {
            Cell::Null => Json::Null,
            Cell::Int(value) => Json::from(*value),
            Cell::Text(text) => Json::from(text.as_str()),
            Cell::Id(id) => Json::from(id.hyphenated().to_string()),
        }
    }

    /// A stored value, read back. A sixteen-byte blob is an id; any other
    /// blob, and any `REAL`, is not something this repository wrote.
    fn from_value(table: &str, column: &str, value: Value) -> Result<Cell, DbError> {
        match value {
            Value::Null => Ok(Cell::Null),
            Value::Integer(value) => Ok(Cell::Int(value)),
            Value::Text(text) => Ok(Cell::Text(text)),
            Value::Blob(bytes) => Uuid::from_slice(&bytes).map(Cell::Id).map_err(|_| {
                invalid(format!(
                    "{table}.{column} holds a {}-byte blob",
                    bytes.len()
                ))
            }),
            Value::Real(_) => Err(invalid(format!("{table}.{column} holds a REAL"))),
        }
    }
}

impl ToSql for Cell {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            Cell::Null => ToSqlOutput::Owned(Value::Null),
            Cell::Int(value) => ToSqlOutput::Owned(Value::Integer(*value)),
            Cell::Text(text) => ToSqlOutput::Owned(Value::Text(text.clone())),
            Cell::Id(id) => ToSqlOutput::Owned(Value::Blob(id.as_bytes().to_vec())),
        })
    }
}

/// One row: every column a table's list names, and nothing else. Built only
/// by [`row`], from an array as long as the list, so a column cannot be left
/// out.
#[derive(Clone, PartialEq, Eq)]
struct Cells {
    table: &'static str,
    values: BTreeMap<&'static str, Cell>,
}

fn row<const N: usize>(
    table: &'static str,
    columns: &[&'static str; N],
    values: [Cell; N],
) -> Cells {
    Cells {
        table,
        values: columns.iter().copied().zip(values).collect(),
    }
}

impl Cells {
    /// The row's canonical payload: its columns, sorted, compact.
    fn payload(&self) -> String {
        let fields: Map<String, Json> = self
            .values
            .iter()
            .map(|(column, cell)| ((*column).to_owned(), cell.json()))
            .collect();
        Json::Object(fields).to_string()
    }

    fn cell(&self, column: &str) -> Result<&Cell, DbError> {
        self.values
            .get(column)
            .ok_or_else(|| invalid(format!("{}.{column} was not read", self.table)))
    }

    fn int(&self, column: &str) -> Result<i64, DbError> {
        self.opt_int(column)?
            .ok_or_else(|| invalid(format!("{}.{column} is NULL", self.table)))
    }

    fn opt_int(&self, column: &str) -> Result<Option<i64>, DbError> {
        match self.cell(column)? {
            Cell::Null => Ok(None),
            Cell::Int(value) => Ok(Some(*value)),
            _ => Err(invalid(format!(
                "{}.{column} is not an integer",
                self.table
            ))),
        }
    }

    fn text(&self, column: &str) -> Result<&str, DbError> {
        self.opt_text(column)?
            .ok_or_else(|| invalid(format!("{}.{column} is NULL", self.table)))
    }

    fn opt_text(&self, column: &str) -> Result<Option<&str>, DbError> {
        match self.cell(column)? {
            Cell::Null => Ok(None),
            Cell::Text(text) => Ok(Some(text.as_str())),
            _ => Err(invalid(format!("{}.{column} is not text", self.table))),
        }
    }

    fn id(&self, column: &str) -> Result<Uuid, DbError> {
        self.opt_id(column)?
            .ok_or_else(|| invalid(format!("{}.{column} is NULL", self.table)))
    }

    fn opt_id(&self, column: &str) -> Result<Option<Uuid>, DbError> {
        match self.cell(column)? {
            Cell::Null => Ok(None),
            Cell::Id(id) => Ok(Some(*id)),
            _ => Err(invalid(format!("{}.{column} is not an id", self.table))),
        }
    }

    fn flag(&self, column: &str) -> Result<bool, DbError> {
        match self.int(column)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid(format!("{}.{column} is not 0 or 1", self.table))),
        }
    }

    /// A required token, by name. The value is named in the error: every
    /// column this is used on holds a vocabulary word, never a person's data.
    fn token<T>(&self, column: &str, read: impl Fn(&str) -> Option<T>) -> Result<T, DbError> {
        let token = self.text(column)?;
        read(token).ok_or_else(|| invalid(format!("{}.{column} is {token:?}", self.table)))
    }

    fn opt_token<T>(
        &self,
        column: &str,
        read: impl Fn(&str) -> Option<T>,
    ) -> Result<Option<T>, DbError> {
        self.opt_text(column)?
            .map(|token| {
                read(token).ok_or_else(|| invalid(format!("{}.{column} is {token:?}", self.table)))
            })
            .transpose()
    }

    fn insert(&self, tx: &Transaction<'_>) -> Result<(), DbError> {
        let columns: Vec<&str> = self.values.keys().copied().collect();
        let holders: Vec<String> = (1..=columns.len()).map(|n| format!("?{n}")).collect();
        tx.execute(
            &format!(
                "INSERT INTO {} ({}) VALUES ({})",
                self.table,
                columns.join(", "),
                holders.join(", ")
            ),
            params_from_iter(self.values.values()),
        )?;
        Ok(())
    }
}

// ── What prepare hands back ────────────────────────────────────────────────

/// Every row of one completed sale, as written and as read back.
#[derive(Clone, PartialEq, Eq)]
struct SaleRows {
    sale: Cells,
    supply: Cells,
    lines: Vec<Cells>,
    taxes: Vec<Cells>,
    discounts: Vec<Cells>,
    summary: Vec<Cells>,
    tenders: Vec<Cells>,
    events: Vec<Cells>,
}

impl SaleRows {
    /// Parents before children: the order the server applies a commit's
    /// members in (`ref/sync-protocol.md` §2, rule 1).
    fn in_order(&self) -> impl Iterator<Item = &Cells> {
        [&self.sale, &self.supply]
            .into_iter()
            .chain(&self.lines)
            .chain(&self.taxes)
            .chain(&self.discounts)
            .chain(&self.summary)
            .chain(&self.tenders)
            .chain(&self.events)
    }
}

/// One fact of a prepared sale: what its envelope member must name, and the
/// payload it must carry.
///
/// `Debug` names the entity and the id and never the payload, which carries
/// the buyer's name on a B2B sale (`.claude/rules/security.md`).
#[derive(Clone, PartialEq, Eq)]
pub struct PreparedMember {
    entity: &'static str,
    entity_id: [u8; 16],
    payload: String,
}

impl PreparedMember {
    #[must_use]
    pub const fn entity(&self) -> &'static str {
        self.entity
    }

    #[must_use]
    pub const fn entity_id(&self) -> &[u8; 16] {
        &self.entity_id
    }

    #[must_use]
    pub fn payload(&self) -> &str {
        &self.payload
    }

    /// This fact as an envelope member under `change_id`, which the caller
    /// mints, as it mints every identity of the finalize.
    #[must_use]
    pub fn as_fact<'a>(&'a self, change_id: &'a [u8; 16]) -> FactMember<'a> {
        FactMember {
            change_id,
            entity: self.entity,
            entity_id: &self.entity_id,
            payload: &self.payload,
        }
    }
}

impl fmt::Debug for PreparedMember {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedMember")
            .field("entity", &self.entity)
            .field("entity_id", &Uuid::from_bytes(self.entity_id))
            .finish_non_exhaustive()
    }
}

/// A completed sale ready to be written: its rows, and the envelope members
/// that must name them.
///
/// Only [`SaleRepository::prepare`] makes one, and its fields are private, so
/// what [`SaleRepository::insert_complete`] writes is exactly what was shown
/// to read back as the sale.
#[derive(Clone, PartialEq)]
pub struct PreparedSale {
    sale: CompletedSale,
    commit: Uuid,
    rows: SaleRows,
    members: Vec<PreparedMember>,
}

impl PreparedSale {
    /// The sale these rows hold.
    #[must_use]
    pub const fn sale(&self) -> &CompletedSale {
        &self.sale
    }

    /// The commit every member must be in.
    #[must_use]
    pub const fn commit_id(&self) -> &[u8; 16] {
        self.commit.as_bytes()
    }

    /// Every fact of the sale, parents before children, for the envelope.
    #[must_use]
    pub fn members(&self) -> &[PreparedMember] {
        &self.members
    }
}

impl fmt::Debug for PreparedSale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedSale")
            .field("sale", &self.sale.cart().id)
            .field("sync_commit", &self.commit)
            .field("members", &self.members)
            .finish_non_exhaustive()
    }
}

// ── What the reads hand back ───────────────────────────────────────────────

/// A completed sale as this register holds it: the sale, and the facts its
/// finalize wrote beside it that a refund, a reprint or a reconciliation
/// reads (1.8.2).
///
/// `Debug` is [`CompletedSale`]'s, which redacts the buyer's name.
#[derive(Debug, Clone, PartialEq)]
pub struct SaleRecord {
    sale: CompletedSale,
    store: StoreId,
    tax_policy: Uuid,
    sync_commit: Uuid,
    original_receipt: Option<Uuid>,
    approvals: Vec<ApprovalId>,
    tender_states: Vec<(TenderId, TenderState)>,
}

impl SaleRecord {
    /// The sale, exactly as it was completed.
    #[must_use]
    pub const fn sale(&self) -> &CompletedSale {
        &self.sale
    }

    /// The store it was sold in, which its register belongs to.
    #[must_use]
    pub const fn store(&self) -> StoreId {
        self.store
    }

    /// The tax computation policy it was priced under: its store's, when it
    /// completed.
    #[must_use]
    pub const fn tax_policy(&self) -> Uuid {
        self.tax_policy
    }

    /// The commit whose manifest names every one of its facts.
    #[must_use]
    pub const fn sync_commit(&self) -> Uuid {
        self.sync_commit
    }

    /// Its original receipt artifact, which a reprint duplicates. `0005`
    /// completes no sale without one, so `None` is a sale completed before
    /// that gate existed.
    #[must_use]
    pub const fn original_receipt(&self) -> Option<Uuid> {
        self.original_receipt
    }

    /// The approval handles spent on it, in the order they were consumed.
    #[must_use]
    pub fn approvals(&self) -> &[ApprovalId] {
        &self.approvals
    }

    /// Where each tender has got to now: the projection of its status events,
    /// which a settlement or reversal after the sale moves on. The tender's
    /// state when the sale completed is the sale's own, [`Tender::state`].
    #[must_use]
    pub fn tender_states(&self) -> &[(TenderId, TenderState)] {
        &self.tender_states
    }
}

// ── The repository ─────────────────────────────────────────────────────────

/// Writes completed sales and reads them back. It updates and deletes nothing.
pub struct SaleRepository<'c> {
    conn: &'c Connection,
}

impl<'c> SaleRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// The rows that hold `sale`, under the commit `commit_id`, and the
    /// envelope members that must name them.
    ///
    /// `price_mode` is the one the sale was priced under, the
    /// `PricingSettings::price_mode` its cart went to payment with. A
    /// completed sale does not carry it, and each line's taxable amount is
    /// read back through it.
    ///
    /// Reads go through the caller's transaction. Each tax component, discount
    /// attribution, summary row and first tender event takes an id from
    /// `ids`, in that order. Nothing is written.
    ///
    /// # Errors
    ///
    /// - [`DbError::SaleRegisterUnknown`]: the sale's register is not on this
    ///   database.
    /// - [`DbError::TaxPolicyMissing`]: its store has no tax computation
    ///   policy, which `0005` requires a completed sale to name.
    /// - [`DbError::SaleNotStorable`]: something the schema cannot hold (a
    ///   department line, a component charged by amount), a product the
    ///   register does not hold, or a sale that would not read back as it was
    ///   given, naming what.
    /// - What [`TaxPackRepository::components_for`] refuses for a line's
    ///   category on the sale's business date.
    pub fn prepare(
        &self,
        tx: &Transaction<'_>,
        sale: &CompletedSale,
        price_mode: PriceMode,
        commit_id: &[u8; 16],
        ids: &dyn IdSource,
    ) -> Result<PreparedSale, DbError> {
        let cart = sale.cart();
        let store: Option<Uuid> = tx
            .query_row(
                "SELECT store_id FROM register WHERE id = ?1",
                [cart.register_id.as_uuid().as_bytes().as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|bytes| id_from(&bytes, "register", "store_id"))
            .transpose()?;
        let store =
            StoreId::from_uuid(store.ok_or(DbError::SaleRegisterUnknown(cart.register_id))?);
        let policy: Option<Vec<u8>> = tx.query_row(
            "SELECT tax_computation_policy_id FROM store WHERE id = ?1",
            [store.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )?;
        let policy = id_from(
            &policy.ok_or(DbError::TaxPolicyMissing(store))?,
            "store",
            "tax_computation_policy_id",
        )?;

        let mut steps = BTreeMap::new();
        for line in &cart.lines {
            let product = line.product_id.ok_or_else(|| {
                not_storable(format!(
                    "line {} is a department sale, which sale_line cannot hold until #330 is \
                     decided",
                    line.id
                ))
            })?;
            let step: Option<i64> = tx
                .query_row(
                    "SELECT qty_step_milli FROM product WHERE id = ?1",
                    [product.as_uuid().as_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()?;
            let step = step.ok_or_else(|| {
                not_storable(format!(
                    "line {}'s product is not on this register",
                    line.id
                ))
            })?;
            steps.insert(line.id, step);
        }

        let context = Context {
            store,
            policy,
            price_mode,
            commit: Uuid::from_bytes(*commit_id),
            steps,
            orders: self.component_orders(sale, store)?,
        };
        let rows = encode(sale, &context, ids)?;

        // The reads decode exactly these columns, so decoding them here is
        // reading the sale back before it is written. A sale that would come
        // back as anything but itself is refused now, not discovered by a
        // refund.
        let stored = sale.to_stored();
        let back = decode(&rows)?;
        if back != stored {
            return Err(not_storable(first_difference(&back, &stored)));
        }

        let members = rows
            .in_order()
            .map(|cells| {
                let key = if cells.table == SALE_SUPPLY_TAX_CONTEXT {
                    "sale_id"
                } else {
                    "id"
                };
                Ok(PreparedMember {
                    entity: cells.table,
                    entity_id: *cells.id(key)?.as_bytes(),
                    payload: cells.payload(),
                })
            })
            .collect::<Result<Vec<_>, DbError>>()?;
        Ok(PreparedSale {
            sale: sale.clone(),
            commit: context.commit,
            rows,
            members,
        })
    }

    /// Write `prepared` and complete it, inside the caller's transaction,
    /// after the envelope that names every one of its members.
    ///
    /// `dependents` runs once the sale's rows exist and before it completes:
    /// it writes the facts the completion gates wait for, which reference
    /// those rows (the stock events, the original receipt and its print job,
    /// the audit row, any approval consumption). The completion is the last
    /// write, and if anything before it fails nothing is completed; the caller
    /// rolls the transaction back.
    ///
    /// # Errors
    ///
    /// - [`DbError::SaleEnvelopeMissing`]: no ready envelope names one of the
    ///   sale's facts.
    /// - [`DbError::SaleEnvelopeMismatch`]: one names it under another commit
    ///   or with another payload.
    /// - Whatever `dependents` returns.
    /// - [`DbError::Sqlite`] for what the schema refuses, the completion gates
    ///   above all: a sale whose receipt, stock fact, audit row or manifest is
    ///   missing does not complete.
    pub fn insert_complete<E, F>(
        &self,
        tx: &Transaction<'_>,
        prepared: &PreparedSale,
        dependents: F,
    ) -> Result<(), E>
    where
        E: From<DbError>,
        F: FnOnce(&Transaction<'_>) -> Result<(), E>,
    {
        check_envelope(tx, prepared)?;
        write_parked(tx, &prepared.rows)?;
        dependents(tx)?;
        complete(tx, prepared.sale.cart().id)?;
        Ok(())
    }

    /// The completed sale `id`, or `None` when there is no completed sale by
    /// that id.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredSaleInvalid`] for rows this build cannot read, or that
    /// do not add up to a sale, and [`DbError::Sqlite`] if the read fails.
    pub fn by_id(&self, id: SaleId) -> Result<Option<SaleRecord>, DbError> {
        let found: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT id FROM sale WHERE id = ?1 AND status = 'completed'",
                [id.as_uuid().as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        found.map(|bytes| self.load(&bytes)).transpose()
    }

    /// The completed sale `register` numbered `receipt_number`. Receipt
    /// numbers are the register's own (`0002`'s `UNIQUE (register_id,
    /// receipt_number)`), so the register is part of the question.
    ///
    /// # Errors
    ///
    /// As [`Self::by_id`].
    pub fn by_receipt_number(
        &self,
        register: RegisterId,
        receipt_number: &str,
    ) -> Result<Option<SaleRecord>, DbError> {
        let found: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT id FROM sale
                  WHERE register_id = ?1 AND receipt_number = ?2 AND status = 'completed'",
                rusqlite::params![register.as_uuid().as_bytes().as_slice(), receipt_number],
                |row| row.get(0),
            )
            .optional()?;
        found.map(|bytes| self.load(&bytes)).transpose()
    }

    /// Every completed sale of `store` on the business date `date`, training
    /// sales included, each marked by its cart's `is_training`.
    ///
    /// They are in each register's own numbering, by register: the receipt
    /// counter is the register's owned sequence, and a device clock orders
    /// nothing (I-7).
    ///
    /// # Errors
    ///
    /// As [`Self::by_id`].
    pub fn for_business_date(
        &self,
        store: StoreId,
        date: BusinessDate,
    ) -> Result<Vec<SaleRecord>, DbError> {
        let mut statement = self.conn.prepare(
            "SELECT id FROM sale
              WHERE store_id = ?1 AND business_date = ?2 AND status = 'completed'
              ORDER BY register_id, receipt_number",
        )?;
        let found = statement
            .query_map(
                rusqlite::params![store.as_uuid().as_bytes().as_slice(), date.to_iso()],
                |row| row.get::<_, Vec<u8>>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        found.iter().map(|bytes| self.load(bytes)).collect()
    }

    /// Each component's place in `store`'s pack on the sale's business date,
    /// for every category whose lines carry tax.
    fn component_orders(
        &self,
        sale: &CompletedSale,
        store: StoreId,
    ) -> Result<BTreeMap<(TaxCategoryId, String), i64>, DbError> {
        let categories: BTreeSet<TaxCategoryId> = sale
            .cart()
            .lines
            .iter()
            .zip(&sale.priced().lines)
            .filter(|(_, priced)| !priced.tax.components.is_empty())
            .map(|(line, _)| line.tax_category_id)
            .collect();
        let pack = TaxPackRepository::new(self.conn);
        let mut orders = BTreeMap::new();
        for category in categories {
            for component in pack.components_for(store, category, sale.business_date())? {
                orders.insert((category, component.code), i64::from(component.sequence));
            }
        }
        Ok(orders)
    }

    /// Every row of the completed sale whose id is `id`, read back into the
    /// sale and the facts written beside it.
    fn load(&self, id: &[u8]) -> Result<SaleRecord, DbError> {
        let rows = SaleRows {
            sale: self.one(SALE, &SALE_COLUMNS, "id = ?1", id)?,
            supply: self.one(SALE_SUPPLY_TAX_CONTEXT, &SUPPLY_COLUMNS, "sale_id = ?1", id)?,
            lines: self.all(
                SALE_LINE,
                &LINE_COLUMNS,
                "sale_id = ?1 ORDER BY line_no",
                id,
            )?,
            taxes: self.all(
                SALE_LINE_TAX,
                &LINE_TAX_COLUMNS,
                "sale_line_id IN (SELECT id FROM sale_line WHERE sale_id = ?1)",
                id,
            )?,
            discounts: self.all(
                SALE_LINE_DISCOUNT,
                &LINE_DISCOUNT_COLUMNS,
                "sale_line_id IN (SELECT id FROM sale_line WHERE sale_id = ?1)",
                id,
            )?,
            summary: self.all(SALE_TAX_SUMMARY, &SUMMARY_COLUMNS, "sale_id = ?1", id)?,
            tenders: self.all(
                SALE_TENDER,
                &TENDER_COLUMNS,
                "sale_id = ?1 ORDER BY tender_no",
                id,
            )?,
            events: self.all(
                TENDER_STATUS_EVENT,
                &EVENT_COLUMNS,
                "event_no = 1 AND tender_id IN (SELECT id FROM sale_tender WHERE sale_id = ?1)",
                id,
            )?,
        };
        let status = rows.sale.text("status")?;
        let doc_type = rows.sale.text("doc_type")?;
        if (status, doc_type) != ("completed", "sale") || rows.sale.opt_id("ref_sale_id")?.is_some()
        {
            return Err(invalid(format!(
                "sale is a {status} {doc_type}, not a completed sale"
            )));
        }
        let sale = CompletedSale::restore(decode(&rows)?)
            .map_err(|refusal| invalid(refusal.to_string()))?;

        let original_receipt: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT id FROM receipt_artifact WHERE sale_id = ?1 AND artifact_kind = 'original'",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        let mut statement = self.conn.prepare(
            "SELECT handle_id FROM approval_consumption
              WHERE effect_id = ?1 ORDER BY consumed_at, handle_id",
        )?;
        let approvals = statement
            .query_map([id], |row| row.get::<_, Vec<u8>>(0))?
            .map(|bytes| {
                id_from(&bytes?, "approval_consumption", "handle_id").map(ApprovalId::from_uuid)
            })
            .collect::<Result<Vec<_>, DbError>>()?;
        let mut tender_states = Vec::with_capacity(sale.tenders().len());
        for tender in sale.tenders() {
            let state: Option<String> = self
                .conn
                .query_row(
                    "SELECT state FROM tender_status_current WHERE tender_id = ?1",
                    [tender.id.as_uuid().as_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()?;
            let state = state
                .ok_or_else(|| invalid(format!("tender {} has no current status", tender.id)))?;
            let state = tender_state_from(&state)
                .ok_or_else(|| invalid(format!("tender {} is in state {state:?}", tender.id)))?;
            tender_states.push((tender.id, state));
        }

        Ok(SaleRecord {
            store: StoreId::from_uuid(rows.sale.id("store_id")?),
            tax_policy: rows.sale.id("tax_computation_policy_id")?,
            sync_commit: rows.sale.id("sync_commit_id")?,
            original_receipt: original_receipt
                .map(|bytes| id_from(&bytes, "receipt_artifact", "id"))
                .transpose()?,
            approvals,
            tender_states,
            sale,
        })
    }

    /// The one row of `table` that `filter` selects.
    fn one(
        &self,
        table: &'static str,
        columns: &[&'static str],
        filter: &str,
        id: &[u8],
    ) -> Result<Cells, DbError> {
        let mut found = self.all(table, columns, filter, id)?;
        match (found.pop(), found.is_empty()) {
            (Some(cells), true) => Ok(cells),
            (None, _) => Err(invalid(format!("the sale has no {table} row"))),
            (Some(_), false) => Err(invalid(format!("the sale has more than one {table} row"))),
        }
    }

    /// Every row of `table` that `filter` selects, in its order.
    fn all(
        &self,
        table: &'static str,
        columns: &[&'static str],
        filter: &str,
        id: &[u8],
    ) -> Result<Vec<Cells>, DbError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {} FROM {table} WHERE {filter}",
            columns.join(", ")
        ))?;
        let raw = statement
            .query_map([id], |row| {
                columns
                    .iter()
                    .enumerate()
                    .map(|(at, _)| row.get::<_, Value>(at))
                    .collect::<rusqlite::Result<Vec<Value>>>()
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        raw.into_iter()
            .map(|values| {
                let values = columns
                    .iter()
                    .zip(values)
                    .map(|(column, value)| Ok((*column, Cell::from_value(table, column, value)?)))
                    .collect::<Result<BTreeMap<_, _>, DbError>>()?;
                Ok(Cells { table, values })
            })
            .collect()
    }
}

// ── Writing ────────────────────────────────────────────────────────────────

/// Each member must be named by a ready envelope in the sale's commit, with
/// exactly its payload.
fn check_envelope(tx: &Transaction<'_>, prepared: &PreparedSale) -> Result<(), DbError> {
    for member in &prepared.members {
        let travelling: Option<(Vec<u8>, String)> = tx
            .query_row(
                "SELECT m.commit_id, m.payload
                   FROM fact_commit_member m
                   JOIN sync_commit_ready ready ON ready.id = m.commit_id
                  WHERE m.entity = ?1 AND m.entity_id = ?2",
                rusqlite::params![member.entity, member.entity_id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match travelling {
            None => {
                return Err(DbError::SaleEnvelopeMissing {
                    entity: member.entity,
                });
            }
            Some((commit, payload))
                if commit.as_slice() != prepared.commit.as_bytes() || payload != member.payload =>
            {
                return Err(DbError::SaleEnvelopeMismatch {
                    entity: member.entity,
                });
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Every row, with the header `parked`: the state `0005` completes from.
fn write_parked(tx: &Transaction<'_>, rows: &SaleRows) -> Result<(), DbError> {
    let mut header = rows.sale.clone();
    header.values.insert("status", Cell::of_text("parked"));
    header.insert(tx)?;
    for cells in rows.in_order().skip(1) {
        cells.insert(tx)?;
    }
    Ok(())
}

/// `parked` to `completed`: the transition `0005`'s gates guard, on the row
/// [`write_parked`] wrote in this transaction.
fn complete(tx: &Transaction<'_>, id: SaleId) -> Result<(), DbError> {
    let changed = tx.execute(
        "UPDATE sale SET status = 'completed' WHERE id = ?1 AND status = 'parked'",
        [id.as_uuid().as_bytes().as_slice()],
    )?;
    if changed == 1 {
        Ok(())
    } else {
        Err(invalid(format!(
            "sale {id} was not the parked row this call wrote"
        )))
    }
}

// ── Encoding: a completed sale into its rows ───────────────────────────────

/// What the rows need that the sale does not carry.
struct Context {
    store: StoreId,
    policy: Uuid,
    price_mode: PriceMode,
    commit: Uuid,
    /// Each line's product's quantity step, which `0003` holds a line to.
    steps: BTreeMap<SaleLineId, i64>,
    /// Each component's place in the store's pack, by category and code.
    orders: BTreeMap<(TaxCategoryId, String), i64>,
}

fn encode(
    sale: &CompletedSale,
    context: &Context,
    ids: &dyn IdSource,
) -> Result<SaleRows, DbError> {
    let cart = sale.cart();
    let priced = sale.priced();
    let sale_id = cart.id.as_uuid();
    let completed_at = sale.completed_at().to_iso8601();

    let header = encode_header(sale, context);
    let supply = row(
        SALE_SUPPLY_TAX_CONTEXT,
        &SUPPLY_COLUMNS,
        [
            Cell::of_id(sale_id),
            Cell::of_text(destination_token(cart.supply.destination)),
            Cell::of_opt_text(cart.supply.reason.map(reason_token)),
            Cell::Null,
            Cell::of_opt_text(cart.supply.evidence_ref.as_deref()),
            Cell::Null,
            Cell::of_text(&completed_at),
        ],
    );

    let mut lines = Vec::with_capacity(cart.lines.len());
    for ((line, priced_line), line_no) in cart.lines.iter().zip(&priced.lines).zip(1_i64..) {
        lines.push(encode_line(sale_id, line, priced_line, line_no, context)?);
    }

    let mut taxes = Vec::new();
    for (line, priced_line) in cart.lines.iter().zip(&priced.lines) {
        for component in &priced_line.tax.components {
            let rate = percentage(component)?;
            let order = *context
                .orders
                .get(&(line.tax_category_id, component.code.clone()))
                .ok_or_else(|| {
                    not_storable(format!(
                        "line {}'s component {} is not in its store's pack on {}",
                        line.id,
                        component.code,
                        sale.business_date().to_iso()
                    ))
                })?;
            taxes.push(row(
                SALE_LINE_TAX,
                &LINE_TAX_COLUMNS,
                [
                    Cell::of_id(ids.next()),
                    Cell::of_id(line.id.as_uuid()),
                    Cell::of_text(&component.code),
                    Cell::of_text(treatment_token(component.treatment)),
                    Cell::of_text("ad_valorem"),
                    Cell::Int(rate.ppm()),
                    Cell::Null,
                    Cell::Null,
                    Cell::Null,
                    Cell::Int(order),
                    Cell::of_text("line_net"),
                    Cell::Int(component.base_amount.minor()),
                    Cell::Null,
                    Cell::Int(component.amount.minor()),
                ],
            ));
        }
    }

    let mut discounts = Vec::new();
    for priced_line in &priced.lines {
        for attribution in &priced_line.discount_attributions {
            discounts.push(row(
                SALE_LINE_DISCOUNT,
                &LINE_DISCOUNT_COLUMNS,
                [
                    Cell::of_id(ids.next()),
                    Cell::of_id(attribution.line_id.as_uuid()),
                    Cell::of_text(source_token(attribution.source)),
                    Cell::of_id(attribution.approver.unwrap_or(attribution.by).as_uuid()),
                    Cell::of_text(&attribution.reason.0),
                    Cell::Int(attribution.amount.minor()),
                    Cell::of_opt_int(attribution.percent.map(Percent::ppm)),
                ],
            ));
        }
    }

    let mut summary = Vec::with_capacity(priced.tax_summary.len());
    for summary_row in &priced.tax_summary {
        if summary_row.per_unit.is_some() {
            return Err(not_storable(format!(
                "its tax summary charges {} by amount, which no approved pack can hold yet",
                summary_row.code
            )));
        }
        // Several categories may charge one component, each at its own place
        // in the pack; the summary row takes the earliest.
        let order = context
            .orders
            .iter()
            .filter(|((_, code), _)| *code == summary_row.code)
            .map(|(_, order)| *order)
            .min()
            .unwrap_or(0);
        summary.push(row(
            SALE_TAX_SUMMARY,
            &SUMMARY_COLUMNS,
            [
                Cell::of_id(ids.next()),
                Cell::of_id(sale_id),
                Cell::of_text(&summary_row.code),
                Cell::of_text(treatment_token(summary_row.treatment)),
                Cell::of_text("ad_valorem"),
                Cell::Int(summary_row.rate.ppm()),
                Cell::Null,
                Cell::Null,
                Cell::Null,
                Cell::Int(order),
                Cell::of_text("line_net"),
                Cell::Int(summary_row.net.minor()),
                Cell::Null,
                Cell::Int(summary_row.net.minor()),
                Cell::Int(summary_row.tax.minor()),
                Cell::Int(summary_row.gross.minor()),
            ],
        ));
    }

    let mut tenders = Vec::with_capacity(sale.tenders().len());
    let mut events = Vec::with_capacity(sale.tenders().len());
    let last = sale.tenders().len();
    if last == 0 && !sale.change().is_zero() {
        return Err(not_storable("it gives change with no tender".to_owned()));
    }
    for (tender, tender_no) in sale.tenders().iter().zip(1_i64..) {
        // Only the tender that settles a sale gives change (1.4.8).
        let change = if usize::try_from(tender_no).is_ok_and(|n| n == last) {
            sale.change().minor()
        } else {
            0
        };
        tenders.push(row(
            SALE_TENDER,
            &TENDER_COLUMNS,
            [
                Cell::of_id(tender.id.as_uuid()),
                Cell::of_id(sale_id),
                Cell::of_text(&tender.type_code),
                Cell::Int(tender.amount.minor()),
                Cell::of_opt_text(tender.psp_ref.as_deref()),
                Cell::Int(change),
                Cell::Int(tender_no),
            ],
        ));
        events.push(row(
            TENDER_STATUS_EVENT,
            &EVENT_COLUMNS,
            [
                Cell::of_id(ids.next()),
                Cell::of_id(tender.id.as_uuid()),
                Cell::of_id(context.commit),
                Cell::Int(1),
                Cell::of_text(tender.state.as_str()),
                Cell::of_opt_text(tender.psp_ref.as_deref()),
                Cell::of_opt_text(tender.masked_pan.as_deref()),
                Cell::of_opt_text(tender.scheme.as_deref()),
                Cell::Null,
                Cell::of_text(&completed_at),
            ],
        ));
    }

    Ok(SaleRows {
        sale: header,
        supply,
        lines,
        taxes,
        discounts,
        summary,
        tenders,
        events,
    })
}

fn encode_header(sale: &CompletedSale, context: &Context) -> Cells {
    let cart = sale.cart();
    let priced = sale.priced();
    let [kind, ppm, amount, reason, by, approver] =
        asked(cart.basket_discount.as_ref().map(|d| Asked {
            kind: d.kind,
            reason: d.reason.clone(),
            by: d.by,
            approver: d.approver,
        }));
    row(
        SALE,
        &SALE_COLUMNS,
        [
            Cell::of_id(cart.id.as_uuid()),
            Cell::of_text(sale.receipt_number()),
            Cell::of_id(cart.register_id.as_uuid()),
            Cell::of_text("completed"),
            Cell::Int(priced.subtotal.minor()),
            Cell::Int(priced.tax_total.minor()),
            Cell::Int(priced.total.minor()),
            Cell::of_text(cart.currency.code()),
            Cell::Null,
            Cell::of_text(&sale.business_date().to_iso()),
            Cell::of_text(&sale.completed_at().to_iso8601()),
            Cell::of_id(context.store.as_uuid()),
            Cell::of_id(cart.shift_id.as_uuid()),
            Cell::of_id(cart.cashier_id.as_uuid()),
            Cell::of_opt_id(cart.customer_id.map(CustomerId::as_uuid)),
            Cell::of_text("sale"),
            Cell::of_opt_text(cart.buyer_tin.as_ref().map(|_| BUYER_TAX_NUMBER)),
            Cell::of_opt_text(cart.buyer_tin.as_deref()),
            Cell::of_opt_text(cart.buyer_name.as_deref()),
            Cell::of_flag(cart.is_training),
            Cell::Int(priced.discount_total.minor()),
            Cell::Int(sale.cash_rounding().map_or(0, |r| r.adjustment.minor())),
            Cell::of_id(context.policy),
            Cell::of_id(context.commit),
            Cell::Null,
            Cell::of_text(&cart.opened_at.to_iso8601()),
            Cell::of_text(price_mode_token(context.price_mode)),
            Cell::of_opt_int(sale.cash_rounding().map(|r| r.original.minor())),
            kind,
            ppm,
            amount,
            reason,
            by,
            approver,
        ],
    )
}

fn encode_line(
    sale_id: Uuid,
    line: &CartLine,
    priced: &PricedLine,
    line_no: i64,
    context: &Context,
) -> Result<Cells, DbError> {
    let product = line.product_id.map(ProductId::as_uuid);
    let (origin, rule_prefix, raw_code) = match &line.price_origin {
        PriceOrigin::Catalog { .. } => ("catalog", None, None),
        PriceOrigin::Label {
            rule_prefix,
            raw_code,
        } => ("label", Some(rule_prefix.as_str()), Some(raw_code.as_str())),
        PriceOrigin::Department { .. } => {
            return Err(not_storable(format!(
                "line {} is a department sale, which sale_line cannot hold until #330 is decided",
                line.id
            )));
        }
    };
    let step = context.steps.get(&line.id).copied().ok_or_else(|| {
        not_storable(format!(
            "line {}'s product is not on this register",
            line.id
        ))
    })?;
    let off = Money::sum(
        priced.discount_attributions.iter().map(|a| a.amount),
        line.unit_price.currency(),
    )
    .map_err(|e| not_storable(format!("line {}'s discounts do not add up: {e}", line.id)))?;
    let [kind, ppm, amount, reason, by, approver] = asked(line.discount.as_ref().map(|d| Asked {
        kind: d.kind,
        reason: d.reason.clone(),
        by: d.by,
        approver: d.approver,
    }));
    let over = line.price_override.as_ref();
    Ok(row(
        SALE_LINE,
        &LINE_COLUMNS,
        [
            Cell::of_id(line.id.as_uuid()),
            Cell::of_id(sale_id),
            Cell::of_opt_id(product),
            Cell::Int(line.qty.milli()),
            Cell::Int(step),
            Cell::Int(line.unit_price.minor()),
            Cell::Int(off.minor()),
            Cell::Int(priced.tax.tax_total.minor()),
            Cell::Int(priced.line_total.minor()),
            Cell::Int(line_no),
            Cell::of_text(&line.name_snapshot),
            Cell::Int(priced.tax.net.minor()),
            Cell::of_id(line.tax_category_id.as_uuid()),
            Cell::of_flag(line.is_weighed),
            Cell::of_text(unit_token(line.unit)),
            Cell::of_text(origin),
            Cell::of_opt_text(rule_prefix),
            Cell::of_opt_text(raw_code),
            Cell::of_opt_int(line.max_price.map(Money::minor)),
            Cell::of_opt_int(line.min_age.map(i64::from)),
            Cell::of_flag(line.age_confirmed),
            Cell::of_opt_int(line.label.map(|l| l.qty.milli())),
            Cell::of_opt_int(line.label.map(|l| l.catalog_unit_price.minor())),
            kind,
            ppm,
            amount,
            reason,
            by,
            approver,
            Cell::of_opt_int(over.map(|o| o.to.minor())),
            Cell::of_opt_text(over.map(|o| o.reason.code())),
            Cell::of_opt_id(over.map(|o| o.by.as_uuid())),
            Cell::of_opt_id(over.and_then(|o| o.approver).map(UserId::as_uuid)),
        ],
    ))
}

/// A discount as it was asked for, in the six cells `0009` gives it: kind,
/// share, amount, reason, who gave it, who approved it.
fn asked(discount: Option<Asked>) -> [Cell; 6] {
    let Some(Asked {
        kind,
        reason,
        by,
        approver,
    }) = discount
    else {
        return [
            Cell::Null,
            Cell::Null,
            Cell::Null,
            Cell::Null,
            Cell::Null,
            Cell::Null,
        ];
    };
    let (token, ppm, amount) = match kind {
        DiscountKind::Percent(share) => ("percent", Cell::Int(share.ppm()), Cell::Null),
        DiscountKind::Amount(off) => ("amount", Cell::Null, Cell::Int(off.minor())),
    };
    [
        Cell::of_text(token),
        ppm,
        amount,
        Cell::Text(reason.0),
        Cell::of_id(by.as_uuid()),
        Cell::of_opt_id(approver.map(UserId::as_uuid)),
    ]
}

fn percentage(component: &ComponentTax) -> Result<Percent, DbError> {
    match component.basis {
        TaxBasis::Percentage { rate } => Ok(rate),
        TaxBasis::PerUnit { .. } | TaxBasis::Compound { .. } => Err(not_storable(format!(
            "component {} is charged by amount, which no approved pack can hold yet (1.3.5)",
            component.code
        ))),
    }
}

// ── Decoding: rows back into a completed sale ──────────────────────────────

fn decode(rows: &SaleRows) -> Result<StoredCompletedSale, DbError> {
    let header = &rows.sale;
    let currency = header.token("currency", |code| Currency::from_code(code).ok())?;
    let money = |minor: i64| Money::from_minor(minor, currency);

    let supply = SupplyTaxContext {
        destination: rows.supply.token("destination_code", destination_from)?,
        reason: rows.supply.opt_token("zero_tax_reason_code", reason_from)?,
        evidence_ref: rows.supply.opt_text("evidence_ref")?.map(str::to_owned),
    };
    let price_mode = header.token("price_mode", price_mode_from)?;
    let basket_discount =
        asked_from(header, "basket_discount", currency)?.map(|asked| BasketDiscount {
            kind: asked.kind,
            reason: asked.reason,
            by: asked.by,
            approver: asked.approver,
        });

    let mut lines = Vec::with_capacity(rows.lines.len());
    let mut priced_lines = Vec::with_capacity(rows.lines.len());
    for (cells, line_no) in rows.lines.iter().zip(1_i64..) {
        if cells.int("line_no")? != line_no {
            return Err(invalid(format!(
                "sale_line {line_no} is numbered {}",
                cells.int("line_no")?
            )));
        }
        let (line, priced) = decode_line(
            cells,
            rows,
            currency,
            price_mode,
            &supply,
            basket_discount.as_ref(),
        )?;
        lines.push(line);
        priced_lines.push(priced);
    }

    let taxes: Vec<LineTax> = priced_lines.iter().map(|l| l.tax.clone()).collect();
    let tax_summary = decode_summary(&rows.summary, &taxes, money, supply.reason)?;

    let mut tenders = Vec::with_capacity(rows.tenders.len());
    let mut change = money(0);
    for (cells, tender_no) in rows.tenders.iter().zip(1_usize..) {
        if usize::try_from(cells.int("tender_no")?).ok() != Some(tender_no) {
            return Err(invalid(format!("sale_tender {tender_no} is out of order")));
        }
        let id = cells.id("id")?;
        let event = rows
            .events
            .iter()
            .find(|event| event.opt_id("tender_id").ok().flatten() == Some(id))
            .ok_or_else(|| invalid(format!("tender {id} has no first status event")))?;
        let psp_ref = cells.opt_text("psp_ref")?.map(str::to_owned);
        if event.opt_text("psp_ref")? != psp_ref.as_deref() {
            return Err(invalid(format!(
                "tender {id}'s first event names another PSP reference"
            )));
        }
        let given = cells.int("change_minor")?;
        if given != 0 && tender_no != rows.tenders.len() {
            return Err(invalid(format!(
                "tender {id} gives change but did not settle the sale"
            )));
        }
        change = money(given);
        tenders.push(Tender {
            id: TenderId::from_uuid(id),
            type_code: cells.text("method")?.to_owned(),
            amount: money(cells.int("amount_minor")?),
            psp_ref,
            masked_pan: event.opt_text("masked_pan")?.map(str::to_owned),
            scheme: event.opt_text("scheme")?.map(str::to_owned),
            state: event.token("state", tender_state_from)?,
        });
    }

    let adjustment = money(header.int("rounding_adj_minor")?);
    let cash_rounding = match header.opt_int("cash_rounding_original_minor")? {
        Some(original) => {
            let original = money(original);
            Some(CashRounding {
                original,
                rounded: original
                    .checked_add(adjustment)
                    .map_err(|e| invalid(format!("the cash rounding does not add up: {e}")))?,
                adjustment,
            })
        }
        None if adjustment.is_zero() => None,
        None => {
            return Err(invalid(
                "sale carries a rounding adjustment and no amount it rounded".to_owned(),
            ));
        }
    };

    let buyer_tin = match (
        header.opt_text("buyer_id_scheme")?,
        header.opt_text("buyer_id_value")?,
    ) {
        (None, None) => None,
        (Some(BUYER_TAX_NUMBER), Some(tin)) => Some(tin.to_owned()),
        (scheme, _) => {
            return Err(invalid(format!(
                "sale's buyer is identified under {scheme:?}, which a cart cannot hold"
            )));
        }
    };

    let cart = Cart {
        id: SaleId::from_uuid(header.id("id")?),
        register_id: RegisterId::from_uuid(header.id("register_id")?),
        shift_id: ShiftId::from_uuid(header.id("shift_id")?),
        cashier_id: UserId::from_uuid(header.id("cashier_id")?),
        currency,
        lines,
        basket_discount,
        customer_id: header.opt_id("customer_id")?.map(CustomerId::from_uuid),
        buyer_tin,
        buyer_name: header.opt_text("buyer_name")?.map(str::to_owned),
        supply,
        is_training: header.flag("is_training")?,
        opened_at: timestamp(header, "opened_at")?,
    };
    let priced = PricedCart {
        lines: priced_lines,
        subtotal: money(header.int("subtotal_minor")?),
        discount_total: money(header.int("discount_minor")?),
        tax_summary,
        tax_total: money(header.int("tax_minor")?),
        total: money(header.int("total_minor")?),
        currency,
    };
    Ok(StoredCompletedSale {
        cart,
        priced,
        tenders,
        cash_rounding,
        change,
        receipt_number: header.text("receipt_number")?.to_owned(),
        business_date: BusinessDate::parse(header.text("business_date")?)
            .map_err(|e| invalid(format!("sale.business_date: {e}")))?,
        completed_at: timestamp(header, "completed_at")?,
    })
}

fn decode_line(
    cells: &Cells,
    rows: &SaleRows,
    currency: Currency,
    price_mode: PriceMode,
    supply: &SupplyTaxContext,
    basket: Option<&BasketDiscount>,
) -> Result<(CartLine, PricedLine), DbError> {
    let money = |minor: i64| Money::from_minor(minor, currency);
    let id = SaleLineId::from_uuid(cells.id("id")?);
    let product = ProductId::from_uuid(cells.id("product_id")?);
    let price_origin = match cells.text("price_origin")? {
        "catalog" => PriceOrigin::Catalog {
            product_id: product,
        },
        "label" => PriceOrigin::Label {
            rule_prefix: cells.text("label_rule_prefix")?.to_owned(),
            raw_code: cells.text("label_raw_code")?.to_owned(),
        },
        other => return Err(invalid(format!("sale_line.price_origin is {other:?}"))),
    };
    let label = match (
        cells.opt_int("label_qty_milli")?,
        cells.opt_int("label_unit_price_minor")?,
    ) {
        (None, None) => None,
        (Some(qty), Some(price)) => Some(DerivedWeight {
            qty: Qty::from_milli(qty),
            catalog_unit_price: money(price),
        }),
        _ => return Err(invalid(format!("line {id}'s label weight is half there"))),
    };
    let discount = asked_from(cells, "line_discount", currency)?.map(|asked| LineDiscount {
        kind: asked.kind,
        reason: asked.reason,
        by: asked.by,
        approver: asked.approver,
    });
    let price_override = match cells.opt_int("override_price_minor")? {
        None => None,
        Some(to) => Some(PriceOverride {
            to: money(to),
            reason: override_reason_from(cells.text("override_reason")?),
            by: UserId::from_uuid(cells.id("override_by")?),
            approver: cells.opt_id("override_approver")?.map(UserId::from_uuid),
        }),
    };
    let min_age = cells
        .opt_int("min_age")?
        .map(|age| u8::try_from(age).map_err(|_| invalid(format!("line {id}'s age is {age}"))))
        .transpose()?;
    let line = CartLine {
        id,
        product_id: Some(product),
        name_snapshot: cells.text("name_snapshot")?.to_owned(),
        unit_price: money(cells.int("unit_price_minor")?),
        price_origin,
        max_price: cells.opt_int("max_price_minor")?.map(money),
        qty: Qty::from_milli(cells.int("qty_milli")?),
        unit: cells.token("unit", unit_from)?,
        is_weighed: cells.flag("is_weighed")?,
        tax_category_id: TaxCategoryId::from_uuid(cells.id("tax_category_id")?),
        discount,
        price_override,
        min_age,
        age_confirmed: cells.flag("is_age_confirmed")?,
        label,
    };

    let mut taxes: Vec<&Cells> = rows
        .taxes
        .iter()
        .filter(|tax| tax.opt_id("sale_line_id").ok().flatten() == Some(id.as_uuid()))
        .collect();
    taxes.sort_by_key(|tax| tax.opt_int("calculation_order").ok().flatten());
    let components = taxes
        .into_iter()
        .map(|tax| decode_component(tax, money))
        .collect::<Result<Vec<_>, DbError>>()?;

    let mut attributions = Vec::new();
    for source in [DiscountSource::ManualLine, DiscountSource::ManualBasket] {
        let asked = match source {
            DiscountSource::ManualLine => line.discount.as_ref().map(|d| (d.by, d.approver)),
            DiscountSource::ManualBasket => basket.map(|d| (d.by, d.approver)),
        };
        for cells in rows.discounts.iter().filter(|d| {
            d.opt_id("sale_line_id").ok().flatten() == Some(id.as_uuid())
                && d.opt_text("source").ok().flatten() == Some(source_token(source))
        }) {
            let (by, approver) = asked.ok_or_else(|| {
                invalid(format!(
                    "line {id} has a {} attribution and no discount asked for it",
                    source_token(source)
                ))
            })?;
            if cells.id("authorized_by")? != approver.unwrap_or(by).as_uuid() {
                return Err(invalid(format!(
                    "line {id}'s {} attribution names another authority",
                    source_token(source)
                )));
            }
            attributions.push(DiscountAttribution {
                line_id: id,
                source,
                amount: money(cells.int("amount_minor")?),
                percent: cells.opt_int("percent_ppm")?.map(Percent::from_ppm),
                reason: DiscountReason(cells.text("reason")?.to_owned()),
                by,
                approver,
            });
        }
    }
    let known = rows
        .discounts
        .iter()
        .filter(|d| d.opt_id("sale_line_id").ok().flatten() == Some(id.as_uuid()))
        .count();
    if known != attributions.len() {
        return Err(invalid(format!(
            "line {id} has a discount attribution from a source no cart holds"
        )));
    }

    let off = Money::sum(attributions.iter().map(|a| a.amount), currency)
        .map_err(|e| invalid(format!("line {id}'s discounts do not add up: {e}")))?;
    if off.minor() != cells.int("discount_minor")? {
        return Err(invalid(format!(
            "line {id}'s discount is not the sum of its attributions"
        )));
    }
    let tax = LineTax {
        net: money(cells.int("net_minor")?),
        components,
        tax_total: money(cells.int("tax_minor")?),
        gross: money(cells.int("total_minor")?),
        supply_reason: supply.reason,
    };
    let taxable = match price_mode {
        PriceMode::Inclusive => tax.gross,
        PriceMode::Exclusive => tax.net,
    };
    let gross_before_discount = taxable
        .checked_add(off)
        .map_err(|e| invalid(format!("line {id}'s gross does not add up: {e}")))?;
    let priced = PricedLine {
        line_id: id,
        gross_before_discount,
        discount_attributions: attributions,
        taxable,
        line_total: tax.gross,
        tax,
    };
    Ok((line, priced))
}

fn decode_component(cells: &Cells, money: impl Fn(i64) -> Money) -> Result<ComponentTax, DbError> {
    let code = cells.text("component_code")?.to_owned();
    if cells.text("calculation_kind")? != "ad_valorem" || cells.text("base_kind")? != "line_net" {
        return Err(invalid(format!(
            "component {code} is not charged as a rate on the line's net"
        )));
    }
    Ok(ComponentTax {
        treatment: cells.token("treatment", treatment_from_token)?,
        basis: TaxBasis::Percentage {
            rate: Percent::from_ppm(cells.int("rate_ppm")?),
        },
        base_amount: money(cells.int("taxable_base_minor")?),
        amount: money(cells.int("tax_minor")?),
        code,
    })
}

/// The stored summary rows, in the order [`summarize_tax`] gives the lines'
/// own results: the receipt's rows are stored, not derived, and here they are
/// held to the lines they summarise. Rows that are not exactly the lines'
/// summary are refused.
fn decode_summary(
    cells: &[Cells],
    taxes: &[LineTax],
    money: impl Fn(i64) -> Money,
    reason: Option<ZeroRatingReason>,
) -> Result<Vec<TaxSummaryRow>, DbError> {
    let mut stored = cells
        .iter()
        .map(|row| {
            if row.text("calculation_kind")? != "ad_valorem" {
                return Err(invalid("a summary row is charged by amount".to_owned()));
            }
            Ok(TaxSummaryRow {
                code: row.text("component_code")?.to_owned(),
                treatment: row.token("treatment", treatment_from_token)?,
                rate: Percent::from_ppm(row.int("rate_ppm")?),
                per_unit: None,
                reason,
                net: money(row.int("net_minor")?),
                tax: money(row.int("tax_minor")?),
                gross: money(row.int("gross_minor")?),
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    let expected = summarize_tax(taxes)
        .map_err(|e| invalid(format!("the lines' taxes do not summarise: {e}")))?;
    for row in &expected {
        let at = stored
            .iter()
            .position(|candidate| candidate == row)
            .ok_or_else(|| invalid(format!("the tax summary has no row for {}", row.code)))?;
        stored.swap_remove(at);
    }
    if stored.is_empty() {
        Ok(expected)
    } else {
        Err(invalid(
            "the tax summary has a row the lines do not add up to".to_owned(),
        ))
    }
}

/// A discount as it was asked for: what [`LineDiscount`] and
/// [`BasketDiscount`] share, and what `0009` stores for each in six cells.
struct Asked {
    kind: DiscountKind,
    reason: DiscountReason,
    by: UserId,
    approver: Option<UserId>,
}

/// A discount as it was asked for, from `prefix`'s six cells.
fn asked_from(cells: &Cells, prefix: &str, currency: Currency) -> Result<Option<Asked>, DbError> {
    let column = |name: &str| format!("{prefix}_{name}");
    let Some(kind) = cells.opt_text(&column("kind"))? else {
        return Ok(None);
    };
    let kind = match kind {
        "percent" => DiscountKind::Percent(Percent::from_ppm(cells.int(&column("ppm"))?)),
        "amount" => DiscountKind::Amount(Money::from_minor(
            cells.int(&column("amount_minor"))?,
            currency,
        )),
        other => {
            return Err(invalid(format!(
                "{}.{prefix}_kind is {other:?}",
                cells.table
            )));
        }
    };
    Ok(Some(Asked {
        kind,
        reason: DiscountReason(cells.text(&column("reason"))?.to_owned()),
        by: UserId::from_uuid(cells.id(&column("by"))?),
        approver: cells.opt_id(&column("approver"))?.map(UserId::from_uuid),
    }))
}

/// Which part of a sale would not come back as it went in, for the refusal.
fn first_difference(read: &StoredCompletedSale, given: &StoredCompletedSale) -> String {
    let mut lines = read.cart.lines.iter().zip(&given.cart.lines).zip(1_usize..);
    if let Some((_, n)) = lines.find(|((a, b), _)| a != b) {
        return format!("line {n} would not read back as it was captured");
    }
    let mut priced = read
        .priced
        .lines
        .iter()
        .zip(&given.priced.lines)
        .zip(1_usize..);
    if let Some((_, n)) = priced.find(|((a, b), _)| a != b) {
        return format!("line {n}'s prices would not read back as they were paid");
    }
    if read.cart != given.cart {
        return "the cart's header would not read back as it was".to_owned();
    }
    if read.priced != given.priced {
        return "the receipt's totals or tax summary would not read back as they were".to_owned();
    }
    let mut tenders = read.tenders.iter().zip(&given.tenders).zip(1_usize..);
    if let Some((_, n)) = tenders.find(|((a, b), _)| a != b) {
        return format!("tender {n} would not read back as it was taken");
    }
    "its cash rounding, change or document facts would not read back as they were".to_owned()
}

// ── The schema's spelling, mapped explicitly (`ref/domain-api.md` §1.2) ──

const fn price_mode_token(mode: PriceMode) -> &'static str {
    match mode {
        PriceMode::Inclusive => "inclusive",
        PriceMode::Exclusive => "exclusive",
    }
}

fn price_mode_from(token: &str) -> Option<PriceMode> {
    match token {
        "inclusive" => Some(PriceMode::Inclusive),
        "exclusive" => Some(PriceMode::Exclusive),
        _ => None,
    }
}

const fn source_token(source: DiscountSource) -> &'static str {
    match source {
        DiscountSource::ManualLine => "manual_line",
        DiscountSource::ManualBasket => "manual_basket",
    }
}

const fn destination_token(destination: SupplyDestination) -> &'static str {
    match destination {
        SupplyDestination::Domestic => "domestic",
        SupplyDestination::Export => "export",
        SupplyDestination::FreeZone => "free_zone",
        SupplyDestination::DevelopmentArea => "development_area",
        SupplyDestination::EligibleBody => "eligible_body",
    }
}

fn destination_from(token: &str) -> Option<SupplyDestination> {
    match token {
        "domestic" => Some(SupplyDestination::Domestic),
        "export" => Some(SupplyDestination::Export),
        "free_zone" => Some(SupplyDestination::FreeZone),
        "development_area" => Some(SupplyDestination::DevelopmentArea),
        "eligible_body" => Some(SupplyDestination::EligibleBody),
        _ => None,
    }
}

const fn reason_token(reason: ZeroRatingReason) -> &'static str {
    match reason {
        ZeroRatingReason::Export => "export",
        ZeroRatingReason::FreeZoneSupply => "free_zone_supply",
        ZeroRatingReason::EligibleEntity => "eligible_entity",
        ZeroRatingReason::ProductCategory => "product_category",
    }
}

fn reason_from(token: &str) -> Option<ZeroRatingReason> {
    match token {
        "export" => Some(ZeroRatingReason::Export),
        "free_zone_supply" => Some(ZeroRatingReason::FreeZoneSupply),
        "eligible_entity" => Some(ZeroRatingReason::EligibleEntity),
        "product_category" => Some(ZeroRatingReason::ProductCategory),
        _ => None,
    }
}

fn tender_state_from(token: &str) -> Option<TenderState> {
    TenderState::ALL
        .into_iter()
        .find(|state| state.as_str() == token)
}

/// The code an override's reason is stored as, read back. The shelf's price
/// reads as its own variant, which loses nothing: `OverrideReason` compares by
/// code, so `Code("displayed_price")` is the same reason.
fn override_reason_from(code: &str) -> OverrideReason {
    if code == OverrideReason::DISPLAYED_PRICE {
        OverrideReason::DisplayedPrice
    } else {
        OverrideReason::Code(code.to_owned())
    }
}

fn timestamp(cells: &Cells, column: &str) -> Result<Timestamp, DbError> {
    Timestamp::parse_iso8601(cells.text(column)?)
        .map_err(|e| invalid(format!("{}.{column}: {e}", cells.table)))
}

fn id_from(bytes: &[u8], table: &'static str, column: &'static str) -> Result<Uuid, DbError> {
    Uuid::from_slice(bytes).map_err(|_| DbError::IdWidthInvalid {
        table,
        column,
        found: bytes.len(),
    })
}

fn not_storable(reason: String) -> DbError {
    DbError::SaleNotStorable { reason }
}

fn invalid(reason: String) -> DbError {
    DbError::StoredSaleInvalid { reason }
}
