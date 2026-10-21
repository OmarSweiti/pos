//! Turning a page of dots into a printable document (microsteps 1.7.4, 1.7.9).
//!
//! `1.7.3` stopped at a [`Bitmap`] that is 1 bit per pixel, MSB first and
//! row-major — which is exactly the layout both raster commands take — so this
//! module is headers and copies rather than a re-encode. The stream it produces
//! is what `receipt_artifact.content_bytes` stores and what a reprint replays
//! byte for byte.
//!
//! ```text
//! ESC @            1B 40                      initialise
//!   per strip, by the profile's raster command:
//! GS  v 0          1D 76 30 00 xL xH yL yH    raster, normal size
//!                  <the strip's own rows>
//!   or
//! GS  ( L  fn 112  1D 28 4C pL pH 30 70 30 01 01 31 xL xH yL yH
//!                  <the strip's own rows>     store in the graphics buffer
//! GS  ( L  fn 50   1D 28 4C 02 00 30 32       print the buffer
//! GS  V            1D 56 01                   partial cut, once, last
//! ```
//!
//! # The drawer pulse is not here, and it never will be
//!
//! `ref/hardware-and-receipts.md` §4 separates them and says why in one
//! sentence: *"the stream is persisted and retried; the pulse is a physical,
//! non-idempotent, cash-access effect. Retrying a stream that contains one
//! opens the drawer again, with no cashier action, no audit entry and nobody
//! watching."* The **cut** stays, because cutting twice wastes a few
//! millimetres of paper and cutting is what makes the document a document.
//!
//! `ReceiptPrinter::open_drawer` is the other call, made once per authorised
//! opening by the checkout or no-sale flow and never by a print worker.
//!
//! # Strips, at the device's limit (1.7.9)
//!
//! 1.7.4 emitted one `GS v 0`, because no device had been held and the band
//! size is a device fact rather than a code fact. The device now has a number:
//! Epson's ESC/POS reference limits one `GS v 0` to **1–2303 dot rows** on the
//! TM-T20III, TM-T20IV and TM-m30III, and to 1–4095 on the TM-T88VII — so a
//! long basket is a stream a TM-T20III will not take whole (case E.93). The
//! limit is [`PrinterProfile::raster_max_rows`], and a page taller than it
//! becomes consecutive commands, each a whole number of rows and none taller
//! than the limit, whose payloads joined are the bitmap's bytes unchanged — so
//! the seam cannot show on paper.
//!
//! **`GS ( L` adds a limit of its own.** Its parameter count `(pL + pH × 256)`
//! is `10 + k` in a 16-bit field, so one function-112 store carries at most
//! 65 525 data bytes: 910 rows at 576 dots, 1365 at 384, whatever the device
//! would take. A `GS ( L` strip is the smaller of the two. An independent
//! emulator printed the 80 mm receipt in 900-row `GS ( L` strips dot-identical
//! to our bitmap (`ref/hardware-and-receipts.md` §6b.8).
//!
//! **An unknown limit refuses the document.** A profile that does not know its
//! limit cannot be bound to a register, for the reason §6a.1 refuses a referent
//! nobody measured — and the emitter cannot size a strip without it.
//!
//! # The cut carries no feed, and that is an open question rather than a choice
//!
//! On many thermal printers the cutter sits ten to fifteen millimetres above
//! the print head, so a bare `GS V` severs the paper at a point the document
//! has not reached and the last lines stay inside the machine. The usual
//! answer is to feed first — `ESC d n`, or `GS V 66 n`, which feeds and cuts
//! in one command — and **the distance is a property of the device**, which is
//! why §6a makes `cut_command` a profile field beside `raster_command`.
//!
//! Some devices feed to the cutter on a bare `GS V` and some do not. Choosing a
//! feed distance with no printer to measure is the same defect as choosing a
//! band size against none, so this emits the baseline and the question goes on
//! the record: **§9's hardware-lab checklist has no check that the cut lands
//! below the footer.** Check 2 covers truncation across the paper, not along
//! it. Whoever qualifies the first printer adds both the feed and the check.

use core::ops::Range;

use crate::profile::{PrinterProfile, ProfileError, RasterCommand};
use crate::render::Bitmap;

/// `ESC @` — initialise. Clears any mode a previous document left behind, so a
/// receipt does not inherit the double-width setting of the one before it.
pub const INIT: [u8; 2] = [0x1B, 0x40];

/// `GS V 1` — partial cut, which leaves a small tab so the receipt hangs rather
/// than dropping on the floor. The alternative, `GS V 0`, is a full cut; which
/// one a device honours is a profile field (§6a) and this is the baseline.
pub const CUT: [u8; 3] = [0x1D, 0x56, 0x01];

/// `GS v 0` with `m = 0` — raster bit image, normal width and height. The four
/// dimension bytes follow.
pub const RASTER: [u8; 4] = [0x1D, 0x76, 0x30, 0x00];

/// `GS ( L` — the graphics function prefix. Two parameter-count bytes follow.
pub const GRAPHICS: [u8; 3] = [0x1D, 0x28, 0x4C];

