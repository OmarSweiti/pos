//! Turning a [`ReceiptModel`] into positioned glyphs (microstep 1.7.3).
//!
//! Three jobs, in this order, because each needs the one before it:
//!
//! 1. **Bidi.** A receipt line is *"خبز عربي"* beside `2 × 0.400` beside
//!    `0.800`. The Unicode Bidirectional Algorithm decides which parts of that
//!    run right-to-left and in what visual order the parts sit;
//!    [`unicode_bidi`] is that algorithm and a shaper deliberately is not —
//!    a shaper works on one direction at a time.
//! 2. **Shaping.** Each directional run goes to `harfrust`, which selects the
//!    contextual Arabic forms — initial, medial, final — and returns advances
//!    in **font units**.
//! 3. **Wrapping and alignment.** Greedy word wrap against the profile's
//!    content width, then the line is placed from the logical start or end,
//!    which is the right edge in Arabic and the left in English.
//!
//! # Font units, not pixels, until the last moment
//!
//! Every measurement here is an `i32` in font units, and a pen position becomes
//! a pixel exactly once: `x_px = pen_units * px_per_em / units_per_em`.
//! Converting each *advance* instead would round a hundred times down a line
//! and drift by a visible amount; converting the accumulated *position* rounds
//! once. `float_arithmetic` is `forbid` workspace-wide, so this is also the
//! only arithmetic available — and it is what makes 1.7.5's byte-stable
//! goldens possible.

use harfrust::{Direction, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use pos_domain::receipt::{ReceiptModel, TextDirection};
use skrifa::prelude::{LocationRef, Size};
use skrifa::{FontRef as OutlineFont, MetadataProvider};
use unicode_bidi::{BidiInfo, Level};

use crate::font::{self, Weight};

use super::{BODY_PX_PER_EM, PrinterProfile, RenderError};

/// Where a line sits across the page, in logical terms.
///
/// `Start` is the right edge of an Arabic receipt and the left edge of an
/// English one. Naming the sides physically is the mistake
/// `scripts/check-logical-css.sh` exists to stop on the screen, and paper has
/// the same failure mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Alignment {
    Start,
    End,
    Center,
}

/// How a run of text is set.
///
/// No `Hash`: `font::Weight` does not derive it, and reaching into 1.7.2's type
/// to add a bound for this one's convenience is the wrong direction of change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStyle {
    pub px_per_em: i32,
    pub weight: Weight,
}

/// One glyph, placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutGlyph {
    pub glyph_id: u16,
    /// Left edge of the glyph's origin, in dots from the page's left edge.
    pub x_px: i32,
    /// The baseline this glyph sits on, in dots from the page's top edge.
    pub baseline_y_px: i32,
    pub style: TextStyle,
}

/// One line of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutLine {
    pub glyphs: Vec<LayoutGlyph>,
    pub baseline_y_px: i32,
}

/// A finished page, before any dot is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub width_px: u32,
    pub height_px: u32,
    pub lines: Vec<LayoutLine>,
    /// Horizontal separators, by the row they occupy.
    pub rules: Vec<i32>,
    /// The document's base direction, kept so the rasteriser never has to
    /// re-derive it from the model.
    pub direction: TextDirection,
}

/// A glyph with its pen position still in font units.
#[derive(Debug, Clone, Copy)]
struct Shaped {
    glyph_id: u16,
    /// Pen position, font units, measured from the run's visual left edge.
    x_units: i32,
}

/// The shaped form of one logical line, in visual order.
#[derive(Debug, Clone)]
struct ShapedLine {
    glyphs: Vec<Shaped>,
    /// Total advance, font units.
    width_units: i32,
}

/// The two embedded faces, parsed once per page.
///
/// `pub(crate)` so [`super::raster`] draws with the same parsed faces this
/// module measured with, rather than parsing them a second time. That is not
/// only cost — a layout measured against one parse and drawn against another is
/// two sources of truth for the same glyph advance, which is the class of
/// defect 1.7.2 and 1.11.1 spent a cross-language test to rule out one layer
/// up.
pub(crate) struct Faces<'a> {
    regular: OutlineFont<'a>,
    bold: OutlineFont<'a>,
    regular_font: FontRef<'a>,
    bold_font: FontRef<'a>,
    regular_shaper: ShaperData,
    bold_shaper: ShaperData,
    /// Design units per em. Both faces are the same family and agree; the
    /// regular one is the reference and `the_two_faces_agree_on_units_per_em`
    /// holds that.
    pub(crate) units_per_em: i32,
    ascender_units: i32,
    descender_units: i32,
    line_gap_units: i32,
}

impl<'a> Faces<'a> {
    pub(crate) fn load() -> Result<Faces<'a>, RenderError> {
        let regular = OutlineFont::from_index(font::REGULAR, 0)
            .map_err(|_| RenderError::FontUnreadable("regular"))?;
        let bold = OutlineFont::from_index(font::BOLD, 0)
            .map_err(|_| RenderError::FontUnreadable("bold"))?;
        let regular_font = FontRef::from_index(font::REGULAR, 0)
            .map_err(|_| RenderError::FontUnreadable("regular"))?;
        let bold_font =
            FontRef::from_index(font::BOLD, 0).map_err(|_| RenderError::FontUnreadable("bold"))?;
        let regular_shaper = ShaperData::new(&regular_font);
        let bold_shaper = ShaperData::new(&bold_font);

        // Unscaled metrics are design units, and this pipeline works in design
        // units until the very last step — so the casts below truncate nothing
        // a font actually stores. `Size::unscaled()` is what asks for them.
        let metrics = regular.metrics(Size::unscaled(), LocationRef::default());
        let units_per_em = i32::from(metrics.units_per_em);
        let ascender_units = design_units(metrics.ascent);
        let descender_units = design_units(metrics.descent);
        let line_gap_units = design_units(metrics.leading);
        Ok(Faces {
            regular,
            bold,
            regular_font,
            bold_font,
            regular_shaper,
            bold_shaper,
            units_per_em,
            ascender_units,
            descender_units,
            line_gap_units,
        })
    }

    /// The outline and metric source for one weight.
    pub(crate) fn face(&self, weight: Weight) -> &OutlineFont<'a> {
        match weight {
            Weight::Regular => &self.regular,
            Weight::Bold => &self.bold,
        }
    }

    /// The shaping source for one weight.
    fn shaper(&self, weight: Weight) -> (&ShaperData, &FontRef<'a>) {
        match weight {
            Weight::Regular => (&self.regular_shaper, &self.regular_font),
            Weight::Bold => (&self.bold_shaper, &self.bold_font),
        }
    }

    /// Distance from one baseline to the next, in dots.
    fn line_height_px(&self, px_per_em: i32) -> i32 {
        let units = self.ascender_units - self.descender_units + self.line_gap_units;
        units_to_px(units, px_per_em, self.units_per_em)
    }

    /// Height above the baseline, in dots.
    fn ascent_px(&self, px_per_em: i32) -> i32 {
        units_to_px(self.ascender_units, px_per_em, self.units_per_em)
    }
}

