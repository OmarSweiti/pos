//! Catalogue value types: what a product is, and what a scanned code means.
//!
//! Three rules shape this file.
//!
//! **A domain type carries its unit in its type, never in its name.** The
//! database spells the columns `max_price_minor`, `reorder_point_milli` and
//! `barcode.pack_qty_milli`, because a bare SQLite integer says nothing about
//! what it counts (conventions §2). Here the same three values are [`Money`],
//! [`Qty`] and [`Qty`], which do say, so the suffix would be noise on a type
//! that already refuses to be added to the wrong thing (I-1, I-2, I-3). A
//! `max_price_minor: i64` field in this crate would be the same value with the
//! currency thrown away.
//!
//! **A regulated product cannot be given an unlawful sale form, and the type
//! system is what says so.** Jordan's tobacco rules forbid selling single
//! cigarettes (`ref/merchant-decisions.md` 5.3b), and the age gate is a
//! *different* control: refusing to sell to a
//! seventeen-year-old does not make an individual-cigarette SKU lawful. So the
//! regulated class and the sale form are one field of one type that has no
//! unlawful inhabitant — [`RegulatedSaleForm`] — rather than two independent
//! fields and a rule written in a comment. Migration `0003`'s
//! `product_regulated_sale_form_insert`/`_update` triggers refuse the same pair
//! in the storage engine, which is defence in depth against a hand-typed
//! `sqlite3` session and a repository with a bug in it. The type is the control.
//!
//! **The pack quantity belongs to the code, not to the product.** One product is
//! sold by the unit and by the case, and each of those is a [`Barcode`] with its
//! own [`Barcode::pack_qty`]. A multiplier stored on the product could not
//! express both, which is how a case of cola comes to charge for one can.
//!
//! Scan parsing, `PriceSource` and `ScanLookup` (`ref/domain-api.md` §4.1–§4.3)
//! arrive with microstep 1.2.4 and are deliberately absent here: nothing in this
//! file decides a price.

use serde::{Deserialize, Serialize};

use crate::ids::{CategoryId, ProductId, TaxCategoryId};
use crate::money::{Money, Qty};

/// The unit a product is priced and sold in.
///
/// Arithmetic never branches on it: a [`Qty`] is milli-units whether it counts
/// cans or grams (I-3). It exists to answer the one question a till has to ask
/// before it accepts a keyed quantity — see [`UnitOfMeasure::is_divisible`] —
/// and to label a quantity on a receipt.
///
/// **The stored `product.unit` column is coarser than this enum on purpose.**
/// Migration `0003` records `'each'`, `'package'`, `'weight'`, `'volume'` or
/// `'length'` beside `qty_step_milli`, so `Kilogram` and `Gram` both store as
/// `'weight'` and a round trip through the database cannot tell them apart. That
/// gap is real and is not this type's to close: microstep 1.2.3 owns the
/// repository mapping, and closing it needs either a finer column or a coarser
/// enum, which is a reviewed decision rather than a cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitOfMeasure {
    Each,
    Kilogram,
    Gram,
    Litre,
    Millilitre,
    Metre,
    Package,
}

impl UnitOfMeasure {
    /// Whether a fractional quantity of this unit means anything.
    ///
    /// `Each` and `Package` are counted, not measured. Selling 0.001 of a can is
    /// not a smaller sale, it is a mispriced one (`ref/domain-api.md` §6.3), and
    /// `0003` says the same thing in SQL by pinning
    /// `qty_step_milli = 1000` for `'each'` and `'package'`. Everything else is
    /// a measure a scale reports a fraction of.
    ///
    /// `const` because a quantity guard has nothing to allocate and nothing to
    /// fail at, and the whole implementation is a `match` over seven variants.
    /// The `match` is exhaustive with no wildcard arm, so an eighth unit does
    /// not compile until somebody decides which side it falls on.
    pub const fn is_divisible(self) -> bool {
        match self {
            UnitOfMeasure::Each | UnitOfMeasure::Package => false,
            UnitOfMeasure::Kilogram
            | UnitOfMeasure::Gram
            | UnitOfMeasure::Litre
            | UnitOfMeasure::Millilitre
            | UnitOfMeasure::Metre => true,
        }
    }
}

/// A class of goods whose sale is restricted by something other than its price.
///
/// One variant today, and `0003`'s `CHECK (regulated_kind IN ('tobacco'))` says
/// the same. It is an enum rather than an `is_tobacco: bool` because the next
/// class — alcohol, a pharmacy line — brings its own rule, and a boolean is the
/// field that has to be renamed the day it arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegulatedKind {
    /// Tobacco. No single sticks, and no promotion or label advertising
    /// (`ref/merchant-decisions.md` 5.3b). This file owns the first of those.
    Tobacco,
}

