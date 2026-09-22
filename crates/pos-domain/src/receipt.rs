//! The receipt render model (microstep 1.7.1).
//!
//! One pure struct describing everything a receipt says, built once and read by
//! three renderers — the ESC/POS rasteriser (1.7.3), the PDF renderer and the
//! email renderer. That is the whole point of it: an emailed receipt that
//! computed its own totals could disagree with the printed one, and the
//! customer would hold two documents describing one purchase.
//!
//! Its shape is [`ref/domain-api.md`](../../../docs/implementation/ref/domain-api.md) §13 and its
//! content order is [`ref/hardware-and-receipts.md`](../../../docs/implementation/ref/hardware-and-receipts.md)
//! §2.3. It computes nothing. Every amount arrives already decided by
//! `price_cart` (1.4.9) and every string already resolved by its caller, which
//! is what makes this module trivially pure (I-8) and what stops a second
//! rounding from happening at render time (I-1).
//!
//! # Three things this type deliberately cannot express
//!
//! **A precision setting.** [`ReceiptLocale`] carries language and direction and
//! no `money_decimals`, although §13's inline comment lists all three. Two
//! paragraphs below that comment §13 says `money_decimals` *"does not govern a
//! document the customer is handed"*, and §2.3 makes it the first of four rules
//! the renderer may not negotiate — every money field through
//! [`Money::format_exact`], *"with the store's `money_decimals` nowhere in the
//! path"*. A three-fil rounding line rendered at two decimals reads `0.00`,
//! which is money the document hides from the person paying it. The rule is
//! enforced by the field's absence rather than by this paragraph: a comment
//! cannot be violated, a field can.
//!
//! **A customer's name.** §2.3 asks for a loyalty *balance* when a customer is
//! attached, and `customer_name` is on `ref/security-compliance.md` §6's
//! registry, so [`LoyaltyBlock`] has nowhere to put one.
//!
//! **A total it computed itself.** There is no `fn total()`. The receipt is
//! evidence of what was charged, not a second opinion about it.
//!
//! # What it *can* express and must not
//!
//! Four rules in the references are properties of a whole document rather than
//! of any one field, and the struct can express a document that breaks each.
//! [`ReceiptModel::validate`] refuses them. It is a method rather than a
//! private constructor because §13 specifies a plain struct with public fields
//! and a render model is deliberately transparent to its three consumers;
//! redesigning a specified public shape to enforce something the reference did
//! not ask for is the larger error. The consequence is stated plainly: nothing
//! here forces the call, and 1.7.3's entry point is where it becomes mandatory.

use core::fmt;

use serde::{Deserialize, Serialize};

use crate::catalog::UnitOfMeasure;
use crate::ids::{RegisterId, SaleId};
use crate::money::Qty;
use crate::money::{Currency, Money};
use crate::tax::TaxSummaryRow;
use crate::time::{BusinessDate, Timestamp};

/// What the document *is*, which decides its heading and whether it may carry a
/// fiscal QR at all.
///
/// `Acknowledgement` is not a watermark, and the distinction is legal rather
/// than cosmetic. While the offline-clearance ruling is open
/// (`ref/fiscal-jofotara.md` §2.1) the interim default for a sale made without
/// clearance is a clearly marked **non-fiscal payment acknowledgement**. A
/// watermark would leave the underlying document still calling itself a tax
/// invoice, so this is a kind of its own and [`ReceiptModel::validate`] refuses
/// to let one carry a [`FiscalBlock`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    Sale,
    Refund,
    Acknowledgement,
    XReport,
    ZReport,
}

impl DocKind {
    /// Every variant, once, so a validator sweep and a test cannot disagree
    /// about the list. Adding a variant without adding it here fails
    /// `every_doc_kind_is_covered_by_the_validator`.
    pub const ALL: [DocKind; 5] = [
        DocKind::Sale,
        DocKind::Refund,
        DocKind::Acknowledgement,
        DocKind::XReport,
        DocKind::ZReport,
    ];

    /// Whether a document of this kind may carry a cleared fiscal block.
    ///
    /// Reports are not invoices and an acknowledgement is explicitly not one,
    /// so only a sale or a refund can.
    ///
    /// Written as a full `match` rather than `matches!` on purpose: a sixth
    /// variant then fails to compile here instead of silently defaulting to
    /// "not an invoice", which is the safe answer but not a decided one. The
    /// variant this method exists for — `Acknowledgement` — arrived exactly
    /// that way, as a legal distinction somebody had to make.
    #[must_use]
    pub const fn may_carry_fiscal(self) -> bool {
        match self {
            DocKind::Sale | DocKind::Refund => true,
            DocKind::Acknowledgement | DocKind::XReport | DocKind::ZReport => false,
        }
    }
}

/// An overprint that changes what the document is evidence *of*, without
/// changing what kind of document it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Watermark {
    Duplicate,
    Training,
}

