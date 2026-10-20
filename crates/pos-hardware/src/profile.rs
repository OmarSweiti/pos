//! A receipt printer's profile: `ref/hardware-and-receipts.md` §6a's record as
//! a type (microstep 1.7.9).
//!
//! §6a opens with the reason this exists: *"'ESC/POS' is a family, not a
//! standard. Two printers that both claim it disagree about the raster command,
//! the status protocol, the cut sequence, the drawer pulse timings and the width
//! in dots."* Every one of those is a fact about a **device**, so none of them
//! may be a constant in a driver. They live here, one record per device, and a
//! driver is bound to a register only through one.
//!
//! # A fact nobody measured is `Unknown`, never a guess
//!
//! No device is bought before milestone 5.8 (§6b), so most facts arrive from a
//! maker's document, and some arrive from nowhere at all. [`Fact::Unknown`] is a
//! value rather than an omitted field, because a missing field reads as a
//! default and a default is a guess somebody did not have to defend. Where the
//! emitter needs a fact — today, the most dot rows one raster command carries —
//! an unknown one refuses the document rather than inventing a number
//! (`ref/hardware-and-receipts.md` §6b.2, rule 2). 1.7.11 moves profiles into
//! data files, with a cited source beside every value; this is the shape they
//! load into.
//!
//! # The width has one source, and the profile checks it
//!
//! 1.7.4 kept the profile out of [`crate::escpos::encode`] so a page's width
//! could not have two sources. It still has one — the bitmap — and the
//! profile's [`PrinterProfile::dot_width`] is a check on it rather than a
//! second answer: a page rendered for one paper and sent to a device of another
//! is refused, not printed off-centre or truncated.

use core::num::NonZeroU32;

use crate::render::PaperWidth;

/// A value a profile states, or says plainly that it does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fact<T> {
    /// Stated by a source: a maker's document, a seller's written answer, or a
    /// capture from a device.
    Known(T),
    /// Nobody has measured it. Never filled by a guess.
    Unknown,
}

/// How bytes reach the device. TCP 9100 first (1.7.10); the others are later
/// steps, written when a qualified device needs one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transport {
    /// Raw TCP to port 9100 — needs no driver on any operating system.
    Tcp9100,
    /// RS-232, through a serial port.
    Serial,
    /// USB, whose class decides whether a vendor driver is needed first.
    Usb,
    /// No wire: the in-process simulator.
    Simulated,
}

/// Which ESC/POS command carries the raster.
///
/// They are not interchangeable. Epson's ESC/POS reference marks `GS v 0`
/// *"[obsolete command]"* that *"will not be supported by future models"* and
/// recommends the graphics function `GS ( L` — function 112 to store, function
/// 50 to print — so a profile says which one its device takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RasterCommand {
    /// `GS v 0` — raster bit image, width in bytes and height in rows, each a
    /// little-endian `u16`.
    GsV0,
    /// `GS ( L` function 112 then function 50 — store the raster in the
    /// graphics buffer, then print it. Its parameter count is a `u16` too, which
    /// is a second limit on a strip's height (see `crate::escpos`).
    GsParenL,
}

/// How the document is cut. 1.7.4's baseline is a partial cut; which one a
/// device honours, and the feed it needs first, are device facts (§6a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CutCommand {
    /// `GS V 1` — leaves a tab, so the receipt hangs rather than drops.
    Partial,
    /// `GS V 0` — severs the paper.
    Full,
}

/// Which pin of the drawer port a pulse drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrawerPin {
    /// Connector pin 2 — `ESC p 0`.
    Pin2,
    /// Connector pin 5 — `ESC p 1`.
    Pin5,
}

/// One drawer pulse: wrong timings do not open the drawer, and a solenoid whose
/// voltage disagrees with the port's never will (§6a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DrawerPulse {
    pub pin: DrawerPin,
    /// The pulse's on-time, in milliseconds.
    pub on_ms: u16,
    /// The time held off after it, in milliseconds.
    pub off_ms: u16,
}

/// Whether the device answers real-time status — what makes §3's rule 1
/// possible, because a device that cannot report paper-out cannot warn before
/// the money is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusProtocol {
    /// `DLE EOT` 1–4, answered over the transport the profile binds.
    DleEot,
    /// The device has none, or has it only on a transport this profile does
    /// not use — Xprinter's `DLE EOT` is serial-only, for one.
    None,
}

/// When the record was last true, and against what (§6a). A profile is a
/// candidate until a device somebody held has been run against it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Qualification {
    /// Not yet run against a device — every profile until milestone 5.8, and
    /// the simulator always.
    NotQualified,
    /// Qualified: a date, a person, and the commit the suite ran at.
    Qualified {
        at: String,
        by: String,
        commit: String,
    },
}

/// `ref/hardware-and-receipts.md` §6a's record, every field present.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrinterProfile {
    /// What a support call is about, and what `receipt_artifact.printer_profile`
    /// stores, so a reprint can refuse a register bound to another profile.
    pub profile_id: String,
    pub maker: String,
    pub model: String,
    /// A range, because behaviour changes under a firmware update.
    pub firmware_qualified: Fact<String>,
    pub transport: Transport,
    /// The dots one line prints — the check on a page's width.
    pub dot_width: u32,
    pub paper_mm: u32,
    pub raster_command: RasterCommand,
    /// The most dot rows one raster command may carry on this device — 2303 on
    /// an Epson TM-T20III, 4095 on a TM-T88VII. A taller page is sent in strips.
    pub raster_max_rows: Fact<NonZeroU32>,
    pub cut_command: CutCommand,
    pub drawer_pulse: Fact<DrawerPulse>,
    pub status_protocol: Fact<StatusProtocol>,
    /// `false` means the device is not qualified for Arabic and may not be sold
    /// to an Arabic-first store.
    pub supports_raster_arabic: bool,
    pub qualification: Qualification,
}