impl RegulatedKind {
    /// The token `product.regulated_kind` stores, which is also the token this
    /// enum serialises as.
    ///
    /// It exists so the spelling lives in exactly one place: an error message a
    /// human reads, a `product` row and a sync payload all quote this string,
    /// and `each_stored_token_is_spelled_once` fails if the derive and this
    /// `match` ever disagree. The inverse — a token back into the enum — is
    /// deliberately absent: whoever parses one has to decide what a corrupt
    /// value becomes, and that decision belongs to the repository that reads
    /// the column (microstep 1.2.3), together with the `DbError` it returns.
    pub const fn as_str(self) -> &'static str {
        match self {
            RegulatedKind::Tobacco => "tobacco",
        }
    }
}

/// How a product leaves the shop.
///
/// The three variants are `0003`'s
/// `CHECK (sale_form IN ('sealed_pack','bulk','service'))` and nothing else, so
/// the domain and the column cannot drift apart in what they can express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaleForm {
    /// A sealed manufacturer pack, sold whole. The only lawful form for a
    /// regulated class, which is why [`RegulatedSaleForm`] can carry a regulated
    /// class only here.
    SealedPack,
    /// Loose goods: produce, bakery, anything a scale or a scoop measures out.
    Bulk,
    /// No physical thing leaves the shop — an e-recharge, a fee, a service
    /// charge. Compare [`Product::is_service`], which answers the narrower
    /// question of whether the sale moves stock.
    Service,
}

impl SaleForm {
    /// The token `product.sale_form` stores, and this enum's wire form. See
    /// [`RegulatedKind::as_str`] for why the parse direction is not here.
    pub const fn as_str(self) -> &'static str {
        match self {
            SaleForm::SealedPack => "sealed_pack",
            SaleForm::Bulk => "bulk",
            SaleForm::Service => "service",
        }
    }
}

/// A regulated class together with the sale form it permits — one value, because
/// the two facts are not independent.
///
/// **The unlawful pair has no inhabitant.** `Tobacco` carries no sale form,
/// because tobacco has exactly one lawful form, so there is no value of this
/// type that means "tobacco sold loose" — not a value a constructor forgot to
/// check, not one a `struct` literal assembled in another crate, and not one a
/// sync payload deserialised. That is the strongest guarantee available here and
/// it is the reason for the shape: the alternative,
///
/// ```text
/// pub regulated_kind: Option<RegulatedKind>,   // two public fields
/// pub sale_form: SaleForm,                     // and six combinations
/// ```
///
/// can express the individual-cigarette SKU that Jordanian tobacco rules
/// forbid, and a `Product::new` that refused it would be bypassed by the first
/// `Product { .. }` literal a later microstep writes. Making the fields private
/// instead would buy the same guarantee at the cost of a sixteen-argument
/// constructor or a duplicate sixteen-field draft struct, and would move the
/// refusal away from the two values it is about.
///
/// The two column names remain addressable, on this type and on [`Product`], as
/// [`regulated_kind`](RegulatedSaleForm::regulated_kind) and
/// [`sale_form`](RegulatedSaleForm::sale_form), and the pair travels on the wire
/// exactly as `product` stores it — see the `SaleFormColumns` note below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "SaleFormColumns", try_from = "SaleFormColumns")]
pub enum RegulatedSaleForm {
    /// No regulated class, and therefore a free choice among the three forms.
    Unregulated(SaleForm),
    /// Tobacco, which needs no sale form: `sale_form()` answers
    /// [`SaleForm::SealedPack`] and nothing can make it answer otherwise.
    Tobacco,
}

/// Everything this module refuses.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CatalogError {
    /// The pair `0003`'s `product_regulated_sale_form_*` triggers also refuse.
    /// It carries both halves because "unlawful" is unactionable without them:
    /// a merchant fixing a bad import needs to know which class and which form.
    #[error("a {} product cannot be sold as {}", .0.as_str(), .1.as_str())]
    UnlawfulSaleForm(RegulatedKind, SaleForm),
}

impl RegulatedSaleForm {
    /// Every unregulated product's default, and the schema's
    /// `sale_form DEFAULT 'sealed_pack'`.
    pub const UNREGULATED_SEALED_PACK: RegulatedSaleForm =
        RegulatedSaleForm::Unregulated(SaleForm::SealedPack);