/// A font metric as an integer number of design units.
///
/// `skrifa` reports unscaled metrics as `f32` because the same call scales
/// them on request; at `Size::unscaled()` they are whole design units, so this
/// is a cast rather than a rounding — and a cast is not arithmetic, which is
/// what `float_arithmetic = "forbid"` leaves available.
#[expect(
    clippy::cast_possible_truncation,
    reason = "an unscaled font metric is a whole design unit, bounded by the format at i16"
)]
fn design_units(metric: f32) -> i32 {
    metric as i32
}

/// Font units to dots. The one conversion in this module, so rounding happens
/// in one place and can be reasoned about once.
///
/// Truncating rather than rounding to nearest: a glyph placed half a dot early
/// is invisible, and truncation is what keeps the arithmetic associative — the
/// property `prop_rendering_is_deterministic` rests on.
const fn units_to_px(units: i32, px_per_em: i32, units_per_em: i32) -> i32 {
    // `units_per_em` is read from a parsed face and is non-zero by the format's
    // own rules; the guard costs nothing and turns a corrupt font into a blank
    // page rather than a division panic.
    if units_per_em == 0 {
        return 0;
    }
    units * px_per_em / units_per_em
}

/// Shape one logical string into visually ordered glyphs, in font units.
///
/// The bidi pass is what makes `2.166` read left-to-right inside an Arabic
/// line while the line itself runs right-to-left. Each directional run is
/// shaped on its own, in its own direction, and the runs are laid down in the
/// visual order [`unicode_bidi`] returns — which is the whole algorithm, and
/// the reason a shaper alone is not enough.
fn shape_line(faces: &Faces<'_>, text: &str, weight: Weight, base_rtl: bool) -> ShapedLine {
    let mut glyphs = Vec::new();
    let mut pen = 0;

    if text.is_empty() {
        return ShapedLine {
            glyphs,
            width_units: 0,
        };
    }

    let base = if base_rtl { Level::rtl() } else { Level::ltr() };
    let info = BidiInfo::new(text, Some(base));

    for para in &info.paragraphs {
        let line = para.range.clone();
        let (levels, runs) = info.visual_runs(para, line);
        for run in runs {
            let Some(level) = levels.get(run.start) else {
                continue;
            };
            let Some(slice) = text.get(run.clone()) else {
                continue;
            };
            let rtl = level.is_rtl();

            let mut buffer = UnicodeBuffer::new();
            buffer.push_str(slice);
            buffer.set_direction(if rtl {
                Direction::RightToLeft
            } else {
                Direction::LeftToRight
            });
            // `guess_segment_properties` selects the Arabic script, and it is
            // load-bearing rather than tidy: without it the shaper plans for no
            // script at all and returns the **isolated** form of every letter —
            // `بيع` comes back as three unjoined glyphs. Measured during the
            // move off `rustybuzz`, which guessed on its own and so hid the
            // requirement.
            buffer.guess_segment_properties();
            let (data, font) = faces.shaper(weight);
            let shaped = data
                .shaper(font)
                .build()
                .shape(buffer, ShapeOptions::default());

            for (info, position) in shaped
                .glyph_infos()
                .iter()
                .zip(shaped.glyph_positions().iter())
            {
                // `glyph_id` is a `u32` in the shaping API and a `u16` in every
                // font format. A value past `u16` cannot come from a parsed
                // face, and silently truncating one would draw the wrong
                // glyph, so it is skipped instead.
                let Ok(glyph_id) = u16::try_from(info.glyph_id) else {
                    continue;
                };
                glyphs.push(Shaped {
                    glyph_id,
                    x_units: pen + position.x_offset,
                });
                pen += position.x_advance;
            }
        }
    }

    ShapedLine {
        glyphs,
        width_units: pen,
    }
}

