//! What a scanned code means, and the one place a label's price becomes a
//! price (microstep 1.2.4, `ref/domain-api.md` §4.1–§4.3).
//!
//! A deli scale prints an EAN-13 whose digits carry a weight or a price.
//! Reading those digits wrong charges the wrong amount, so the reading is pure,
//! table-driven, and refuses rather than guesses (E.40).
//!
//! Three rules shape this file.
//!
//! **No type here can be built from a number the caller chose.** A line's price
//! comes from a [`PriceSource`], whose amount is private. Of its constructors,
//! one takes a catalogue [`Product`], one is private to [`resolve_scan`] and
//! derives the amount from the scanned label itself, and the third — a
//! department sale — is the one the cart refuses on its ordinary path. Neither
//! `PriceSource` nor [`ScanLookup`] implements `Deserialize`: a JSON document is
//! a number the caller chose, and deserializing one into a price-bearing value
//! would reopen exactly the path the private fields close. `ApprovalHandle` is
//! closed against serde for the same reason.
//!
//! **Parsing and resolving are two steps, and only the second sees a product.**
//! [`parse_scan`] returns an opaque [`ParsedScan`] whose one public reading is
//! the item code to look up. The shell looks it up and hands every candidate
//! back to [`resolve_scan`], which refuses a candidate that does not match that
//! item code. So a shell bug cannot attach a label's price to a product the
//! label did not name.
//!
//! **A damaged label and an unusable rule are different failures.** A checksum,
//! a length or a stray character is the label's fault and the cashier rescans.
//! A rule whose spans fall outside the code is the store's configuration, and
//! scale evidence is `trade_scale`'s. The operator has to see which one stopped
//! the scan, so each is its own error.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{BarcodeKind, Product};
use crate::ids::{ProductId, TaxCategoryId};
use crate::money::{Currency, Money, Qty, RoundingRule};

/// The length of an EAN-13, the only layout a scale label is read in.
pub const EAN13_LEN: usize = 13;

/// A store-configured layout for scale labels: which prefix marks one, where
/// its item code and its value sit, and what the value counts.
///
/// Spans are `(start, len)`, zero-based, in the order `0007`'s
/// `item_code_start`/`item_code_len` and `value_start`/`value_len` columns
/// store them, so the repository copies them rather than converting them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddedBarcodeRule {
    /// The leading digits that mark a label this rule reads. An empty prefix
    /// matches nothing: it would otherwise turn every product barcode into a
    /// scale label.
    pub prefix: String,
    pub item_code_span: (usize, usize),
    pub value_span: (usize, usize),
    pub value_kind: EmbeddedValue,
    /// The divisor applied to the value's digits. The division must be exact:
    /// a remainder would be a fraction of a fil or of a gram, silently dropped.
    pub value_scale: u32,
    /// Whether the EAN-13 check digit is verified. `0007` defaults it on, and a
    /// rule with it off can only guess at a damaged label (E.40).
    pub verify_checksum: bool,
}

/// What an embedded value counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddedValue {
    /// A weight, in milli-units of the product's unit — grams of a kilogram.
    WeightMilli,
    /// A price, in the currency's minor units.
    PriceMinor,
}

impl EmbeddedValue {
    const fn barcode_kind(self) -> BarcodeKind {
        match self {
            Self::WeightMilli => BarcodeKind::WeightEmbedded,
            Self::PriceMinor => BarcodeKind::PriceEmbedded,
        }
    }
}

/// Opaque evidence from [`parse_scan`]. Dependent crates can read the item code
/// to look up and nothing else: the parsed value, the matched rule and the raw
/// code are private, so nothing outside this crate can build or alter one.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedScan {
    item_code: String,
    raw_code: String,
    kind: ParsedScanKind,
}

#[derive(Debug, Clone, PartialEq)]
enum ParsedScanKind {
    Plain,
    Weight {
        qty_milli: i64,
        rule: EmbeddedBarcodeRule,
    },
    Price {
        price_minor: i64,
        rule: EmbeddedBarcodeRule,
    },
}

impl ParsedScan {
    /// The code to look up: the whole scan for an ordinary barcode, the item
    /// code a rule extracted for a scale label.
    pub fn item_code(&self) -> &str {
        &self.item_code
    }
}

