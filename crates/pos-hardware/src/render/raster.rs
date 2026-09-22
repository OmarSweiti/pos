//! Turning a [`Layout`] into the dots a thermal head prints (microstep 1.7.3).
//!
//! The output is **1 bit per pixel, MSB first, row-major** — one byte per eight
//! horizontal dots, the leftmost dot in the high bit. That is not a choice:
//! it is exactly the layout `GS v 0` takes, so 1.7.4's emitter is a header and
//! a copy rather than a re-encode, and a bug there cannot be a transposition
//! bug here.
//!
//! # The one float in the pipeline
//!
//! Everything in [`super::layout`] is `i32`, because `float_arithmetic` is
//! `forbid` workspace-wide. One float is unavoidable here: `tiny-skia` fills a
//! path under an `f32` [`Transform`], and the transform that takes a glyph
//! outline from font units to dots is `px_per_em / units_per_em`.
//!
//! It is produced by dividing two [`Decimal`]s and converting at the boundary.
//! `Decimal / Decimal` is not float arithmetic, `rust_decimal` is already in
//! this workspace for exactly this shape of problem — exact intermediate
//! arithmetic, one conversion at the edge — and it therefore adds no
//! supply-chain surface. One division, in one function, and every other number
//! on the page is an integer that was decided before this module ran.
//!
//! Note what this is *not*: it is not a way around the lint. The lint exists so
//! no float touches money (I-1), and nothing here is money — every amount
//! arrived as a string that [`pos_domain::money::Money::format_exact`] had
//! already rendered.

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{GlyphId, MetadataProvider};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

use super::layout::{Faces, Layout};
use super::{PrinterProfile, RenderError};

/// A 1-bit page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub width_px: u32,
    pub height_px: u32,
    /// `ceil(width_px / 8) * height_px` bytes, MSB first, row-major.
    pub bits: Vec<u8>,
}

/// Bytes needed for one row of `width_px` dots.
///
/// One definition, used by [`Bitmap::row_bytes`] and by [`threshold`] which
/// allocates. The mutation sweep is why: with the stride computed twice, an
/// `/ 8` where a `div_ceil(8)` belonged survived every test, because **both**
/// shipped paper widths are exact multiples of eight and no rendered page can
/// tell the two apart. A second definition of a number is a second chance to
/// get it wrong in a place nothing checks.
const fn row_bytes_for(width_px: u32) -> u32 {
    width_px.div_ceil(8)
}

impl Bitmap {
    /// Bytes per row, which `GS v 0` calls `xL`/`xH`.
    #[must_use]
    pub const fn row_bytes(&self) -> u32 {
        row_bytes_for(self.width_px)
    }

    /// Whether the dot at `(x, y)` is set. Out of bounds is unset rather than a
    /// panic: a caller inspecting a page it did not lay out is diagnosing
    /// something, and dying mid-diagnosis helps nobody.
    #[must_use]
    pub fn dot(&self, x: u32, y: u32) -> bool {
        if x >= self.width_px || y >= self.height_px {
            return false;
        }
        let index = (y * self.row_bytes() + x / 8) as usize;
        let Some(byte) = self.bits.get(index) else {
            return false;
        };
        // MSB first: dot 0 of a byte is bit 7.
        byte & (0x80 >> (x % 8)) != 0
    }

    /// How many dots are set. The cheapest honest assertion about a page that
    /// is supposed to have something on it.
    #[must_use]
    pub fn ink(&self) -> u32 {
        self.bits.iter().map(|b| b.count_ones()).sum()
    }
}

/// Collects a glyph outline into a path, in font units.
///
/// The coordinates are passed through untouched — no arithmetic, which is both
/// the lint's requirement and the accurate thing to do: scaling belongs in the
/// one [`Transform`] below, where it happens once per glyph instead of once per
/// control point.
struct Outline {
    builder: PathBuilder,
}

impl OutlinePen for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.builder.close();
    }
}

