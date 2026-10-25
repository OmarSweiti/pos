//! Products: the catalogue's lookups and its one save path (microstep 1.2.3).
//!
//! Every way a cashier reaches a product goes through here — a scan
//! ([`ProductRepository::by_barcode`]), a scale label's item code
//! ([`ProductRepository::by_plu`]), a typed search
//! ([`ProductRepository::search`]) and a line's product id
//! ([`ProductRepository::by_id`]) — and every way a product reaches the
//! catalogue goes through [`ProductRepository::upsert`].
//!
//! What this repository holds to, beyond the schema:
//!
//! * **A retired product is not found.** `deleted_at` is a tombstone, so every
//!   lookup skips it, and an upsert refuses to write over one
//!   ([`DbError::ProductRetired`]). An *inactive* product is different: it is
//!   found, carrying `is_active = false`, so the cart can refuse it by name
//!   (E.38) instead of the till reporting a stocked item as unknown — which
//!   would offer a quick-add of a product that already exists.
//! * **A save is checked by the domain first.** [`Product::check_saveable`]
//!   refuses a price above the ministry ceiling (E.71) and an `is_weighed` that
//!   disagrees with the unit, before any row is touched. `pos-db` knows no
//!   business rule (conventions §3); it calls the one the domain owns.
//! * **A barcode has one live owner** (E.36). `0003`'s partial unique index
//!   refuses a second live claim; [`ProductRepository::add_barcode`] asks
//!   first, so the refusal names the product holding the code.
//! * **The typed text never becomes FTS5 syntax** (E.39b) —
//!   [`crate::search::match_expression`] builds the expression, and a search
//!   that finds nothing is an empty list, never an error.
//!
//! ## How a `Product` maps to `product`
//!
//! * `unit` stores the dimension, the enum names the unit: `Each` ↔ `'each'`,
//!   `Package` ↔ `'package'`, `Kilogram` ↔ `'weight'`, `Litre` ↔ `'volume'`,
//!   `Metre` ↔ `'length'`. One for one since 1.2.3 removed `Gram` and
//!   `Millilitre` (`ref/domain-api.md` §4.0).
//! * `qty_step_milli` has no field. It is written from the unit — 1000 for
//!   counted goods, which `0003`'s triggers require, and 1 for measures, the
//!   step the cart's own rule implies (whole units for counted goods, any
//!   milli-unit otherwise) — so the database never refuses a quantity the
//!   domain accepted.
//! * `name` is `0001`'s column, still `NOT NULL`. It is written as the Arabic
//!   name, the rule `0003`'s backfill used, and read only where `name_ar` is
//!   missing.
//! * `max_price_minor` is in the product's own currency; `check_saveable` has
//!   already refused a ceiling in any other.
//! * `version` and `updated_at` are not written. They belong to the catalogue
//!   sync (Phase 3), which will own what they mean on a register.
//!
//! A product with no tax category is legal in storage only while it is
//! inactive (`product_active_requires_tax_category_*`), and a `Product` cannot
//! represent one. Lookups therefore pass over such a row — it cannot be sold,
//! and one legacy row must not turn a cashier's search into an error — while
//! [`ProductRepository::by_id`], asked for it by name, reports it.