    /// Build the pair from the two values a `product` row stores, refusing the
    /// combination the trigger refuses.
    ///
    /// This is the single funnel for every pair that arrived as two separate
    /// values — a database row, a sync payload, a CSV import, the deserialiser
    /// below — which is why the refusal lives here rather than in three callers
    /// that each have to remember it.
    ///
    /// It **refuses** rather than coercing. Reading bulk tobacco as a sealed
    /// pack would silence exactly the SKU the rule exists to prevent, and
    /// reading it as unregulated would drop a restriction; either way a row
    /// somebody has to look at would stop being visible.
    pub const fn from_parts(
        regulated_kind: Option<RegulatedKind>,
        sale_form: SaleForm,
    ) -> Result<RegulatedSaleForm, CatalogError> {
        match (regulated_kind, sale_form) {
            (None, form) => Ok(RegulatedSaleForm::Unregulated(form)),
            (Some(RegulatedKind::Tobacco), SaleForm::SealedPack) => Ok(RegulatedSaleForm::Tobacco),
            (Some(kind), form) => Err(CatalogError::UnlawfulSaleForm(kind, form)),
        }
    }

    /// The regulated class, or `None` for an ordinary product. What
    /// `product.regulated_kind` holds.
    pub const fn regulated_kind(self) -> Option<RegulatedKind> {
        match self {
            RegulatedSaleForm::Unregulated(_) => None,
            RegulatedSaleForm::Tobacco => Some(RegulatedKind::Tobacco),
        }
    }

    /// The sale form. What `product.sale_form` holds — total, because a
    /// regulated class implies its one lawful form rather than omitting it.
    pub const fn sale_form(self) -> SaleForm {
        match self {
            RegulatedSaleForm::Unregulated(form) => form,
            RegulatedSaleForm::Tobacco => SaleForm::SealedPack,
        }
    }
}

/// The two columns `product` stores, and the wire form of a
/// [`RegulatedSaleForm`].
///
/// The pair travels as the schema spells it —
/// `{"regulated_kind":null,"sale_form":"sealed_pack"}` — rather than as the
/// enum's own external tagging, so a `product` row, a sync payload and this file
/// use one vocabulary. Two things follow from routing serde through
/// [`RegulatedSaleForm::from_parts`]: a payload claiming bulk tobacco is
/// **refused on arrival** instead of being read as something lawful, and the
/// refusal has exactly one implementation.
///
/// It is private because it is a representation, not a domain concept: a caller
/// with two loose values calls `from_parts` and gets a typed refusal, and
/// nothing outside this module needs to name the intermediate struct.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct SaleFormColumns {
    regulated_kind: Option<RegulatedKind>,
    sale_form: SaleForm,
}

impl From<RegulatedSaleForm> for SaleFormColumns {
    fn from(value: RegulatedSaleForm) -> SaleFormColumns {
        SaleFormColumns {
            regulated_kind: value.regulated_kind(),
            sale_form: value.sale_form(),
        }
    }
}

impl TryFrom<SaleFormColumns> for RegulatedSaleForm {
    type Error = CatalogError;

    fn try_from(columns: SaleFormColumns) -> Result<RegulatedSaleForm, CatalogError> {
        RegulatedSaleForm::from_parts(columns.regulated_kind, columns.sale_form)
    }
}