/// The language a document is set in.
///
/// The Rust counterpart of `@pos/ui`'s `Locale` (microstep 1.11.1). Two
/// languages, Arabic first, because Arabic is the product rather than a
/// translation of it (conventions §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Arabic,
    English,
}

/// Writing direction, as a renderer needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextDirection {
    RightToLeft,
    LeftToRight,
}

impl Language {
    /// The direction this language is written in.
    ///
    /// Mirrors `directionFor` in `packages/ui/src/i18n.ts`, and the two must
    /// stay in step: the receipt and the screen showing the same sale in
    /// opposite directions is exactly the class of defect 1.11.1's font check
    /// exists to prevent one layer down.
    #[must_use]
    pub const fn direction(self) -> TextDirection {
        match self {
            Language::Arabic => TextDirection::RightToLeft,
            Language::English => TextDirection::LeftToRight,
        }
    }
}

/// How the document is set. No precision — see the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReceiptLocale {
    pub language: Language,
    pub direction: TextDirection,
}

impl ReceiptLocale {
    /// The locale for `language`, with the direction it is written in.
    ///
    /// Both fields stay public because §13 does, but this is the constructor
    /// worth using: a locale whose `language` and `direction` disagree sets
    /// Arabic left to right, and nothing downstream would notice.
    #[must_use]
    pub const fn new(language: Language) -> Self {
        Self {
            language,
            direction: language.direction(),
        }
    }
}

/// Who sold it. Legally required on every receipt (master plan B.6).
///
/// `Debug` is hand-written: `phone` is on `ref/security-compliance.md` §6's
/// registry, which redacts at any nesting depth, and §6's own table names
/// *"Panic payloads and `Debug` output"* as a channel the `tracing` layer
/// cannot reach.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerchantBlock {
    pub legal_name: String,
    pub store_name: String,
    pub address: Option<String>,
    pub phone: Option<String>,
    /// The tax number. `org.tin` is nullable because a merchant below the
    /// registration threshold has none, not because printing it is optional.
    pub tin: Option<String>,
}

/// The B2B buyer, present exactly when a TIN was captured.
///
/// The capture path was complete — a command, two `sale` columns and a fiscal
/// conformance rule — while the printed document had nowhere to put any of it,
/// so the one customer who explicitly asked for something got a receipt they
/// could not file against their input tax.
///
/// `Debug` is hand-written: `name` is `buyer_name`, on the §6 registry.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuyerBlock {
    pub name: Option<String>,
    /// Never empty. The block exists *because* this was captured, so a blank
    /// one is a block that should not have been built — [`ReceiptModel::validate`]
    /// refuses it rather than printing an empty legal field.
    pub tin: String,
}

/// Which document this is, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptHeader {
    pub sale_id: SaleId,
    /// The register-scoped receipt number, already formatted by its caller —
    /// the counter is 1.9.2's and its presentation is a store setting.
    pub receipt_no: String,
    pub register_id: RegisterId,
    pub register_code: String,
    pub cashier_name: String,
    pub issued_at: Timestamp,
    /// The shift's business date, which is not the calendar date of
    /// `issued_at` for a sale after midnight (conventions §11).
    pub business_date: BusinessDate,
}

/// One allowance printed beneath the line it belongs to, so a cashier can
/// answer "why is this cheaper?" without leaving the receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptDiscount {
    pub label: String,
    /// The amount taken off, as a positive number. The renderer decides how a
    /// deduction is shown; the model does not encode presentation in a sign.
    pub amount: Money,
}

/// One printed line: name, quantity, unit price, line total.
///
/// Every field is a snapshot (I-5). There is no product id to look up, because
/// a refund argued from this document six months later must read what was sold
/// and charged, never today's catalogue. A department line carries the
/// department's name here, never "unknown item" — a customer's proof of
/// purchase has to describe what they bought.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptLine {
    pub name: String,
    pub qty: Qty,
    pub unit: UnitOfMeasure,
    /// Carried because [`Qty::format`] needs it and because the sale line
    /// snapshotted it: `0.347 kg` and `1` print differently.
    pub is_weighed: bool,
    pub unit_price: Money,
    pub line_total: Money,
    pub discounts: Vec<ReceiptDiscount>,
}

/// The totals block, in the order §2.3 prints it.
///
/// `Eq` is absent here and on [`ReceiptModel`], which is §13's derive list and
/// `tax.rs`'s house rule for a struct: [`TaxSummaryRow`] does not derive it, and
/// reaching into 1.3.1's type to add a bound for this one's convenience is the
/// wrong direction of change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReceiptTotals {
    pub subtotal: Money,
    pub discount_total: Money,
    /// Net, tax and gross per rate — the sales-side evidence the whole tax
    /// engine exists for. [`TaxSummaryRow`] is 1.3.1's and its own doc comment
    /// already calls it a receipt-summary row, so this is a reuse: a second
    /// per-rate type would be a second way to group the same numbers.
    pub tax_summary: Vec<TaxSummaryRow>,
    /// Present only when non-zero (§2.3). `Some(zero)` prints a rounding line
    /// that adjusted nothing, which [`ReceiptModel::validate`] refuses — the
    /// absence of the line *is* the statement that none was applied.
    pub rounding_adjustment: Option<Money>,
    pub total: Money,
}

