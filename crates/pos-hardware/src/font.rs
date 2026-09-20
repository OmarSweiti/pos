//! The embedded typeface (microstep 1.7.2, gap G-5).
//!
//! **IBM Plex Sans Arabic**, Regular and Bold, compiled into the binary.
//!
//! `ref/hardware-and-receipts.md:110` asks for two things at once: the font is
//! *"embedded in the app"*, and it is **the same file the UI uses**. A register
//! trades offline by design, so a font fetched over a network is not available
//! when it is needed; and a font the UI resolves separately is not the file the
//! rasteriser drew with, which is exactly how a receipt stops looking like the
//! screen. One copy of the bytes, read from one place, answers both.
//!
//! This module is that place. `1.7.3` builds the raster loader on these
//! constants and `1.11.1` wires the UI to them, so a later divergence between
//! paper and screen has to be a bug in one of those rather than two fonts.
//!
//! Arabic is the product rather than a translation of it (conventions §10),
//! which is why the typeface that renders it is a shipped asset and not a
//! deployment detail.
//!
//! The bytes and their licence live in `assets/fonts/`, whose `README.md`
//! records provenance and the one OFL clause that binds a later subsetting
//! step. They are included at **compile time**: nothing here opens a file, so a
//! register missing its assets directory cannot start up half-working.

/// Which face. Receipts set totals in bold; body text is regular.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Bold,
}

/// The family name, as the font itself reports it.
pub const FAMILY: &str = "IBM Plex Sans Arabic";

/// IBM Plex Sans Arabic Regular, TrueType.
pub const REGULAR: &[u8] = include_bytes!("../../../assets/fonts/IBMPlexSansArabic-Regular.ttf");

/// IBM Plex Sans Arabic Bold, TrueType.
pub const BOLD: &[u8] = include_bytes!("../../../assets/fonts/IBMPlexSansArabic-Bold.ttf");

/// The SIL Open Font License 1.1, as shipped with the family.
///
/// Compiled in beside the faces rather than merely committed next to them:
/// OFL 1.1 requires the licence to travel with the font, and a binary that
/// embeds the glyphs while leaving the licence on disk has not done that.
pub const LICENCE: &str = include_str!("../../../assets/fonts/LICENSE.txt");

/// The bytes for one face.
#[must_use]
pub const fn face(weight: Weight) -> &'static [u8] {
    match weight {
        Weight::Regular => REGULAR,
        Weight::Bold => BOLD,
    }
}