/// Function 112's fixed parameters after the count: `m = 48`, `fn = 112`,
/// `a = 48` (monochrome), `bx = by = 1` (normal size), `c = 49` (first colour).
/// The four dimension bytes — width and height in **dots** — follow.
pub const GRAPHICS_STORE: [u8; 6] = [0x30, 0x70, 0x30, 0x01, 0x01, 0x31];

/// `GS ( L` function 50 — print the graphics buffer.
pub const GRAPHICS_PRINT: [u8; 7] = [0x1D, 0x28, 0x4C, 0x02, 0x00, 0x30, 0x32];

/// `ESC p` — the drawer pulse prefix. Declared here so a test can assert its
/// **absence**, and for no other reason: nothing in this module emits it.
pub const DRAWER_PULSE_PREFIX: [u8; 2] = [0x1B, 0x70];

/// The largest value a 16-bit field can carry.
///
/// `GS v 0` takes both dimensions as little-endian `u16`, and `GS ( L` its
/// dimensions and its parameter count. Neither dimension limit is reachable
/// today — the two paper widths are 72 and 48 bytes per row, and
/// `render::MAX_PAGE_HEIGHT_PX` is 32 768 — which is why they are tested
/// directly rather than through a page nothing can produce.
pub const MAX_DIMENSION: u32 = u16::MAX as u32;

/// Function 112's parameters before its data: `m fn a bx by c xL xH yL yH`.
const GRAPHICS_STORE_FIXED: u32 = 10;

/// The most data one function-112 store can carry: its 16-bit parameter count
/// is `10 + k`.
pub const MAX_GRAPHICS_DATA: u32 = MAX_DIMENSION - GRAPHICS_STORE_FIXED;

/// Why a page could not become a document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EscPosError {
    /// More bytes across than the header can name.
    #[error("a page {row_bytes} bytes across cannot be named in a 16-bit header")]
    TooWide { row_bytes: u32 },
    /// More lines down than the header can name.
    ///
    /// Pages are sent in strips, but a page's height still has to fit the
    /// 16-bit field it is measured against: `render::MAX_PAGE_HEIGHT_PX` is
    /// well below it, so a page past it is a runaway, not a receipt.
    #[error("a page {height_px} dots tall cannot be named in a 16-bit header")]
    TooTall { height_px: u32 },
    /// A page with no lines at all.
    ///
    /// Refused rather than emitted: the stream would be an initialise and a cut
    /// with nothing between them, which spends paper and a cutter cycle to hand
    /// a customer a blank tab. `render_receipt` cannot produce one — a page is
    /// at least its margins — so reaching here is a caller's bug, and a loud
    /// one is cheaper than a quiet one.
    #[error("a page with no lines is not a document")]
    Empty,
    /// The bitmap's buffer does not hold the page it claims to be.
    ///
    /// Refused rather than padded or truncated: a short buffer printed as a
    /// full page is a receipt whose bottom is missing with nothing to say so,
    /// and this is the last place that can still tell.
    #[error("a {width_px}x{height_px} page needs {expected} bytes and carries {found}")]
    Truncated {
        width_px: u32,
        height_px: u32,
        expected: usize,
        found: usize,
    },
    /// A page rendered for one paper, sent to a device of another.
    ///
    /// The width has one source — the bitmap — and the profile's `dot_width` is
    /// a check on it rather than a second answer. Printed anyway, a 384-dot page
    /// on a 576-dot head sits off-centre and a 576-dot page on a 384-dot head
    /// loses its right edge — where an Arabic receipt keeps its labels.
    #[error("profile {profile_id} prints {profile_px} dots across, the page is {page_px}")]
    WidthMismatch {
        profile_id: String,
        page_px: u32,
        profile_px: u32,
    },
    /// The profile does not know a fact the emitter needs: its width, its
    /// raster command, or how many rows one command carries.
    #[error(transparent)]
    Profile(#[from] ProfileError),
}

/// One raster command's share of the page: whole rows, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Strip {
    first_row: u32,
    rows: u32,
}

/// Everything about a document except its bytes, decided once so the emitter
/// and [`payload_ranges`] cannot disagree about where a strip begins.
struct Plan {
    command: RasterCommand,
    row_bytes: u32,
    strips: Vec<Strip>,
}

/// Check a page against its profile, and cut it into strips.
fn plan(page: &Bitmap, profile: &PrinterProfile) -> Result<Plan, EscPosError> {
    if page.height_px == 0 || page.width_px == 0 {
        return Err(EscPosError::Empty);
    }
    let profile_px = profile.dots_per_line()?.get();
    if page.width_px != profile_px {
        return Err(EscPosError::WidthMismatch {
            profile_id: profile.profile_id.clone(),
            page_px: page.width_px,
            profile_px,
        });
    }

    let command = profile.raster_framing()?;
    let row_bytes = page.row_bytes();
    let width_field = match command {
        RasterCommand::GsV0 => row_bytes,
        RasterCommand::GsParenL => page.width_px,
    };
    if width_field > MAX_DIMENSION {
        return Err(EscPosError::TooWide { row_bytes });
    }
    if page.height_px > MAX_DIMENSION {
        return Err(EscPosError::TooTall {
            height_px: page.height_px,
        });
    }

    let expected = (row_bytes as usize).saturating_mul(page.height_px as usize);
    if page.bits.len() != expected {
        return Err(EscPosError::Truncated {
            width_px: page.width_px,
            height_px: page.height_px,
            expected,
            found: page.bits.len(),
        });
    }

    // The device's limit, and the command's own: `GS v 0` names a strip's
    // height in 16 bits, and `GS ( L` names its whole payload in 16 bits.
    let device_rows = profile.row_limit()?.get();
    let command_rows = match command {
        RasterCommand::GsV0 => MAX_DIMENSION,
        RasterCommand::GsParenL => MAX_GRAPHICS_DATA / row_bytes.max(1),
    };
    let per_strip = device_rows.min(command_rows);
    if per_strip == 0 {
        // A single row is more data than one `GS ( L` store can carry.
        return Err(EscPosError::TooWide { row_bytes });
    }

    let mut strips = Vec::new();
    let mut first_row = 0;
    while first_row < page.height_px {
        let rows = per_strip.min(page.height_px - first_row);
        strips.push(Strip { first_row, rows });
        first_row += rows;
    }
    Ok(Plan {
        command,
        row_bytes,
        strips,
    })
}