use pos_domain::{
    Barcode, BarcodeKind, CategoryId, Currency, MatchedOn, Money, Product, ProductHit, ProductId,
    Qty, RegulatedKind, RegulatedSaleForm, SaleForm, TaxCategoryId, UnitOfMeasure,
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use uuid::Uuid;

use crate::{DbError, search};

/// Ids are a UUIDv7's sixteen bytes, as everywhere in `pos-db`.
pub type Id = [u8; 16];

/// Every column a `Product` is read from, in [`StoredProduct::from_row`]'s
/// order, for a query that names the product table `p`.
const PRODUCT_COLUMNS: &str = "p.id, p.sku, COALESCE(p.name_ar, p.name), p.name_en, \
     p.category_id, p.tax_category_id, p.unit, p.price_minor, p.currency, p.is_weighed, \
     p.is_service, p.is_active, p.min_age, p.max_price_minor, p.reorder_point_milli, \
     p.regulated_kind, p.sale_form";

/// How many columns [`PRODUCT_COLUMNS`] reads, so a query can append its own.
const PRODUCT_COLUMN_COUNT: usize = 17;

/// Reads and saves catalogue products.
pub struct ProductRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ProductRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// The product with this id, unless it has been retired.
    ///
    /// An inactive product is returned: refusing to sell it is the cart's
    /// decision (E.38), and a refund reads the sale line, not this (I-5).
    ///
    /// # Errors
    ///
    /// [`DbError::StoredProductInvalid`] for a row a `Product` cannot hold —
    /// one with no tax category, say — rather than a guess at it.
    pub fn by_id(&self, id: ProductId) -> Result<Option<Product>, DbError> {
        let stored = self
            .conn
            .query_row(
                &format!("SELECT {PRODUCT_COLUMNS} FROM product p WHERE p.id = ?1 AND p.deleted_at IS NULL"),
                [id.as_uuid().as_bytes()],
                StoredProduct::from_row,
            )
            .optional()?;
        stored.map(StoredProduct::into_product).transpose()
    }

    /// The products a scanned code belongs to, each with the code's own pack
    /// quantity (E.78).
    ///
    /// A `Vec` because the scan path's contract is a list of candidates for
    /// `resolve_scan`, which turns more than one into
    /// `ScanLookup::Ambiguous` (E.36). Today the partial unique index allows
    /// one live claim, so the list holds at most one; if that ever loosens, it
    /// is ordered newest active first, as E.36 asks.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredProductInvalid`] for a row that cannot be read.
    pub fn by_barcode(&self, code: &str) -> Result<Vec<ProductHit>, DbError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {PRODUCT_COLUMNS}, b.code, b.kind, b.pack_qty_milli
               FROM barcode b
               JOIN product p ON p.id = b.product_id
              WHERE b.code = ?1
                AND b.deleted_at IS NULL
                AND p.deleted_at IS NULL
                AND p.tax_category_id IS NOT NULL
              ORDER BY p.is_active DESC, p.rowid DESC"
        ))?;
        let rows = statement.query_map([code], |row| {
            Ok((
                StoredProduct::from_row(row)?,
                row.get::<_, String>(PRODUCT_COLUMN_COUNT)?,
                row.get::<_, String>(PRODUCT_COLUMN_COUNT + 1)?,
                row.get::<_, i64>(PRODUCT_COLUMN_COUNT + 2)?,
            ))
        })?;
        let mut hits = Vec::new();
        for row in rows {
            let (stored, code, kind, pack_qty_milli) = row?;
            hits.push(ProductHit {
                product: stored.into_product()?,
                matched_on: MatchedOn::Barcode {
                    code,
                    kind: barcode_kind_from_token(&kind)?,
                    pack_qty: Qty::from_milli(pack_qty_milli),
                },
            });
        }
        Ok(hits)
    }

    /// The product a live PLU code belongs to — the item code a scale label
    /// carries, or one a cashier keys for loose produce.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredProductInvalid`] for a row that cannot be read.
    pub fn by_plu(&self, code: &str) -> Result<Option<Product>, DbError> {
        let stored = self
            .conn
            .query_row(
                &format!(
                    "SELECT {PRODUCT_COLUMNS}
                       FROM plu_code c
                       JOIN product p ON p.id = c.product_id
                      WHERE c.code = ?1
                        AND c.deleted_at IS NULL
                        AND p.deleted_at IS NULL
                        AND p.tax_category_id IS NOT NULL"
                ),
                [code],
                StoredProduct::from_row,
            )
            .optional()?;
        stored.map(StoredProduct::into_product).transpose()
    }

    /// Products matching what a cashier typed, best first, at most `limit`.
    ///
    /// The scanner-fallback path: any text at all is a valid query, and text
    /// with nothing to search for — empty, spaces, a lone `"` — is an empty
    /// list rather than an error (E.39b). Ranking is FTS5's own: a product
    /// spelled exactly as typed matches both Arabic branches and comes first.
    /// Ties are broken by SKU, so the same query lists the same order twice.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] only for a database that cannot be read, and
    /// [`DbError::StoredProductInvalid`] for a row that cannot be — never for
    /// anything typed.
    pub fn search(&self, typed: &str, limit: u32) -> Result<Vec<ProductHit>, DbError> {
        let Some(expression) = search::match_expression(typed) else {
            return Ok(Vec::new());
        };
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut statement = self.conn.prepare(&format!(
            "SELECT {PRODUCT_COLUMNS}
               FROM product_fts f
               JOIN product_fts_map m ON m.rowid = f.rowid
               JOIN product p ON p.id = m.product_id
              WHERE product_fts MATCH ?1
                AND p.deleted_at IS NULL
                AND p.tax_category_id IS NOT NULL
              ORDER BY f.rank, p.sku
              LIMIT ?2"
        ))?;
        let rows = statement.query_map(params![expression, limit], StoredProduct::from_row)?;
        let mut hits = Vec::new();
        for row in rows {
            hits.push(ProductHit {
                product: row?.into_product()?,
                matched_on: MatchedOn::Name,
            });
        }
        Ok(hits)
    }

    /// Save a product: insert it, or replace every field of the live row with
    /// this id.
    ///
    /// One statement, `INSERT … ON CONFLICT (id) DO UPDATE`, so `0007`'s
    /// update trigger re-indexes the product for search — never
    /// `INSERT OR REPLACE`, which deletes the row and would drop it from the
    /// index map. A product with no row is inserted; a live one is updated; a
    /// retired one is refused, because the conflict clause only updates where
    /// `deleted_at IS NULL` and a write that changed nothing is reported.
    ///
    /// The statement is prepared once per connection and reused. Preparing it
    /// compiles every trigger on `product` and both of `0007`'s index triggers
    /// into it, and a catalogue load — 1.2.7's 50 000-product fixture, or a
    /// full catalogue sync — spent most of its time doing that again for
    /// every save.
    ///
    /// # Errors
    ///
    /// [`DbError::CatalogSaveRefused`] for what [`Product::check_saveable`]
    /// refuses, [`DbError::ProductRetired`] for a tombstoned id, and
    /// [`DbError::Sqlite`] for what the schema refuses — a SKU another product
    /// holds, an unknown category or tax category.
    pub fn upsert(&self, tx: &Transaction<'_>, product: &Product) -> Result<(), DbError> {
        product.check_saveable()?;
        let changed = tx.prepare_cached(
            "INSERT INTO product (id, sku, name, name_ar, name_en, category_id, tax_category_id,
                                  unit, qty_step_milli, is_weighed, is_service, is_active,
                                  price_minor, currency, min_age, max_price_minor,
                                  reorder_point_milli, regulated_kind, sale_form)
             VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT (id) DO UPDATE SET
                 sku = excluded.sku, name = excluded.name, name_ar = excluded.name_ar,
                 name_en = excluded.name_en, category_id = excluded.category_id,
                 tax_category_id = excluded.tax_category_id, unit = excluded.unit,
                 qty_step_milli = excluded.qty_step_milli, is_weighed = excluded.is_weighed,
                 is_service = excluded.is_service, is_active = excluded.is_active,
                 price_minor = excluded.price_minor, currency = excluded.currency,
                 min_age = excluded.min_age, max_price_minor = excluded.max_price_minor,
                 reorder_point_milli = excluded.reorder_point_milli,
                 regulated_kind = excluded.regulated_kind, sale_form = excluded.sale_form
             WHERE product.deleted_at IS NULL",
        )?
        .execute(params![
                product.id.as_uuid().as_bytes(),
                product.sku,
                product.name_ar,
                product.name_en,
                product.category_id.map(|id| id.as_uuid().into_bytes()),
                product.tax_category_id.as_uuid().as_bytes(),
                unit_token(product.unit),
                qty_step_milli(product.unit),
                product.is_weighed,
                product.is_service,
                product.is_active,
                product.unit_price.minor(),
                product.unit_price.currency().code(),
                product.min_age,
                product.max_price.map(Money::minor),
                product.reorder_point.map(Qty::milli),
                product.regulated_kind().map(RegulatedKind::as_str),
                product.sale_form().as_str(),
            ])?;
        if changed == 0 {
            return Err(DbError::ProductRetired(product.id));
        }
        Ok(())
    }

    /// Give a product a code, with the row id the caller minted.
    ///
    /// E.36: a code a live barcode row already holds is refused, naming the
    /// holder, whether the holder is another product or this one. Retiring the
    /// old claim (`deleted_at`) is what frees the code; the partial unique
    /// index `idx_barcode_code_live` stays the backstop beneath this check.
    /// The code's pack quantity and kind are `0003`'s triggers' to police.
    ///
    /// # Errors
    ///
    /// [`DbError::BarcodeAlreadyClaimed`], or [`DbError::Sqlite`] for what the
    /// schema refuses.
    pub fn add_barcode(
        &self,
        tx: &Transaction<'_>,
        id: &Id,
        barcode: &Barcode,
    ) -> Result<(), DbError> {
        let holder: Option<Vec<u8>> = tx
            .query_row(
                "SELECT product_id FROM barcode WHERE code = ?1 AND deleted_at IS NULL",
                [&barcode.code],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(holder) = holder {
            return Err(DbError::BarcodeAlreadyClaimed {
                code: barcode.code.clone(),
                holder: ProductId::from_uuid(decode_id("barcode", "product_id", holder)?),
            });
        }
        tx.execute(
            "INSERT INTO barcode (id, product_id, code, kind, pack_qty_milli, is_primary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.as_slice(),
                barcode.product_id.as_uuid().as_bytes(),
                barcode.code,
                barcode_kind_token(barcode.kind),
                barcode.pack_qty.milli(),
                barcode.is_primary,
            ],
        )?;
        Ok(())
    }
}