/// Greedy word wrap, measured in font units.
///
/// Breaks on ASCII space, which is what a receipt contains: product names, a
/// currency amount and a quantity. **Arabic does not join across a space**, so
/// measuring a word on its own and summing is exact rather than an
/// approximation — the one place a naive wrapper would be wrong for this script
/// is not reachable here. That is also why this is linear: an earlier draft
/// re-shaped the whole accumulated candidate for every word, which is
/// quadratic in the line and was the largest single cost in the suite.
///
/// A word too long for the measure is **broken at a character boundary**, not
/// left to overhang. An earlier draft emitted it whole on the argument that a
/// truncated SKU is worse than an overhang; both are true and both are wrong,
/// because an overhang is not visible in the bitmap — it is *outside* it, so
/// the dots are simply gone. A UUID printed with its middle intact and its ends
/// missing is the failure this avoids, and the fiscal block's 36-character
/// identifier is exactly that case on 58 mm paper.
fn wrap(
    faces: &Faces<'_>,
    text: &str,
    weight: Weight,
    base_rtl: bool,
    max_units: i32,
) -> Vec<String> {
    if text.trim().is_empty() {
        return vec![String::new()];
    }

    let space = shape_line(faces, " ", weight, base_rtl).width_units;
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_units = 0;

    for word in text.split(' ').filter(|w| !w.is_empty()) {
        for (piece, piece_units) in split_to_fit(faces, word, weight, base_rtl, max_units) {
            let joined = if current.is_empty() {
                piece_units
            } else {
                current_units + space + piece_units
            };

            if current.is_empty() {
                current = piece;
                current_units = piece_units;
            } else if joined <= max_units {
                current.push(' ');
                current.push_str(&piece);
                current_units = joined;
            } else {
                lines.push(core::mem::take(&mut current));
                current = piece;
                current_units = piece_units;
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Break one word into pieces that each fit the measure.
///
/// Returns the word untouched whenever it fits, which is every word on a real
/// receipt. The fallback walks characters rather than bytes, so a break never
/// lands inside a UTF-8 sequence and never inside an Arabic letter — and a
/// single character wider than the whole measure is emitted alone, because the
/// alternative is an empty piece and a loop that does not terminate.
/// Each piece carries the width it was measured at, so [`wrap`] never shapes
/// the same text twice — which is the common path, since almost every word on a
/// receipt fits and comes straight back out of the first branch.
fn split_to_fit(
    faces: &Faces<'_>,
    word: &str,
    weight: Weight,
    base_rtl: bool,
    max_units: i32,
) -> Vec<(String, i32)> {
    let whole = shape_line(faces, word, weight, base_rtl).width_units;
    if max_units <= 0 || whole <= max_units {
        return vec![(word.to_owned(), whole)];
    }

    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut piece_units = 0;
    for ch in word.chars() {
        let mut candidate = piece.clone();
        candidate.push(ch);
        let candidate_units = shape_line(faces, &candidate, weight, base_rtl).width_units;
        if piece.is_empty() || candidate_units <= max_units {
            piece = candidate;
            piece_units = candidate_units;
        } else {
            pieces.push((core::mem::take(&mut piece), piece_units));
            piece.push(ch);
            piece_units = shape_line(faces, &piece, weight, base_rtl).width_units;
        }
    }
    if !piece.is_empty() {
        pieces.push((piece, piece_units));
    }
    pieces
}

/// Where a line of `width_px` starts, given the page and its direction.
///
/// `Start` and `End` are logical: on an Arabic receipt `Start` is the right
/// margin. This is the function `layout_columns_align_in_rtl` proves, and the
/// reason the label and the amount of a totals row swap sides with the
/// language rather than with a flag somebody has to remember to set.
const fn start_x_px(
    alignment: Alignment,
    rtl: bool,
    width_px: i32,
    content_width_px: i32,
    margin_px: i32,
) -> i32 {
    // Never negative. A line wider than the box would otherwise start off the
    // left edge when centred, and the dots there are not clipped — they are
    // never drawn, so the beginning of the line is silently missing. Wrapping
    // makes this unreachable for text; the floor is what keeps it unreachable
    // for a single character wider than the paper.
    let leading = if content_width_px > width_px {
        content_width_px - width_px
    } else {
        0
    };
    match (alignment, rtl) {
        (Alignment::Start, false) | (Alignment::End, true) => margin_px,
        (Alignment::Start, true) | (Alignment::End, false) => margin_px + leading,
        (Alignment::Center, _) => margin_px + leading / 2,
    }
}

/// A page under construction.
struct Page<'a> {
    faces: &'a Faces<'a>,
    profile: PrinterProfile,
    rtl: bool,
    lines: Vec<LayoutLine>,
    rules: Vec<i32>,
    /// The baseline the next line will sit on.
    cursor_y_px: i32,
}

impl Page<'_> {
    /// Add one paragraph, wrapped, at one alignment.
    fn text(&mut self, text: &str, style: TextStyle, alignment: Alignment) {
        let max_units = px_to_units(
            self.profile.content_width_px(),
            style.px_per_em,
            self.faces.units_per_em,
        );
        for logical in wrap(self.faces, text, style.weight, self.rtl, max_units) {
            let shaped = shape_line(self.faces, &logical, style.weight, self.rtl);
            self.place(&shaped, style, alignment);
        }
    }

    /// Add a label and an amount on one row, at opposite ends.
    ///
    /// The amount never moves: it is the column a cashier reads down, and one
    /// that shifts by a character between two rows is unreadable at a glance.
    /// So the **label** is what yields — it is wrapped into the width left
    /// over, and its overflow continues on the rows beneath, with the amount
    /// on the first of them.
    ///
    /// Without that, a long discount label and its amount are placed at
    /// opposite ends of a row they do not both fit on, and the two runs print
    /// over each other. Nothing in the references says so because nothing in
    /// the references imagined a label written by a merchant.
    fn row(&mut self, label: &str, amount: &str, style: TextStyle) {
        let content = self.profile.content_width_px();
        let margin = self.profile.margin_px();

        let amount_shaped = shape_line(self.faces, amount, style.weight, self.rtl);
        let amount_px = units_to_px(
            amount_shaped.width_units,
            style.px_per_em,
            self.faces.units_per_em,
        );

        // An amount wider than the whole box is not an amount — it is the
        // fiscal UUID, or a per-rate triple on 58 mm paper. Two columns cannot
        // hold it, so the row becomes two wrapped paragraphs instead of
        // overflowing the paper. Found by `a_long_label_never_collides_with_its_amount`,
        // which put a glyph at x=389 on a 384-dot page: the label was being
        // wrapped and the amount never was.
        if amount_px > content {
            self.text(label, style, Alignment::Start);
            self.text(amount, style, Alignment::Start);
            return;
        }

        // One em of clear space between the two columns, so they are read as
        // two columns rather than as one run of text.
        //
        // **The mutation sweep's one survivor**, recorded rather than left to
        // be rediscovered: removing this gutter loses no data and breaks no
        // test, because a label only reaches its amount when it is long enough
        // to have wrapped anyway. What it costs is legibility — two columns
        // that touch read as one run — and legibility on paper is judged by
        // 1.7.5's goldens under a native reader's eye, which is the review this
        // line is waiting for rather than an assertion it is missing.
        let label_room = content - amount_px - style.px_per_em;
        // The fast path is the common one and it is worth having: almost every
        // label on a receipt fits beside its amount, and `wrap` shapes a space
        // and each word to find that out. Shaping is the whole cost of a
        // layout — measured at ~180 ms for a full receipt in a debug build — so
        // one shape here rather than three is the difference between a property
        // that runs in ten seconds and one that runs in fifty.
        let whole = shape_line(self.faces, label, style.weight, self.rtl).width_units;
        let whole_px = units_to_px(whole, style.px_per_em, self.faces.units_per_em);
        let label_lines = if label_room <= 0 {
            vec![String::new()]
        } else if whole_px <= label_room {
            vec![label.to_owned()]
        } else {
            wrap(
                self.faces,
                label,
                style.weight,
                self.rtl,
                px_to_units(label_room, style.px_per_em, self.faces.units_per_em),
            )
        };

        for (index, logical) in label_lines.iter().enumerate() {
            let label_shaped = shape_line(self.faces, logical, style.weight, self.rtl);
            let label_px = units_to_px(
                label_shaped.width_units,
                style.px_per_em,
                self.faces.units_per_em,
            );
            let baseline = self.cursor_y_px + self.faces.ascent_px(style.px_per_em);
            let label_x = start_x_px(Alignment::Start, self.rtl, label_px, content, margin);

            let mut glyphs = Vec::with_capacity(label_shaped.glyphs.len());
            self.emit(&mut glyphs, &label_shaped, label_x, baseline, style);
            if index == 0 {
                let amount_x = start_x_px(Alignment::End, self.rtl, amount_px, content, margin);
                self.emit(&mut glyphs, &amount_shaped, amount_x, baseline, style);
            }

            self.lines.push(LayoutLine {
                glyphs,
                baseline_y_px: baseline,
            });
            self.cursor_y_px += self.faces.line_height_px(style.px_per_em);
        }
    }

    fn place(&mut self, shaped: &ShapedLine, style: TextStyle, alignment: Alignment) {
        let width_px = units_to_px(shaped.width_units, style.px_per_em, self.faces.units_per_em);
        let x = start_x_px(
            alignment,
            self.rtl,
            width_px,
            self.profile.content_width_px(),
            self.profile.margin_px(),
        );
        let baseline = self.cursor_y_px + self.faces.ascent_px(style.px_per_em);

        let mut glyphs = Vec::with_capacity(shaped.glyphs.len());
        self.emit(&mut glyphs, shaped, x, baseline, style);
        self.lines.push(LayoutLine {
            glyphs,
            baseline_y_px: baseline,
        });
        self.cursor_y_px += self.faces.line_height_px(style.px_per_em);
    }

    fn emit(
        &self,
        out: &mut Vec<LayoutGlyph>,
        shaped: &ShapedLine,
        x_px: i32,
        baseline_y_px: i32,
        style: TextStyle,
    ) {
        for glyph in &shaped.glyphs {
            out.push(LayoutGlyph {
                glyph_id: glyph.glyph_id,
                x_px: x_px + units_to_px(glyph.x_units, style.px_per_em, self.faces.units_per_em),
                baseline_y_px,
                style,
            });
        }
    }

    /// A horizontal separator, and the space around it.
    fn rule(&mut self) {
        let gap = BODY_PX_PER_EM / 3;
        self.cursor_y_px += gap;
        self.rules.push(self.cursor_y_px);
        self.cursor_y_px += gap;
    }

    fn gap(&mut self) {
        self.cursor_y_px += BODY_PX_PER_EM / 2;
    }
}

/// Dots to font units — wrapping's measuring stick, so a candidate line is
/// compared without converting every advance.
const fn px_to_units(px: i32, px_per_em: i32, units_per_em: i32) -> i32 {
    if px_per_em == 0 {
        return 0;
    }
    px * units_per_em / px_per_em
}

/// Lay a validated model out for one paper width.
///
/// The order is `ref/hardware-and-receipts.md` §2.3's receipt anatomy, which is
/// itself master plan B.6 and C.11. It is not a design choice to revisit here:
/// the merchant block is legally required at the top, and the fiscal QR sits
/// below the tenders because that is where a customer and an auditor both look
/// for it.
///
/// # Errors
///
/// Refuses an unreadable embedded face and a page past
/// [`super::MAX_PAGE_HEIGHT_PX`].
pub fn lay_out(model: &ReceiptModel, profile: PrinterProfile) -> Result<Layout, RenderError> {
    let faces = Faces::load()?;
    lay_out_with(&faces, model, profile)
}

/// [`lay_out`], against faces the caller already parsed.
///
/// # Errors
///
/// Refuses a page past [`super::MAX_PAGE_HEIGHT_PX`].
pub(crate) fn lay_out_with(
    faces: &Faces<'_>,
    model: &ReceiptModel,
    profile: PrinterProfile,
) -> Result<Layout, RenderError> {
    let direction = model.locale.direction;
    let rtl = matches!(direction, TextDirection::RightToLeft);

    let body = TextStyle {
        px_per_em: BODY_PX_PER_EM,
        weight: Weight::Regular,
    };
    let strong = TextStyle {
        px_per_em: BODY_PX_PER_EM,
        weight: Weight::Bold,
    };
    // The total is the largest element on the page: a cashier reads it a
    // thousand times a day and a customer reads it upside down
    // (`ref/ui-spec.md` §3, which is about the screen and is true of paper).
    let total = TextStyle {
        px_per_em: BODY_PX_PER_EM * 3 / 2,
        weight: Weight::Bold,
    };

    let mut page = Page {
        faces,
        profile,
        rtl,
        lines: Vec::new(),
        rules: Vec::new(),
        cursor_y_px: profile.margin_px(),
    };

    // ── merchant, legally required (master plan B.6) ────────────────────
    page.text(&model.merchant.legal_name, strong, Alignment::Center);
    page.text(&model.merchant.store_name, body, Alignment::Center);
    if let Some(address) = &model.merchant.address {
        page.text(address, body, Alignment::Center);
    }
    if let Some(tin) = &model.merchant.tin {
        page.text(tin, body, Alignment::Center);
    }
    page.gap();

    let language = model.locale.language;

    // ── what this document is ───────────────────────────────────────────
    page.text(
        words::doc_kind(model.doc_kind, language),
        strong,
        Alignment::Center,
    );
    if let Some(mark) = model.watermark {
        page.text(words::watermark(mark, language), strong, Alignment::Center);
    }

    // ── the B2B buyer, when a TIN was captured ──────────────────────────
    if let Some(buyer) = &model.buyer {
        page.gap();
        if let Some(name) = &buyer.name {
            page.row(words::buyer(language), name, body);
        }
        page.row(words::tax_number(language), &buyer.tin, body);
    }

    // ── header ──────────────────────────────────────────────────────────
    page.gap();
    page.row(words::receipt_no(language), &model.header.receipt_no, body);
    page.row(words::register(language), &model.header.register_code, body);
    page.row(words::cashier(language), &model.header.cashier_name, body);
    // §2.3 asks for "date & time", and it is the field a customer needs to
    // return goods and an auditor needs to place the document in a period. The
    // business date is not the calendar date of `issued_at` for a sale after
    // midnight (conventions §11), so both are printed: the instant the sale
    // happened, and the trading day it belongs to.
    page.row(
        words::issued_at(language),
        &model.header.issued_at.to_iso8601(),
        body,
    );
    page.row("", &model.header.business_date.to_string(), body);
    page.rule();

    // ── lines, each with its allowances beneath it ──────────────────────
    for line in &model.lines {
        page.text(&line.name, body, Alignment::Start);
        // "qty × unit · line total" (§2.3). The quantity renders through
        // `Qty::format`, which takes `is_weighed` because `0.347 kg` and `2`
        // are different documents; the amounts render through
        // `Money::format_exact` and nothing else.
        page.row(
            &format!(
                "{} × {}",
                line.qty.format(line.is_weighed),
                format_amount(&line.unit_price)
            ),
            &format_amount(&line.line_total),
            body,
        );
        for discount in &line.discounts {
            page.row(&discount.label, &format_amount(&discount.amount), body);
        }
    }
    page.rule();

    // ── totals ──────────────────────────────────────────────────────────
    page.row(
        words::subtotal(language),
        &format_amount(&model.totals.subtotal),
        body,
    );
    page.row(
        words::discount(language),
        &format_amount(&model.totals.discount_total),
        body,
    );
    // Net, tax and gross per rate — the sales-side evidence the whole tax
    // engine exists for, and the row a bi-monthly return is reconstructed from.
    // All three, not just the tax: a return needs the base it was charged on.
    for row in &model.totals.tax_summary {
        page.row(
            &format!("{} {}", row.code, row.rate.format()),
            &format!(
                "{} / {} / {}",
                format_amount(&row.net),
                format_amount(&row.tax),
                format_amount(&row.gross)
            ),
            body,
        );
    }
    if let Some(adjustment) = &model.totals.rounding_adjustment {
        page.row(words::rounding(language), &format_amount(adjustment), body);
    }
    page.row(
        words::total(language),
        &format_amount(&model.totals.total),
        total,
    );
    page.rule();

    // ── tenders, change, loyalty ────────────────────────────────────────
    for tender in &model.tenders {
        let label = match (&tender.masked_pan, &tender.scheme) {
            (Some(pan), Some(scheme)) => format!("{} {scheme} {pan}", tender.label),
            (Some(pan), None) => format!("{} {pan}", tender.label),
            _ => tender.label.clone(),
        };
        page.row(&label, &format_amount(&tender.amount), body);
    }
    if let Some(change) = &model.change {
        page.row(words::change(language), &format_amount(change), body);
    }
    if let Some(loyalty) = &model.loyalty {
        page.row(
            words::loyalty(language),
            &loyalty.balance_points.to_string(),
            body,
        );
    }

    // ── the fiscal document, once cleared ───────────────────────────────
    //
    // The UUID, not the QR. `ref/fiscal-jofotara.md:450` puts "QR payload →
    // raster for the receipt" at microstep 2.7.9, and the payload on
    // `FiscalBlock` is opaque here by design — 1.7.1 recorded that it is
    // persisted so a reprint days later carries the identical QR (E.46).
    if let Some(fiscal) = &model.fiscal {
        page.rule();
        page.row(words::fiscal(language), &fiscal.uuid, body);
    }

    // ── footer ──────────────────────────────────────────────────────────
    page.rule();
    if let Some(policy) = &model.footer.return_policy {
        page.text(policy, body, Alignment::Center);
    }
    if let Some(thanks) = &model.footer.thank_you {
        page.text(thanks, body, Alignment::Center);
    }

    let height_px = u32::try_from(page.cursor_y_px + profile.margin_px()).unwrap_or(u32::MAX);
    if height_px > super::MAX_PAGE_HEIGHT_PX {
        return Err(RenderError::PageTooTall {
            height_px,
            limit_px: super::MAX_PAGE_HEIGHT_PX,
        });
    }

    Ok(Layout {
        width_px: profile.width_px(),
        height_px,
        lines: page.lines,
        rules: page.rules,
        direction,
    })
}

/// A money amount as the page prints it.
///
/// [`Money::format_exact`] and nothing else — the currency's own exponent,
/// with the store's `money_decimals` nowhere in the path
/// (`ref/hardware-and-receipts.md` §2.3, first non-negotiable rule). 1.7.1 made
/// that structural by leaving the setting off `ReceiptLocale`; this function is
/// where it would otherwise have crept back in.
fn format_amount(amount: &pos_domain::money::Money) -> String {
    amount.format_exact()
}

/// Every fixed word a receipt prints, in both languages.
///
/// **Not the screen's catalogue.** `apps/terminal/src/i18n` is a React
/// concern; this crate renders paper, and a receipt is read by a customer, a
/// merchant and eventually a tax authority rather than by a user interface.
/// Wiring paper to the front end's catalogue would also make the printed
/// document depend on a package that cannot be loaded without a webview.
///
/// One `match` per word, in the document's own language, keyed off
/// [`ReceiptLocale::language`]. 1.7.5's goldens pin every one of them.
mod words {
    use pos_domain::receipt::{DocKind, Language, Watermark};

    pub(super) const fn doc_kind(kind: DocKind, language: Language) -> &'static str {
        match (kind, language) {
            (DocKind::Sale, Language::Arabic) => "فاتورة بيع",
            (DocKind::Sale, Language::English) => "SALE",
            (DocKind::Refund, Language::Arabic) => "إشعار دائن",
            (DocKind::Refund, Language::English) => "REFUND",
            (DocKind::Acknowledgement, Language::Arabic) => "إشعار استلام غير ضريبي",
            (DocKind::Acknowledgement, Language::English) => "ACKNOWLEDGEMENT",
            (DocKind::XReport, Language::Arabic) => "تقرير X",
            (DocKind::XReport, Language::English) => "X REPORT",
            (DocKind::ZReport, Language::Arabic) => "تقرير Z",
            (DocKind::ZReport, Language::English) => "Z REPORT",
        }
    }

    pub(super) const fn watermark(mark: Watermark, language: Language) -> &'static str {
        match (mark, language) {
            (Watermark::Duplicate, Language::Arabic) => "نسخة",
            (Watermark::Duplicate, Language::English) => "DUPLICATE",
            (Watermark::Training, Language::Arabic) => "تدريب",
            (Watermark::Training, Language::English) => "TRAINING",
        }
    }

    macro_rules! word {
        ($name:ident, $ar:expr, $en:expr) => {
            pub(super) const fn $name(language: Language) -> &'static str {
                match language {
                    Language::Arabic => $ar,
                    Language::English => $en,
                }
            }
        };
    }

    word!(buyer, "المشتري", "Buyer");
    word!(tax_number, "الرقم الضريبي", "TIN");
    word!(receipt_no, "رقم الفاتورة", "Receipt");
    word!(register, "الصندوق", "Register");
    word!(cashier, "الكاشير", "Cashier");
    word!(issued_at, "التاريخ", "Date");
    word!(subtotal, "المجموع الفرعي", "Subtotal");
    word!(discount, "الخصم", "Discount");
    word!(rounding, "تقريب النقد", "Rounding");
    word!(total, "الإجمالي", "TOTAL");
    word!(change, "الباقي", "Change");
    word!(loyalty, "رصيد النقاط", "Points");
    word!(fiscal, "رقم الفاتورة الضريبية", "Fiscal UUID");
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::render::tests_support::model;
    use pos_domain::receipt::{Language, ReceiptLocale};

    fn faces() -> Faces<'static> {
        Faces::load().expect("the embedded faces parse")
    }

    /// The named test: the pipeline draws with the face 1.7.2 embedded, and
    /// nothing else. There is no font path, no system lookup and no fallback —
    /// which is the whole point, and is why `cosmic-text`'s `fontdb` is not in
    /// this crate's graph.
    #[test]
    fn rasterizer_loads_the_embedded_font() {
        let faces = faces();

        assert_eq!(faces.units_per_em, 1000);
        assert!(faces.ascender_units > 0);
        assert!(faces.descender_units < 0);

        // The two faces really are two faces. A bold byte-identical to the
        // regular renders a total that does not stand out, and 1.7.2 already
        // holds that for the *bytes*; this holds it for the parsed faces the
        // renderer actually draws with.
        let regular = shape_line(&faces, "Total", Weight::Regular, false);
        let bold = shape_line(&faces, "Total", Weight::Bold, false);
        assert!(regular.width_units > 0);
        assert_ne!(regular.width_units, bold.width_units);
    }

    /// The exact glyphs and pen positions of one Arabic word.
    ///
    /// A golden, and the sweep is why. Forcing every run to shape
    /// left-to-right — ignoring the bidi level entirely — survived every other
    /// test: `arabic_joining_uses_contextual_glyphs` compares joined against
    /// isolated and both move together, and `rtl_run_orders_visual_positions`
    /// measures the order of *runs* rather than the order inside one.
    ///
    /// These numbers come from the committed face, whose bytes 1.7.2 pins and
    /// whose licence travels with it. They move only when the font does, which
    /// is a reviewed act — the same contract `1.6.6`'s pinned chain hash has.
    #[test]
    fn the_arabic_glyph_sequence_is_pinned() {
        let faces = faces();
        let shaped = shape_line(&faces, "بيع", Weight::Regular, true);
        let placed: Vec<(u16, i32)> = shaped
            .glyphs
            .iter()
            .map(|g| (g.glyph_id, g.x_units))
            .collect();

        assert_eq!(placed, vec![(750, 0), (336, 572), (389, 882)]);
        assert_eq!(shaped.width_units, 1_135);
    }

    /// A mark's own offset reaches the page.
    ///
    /// A shaper positions a combining mark with a zero advance and a
    /// non-zero `x_offset`; dropping that offset stacks every diacritic on the
    /// letter's origin instead of over its shoulder. Unvocalised Arabic — every
    /// other string in this suite — has no marks at all, so the sweep's "drop
    /// the offset" mutation survived until a vocalised word was tested.
    ///
    /// `بَيْع` is `بيع` with fatha and sukun. The two marks carry no advance, so
    /// the word is the same width; what changes is that two glyphs sit at
    /// positions no advance would have produced.
    #[test]
    fn a_combining_mark_keeps_its_own_offset() {
        let faces = faces();
        let plain = shape_line(&faces, "بيع", Weight::Regular, true);
        let vocalised = shape_line(&faces, "بَيْع", Weight::Regular, true);

        assert_eq!(vocalised.glyphs.len(), plain.glyphs.len() + 2);
        // Marks have no advance, so the word did not get wider.
        assert_eq!(vocalised.width_units, plain.width_units);

        let placed: Vec<(u16, i32)> = vocalised
            .glyphs
            .iter()
            .map(|g| (g.glyph_id, g.x_units))
            .collect();
        // 614 = 572 + 42 and 888 = 882 + 6 — the pen position plus the mark's
        // own offset. Without the offset they would read 572 and 882.
        assert_eq!(
            placed,
            vec![(750, 0), (1573, 614), (336, 572), (1600, 888), (389, 882),]
        );
    }

    #[test]
    fn the_two_faces_agree_on_units_per_em() {
        let faces = faces();
        let upem = |f: &OutlineFont<'_>| {
            f.metrics(Size::unscaled(), LocationRef::default())
                .units_per_em
        };
        assert_eq!(upem(&faces.regular), upem(&faces.bold));
    }

    /// The named test, and the one that proves this is a shaper rather than a
    /// character-to-glyph table.
    ///
    /// `بيع` — BEH, YEH, AIN — joins, so each letter takes its initial, medial
    /// and final form. Shaped in isolation each takes its isolated form. Every
    /// one of the three glyph ids must differ between the two, or the receipt
    /// is printing disconnected letters, which is the exact failure that made
    /// rasterising necessary instead of a printer codepage.
    #[test]
    fn arabic_joining_uses_contextual_glyphs() {
        let faces = faces();

        let joined = shape_line(&faces, "بيع", Weight::Regular, true);
        assert_eq!(joined.glyphs.len(), 3, "three letters, three glyphs");

        let isolated: Vec<u16> = ["ب", "ي", "ع"]
            .iter()
            .map(|letter| {
                let shaped = shape_line(&faces, letter, Weight::Regular, true);
                shaped.glyphs.first().expect("one glyph").glyph_id
            })
            .collect();

        // Visual order is right to left, so the first glyph is the rightmost —
        // which is BEH, the first letter.
        for (index, (joined_glyph, isolated_id)) in
            joined.glyphs.iter().zip(isolated.iter()).enumerate()
        {
            assert_ne!(
                joined_glyph.glyph_id, *isolated_id,
                "letter {index} took its isolated form inside a word"
            );
        }
    }

    /// The named test: the document's base direction decides which run of a
    /// mixed line sits where, and the glyphs come out in **visual** order —
    /// left to right across the paper, whichever way the language runs.
    ///
    /// An earlier draft of this test asserted that shaping `بيع` alone with an
    /// LTR base would start with a different glyph. It does not, and the
    /// algorithm is right: Arabic characters are strongly right-to-left, so
    /// they are reordered inside their own run no matter what the paragraph
    /// says. The base level governs *neutral* characters and the order of runs
    /// **relative to each other** — which is what this now measures, on a line
    /// that actually has two runs.
    #[test]
    fn rtl_run_orders_visual_positions() {
        let faces = faces();

        let latin: Vec<u16> = shape_line(&faces, "Cola", Weight::Regular, false)
            .glyphs
            .iter()
            .map(|g| g.glyph_id)
            .collect();

        let mut first_latin_index = Vec::new();
        for base_rtl in [true, false] {
            let line = shape_line(&faces, "بيع Cola", Weight::Regular, base_rtl);

            let positions: Vec<i32> = line.glyphs.iter().map(|g| g.x_units).collect();
            assert!(
                positions.is_sorted(),
                "pen positions must advance left to right: {positions:?}"
            );

            let ids: Vec<u16> = line.glyphs.iter().map(|g| g.glyph_id).collect();
            let at = ids
                .windows(latin.len())
                .position(|w| w == latin.as_slice())
                .expect("the Latin run survives");
            first_latin_index.push(at);
        }

        // On an Arabic receipt the Latin run is the leftmost thing on the line;
        // on an English one it comes after the Arabic. Same characters, same
        // shaping, opposite visual order — which is the whole of what a bidi
        // pass buys and what a shaper alone would not have done.
        assert_eq!(
            first_latin_index.first(),
            Some(&0),
            "with an RTL base the Latin run must sit at the left edge"
        );
        assert!(
            first_latin_index.last().copied().unwrap_or(0) > 0,
            "with an LTR base the Arabic run must come first"
        );
    }

    /// Western digits inside an Arabic line keep their own direction.
    ///
    /// This is the case the Unicode Bidirectional Algorithm exists for, and the
    /// reason `unicode-bidi` is a dependency: `harfrust` shapes one direction
    /// at a time and would happily set `2.166` backwards inside an Arabic run.
    #[test]
    fn digits_render_left_to_right_inside_an_arabic_line() {
        let faces = faces();

        // The digits alone, left to right — the order they must keep.
        let alone = shape_line(&faces, "2.166", Weight::Regular, false);
        let alone_ids: Vec<u16> = alone.glyphs.iter().map(|g| g.glyph_id).collect();

        let mixed = shape_line(&faces, "الإجمالي 2.166", Weight::Regular, true);
        let mixed_ids: Vec<u16> = mixed.glyphs.iter().map(|g| g.glyph_id).collect();

        // The digit run appears somewhere in the line, in its own order,
        // unreversed. A bidi-less pipeline produces `661.2` here.
        assert!(
            mixed_ids
                .windows(alone_ids.len())
                .any(|window| window == alone_ids.as_slice()),
            "the digit run was reordered: {mixed_ids:?} does not contain {alone_ids:?}"
        );
    }

    #[test]
    fn a_latin_run_inside_arabic_keeps_its_own_direction() {
        let faces = faces();

        let alone = shape_line(&faces, "Cola", Weight::Regular, false);
        let alone_ids: Vec<u16> = alone.glyphs.iter().map(|g| g.glyph_id).collect();

        let mixed = shape_line(&faces, "مشروب Cola بارد", Weight::Regular, true);
        let mixed_ids: Vec<u16> = mixed.glyphs.iter().map(|g| g.glyph_id).collect();

        assert!(
            mixed_ids
                .windows(alone_ids.len())
                .any(|window| window == alone_ids.as_slice()),
            "the Latin run was reordered inside an Arabic line"
        );
    }

    /// The named test: a product name too long for the paper becomes two lines,
    /// not one that runs off the edge.
    #[test]
    fn layout_wraps_long_arabic_names() {
        let faces = faces();
        let profile = PrinterProfile::Mm80;
        let style = TextStyle {
            px_per_em: BODY_PX_PER_EM,
            weight: Weight::Regular,
        };
        let max_units = px_to_units(
            profile.content_width_px(),
            style.px_per_em,
            faces.units_per_em,
        );

        let short = "خبز عربي";
        assert_eq!(
            wrap(&faces, short, Weight::Regular, true, max_units).len(),
            1
        );

        let long = "خبز عربي طازج مخبوز على الحجر مع بذور السمسم والحبة السوداء من مخبز الحي";
        let lines = wrap(&faces, long, Weight::Regular, true, max_units);
        assert!(lines.len() > 1, "a long name must wrap, got {lines:?}");

        // Every produced line fits, and nothing was dropped.
        for line in &lines {
            let width = shape_line(&faces, line, Weight::Regular, true).width_units;
            assert!(
                width <= max_units,
                "{line:?} is {width} units, over {max_units}"
            );
        }
        assert_eq!(lines.join(" "), long, "wrapping lost or reordered words");
    }

    /// A word with no space in it that is wider than the measure is **broken**,
    /// not left to overhang and not cut.
    ///
    /// Found by `prop_no_glyph_is_placed_outside_the_page`, which put a glyph
    /// at `x = -48`: the fiscal block's 36-character UUID has no space in it,
    /// does not fit a 58 mm line, and was being centred — so it hung off both
    /// edges, and the dots past the paper are not clipped, they are never
    /// drawn. Every character survives here, in order.
    #[test]
    fn an_overlong_word_is_broken_rather_than_lost() {
        let faces = faces();
        let uuid = "9f1c0f7a-0000-4000-8000-000000000000";
        let max = px_to_units(
            PrinterProfile::Mm58.content_width_px(),
            BODY_PX_PER_EM,
            faces.units_per_em,
        );

        let pieces = wrap(&faces, uuid, Weight::Regular, false, max);
        assert!(pieces.len() > 1, "the UUID must break, got {pieces:?}");
        assert_eq!(pieces.concat(), uuid, "breaking lost characters");
        for piece in &pieces {
            let width = shape_line(&faces, piece, Weight::Regular, false).width_units;
            assert!(width <= max, "{piece:?} is {width} units, over {max}");
        }
    }

    /// A measure too small for any character still terminates, one character
    /// per line, rather than looping on an empty piece.
    #[test]
    fn a_measure_narrower_than_one_character_still_terminates() {
        let faces = faces();
        let pieces = wrap(&faces, "ABC", Weight::Regular, false, 1);
        assert_eq!(pieces, vec!["A", "B", "C"]);
    }

    #[test]
    fn an_empty_line_still_occupies_its_row() {
        let faces = faces();
        assert_eq!(
            wrap(&faces, "", Weight::Regular, true, 10_000),
            vec![String::new()]
        );
        assert_eq!(
            wrap(&faces, "   ", Weight::Regular, true, 10_000),
            vec![String::new()]
        );
        assert_eq!(shape_line(&faces, "", Weight::Regular, true).width_units, 0);
    }

    /// The named test: a totals row puts its label at the logical start and its
    /// amount at the logical end, and those are opposite physical sides in the
    /// two languages.
    #[test]
    fn layout_columns_align_in_rtl() {
        let profile = PrinterProfile::Mm80;
        let content = profile.content_width_px();
        let margin = profile.margin_px();

        // A 100-dot label and a 60-dot amount on a 552-dot content width.
        let label_rtl = start_x_px(Alignment::Start, true, 100, content, margin);
        let amount_rtl = start_x_px(Alignment::End, true, 60, content, margin);
        let label_ltr = start_x_px(Alignment::Start, false, 100, content, margin);
        let amount_ltr = start_x_px(Alignment::End, false, 60, content, margin);

        // Arabic: the label is at the right, the amount at the left.
        assert!(
            label_rtl > amount_rtl,
            "RTL label must sit right of the amount"
        );
        // English: the other way round.
        assert!(
            label_ltr < amount_ltr,
            "LTR label must sit left of the amount"
        );

        // Both stay inside the margins, whichever way round they are.
        for (x, width) in [
            (label_rtl, 100),
            (amount_rtl, 60),
            (label_ltr, 100),
            (amount_ltr, 60),
        ] {
            assert!(x >= margin, "{x} starts inside the margin");
            assert!(
                x + width <= margin + content,
                "{x}+{width} runs past the content box"
            );
        }

        // A centred line is centred, not started.
        let centred = start_x_px(Alignment::Center, true, 100, content, margin);
        assert_eq!(centred, margin + (content - 100) / 2);
    }

    /// A line wider than the box never starts off the page.
    ///
    /// Wrapping makes this unreachable through `lay_out`, which is why the
    /// sweep's "remove the floor" mutation survived: the guard is defence in
    /// depth against a single character wider than the paper, and defence in
    /// depth still has to be proved. `prop_no_glyph_is_placed_outside_the_page`
    /// found the original defect — a 36-character fiscal UUID centred on 58 mm
    /// paper started at **x = -48** — and the fix was to break the word; this
    /// is the second line of defence behind it.
    #[test]
    fn a_line_wider_than_the_page_still_starts_on_it() {
        let profile = PrinterProfile::Mm58;
        let content = profile.content_width_px();
        let margin = profile.margin_px();

        for alignment in [Alignment::Start, Alignment::End, Alignment::Center] {
            for rtl in [true, false] {
                let x = start_x_px(alignment, rtl, content * 3, content, margin);
                assert_eq!(
                    x, margin,
                    "{alignment:?}/{rtl} placed an over-wide line at {x}"
                );
            }
        }
    }

    /// Whether any row of the page sets `text`.
    ///
    /// Glyph ids, not characters: a laid-out page has no text left on it, so
    /// the only honest way to ask "does this appear" is to shape the expected
    /// string and look for its glyphs. Shaping the needle the same way the page
    /// shaped the haystack is what makes the comparison meaningful — the same
    /// contextual forms, the same direction.
    fn page_sets(layout: &Layout, faces: &Faces<'_>, text: &str, weight: Weight) -> bool {
        let rtl = matches!(layout.direction, TextDirection::RightToLeft);
        let needle: Vec<u16> = shape_line(faces, text, weight, rtl)
            .glyphs
            .iter()
            .map(|g| g.glyph_id)
            .collect();
        if needle.is_empty() {
            return false;
        }
        layout.lines.iter().any(|line| {
            let row: Vec<u16> = line.glyphs.iter().map(|g| g.glyph_id).collect();
            row.windows(needle.len()).any(|w| w == needle.as_slice())
        })
    }

    /// §2.3's line is *"name · qty × unit · line total"*, and an earlier draft
    /// printed only the name.
    ///
    /// A receipt that says what was bought and not how many or for how much is
    /// not a proof of purchase. Found by reading the anatomy against the
    /// layout rather than by mutating the layout — no mutation of code that was
    /// never written can fail.
    #[test]
    fn a_line_prints_its_quantity_and_its_total() {
        let faces = faces();
        let m = model();
        let layout = lay_out(&m, PrinterProfile::Mm80).expect("lays out");

        let line = m.lines.first().expect("the fixture has a line");
        assert!(page_sets(&layout, &faces, &line.name, Weight::Regular));
        assert!(
            page_sets(&layout, &faces, "2 × 0.400", Weight::Regular),
            "the quantity and unit price are missing"
        );
        assert!(
            page_sets(&layout, &faces, "0.800", Weight::Regular),
            "the line total is missing"
        );
    }

    /// The totals block carries words, in the document's own language.
    ///
    /// An earlier draft placed every amount against an empty label, so the
    /// block was a column of bare numbers. The words are this crate's, not the
    /// screen's catalogue: paper is read by a customer and a tax authority
    /// rather than by a user interface.
    #[test]
    fn the_totals_block_is_labelled_in_the_documents_language() {
        let faces = faces();

        for (language, direction, subtotal, total) in [
            (
                Language::Arabic,
                TextDirection::RightToLeft,
                "المجموع الفرعي",
                "الإجمالي",
            ),
            (
                Language::English,
                TextDirection::LeftToRight,
                "Subtotal",
                "TOTAL",
            ),
        ] {
            let mut m = model();
            m.locale = ReceiptLocale::new(language);
            assert_eq!(m.locale.direction, direction);

            let layout = lay_out(&m, PrinterProfile::Mm80).expect("lays out");
            assert!(
                page_sets(&layout, &faces, subtotal, Weight::Regular),
                "{language:?} receipt has no {subtotal:?} label"
            );
            // The total is set in the large bold face, so it is looked for in
            // that weight — a needle shaped in the wrong face finds nothing,
            // which is itself the assertion that the total stands out.
            assert!(
                page_sets(&layout, &faces, total, Weight::Bold),
                "{language:?} receipt has no {total:?} label"
            );
        }
    }

    /// §2.3 asks for *"tax summary BY RATE: net / tax / gross per rate"* — all
    /// three, not the tax alone.
    ///
    /// This is the row a bi-monthly return is reconstructed from, and a return
    /// needs the base an amount was charged on. Printing the tax three times
    /// survived every other test, because nothing looked inside the triple.
    #[test]
    fn the_tax_summary_prints_net_tax_and_gross_per_rate() {
        let faces = faces();
        let m = model();
        let layout = lay_out(&m, PrinterProfile::Mm80).expect("lays out");

        let row = m
            .totals
            .tax_summary
            .first()
            .expect("the fixture has a rate");
        assert_ne!(row.net, row.tax, "the fixture cannot tell the three apart");
        assert_ne!(row.tax, row.gross);

        assert!(
            page_sets(
                &layout,
                &faces,
                &format!(
                    "{} / {} / {}",
                    row.net.format_exact(),
                    row.tax.format_exact(),
                    row.gross.format_exact()
                ),
                Weight::Regular
            ),
            "the per-rate row does not print net / tax / gross"
        );
        // And the rate itself, because two components at different rates are
        // indistinguishable without it.
        assert!(page_sets(
            &layout,
            &faces,
            &row.rate.format(),
            Weight::Regular
        ));
    }

    /// §2.3 asks for *"date & time"*, and an earlier draft printed neither.
    ///
    /// Both the instant and the business date, because they differ for a sale
    /// after midnight (conventions §11) and a customer returning goods and an
    /// auditor placing the document in a period need different ones.
    #[test]
    fn a_receipt_prints_its_date_and_its_business_date() {
        let faces = faces();
        let m = model();
        let layout = lay_out(&m, PrinterProfile::Mm80).expect("lays out");

        assert!(
            page_sets(
                &layout,
                &faces,
                &m.header.issued_at.to_iso8601(),
                Weight::Regular
            ),
            "the receipt does not say when it was issued"
        );
        assert!(
            page_sets(
                &layout,
                &faces,
                &m.header.business_date.to_string(),
                Weight::Regular
            ),
            "the receipt does not say which trading day it belongs to"
        );
    }

    /// A label long enough to reach its own amount is wrapped, not printed over
    /// it.
    ///
    /// Nothing in the references says so, because nothing in the references
    /// imagined a discount label written by a merchant. Two runs placed at
    /// opposite ends of a row they do not both fit on overlap, and overlapping
    /// glyphs on a thermal receipt are a smear.
    #[test]
    fn a_long_label_never_collides_with_its_amount() {
        let profile = PrinterProfile::Mm58;
        let mut m = model();
        if let Some(discount) = m
            .lines
            .first_mut()
            .and_then(|line| line.discounts.first_mut())
        {
            discount.label = "عرض الخبز العربي الطازج لكل زبون يشتري أكثر من رغيفين".to_owned();
        }

        let layout = lay_out(&m, profile).expect("lays out");
        let content = profile.content_width_px();
        let margin = profile.margin_px();

        // No row places a glyph outside the content box, and no row is wider
        // than the box — which is the same statement as "nothing overlaps",
        // because a collision is two runs sharing a row wider than it is.
        for line in &layout.lines {
            for glyph in &line.glyphs {
                assert!(
                    glyph.x_px >= margin && glyph.x_px < margin + content,
                    "a glyph at {} is outside [{margin}, {})",
                    glyph.x_px,
                    margin + content
                );
            }
        }
    }

    /// The whole document, both ways round.
    #[test]
    fn a_full_receipt_lays_out_in_both_directions() {
        for (direction, rtl) in [
            (TextDirection::RightToLeft, true),
            (TextDirection::LeftToRight, false),
        ] {
            let mut m = model();
            m.locale.direction = direction;
            let layout = lay_out(&m, PrinterProfile::Mm80).expect("lays out");

            assert_eq!(layout.direction, direction);
            assert!(!layout.lines.is_empty());
            assert!(!layout.rules.is_empty(), "the anatomy prints separators");
            assert!(layout.height_px > 0);

            let glyphs: usize = layout.lines.iter().map(|l| l.glyphs.len()).sum();
            assert!(glyphs > 20, "{rtl} receipt drew only {glyphs} glyphs");
        }
    }

    /// Baselines only ever move down the page.
    #[test]
    fn lines_are_laid_out_top_to_bottom() {
        let layout = lay_out(&model(), PrinterProfile::Mm80).expect("lays out");
        let baselines: Vec<i32> = layout.lines.iter().map(|l| l.baseline_y_px).collect();
        // **Strictly** increasing, not merely sorted. `is_sorted` alone let the
        // sweep's "lines never advance" mutation through: with the text rows
        // all sharing one baseline the two-column rows still advanced, so the
        // sequence stayed sorted while every paragraph printed on top of the
        // one before it. Each `LayoutLine` is one row of the page, so no two
        // may share a baseline.
        assert!(
            baselines.windows(2).all(|pair| match pair {
                [a, b] => a < b,
                _ => true,
            }),
            "two rows share a baseline: {baselines:?}"
        );
    }

    /// Integer arithmetic, asserted rather than trusted.
    ///
    /// `float_arithmetic` is `forbid`, so a float in this module is a compile
    /// error and this test cannot be what defends that. What it *can* check is
    /// the consequence the lint is there for: the conversion is exact integer
    /// division, so it is reproducible on any machine — which is what 1.7.5's
    /// byte-stable goldens rest on.
    #[test]
    fn layout_advances_are_computed_without_floating_point() {
        assert_eq!(units_to_px(1_000, 24, 1_000), 24);
        assert_eq!(units_to_px(500, 24, 1_000), 12);
        // Truncating, not rounding: 1 unit at 24/1000 is 0 dots, both times.
        assert_eq!(units_to_px(1, 24, 1_000), 0);
        assert_eq!(units_to_px(41, 24, 1_000), 0);
        assert_eq!(units_to_px(42, 24, 1_000), 1);
        // A face with a different em. IBM Plex Sans Arabic is 1 000 units, so
        // every case above is also what a body that ignored `units_per_em` and
        // divided by a literal 1 000 would produce — which is precisely the
        // mutation that survived the first sweep. 2 048 is the other common
        // value and the one that tells them apart.
        assert_eq!(units_to_px(2_048, 24, 2_048), 24);
        assert_eq!(units_to_px(1_024, 24, 2_048), 12);
        assert_eq!(px_to_units(24, 24, 2_048), 2_048);
        // A corrupt face divides by zero without dividing by zero.
        assert_eq!(units_to_px(1_000, 24, 0), 0);
        assert_eq!(px_to_units(24, 24, 1_000), 1_000);
        assert_eq!(px_to_units(24, 0, 1_000), 0);
    }
}