/// One catalogue product.
///
/// It is a value, not a handle: a repository returns an owned `Product` and the
/// domain reads it (conventions §3). A completed sale never reads it — I-5 copies
/// the price and the name onto the sale line at capture time, so a report or a
/// refund six months later reads that line and not today's catalogue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Product {
    pub id: ProductId,
    /// The merchant's own code. A lookup key like a barcode, never identity —
    /// identity is [`Product::id`].
    pub sku: String,
    /// Arabic, and not optional: the register is Arabic-first (conventions §10)
    /// and a product with no Arabic name is a product a cashier cannot read.
    pub name_ar: String,
    pub name_en: Option<String>,
    pub category_id: Option<CategoryId>,
    pub tax_category_id: TaxCategoryId,
    pub unit: UnitOfMeasure,
    /// THE price. It lives here so that `PriceSource::from_catalog` (1.2.4) can
    /// take no amount at all: the only way to price a catalogue line is to hand
    /// over a `Product`, which the webview cannot fabricate.
    pub unit_price: Money,
    /// Whether the quantity comes from a scale rather than a count.
    ///
    /// It duplicates information [`Product::unit`] already carries — `0003`'s
    /// `product_quantity_kind_*` triggers tie `'each'`/`'package'` to
    /// `is_weighed = 0` and `'weight'`/`'volume'`/`'length'` to `is_weighed = 1`
    /// — and it exists because a sale line snapshots it (I-5) and because
    /// [`Qty::format`] takes it as a display hint. This type does not police the
    /// agreement between the two fields; the storage engine does, and microstep
    /// 1.2.3 owns which of the two wins when it maps the columns.
    pub is_weighed: bool,
    /// True for a product that moves no stock at all: an e-recharge, a fee (J.1).
    /// It is narrower than [`SaleForm::Service`], which describes the form the
    /// sale takes, and the stock ledger (1.10.x) is what reads this one.
    pub is_service: bool,
    pub is_active: bool,
    /// The minimum age in years, where a law sets one (E.69).
    ///
    /// **A different control from the sale form, and neither substitutes for the
    /// other.** A cashier confirming that a customer is eighteen does not make a
    /// loose-cigarette SKU lawful, and a sealed pack is not exempt from the age
    /// check. That is why this is an ordinary field while the sale form is not.
    pub min_age: Option<u8>,
    /// A ministry price ceiling, where one applies (J.3, E.71).
    ///
    /// `Money`, not `i64`: the column is `max_price_minor` because SQLite has
    /// nowhere else to say what the integer counts, and this field has. Nothing
    /// checks here that the ceiling is in the same currency as
    /// [`Product::unit_price`], because nothing has to — `Money::checked_cmp`
    /// refuses to order two currencies, so a mismatched ceiling becomes a typed
    /// error at the comparison rather than a silently passed ceiling.
    pub max_price: Option<Money>,
    /// The on-hand level at which this product should be reordered, in units.
    pub reorder_point: Option<Qty>,
    /// The regulated class and the sale form, as one value that cannot express
    /// an unlawful pair. [`Product::regulated_kind`] and [`Product::sale_form`]
    /// read the two column values back out.
    pub regulated_sale_form: RegulatedSaleForm,
}

impl Product {
    /// The regulated class, or `None`. What `product.regulated_kind` holds.
    pub const fn regulated_kind(&self) -> Option<RegulatedKind> {
        self.regulated_sale_form.regulated_kind()
    }

    /// The sale form. What `product.sale_form` holds. For a regulated product
    /// this is [`SaleForm::SealedPack`] and no other answer is representable.
    pub const fn sale_form(&self) -> SaleForm {
        self.regulated_sale_form.sale_form()
    }
}

/// The symbology of a scanned code, and — for the last two — the fact that its
/// digits carry a value rather than only an identity.
///
/// The seven variants are `0003`'s `barcode.kind` `CHECK` list, in its order.
/// Parsing the embedded layouts is 1.2.4's work; this enum only says which
/// layout a stored code claims to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarcodeKind {
    Ean13,
    Ean8,
    Upca,
    Code128,
    /// A code the shop printed for itself, and the one symbology with no
    /// external authority to check it against.
    Internal,
    /// A scale label whose digits carry a price.
    PriceEmbedded,
    /// A scale label whose digits carry a weight.
    WeightEmbedded,
}