/// One `product` row exactly as stored, before it is asked to be a `Product`.
///
/// The split exists because a `query_map` closure can only fail with a
/// `rusqlite::Error`, and a row that reads cleanly as SQL can still be one the
/// domain cannot hold — an unknown currency, an age above 255.
struct StoredProduct {
    id: Vec<u8>,
    sku: String,
    name_ar: String,
    name_en: Option<String>,
    category_id: Option<Vec<u8>>,
    tax_category_id: Option<Vec<u8>>,
    unit: String,
    price_minor: i64,
    currency: String,
    is_weighed: bool,
    is_service: bool,
    is_active: bool,
    min_age: Option<i64>,
    max_price_minor: Option<i64>,
    reorder_point_milli: Option<i64>,
    regulated_kind: Option<String>,
    sale_form: String,
}

impl StoredProduct {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            sku: row.get(1)?,
            name_ar: row.get(2)?,
            name_en: row.get(3)?,
            category_id: row.get(4)?,
            tax_category_id: row.get(5)?,
            unit: row.get(6)?,
            price_minor: row.get(7)?,
            currency: row.get(8)?,
            is_weighed: row.get(9)?,
            is_service: row.get(10)?,
            is_active: row.get(11)?,
            min_age: row.get(12)?,
            max_price_minor: row.get(13)?,
            reorder_point_milli: row.get(14)?,
            regulated_kind: row.get(15)?,
            sale_form: row.get(16)?,
        })
    }

    fn into_product(self) -> Result<Product, DbError> {
        let id = ProductId::from_uuid(decode_id("product", "id", self.id)?);
        let Some(tax_category_id) = self.tax_category_id else {
            return Err(invalid(format!(
                "product {id} has no tax category, so it cannot be priced into a line"
            )));
        };
        let currency = Currency::from_code(&self.currency).map_err(|_| {
            invalid(format!(
                "product {id} is priced in unknown currency {:?}",
                self.currency
            ))
        })?;
        let min_age = self.min_age.map(u8::try_from).transpose().map_err(|_| {
            invalid(format!(
                "product {id} has min_age {:?}, which is no age",
                self.min_age
            ))
        })?;
        let regulated_kind = self
            .regulated_kind
            .as_deref()
            .map(|token| regulated_kind_from_token(id, token))
            .transpose()?;
        let regulated_sale_form = RegulatedSaleForm::from_parts(
            regulated_kind,
            sale_form_from_token(id, &self.sale_form)?,
        )
        .map_err(|refused| invalid(format!("product {id}: {refused}")))?;
        Ok(Product {
            id,
            sku: self.sku,
            name_ar: self.name_ar,
            name_en: self.name_en,
            category_id: self
                .category_id
                .map(|bytes| decode_id("product", "category_id", bytes).map(CategoryId::from_uuid))
                .transpose()?,
            tax_category_id: TaxCategoryId::from_uuid(decode_id(
                "product",
                "tax_category_id",
                tax_category_id,
            )?),
            unit: unit_from_token(id, &self.unit)?,
            unit_price: Money::from_minor(self.price_minor, currency),
            is_weighed: self.is_weighed,
            is_service: self.is_service,
            is_active: self.is_active,
            min_age,
            max_price: self
                .max_price_minor
                .map(|minor| Money::from_minor(minor, currency)),
            reorder_point: self.reorder_point_milli.map(Qty::from_milli),
            regulated_sale_form,
        })
    }
}