/// The scale from font units to dots, as `tiny-skia` needs it.
///
/// See the module documentation for why this is the one float and why it comes
/// from `Decimal`.
///
/// The vertical scale is obtained by passing a **negated `px_per_em`**, not by
/// negating the result. `-scale` on an `f32` is unary float arithmetic and
/// `float_arithmetic` is `forbid`, so it is `E0453` rather than a warning — the
/// lint caught this exact line during development, which is the control working
/// as `Cargo.toml` describes it. Negating the integer before the division is
/// the same number by a route that involves no float operator, and it is also
/// the clearer one: the flip is a property of the coordinate system, decided
/// with the other integers, rather than a sign applied to a scale factor.
fn dots_per_unit(px_per_em: i32, units_per_em: i32) -> Option<f32> {
    if units_per_em == 0 {
        return None;
    }
    (Decimal::from(px_per_em) / Decimal::from(units_per_em)).to_f32()
}

/// Rasterise a laid-out page.
///
/// # Errors
///
/// Refuses an unreadable embedded face and a page too tall to allocate.
pub fn rasterise(layout: &Layout, profile: PrinterProfile) -> Result<Bitmap, RenderError> {
    let faces = Faces::load()?;
    rasterise_with(&faces, layout, profile)
}

/// [`rasterise`], against faces the caller already parsed.
///
/// # Errors
///
/// Refuses a page past [`super::MAX_PAGE_HEIGHT_PX`].
pub(crate) fn rasterise_with(
    faces: &Faces<'_>,
    layout: &Layout,
    profile: PrinterProfile,
) -> Result<Bitmap, RenderError> {
    let units_per_em = faces.units_per_em;

    if layout.height_px > super::MAX_PAGE_HEIGHT_PX {
        return Err(RenderError::PageTooTall {
            height_px: layout.height_px,
            limit_px: super::MAX_PAGE_HEIGHT_PX,
        });
    }

    // A zero-dimension page is a valid answer for an empty layout, and
    // `Pixmap::new` refuses one — so it is answered here rather than by an
    // error nobody can act on.
    let Some(mut pixmap) = Pixmap::new(layout.width_px, layout.height_px.max(1)) else {
        return Err(RenderError::PageTooTall {
            height_px: layout.height_px,
            limit_px: super::MAX_PAGE_HEIGHT_PX,
        });
    };

    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    // Antialiased then thresholded, rather than aliased. A thermal head is one
    // bit deep either way; the difference is that an antialiased outline picks
    // the dot whose coverage is greatest, which is what keeps an Arabic
    // letter's thin joining stroke from disappearing at 20 px/em.
    paint.anti_alias = true;

    for line in &layout.lines {
        for glyph in &line.glyphs {
            let face = faces.face(glyph.style.weight);
            let Some(scale_x) = dots_per_unit(glyph.style.px_per_em, units_per_em) else {
                continue;
            };
            // Font units are y-up from the baseline; a raster is y-down from
            // the top. The flip is the negated em size, not a negated scale.
            let Some(scale_y) = dots_per_unit(-glyph.style.px_per_em, units_per_em) else {
                continue;
            };

            let mut outline = Outline {
                builder: PathBuilder::new(),
            };
            // A glyph with no outline is a space, and every receipt is full of
            // them. Not an error, and not worth a branch that says so.
            let Some(drawing) = face
                .outline_glyphs()
                .get(GlyphId::new(u32::from(glyph.glyph_id)))
            else {
                continue;
            };
            if drawing
                .draw(
                    DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                    &mut outline,
                )
                .is_err()
            {
                continue;
            }
            let Some(path) = outline.builder.finish() else {
                continue;
            };

            let transform = Transform::from_row(
                scale_x,
                0.0,
                0.0,
                scale_y,
                pixel(glyph.x_px),
                pixel(glyph.baseline_y_px),
            );
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
    }

    for y in &layout.rules {
        draw_rule(&mut pixmap, *y, profile);
    }

    Ok(threshold(&pixmap, layout.width_px, layout.height_px))
}

/// An `i32` dot coordinate as `tiny-skia` wants it.
///
/// A cast, not arithmetic. Every page coordinate was decided in integers by
/// [`super::layout`]; this is the boundary where they become the floats the
/// rasteriser works in.
#[expect(
    clippy::cast_precision_loss,
    reason = "a page coordinate is bounded by MAX_PAGE_HEIGHT_PX, far inside f32's exact integer range"
)]
const fn pixel(value: i32) -> f32 {
    value as f32
}

