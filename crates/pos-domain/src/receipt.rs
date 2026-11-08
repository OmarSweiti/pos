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
//! §2.3. It rounds nothing. Every amount arrives already decided by
//! `price_cart` (1.4.9) and the tender machine (1.5.3), and every string
//! already resolved by its caller, which is what makes this module trivially
//! pure (I-8) and what stops a second rounding from happening at render time
//! (I-1). [`build_receipt_model`] (1.8.3) adds one figure, the total the
//! customer was charged, and it is a sum of two amounts the sale already holds.
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
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cart::CompletedSale;
use crate::catalog::UnitOfMeasure;
use crate::ids::{RegisterId, SaleId};
use crate::money::Qty;
use crate::money::{Currency, Money, MoneyError};
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
    /// A tender whose kind has no printed name in the document's language
    /// (1.8.3). A tender kind's code is the store's configuration, not a
    /// customer's, so the error names it.
    #[error("the tender kind {code:?} has no printed name")]
    TenderUnlabelled { code: String },
    /// A discount whose reason has no printed name in the document's language
    /// (1.8.3). The reason is a code from the store's list, so the error names
    /// it.
    #[error("the discount reason {code:?} has no printed name")]
    DiscountUnlabelled { code: String },
    /// A loyalty balance for a sale with no customer attached (1.8.3).
    #[error("a loyalty balance is printed only for a sale with a customer attached")]
    LoyaltyWithoutCustomer,
    /// A training sale handed a cleared fiscal block (1.8.3). A training sale
    /// never reaches the fiscal queue, so a block is one that belongs to
    /// another sale.
    #[error("a training sale is never a fiscal invoice")]
    TrainingSaleCleared,
    /// What adding the cash rounding to the priced total refuses (1.8.3). A
    /// [`CompletedSale`] has already added the two, so this does not arise from
    /// a sale `complete` made.
    #[error(transparent)]
    Money(#[from] MoneyError),
}

/// Whether the document a sale is handed over with is a tax invoice
/// (`ref/fiscal-jofotara.md` §2.1, `ref/hardware-and-receipts.md` §2.3).
///
/// [`build_receipt_model`] reads the heading from this and takes no heading as
/// a choice, so no caller can hand a customer a document that calls itself an
/// invoice before JoFotara has cleared it. The shell reads which variant
/// applies from the store's `fiscal_obligation` and the sale's fiscal result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FiscalStanding {
    /// The store owes no fiscal invoice: `store.fiscal_obligation` is
    /// `'exempt'`, with its evidence. The sale prints as a sale, with no fiscal
    /// block.
    Exempt,
    /// The store owes one, and this sale has not cleared. Until the outage
    /// ruling that 2.7.0 owns says otherwise, the customer is handed a clearly
    /// marked non-fiscal payment acknowledgement.
    Uncleared,
    /// Cleared: the invoice JoFotara returned, printed with the sale.
    Cleared(FiscalBlock),
}