/// The bytes before a strip's payload, for the profile's raster command.
fn strip_header(command: RasterCommand, page: &Bitmap, row_bytes: u32, strip: Strip) -> Vec<u8> {
    match command {
        RasterCommand::GsV0 => {
            let mut header = RASTER.to_vec();
            header.extend_from_slice(&little_endian(row_bytes));
            header.extend_from_slice(&little_endian(strip.rows));
            header
        }
        RasterCommand::GsParenL => {
            // `plan` bounded the payload to MAX_GRAPHICS_DATA, so this sum fits
            // the 16-bit count.
            let count = GRAPHICS_STORE_FIXED + row_bytes.saturating_mul(strip.rows);
            let mut header = GRAPHICS.to_vec();
            header.extend_from_slice(&little_endian(count));
            header.extend_from_slice(&GRAPHICS_STORE);
            header.extend_from_slice(&little_endian(page.width_px));
            header.extend_from_slice(&little_endian(strip.rows));
            header
        }
    }
}

/// The bytes after a strip's payload.
const fn strip_trailer(command: RasterCommand) -> &'static [u8] {
    match command {
        RasterCommand::GsV0 => &[],
        RasterCommand::GsParenL => &GRAPHICS_PRINT,
    }
}

/// A strip's rows within the bitmap's buffer.
fn strip_rows(row_bytes: u32, strip: Strip) -> Range<usize> {
    let start = (row_bytes as usize).saturating_mul(strip.first_row as usize);
    start..start.saturating_add((row_bytes as usize).saturating_mul(strip.rows as usize))
}

/// Encode one page as an ESC/POS document for one device.
///
/// The bitmap is copied verbatim, a strip at a time. `1.7.3` produces
/// MSB-first, row-major bits at the paper's width for precisely this reason, so
/// nothing here shifts, pads or reorders a byte — and a transposition bug in
/// the printed output can therefore only be a bug in the rasteriser.
///
/// # Errors
///
/// Refuses a page with no dots, a page whose width disagrees with the
/// profile's, a page either of whose dimensions exceeds what its command can
/// name, a bitmap whose buffer does not hold the page it describes, and a
/// profile that does not know its width, its raster command, or how many rows
/// one raster command carries.
pub fn encode(page: &Bitmap, profile: &PrinterProfile) -> Result<Vec<u8>, EscPosError> {
    let plan = plan(page, profile)?;
    let command = plan.command;

    let mut out = Vec::with_capacity(
        INIT.len()
            + CUT.len()
            + page.bits.len()
            + plan.strips.len()
                * (GRAPHICS.len() + 2 + GRAPHICS_STORE.len() + 4 + GRAPHICS_PRINT.len()),
    );
    out.extend_from_slice(&INIT);
    for strip in &plan.strips {
        let range = strip_rows(plan.row_bytes, *strip);
        let rows = page.bits.get(range).ok_or(EscPosError::Truncated {
            width_px: page.width_px,
            height_px: page.height_px,
            expected: (plan.row_bytes as usize).saturating_mul(page.height_px as usize),
            found: page.bits.len(),
        })?;
        out.extend_from_slice(&strip_header(command, page, plan.row_bytes, *strip));
        out.extend_from_slice(rows);
        out.extend_from_slice(strip_trailer(command));
    }
    out.extend_from_slice(&CUT);
    Ok(out)
}

/// A dimension or count as the two little-endian bytes ESC/POS wants.
///
/// The caller has already refused anything past [`MAX_DIMENSION`], so the
/// truncation here cannot lose a bit — which is why this is a `const fn` taking
/// a checked value rather than a fallible conversion repeated at every call
/// site.
const fn little_endian(value: u32) -> [u8; 2] {
    [(value & 0xFF) as u8, ((value >> 8) & 0xFF) as u8]
}

