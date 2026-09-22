//! The receipt raster pipeline (microstep 1.7.3).
//!
//! ```text
//! ReceiptModel → layout (bidi runs, shaping, wrapping, columns)  [layout.rs]
//!              → 1-bit bitmap at the printer's width             [raster.rs]
//!              → GS v 0 raster bytes                             [1.7.4]
//! ```
//!
//! **Do not fight printer codepages.** Windows-1256 text mode neither shapes
//! Arabic letters nor reorders a right-to-left run, so a codepage receipt is
//! unreadable in the one language this product is written for. Rasterising is
//! the field consensus and it is also the only way bilingual mixing looks
//! correct — an Arabic product name beside a Latin SKU beside a Western-digit
//! price is every line of every receipt this register prints.
//!
//! The last arrow belongs to **1.7.4**: this module stops at a [`Bitmap`],
//! which is already in the layout `GS v 0` wants, so the emitter is a header
//! and a copy rather than a re-encode.
//!
//! # Everything here is integer arithmetic
//!
//! `float_arithmetic` is `forbid` in `[workspace.lints.clippy]` and
//! `scripts/check-workspace-lints.py` asserts that exact level, so no code in
//! this pipeline may apply an arithmetic operator to a float — an `#[allow]` is
//! `E0453`, not a warning. Glyph geometry is float maths by nature, so the
//! layout is done in **font units** instead: `harfrust` reports advances in
//! them, and `advance_px = advance_units * px_per_em / units_per_em` is `i32`
//! end to end.
//!
//! That is better than the float version rather than a concession to a lint.
//! 1.7.5 requires `golden_receipts_are_byte_stable`, and an accumulated `f32`
//! pen position is exactly how a golden starts differing between a developer's
//! machine and CI.
//!
//! Exactly one float survives, in [`raster`], and it is documented there.

pub mod layout;
pub mod raster;

use pos_domain::receipt::{ReceiptError, ReceiptModel};

pub use layout::{Alignment, Layout, LayoutGlyph, LayoutLine, TextStyle};
pub use raster::Bitmap;

/// A printer's paper width, which decides every horizontal measurement.
///
/// The pixel counts are the two thermal formats this product supports at
/// 203 dpi (`ref/hardware-and-receipts.md` §2). They are not a preference: a
/// raster wider than the head is truncated by the printer, and a narrower one
/// leaves the receipt visibly off-centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrinterProfile {
    /// 80 mm paper — 576 printable dots.
    Mm80,
    /// 58 mm paper — 384 printable dots.
    Mm58,
}

impl PrinterProfile {
    /// Every profile, once, so a sweep and a test cannot disagree about the
    /// list. Written as a full `match` below rather than a `matches!` so a
    /// third paper width fails to compile until somebody decides its dots.
    pub const ALL: [PrinterProfile; 2] = [PrinterProfile::Mm80, PrinterProfile::Mm58];

    /// Printable dots across.
    #[must_use]
    pub const fn width_px(self) -> u32 {
        match self {
            PrinterProfile::Mm80 => 576,
            PrinterProfile::Mm58 => 384,
        }
    }

    /// The margin held clear on each side, in dots.
    #[must_use]
    pub const fn margin_px(self) -> i32 {
        match self {
            PrinterProfile::Mm80 => 12,
            PrinterProfile::Mm58 => 8,
        }
    }

    /// The width available to content, after both margins.
    #[must_use]
    pub const fn content_width_px(self) -> i32 {
        // `width_px` is 576 or 384 and `margin_px` is 12 or 8, so this cannot
        // overflow or go negative; the profile list is closed and both are
        // `const`.
        self.width_px() as i32 - 2 * self.margin_px()
    }
}