/// What a receipt says that its sale does not (`ref/domain-api.md` §13).
///
/// Every field arrives resolved, as the model's own do: the shell reads the
/// store, the register, the cashier and the store's lists, and this crate reads
/// nothing (I-8). `Debug` is derived and reaches [`MerchantBlock`]'s own, so
/// the merchant's phone stays redacted.
#[derive(Debug, Clone, PartialEq)]
pub struct ReceiptContext {
    pub merchant: MerchantBlock,
    /// `register.code`.
    pub register_code: String,
    /// The cashier's name as the receipt prints it.
    pub cashier_name: String,
    /// The language the document is set in: `store.receipt_locale`.
    pub language: Language,
    /// Whether the document is a tax invoice, and its fiscal block once it is.
    pub fiscal: FiscalStanding,
    /// Each tender kind's printed name in `language`, by `tender_type.code`.
    pub tender_labels: BTreeMap<String, String>,
    /// Each discount reason's printed name in `language`, by its code.
    pub discount_labels: BTreeMap<String, String>,
    /// The attached customer's points balance, read only when the sale names a
    /// customer.
    pub loyalty: Option<LoyaltyBlock>,
    /// The footer, resolved from the template version this receipt is written
    /// under.
    pub footer: FooterBlock,
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

/// The receipt for `sale`: the one document the printer, the PDF and the email
/// all render (`ref/domain-api.md` §13, 1.8.3).
///
/// Every amount is the sale's own, read and never worked out again:
/// - **A line** prints its snapshot name, its quantity, what it charges a unit
///   ([`CartLine::price`](crate::cart::CartLine::price), an override
///   included) and its gross before discounts, as `price_cart` priced it.
///   Beneath it go its discount attributions, its own discount and its share
///   of the basket's, each under its reason's printed name.
/// - **The totals** are `price_cart`'s subtotal, discount total and tax rows.
/// - **The cash-rounding line** is the final cash tender's adjustment, printed
///   only when it adjusted something.
/// - **The total** is what the customer was charged: the priced total plus that
///   adjustment, the one figure this adds.
/// - **The tenders** are the sale's, in the order they were taken. The change
///   is printed when there was any.
///
/// So the document adds up on its face. Its lines come to its subtotal, and
/// its line discounts to its discount total. The subtotal less the discounts,
/// plus the tax where prices exclude it, plus the rounding, is its total, and
/// its tenders less the change come to the same total.
/// `prop_a_receipt_adds_up_to_its_own_total` holds it to that.
///
/// **The heading follows `ctx.fiscal`** ([`FiscalStanding`]). A training sale
/// carries the TRAINING watermark, and a business buyer's block is printed
/// whenever the sale captured a TIN.
///
/// The result has passed [`ReceiptModel::validate`]. A builder that could hand
/// back a document its own validator refuses would leave the refusal to the
/// printer, after the sale had completed.
///
/// # Errors
///
/// - [`ReceiptError::TrainingSaleCleared`]: a training sale given a fiscal
///   block.
/// - [`ReceiptError::LoyaltyWithoutCustomer`]: a loyalty balance for a sale
///   with no customer.
/// - [`ReceiptError::DiscountUnlabelled`] and
///   [`ReceiptError::TenderUnlabelled`]: a reason or a tender kind with no
///   printed name. A blank name is no name.
/// - [`ReceiptError::Money`]: the rounding cannot be added to the total, which
///   a sale `complete` made never meets.
/// - Whatever [`ReceiptModel::validate`] refuses of the document, such as a
///   buyer block whose TIN is blank.
pub fn build_receipt_model(
    sale: &CompletedSale,
    ctx: &ReceiptContext,
) -> Result<ReceiptModel, ReceiptError> {
    let cart = sale.cart();
    let priced = sale.priced();
    if cart.is_training && matches!(ctx.fiscal, FiscalStanding::Cleared(_)) {
        return Err(ReceiptError::TrainingSaleCleared);
    }
    if ctx.loyalty.is_some() && cart.customer_id.is_none() {
        return Err(ReceiptError::LoyaltyWithoutCustomer);
    }
    let (doc_kind, fiscal) = match &ctx.fiscal {
        FiscalStanding::Exempt => (DocKind::Sale, None),
        FiscalStanding::Uncleared => (DocKind::Acknowledgement, None),
        FiscalStanding::Cleared(block) => (DocKind::Sale, Some(block.clone())),
    };

    // A completed sale's prices are its cart's lines', one for one and in
    // order: `CompletedSale::restore` refuses any other pairing, and `complete`
    // never makes one.
    let mut lines = Vec::with_capacity(cart.lines.len());
    for (line, money) in cart.lines.iter().zip(&priced.lines) {
        let mut discounts = Vec::with_capacity(money.discount_attributions.len());
        for attribution in &money.discount_attributions {
            let code = &attribution.reason.0;
            discounts.push(ReceiptDiscount {
                label: printed_name(&ctx.discount_labels, code)
                    .ok_or_else(|| ReceiptError::DiscountUnlabelled { code: code.clone() })?,
                amount: attribution.amount,
            });
        }
        lines.push(ReceiptLine {
            name: line.name_snapshot.clone(),
            qty: line.qty,
            unit: line.unit,
            is_weighed: line.is_weighed,
            unit_price: line.price(),
            line_total: money.gross_before_discount,
            discounts,
        });
    }

    let mut tenders = Vec::with_capacity(sale.tenders().len());
    for tender in sale.tenders() {
        tenders.push(ReceiptTender {
            label: printed_name(&ctx.tender_labels, &tender.type_code).ok_or_else(|| {
                ReceiptError::TenderUnlabelled {
                    code: tender.type_code.clone(),
                }
            })?,
            amount: tender.amount,
            masked_pan: tender.masked_pan.clone(),
            scheme: tender.scheme.clone(),
        });
    }

    let rounding_adjustment = sale
        .cash_rounding()
        .map(|rounding| rounding.adjustment)
        .filter(|adjustment| !adjustment.is_zero());
    let total = match rounding_adjustment {
        Some(adjustment) => priced.total.checked_add(adjustment)?,
        None => priced.total,
    };

    let model = ReceiptModel {
        doc_kind,
        watermark: cart.is_training.then_some(Watermark::Training),
        merchant: ctx.merchant.clone(),
        buyer: cart.buyer_tin.as_ref().map(|tin| BuyerBlock {
            name: cart.buyer_name.clone(),
            tin: tin.clone(),
        }),
        header: ReceiptHeader {
            sale_id: cart.id,
            receipt_no: sale.receipt_number().to_owned(),
            register_id: cart.register_id,
            register_code: ctx.register_code.clone(),
            cashier_name: ctx.cashier_name.clone(),
            issued_at: sale.completed_at(),
            business_date: sale.business_date(),
        },
        lines,
        totals: ReceiptTotals {
            subtotal: priced.subtotal,
            discount_total: priced.discount_total,
            tax_summary: priced.tax_summary.clone(),
            rounding_adjustment,
            total,
        },
        tenders,
        change: Some(sale.change()).filter(|change| !change.is_zero()),
        loyalty: ctx.loyalty,
        fiscal,
        footer: ctx.footer.clone(),
        locale: ReceiptLocale::new(ctx.language),
    };
    model.validate()?;
    Ok(model)
}

/// `code`'s printed name in `labels`, when it has one that is not blank.
fn printed_name(labels: &BTreeMap<String, String>, code: &str) -> Option<String> {
    labels
        .get(code)
        .filter(|name| !name.trim().is_empty())
        .cloned()
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

    /// GST registration and the electronic-invoice obligation are two
    /// settings (`ref/tax-jordan.md` §2), and one never decides the other. A
    /// store not registered for GST charges no tax and its receipt has no tax
    /// rows, yet it may still owe an invoice; a registered store may owe none.
    /// So every pairing of the two fixtures is a valid sale document, and the
    /// fiscal block is exactly what the fiscal fixture says.
    #[test]
    fn unregistered_gst_profile_does_not_disable_fiscal_obligation() {
        use crate::money::{Qty, RoundingRule};
        use crate::tax::{
            PriceMode, StoreTaxProfile, SupplyDestination, SupplyTaxContext, TaxBase, TaxBasis,
            TaxComponent, compute_line_tax, summarize_tax,
        };

        let gst = TaxComponent {
            code: "SYNTH_GST".to_owned(),
            treatment: TaxTreatment::Standard,
            basis: TaxBasis::Percentage {
                rate: Percent::from_ppm(160_000),
            },
            sequence: 0,
            base: TaxBase::LineNet,
            is_inclusive_capable: true,
        };
        let domestic = SupplyTaxContext {
            destination: SupplyDestination::Domestic,
            reason: None,
            evidence_ref: None,
        };
        let summary = |profile| {
            let line = compute_line_tax(
                jod(2_166),
                Qty::ONE,
                PriceMode::Inclusive,
                std::slice::from_ref(&gst),
                profile,
                &domestic,
                RoundingRule::HalfAwayFromZero,
            )
            .unwrap();
            summarize_tax(&[line]).unwrap()
        };

        for profile in [StoreTaxProfile::Unregistered, StoreTaxProfile::Standard] {
            let rows = summary(profile);
            assert_eq!(rows.is_empty(), profile == StoreTaxProfile::Unregistered);
            for invoice in [Some(fiscal()), None] {
                let mut sale = model();
                sale.totals.tax_summary = rows.clone();
                sale.fiscal = invoice.clone();
                assert_eq!(sale.validate(), Ok(()), "{profile:?}, {invoice:?}");
                assert_eq!(sale.fiscal, invoice, "{profile:?} decided the invoice");
            }
        }
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

    /// `build_receipt_model` (1.8.3): a completed sale, made through the
    /// domain's own transitions, turned into its receipt.
    mod built {
        use super::*;
        use crate::cart::{
            Cart, CartContext, CartLine, CompletedSale, FinalizeEffects, PricingSettings, TaxBands,
            add_tender, begin_finalize, begin_tender, complete, open, remaining_due,
        };
        use crate::catalog::PriceOrigin;
        use crate::ids::{
            CustomerId, ProductId, SaleLineId, ShiftId, TaxCategoryId, TenderId, UserId,
        };
        use crate::money::{RoundingDirection, RoundingRule};
        use crate::pricing::{
            BasketDiscount, DiscountKind, DiscountReason, LineDiscount, OverrideReason,
            PriceOverride,
        };
        use crate::tax::{PriceMode, StoreTaxProfile, TaxBase, TaxBasis, TaxComponent};
        use crate::tender::{
            CashRoundingPolicy, Tender, TenderState, final_tender_rounding, standard_tender_type,
        };

        fn id(n: u128) -> Uuid {
            Uuid::from_u128(n)
        }

        fn band(n: u8) -> TaxCategoryId {
            TaxCategoryId::from_uuid(id(0x5000 + u128::from(n)))
        }

        fn gst(treatment: TaxTreatment, ppm: i64) -> Vec<TaxComponent> {
            vec![TaxComponent {
                code: "SYN_GST".to_owned(),
                treatment,
                basis: TaxBasis::Percentage {
                    rate: Percent::from_ppm(ppm),
                },
                sequence: 1,
                base: TaxBase::LineNet,
                is_inclusive_capable: true,
            }]
        }

        /// A synthetic pack: 16% standard, zero-rated and exempt.
        fn bands() -> TaxBands {
            TaxBands::from([
                (band(0), gst(TaxTreatment::Standard, 160_000)),
                (band(1), gst(TaxTreatment::Zero, 0)),
                (band(2), gst(TaxTreatment::Exempt, 0)),
            ])
        }

        fn settings(price_mode: PriceMode, rule: RoundingRule) -> PricingSettings {
            PricingSettings {
                price_mode,
                profile: StoreTaxProfile::Standard,
                rule,
            }
        }

        /// The fixture store's coin: five fils, to the nearest.
        const COIN: CashRoundingPolicy = CashRoundingPolicy {
            step_minor: 5,
            direction: RoundingDirection::Nearest,
        };

        /// When the cart was opened: 2026-11-05 08:00 UTC, 11:00 in Amman,
        /// on the business day the sale is dated by.
        fn moment() -> Timestamp {
            Timestamp::from_epoch_milliseconds(1_793_865_600_000).unwrap()
        }

        /// When the sale completed, a minute and a half later, so a receipt
        /// dated by its cart's opening cannot pass for one dated by its sale.
        fn completed() -> Timestamp {
            Timestamp::from_epoch_milliseconds(1_793_865_690_000).unwrap()
        }

        fn till(is_training: bool) -> CartContext {
            CartContext {
                register_id: RegisterId::from_uuid(id(2)),
                shift_id: ShiftId::from_uuid(id(3)),
                cashier_id: UserId::from_uuid(id(4)),
                currency: Currency::JOD,
                is_training,
                opened_at: moment(),
            }
        }

        fn basket(is_training: bool) -> Cart {
            open(SaleId::from_uuid(id(1)), &till(is_training))
        }

        /// Line `n`: `milli` of `name` at `price` fils a unit, in tax band `b`.
        fn item(n: u128, name: &str, price: i64, milli: i64, weighed: bool, b: u8) -> CartLine {
            let product = ProductId::from_uuid(id(0x2000 + n));
            CartLine {
                id: SaleLineId::from_uuid(id(0x1000 + n)),
                product_id: Some(product),
                name_snapshot: name.to_owned(),
                unit_price: jod(price),
                price_origin: PriceOrigin::Catalog {
                    product_id: product,
                },
                max_price: None,
                qty: Qty::from_milli(milli),
                unit: if weighed {
                    UnitOfMeasure::Kilogram
                } else {
                    UnitOfMeasure::Each
                },
                is_weighed: weighed,
                tax_category_id: band(b),
                discount: None,
                price_override: None,
                min_age: None,
                age_confirmed: false,
                label: None,
            }
        }

        fn oil() -> CartLine {
            item(1, "زيت زيتون اصطناعي", 1_250, 1_000, false, 0)
        }

        /// 101 g of cheese at 8.500 a kilogram: 0.8585, priced once at 0.859.
        fn cheese() -> CartLine {
            item(2, "جبنة اصطناعية", 8_500, 101, true, 0)
        }

        fn tender(n: u128, code: &str, minor: i64) -> Tender {
            Tender {
                id: TenderId::from_uuid(id(0x3000 + n)),
                type_code: code.to_owned(),
                amount: jod(minor),
                psp_ref: None,
                masked_pan: None,
                scheme: None,
                state: TenderState::Collected,
            }
        }

        fn cash(n: u128, minor: i64) -> Tender {
            tender(n, "cash", minor)
        }

        fn card(n: u128, minor: i64) -> Tender {
            Tender {
                psp_ref: Some(format!("SYN-PSP-{n}")),
                masked_pan: Some("**** **** **** 4242".to_owned()),
                scheme: Some("visa".to_owned()),
                ..tender(n, "card", minor)
            }
        }

        fn effects() -> FinalizeEffects {
            FinalizeEffects {
                receipt_number: "REG01-000042".to_owned(),
                business_date: BusinessDate::new(2026, 11, 5).unwrap(),
                completed_at: completed(),
            }
        }

        /// `cart` paid for by `tenders`, in order, and completed.
        fn sold(cart: Cart, price_mode: PriceMode, tenders: Vec<Tender>) -> CompletedSale {
            let settings = settings(price_mode, RoundingRule::HalfAwayFromZero);
            let paying = begin_tender(cart, &bands(), &settings, COIN).unwrap();
            let paid = tenders
                .into_iter()
                .fold(paying, |t, next| add_tender(t, next).unwrap());
            complete(begin_finalize(paid).unwrap(), effects()).unwrap()
        }

        fn reason(code: &str) -> DiscountReason {
            DiscountReason(code.to_owned())
        }

        fn context() -> ReceiptContext {
            ReceiptContext {
                merchant: MerchantBlock {
                    legal_name: "مؤسسة اصطناعية للتجارة".to_owned(),
                    store_name: "متجر اصطناعي".to_owned(),
                    address: Some("عمان".to_owned()),
                    phone: Some(MERCHANT_PHONE.to_owned()),
                    tin: Some("123456789".to_owned()),
                },
                register_code: "REG01".to_owned(),
                cashier_name: "Synthetic Cashier".to_owned(),
                language: Language::Arabic,
                fiscal: FiscalStanding::Uncleared,
                tender_labels: BTreeMap::from([
                    ("cash".to_owned(), "نقدًا".to_owned()),
                    ("card".to_owned(), "بطاقة".to_owned()),
                ]),
                discount_labels: BTreeMap::from([
                    ("damaged".to_owned(), "عبوة تالفة".to_owned()),
                    ("regular".to_owned(), "زبون دائم".to_owned()),
                ]),
                loyalty: None,
                footer: FooterBlock {
                    return_policy: Some("الإرجاع خلال ١٤ يومًا".to_owned()),
                    thank_you: Some("شكرًا لتسوقكم".to_owned()),
                },
            }
        }

        /// The sale the tender suite pays for: oil and 101 g of cheese, 2.109,
        /// asked as 2.110 in cash. 1.000 is taken exactly, then 5.000 settles
        /// the 1.110 still asked and gives 3.890 back.
        fn the_cash_sale() -> CompletedSale {
            let mut cart = basket(false);
            cart.lines = vec![oil(), cheese()];
            sold(
                cart,
                PriceMode::Inclusive,
                vec![cash(1, 1_000), cash(2, 5_000)],
            )
        }

        #[test]
        fn a_cash_sale_prints_its_lines_totals_rounding_and_change() {
            let sale = the_cash_sale();
            let m = build_receipt_model(&sale, &context()).unwrap();

            // The header is the sale's: its number and moment are the
            // finalize's, its register the cart's, and the names the context's.
            assert_eq!(
                m.header,
                ReceiptHeader {
                    sale_id: SaleId::from_uuid(id(1)),
                    receipt_no: "REG01-000042".to_owned(),
                    register_id: RegisterId::from_uuid(id(2)),
                    register_code: "REG01".to_owned(),
                    cashier_name: "Synthetic Cashier".to_owned(),
                    issued_at: completed(),
                    business_date: BusinessDate::new(2026, 11, 5).unwrap(),
                }
            );

            // Each line: its snapshot name, quantity, unit price and gross.
            let printed: Vec<_> = m
                .lines
                .iter()
                .map(|l| {
                    (
                        l.name.as_str(),
                        l.qty.milli(),
                        l.is_weighed,
                        l.unit_price.minor(),
                        l.line_total.minor(),
                    )
                })
                .collect();
            assert_eq!(
                printed,
                [
                    ("زيت زيتون اصطناعي", 1_000, false, 1_250, 1_250),
                    ("جبنة اصطناعية", 101, true, 8_500, 859),
                ]
            );
            assert!(m.lines.iter().all(|l| l.discounts.is_empty()));

            // 2.109 owed, asked as 2.110: the rounding prints, and the total is
            // what was charged.
            assert_eq!(m.totals.subtotal, jod(2_109));
            assert_eq!(m.totals.discount_total, jod(0));
            assert_eq!(m.totals.tax_summary, sale.priced().tax_summary);
            assert_eq!(m.totals.rounding_adjustment, Some(jod(1)));
            assert_eq!(m.totals.total, jod(2_110));

            // Both tenders, as taken, and the change.
            let tenders: Vec<_> = m
                .tenders
                .iter()
                .map(|t| (t.label.as_str(), t.amount.minor()))
                .collect();
            assert_eq!(tenders, [("نقدًا", 1_000), ("نقدًا", 5_000)]);
            assert_eq!(m.change, Some(jod(3_890)));

            // The rest is the context's.
            assert_eq!(m.doc_kind, DocKind::Acknowledgement);
            assert_eq!(
                (m.watermark, m.buyer.clone(), m.loyalty, m.fiscal.clone()),
                (None, None, None, None)
            );
            assert_eq!(m.merchant, context().merchant);
            assert_eq!(m.footer, context().footer);
            assert_eq!(m.locale, ReceiptLocale::new(Language::Arabic));
            assert_eq!(m.validate(), Ok(()));

            // The language is the store's, and the direction follows it.
            let english = ReceiptContext {
                language: Language::English,
                ..context()
            };
            assert_eq!(
                build_receipt_model(&sale, &english).unwrap().locale,
                ReceiptLocale::new(Language::English)
            );
        }

        /// The heading is decided by the fiscal standing, for each of the
        /// three, and never chosen beside it.
        #[test]
        fn the_heading_is_an_invoice_only_once_cleared_or_exempt() {
            for (standing, kind, block) in [
                (FiscalStanding::Exempt, DocKind::Sale, None),
                (FiscalStanding::Uncleared, DocKind::Acknowledgement, None),
                (
                    FiscalStanding::Cleared(fiscal()),
                    DocKind::Sale,
                    Some(fiscal()),
                ),
            ] {
                let ctx = ReceiptContext {
                    fiscal: standing.clone(),
                    ..context()
                };
                let m = build_receipt_model(&the_cash_sale(), &ctx).unwrap();
                assert_eq!((m.doc_kind, m.fiscal), (kind, block), "{standing:?}");
            }
        }

        #[test]
        fn a_training_sale_is_watermarked_and_never_cleared() {
            let mut cart = basket(true);
            cart.lines = vec![oil()];
            let training = sold(cart, PriceMode::Inclusive, vec![cash(1, 1_250)]);

            for standing in [FiscalStanding::Exempt, FiscalStanding::Uncleared] {
                let ctx = ReceiptContext {
                    fiscal: standing,
                    ..context()
                };
                let m = build_receipt_model(&training, &ctx).unwrap();
                assert_eq!(m.watermark, Some(Watermark::Training));
            }

            let cleared = ReceiptContext {
                fiscal: FiscalStanding::Cleared(fiscal()),
                ..context()
            };
            assert_eq!(
                build_receipt_model(&training, &cleared),
                Err(ReceiptError::TrainingSaleCleared)
            );
            // A live sale may carry one, and has no watermark.
            let live = build_receipt_model(&the_cash_sale(), &cleared).unwrap();
            assert_eq!(live.watermark, None);
        }

        /// Each line prints its gross, and beneath it its own discount, then
        /// its share of the basket's, under each reason's printed name.
        #[test]
        fn discounts_print_beneath_their_line_under_their_reasons_name() {
            let mut cart = basket(false);
            let mut tea = item(3, "شاي اصطناعي", 2_000, 2_000, false, 0);
            tea.discount = Some(LineDiscount {
                kind: DiscountKind::Percent(Percent::from_ppm(100_000)),
                reason: reason("damaged"),
                by: UserId::from_uuid(id(4)),
                approver: None,
            });
            cart.lines = vec![tea, oil()];
            cart.basket_discount = Some(BasketDiscount {
                kind: DiscountKind::Amount(jod(500)),
                reason: reason("regular"),
                by: UserId::from_uuid(id(4)),
                approver: None,
            });
            // 4.000 less 0.400 is 3.600 of tea, and 1.250 of oil: 4.850, less
            // 0.500 divided 3.600 : 1.250, is 4.350.
            let sale = sold(cart, PriceMode::Inclusive, vec![cash(1, 4_350)]);
            let m = build_receipt_model(&sale, &context()).unwrap();

            let printed: Vec<_> = m
                .lines
                .iter()
                .map(|l| {
                    (
                        l.line_total.minor(),
                        l.discounts
                            .iter()
                            .map(|d| (d.label.as_str(), d.amount.minor()))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect();
            assert_eq!(
                printed,
                [
                    (4_000, vec![("عبوة تالفة", 400), ("زبون دائم", 371)]),
                    (1_250, vec![("زبون دائم", 129)]),
                ]
            );
            assert_eq!(m.totals.subtotal, jod(5_250));
            assert_eq!(m.totals.discount_total, jod(900));
            assert_eq!(m.totals.total, jod(4_350));
            assert_eq!((m.totals.rounding_adjustment, m.change), (None, None));
        }

        /// The line prints the price it charges, not the one it was captured
        /// at.
        #[test]
        fn an_overridden_line_prints_the_price_it_charges() {
            let mut cart = basket(false);
            let mut over = oil();
            over.price_override = Some(PriceOverride {
                to: jod(1_000),
                reason: OverrideReason::from("damaged".to_owned()),
                by: UserId::from_uuid(id(4)),
                approver: None,
            });
            cart.lines = vec![over];
            let sale = sold(cart, PriceMode::Inclusive, vec![cash(1, 1_000)]);
            let line = build_receipt_model(&sale, &context())
                .unwrap()
                .lines
                .into_iter()
                .next()
                .expect("one line");
            assert_eq!((line.unit_price, line.line_total), (jod(1_000), jod(1_000)));
        }

        /// Where prices exclude tax, the tax rows are added to the total.
        #[test]
        fn prices_excluding_tax_add_their_tax_to_the_total() {
            let mut cart = basket(false);
            cart.lines = vec![item(4, "خدمة اصطناعية", 1_000, 1_000, false, 0)];
            let sale = sold(cart, PriceMode::Exclusive, vec![cash(1, 1_160)]);
            let m = build_receipt_model(&sale, &context()).unwrap();
            assert_eq!(m.totals.subtotal, jod(1_000));
            let tax: Vec<_> = m.totals.tax_summary.iter().map(|r| r.tax.minor()).collect();
            assert_eq!(tax, [160]);
            assert_eq!(m.totals.total, jod(1_160));
            assert_eq!((m.totals.rounding_adjustment, m.change), (None, None));
        }

        /// A card is charged exactly, so no rounding prints, and the masked
        /// number the terminal returned prints with it.
        #[test]
        fn a_card_settles_exactly_and_prints_its_masked_number() {
            let mut cart = basket(false);
            cart.lines = vec![oil(), cheese()];
            let sale = sold(
                cart,
                PriceMode::Inclusive,
                vec![cash(1, 1_000), card(2, 1_109)],
            );
            let m = build_receipt_model(&sale, &context()).unwrap();

            assert_eq!(m.totals.rounding_adjustment, None);
            assert_eq!(m.totals.total, jod(2_109));
            assert_eq!(m.change, None);
            assert_eq!(
                m.tenders.last(),
                Some(&ReceiptTender {
                    label: "بطاقة".to_owned(),
                    amount: jod(1_109),
                    masked_pan: Some("**** **** **** 4242".to_owned()),
                    scheme: Some("visa".to_owned()),
                })
            );
        }

        /// A discount of everything is a sale of nothing (E.18), paid by no
        /// tender, and its receipt still describes what was taken.
        #[test]
        fn a_sale_of_nothing_prints_its_lines_and_no_tender() {
            let mut cart = basket(false);
            let mut free = oil();
            free.discount = Some(LineDiscount {
                kind: DiscountKind::Percent(Percent::from_ppm(1_000_000)),
                reason: reason("damaged"),
                by: UserId::from_uuid(id(4)),
                approver: None,
            });
            cart.lines = vec![free];
            let sale = sold(cart, PriceMode::Inclusive, vec![]);
            let m = build_receipt_model(&sale, &context()).unwrap();

            assert_eq!(m.lines.len(), 1);
            assert_eq!(m.totals.total, jod(0));
            assert!(m.tenders.is_empty());
            assert_eq!((m.totals.rounding_adjustment, m.change), (None, None));
        }

        #[test]
        fn a_business_buyer_is_printed_when_a_tin_was_captured() {
            let with = |tin: Option<&str>, name: Option<&str>| {
                let mut cart = basket(false);
                cart.lines = vec![oil()];
                cart.buyer_tin = tin.map(str::to_owned);
                cart.buyer_name = name.map(str::to_owned);
                build_receipt_model(
                    &sold(cart, PriceMode::Inclusive, vec![cash(1, 1_250)]),
                    &context(),
                )
            };

            assert_eq!(
                with(Some("987654321"), Some(BUYER_NAME)).unwrap().buyer,
                Some(BuyerBlock {
                    name: Some(BUYER_NAME.to_owned()),
                    tin: "987654321".to_owned(),
                })
            );
            // A TIN alone is a buyer, and a name alone is not.
            assert_eq!(
                with(Some("987654321"), None).unwrap().buyer,
                Some(BuyerBlock {
                    name: None,
                    tin: "987654321".to_owned(),
                })
            );
            assert_eq!(with(None, Some(BUYER_NAME)).unwrap().buyer, None);
            // A blank TIN is the validator's refusal, reached through the builder.
            assert_eq!(
                with(Some("  "), Some(BUYER_NAME)),
                Err(ReceiptError::BuyerWithoutTin)
            );
        }

        /// What the receipt cannot print, it names, rather than printing a
        /// code or a blank in its place.
        #[test]
        fn a_receipt_names_what_it_cannot_print() {
            let sale = the_cash_sale();

            for labels in [
                BTreeMap::new(),
                BTreeMap::from([("cash".to_owned(), " ".to_owned())]),
            ] {
                let ctx = ReceiptContext {
                    tender_labels: labels,
                    ..context()
                };
                assert_eq!(
                    build_receipt_model(&sale, &ctx),
                    Err(ReceiptError::TenderUnlabelled {
                        code: "cash".to_owned()
                    })
                );
            }

            let mut cart = basket(false);
            let mut tea = item(3, "شاي اصطناعي", 2_000, 1_000, false, 0);
            tea.discount = Some(LineDiscount {
                kind: DiscountKind::Amount(jod(100)),
                reason: reason("damaged"),
                by: UserId::from_uuid(id(4)),
                approver: None,
            });
            cart.lines = vec![tea];
            let discounted = sold(cart, PriceMode::Inclusive, vec![cash(1, 1_900)]);
            let ctx = ReceiptContext {
                discount_labels: BTreeMap::new(),
                ..context()
            };
            assert_eq!(
                build_receipt_model(&discounted, &ctx),
                Err(ReceiptError::DiscountUnlabelled {
                    code: "damaged".to_owned()
                })
            );
        }

        /// A balance belongs to a customer, so it prints only for a sale with
        /// one attached.
        #[test]
        fn a_loyalty_balance_prints_only_for_an_attached_customer() {
            let ctx = ReceiptContext {
                loyalty: Some(LoyaltyBlock {
                    balance_points: 120,
                }),
                ..context()
            };
            assert_eq!(
                build_receipt_model(&the_cash_sale(), &ctx),
                Err(ReceiptError::LoyaltyWithoutCustomer)
            );

            let mut cart = basket(false);
            cart.lines = vec![oil()];
            cart.customer_id = Some(CustomerId::from_uuid(id(9)));
            let sale = sold(cart, PriceMode::Inclusive, vec![cash(1, 1_250)]);
            assert_eq!(
                build_receipt_model(&sale, &ctx).unwrap().loyalty,
                Some(LoyaltyBlock {
                    balance_points: 120
                })
            );
        }

        #[test]
        fn merchant_phone_never_reaches_the_contexts_debug_output() {
            let rendered = format!("{:?}", context());
            assert!(
                !rendered.contains(MERCHANT_PHONE),
                "the merchant's phone reached Debug output"
            );
        }

        fn price_modes() -> impl Strategy<Value = PriceMode> {
            prop::sample::select(vec![PriceMode::Inclusive, PriceMode::Exclusive])
        }

        fn rounding_rules() -> impl Strategy<Value = RoundingRule> {
            prop::sample::select(vec![
                RoundingRule::HalfAwayFromZero,
                RoundingRule::HalfEven,
                RoundingRule::Floor,
                RoundingRule::Ceil,
            ])
        }

        fn coins() -> impl Strategy<Value = CashRoundingPolicy> {
            (
                prop::sample::select(vec![1_i64, 5, 10, 50, 100]),
                prop::sample::select(vec![
                    RoundingDirection::Nearest,
                    RoundingDirection::Up,
                    RoundingDirection::Down,
                ]),
            )
                .prop_map(|(step_minor, direction)| CashRoundingPolicy {
                    step_minor,
                    direction,
                })
        }

        /// A line's price, its quantity in milli-units (or whole units when
        /// it is not weighed), its tax band and its own discount, in ppm.
        type LinePlan = (i64, i64, bool, u8, Option<i64>);

        fn line_plans() -> impl Strategy<Value = Vec<LinePlan>> {
            prop::collection::vec(
                (
                    1_i64..=50_000,
                    1_i64..=20_000,
                    any::<bool>(),
                    0_u8..=2,
                    prop::option::of(1_i64..=1_000_000),
                ),
                1..=6,
            )
        }

        /// The tenders taken before the last: cash or a card, each a share of
        /// what is still owed, in ppm.
        fn partials() -> impl Strategy<Value = Vec<(bool, i64)>> {
            prop::collection::vec((any::<bool>(), 1_i64..1_000_000), 0..=4)
        }

        /// A cart of `plans`, with a basket discount of `basket_ppm` when one
        /// is drawn.
        fn cart_of(plans: &[LinePlan], basket_ppm: Option<i64>) -> Cart {
            let mut cart = basket(false);
            for (n, &(price, qty, weighed, b, off)) in (1_u128..).zip(plans) {
                let milli = if weighed { qty } else { (qty % 20 + 1) * 1_000 };
                let mut line = item(n, &format!("صنف اصطناعي {n}"), price, milli, weighed, b);
                line.discount = off.map(|ppm| LineDiscount {
                    kind: DiscountKind::Percent(Percent::from_ppm(ppm)),
                    reason: reason("damaged"),
                    by: UserId::from_uuid(id(4)),
                    approver: None,
                });
                cart.lines.push(line);
            }
            cart.basket_discount = basket_ppm.map(|ppm| BasketDiscount {
                kind: DiscountKind::Percent(Percent::from_ppm(ppm)),
                reason: reason("regular"),
                by: UserId::from_uuid(id(4)),
                approver: None,
            });
            cart
        }

        fn sum(amounts: impl Iterator<Item = Money>) -> i128 {
            amounts.map(|a| i128::from(a.minor())).sum()
        }

        proptest! {
            #![proptest_config(domain_proptest_config())]

            /// A receipt adds up on its face (`ref/hardware-and-receipts.md`
            /// §2.3): "a receipt whose visible rows do not add up to its own
            /// printed total is not proof of anything". However a sale is
            /// discounted, taxed, rounded and split across tenders, its lines
            /// come to its subtotal and their discounts to its discount total;
            /// the subtotal less the discounts, plus the tax where prices
            /// exclude it, plus the cash rounding, is its total; and the
            /// tenders less the change come to that same total.
            #[test]
            fn prop_a_receipt_adds_up_to_its_own_total(
                plans in line_plans(),
                basket_ppm in prop::option::of(1_i64..=1_000_000),
                price_mode in price_modes(),
                rule in rounding_rules(),
                coin in coins(),
                before in partials(),
                last_in_cash in any::<bool>(),
                extra in 0_i64..=20_000,
            ) {
                let cart = cart_of(&plans, basket_ppm);
                let mut t = begin_tender(cart, &bands(), &settings(price_mode, rule), coin).unwrap();
                let cash_kind = standard_tender_type("cash").unwrap();
                for (n, &(in_cash, ppm)) in (1_u128..).zip(&before) {
                    let left = remaining_due(&t).unwrap().minor();
                    let part = left * ppm / 1_000_000;
                    if part <= 0 || part >= left {
                        continue;
                    }
                    // Cash short of the remainder is taken exactly; cash at or
                    // past the coin step below it settles the sale with change.
                    t = add_tender(t, if in_cash { cash(n, part) } else { card(n, part) }).unwrap();
                }
                let left = remaining_due(&t).unwrap();
                if !left.is_zero() {
                    let last = if last_in_cash {
                        let asked = final_tender_rounding(&cash_kind, left, coin.step_minor, coin.direction)
                            .unwrap()
                            .map_or(left, |r| r.rounded);
                        cash(99, asked.minor() + extra)
                    } else {
                        card(99, left.minor())
                    };
                    t = add_tender(t, last).unwrap();
                }
                let sale = complete(begin_finalize(t).unwrap(), effects()).unwrap();
                let m = build_receipt_model(&sale, &context()).unwrap();

                let total = i128::from(m.totals.total.minor());
                let subtotal = i128::from(m.totals.subtotal.minor());
                let off = i128::from(m.totals.discount_total.minor());
                prop_assert_eq!(sum(m.lines.iter().map(|l| l.line_total)), subtotal);
                prop_assert_eq!(
                    sum(m.lines.iter().flat_map(|l| l.discounts.iter().map(|d| d.amount))),
                    off
                );
                let tax = match price_mode {
                    PriceMode::Exclusive => sum(m.totals.tax_summary.iter().map(|r| r.tax)),
                    PriceMode::Inclusive => 0,
                };
                let rounding = sum(m.totals.rounding_adjustment.into_iter());
                prop_assert_eq!(subtotal - off + tax + rounding, total);
                let change = sum(m.change.into_iter());
                prop_assert_eq!(sum(m.tenders.iter().map(|t| t.amount)) - change, total);
            }
        }
    }
}