/// A code that resolves to a product, and how many units it means.
///
/// A product carries several of these — multipacks, supplier relabels — so the
/// code is a lookup key and identity stays with [`Product::id`] (master plan
/// C.1). The `barcode` row's own primary key is not here: the domain reads the
/// lookup fact, and there is no `BarcodeId` among the typed ids because nothing
/// pure has needed to name one row of this table yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Barcode {
    pub product_id: ProductId,
    pub code: String,
    pub kind: BarcodeKind,
    pub is_primary: bool,
    /// How many units this code means: [`Qty::ONE`] for a single unit,
    /// `Qty::from_units(6)` for a six-pack outer.
    ///
    /// **It belongs to the code, not to the product**, because the same product
    /// is sold by the unit and by the case and both codes are live at once. The
    /// master plan and the schema both name multipacks as the reason a product
    /// carries several codes and neither carried the multiplier, so a case of
    /// cola scanned on its outer barcode charged for one can and decremented one
    /// unit.
    ///
    /// The column is `barcode.pack_qty_milli INTEGER NOT NULL DEFAULT 1000` with
    /// `CHECK (pack_qty_milli > 0)`, so every code configured before the column
    /// existed still means one unit. This type does not repeat that check —
    /// `Barcode` has public fields per `ref/domain-api.md` §4 — so a zero or
    /// negative pack quantity is refused at the storage engine and not in
    /// memory.
    pub pack_qty: Qty,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::money::Currency;
    use uuid::Uuid;

    // These exact wire fixtures are review tripwires, in the shape `money.rs`
    // established. A `Product` crosses the IPC boundary and the sync wire, so
    // changing either fixture is an intentional, reviewed act rather than an
    // incidental serde refactor — and the regulated pair travels as the two
    // column names on purpose.
    const GOLDEN_PRODUCT_JSON: &str = concat!(
        r#"{"id":"00000000-0000-0000-0000-000000000001","sku":"COLA-330","#,
        r#""name_ar":"كولا","name_en":"Cola 330ml","category_id":null,"#,
        r#""tax_category_id":"00000000-0000-0000-0000-000000000002","#,
        r#""unit":"each","unit_price":{"minor":450,"currency":"JOD"},"#,
        r#""is_weighed":false,"is_service":false,"is_active":true,"#,
        r#""min_age":null,"max_price":null,"reorder_point":null,"#,
        r#""regulated_sale_form":{"regulated_kind":null,"sale_form":"sealed_pack"}}"#,
    );
    const GOLDEN_BARCODE_JSON: &str = concat!(
        r#"{"product_id":"00000000-0000-0000-0000-000000000001","#,
        r#""code":"5449000000996","kind":"ean13","is_primary":true,"pack_qty":1000}"#,
    );
    const GOLDEN_TOBACCO_PAIR_JSON: &str =
        r#"{"regulated_kind":"tobacco","sale_form":"sealed_pack"}"#;

    // Every unit, listed once. It drives the divisibility sweep below, so
    // `the_unit_table_lists_every_variant_exactly_once` is what stops an eighth
    // unit from slipping past the sweep and the token check at the same time.
    const ALL_UNITS: [UnitOfMeasure; 7] = [
        UnitOfMeasure::Each,
        UnitOfMeasure::Kilogram,
        UnitOfMeasure::Gram,
        UnitOfMeasure::Litre,
        UnitOfMeasure::Millilitre,
        UnitOfMeasure::Metre,
        UnitOfMeasure::Package,
    ];

    // Every sale form and every regulated class, listed once each, so the
    // exhaustive pair sweep really is exhaustive.
    const ALL_SALE_FORMS: [SaleForm; 3] = [SaleForm::SealedPack, SaleForm::Bulk, SaleForm::Service];
    const ALL_REGULATED_KINDS: [RegulatedKind; 1] = [RegulatedKind::Tobacco];

    // Every value `RegulatedSaleForm` can hold. Four, and the point of the type
    // is that there is no fifth.
    const ALL_REGULATED_SALE_FORMS: [RegulatedSaleForm; 4] = [
        RegulatedSaleForm::Unregulated(SaleForm::SealedPack),
        RegulatedSaleForm::Unregulated(SaleForm::Bulk),
        RegulatedSaleForm::Unregulated(SaleForm::Service),
        RegulatedSaleForm::Tobacco,
    ];

    // Obviously synthetic fixture ids: readable in a golden file, and not a
    // UUIDv7, so nobody mistakes one for something a register minted.
    fn fixture_id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn cola() -> Product {
        Product {
            id: ProductId::from_uuid(fixture_id(1)),
            sku: "COLA-330".to_owned(),
            name_ar: "كولا".to_owned(),
            name_en: Some("Cola 330ml".to_owned()),
            category_id: None,
            tax_category_id: TaxCategoryId::from_uuid(fixture_id(2)),
            unit: UnitOfMeasure::Each,
            unit_price: Money::from_minor(450, Currency::JOD),
            is_weighed: false,
            is_service: false,
            is_active: true,
            min_age: None,
            max_price: None,
            reorder_point: None,
            regulated_sale_form: RegulatedSaleForm::UNREGULATED_SEALED_PACK,
        }
    }

    fn barcode(code: &str, pack_qty: Qty) -> Barcode {
        Barcode {
            product_id: ProductId::from_uuid(fixture_id(1)),
            code: code.to_owned(),
            kind: BarcodeKind::Ean13,
            is_primary: pack_qty == Qty::ONE,
            pack_qty,
        }
    }

    #[test]
    fn a_barcode_pack_quantity_is_a_qty() {
        // The field is a `Qty`, so the unit rides in the type: a single-unit
        // code means one unit and a six-pack outer means six, and neither is a
        // bare integer whose meaning depends on the column it came from. The
        // milli-unit values are asserted too, because `Qty::ONE` being 1000
        // milli-units (I-3) is the whole reason `pack_qty_milli DEFAULT 1000`
        // means "one unit" in the schema.
        let single = barcode("5449000000996", Qty::ONE);
        assert_eq!(single.pack_qty, Qty::ONE);
        assert_eq!(single.pack_qty.milli(), 1_000);

        let outer = barcode("5449000000019", Qty::from_units(6).unwrap());
        assert_eq!(outer.pack_qty, Qty::from_milli(6_000));
        assert_eq!(outer.pack_qty.milli(), 6_000);
        assert!(outer.pack_qty.is_whole_units());

        // A weighed pack is expressible in the same field and needs no second
        // representation: half a kilo is 500 milli-units.
        let half_kilo = barcode("2000000000005", Qty::from_milli(500));
        assert_eq!(half_kilo.pack_qty, Qty::from_milli(500));
        assert!(!half_kilo.pack_qty.is_whole_units());
    }

    #[test]
    fn the_same_product_sells_by_unit_and_by_case_through_two_barcodes() {
        // The microstep's "done when", stated as code: ONE product id, TWO
        // barcode records, and the quantity a scan means comes from the record
        // that matched rather than from the product. A multiplier stored on the
        // product could not answer both questions at once.
        let product_id = ProductId::from_uuid(fixture_id(1));
        let codes = [
            barcode("5449000000996", Qty::ONE),
            barcode("5449000000019", Qty::from_units(6).unwrap()),
        ];

        // The lookup a repository will perform in 1.2.3, written here as the
        // pure function it is: a code in, the product and the quantity out.
        let resolve = |scanned: &str| -> Option<(ProductId, Qty)> {
            codes
                .iter()
                .find(|candidate| candidate.code == scanned)
                .map(|found| (found.product_id, found.pack_qty))
        };

        assert_eq!(resolve("5449000000996"), Some((product_id, Qty::ONE)));
        assert_eq!(
            resolve("5449000000019"),
            Some((product_id, Qty::from_units(6).unwrap()))
        );
        assert_eq!(resolve("0000000000000"), None);

        // Same product, different quantities — which is the claim.
        let (first_product, first_qty) = resolve("5449000000996").unwrap();
        let (second_product, second_qty) = resolve("5449000000019").unwrap();
        assert_eq!(first_product, second_product);
        assert_ne!(first_qty, second_qty);
    }

    #[test]
    fn tobacco_product_requires_a_sealed_pack_sale_form() {
        // Tobacco sold loose is the individual-cigarette SKU the rules forbid,
        // and it is refused at construction rather than documented: there is no
        // `RegulatedSaleForm` value that means it, so `from_parts` — the only
        // way two loose column values become one — has to fail.
        assert_eq!(
            RegulatedSaleForm::from_parts(Some(RegulatedKind::Tobacco), SaleForm::Bulk),
            Err(CatalogError::UnlawfulSaleForm(
                RegulatedKind::Tobacco,
                SaleForm::Bulk
            ))
        );
        assert_eq!(
            RegulatedSaleForm::from_parts(Some(RegulatedKind::Tobacco), SaleForm::Service),
            Err(CatalogError::UnlawfulSaleForm(
                RegulatedKind::Tobacco,
                SaleForm::Service
            ))
        );
        // The refusal names both halves, because a merchant repairing an import
        // needs to know which class and which form.
        assert_eq!(
            RegulatedSaleForm::from_parts(Some(RegulatedKind::Tobacco), SaleForm::Bulk)
                .unwrap_err()
                .to_string(),
            "a tobacco product cannot be sold as bulk"
        );

        // The one lawful pair is accepted, and reads back as both columns.
        let sealed =
            RegulatedSaleForm::from_parts(Some(RegulatedKind::Tobacco), SaleForm::SealedPack)
                .unwrap();
        assert_eq!(sealed, RegulatedSaleForm::Tobacco);
        assert_eq!(sealed.regulated_kind(), Some(RegulatedKind::Tobacco));
        assert_eq!(sealed.sale_form(), SaleForm::SealedPack);

        // A tobacco product answers `SealedPack` however it was built, because
        // the value carries no other answer.
        let cigarettes = Product {
            regulated_sale_form: RegulatedSaleForm::Tobacco,
            min_age: Some(18),
            ..cola()
        };
        assert_eq!(cigarettes.regulated_kind(), Some(RegulatedKind::Tobacco));
        assert_eq!(cigarettes.sale_form(), SaleForm::SealedPack);

        // And the age gate is a separate control in both directions: an
        // age-restricted product may be sold loose, and a sealed pack is not
        // exempt from the age check. Neither field implies the other.
        let energy_drink = Product {
            min_age: Some(16),
            regulated_sale_form: RegulatedSaleForm::Unregulated(SaleForm::Bulk),
            ..cola()
        };
        assert_eq!(energy_drink.regulated_kind(), None);
        assert_eq!(energy_drink.sale_form(), SaleForm::Bulk);
        assert_eq!(cigarettes.min_age, Some(18));
    }

    #[test]
    fn the_regulated_pair_round_trips_through_its_two_columns() {
        // A bounded universal claim, so it is checked by exhaustion rather than
        // by generation (conventions §5.1): three sale forms times two regulated
        // states is six pairs, and a proptest over six cases would be a weaker
        // proof with more machinery. Every pair is either refused or survives
        // the trip back out as itself, and nothing is coerced on the way.
        for form in ALL_SALE_FORMS {
            let unregulated = RegulatedSaleForm::from_parts(None, form).unwrap();
            assert_eq!(unregulated.regulated_kind(), None);
            assert_eq!(unregulated.sale_form(), form);

            for kind in ALL_REGULATED_KINDS {
                match RegulatedSaleForm::from_parts(Some(kind), form) {
                    Ok(pair) => {
                        assert_eq!(pair.regulated_kind(), Some(kind));
                        assert_eq!(pair.sale_form(), form);
                        assert_eq!(form, SaleForm::SealedPack, "the only lawful regulated form");
                    }
                    Err(error) => {
                        assert_eq!(error, CatalogError::UnlawfulSaleForm(kind, form));
                        assert_ne!(form, SaleForm::SealedPack);
                    }
                }
            }
        }

        // From the other end: every value the type can hold rebuilds itself from
        // the two columns it reports, so the pair is a faithful projection and
        // not a lossy one. A regulated value always reports the sealed pack.
        for pair in ALL_REGULATED_SALE_FORMS {
            assert_eq!(
                RegulatedSaleForm::from_parts(pair.regulated_kind(), pair.sale_form()),
                Ok(pair)
            );
            if pair.regulated_kind().is_some() {
                assert_eq!(pair.sale_form(), SaleForm::SealedPack);
            }
        }
    }

    #[test]
    fn an_unlawful_regulated_pair_is_refused_on_the_wire() {
        // The same refusal, at the boundary a sync payload arrives through. A
        // JSON document claiming bulk tobacco is a deserialisation error, not a
        // product that quietly became a sealed pack or quietly lost its class.
        assert!(
            serde_json::from_str::<RegulatedSaleForm>(
                r#"{"regulated_kind":"tobacco","sale_form":"bulk"}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<RegulatedSaleForm>(
                r#"{"regulated_kind":"tobacco","sale_form":"service"}"#
            )
            .is_err()
        );
        assert_eq!(
            serde_json::from_str::<RegulatedSaleForm>(GOLDEN_TOBACCO_PAIR_JSON).unwrap(),
            RegulatedSaleForm::Tobacco
        );

        // And through a whole product, which is how one actually arrives.
        let tobacco_product = serde_json::to_string(&Product {
            regulated_sale_form: RegulatedSaleForm::Tobacco,
            ..cola()
        })
        .unwrap();
        assert!(tobacco_product.contains(GOLDEN_TOBACCO_PAIR_JSON));
        assert_eq!(
            serde_json::from_str::<Product>(&tobacco_product)
                .unwrap()
                .sale_form(),
            SaleForm::SealedPack
        );
        assert!(
            serde_json::from_str::<Product>(
                &tobacco_product.replace(r#""sale_form":"sealed_pack""#, r#""sale_form":"bulk""#)
            )
            .is_err(),
            "a product carrying an unlawful pair must not deserialise"
        );
    }

    #[test]
    fn every_unit_of_measure_declares_whether_it_divides() {
        // Counted units cannot be sold in fractions: 0.001 of a can is a
        // mispriced sale, not a smaller one. Measures can. Every variant is
        // asserted individually rather than by a rule restated here, because a
        // sweep that recomputes the implementation proves nothing.
        assert!(!UnitOfMeasure::Each.is_divisible());
        assert!(!UnitOfMeasure::Package.is_divisible());
        assert!(UnitOfMeasure::Kilogram.is_divisible());
        assert!(UnitOfMeasure::Gram.is_divisible());
        assert!(UnitOfMeasure::Litre.is_divisible());
        assert!(UnitOfMeasure::Millilitre.is_divisible());
        assert!(UnitOfMeasure::Metre.is_divisible());

        // Exactly two of the seven are counted, so a new unit that silently
        // joined the counted arm — or an existing one that left it — is visible
        // here as well as in its own assertion above.
        let counted = ALL_UNITS.iter().filter(|u| !u.is_divisible()).count();
        assert_eq!(counted, 2);
    }

    #[test]
    fn the_unit_table_lists_every_variant_exactly_once() {
        // The table drives the sweep above, so a duplicate or a missing entry
        // would weaken it invisibly. `UnitOfMeasure` has no way to enumerate
        // itself, and a `match` on a fixed array cannot notice an eighth
        // variant, so this pins the count the table claims.
        assert_eq!(ALL_UNITS.len(), 7);
        for (index, unit) in ALL_UNITS.iter().enumerate() {
            assert!(
                !ALL_UNITS.iter().take(index).any(|earlier| earlier == unit),
                "{unit:?} is listed twice"
            );
        }
        assert_eq!(ALL_SALE_FORMS.len(), 3);
        assert_eq!(ALL_REGULATED_SALE_FORMS.len(), 4);
    }

    #[test]
    fn each_stored_token_is_spelled_once() {
        // `as_str` and the serde derive must agree, or an error message and a
        // stored column would quote different spellings of the same fact.
        for kind in ALL_REGULATED_KINDS {
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::Value::String(kind.as_str().to_owned())
            );
        }
        for form in ALL_SALE_FORMS {
            assert_eq!(
                serde_json::to_value(form).unwrap(),
                serde_json::Value::String(form.as_str().to_owned())
            );
        }

        // And the tokens are `0003`'s CHECK lists, restated here so that a
        // rename in either enum fails rather than desynchronising the schema
        // from the domain. `UnitOfMeasure` is deliberately absent: the `unit`
        // column's five values are coarser than its seven, and pretending
        // otherwise here would hide that.
        assert_eq!(RegulatedKind::Tobacco.as_str(), "tobacco");
        assert_eq!(
            ALL_SALE_FORMS.map(SaleForm::as_str),
            ["sealed_pack", "bulk", "service"]
        );
        assert_eq!(
            [
                BarcodeKind::Ean13,
                BarcodeKind::Ean8,
                BarcodeKind::Upca,
                BarcodeKind::Code128,
                BarcodeKind::Internal,
                BarcodeKind::PriceEmbedded,
                BarcodeKind::WeightEmbedded,
            ]
            .map(|kind| serde_json::to_string(&kind).unwrap()),
            [
                r#""ean13""#,
                r#""ean8""#,
                r#""upca""#,
                r#""code128""#,
                r#""internal""#,
                r#""price_embedded""#,
                r#""weight_embedded""#,
            ]
        );
    }

    #[test]
    fn golden_catalog_json_is_stable() {
        assert_eq!(serde_json::to_string(&cola()).unwrap(), GOLDEN_PRODUCT_JSON);
        assert_eq!(
            serde_json::from_str::<Product>(GOLDEN_PRODUCT_JSON).unwrap(),
            cola()
        );

        let single = barcode("5449000000996", Qty::ONE);
        assert_eq!(serde_json::to_string(&single).unwrap(), GOLDEN_BARCODE_JSON);
        assert_eq!(
            serde_json::from_str::<Barcode>(GOLDEN_BARCODE_JSON).unwrap(),
            single
        );

        // A quantity is milli-units on the wire as well as in memory, so
        // `pack_qty: 1000` is one unit and a reader who takes it for one
        // thousand units has been told the truth by the schema's column name.
        assert_eq!(
            serde_json::to_value(barcode("5449000000019", Qty::from_units(6).unwrap()).pack_qty)
                .unwrap(),
            serde_json::Value::from(6_000)
        );
    }

    #[test]
    fn a_price_ceiling_in_another_currency_cannot_be_compared_silently() {
        // Why `max_price` needs no currency check at construction: the ceiling
        // is `Money`, and `Money` refuses to order two currencies. A USD ceiling
        // on a JOD product is a typed error the moment somebody compares them,
        // rather than a ceiling that silently passes every price.
        let capped = Product {
            unit_price: Money::from_minor(450, Currency::JOD),
            max_price: Some(Money::from_minor(500, Currency::JOD)),
            ..cola()
        };
        assert_eq!(
            capped
                .unit_price
                .checked_cmp(capped.max_price.unwrap())
                .unwrap(),
            core::cmp::Ordering::Less
        );

        let mismatched = Product {
            max_price: Some(Money::from_minor(500, Currency::USD)),
            ..cola()
        };
        assert!(
            mismatched
                .unit_price
                .checked_cmp(mismatched.max_price.unwrap())
                .is_err()
        );
    }
}