/// What can go wrong between a model and a page.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// The model describes a document that may not exist. Reported before
    /// anything is measured, so a refusal never depends on how far the layout
    /// happened to get.
    #[error("the receipt model is not a valid document: {0}")]
    InvalidModel(#[from] ReceiptError),
    /// The embedded face did not parse. This is a build-time asset, so it can
    /// only mean the bytes in `pos_hardware::font` are not a font.
    #[error("the embedded {0} face could not be parsed")]
    FontUnreadable(&'static str),
    /// The page came out larger than a raster can address.
    #[error("the rendered page is {height_px} dots tall, past the {limit_px} limit")]
    PageTooTall { height_px: u32, limit_px: u32 },
}

/// The body text size, in dots per em, on every paper.
///
/// **The same on both profiles, and that is the point.** A first draft set
/// smaller type on 58 mm so the receipt would keep its shape, and the effect
/// was that a long product name occupied the same number of rows on both
/// papers — the narrow measure and the smaller type cancelled. That is not
/// reflow; it is the same document photographed smaller, and E.49
/// (`ref/hardware-and-receipts.md`) asks for the other thing.
///
/// Neither reference specifies a size per paper, so the per-profile figure was
/// an invention, and removing it makes the narrow paper behave the way its
/// name says: one measure of type, a shorter line, more rows. A future profile
/// that genuinely needs its own size changes this to a method on
/// [`PrinterProfile`]; nothing today does.
///
/// 24 dots at 203 dpi is about 3 mm, which puts roughly 30 Arabic characters on
/// a 58 mm line — the density thermal receipts are actually set at.
pub const BODY_PX_PER_EM: i32 = 24;

/// The tallest page this pipeline will produce.
///
/// A receipt is continuous paper, so nothing physical stops one growing — but a
/// 20 000-line document is a runaway loop rather than a sale, and a rasteriser
/// that allocates for it before failing is a register that dies instead of
/// refusing. Generous enough for a hundred-line basket at either profile.
pub const MAX_PAGE_HEIGHT_PX: u32 = 32_768;

/// Turn a receipt into the dots a thermal head prints.
///
/// # Errors
///
/// Refuses an invalid model before laying anything out, an unreadable embedded
/// face, and a page past [`MAX_PAGE_HEIGHT_PX`].
pub fn render_receipt(
    model: &ReceiptModel,
    profile: PrinterProfile,
) -> Result<Bitmap, RenderError> {
    // 1.7.1 left `ReceiptModel`'s fields public, as `ref/domain-api.md` §13
    // specifies, so nothing forces this call except this line and
    // `an_invalid_model_is_refused_before_anything_is_laid_out`. It is first on
    // purpose: an acknowledgement carrying a fiscal QR must be refused as a
    // document, not discovered as a missing glyph.
    model.validate()?;

    // Parsed once and handed to both halves: measuring against one parse and
    // drawing against another is two answers to "how wide is this glyph".
    let faces = layout::Faces::load()?;
    let page = layout::lay_out_with(&faces, model, profile)?;
    raster::rasterise_with(&faces, &page, profile)
}

/// The receipt every test in this module renders.
///
/// One fixture, shared by `layout` and `raster`, so the two halves are proved
/// against the same document and a change to it cannot quietly make one of them
/// vacuous. It is a complete §2.3 anatomy — merchant, buyer, header, a line
/// with an allowance, a tax summary, a tender, a fiscal block and a footer —
/// because a fixture with empty optionals tests the fields it populates, which
/// is the lesson 1.7.1's serialisation test learned from its own mutation
/// sweep.
#[cfg(test)]
pub(crate) mod tests_support {
    // A `#[cfg(test)]` module is still ordinary code to clippy: the workspace
    // denies on `expect` apply here as they do not inside a `mod tests`. The
    // allowance is the same one every test module takes, written once for the
    // fixture the three of them share.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use pos_domain::catalog::UnitOfMeasure;
    use pos_domain::ids::{RegisterId, SaleId};
    use pos_domain::money::{Currency, Money, Percent, Qty};
    use pos_domain::receipt::{
        BuyerBlock, DocKind, FiscalBlock, FooterBlock, Language, LoyaltyBlock, MerchantBlock,
        ReceiptDiscount, ReceiptHeader, ReceiptLine, ReceiptLocale, ReceiptModel, ReceiptTender,
        ReceiptTotals,
    };
    use pos_domain::tax::{TaxSummaryRow, TaxTreatment};
    use pos_domain::time::{BusinessDate, Timestamp};
    use uuid::Uuid;

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, Currency::JOD)
    }

    /// The smallest document a property needs.
    ///
    /// Laying out the full fixture costs about 180 ms in a debug build —
    /// shaping is the whole of it — so 256 cases of it is 45 seconds against
    /// conventions §5.1's three-minute budget for the *entire* suite. The
    /// placement rules a property checks do not care how many rows there are,
    /// so the properties render this and the example tests render [`model`].
    /// Measured, not assumed: the first draft used the full fixture and one
    /// property alone took 116 seconds.
    #[must_use]
    pub(crate) fn minimal() -> ReceiptModel {
        let mut m = model();
        m.merchant.store_name = String::new();
        m.merchant.address = None;
        m.merchant.phone = None;
        m.merchant.tin = None;
        m.buyer = None;
        // Empty strings short-circuit in the shaper, so the fixed rows of the
        // document cost nothing and the only text a case actually shapes is the
        // one the generator varied. That is the whole point of this fixture.
        m.header.receipt_no = String::new();
        m.header.register_code = String::new();
        m.header.cashier_name = String::new();
        m.totals.tax_summary.clear();
        m.totals.rounding_adjustment = None;
        m.tenders.clear();
        m.change = None;
        m.loyalty = None;
        m.fiscal = None;
        m.footer = FooterBlock {
            return_policy: None,
            thank_you: None,
        };
        if let Some(line) = m.lines.first_mut() {
            line.discounts.clear();
        }
        m
    }

    #[must_use]
    pub(crate) fn model() -> ReceiptModel {
        ReceiptModel {
            doc_kind: DocKind::Sale,
            watermark: None,
            merchant: MerchantBlock {
                legal_name: "مؤسسة النور التجارية".to_owned(),
                store_name: "النور".to_owned(),
                address: Some("عمان، الأردن".to_owned()),
                phone: Some("+962 79 000 0000".to_owned()),
                tin: Some("123456789".to_owned()),
            },
            buyer: Some(BuyerBlock {
                name: Some("شركة الأمانة للتجارة".to_owned()),
                tin: "987654321".to_owned(),
            }),
            header: ReceiptHeader {
                sale_id: SaleId::from_uuid(Uuid::from_u128(1)),
                receipt_no: "REG01-000042".to_owned(),
                register_id: RegisterId::from_uuid(Uuid::from_u128(2)),
                register_code: "REG01".to_owned(),
                cashier_name: "ليلى".to_owned(),
                issued_at: Timestamp::from_epoch_milliseconds(1_758_000_000_000)
                    .expect("a valid instant"),
                business_date: BusinessDate::new(2026, 9, 22).expect("a real date"),
            },
            lines: vec![ReceiptLine {
                name: "خبز عربي طازج".to_owned(),
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
                rounding_adjustment: Some(jod(-3)),
                total: jod(2_163),
            },
            tenders: vec![ReceiptTender {
                label: "نقدًا".to_owned(),
                amount: jod(2_163),
                masked_pan: None,
                scheme: None,
            }],
            change: Some(jod(0)),
            loyalty: Some(LoyaltyBlock {
                balance_points: 120,
            }),
            fiscal: Some(FiscalBlock {
                uuid: "9f1c0f7a-0000-4000-8000-000000000000".to_owned(),
                qr_payload: "AQIDBAU=".to_owned(),
            }),
            footer: FooterBlock {
                return_policy: Some("الإرجاع خلال أربعة عشر يومًا".to_owned()),
                thank_you: Some("شكرًا لتسوقكم".to_owned()),
            },
            locale: ReceiptLocale::new(Language::Arabic),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn every_profile_has_a_width_a_margin_and_a_type_size() {
        for profile in PrinterProfile::ALL {
            assert!(profile.width_px() > 0);
            assert!(profile.margin_px() > 0);
            // Content must fit inside the paper with both margins to spare.
            assert!(profile.content_width_px() > 0);
            assert!(
                profile.content_width_px() < profile.width_px() as i32,
                "{profile:?} content is not inside its paper"
            );
        }
        assert_eq!(PrinterProfile::ALL.len(), 2);
    }

    /// One type size, both papers — see [`BODY_PX_PER_EM`]. The narrow profile
    /// reflows because its *measure* is shorter, which is what E.49 asks for;
    /// a per-profile size cancelled the effect and was removed.
    #[test]
    fn both_papers_are_set_in_the_same_type() {
        // A 58 mm line still holds a useful number of ems; below about ten the
        // receipt stops being a document and becomes a column.
        for profile in PrinterProfile::ALL {
            let ems = profile.content_width_px() / BODY_PX_PER_EM;
            assert!(ems >= 10, "{profile:?} fits only {ems} ems per line");
        }
    }

    /// The two widths are the printable dot counts at 203 dpi, and they are the
    /// numbers `ref/hardware-and-receipts.md` §2 names. A test rather than a
    /// comment because a wrong width is invisible until paper comes out of a
    /// machine nobody has bought yet (#68).
    #[test]
    fn the_paper_widths_are_the_specified_dot_counts() {
        assert_eq!(PrinterProfile::Mm80.width_px(), 576);
        assert_eq!(PrinterProfile::Mm58.width_px(), 384);
    }
}