/// The dimension `product.unit` stores for a unit.
const fn unit_token(unit: UnitOfMeasure) -> &'static str {
    match unit {
        UnitOfMeasure::Each => "each",
        UnitOfMeasure::Package => "package",
        UnitOfMeasure::Kilogram => "weight",
        UnitOfMeasure::Litre => "volume",
        UnitOfMeasure::Metre => "length",
    }
}

/// The unit a stored dimension means. The inverse of [`unit_token`], and a
/// named error for anything else — `0003`'s `CHECK` admits nothing else, so
/// reaching that arm means the row was not written through this schema.
fn unit_from_token(id: ProductId, token: &str) -> Result<UnitOfMeasure, DbError> {
    match token {
        "each" => Ok(UnitOfMeasure::Each),
        "package" => Ok(UnitOfMeasure::Package),
        "weight" => Ok(UnitOfMeasure::Kilogram),
        "volume" => Ok(UnitOfMeasure::Litre),
        "length" => Ok(UnitOfMeasure::Metre),
        other => Err(invalid(format!("product {id} has unit {other:?}"))),
    }
}

/// `qty_step_milli` for a unit: whole units for counted goods, any milli-unit
/// for a measure.
const fn qty_step_milli(unit: UnitOfMeasure) -> i64 {
    if unit.is_divisible() { 1 } else { 1_000 }
}

