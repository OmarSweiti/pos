//! Turning a page of dots into a printable document (microstep 1.7.4).
//!
//! `1.7.3` stopped at a [`Bitmap`] that is 1 bit per pixel, MSB first and
//! row-major — which is exactly the layout `GS v 0` takes — so this module is a
//! header and a copy rather than a re-encode. The stream it produces is what
//! `receipt_artifact.content_bytes` stores and what a reprint replays byte for
//! byte.
//!
//! ```text
//! ESC @            1B 40                      initialise
//! GS  v 0          1D 76 30 00 xL xH yL yH    raster, normal size
//!                  <the bitmap's own bytes>
//! GS  V            1D 56 01                   partial cut
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
//! # One `GS v 0`, not a banded sequence
//!
//! Field ESC/POS libraries commonly split a raster into bands, because real
//! printers have finite buffers. The specification does not require it — the
//! line count is a 16-bit field and one command is correct — and the right band
//! size is a **device** fact rather than a code fact.
//!
//! `ref/hardware-and-receipts.md` §6a is why that settles it: *"'ESC/POS' is a
//! family, not a standard. Two printers that both claim it disagree about the
//! raster command, the status protocol, the cut sequence, the drawer pulse
//! timings and the width in dots."* A profile record carries `raster_command`
//! for exactly this reason, and **the matrix ships empty except the
//! simulator** — so there is no device whose buffer could be measured, and
//! inventing a band size against none would be the defect §6a.1 refuses for
//! the reference register. Banding belongs to a qualified profile, behind #68.
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
//! band size, so this emits the baseline and the question goes on the record:
//! **§9's hardware-lab checklist has no check that the cut lands below the
//! footer.** Check 2 covers truncation across the paper, not along it. Whoever
//! qualifies the first printer adds both the feed and the check.

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

/// `ESC p` — the drawer pulse prefix. Declared here so a test can assert its
/// **absence**, and for no other reason: nothing in this module emits it.
pub const DRAWER_PULSE_PREFIX: [u8; 2] = [0x1B, 0x70];

/// The largest value either dimension field can carry.
///
/// `GS v 0` takes both as little-endian `u16`. Neither limit is reachable
/// today — the two paper widths are 72 and 48 bytes per row, and
/// `render::MAX_PAGE_HEIGHT_PX` is 32 768 — which is why they are tested
/// directly rather than through a page nothing can produce.
pub const MAX_DIMENSION: u32 = u16::MAX as u32;

/// Why a page could not become a document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EscPosError {
    /// More bytes across than the header can name.
    #[error("a page {row_bytes} bytes across cannot be named in a 16-bit header")]
    TooWide { row_bytes: u32 },
    /// More lines down than the header can name.
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
}

/// The fixed bytes around a page: `ESC @` before, `GS v 0` and its four
/// dimension bytes, and `GS V` after.
const FRAME_LEN: usize = INIT.len() + RASTER.len() + 4 + CUT.len();