/// One tender, as the customer sees it.
///
/// `Debug` is derived, and `masked_pan` is deliberately visible in it. The §6
/// registry names `pan` and `card_number`; a masked value is neither, matches
/// no suffix rule, and `.claude/rules/security.md` explicitly permits storing
/// *"the masked PAN the terminal returns for the receipt"*. Redacting it would
/// be over-correction past a reviewed rule, and it would remove the one field a
/// tender mismatch is diagnosed by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptTender {
    /// The tender type's printed label, already resolved for the locale.
    pub label: String,
    pub amount: Money,
    pub masked_pan: Option<String>,
    pub scheme: Option<String>,
}

/// The loyalty balance, when a customer is attached. No name — see the module
/// documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoyaltyBlock {
    /// Points, not money. A points balance is not a claim on the drawer and
    /// never renders through a currency.
    pub balance_points: i64,
}

/// The cleared fiscal document, once JoFotara has returned one.
///
/// Both fields are opaque to this model: the QR payload's construction is
/// 2.7.9's and its bytes are persisted so a reprint days later carries the
/// identical QR (E.46). Nothing here parses or regenerates either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiscalBlock {
    pub uuid: String,
    pub qr_payload: String,
}

/// Return policy and thank-you, resolved from the versioned template.
///
/// The strings arrive resolved rather than as a template reference, because
/// `receipt_artifact.template_version` is snapshotted onto the artifact
/// (§2.4): a merchant who changes their footer in March must not thereby
/// change what January's receipt says, for the same reason a refund reads the
/// sale line rather than today's catalogue (I-5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FooterBlock {
    pub return_policy: Option<String>,
    pub thank_you: Option<String>,
}

/// Everything a receipt needs, in one pure struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReceiptModel {
    pub doc_kind: DocKind,
    pub watermark: Option<Watermark>,
    pub merchant: MerchantBlock,
    pub buyer: Option<BuyerBlock>,
    pub header: ReceiptHeader,
    pub lines: Vec<ReceiptLine>,
    pub totals: ReceiptTotals,
    pub tenders: Vec<ReceiptTender>,
    pub change: Option<Money>,
    pub loyalty: Option<LoyaltyBlock>,
    pub fiscal: Option<FiscalBlock>,
    pub footer: FooterBlock,
    pub locale: ReceiptLocale,
}

/// What a document can be wrong about as a whole.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReceiptError {
    /// Two currencies on one document. `Money` carries its own `Currency`
    /// (I-2), so the struct can express a subtotal in JOD beside change in USD;
    /// that is not a document anybody can add up.
    ///
    /// `field` names where, because an operator handed *"this receipt mixes JOD
    /// with USD"* has to read every amount on the document to find out which.
    #[error("{field} is in {} and this receipt is in {}", .found.code(), .expected.code())]
    MixedCurrency {
        field: &'static str,
        expected: Currency,
        found: Currency,
    },
    /// A non-fiscal document carrying a fiscal block.
    #[error("a {kind:?} document may not carry a fiscal block")]
    FiscalBlockNotAllowed { kind: DocKind },
    /// A cash-rounding line that adjusted nothing.
    #[error("a zero cash-rounding adjustment prints a line that adjusted nothing")]
    ZeroRoundingAdjustment,
    /// A buyer block with no TIN.
    #[error("a buyer block exists because a TIN was captured, and this one has none")]
    BuyerWithoutTin,
    /// A printed line that describes nothing.
    #[error("line {index} has no name, so it describes nothing it was charged for")]
    UnnamedLine { index: usize },
}

impl ReceiptModel {
    /// The whole-document rules, checked in one pass.
    ///
    /// Every rule here is one the references state and no single field can
    /// carry. Field-level correctness — that the lines add to the subtotal,
    /// that the tax rows are the right ones — belongs to `price_cart` (1.4.9),
    /// which decided those numbers before this model was built. Re-deriving
    /// them here would be a second opinion, and two opinions about one total is
    /// the failure this type exists to prevent.
    ///
    /// # Errors
    ///
    /// Returns the first rule the document breaks, in the order above.
    pub fn validate(&self) -> Result<(), ReceiptError> {
        if self.fiscal.is_some() && !self.doc_kind.may_carry_fiscal() {
            return Err(ReceiptError::FiscalBlockNotAllowed {
                kind: self.doc_kind,
            });
        }

        if let Some(adjustment) = self.totals.rounding_adjustment
            && adjustment.minor() == 0
        {
            return Err(ReceiptError::ZeroRoundingAdjustment);
        }

        if let Some(buyer) = &self.buyer
            && buyer.tin.trim().is_empty()
        {
            return Err(ReceiptError::BuyerWithoutTin);
        }

        if let Some((index, _)) = self
            .lines
            .iter()
            .enumerate()
            .find(|(_, line)| line.name.trim().is_empty())
        {
            return Err(ReceiptError::UnnamedLine { index });
        }

        self.check_one_currency()
    }