/// Where each strip's raster payload sits inside an encoded stream, in order.
///
/// Exported because the interesting assertions about a document are about what
/// is *outside* the payloads — see the module tests and
/// `receipt_retry_never_repeats_the_drawer_pulse`. A caller that wants to check
/// a stream should not have to re-derive this arithmetic and get it wrong, so
/// it comes from the same plan the emitter follows.
///
/// # Errors
///
/// Exactly [`encode`]'s, because it is the same plan.
pub fn payload_ranges(
    page: &Bitmap,
    profile: &PrinterProfile,
) -> Result<Vec<Range<usize>>, EscPosError> {
    let plan = plan(page, profile)?;
    let command = plan.command;
    let mut at = INIT.len();
    let mut ranges = Vec::with_capacity(plan.strips.len());
    for strip in &plan.strips {
        at += strip_header(command, page, plan.row_bytes, *strip).len();
        let len = strip_rows(plan.row_bytes, *strip).len();
        ranges.push(at..at + len);
        at += len + strip_trailer(command).len();
    }
    Ok(ranges)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use core::num::NonZeroU32;

    use super::*;
    use crate::profile::{Fact, Parameter};
    use crate::render::tests_support::model;
    use crate::render::{PaperWidth, render_receipt};
    use crate::{
        FailReason, PrintOutcome, PrinterStatus, ReceiptPrinter, RenderedReceipt, SimulatedPrinter,
    };
    use pos_test_support::io_proptest_config;
    use proptest::prelude::*;

    /// A page of `height` rows at `width` dots, filled with `fill`.
    fn page(width_px: u32, height_px: u32, fill: u8) -> Bitmap {
        let row_bytes = width_px.div_ceil(8);
        Bitmap {
            width_px,
            height_px,
            bits: vec![fill; (row_bytes * height_px) as usize],
        }
    }

    /// A page whose every byte differs and no two rows agree, so a shift, a
    /// reversal, a dropped row or a row moved across a strip boundary all show.
    fn patterned(width_px: u32, height_px: u32) -> Bitmap {
        let mut bitmap = page(width_px, height_px, 0);
        for (index, byte) in bitmap.bits.iter_mut().enumerate() {
            *byte = u8::try_from(index % 251)
                .unwrap_or(0)
                .wrapping_mul(37)
                .wrapping_add(11);
        }
        bitmap
    }

    /// The simulator's profile at an arbitrary width, with a chosen limit and
    /// command — what every example below binds.
    fn profile(width_px: u32, max_rows: u32, command: RasterCommand) -> PrinterProfile {
        let mut profile = PrinterProfile::simulator(PaperWidth::Mm80);
        profile.dot_width = Fact::Known(NonZeroU32::new(width_px).unwrap());
        profile.raster_max_rows = Fact::Known(NonZeroU32::new(max_rows).unwrap());
        profile.raster_command = Fact::Known(command);
        profile
    }

    /// One strip, as 1.7.4 emitted for every page: the limit is never reached.
    fn one_strip(width_px: u32) -> PrinterProfile {
        profile(width_px, MAX_DIMENSION, RasterCommand::GsV0)
    }

    /// Every payload, joined in order.
    fn joined(stream: &[u8], ranges: &[Range<usize>]) -> Vec<u8> {
        ranges
            .iter()
            .flat_map(|range| stream.get(range.clone()).unwrap().iter().copied())
            .collect()
    }

    /// Whether `ESC p` appears anywhere that is not a raster payload.
    ///
    /// Scanning the **whole** stream would be the wrong test and would be
    /// intermittently red: a payload is an arbitrary bitmap, so `1B 70` occurs
    /// inside one by coincidence about once in every 65 536 byte pairs — which
    /// a 576-dot receipt reaches within roughly 900 rows.
    /// `a_bitmap_containing_the_pulse_bytes_is_still_a_document` is that case
    /// made deliberate.
    fn pulse_outside_payloads(stream: &[u8], payloads: &[Range<usize>]) -> bool {
        let inside = |at: usize| payloads.iter().any(|range| range.contains(&at));
        stream
            .windows(DRAWER_PULSE_PREFIX.len())
            .enumerate()
            .any(|(at, pair)| {
                pair == DRAWER_PULSE_PREFIX
                    && !(inside(at) && inside(at + DRAWER_PULSE_PREFIX.len() - 1))
            })
    }

    /// How many raster commands a stream carries, read from the stream itself
    /// rather than from the plan that produced it.
    fn commands_in(stream: &[u8], payloads: &[Range<usize>], prefix: &[u8]) -> usize {
        let outside = |at: usize| !payloads.iter().any(|range| range.contains(&at));
        stream
            .windows(prefix.len())
            .enumerate()
            .filter(|(at, window)| *window == prefix && outside(*at))
            .count()
    }

    /// The named test: the fixed bytes are the fixed bytes.
    ///
    /// Pinned as literals rather than read back from the constants, because the
    /// constants are what a mutation would change. `1B 40` and `1D 56 01` are
    /// ESC/POS's own numbers and they move only if the baseline profile does —
    /// which is a decision, not an edit.
    #[test]
    fn escpos_init_cut_bytes_are_exact() {
        assert_eq!(INIT, [0x1B, 0x40], "ESC @");
        assert_eq!(CUT, [0x1D, 0x56, 0x01], "GS V 1");
        assert_eq!(RASTER, [0x1D, 0x76, 0x30, 0x00], "GS v 0, m = 0");
        assert_eq!(DRAWER_PULSE_PREFIX, [0x1B, 0x70], "ESC p");
        assert_eq!(GRAPHICS, [0x1D, 0x28, 0x4C], "GS ( L");
        assert_eq!(
            GRAPHICS_STORE,
            [0x30, 0x70, 0x30, 0x01, 0x01, 0x31],
            "m 48, fn 112, monochrome, 1×1, first colour"
        );
        assert_eq!(
            GRAPHICS_PRINT,
            [0x1D, 0x28, 0x4C, 0x02, 0x00, 0x30, 0x32],
            "GS ( L fn 50"
        );
        assert_eq!(MAX_GRAPHICS_DATA, 65_525);

        let stream = encode(&page(64, 2, 0xFF), &one_strip(64)).expect("encodes");
        assert!(stream.starts_with(&INIT), "a document begins with ESC @");
        assert!(stream.ends_with(&CUT), "a document ends with a cut");
    }

    /// The named test, and the reason the pulse left the byte stream.
    ///
    /// Two halves, and the second is the one that matters. A stream is emitted
    /// and sent **twice**, as a retried print job does; the simulator's drawer
    /// counter stays at zero, because nothing in the document can move it. And
    /// the pulse's two bytes appear nowhere outside the raster payloads, which
    /// is where a document could otherwise smuggle one.
    #[test]
    fn receipt_retry_never_repeats_the_drawer_pulse() {
        let bitmap = render_receipt(&model(), PaperWidth::Mm80).expect("renders");
        let profile = PrinterProfile::simulator(PaperWidth::Mm80);
        let stream = encode(&bitmap, &profile).expect("encodes");

        let printer = SimulatedPrinter::new();
        let doc = RenderedReceipt {
            bytes: stream.clone(),
        };
        assert_eq!(printer.print(&doc), PrintOutcome::Printed);
        assert_eq!(
            printer.print(&doc),
            PrintOutcome::Printed,
            "prints again — this is the retry"
        );

        assert_eq!(
            printer.printed.lock().unwrap().len(),
            2,
            "both attempts ran"
        );
        assert_eq!(
            *printer.drawer_opens.lock().unwrap(),
            0,
            "a retried document opened the drawer"
        );

        // And the bytes could not have, whatever a driver did with them.
        let payloads = payload_ranges(&bitmap, &profile).expect("plans");
        assert!(!pulse_outside_payloads(&stream, &payloads));
    }

    #[test]
    fn a_stream_is_exactly_init_raster_and_cut() {
        let bitmap = page(64, 3, 0xAA);
        let profile = one_strip(64);
        let stream = encode(&bitmap, &profile).expect("encodes");

        let frame = INIT.len() + RASTER.len() + 4 + CUT.len();
        assert_eq!(
            stream.len(),
            frame + bitmap.bits.len(),
            "no byte is unaccounted for"
        );

        let payloads = payload_ranges(&bitmap, &profile).expect("plans");
        assert_eq!(payloads.len(), 1);
        let only = payloads.first().expect("one strip");
        assert_eq!(only.start, INIT.len() + RASTER.len() + 4);
        assert_eq!(only.end, stream.len() - CUT.len());
        assert_eq!(stream.get(only.clone()), Some(bitmap.bits.as_slice()));
    }

    #[test]
    fn the_raster_header_declares_the_bitmaps_own_dimensions() {
        // 520 dots is 65 bytes across, so the low byte alone would be enough
        // and a byte-swapped header would still look plausible. 300 lines put
        // the height past 255 for the same reason — both fields are asserted
        // where a single-byte confusion is visible.
        let bitmap = page(520, 300, 0);
        let stream = encode(&bitmap, &one_strip(520)).expect("encodes");
        let header = INIT.len() + RASTER.len();

        assert_eq!(bitmap.row_bytes(), 65);
        assert_eq!(stream.get(header), Some(&65), "xL");
        assert_eq!(stream.get(header + 1), Some(&0), "xH");
        assert_eq!(stream.get(header + 2), Some(&44), "yL — 300 is 0x012C");
        assert_eq!(stream.get(header + 3), Some(&1), "yH");
    }

    #[test]
    fn the_raster_payload_is_the_bitmaps_bytes_unchanged() {
        let bitmap = patterned(32, 4);
        let profile = one_strip(32);
        let stream = encode(&bitmap, &profile).expect("encodes");
        let payloads = payload_ranges(&bitmap, &profile).expect("plans");
        assert_eq!(joined(&stream, &payloads), bitmap.bits);
    }

    #[test]
    fn both_profiles_emit_their_own_row_width() {
        for (paper, row_bytes) in [(PaperWidth::Mm80, 72), (PaperWidth::Mm58, 48)] {
            let bitmap = render_receipt(&model(), paper).expect("renders");
            assert_eq!(bitmap.row_bytes(), row_bytes, "{paper:?}");

            let profile = PrinterProfile::simulator(paper);
            let stream = encode(&bitmap, &profile).expect("encodes");
            let header = INIT.len() + RASTER.len();
            assert_eq!(
                stream.get(header).copied(),
                u8::try_from(row_bytes).ok(),
                "{paper:?} declared the wrong width"
            );
            let payloads = payload_ranges(&bitmap, &profile).expect("plans");
            assert_eq!(joined(&stream, &payloads), bitmap.bits, "{paper:?}");
        }
    }

    /// A page whose buffer does not hold the page it claims to be is refused,
    /// not padded and not truncated.
    ///
    /// This is the last place that can still tell. A short buffer printed as a
    /// full page is a receipt whose bottom is missing with nothing to say so.
    #[test]
    fn a_page_whose_rows_do_not_fill_its_buffer_is_refused() {
        let mut short = page(64, 4, 0);
        short.bits.pop();
        assert_eq!(
            encode(&short, &one_strip(64)),
            Err(EscPosError::Truncated {
                width_px: 64,
                height_px: 4,
                expected: 32,
                found: 31,
            })
        );

        let mut long = page(64, 4, 0);
        long.bits.push(0);
        assert!(matches!(
            encode(&long, &one_strip(64)),
            Err(EscPosError::Truncated { .. })
        ));

        // And the exact page encodes.
        assert!(encode(&page(64, 4, 0), &one_strip(64)).is_ok());
    }

    #[test]
    fn a_page_too_wide_for_a_sixteen_bit_header_is_refused() {
        let width_px = (MAX_DIMENSION + 1) * 8;
        let wide = Bitmap {
            width_px,
            height_px: 1,
            bits: Vec::new(),
        };
        assert_eq!(
            encode(&wide, &one_strip(width_px)),
            Err(EscPosError::TooWide {
                row_bytes: MAX_DIMENSION + 1
            })
        );
    }

    #[test]
    fn a_page_too_tall_for_a_sixteen_bit_header_is_refused() {
        let tall = Bitmap {
            width_px: 8,
            height_px: MAX_DIMENSION + 1,
            bits: Vec::new(),
        };
        assert_eq!(
            encode(&tall, &one_strip(8)),
            Err(EscPosError::TooTall {
                height_px: MAX_DIMENSION + 1
            })
        );

        // The largest representable page is accepted as far as its header goes;
        // it fails on its buffer, which is the next guard rather than this one.
        let edge = Bitmap {
            width_px: 8,
            height_px: MAX_DIMENSION,
            bits: Vec::new(),
        };
        assert!(matches!(
            encode(&edge, &one_strip(8)),
            Err(EscPosError::Truncated { .. })
        ));
    }

    /// The case that makes a naive pulse-scan wrong, made deliberate.
    #[test]
    fn a_bitmap_containing_the_pulse_bytes_is_still_a_document() {
        let mut bitmap = page(16, 2, 0);
        // `1B 70` straddling the middle of the payload.
        if let Some(byte) = bitmap.bits.get_mut(1) {
            *byte = 0x1B;
        }
        if let Some(byte) = bitmap.bits.get_mut(2) {
            *byte = 0x70;
        }

        let profile = one_strip(16);
        let stream = encode(&bitmap, &profile).expect("a page is a page whatever its bits say");

        // The naive test would fail here — the bytes really are in the stream.
        assert!(
            stream.windows(2).any(|pair| pair == DRAWER_PULSE_PREFIX),
            "the fixture must actually contain the pulse bytes"
        );
        // The correct one passes: they are inside the payload and nowhere else.
        let payloads = payload_ranges(&bitmap, &profile).expect("plans");
        assert!(!pulse_outside_payloads(&stream, &payloads));
    }

    #[test]
    fn a_page_with_no_lines_is_not_a_document() {
        assert_eq!(
            encode(&page(64, 0, 0), &one_strip(64)),
            Err(EscPosError::Empty)
        );
        assert_eq!(
            encode(&page(0, 4, 0), &one_strip(64)),
            Err(EscPosError::Empty)
        );
        // One line is a document, however little it says.
        assert!(encode(&page(64, 1, 0), &one_strip(64)).is_ok());
    }

    /// The simulator refuses a document when the paper is out, so a retry is a
    /// real retry rather than a second successful print.
    #[test]
    fn a_refused_document_still_leaves_the_drawer_shut() {
        let printer = SimulatedPrinter::new();
        *printer.force_status.lock().unwrap() = PrinterStatus::PaperOut;

        let stream = encode(&page(64, 2, 0xFF), &one_strip(64)).expect("encodes");
        let doc = RenderedReceipt { bytes: stream };
        assert_eq!(
            printer.print(&doc),
            PrintOutcome::Failed(FailReason::PaperOut)
        );

        assert_eq!(*printer.drawer_opens.lock().unwrap(), 0);
        assert!(printer.printed.lock().unwrap().is_empty());
    }

    /// The named test (E.93): one row past the limit is two commands, not one
    /// command a TM-T20III will not take.
    #[test]
    fn a_page_one_row_taller_than_the_limit_becomes_two_commands() {
        let limit = 2303;
        let bitmap = patterned(576, limit + 1);
        let profile = profile(576, limit, RasterCommand::GsV0);
        let stream = encode(&bitmap, &profile).expect("encodes");
        let payloads = payload_ranges(&bitmap, &profile).expect("plans");

        assert_eq!(payloads.len(), 2, "two strips");
        assert_eq!(
            commands_in(&stream, &payloads, &RASTER),
            2,
            "two GS v 0 commands"
        );
        let row_bytes = 72;
        assert_eq!(
            payloads.first().expect("a first strip").len(),
            row_bytes * limit as usize,
            "the first is the limit"
        );
        assert_eq!(
            payloads.get(1).expect("a second strip").len(),
            row_bytes,
            "the second is the one row left"
        );

        // And at exactly the limit it is still one.
        let exact = patterned(576, limit);
        assert_eq!(payload_ranges(&exact, &profile).expect("plans").len(), 1);
    }

    /// The named test: the seam cannot show on paper, because the strips joined
    /// are the bitmap — every byte, in order, with nothing between them that a
    /// printer would draw.
    #[test]
    fn joined_strip_payloads_are_the_bitmap_unchanged() {
        let bitmap = patterned(576, 5_450);
        for command in [RasterCommand::GsV0, RasterCommand::GsParenL] {
            let profile = profile(576, 2303, command);
            let stream = encode(&bitmap, &profile).expect("encodes");
            let payloads = payload_ranges(&bitmap, &profile).expect("plans");
            assert_eq!(joined(&stream, &payloads), bitmap.bits, "{command:?}");
        }
    }

    /// The named test: one initialise first, one cut last, whatever the number
    /// of strips — a cut between strips would hand the customer a receipt in
    /// pieces.
    #[test]
    fn the_cut_appears_once_after_every_strip() {
        let bitmap = patterned(576, 5_450);
        for command in [RasterCommand::GsV0, RasterCommand::GsParenL] {
            let profile = profile(576, 2303, command);
            let stream = encode(&bitmap, &profile).expect("encodes");
            let payloads = payload_ranges(&bitmap, &profile).expect("plans");

            assert!(stream.starts_with(&INIT), "{command:?}");
            assert!(stream.ends_with(&CUT), "{command:?}");
            assert_eq!(
                commands_in(&stream, &payloads, &CUT),
                1,
                "{command:?}: one cut"
            );
            assert_eq!(
                commands_in(&stream, &payloads, &INIT),
                1,
                "{command:?}: one ESC @"
            );
            let last_payload_end = payloads.last().map_or(0, |range| range.end);
            assert!(
                last_payload_end <= stream.len() - CUT.len(),
                "{command:?}: the cut follows the last strip"
            );
        }
    }

    /// The named test: `GS ( L` carries the same rows, framed as store-then-
    /// print, and no strip is taller than the command's own 16-bit count
    /// allows — 910 rows at 576 dots — even when the device would take more.
    #[test]
    fn gs_paren_l_frames_the_same_rows_within_its_own_limit() {
        let bitmap = patterned(576, 2_000);
        let device_allows = profile(576, 4095, RasterCommand::GsParenL);
        let stream = encode(&bitmap, &device_allows).expect("encodes");
        let payloads = payload_ranges(&bitmap, &device_allows).expect("plans");

        assert_eq!(MAX_GRAPHICS_DATA / 72, 910);
        assert_eq!(payloads.len(), 3, "910 + 910 + 180 rows");
        for range in &payloads {
            assert!(
                range.len() <= MAX_GRAPHICS_DATA as usize,
                "a strip past the 16-bit count"
            );
        }
        assert_eq!(
            commands_in(&stream, &payloads, &GRAPHICS_PRINT),
            3,
            "one print per store"
        );
        assert_eq!(joined(&stream, &payloads), bitmap.bits, "the same rows");

        // The first store's header: count = 10 + 910 × 72 = 65 530, width 576,
        // height 910 — all little-endian.
        let header = INIT.len();
        assert_eq!(stream.get(header..header + 3), Some(GRAPHICS.as_slice()));
        assert_eq!(
            stream.get(header + 3..header + 5),
            Some([0xFA, 0xFF].as_slice())
        );
        assert_eq!(
            stream.get(header + 5..header + 11),
            Some(GRAPHICS_STORE.as_slice())
        );
        assert_eq!(
            stream.get(header + 11..header + 13),
            Some([0x40, 0x02].as_slice())
        );
        assert_eq!(
            stream.get(header + 13..header + 15),
            Some([0x8E, 0x03].as_slice())
        );

        // At 384 dots the command allows 1365 rows; a 2303-row device limit
        // still loses to it.
        let narrow = patterned(384, 1_366);
        let narrow_profile = profile(384, 2303, RasterCommand::GsParenL);
        assert_eq!(
            payload_ranges(&narrow, &narrow_profile)
                .expect("plans")
                .len(),
            2
        );
    }

    /// The named test: a page rendered for one paper and sent to a device of
    /// another is refused, not printed off-centre or with its edge cut away.
    #[test]
    fn a_width_disagreeing_with_the_profile_is_refused() {
        let narrow = render_receipt(&model(), PaperWidth::Mm58).expect("renders");
        assert_eq!(
            encode(&narrow, &PrinterProfile::simulator(PaperWidth::Mm80)),
            Err(EscPosError::WidthMismatch {
                profile_id: "simulator-80mm".to_owned(),
                page_px: 384,
                profile_px: 576,
            })
        );
    }

    /// The named test: a profile that does not know its row limit refuses the
    /// document, rather than guessing a band size against no device.
    #[test]
    fn an_unknown_row_limit_refuses_the_document() {
        let mut unknown = PrinterProfile::simulator(PaperWidth::Mm80);
        unknown.raster_max_rows = Fact::Unknown;
        let bitmap = render_receipt(&model(), PaperWidth::Mm80).expect("renders");
        assert_eq!(
            encode(&bitmap, &unknown),
            Err(EscPosError::Profile(ProfileError::Unknown {
                profile_id: "simulator-80mm".to_owned(),
                parameter: Parameter::RasterMaxRows,
            }))
        );
    }

    /// 1.7.11 generalises the rule above: a profile read from a file may leave
    /// any fact unknown, and the emitter refuses rather than guesses the two
    /// others it reads — the width it checks the page against, and the command
    /// that frames the strips.
    #[test]
    fn an_unknown_width_or_framing_refuses_the_document() {
        let bitmap = render_receipt(&model(), PaperWidth::Mm80).expect("renders");

        let mut no_width = PrinterProfile::simulator(PaperWidth::Mm80);
        no_width.dot_width = Fact::Unknown;
        assert_eq!(
            encode(&bitmap, &no_width),
            Err(EscPosError::Profile(ProfileError::Unknown {
                profile_id: "simulator-80mm".to_owned(),
                parameter: Parameter::DotWidth,
            }))
        );

        let mut no_framing = PrinterProfile::simulator(PaperWidth::Mm80);
        no_framing.raster_command = Fact::Unknown;
        assert_eq!(
            payload_ranges(&bitmap, &no_framing),
            Err(EscPosError::Profile(ProfileError::Unknown {
                profile_id: "simulator-80mm".to_owned(),
                parameter: Parameter::RasterCommand,
            }))
        );
    }

    proptest! {
        #![proptest_config(io_proptest_config())]

        /// Encoding is a function.
        ///
        /// `receipt_artifact.content_bytes` stores this stream and a reprint
        /// replays it, so `reprint_is_byte_identical_including_qr` (1.7.7) and
        /// the meaning of `content_hash` both rest on it.
        #[test]
        fn prop_emitting_is_deterministic(
            width_px in 8u32..=640,
            height_px in 1u32..64,
            fill in any::<u8>(),
            limit in 1u32..=80,
            gs_paren_l in any::<bool>(),
        ) {
            let command = if gs_paren_l { RasterCommand::GsParenL } else { RasterCommand::GsV0 };
            let bitmap = page(width_px, height_px, fill);
            let profile = profile(width_px, limit, command);
            prop_assert_eq!(encode(&bitmap, &profile)?, encode(&bitmap, &profile)?);
        }

        /// A stream is the page and a fixed frame, and nothing else (1.7.4).
        ///
        /// Stated as arithmetic rather than as a shape, for a page the limit
        /// leaves whole: the document is exactly thirteen bytes longer than the
        /// bitmap — `ESC @`, the `GS v 0` header and its four dimension bytes,
        /// and the cut — the payload is the bitmap, and the pulse is absent from
        /// every byte that is not the payload. 1.7.9's strips generalise this
        /// below; a page that fits one command is still exactly 1.7.4's document.
        #[test]
        fn prop_the_stream_is_the_page_plus_a_fixed_frame(
            width_px in 8u32..=640,
            height_px in 1u32..64,
            fill in any::<u8>(),
        ) {
            let bitmap = page(width_px, height_px, fill);
            let profile = one_strip(width_px);
            let stream = encode(&bitmap, &profile)?;
            let payloads = payload_ranges(&bitmap, &profile)?;

            prop_assert_eq!(stream.len(), INIT.len() + RASTER.len() + 4 + CUT.len() + bitmap.bits.len());
            prop_assert!(stream.starts_with(&INIT));
            prop_assert!(stream.ends_with(&CUT));
            prop_assert_eq!(payloads.len(), 1);
            prop_assert_eq!(joined(&stream, &payloads), bitmap.bits.clone());
            prop_assert!(!pulse_outside_payloads(&stream, &payloads));
        }

        /// A stream is the page, cut into strips no taller than the limit, and
        /// nothing else: an initialise first, a cut last, every payload a whole
        /// number of rows, the payloads joined the bitmap, and the pulse absent
        /// from every byte that is not a payload.
        #[test]
        fn prop_strips_join_to_the_bitmap_and_respect_the_limit(
            width_px in 8u32..=640,
            height_px in 1u32..300,
            limit in 1u32..=120,
            gs_paren_l in any::<bool>(),
        ) {
            let command = if gs_paren_l { RasterCommand::GsParenL } else { RasterCommand::GsV0 };
            let bitmap = patterned(width_px, height_px);
            let profile = profile(width_px, limit, command);
            let stream = encode(&bitmap, &profile)?;
            let payloads = payload_ranges(&bitmap, &profile)?;
            let row_bytes = bitmap.row_bytes() as usize;

            prop_assert!(stream.starts_with(&INIT));
            prop_assert!(stream.ends_with(&CUT));
            prop_assert_eq!(joined(&stream, &payloads), bitmap.bits.clone());
            for range in &payloads {
                prop_assert_eq!(range.len() % row_bytes, 0);
                prop_assert!(range.len() / row_bytes <= limit as usize);
                prop_assert!(range.len() <= MAX_GRAPHICS_DATA as usize || !gs_paren_l);
            }
            prop_assert!(!pulse_outside_payloads(&stream, &payloads));
        }
    }
}