/// Why a scan could not be read.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ScanError {
    /// E.40: the check digit disagrees. Reject, never guess.
    #[error("checksum failed")]
    ChecksumFailed,
    #[error("length {0} invalid for {1:?}")]
    BadLength(usize, BarcodeKind),
    /// A value of zero, or one the rule's divisor does not divide exactly.
    #[error("embedded value out of range")]
    ValueOutOfRange,
    #[error("non-numeric in numeric span")]
    NonNumeric,
    /// The scanner sent bytes that are not text. Named, so a misconfigured
    /// keyboard layout is diagnosable rather than a silent miss.
    #[error("the scan is not UTF-8 text")]
    NotUtf8,
    #[error("the scan is empty")]
    Empty,
    /// The store's rule does not fit the code — a span outside it, or empty.
    /// Configuration, not a damaged label.
    #[error("the embedded-barcode rule for prefix {0:?} does not fit an EAN-13")]
    RuleMalformed(String),
    /// A candidate the shell found does not match the scanned item code.
    #[error("a candidate product does not match the scanned code")]
    CandidateForAnotherCode,
    /// A label price was asked of a scan that carries no price.
    #[error("this scan carries no label price")]
    NotAPriceLabel,
}

/// Read raw scanner bytes.
///
/// # Errors
///
/// [`ScanError::NotUtf8`] for bytes that are not text, then whatever
/// [`parse_scan`] refuses.
pub fn parse_scan_bytes(
    input: &[u8],
    rules: &[EmbeddedBarcodeRule],
) -> Result<ParsedScan, ScanError> {
    let raw = core::str::from_utf8(input).map_err(|_| ScanError::NotUtf8)?;
    parse_scan(raw, rules)
}

/// Read a scanned code against the store's scale-label rules.
///
/// A code no rule claims is an ordinary barcode, returned whole for lookup. A
/// code a rule claims must be an EAN-13 of digits whose check digit agrees
/// (when the rule verifies it); its item code and value are then read from the
/// rule's spans. When two rules' prefixes both match, the longer prefix wins:
/// it is the more specific layout.
///
/// # Errors
///
/// [`ScanError`] for an empty scan, and for a claimed code that is the wrong
/// length, not all digits, fails its checksum, carries a zero or inexact value,
/// or does not fit its rule.
pub fn parse_scan(raw_code: &str, rules: &[EmbeddedBarcodeRule]) -> Result<ParsedScan, ScanError> {
    if raw_code.is_empty() {
        return Err(ScanError::Empty);
    }
    let Some(rule) = claiming_rule(raw_code, rules).and_then(|index| rules.get(index)) else {
        return Ok(ParsedScan {
            item_code: raw_code.to_owned(),
            raw_code: raw_code.to_owned(),
            kind: ParsedScanKind::Plain,
        });
    };

    let length = raw_code.chars().count();
    if length != EAN13_LEN {
        return Err(ScanError::BadLength(length, rule.value_kind.barcode_kind()));
    }
    if !raw_code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ScanError::NonNumeric);
    }
    if rule.verify_checksum && !ean13_checksum_ok(raw_code) {
        return Err(ScanError::ChecksumFailed);
    }
    let item_code = span(raw_code, rule.item_code_span, rule)?;
    let digits = span(raw_code, rule.value_span, rule)?;
    let raw_value: i64 = digits.parse().map_err(|_| ScanError::NonNumeric)?;
    let scale = i64::from(rule.value_scale);
    if scale == 0 || raw_value % scale != 0 {
        return Err(ScanError::ValueOutOfRange);
    }
    let value = raw_value / scale;
    if value <= 0 {
        return Err(ScanError::ValueOutOfRange);
    }
    let kind = match rule.value_kind {
        EmbeddedValue::WeightMilli => ParsedScanKind::Weight {
            qty_milli: value,
            rule: rule.clone(),
        },
        EmbeddedValue::PriceMinor => ParsedScanKind::Price {
            price_minor: value,
            rule: rule.clone(),
        },
    };
    Ok(ParsedScan {
        item_code: item_code.to_owned(),
        raw_code: raw_code.to_owned(),
        kind,
    })
}

/// Which of `rules` claims `raw_code` as a scale label — its index — or `None`
/// for an ordinary barcode.
///
/// The choice [`parse_scan`] makes, and the only implementation of it: among
/// the rules whose non-empty prefix the code starts with, the longest prefix
/// wins. It is public because the shell must know *which* store layout read a
/// label — and so which scale printed it — before a label may price a line,
/// and a second copy of this choice in the shell is how the scale that is
/// checked and the layout that is read come to differ.
pub fn claiming_rule(raw_code: &str, rules: &[EmbeddedBarcodeRule]) -> Option<usize> {
    rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| !rule.prefix.is_empty() && raw_code.starts_with(rule.prefix.as_str()))
        .max_by_key(|(_, rule)| rule.prefix.len())
        .map(|(index, _)| index)
}