/// Encode one page as an ESC/POS document.
///
/// The bitmap is copied verbatim. `1.7.3` produces MSB-first, row-major bits at
/// the profile's width for precisely this reason, so nothing here shifts,
/// pads or reorders a byte — and a transposition bug in the printed output can
/// therefore only be a bug in the rasteriser.
///
/// # Errors
///
/// Refuses a page either of whose dimensions exceeds [`MAX_DIMENSION`], and a
/// bitmap whose buffer does not hold the page it describes.
pub fn encode(page: &Bitmap) -> Result<Vec<u8>, EscPosError> {
    if page.height_px == 0 || page.width_px == 0 {
        return Err(EscPosError::Empty);
    }

    let row_bytes = page.row_bytes();
    if row_bytes > MAX_DIMENSION {
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

    let mut out = Vec::with_capacity(FRAME_LEN + expected);
    out.extend_from_slice(&INIT);
    out.extend_from_slice(&RASTER);
    out.extend_from_slice(&little_endian(row_bytes));
    out.extend_from_slice(&little_endian(page.height_px));
    out.extend_from_slice(&page.bits);
    out.extend_from_slice(&CUT);
    Ok(out)
}

/// A dimension as the two little-endian bytes `GS v 0` wants.
///
/// The caller has already refused anything past [`MAX_DIMENSION`], so the
/// truncation here cannot lose a bit — which is why this is a `const fn` taking
/// a checked value rather than a fallible conversion repeated at both call
/// sites.
const fn little_endian(value: u32) -> [u8; 2] {
    [(value & 0xFF) as u8, ((value >> 8) & 0xFF) as u8]
}

/// Where the raster payload sits inside an encoded stream.
///
/// Exported because the interesting assertions about a document are about what
/// is *outside* the payload — see the module tests and
/// `receipt_retry_never_repeats_the_drawer_pulse`. A caller that wants to check
/// a stream should not have to re-derive this arithmetic and get it wrong.
#[must_use]
pub fn payload_range(page: &Bitmap) -> core::ops::Range<usize> {
    let start = INIT.len() + RASTER.len() + 4;
    let len = (page.row_bytes() as usize).saturating_mul(page.height_px as usize);
    start..start.saturating_add(len)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::render::tests_support::model;
    use crate::render::{PrinterProfile, render_receipt};
    use crate::{PrinterStatus, ReceiptPrinter, RenderedReceipt, SimulatedPrinter};
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

        let stream = encode(&page(64, 2, 0xFF)).expect("encodes");
        assert!(stream.starts_with(&INIT), "a document begins with ESC @");
        assert!(stream.ends_with(&CUT), "a document ends with a cut");
    }

    /// The named test, and the reason the pulse left the byte stream.
    ///
    /// Two halves, and the second is the one that matters. A stream is emitted
    /// and sent **twice**, as a retried print job does; the simulator's drawer
    /// counter stays at zero, because nothing in the document can move it. And
    /// the pulse's two bytes appear nowhere outside the raster payload, which
    /// is where a document could otherwise smuggle one.
    #[test]
    fn receipt_retry_never_repeats_the_drawer_pulse() {
        let bitmap = render_receipt(&model(), PrinterProfile::Mm80).expect("renders");
        let stream = encode(&bitmap).expect("encodes");

        let printer = SimulatedPrinter::new();
        let doc = RenderedReceipt {
            bytes: stream.clone(),
        };
        printer.print(&doc).expect("prints");
        printer
            .print(&doc)
            .expect("prints again — this is the retry");

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
        assert!(!pulse_outside_payload(&stream, &bitmap));
    }

    /// Whether `ESC p` appears anywhere that is not the raster payload.
    ///
    /// Scanning the **whole** stream would be the wrong test and would be
    /// intermittently red: the payload is an arbitrary bitmap, so `1B 70`
    /// occurs inside it by coincidence about once in every 65 536 byte pairs —
    /// which a 576-dot receipt reaches within roughly 900 rows.
    /// `a_bitmap_containing_the_pulse_bytes_is_still_a_document` is that case
    /// made deliberate.
    fn pulse_outside_payload(stream: &[u8], page: &Bitmap) -> bool {
        let payload = payload_range(page);
        stream
            .windows(DRAWER_PULSE_PREFIX.len())
            .enumerate()
            .any(|(at, pair)| {
                pair == DRAWER_PULSE_PREFIX
                    && !(payload.contains(&at)
                        && payload.contains(&(at + DRAWER_PULSE_PREFIX.len() - 1)))
            })
    }

    #[test]
    fn a_stream_is_exactly_init_raster_and_cut() {
        let bitmap = page(64, 3, 0xAA);
        let stream = encode(&bitmap).expect("encodes");

        let expected_len = FRAME_LEN + bitmap.bits.len();
        assert_eq!(stream.len(), expected_len, "no byte is unaccounted for");

        let payload = payload_range(&bitmap);
        assert_eq!(payload.start, INIT.len() + RASTER.len() + 4);
        assert_eq!(payload.end, stream.len() - CUT.len());
        assert_eq!(stream.get(payload.clone()), Some(bitmap.bits.as_slice()));
    }

    #[test]
    fn the_raster_header_declares_the_bitmaps_own_dimensions() {
        // 520 dots is 65 bytes across, so the low byte alone would be enough
        // and a byte-swapped header would still look plausible. 300 lines put
        // the height past 255 for the same reason — both fields are asserted
        // where a single-byte confusion is visible.
        let bitmap = page(520, 300, 0);
        let stream = encode(&bitmap).expect("encodes");
        let header = INIT.len() + RASTER.len();

        assert_eq!(bitmap.row_bytes(), 65);
        assert_eq!(stream.get(header), Some(&65), "xL");
        assert_eq!(stream.get(header + 1), Some(&0), "xH");
        assert_eq!(stream.get(header + 2), Some(&44), "yL — 300 is 0x012C");
        assert_eq!(stream.get(header + 3), Some(&1), "yH");
    }

    #[test]
    fn the_raster_payload_is_the_bitmaps_bytes_unchanged() {
        let mut bitmap = page(32, 4, 0);
        for (index, byte) in bitmap.bits.iter_mut().enumerate() {
            // A pattern where every byte differs and no two rows agree, so a
            // shift, a reversal or a row transposition all show.
            *byte = u8::try_from(index)
                .unwrap_or(0)
                .wrapping_mul(37)
                .wrapping_add(11);
        }

        let stream = encode(&bitmap).expect("encodes");
        assert_eq!(
            stream.get(payload_range(&bitmap)),
            Some(bitmap.bits.as_slice())
        );
    }

    #[test]
    fn both_profiles_emit_their_own_row_width() {
        for (profile, row_bytes) in [(PrinterProfile::Mm80, 72), (PrinterProfile::Mm58, 48)] {
            let bitmap = render_receipt(&model(), profile).expect("renders");
            assert_eq!(bitmap.row_bytes(), row_bytes, "{profile:?}");

            let stream = encode(&bitmap).expect("encodes");
            let header = INIT.len() + RASTER.len();
            assert_eq!(
                stream.get(header).copied(),
                u8::try_from(row_bytes).ok(),
                "{profile:?} declared the wrong width"
            );
            assert_eq!(stream.len(), FRAME_LEN + bitmap.bits.len());
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
            encode(&short),
            Err(EscPosError::Truncated {
                width_px: 64,
                height_px: 4,
                expected: 32,
                found: 31,
            })
        );

        let mut long = page(64, 4, 0);
        long.bits.push(0);
        assert!(matches!(encode(&long), Err(EscPosError::Truncated { .. })));

        // And the exact page encodes.
        assert!(encode(&page(64, 4, 0)).is_ok());
    }

    #[test]
    fn a_page_too_wide_for_a_sixteen_bit_header_is_refused() {
        let wide = Bitmap {
            width_px: (MAX_DIMENSION + 1) * 8,
            height_px: 1,
            bits: Vec::new(),
        };
        assert_eq!(
            encode(&wide),
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
            encode(&tall),
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
        assert!(matches!(encode(&edge), Err(EscPosError::Truncated { .. })));
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

        let stream = encode(&bitmap).expect("a page is a page whatever its bits say");

        // The naive test would fail here — the bytes really are in the stream.
        assert!(
            stream.windows(2).any(|pair| pair == DRAWER_PULSE_PREFIX),
            "the fixture must actually contain the pulse bytes"
        );
        // The correct one passes: they are inside the payload and nowhere else.
        assert!(!pulse_outside_payload(&stream, &bitmap));
    }

    #[test]
    fn a_page_with_no_lines_is_not_a_document() {
        assert_eq!(encode(&page(64, 0, 0)), Err(EscPosError::Empty));
        assert_eq!(encode(&page(0, 4, 0)), Err(EscPosError::Empty));
        // One line is a document, however little it says.
        assert!(encode(&page(64, 1, 0)).is_ok());
    }

    /// The simulator refuses a document when the paper is out, so a retry is a
    /// real retry rather than a second successful print.
    #[test]
    fn a_refused_document_still_leaves_the_drawer_shut() {
        let printer = SimulatedPrinter::new();
        *printer.force_status.lock().unwrap() = PrinterStatus::PaperOut;

        let stream = encode(&page(64, 2, 0xFF)).expect("encodes");
        let doc = RenderedReceipt { bytes: stream };
        assert!(printer.print(&doc).is_err());

        assert_eq!(*printer.drawer_opens.lock().unwrap(), 0);
        assert!(printer.printed.lock().unwrap().is_empty());
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
        ) {
            let bitmap = page(width_px, height_px, fill);
            prop_assert_eq!(encode(&bitmap)?, encode(&bitmap)?);
        }

        /// A stream is the page and a fixed frame, and nothing else.
        ///
        /// Stated as arithmetic rather than as a shape: whatever the page, the
        /// document is exactly eleven bytes longer than the bitmap, the payload
        /// is the bitmap, and the pulse is absent from every byte that is not
        /// the payload.
        #[test]
        fn prop_the_stream_is_the_page_plus_a_fixed_frame(
            width_px in 8u32..=640,
            height_px in 1u32..64,
            fill in any::<u8>(),
        ) {
            let bitmap = page(width_px, height_px, fill);
            let stream = encode(&bitmap)?;

            prop_assert_eq!(stream.len(), FRAME_LEN + bitmap.bits.len());
            prop_assert!(stream.starts_with(&INIT));
            prop_assert!(stream.ends_with(&CUT));
            prop_assert_eq!(
                stream.get(payload_range(&bitmap)),
                Some(bitmap.bits.as_slice())
            );
            prop_assert!(!pulse_outside_payload(&stream, &bitmap));
        }
    }
}