/// A horizontal separator, drawn directly rather than shaped.
///
/// §2.3 prints a rule between the header, the lines, the totals and the
/// tenders. Setting one as a row of box-drawing characters would depend on the
/// font having them and on their advance dividing the width exactly; a filled
/// row of dots depends on neither, and is what a thermal printer draws anyway.
fn draw_rule(pixmap: &mut Pixmap, y_px: i32, profile: PrinterProfile) {
    let Ok(y) = u32::try_from(y_px) else {
        return;
    };
    if y >= pixmap.height() {
        return;
    }
    let Ok(margin) = u32::try_from(profile.margin_px()) else {
        return;
    };
    let width = pixmap.width();
    let end = width.saturating_sub(margin);

    let pixels = pixmap.pixels_mut();
    for x in margin..end {
        let index = (y * width + x) as usize;
        if let Some(pixel) = pixels.get_mut(index) {
            // `PremultipliedColorU8` has no const black, and `from_rgba`
            // returns `None` only for a non-premultiplied value; opaque black
            // always is one.
            if let Some(black) = tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, 255) {
                *pixel = black;
            }
        }
    }
}

/// Coverage to dots.
///
/// The threshold is the midpoint: a dot whose glyph covers more than half of it
/// is printed. Anything cleverer — dithering, a different cutoff per profile —
/// is a decision about how paper looks that belongs with the goldens a native
/// reader signs off in 1.7.5, not in the first version of the rasteriser.
fn threshold(pixmap: &Pixmap, width_px: u32, height_px: u32) -> Bitmap {
    let row_bytes = row_bytes_for(width_px);
    let mut bits = vec![0u8; (row_bytes * height_px) as usize];
    let pixels = pixmap.pixels();

    for y in 0..height_px {
        for x in 0..width_px {
            let Some(pixel) = pixels.get((y * pixmap.width() + x) as usize) else {
                continue;
            };
            if pixel.alpha() <= 127 {
                continue;
            }
            let index = (y * row_bytes + x / 8) as usize;
            if let Some(byte) = bits.get_mut(index) {
                *byte |= 0x80 >> (x % 8);
            }
        }
    }

    Bitmap {
        width_px,
        height_px,
        bits,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::render::layout::lay_out;
    use crate::render::tests_support::{minimal, model};
    use crate::render::{MAX_PAGE_HEIGHT_PX, render_receipt};
    use pos_domain::receipt::{DocKind, FiscalBlock};
    use pos_test_support::io_proptest_config;
    use proptest::prelude::*;

    /// The named test: a page is exactly as wide as the paper it is for, at
    /// both profiles, and there are dots on it.
    #[test]
    fn raster_width_matches_profile() {
        for profile in PrinterProfile::ALL {
            let page = render_receipt(&model(), profile).expect("renders");

            assert_eq!(page.width_px, profile.width_px());
            assert_eq!(page.row_bytes(), profile.width_px().div_ceil(8));
            assert_eq!(
                page.bits.len() as u32,
                page.row_bytes() * page.height_px,
                "{profile:?} allocated the wrong number of bytes"
            );
            assert!(page.ink() > 0, "{profile:?} produced a blank page");
        }
    }

    /// The named test (E.49): the narrow profile **reflows** rather than losing
    /// the end of every line.
    ///
    /// The measurement has to be made where reflow actually happens. An earlier
    /// draft compared the two pages' heights on the plain fixture and asserted
    /// the 58 mm page was taller; it is *shorter* (811 dots against 986), and
    /// correctly so — the narrow profile sets 20 px/em against 24, so a receipt
    /// whose lines all fit at both widths simply comes out smaller. The test was
    /// measuring type size and calling it reflow.
    ///
    /// So: a product name that does not fit on either paper, and the assertion
    /// that the **narrower one takes more rows to say the same thing**. A
    /// truncating renderer produces the same number of rows on both, and a
    /// renderer that lets text overhang produces dots past the paper edge — the
    /// second half of this test rules that out.
    #[test]
    fn narrow_profile_reflows_rather_than_truncates() {
        const LONG: &str =
            "خبز عربي طازج مخبوز على الحجر مع بذور السمسم والحبة السوداء من مخبز الحي القديم";

        let mut m = model();
        if let Some(line) = m.lines.first_mut() {
            line.name = LONG.to_owned();
        }

        // Rows the long name *added*, per paper, rather than rows in the whole
        // receipt. The second draft of this test compared whole-receipt heights
        // and read 31 against 31: the totals block's own wrapping moves the
        // other way on narrow paper and cancelled the effect exactly. Isolating
        // the variable is what makes the measurement mean what its name says.
        let added = |profile: PrinterProfile| -> usize {
            let long = lay_out(&m, profile).expect("lays out").lines.len();
            let short = lay_out(&model(), profile).expect("lays out").lines.len();
            long.saturating_sub(short)
        };

        let wide_rows = added(PrinterProfile::Mm80);
        let narrow_rows = added(PrinterProfile::Mm58);
        assert!(
            narrow_rows > wide_rows,
            "the long name took {narrow_rows} extra rows at 58 mm and {wide_rows} at 80 mm — it did not reflow"
        );

        let wide_page = render_receipt(&m, PrinterProfile::Mm80).expect("renders 80mm");
        let narrow_page = render_receipt(&m, PrinterProfile::Mm58).expect("renders 58mm");
        assert_eq!(wide_page.width_px, 576);
        assert_eq!(narrow_page.width_px, 384);

        // Ink scales with type size, not with how much text survived. The
        // narrow profile sets 20 px/em against 24, so its glyphs cover roughly
        // (20/24)² of the area — call it half, generously, and anything below
        // that is text that went missing rather than text that got smaller.
        let floor = wide_page.ink() / 2;
        assert!(
            narrow_page.ink() > floor,
            "58 mm has {} ink against 80 mm's {}; below {floor} means content was dropped",
            narrow_page.ink(),
            wide_page.ink()
        );

        // And nothing overhangs the narrower paper.
        for y in 0..narrow_page.height_px {
            for x in narrow_page.width_px..narrow_page.row_bytes() * 8 {
                assert!(!narrow_page.dot(x, y), "a dot at {x},{y} is past the paper");
            }
        }
    }

    /// The bit layout is `GS v 0`'s, so 1.7.4 is a header and a copy.
    #[test]
    fn the_bitmap_is_one_bit_per_pixel_msb_first() {
        let mut page = Bitmap {
            width_px: 16,
            height_px: 2,
            bits: vec![0; 4],
        };
        assert_eq!(page.row_bytes(), 2);

        // Set the leftmost dot of row 0 and the rightmost of row 1 by hand, and
        // read them back through `dot`.
        if let Some(byte) = page.bits.first_mut() {
            *byte = 0x80;
        }
        if let Some(byte) = page.bits.get_mut(3) {
            *byte = 0x01;
        }

        assert!(page.dot(0, 0), "bit 7 of byte 0 is dot 0");
        assert!(!page.dot(1, 0));
        assert!(page.dot(15, 1), "bit 0 of the last byte is the last dot");
        assert!(!page.dot(14, 1));
        assert_eq!(page.ink(), 2);

        // Out of bounds reads unset rather than panicking.
        assert!(!page.dot(16, 0));
        assert!(!page.dot(0, 2));
    }

    /// A row width that is not a multiple of eight still allocates whole bytes,
    /// and the padding dots stay clear.
    #[test]
    fn a_ragged_width_pads_its_last_byte_with_clear_dots() {
        let page = Bitmap {
            width_px: 12,
            height_px: 1,
            bits: vec![0xFF, 0xF0],
        };
        assert_eq!(page.row_bytes(), 2);
        for x in 0..12 {
            assert!(page.dot(x, 0), "dot {x} should be set");
        }
        for x in 12..16 {
            assert!(
                !page.dot(x, 0),
                "dot {x} is past the page and must be clear"
            );
        }
    }

    /// The entry point refuses a document that may not exist, before it
    /// measures anything.
    ///
    /// 1.7.1 deliberately left `ReceiptModel`'s fields public, so nothing
    /// forces `validate` except the call in `render_receipt` and this test.
    /// It is the whole of that microstep's inherited obligation.
    #[test]
    fn an_invalid_model_is_refused_before_anything_is_laid_out() {
        let mut m = model();
        m.doc_kind = DocKind::Acknowledgement;
        m.fiscal = Some(FiscalBlock {
            uuid: "9f1c0f7a-0000-4000-8000-000000000000".to_owned(),
            qr_payload: "AQIDBAU=".to_owned(),
        });

        let refusal = render_receipt(&m, PrinterProfile::Mm80);
        assert!(
            matches!(refusal, Err(RenderError::InvalidModel(_))),
            "a non-fiscal acknowledgement carrying a fiscal block was rendered: {refusal:?}"
        );

        // The same document without the block renders, so the refusal is the
        // rule and not the fixture.
        m.fiscal = None;
        assert!(render_receipt(&m, PrinterProfile::Mm80).is_ok());
    }

    #[test]
    fn the_page_height_limit_is_a_refusal_rather_than_an_allocation() {
        let layout = super::super::layout::Layout {
            width_px: 576,
            height_px: MAX_PAGE_HEIGHT_PX + 1,
            lines: Vec::new(),
            rules: Vec::new(),
            direction: pos_domain::receipt::TextDirection::RightToLeft,
        };
        assert!(matches!(
            rasterise(&layout, PrinterProfile::Mm80),
            Err(RenderError::PageTooTall { .. })
        ));
    }

    /// The scale is the one float, and it is exact for the sizes this pipeline
    /// uses. A corrupt face divides by zero without dividing by zero.
    #[test]
    fn the_only_float_is_the_scale_and_it_is_exact() {
        assert_eq!(dots_per_unit(24, 1_000), Some(0.024));
        assert_eq!(dots_per_unit(-24, 1_000), Some(-0.024));
        assert_eq!(dots_per_unit(20, 1_000), Some(0.02));
        assert_eq!(dots_per_unit(24, 0), None);
    }

    // `io_proptest_config` — 256 cases, not `domain_proptest_config`'s 4 096.
    //
    // Conventions §5.1 sets the lower default for crates whose properties are
    // expensive per case, on the argument that *"a slow database property that
    // nobody runs protects nothing"*. Rendering is not I/O, but it is the same
    // economics: every case here shapes a whole receipt and rasterises it, and
    // at the domain count these two properties alone took the suite past the
    // three-minute budget §5.1 sets for `just test`. Measured, not guessed —
    // the first draft used the domain config and did not finish.
    /// Glyphs are actually drawn, and the sweep is why this is not
    /// `assert!(ink() > 0)`.
    ///
    /// Deleting the `fill_path` call — drawing **no glyphs at all** — survived
    /// every test in the first sweep, because the horizontal separators are
    /// drawn separately and kept the page from being blank. A page with more
    /// text must carry more ink than the same page with less; rules are
    /// identical in both, so the difference is glyphs and nothing else.
    #[test]
    fn a_longer_receipt_carries_more_ink_than_a_shorter_one() {
        let short = render_receipt(&minimal(), PrinterProfile::Mm80).expect("renders");

        let mut long_model = minimal();
        if let Some(line) = long_model.lines.first_mut() {
            line.name = "خبز عربي طازج مخبوز على الحجر مع بذور السمسم والحبة السوداء".to_owned();
        }
        let long = render_receipt(&long_model, PrinterProfile::Mm80).expect("renders");

        assert!(
            long.ink() > short.ink(),
            "more text produced no more ink: {} against {}",
            long.ink(),
            short.ink()
        );
    }

    /// Letters sit **above** their baseline, which is the vertical flip.
    ///
    /// Font outlines are y-up from the baseline and a raster is y-down from the
    /// top, so the transform carries a negated scale. Dropping it draws every
    /// glyph mirrored below its own baseline.
    ///
    /// Asserting merely that *some* ink lands above the baseline is not enough,
    /// and the sweep proved it: Arabic is full of strokes that descend, so a
    /// mirrored page still puts ink above the line. What separates the two is
    /// the **balance** — measured, not guessed: 1 234 dots above against 327
    /// below when the flip is right, and 148 against 1 340 when it is not.
    #[test]
    fn glyphs_sit_above_their_baseline() {
        use crate::render::layout::lay_out;

        let m = minimal();
        let profile = PrinterProfile::Mm80;
        let layout = lay_out(&m, profile).expect("lays out");
        let page = render_receipt(&m, profile).expect("renders");

        let mut baselines = layout.lines.iter().map(|line| line.baseline_y_px);
        let first = baselines.next().expect("the receipt has a first line");
        let second = baselines.next().expect("the receipt has a second line");
        let first = u32::try_from(first).expect("a baseline on the page");
        let second = u32::try_from(second).expect("a baseline on the page");

        let ink_in = |rows: core::ops::Range<u32>| -> u32 {
            rows.map(|y| {
                u32::try_from((0..page.width_px).filter(|x| page.dot(*x, y)).count())
                    .unwrap_or(u32::MAX)
            })
            .sum()
        };

        let above = ink_in(0..first);
        let below = ink_in(first..second);

        assert!(above > 0, "the first line put no ink above its baseline");
        assert!(
            above > below,
            "more ink below the first baseline ({below}) than above it ({above}) — the page is mirrored"
        );
    }

    /// A separator runs between the margins, and the bit order is MSB first —
    /// both proved on a page that was actually rendered.
    ///
    /// Two sweep survivors met here. Drawing rules edge to edge passed
    /// everything, because no test looked at where a rule starts. And writing
    /// the bitmap **LSB first** passed too: `the_bitmap_is_one_bit_per_pixel_msb_first`
    /// builds a `Bitmap` by hand and reads it back, so writer and reader were
    /// never compared. A rule is the one feature whose exact extent is known in
    /// advance, which makes it the thing that can tell the two apart: mirror
    /// the bits inside a byte and the margin stops being clear.
    #[test]
    fn a_rule_runs_between_the_margins_and_reads_back_msb_first() {
        use crate::render::layout::lay_out;

        for profile in PrinterProfile::ALL {
            let m = model();
            let layout = lay_out(&m, profile).expect("lays out");
            let page = render_receipt(&m, profile).expect("renders");

            let y = layout
                .rules
                .first()
                .copied()
                .expect("the anatomy has rules");
            let y = u32::try_from(y).expect("a rule on the page");
            let margin = u32::try_from(profile.margin_px()).expect("a margin");

            for x in 0..margin {
                assert!(
                    !page.dot(x, y),
                    "{profile:?}: the rule reaches x={x}, inside the {margin}-dot margin"
                );
                assert!(
                    !page.dot(page.width_px - 1 - x, y),
                    "{profile:?}: the rule reaches the far margin"
                );
            }
            assert!(
                page.dot(margin, y),
                "{profile:?}: the rule does not start at the margin"
            );
            assert!(
                page.dot(page.width_px - margin - 1, y),
                "{profile:?}: the rule does not reach the far margin"
            );
        }
    }

    // Both properties measure the **layout**, because that is where placement
    // is decided; rasterising is a pure function of a layout, and
    // `a_page_renders_to_the_same_bytes_twice` covers the dots.
    //
    // Their cost is worth recording, because it moved twice and the second move
    // was not an optimisation anybody wrote. Against `render_receipt` on the
    // full fixture they took **186 seconds** at 256 cases — conventions §5.1
    // budgets three minutes for the whole of `just test` — and measuring the
    // layout of `minimal()` instead brought that to 31. Then the shaper changed
    // from `rustybuzz` to `harfrust` for an unrelated reason, an unmaintained
    // advisory, and the same properties came back in **three seconds**. The
    // budget was never really the fixture; it was the shaper.
    proptest! {
        #![proptest_config(io_proptest_config())]

        /// The same model lays out identically, every time.
        ///
        /// The layout half of determinism, varied. 1.7.5's byte-stable goldens
        /// need rendering to be a function of its input and nothing else, and
        /// the integer pen positions are what make that true across machines.
        #[test]
        fn prop_rendering_is_deterministic(
            name in "[\\u0600-\\u06FFa-zA-Z0-9][\\u0600-\\u06FF a-zA-Z0-9]{0,48}",
            profile_index in 0usize..2,
        ) {
            let profile = PrinterProfile::ALL
                .get(profile_index)
                .copied()
                .unwrap_or(PrinterProfile::Mm80);

            let mut m = minimal();
            if let Some(line) = m.lines.first_mut() {
                line.name = name;
            }

            prop_assert_eq!(lay_out(&m, profile)?, lay_out(&m, profile)?);
        }

        /// Nothing is placed outside the paper, whatever the receipt says.
        ///
        /// The generator varies the fields that decide how wide a line wants to
        /// be, over Arabic, Latin, digits and spaces, past the width of either
        /// paper. A renderer that let a long word run off the edge would place
        /// glyphs the head cannot print, and the customer would get a receipt
        /// with the end of every line missing.
        #[test]
        fn prop_no_glyph_is_placed_outside_the_page(
            // A leading non-space, because 1.7.1 refuses a line that describes
            // nothing and a blank name is that rule firing rather than a defect
            // here.
            name in "[\\u0600-\\u06FFa-zA-Z0-9][\\u0600-\\u06FF a-zA-Z0-9]{0,48}",
            profile_index in 0usize..2,
        ) {
            let profile = PrinterProfile::ALL
                .get(profile_index)
                .copied()
                .unwrap_or(PrinterProfile::Mm80);

            let mut m = minimal();
            if let Some(line) = m.lines.first_mut() {
                line.name = name.clone();
            }
            m.merchant.legal_name = name;

            let layout = lay_out(&m, profile)?;
            prop_assert_eq!(layout.width_px, profile.width_px());

            let width = i32::try_from(layout.width_px).unwrap_or(i32::MAX);
            for line in &layout.lines {
                for glyph in &line.glyphs {
                    prop_assert!(glyph.x_px >= 0, "a glyph at x={}", glyph.x_px);
                    prop_assert!(
                        glyph.x_px < width,
                        "a glyph at x={} on a {}-dot page",
                        glyph.x_px,
                        width
                    );
                    prop_assert!(glyph.baseline_y_px >= 0);
                    prop_assert!(
                        glyph.baseline_y_px < i32::try_from(layout.height_px).unwrap_or(i32::MAX),
                        "a glyph below the page"
                    );
                }
            }
        }

    }

    /// Rendering is a function: the same model gives the same **dots**.
    ///
    /// This is 1.7.5's precondition — `golden_receipts_are_byte_stable` can
    /// only hold if it is true — and it is what the integer layout buys,
    /// because an `f32` pen position accumulated down a line is reproducible on
    /// one machine and not necessarily on the next.
    ///
    /// Six representative strings rather than a property, because this one runs
    /// the whole pipeline including the rasteriser, and
    /// `prop_rendering_is_deterministic` already varies the input at the layout
    /// where placement is decided.
    #[test]
    fn a_page_renders_to_the_same_bytes_twice() {
        const SUBJECTS: [&str; 6] = [
            "خبز عربي",                                     // Arabic, joined
            "بَيْع",                                          // Arabic with marks
            "Cola 330ml",                                   // Latin and digits
            "حليب Almarai ١ لتر",                           // mixed scripts and both numeral sets
            "خبز عربي طازج مخبوز على الحجر مع بذور السمسم", // long enough to wrap
            "9f1c0f7a-0000-4000-8000-000000000000",         // unbreakable, wider than 58 mm
        ];

        for profile in PrinterProfile::ALL {
            for subject in SUBJECTS {
                let mut m = model();
                if let Some(line) = m.lines.first_mut() {
                    line.name = subject.to_owned();
                }

                let first = render_receipt(&m, profile).expect("renders");
                let second = render_receipt(&m, profile).expect("renders");
                assert_eq!(
                    first, second,
                    "{profile:?} rendered {subject:?} differently twice"
                );
                assert!(first.ink() > 0, "{profile:?}/{subject:?} came out blank");
            }
        }
    }
}