/// The digits a rule's `(start, len)` names, or a refusal of the rule.
fn span<'a>(
    code: &'a str,
    (start, len): (usize, usize),
    rule: &EmbeddedBarcodeRule,
) -> Result<&'a str, ScanError> {
    let malformed = || ScanError::RuleMalformed(rule.prefix.clone());
    let end = start.checked_add(len).ok_or_else(malformed)?;
    if len == 0 || end > code.len() {
        return Err(malformed());
    }
    code.get(start..end).ok_or_else(malformed)
}

/// Whether `code` is thirteen digits whose last is the EAN-13 check digit of
/// the first twelve: weights 1 and 3 alternating from the left, and the check
/// digit brings the weighted sum to a multiple of ten.
pub fn ean13_checksum_ok(code: &str) -> bool {
    let digits: Vec<u32> = code.chars().filter_map(|c| c.to_digit(10)).collect();
    let [body @ .., check] = digits.as_slice() else {
        return false;
    };
    if digits.len() != EAN13_LEN || code.len() != EAN13_LEN {
        return false;
    }
    let sum: u32 = body
        .iter()
        .zip([1, 3].iter().cycle())
        .map(|(digit, weight)| digit * weight)
        .sum();
    (10 - sum % 10) % 10 == *check
}

/// A price and where it came from.
///
/// `amount` is private, so the only way to get one is a constructor, and of the
/// three, two take no amount at all. No `Deserialize`: see the module
/// documentation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PriceSource {
    amount: Money,
    origin: PriceOrigin,
}

/// Where a line's price came from — the evidence a sale line keeps (I-5).
///
/// It carries no amount, so deserializing one prices nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceOrigin {
    Catalog {
        product_id: ProductId,
    },
    Label {
        rule_prefix: String,
        raw_code: String,
    },
    /// §6.5, and only §6.5: the cart's ordinary path refuses it.
    Department {
        tax_category_id: TaxCategoryId,
    },
}

impl PriceSource {
    /// The catalogue's price. Takes no amount: that is the entire point.
    pub fn from_catalog(product: &Product) -> PriceSource {
        PriceSource {
            amount: product.unit_price,
            origin: PriceOrigin::Catalog {
                product_id: product.id,
            },
        }
    }

    /// The price a scale printed on the label, read from the parse itself.
    /// Private: only [`resolve_scan`] calls it, so the scan path cannot become a
    /// general price-entry path.
    fn from_label(scan: &ParsedScan, currency: Currency) -> Result<PriceSource, ScanError> {
        match &scan.kind {
            ParsedScanKind::Price { price_minor, rule } => Ok(PriceSource {
                amount: Money::from_minor(*price_minor, currency),
                origin: PriceOrigin::Label {
                    rule_prefix: rule.prefix.clone(),
                    raw_code: scan.raw_code.clone(),
                },
            }),
            ParsedScanKind::Plain | ParsedScanKind::Weight { .. } => Err(ScanError::NotAPriceLabel),
        }
    }

    /// The one constructor that takes an amount. The cart's ordinary
    /// `add_line` refuses its origin; only the capability-gated, capped and
    /// audited department sale accepts it (§6.5).
    pub fn from_department(amount: Money, tax_category_id: TaxCategoryId) -> PriceSource {
        PriceSource {
            amount,
            origin: PriceOrigin::Department { tax_category_id },
        }
    }

    pub fn amount(&self) -> Money {
        self.amount
    }

    pub fn origin(&self) -> &PriceOrigin {
        &self.origin
    }
}

/// One product a lookup found, and how it was found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductHit {
    pub product: Product,
    pub matched_on: MatchedOn,
}

/// How a lookup matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchedOn {
    /// A barcode, with the code's own pack quantity: a six-pack's outer code
    /// means six units of the product (E.78). It travels with the match because
    /// it belongs to the code, not to the product.
    Barcode {
        code: String,
        kind: BarcodeKind,
        pack_qty: Qty,
    },
    Plu(String),
    Name,
    Tile,
}

impl ProductHit {
    /// Whether this hit was found by the code a scan was read as. A name or a
    /// tile is never a scan's match.
    fn matches_code(&self, item_code: &str) -> bool {
        match &self.matched_on {
            MatchedOn::Barcode { code, .. } | MatchedOn::Plu(code) => code == item_code,
            MatchedOn::Name | MatchedOn::Tile => false,
        }
    }
}

/// The weight a label's price implies at today's catalogue price per unit.
///
/// **Advisory, and never money.** A label printed at 09:00 for 0.416 JOD says
/// nothing about the price per kilogram at 21:00 (E.80). It feeds the stock
/// ledger as an estimated quantity, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedWeight {
    pub qty: Qty,
    pub catalog_unit_price: Money,
}