fn regulated_kind_from_token(id: ProductId, token: &str) -> Result<RegulatedKind, DbError> {
    [RegulatedKind::Tobacco]
        .into_iter()
        .find(|kind| kind.as_str() == token)
        .ok_or_else(|| invalid(format!("product {id} has regulated_kind {token:?}")))
}

fn sale_form_from_token(id: ProductId, token: &str) -> Result<SaleForm, DbError> {
    [SaleForm::SealedPack, SaleForm::Bulk, SaleForm::Service]
        .into_iter()
        .find(|form| form.as_str() == token)
        .ok_or_else(|| invalid(format!("product {id} has sale_form {token:?}")))
}

/// `barcode.kind`'s token for a kind: `0003`'s `CHECK` list, which is also the
/// enum's wire form (`each_stored_token_is_spelled_once` pins both).
const fn barcode_kind_token(kind: BarcodeKind) -> &'static str {
    match kind {
        BarcodeKind::Ean13 => "ean13",
        BarcodeKind::Ean8 => "ean8",
        BarcodeKind::Upca => "upca",
        BarcodeKind::Code128 => "code128",
        BarcodeKind::Internal => "internal",
        BarcodeKind::PriceEmbedded => "price_embedded",
        BarcodeKind::WeightEmbedded => "weight_embedded",
    }
}

fn barcode_kind_from_token(token: &str) -> Result<BarcodeKind, DbError> {
    [
        BarcodeKind::Ean13,
        BarcodeKind::Ean8,
        BarcodeKind::Upca,
        BarcodeKind::Code128,
        BarcodeKind::Internal,
        BarcodeKind::PriceEmbedded,
        BarcodeKind::WeightEmbedded,
    ]
    .into_iter()
    .find(|kind| barcode_kind_token(*kind) == token)
    .ok_or_else(|| invalid(format!("barcode kind {token:?}")))
}

/// A stored `BLOB(16)` id, or a named error for any other width.
fn decode_id(table: &'static str, column: &'static str, bytes: Vec<u8>) -> Result<Uuid, DbError> {
    Uuid::from_slice(&bytes).map_err(|_| DbError::IdWidthInvalid {
        table,
        column,
        found: bytes.len(),
    })
}

fn invalid(reason: String) -> DbError {
    DbError::StoredProductInvalid { reason }
}