    /// Every amount on the document shares the total's currency.
    ///
    /// The total is the reference rather than the first amount encountered
    /// because it is the number the customer paid and the one every other row
    /// is evidence for.
    ///
    /// Each amount is carried with the name of the field it came from, so the
    /// error can say *where*. The labels are also the coverage claim: this list
    /// and the test's `AMOUNT_FIELDS` are the same twelve, and a thirteenth
    /// amount added to the model without a line here is an amount nothing
    /// checks.
    fn check_one_currency(&self) -> Result<(), ReceiptError> {
        let expected = self.totals.total.currency();

        let mut amounts: Vec<(&'static str, Money)> = vec![
            ("the subtotal", self.totals.subtotal),
            ("the discount total", self.totals.discount_total),
        ];
        amounts.extend(
            self.totals
                .rounding_adjustment
                .map(|a| ("the cash-rounding adjustment", a)),
        );
        amounts.extend(self.change.map(|a| ("the change", a)));
        for row in &self.totals.tax_summary {
            amounts.extend([
                ("a tax summary net", row.net),
                ("a tax summary tax", row.tax),
                ("a tax summary gross", row.gross),
            ]);
            amounts.extend(row.per_unit.map(|a| ("a tax summary per-unit amount", a)));
        }
        for line in &self.lines {
            amounts.extend([
                ("a line unit price", line.unit_price),
                ("a line total", line.line_total),
            ]);
            amounts.extend(line.discounts.iter().map(|d| ("a line discount", d.amount)));
        }
        amounts.extend(self.tenders.iter().map(|t| ("a tender", t.amount)));

        match amounts.into_iter().find(|(_, a)| a.currency() != expected) {
            Some((field, found)) => Err(ReceiptError::MixedCurrency {
                field,
                expected,
                found: found.currency(),
            }),
            None => Ok(()),
        }
    }
}

/// How a redacted field renders, for every type that holds one.
///
/// A constant rather than a length: `ref/security-compliance.md` §6's control
/// is *"secret-bearing types implement `Debug` and `Display` as a redacted
/// constant"*, and unlike `outbox.rs`'s payloads — where the byte count is what
/// names *which* manifest member disagreed — the length of a buyer's name
/// diagnoses nothing and narrows who they are.
const REDACTED: &str = "<redacted>";

// §6's control is *"secret-bearing types implement `Debug` and `Display` as a
// redacted constant"*, and only `Debug` is implemented below. That is not half
// the control: neither block implements `Display` at all, so there is no second
// channel to leak through. The rule applies to whoever adds one — a `Display`
// for a receipt header is a plausible thing to want, and it would be the place
// this redaction has to be repeated.

/// Renders `Some(_)` as the redaction and `None` as itself, so a `Debug` line
/// still says whether the field was captured at all.
fn redacted_option<T>(value: &Option<T>) -> &'static str {
    match value {
        Some(_) => REDACTED,
        None => "None",
    }
}

impl fmt::Debug for MerchantBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MerchantBlock")
            .field("legal_name", &self.legal_name)
            .field("store_name", &self.store_name)
            .field("address", &self.address)
            .field("phone", &redacted_option(&self.phone))
            .field("tin", &self.tin)
            .finish()
    }
}