/// What a scan resolved to. Every variant is a decision the till renders
/// differently, which is why this is an enum and not an `Option<Product>`.
///
/// No `Deserialize`: see the module documentation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanLookup {
    /// One product. `pack_qty` is the barcode's multiplier, not the product's.
    Product { hit: ProductHit, pack_qty: Qty },
    /// The label carries a weight: that weight is the quantity, at the
    /// catalogue's price.
    WeightEmbedded { hit: ProductHit, qty: Qty },
    /// The label carries a price, and the label is what is charged. Matchable
    /// anywhere, constructible only by [`resolve_scan`].
    #[non_exhaustive]
    PriceEmbedded {
        hit: ProductHit,
        price: PriceSource,
        derived: Option<DerivedWeight>,
    },
    /// More than one product claims the code (E.36). The till says that it
    /// is choosing rather than choosing silently.
    Ambiguous { hits: Vec<ProductHit> },
    /// E.40, for a shell that turns a [`ScanError::ChecksumFailed`] into what
    /// the till shows.
    ChecksumFailed { code: String },
    /// E.39: nothing claims the code, and the queue must not stall.
    Unknown { code: String },
}

/// Turn a parse and the products the shell found for its item code into what
/// the till does next.
///
/// # Errors
///
/// [`ScanError::CandidateForAnotherCode`] when any candidate was not found by
/// the scan's own item code — the shell's lookup and this scan disagree, and a
/// label's price must not land on a product the label did not name.
pub fn resolve_scan(
    parsed: ParsedScan,
    candidates: Vec<ProductHit>,
    currency: Currency,
) -> Result<ScanLookup, ScanError> {
    if candidates
        .iter()
        .any(|hit| !hit.matches_code(parsed.item_code()))
    {
        return Err(ScanError::CandidateForAnotherCode);
    }
    let mut candidates = candidates;
    let hit = match candidates.len() {
        0 => {
            return Ok(ScanLookup::Unknown {
                code: parsed.raw_code,
            });
        }
        1 => candidates.remove(0),
        _ => return Ok(ScanLookup::Ambiguous { hits: candidates }),
    };
    match &parsed.kind {
        ParsedScanKind::Plain => {
            let pack_qty = match &hit.matched_on {
                MatchedOn::Barcode { pack_qty, .. } => *pack_qty,
                MatchedOn::Plu(_) | MatchedOn::Name | MatchedOn::Tile => Qty::ONE,
            };
            Ok(ScanLookup::Product { hit, pack_qty })
        }
        ParsedScanKind::Weight { qty_milli, .. } => Ok(ScanLookup::WeightEmbedded {
            hit,
            qty: Qty::from_milli(*qty_milli),
        }),
        ParsedScanKind::Price { .. } => {
            let price = PriceSource::from_label(&parsed, currency)?;
            let derived = derived_weight(&hit.product, price.amount());
            Ok(ScanLookup::PriceEmbedded {
                hit,
                price,
                derived,
            })
        }
    }
}