/// Why a profile cannot be bound to a register.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    /// §6a.1 refuses a referent nobody measured, and the emitter cannot size a
    /// strip without this one.
    #[error("profile {profile_id} does not know how many dot rows one raster command carries")]
    UnknownRowLimit { profile_id: String },
    /// A device that prints no dots is not a device.
    #[error("profile {profile_id} states a dot width of zero")]
    ZeroDotWidth { profile_id: String },
}

/// The simulator's limit on one raster command: the strictest seeded device's.
///
/// The simulator has no buffer to overflow, so it could state any number — and
/// any number larger than a real device's would hide the strip path from every
/// test that prints through it. It states the TM-T20III's 2303 instead
/// (Epson's ESC/POS reference; `docs/hardware-lab.md` §4.2), so a receipt
/// longer than about 29 cm is banded in CI exactly as it will be on the device
/// the seed names first (case E.93).
pub const SIMULATOR_RASTER_MAX_ROWS: u32 = 2303;

impl PrinterProfile {
    /// The simulator's profile for one paper — the one row §6a's matrix carries
    /// before milestone 5.8.
    #[must_use]
    pub fn simulator(paper: PaperWidth) -> Self {
        let (profile_id, paper_mm) = match paper {
            PaperWidth::Mm80 => ("simulator-80mm", 80),
            PaperWidth::Mm58 => ("simulator-58mm", 58),
        };
        Self {
            profile_id: profile_id.to_owned(),
            maker: "pos-hardware".to_owned(),
            model: "SimulatedPrinter".to_owned(),
            firmware_qualified: Fact::Known("not applicable: software".to_owned()),
            transport: Transport::Simulated,
            dot_width: paper.width_px(),
            paper_mm,
            raster_command: RasterCommand::GsV0,
            raster_max_rows: NonZeroU32::new(SIMULATOR_RASTER_MAX_ROWS)
                .map_or(Fact::Unknown, Fact::Known),
            cut_command: CutCommand::Partial,
            drawer_pulse: Fact::Known(DrawerPulse {
                pin: DrawerPin::Pin2,
                on_ms: 50,
                off_ms: 500,
            }),
            status_protocol: Fact::Known(StatusProtocol::DleEot),
            supports_raster_arabic: true,
            qualification: Qualification::NotQualified,
        }
    }

    /// Whether this profile can be bound to a register: every fact the emitter
    /// needs is known, and the device prints at least one dot.
    ///
    /// # Errors
    ///
    /// [`ProfileError::UnknownRowLimit`] when nobody has measured how many rows
    /// one raster command carries, and [`ProfileError::ZeroDotWidth`] for a
    /// device that prints no dots.
    pub fn bindable(&self) -> Result<(), ProfileError> {
        if self.dot_width == 0 {
            return Err(ProfileError::ZeroDotWidth {
                profile_id: self.profile_id.clone(),
            });
        }
        self.row_limit().map(|_| ())
    }

    /// The most dot rows one raster command may carry on this device.
    ///
    /// # Errors
    ///
    /// [`ProfileError::UnknownRowLimit`] when the profile does not know.
    pub fn row_limit(&self) -> Result<NonZeroU32, ProfileError> {
        match self.raster_max_rows {
            Fact::Known(rows) => Ok(rows),
            Fact::Unknown => Err(ProfileError::UnknownRowLimit {
                profile_id: self.profile_id.clone(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn the_simulator_profile_states_every_fact_the_emitter_needs() {
        for paper in PaperWidth::ALL {
            let profile = PrinterProfile::simulator(paper);
            assert_eq!(profile.dot_width, paper.width_px(), "{paper:?}");
            assert_eq!(profile.bindable(), Ok(()), "{paper:?}");
            assert_eq!(
                profile.row_limit().map(NonZeroU32::get),
                Ok(SIMULATOR_RASTER_MAX_ROWS)
            );
            assert_eq!(profile.qualification, Qualification::NotQualified);
        }
    }

    /// The named rule: a profile whose row limit nobody measured cannot be
    /// bound to a register, for the reason §6a.1 refuses an unmeasured referent.
    #[test]
    fn a_profile_with_an_unknown_row_limit_cannot_be_bound() {
        let mut profile = PrinterProfile::simulator(PaperWidth::Mm80);
        profile.raster_max_rows = Fact::Unknown;
        assert_eq!(
            profile.bindable(),
            Err(ProfileError::UnknownRowLimit {
                profile_id: "simulator-80mm".to_owned()
            })
        );
    }

    #[test]
    fn a_profile_that_prints_no_dots_cannot_be_bound() {
        let mut profile = PrinterProfile::simulator(PaperWidth::Mm58);
        profile.dot_width = 0;
        assert!(matches!(
            profile.bindable(),
            Err(ProfileError::ZeroDotWidth { .. })
        ));
    }
}