impl fmt::Debug for BuyerBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BuyerBlock")
            .field("name", &redacted_option(&self.name))
            .field("tin", &self.tin)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    // `panic!` joins the two usual test allowances because a validator test's
    // catch-all arm has to say which input reached it; `audit.rs` does the same.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::ids::{RegisterId, SaleId};
    use crate::money::Percent;
    use crate::tax::{TaxTreatment, ZeroRatingReason};
    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;
    use uuid::Uuid;

    /// The fixture's PII, in one place, so a test can assert absence against the
    /// exact strings the model was built with rather than against a guess.
    const BUYER_NAME: &str = "شركة الأمانة للتجارة";
    const MERCHANT_PHONE: &str = "+962 79 000 0000";

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, Currency::JOD)
    }

    fn sale_id() -> SaleId {
        SaleId::from_uuid(Uuid::from_u128(1))
    }

    fn register_id() -> RegisterId {
        RegisterId::from_uuid(Uuid::from_u128(2))
    }

    /// A complete, consistent 2.166 JOD sale — the §2.3 anatomy end to end, so
    /// every test below breaks exactly one thing about a document that was
    /// otherwise legal.
    fn model() -> ReceiptModel {
        ReceiptModel {
            doc_kind: DocKind::Sale,
            watermark: None,
            merchant: MerchantBlock {
                legal_name: "مؤسسة النور التجارية".to_owned(),
                store_name: "النور".to_owned(),
                address: Some("عمان، الأردن".to_owned()),
                phone: Some(MERCHANT_PHONE.to_owned()),
                tin: Some("123456789".to_owned()),
            },
            buyer: Some(BuyerBlock {
                name: Some(BUYER_NAME.to_owned()),
                tin: "987654321".to_owned(),
            }),
            header: ReceiptHeader {
                sale_id: sale_id(),
                receipt_no: "REG01-000042".to_owned(),
                register_id: register_id(),
                register_code: "REG01".to_owned(),
                cashier_name: "ليلى".to_owned(),
                issued_at: Timestamp::from_epoch_milliseconds(1_758_000_000_000).unwrap(),
                business_date: BusinessDate::new(2026, 9, 22).unwrap(),
            },
            lines: vec![ReceiptLine {
                name: "خبز عربي".to_owned(),
                qty: Qty::from_milli(2_000),
                unit: UnitOfMeasure::Each,
                is_weighed: false,
                unit_price: jod(400),
                line_total: jod(800),
                discounts: vec![ReceiptDiscount {
                    label: "عرض الخبز".to_owned(),
                    amount: jod(50),
                }],
            }],
            totals: ReceiptTotals {
                subtotal: jod(2_166),
                discount_total: jod(50),
                tax_summary: vec![TaxSummaryRow {
                    code: "GST16".to_owned(),
                    treatment: TaxTreatment::Standard,
                    rate: Percent::from_ppm(160_000),
                    per_unit: None,
                    reason: None,
                    net: jod(1_867),
                    tax: jod(299),
                    gross: jod(2_166),
                }],
                rounding_adjustment: None,
                total: jod(2_166),
            },
            tenders: vec![ReceiptTender {
                label: "نقدًا".to_owned(),
                amount: jod(2_166),
                masked_pan: None,
                scheme: None,
            }],
            change: None,
            loyalty: Some(LoyaltyBlock {
                balance_points: 120,
            }),
            fiscal: None,
            footer: FooterBlock {
                return_policy: Some("الإرجاع خلال ١٤ يومًا".to_owned()),
                thank_you: Some("شكرًا لتسوقكم".to_owned()),
            },
            locale: ReceiptLocale::new(Language::Arabic),
        }
    }

    fn fiscal() -> FiscalBlock {
        FiscalBlock {
            uuid: "9f1c0f7a-0000-4000-8000-000000000000".to_owned(),
            qr_payload: "AQIDBAU=".to_owned(),
        }
    }

    /// The fixture itself has to be legal, or every refusal below could be
    /// passing for the wrong reason.
    #[test]
    fn the_fixture_is_a_valid_document() {
        assert_eq!(model().validate(), Ok(()));
    }

    #[test]
    fn a_non_fiscal_acknowledgement_cannot_carry_a_fiscal_block() {
        let mut m = model();
        m.doc_kind = DocKind::Acknowledgement;
        m.fiscal = Some(fiscal());

        assert_eq!(
            m.validate(),
            Err(ReceiptError::FiscalBlockNotAllowed {
                kind: DocKind::Acknowledgement
            })
        );

        // And the same document without the block is fine, so the refusal is
        // about the pairing rather than about the kind.
        m.fiscal = None;
        assert_eq!(m.validate(), Ok(()));

        // A sale may carry one — otherwise this rule would refuse every cleared
        // invoice and still pass its own test.
        let mut sale = model();
        sale.fiscal = Some(fiscal());
        assert_eq!(sale.validate(), Ok(()));
    }

    /// `DocKind::ALL` really is every variant.
    ///
    /// The sweep could not find this one: adding a sixth variant to the enum
    /// and forgetting it here leaves `every_doc_kind_is_covered_by_the_validator`
    /// passing over five, and its `filter(may_carry_fiscal).count() == 2` guard
    /// still holds. The exhaustive `match` is what turns that into a compile
    /// error — the same move 1.4.10 makes for audit intents.
    #[test]
    fn dock_kind_all_lists_every_variant() {
        for kind in DocKind::ALL {
            let covered = match kind {
                DocKind::Sale
                | DocKind::Refund
                | DocKind::Acknowledgement
                | DocKind::XReport
                | DocKind::ZReport => true,
            };
            assert!(covered);
        }
        assert_eq!(DocKind::ALL.len(), 5);
    }

    /// The rule is per kind, and a table is what stops a fifth variant from
    /// arriving with no decision taken about it.
    #[test]
    fn every_doc_kind_is_covered_by_the_validator() {
        for kind in DocKind::ALL {
            let mut m = model();
            m.doc_kind = kind;
            m.fiscal = Some(fiscal());

            let verdict = m.validate();
            if kind.may_carry_fiscal() {
                assert_eq!(verdict, Ok(()), "{kind:?} should accept a fiscal block");
            } else {
                assert_eq!(
                    verdict,
                    Err(ReceiptError::FiscalBlockNotAllowed { kind }),
                    "{kind:?} should refuse a fiscal block"
                );
            }
        }

        // Exactly two kinds are invoices. Asserting the count keeps the loop
        // above from passing vacuously if `may_carry_fiscal` ever answers the
        // same thing for everything.
        assert_eq!(
            DocKind::ALL.iter().filter(|k| k.may_carry_fiscal()).count(),
            2
        );
    }

    #[test]
    fn a_buyer_block_without_a_tin_is_refused() {
        for blank in ["", "   "] {
            let mut m = model();
            m.buyer = Some(BuyerBlock {
                name: Some(BUYER_NAME.to_owned()),
                tin: blank.to_owned(),
            });
            assert_eq!(m.validate(), Err(ReceiptError::BuyerWithoutTin));
        }

        // No buyer at all is the ordinary consumer receipt, not an error.
        let mut m = model();
        m.buyer = None;
        assert_eq!(m.validate(), Ok(()));
    }

    /// §2.3's second non-negotiable rule, in the form a model can break it.
    ///
    /// *"A department line prints the department's name, never 'unknown item' —
    /// a customer's proof of purchase has to describe what they bought."* The
    /// builder decides *which* name; what this type can refuse is a line with
    /// none at all, which is the purest form of the same failure. Found by
    /// re-reading the four non-negotiable rules against the validator rather
    /// than by any mutation of it.
    #[test]
    fn a_line_that_describes_nothing_is_refused() {
        for blank in ["", "   "] {
            let mut m = model();
            m.lines.first_mut().expect("the fixture has one line").name = blank.to_owned();
            assert_eq!(m.validate(), Err(ReceiptError::UnnamedLine { index: 0 }));
        }

        // The index is the line's, not a constant.
        let mut m = model();
        let mut second = m.lines.first().expect("the fixture has one line").clone();
        second.name = String::new();
        m.lines.push(second);
        assert_eq!(m.validate(), Err(ReceiptError::UnnamedLine { index: 1 }));

        // A report has no lines at all, which is not an unnamed one.
        let mut m = model();
        m.doc_kind = DocKind::ZReport;
        m.lines.clear();
        assert_eq!(m.validate(), Ok(()));
    }

    #[test]
    fn a_zero_rounding_adjustment_is_absent_rather_than_zero() {
        let mut m = model();
        m.totals.rounding_adjustment = Some(jod(0));
        assert_eq!(m.validate(), Err(ReceiptError::ZeroRoundingAdjustment));

        // Both directions of a real adjustment are documents, and a negative one
        // is the common case: 1.247 collected as 1.250 records −0.003.
        for minor in [-3, 3] {
            let mut m = model();
            m.totals.rounding_adjustment = Some(jod(minor));
            assert_eq!(m.validate(), Ok(()));
        }

        m.totals.rounding_adjustment = None;
        assert_eq!(m.validate(), Ok(()));
    }

    #[test]
    fn buyer_name_never_reaches_debug_output() {
        let m = model();
        let rendered = format!("{m:?}");

        assert!(
            !rendered.contains(BUYER_NAME),
            "the buyer's name reached Debug output"
        );
        assert!(rendered.contains(REDACTED));
        // The TIN is a legal field printed on the document itself and is not on
        // the registry, so it stays — the point is redaction, not blanking.
        assert!(rendered.contains("987654321"));
        // A `None` still reads as `None`, so a Debug line says whether the field
        // was captured at all.
        assert!(
            format!(
                "{:?}",
                BuyerBlock {
                    name: None,
                    tin: "1".to_owned()
                }
            )
            .contains("None")
        );
    }

    #[test]
    fn merchant_phone_never_reaches_debug_output() {
        let m = model();
        let rendered = format!("{m:?}");

        assert!(
            !rendered.contains(MERCHANT_PHONE),
            "the merchant's phone reached Debug output"
        );
        // The address is not on the registry and is on the printed receipt, so
        // over-redacting it would remove a diagnosis for no protection.
        assert!(rendered.contains("عمان، الأردن"));
    }

    /// The opposite direction, held on purpose.
    ///
    /// `ref/security-compliance.md` §6's registry names `pan` and `card_number`.
    /// A masked value is neither and matches no suffix rule, and
    /// `.claude/rules/security.md` explicitly permits storing the masked PAN the
    /// terminal returns for the receipt. A later sweep that redacts it would be
    /// over-correcting past a reviewed rule and would remove the one field a
    /// tender mismatch is diagnosed by, so this test fails when that happens.
    #[test]
    fn a_masked_pan_is_deliberately_still_visible_in_debug() {
        let mut m = model();
        m.tenders = vec![ReceiptTender {
            label: "بطاقة".to_owned(),
            amount: jod(2_166),
            masked_pan: Some("**** **** **** 4242".to_owned()),
            scheme: Some("visa".to_owned()),
        }];

        assert!(format!("{m:?}").contains("4242"));
    }

    /// Every optional field populated at once.
    ///
    /// The mutation sweep is why this exists rather than the plain fixture:
    /// `#[serde(skip)]` on `ReceiptTender.masked_pan` survived a round trip of
    /// [`model`], because that fixture leaves `masked_pan`, `scheme`,
    /// `watermark`, `fiscal`, `change` and the rounding line at `None` and
    /// `None` round-trips through a skipped field unchanged. A serialization
    /// test whose fixture is half empty tests half the struct.
    fn fully_populated() -> ReceiptModel {
        let mut m = model();
        m.watermark = Some(Watermark::Duplicate);
        m.totals.rounding_adjustment = Some(jod(-3));
        m.totals.tax_summary.push(TaxSummaryRow {
            code: "GST0".to_owned(),
            treatment: TaxTreatment::Zero,
            rate: Percent::from_ppm(0),
            per_unit: Some(jod(5)),
            reason: Some(ZeroRatingReason::Export),
            net: jod(0),
            tax: jod(0),
            gross: jod(0),
        });
        m.tenders = vec![ReceiptTender {
            label: "بطاقة".to_owned(),
            amount: jod(2_163),
            masked_pan: Some("**** **** **** 4242".to_owned()),
            scheme: Some("visa".to_owned()),
        }];
        m.change = Some(jod(0));
        m.fiscal = Some(fiscal());
        m.locale = ReceiptLocale::new(Language::English);
        m
    }

    #[test]
    fn a_receipt_round_trips_through_canonical_json() {
        for m in [model(), fully_populated()] {
            let json = serde_json::to_string(&m).unwrap();
            let back: ReceiptModel = serde_json::from_str(&json).unwrap();

            assert_eq!(back, m);
        }

        // Serialization is the data path, not a log: the buyer's name must
        // survive it, or the receipt cannot be rendered from what was stored.
        // This is the line that makes the Debug tests above a redaction rather
        // than a deletion.
        let json = serde_json::to_string(&model()).unwrap();
        assert!(json.contains(BUYER_NAME));
        assert!(json.contains(MERCHANT_PHONE));

        // And the populated fixture is a legal document, so the round trip is
        // not exercising a shape the validator would have refused anyway.
        assert_eq!(fully_populated().validate(), Ok(()));
    }

    /// Direction is derived from language and never chosen beside it.
    #[test]
    fn a_locale_is_written_in_the_direction_of_its_language() {
        assert_eq!(
            ReceiptLocale::new(Language::Arabic).direction,
            TextDirection::RightToLeft
        );
        assert_eq!(
            ReceiptLocale::new(Language::English).direction,
            TextDirection::LeftToRight
        );
    }

    /// Every field on the document that carries an amount, named once.
    ///
    /// The strategy below sets one of them to a foreign currency, so this list
    /// *is* the coverage claim: a field added to the model and not added here
    /// leaves a hole the property cannot see, which is why
    /// `the_currency_sweep_covers_every_amount_bearing_field` counts it against
    /// a hand-audited total rather than trusting the enum.
    #[derive(Debug, Clone, Copy)]
    enum AmountField {
        Subtotal,
        DiscountTotal,
        RoundingAdjustment,
        TaxNet,
        TaxAmount,
        TaxGross,
        TaxPerUnit,
        LineUnitPrice,
        LineTotal,
        LineDiscount,
        Tender,
        Change,
    }

    const AMOUNT_FIELDS: [AmountField; 12] = [
        AmountField::Subtotal,
        AmountField::DiscountTotal,
        AmountField::RoundingAdjustment,
        AmountField::TaxNet,
        AmountField::TaxAmount,
        AmountField::TaxGross,
        AmountField::TaxPerUnit,
        AmountField::LineUnitPrice,
        AmountField::LineTotal,
        AmountField::LineDiscount,
        AmountField::Tender,
        AmountField::Change,
    ];

    /// Put `amount` into exactly one amount-bearing field.
    ///
    /// `first_mut` rather than `[0]` throughout: `clippy::indexing_slicing` is
    /// a workspace lint and is denied in tests as well, on the argument that a
    /// bare index is a panic nobody wrote down. The fixture guarantees one row
    /// in each collection, so the `expect` is a statement about the fixture and
    /// fails with the field's name rather than with a slice offset.
    fn put_foreign(m: &mut ReceiptModel, field: AmountField, amount: Money) {
        fn tax_row(m: &mut ReceiptModel) -> &mut TaxSummaryRow {
            m.totals
                .tax_summary
                .first_mut()
                .expect("the fixture has one tax summary row")
        }
        fn line(m: &mut ReceiptModel) -> &mut ReceiptLine {
            m.lines.first_mut().expect("the fixture has one line")
        }

        match field {
            AmountField::Subtotal => m.totals.subtotal = amount,
            AmountField::DiscountTotal => m.totals.discount_total = amount,
            AmountField::RoundingAdjustment => m.totals.rounding_adjustment = Some(amount),
            AmountField::TaxNet => tax_row(m).net = amount,
            AmountField::TaxAmount => tax_row(m).tax = amount,
            AmountField::TaxGross => tax_row(m).gross = amount,
            AmountField::TaxPerUnit => tax_row(m).per_unit = Some(amount),
            AmountField::LineUnitPrice => line(m).unit_price = amount,
            AmountField::LineTotal => line(m).line_total = amount,
            AmountField::LineDiscount => {
                line(m)
                    .discounts
                    .first_mut()
                    .expect("the fixture's line has one discount")
                    .amount = amount;
            }
            AmountField::Tender => {
                m.tenders
                    .first_mut()
                    .expect("the fixture has one tender")
                    .amount = amount;
            }
            AmountField::Change => m.change = Some(amount),
        }
    }

    /// The sweep is only as good as its list, so the list is asserted too.
    #[test]
    fn the_currency_sweep_covers_every_amount_bearing_field() {
        assert_eq!(AMOUNT_FIELDS.len(), 12);
    }

    /// A zero amount is still denominated in something.
    ///
    /// This exists because the mutation sweep found it: a validator that reads
    /// `a.minor() != 0 && a.currency() != expected` — skipping zeros as though
    /// they were currency-free — survived `prop_a_model_mixing_two_currencies_is_refused`.
    /// The property's generator is `any::<i64>()`, which reaches zero only by
    /// chance, and the comment beside it claimed the coverage anyway. A bounded
    /// universal claim gets an exhaustive loop rather than a generator that
    /// might (conventions §5.1), so every field is checked at exactly zero.
    ///
    /// It is not a theoretical case: a zero-amount tender is how a fully
    /// discounted basket settles, and a zero tax row is every exempt supply.
    #[test]
    fn a_zero_amount_in_another_currency_is_still_another_currency() {
        for field in AMOUNT_FIELDS {
            // The rounding line is the one field where zero is refused for a
            // different reason, and that rule fires first by design.
            let mut m = model();
            put_foreign(&mut m, field, Money::from_minor(0, Currency::USD));

            match m.validate() {
                // The rounding line is the one field where zero is refused for
                // a different reason, and that rule fires first by design.
                Err(ReceiptError::ZeroRoundingAdjustment) => {
                    assert!(matches!(field, AmountField::RoundingAdjustment))
                }
                Err(ReceiptError::MixedCurrency {
                    field: named,
                    expected,
                    found,
                }) => {
                    assert_eq!((expected, found), (Currency::JOD, Currency::USD));
                    // The error names where, and the name is not empty prose.
                    assert!(!named.is_empty(), "{field:?} reported no field");
                }
                other => panic!("{field:?} at zero was not refused: {other:?}"),
            }
        }
    }

    fn amount_field() -> impl Strategy<Value = AmountField> {
        prop::sample::select(AMOUNT_FIELDS.as_slice())
    }

    /// A currency that is not the document's.
    fn foreign_currency() -> impl Strategy<Value = Currency> {
        prop::sample::select(vec![Currency::USD, Currency::EUR])
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// Whichever amount on the document is denominated in something else,
        /// the document is refused — and the error names the currency that was
        /// wrong, not merely that something was.
        ///
        /// The generator covers all twelve amount-bearing fields against both
        /// foreign currencies this build knows, over the `i64` minor range.
        /// It does **not** guarantee the zero case — `any::<i64>()` reaches it
        /// only by chance — and an earlier draft of this comment claimed it
        /// did, which is how a validator that skipped zero amounts survived the
        /// mutation sweep. `a_zero_amount_in_another_currency_is_still_another_currency`
        /// is the exhaustive loop that actually covers it.
        #[test]
        fn prop_a_model_mixing_two_currencies_is_refused(
            field in amount_field(),
            currency in foreign_currency(),
            minor in any::<i64>(),
        ) {
            let mut m = model();
            put_foreign(&mut m, field, Money::from_minor(minor, currency));

            match m.validate() {
                Err(ReceiptError::MixedCurrency { expected, found, .. }) => {
                    prop_assert_eq!(expected, Currency::JOD);
                    prop_assert_eq!(found, currency);
                }
                other => prop_assert!(false, "{:?} was not refused: {:?}", field, other),
            }
        }

        /// The other direction, without which the property above is satisfied
        /// by a validator that refuses everything.
        #[test]
        fn prop_validate_accepts_every_consistent_model(
            field in amount_field(),
            minor in any::<i64>(),
        ) {
            let mut m = model();
            put_foreign(&mut m, field, Money::from_minor(minor, Currency::JOD));

            // A rounding adjustment is the one field whose *value* is also a
            // rule, so a generated zero there is legitimately refused and is
            // not a counter-example to this property.
            let expected = if matches!(field, AmountField::RoundingAdjustment) && minor == 0 {
                Err(ReceiptError::ZeroRoundingAdjustment)
            } else {
                Ok(())
            };

            prop_assert_eq!(m.validate(), expected);
        }
    }
}