/// The quantity `label` buys at the product's catalogue price, rounded once,
/// half away from zero — or `None` for a product that is not weighed, priced
/// in another currency, or priced at nothing.
fn derived_weight(product: &Product, label: Money) -> Option<DerivedWeight> {
    let unit_price = product.unit_price;
    if !product.is_weighed || unit_price.currency() != label.currency() || unit_price.minor() <= 0 {
        return None;
    }
    let milli = Decimal::from(label.minor())
        .checked_mul(Decimal::from(Qty::ONE.milli()))?
        .checked_div(Decimal::from(unit_price.minor()))?;
    let qty_milli = RoundingRule::HalfAwayFromZero.round_to_i64(milli).ok()?;
    Some(DerivedWeight {
        qty: Qty::from_milli(qty_milli),
        catalog_unit_price: unit_price,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::catalog::{RegulatedSaleForm, SaleForm, UnitOfMeasure};
    use crate::ids::{ProductId, TaxCategoryId};

    const JOD: Currency = Currency::JOD;
    const ITEM: &str = "12345";

    /// The price layout the test name spells: `2`, five digits of item code,
    /// the GS1 price check digit (which this rule does not read; the EAN-13
    /// check digit already catches any one-digit corruption), five digits of
    /// price in fils, and the EAN-13 check digit.
    fn price_rule() -> EmbeddedBarcodeRule {
        EmbeddedBarcodeRule {
            prefix: "2".to_owned(),
            item_code_span: (1, 5),
            value_span: (7, 5),
            value_kind: EmbeddedValue::PriceMinor,
            value_scale: 1,
            verify_checksum: true,
        }
    }

    /// The weight layout: `2`, five digits of item code, five digits of grams,
    /// a digit this rule does not read, and the check digit.
    fn weight_rule() -> EmbeddedBarcodeRule {
        EmbeddedBarcodeRule {
            prefix: "2".to_owned(),
            item_code_span: (1, 5),
            value_span: (6, 5),
            value_kind: EmbeddedValue::WeightMilli,
            value_scale: 1,
            verify_checksum: true,
        }
    }

    /// GS1's own description of the check digit, written independently of
    /// [`ean13_checksum_ok`]: from the rightmost data digit leftwards, weights
    /// 3, 1, 3, 1…, and the digit that brings the sum to a multiple of ten.
    fn reference_check_digit(first_twelve: &str) -> u32 {
        let sum: u32 = first_twelve
            .chars()
            .rev()
            .enumerate()
            .map(|(i, c)| c.to_digit(10).unwrap() * if i % 2 == 0 { 3 } else { 1 })
            .sum();
        (10 - sum % 10) % 10
    }

    fn with_check(first_twelve: &str) -> String {
        assert_eq!(first_twelve.len(), 12);
        format!("{first_twelve}{}", reference_check_digit(first_twelve))
    }

    fn price_label(item: &str, price_minor: i64) -> String {
        with_check(&format!("2{item}7{price_minor:05}"))
    }

    fn weight_label(item: &str, grams: i64) -> String {
        with_check(&format!("2{item}{grams:05}0"))
    }

    /// White cheese at `price_minor` fils a kilogram.
    fn cheese(price_minor: i64) -> Product {
        Product {
            id: ProductId::from_uuid(Uuid::from_u128(0xC4)),
            sku: "CHEESE-KG".to_owned(),
            name_ar: "جبنة بيضاء".to_owned(),
            name_en: Some("White cheese".to_owned()),
            category_id: None,
            tax_category_id: TaxCategoryId::from_uuid(Uuid::from_u128(0x7A)),
            unit: UnitOfMeasure::Kilogram,
            unit_price: Money::from_minor(price_minor, JOD),
            is_weighed: true,
            is_service: false,
            is_active: true,
            min_age: None,
            max_price: None,
            reorder_point: None,
            regulated_sale_form: RegulatedSaleForm::Unregulated(SaleForm::Bulk),
        }
    }

    fn by_plu(product: Product, code: &str) -> ProductHit {
        ProductHit {
            product,
            matched_on: MatchedOn::Plu(code.to_owned()),
        }
    }

    fn priced(lookup: &ScanLookup) -> (&PriceSource, Option<DerivedWeight>) {
        match lookup {
            ScanLookup::PriceEmbedded { price, derived, .. } => (price, *derived),
            other => panic!("expected a price-embedded lookup, got {other:?}"),
        }
    }

    // ── the named tests ─────────────────────────────────────────────────

    #[test]
    fn weight_embedded_2xxxxxwwwww_parses() {
        let code = weight_label(ITEM, 1_250);
        let parsed = parse_scan(&code, &[weight_rule()]).unwrap();
        assert_eq!(parsed.item_code(), ITEM);
        assert_eq!(
            parsed.kind,
            ParsedScanKind::Weight {
                qty_milli: 1_250,
                rule: weight_rule()
            }
        );
        let lookup = resolve_scan(parsed, vec![by_plu(cheese(8_500), ITEM)], JOD).unwrap();
        assert_eq!(
            lookup,
            ScanLookup::WeightEmbedded {
                hit: by_plu(cheese(8_500), ITEM),
                qty: Qty::from_milli(1_250)
            }
        );
    }

    #[test]
    fn price_embedded_2xxxxxpmmmmmc_parses() {
        let code = price_label(ITEM, 416);
        assert_eq!(code.len(), EAN13_LEN);
        let parsed = parse_scan(&code, &[price_rule()]).unwrap();
        assert_eq!(parsed.item_code(), ITEM);
        assert_eq!(parsed.raw_code, code);
        assert_eq!(
            parsed.kind,
            ParsedScanKind::Price {
                price_minor: 416,
                rule: price_rule()
            }
        );
    }

    #[test]
    fn non_utf8_scan_is_a_named_error() {
        assert_eq!(
            parse_scan_bytes(&[0xFF, 0xFE, 0x32], &[price_rule()]),
            Err(ScanError::NotUtf8)
        );
        let code = price_label(ITEM, 416);
        assert_eq!(
            parse_scan_bytes(code.as_bytes(), &[price_rule()]).unwrap(),
            parse_scan(&code, &[price_rule()]).unwrap()
        );
    }

    /// The item code a rule extracted is what gets looked up, and a candidate
    /// the shell found by anything else is refused: a label's price lands
    /// only on the product the label names.
    #[test]
    fn price_embedded_resolves_item_code_before_constructing_lookup() {
        let code = price_label(ITEM, 416);
        let parsed = parse_scan(&code, &[price_rule()]).unwrap();
        assert_eq!(
            parsed.item_code(),
            ITEM,
            "the item code, not the whole label"
        );

        let strangers = [
            by_plu(cheese(8_500), "99999"),
            ProductHit {
                product: cheese(8_500),
                matched_on: MatchedOn::Barcode {
                    code: code.clone(),
                    kind: BarcodeKind::Ean13,
                    pack_qty: Qty::ONE,
                },
            },
            ProductHit {
                product: cheese(8_500),
                matched_on: MatchedOn::Name,
            },
        ];
        for stranger in strangers {
            assert_eq!(
                resolve_scan(parsed.clone(), vec![stranger.clone()], JOD),
                Err(ScanError::CandidateForAnotherCode),
                "{stranger:?}"
            );
        }
        let lookup = resolve_scan(parsed, vec![by_plu(cheese(8_500), ITEM)], JOD).unwrap();
        assert!(matches!(lookup, ScanLookup::PriceEmbedded { .. }));
    }

    /// E.80's charging half: what a price label resolves to is the label's own
    /// figure, to the fil, with the label as its evidence — whatever the
    /// catalogue price per kilogram is.
    #[test]
    fn price_embedded_line_total_equals_the_label() {
        let code = price_label(ITEM, 416);
        let parsed = parse_scan(&code, &[price_rule()]).unwrap();
        let lookup = resolve_scan(parsed, vec![by_plu(cheese(8_500), ITEM)], JOD).unwrap();
        let (price, _) = priced(&lookup);
        assert_eq!(price.amount(), Money::from_minor(416, JOD));
        assert_eq!(
            price.origin(),
            &PriceOrigin::Label {
                rule_prefix: "2".to_owned(),
                raw_code: code
            }
        );
    }

    /// The label was printed at 09:00 at 8.500 JOD a kilogram; at 21:00 the
    /// kilogram costs 9.900. The customer is charged the sticker either way;
    /// only the advisory weight the stock ledger records moves.
    #[test]
    fn price_embedded_after_a_price_per_kilo_change_still_charges_the_label() {
        let code = price_label(ITEM, 416);
        let resolve = |price_per_kg: i64| {
            let parsed = parse_scan(&code, &[price_rule()]).unwrap();
            resolve_scan(parsed, vec![by_plu(cheese(price_per_kg), ITEM)], JOD).unwrap()
        };
        let (morning, evening) = (resolve(8_500), resolve(9_900));
        let ((morning_price, morning_weight), (evening_price, evening_weight)) =
            (priced(&morning), priced(&evening));
        assert_eq!(morning_price, evening_price);
        assert_eq!(morning_price.amount(), Money::from_minor(416, JOD));
        // 416 / 8500 kg = 48.94 g → 49; 416 / 9900 kg = 42.02 g → 42.
        assert_eq!(
            morning_weight,
            Some(DerivedWeight {
                qty: Qty::from_milli(49),
                catalog_unit_price: Money::from_minor(8_500, JOD)
            })
        );
        assert_eq!(
            evening_weight,
            Some(DerivedWeight {
                qty: Qty::from_milli(42),
                catalog_unit_price: Money::from_minor(9_900, JOD)
            })
        );
    }

    #[test]
    fn from_label_refuses_a_weight_rule() {
        let weighed = parse_scan(&weight_label(ITEM, 1_250), &[weight_rule()]).unwrap();
        assert_eq!(
            PriceSource::from_label(&weighed, JOD),
            Err(ScanError::NotAPriceLabel)
        );
        let plain = parse_scan("6291041500213", &[price_rule()]).unwrap();
        assert_eq!(
            PriceSource::from_label(&plain, JOD),
            Err(ScanError::NotAPriceLabel)
        );
    }

    // ── what else a scan may be ─────────────────────────────────────────

    /// A code no rule claims is an ordinary barcode, looked up whole, and it
    /// brings its code's pack quantity: the outer case means six (E.78).
    #[test]
    fn a_plain_barcode_brings_its_codes_pack_quantity() {
        let parsed = parse_scan("6291041500213", &[price_rule()]).unwrap();
        assert_eq!(parsed.item_code(), "6291041500213");
        let six_pack = ProductHit {
            product: cheese(8_500),
            matched_on: MatchedOn::Barcode {
                code: "6291041500213".to_owned(),
                kind: BarcodeKind::Ean13,
                pack_qty: Qty::from_units(6).unwrap(),
            },
        };
        assert_eq!(
            resolve_scan(parsed, vec![six_pack.clone()], JOD).unwrap(),
            ScanLookup::Product {
                hit: six_pack,
                pack_qty: Qty::from_units(6).unwrap()
            }
        );
    }

    #[test]
    fn no_candidate_is_unknown_and_two_are_ambiguous() {
        let code = price_label(ITEM, 416);
        let parsed = parse_scan(&code, &[price_rule()]).unwrap();
        assert_eq!(
            resolve_scan(parsed.clone(), Vec::new(), JOD).unwrap(),
            ScanLookup::Unknown { code }
        );
        let two = vec![by_plu(cheese(8_500), ITEM), by_plu(cheese(9_000), ITEM)];
        assert_eq!(
            resolve_scan(parsed, two.clone(), JOD).unwrap(),
            ScanLookup::Ambiguous { hits: two }
        );
    }

    /// A damaged label and a broken rule are different failures, and each
    /// says which it is.
    #[test]
    fn a_damaged_label_and_a_broken_rule_are_told_apart() {
        assert_eq!(parse_scan("", &[price_rule()]), Err(ScanError::Empty));
        assert_eq!(
            parse_scan("212345700416", &[price_rule()]),
            Err(ScanError::BadLength(12, BarcodeKind::PriceEmbedded))
        );
        assert_eq!(
            parse_scan("2123457004a63", &[price_rule()]),
            Err(ScanError::NonNumeric)
        );
        let mut beyond = price_rule();
        beyond.value_span = (9, 5);
        assert_eq!(
            parse_scan(&price_label(ITEM, 416), &[beyond]),
            Err(ScanError::RuleMalformed("2".to_owned()))
        );
        let mut empty = price_rule();
        empty.item_code_span = (1, 0);
        assert_eq!(
            parse_scan(&price_label(ITEM, 416), &[empty]),
            Err(ScanError::RuleMalformed("2".to_owned()))
        );
    }

    /// A zero, and a value the rule's divisor does not divide exactly, are
    /// refused rather than truncated.
    #[test]
    fn a_zero_or_inexact_value_is_refused() {
        assert_eq!(
            parse_scan(&price_label(ITEM, 0), &[price_rule()]),
            Err(ScanError::ValueOutOfRange)
        );
        let mut tenths = price_rule();
        tenths.value_scale = 10;
        assert_eq!(
            parse_scan(&price_label(ITEM, 415), &[tenths.clone()]),
            Err(ScanError::ValueOutOfRange)
        );
        assert_eq!(
            parse_scan(&price_label(ITEM, 410), &[tenths.clone()])
                .unwrap()
                .kind,
            ParsedScanKind::Price {
                price_minor: 41,
                rule: tenths
            }
        );
        let mut nothing = price_rule();
        nothing.value_scale = 0;
        assert_eq!(
            parse_scan(&price_label(ITEM, 410), &[nothing]),
            Err(ScanError::ValueOutOfRange)
        );
    }

    /// When two prefixes claim a code the longer, more specific one reads it,
    /// and an empty prefix claims nothing.
    #[test]
    fn the_longer_prefix_wins_and_an_empty_prefix_claims_nothing() {
        let mut deli = weight_rule();
        deli.prefix = "21".to_owned();
        deli.item_code_span = (2, 4);
        let code = with_check("212345012500");
        let parsed = parse_scan(&code, &[price_rule(), deli.clone()]).unwrap();
        assert_eq!(parsed.item_code(), "2345");
        assert!(matches!(parsed.kind, ParsedScanKind::Weight { .. }));

        let mut greedy = price_rule();
        greedy.prefix = String::new();
        let parsed = parse_scan("6291041500213", &[greedy]).unwrap();
        assert_eq!(parsed.kind, ParsedScanKind::Plain);
    }

    /// The weight a label implies is advisory: absent for a counted product,
    /// one priced at nothing, or one priced in another currency.
    #[test]
    fn a_derived_weight_exists_only_where_it_can_mean_something() {
        let label = Money::from_minor(416, JOD);
        let mut counted = cheese(8_500);
        counted.is_weighed = false;
        assert_eq!(derived_weight(&counted, label), None);
        assert_eq!(derived_weight(&cheese(0), label), None);
        let mut dollars = cheese(8_500);
        dollars.unit_price = Money::from_minor(8_500, Currency::from_code("USD").unwrap());
        assert_eq!(derived_weight(&dollars, label), None);
    }

    proptest! {
        // The crate's one shared configuration (conventions §5.1).
        #![proptest_config(domain_proptest_config())]

        /// The check digit this crate verifies is GS1's, for every twelve
        /// digits: the reference digit passes and each of the other nine fails.
        #[test]
        fn prop_ean13_checksum_matches_reference(first_twelve in "[0-9]{12}") {
            let check = reference_check_digit(&first_twelve);
            for digit in 0..10 {
                let code = format!("{first_twelve}{digit}");
                prop_assert_eq!(ean13_checksum_ok(&code), digit == check, "{}", code);
            }
        }

        /// Whatever item code and value a label is printed with, parsing it
        /// gives back exactly that item code and that value.
        #[test]
        fn prop_embedded_parse_roundtrip(
            item in "[0-9]{5}",
            value in 1_i64..=99_999,
            weighed in any::<bool>(),
        ) {
            let (code, rule) = if weighed {
                (weight_label(&item, value), weight_rule())
            } else {
                (price_label(&item, value), price_rule())
            };
            let parsed = parse_scan(&code, std::slice::from_ref(&rule)).unwrap();
            prop_assert_eq!(parsed.item_code(), item.as_str());
            prop_assert_eq!(&parsed.raw_code, &code);
            let expected = if weighed {
                ParsedScanKind::Weight { qty_milli: value, rule }
            } else {
                ParsedScanKind::Price { price_minor: value, rule }
            };
            prop_assert_eq!(parsed.kind, expected);
        }

        /// E.40: change any one digit of a price label and it never parses as
        /// a priced label. Every corruption past the prefix fails the check
        /// digit; a corrupted prefix stops being a scale label at all and is
        /// looked up as an ordinary code, which carries no price.
        #[test]
        fn prop_corrupt_digit_never_parses_clean(
            item in "[0-9]{5}",
            value in 1_i64..=99_999,
            position in 0_usize..EAN13_LEN,
            shift in 1_u32..10,
        ) {
            let code = price_label(&item, value);
            let mut digits: Vec<u32> = code.chars().map(|c| c.to_digit(10).unwrap()).collect();
            let digit = digits.get_mut(position).unwrap();
            *digit = (*digit + shift) % 10;
            let corrupted: String = digits.iter().map(|d| char::from_digit(*d, 10).unwrap()).collect();
            prop_assert_ne!(&corrupted, &code);
            match parse_scan(&corrupted, &[price_rule()]) {
                Err(ScanError::ChecksumFailed) => prop_assert!(position > 0),
                Ok(parsed) => {
                    prop_assert_eq!(position, 0);
                    prop_assert_eq!(parsed.kind, ParsedScanKind::Plain);
                }
                Err(other) => prop_assert!(false, "unexpected {:?}", other),
            }
        }
    }

    #[test]
    fn the_claiming_rule_is_the_rule_the_parse_reads_with() {
        // The longest matching prefix wins, an empty prefix claims nothing,
        // and a code no rule claims is an ordinary barcode.
        let short = price_rule();
        let long = EmbeddedBarcodeRule {
            prefix: "23".to_owned(),
            ..weight_rule()
        };
        let empty = EmbeddedBarcodeRule {
            prefix: String::new(),
            ..price_rule()
        };
        let rules = [short, long.clone(), empty];
        assert_eq!(claiming_rule("2345670004162", &rules), Some(1));
        assert_eq!(claiming_rule("2045670004162", &rules), Some(0));
        assert_eq!(claiming_rule("6291041500213", &rules), None);
        assert_eq!(claiming_rule("6291041500213", &[]), None);

        // And it is the rule the parse reads with: the "23" layout reads a
        // weight, the "2" layout would have read a price.
        let label = weight_label("34567", 1250);
        assert!(label.starts_with("23"));
        let parsed = parse_scan(&label, &rules).unwrap();
        assert!(matches!(parsed.kind, ParsedScanKind::Weight { ref rule, .. } if *rule == long));
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// A code a store layout claims is never read as an ordinary barcode,
        /// and a code none claims always is: whatever the shell decides about
        /// the claiming layout's scale, the parse agrees with it about which
        /// codes are labels.
        #[test]
        fn prop_a_claimed_code_is_never_read_as_plain(
            code in "[0-9]{1,14}",
            prefixes in proptest::collection::vec("[0-9]{0,3}", 0..4),
        ) {
            let rules: Vec<EmbeddedBarcodeRule> = prefixes
                .into_iter()
                .map(|prefix| EmbeddedBarcodeRule { prefix, ..price_rule() })
                .collect();
            let read_as_plain = matches!(
                parse_scan(&code, &rules),
                Ok(ParsedScan { kind: ParsedScanKind::Plain, .. })
            );
            match claiming_rule(&code, &rules) {
                None => prop_assert!(read_as_plain, "{} is claimed by no rule yet was not read as plain", code),
                Some(index) => {
                    let prefix_matches = rules
                        .get(index)
                        .is_some_and(|rule| !rule.prefix.is_empty() && code.starts_with(rule.prefix.as_str()));
                    prop_assert!(prefix_matches, "rule {} does not claim {}", index, code);
                    prop_assert!(!read_as_plain, "{} is claimed by rule {} yet was read as plain", code, index);
                }
            }
        }
    }
}
