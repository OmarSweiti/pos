//! A receipt printer's profile: `ref/hardware-and-receipts.md` §6a's record as
//! a type (microstep 1.7.9), and every device's profile as data (1.7.11).
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
//! default and a default is a guess somebody did not have to defend.
//!
//! 1.7.9 applied that to one fact, the most dot rows one raster command
//! carries. 1.7.11 applies it to every fact, because its profiles come from
//! [`data`] files written before any device was held, and most of them leave
//! something unknown. A profile with an unknown in a parameter the driver needs
//! cannot be bound to a register — [`BINDING`] lists those parameters and
//! [`PrinterProfile::unresolved`] names the ones a profile is missing — and the
//! emitter refuses a document whose width, framing or row limit is unknown
//! rather than inventing one (`ref/hardware-and-receipts.md` §6b.2, rule 2).
//!
//! # The width has one source, and the profile checks it
//!
//! 1.7.4 kept the profile out of [`crate::escpos::encode`] so a page's width
//! could not have two sources. It still has one — the bitmap — and the
//! profile's [`PrinterProfile::dot_width`] is a check on it rather than a
//! second answer: a page rendered for one paper and sent to a device of another
//! is refused, not printed off-centre or truncated.

use core::fmt;
use core::num::NonZeroU32;

use serde::Deserialize;

use crate::render::PaperWidth;

pub mod data;

/// A value a profile states, or says plainly that it does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fact<T> {
    /// Stated by a source: a maker's document, a seller's written answer, or a
    /// capture from a device.
    Known(T),
    /// Nobody has measured it. Never filled by a guess.
    Unknown,
}

impl<T> Fact<T> {
    /// Whether nobody has stated this value.
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    /// The value, when somebody has stated it.
    #[must_use]
    pub const fn known(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown => None,
        }
    }
}

/// How bytes reach the device. TCP 9100 first (1.7.10); the others are later
/// steps, written when a qualified device needs one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Transport {
    /// Raw TCP to port 9100 — needs no driver on any operating system.
    #[serde(rename = "tcp_9100")]
    Tcp9100,
    /// RS-232, through a serial port.
    #[serde(rename = "serial")]
    Serial,
    /// USB, whose class decides whether a vendor driver is needed first.
    #[serde(rename = "usb")]
    Usb,
    /// No wire: the in-process simulator.
    #[serde(rename = "simulated")]
    Simulated,
}

/// Which ESC/POS command carries the raster.
///
/// They are not interchangeable. Epson's ESC/POS reference marks `GS v 0`
/// *"[obsolete command]"* that *"will not be supported by future models"* and
/// recommends the graphics function `GS ( L` — function 112 to store, function
/// 50 to print — so a profile says which one its device takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum RasterCommand {
    /// `GS v 0` — raster bit image, width in bytes and height in rows, each a
    /// little-endian `u16`.
    #[serde(rename = "gs_v_0")]
    GsV0,
    /// `GS ( L` function 112 then function 50 — store the raster in the
    /// graphics buffer, then print it. Its parameter count is a `u16` too, which
    /// is a second limit on a strip's height (see `crate::escpos`).
    #[serde(rename = "gs_paren_l")]
    GsParenL,
}

/// How the document is cut. 1.7.4's baseline is a partial cut; which one a
/// device honours, and the feed it needs first, are device facts (§6a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum CutCommand {
    /// `GS V 1` — leaves a tab, so the receipt hangs rather than drops.
    #[serde(rename = "partial")]
    Partial,
    /// `GS V 0` — severs the paper.
    #[serde(rename = "full")]
    Full,
}

/// Which pin of the drawer port a pulse drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum DrawerPin {
    /// Connector pin 2 — `ESC p 0`.
    #[serde(rename = "pin_2")]
    Pin2,
    /// Connector pin 5 — `ESC p 1`.
    #[serde(rename = "pin_5")]
    Pin5,
}

/// One drawer pulse: wrong timings do not open the drawer, and a solenoid whose
/// voltage disagrees with the port's never will (§6a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum StatusProtocol {
    /// `DLE EOT` 1–4, answered over the transport the profile binds.
    #[serde(rename = "dle_eot")]
    DleEot,
    /// The device has none, or has it only on a transport this profile does
    /// not use — Xprinter's `DLE EOT` is serial-only, for one.
    #[serde(rename = "none")]
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

/// A receipt-printer value a register cannot be bound without, named as it is
/// in a profile file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Parameter {
    Transport,
    DotWidth,
    RasterCommand,
    RasterMaxRows,
    AutoCutter,
    CutCommand,
    CutFeedRows,
    StatusProtocol,
    DrawerPulse,
    SupportsRasterArabic,
}

impl Parameter {
    /// The value's name in a profile file.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Transport => "transport",
            Self::DotWidth => "dot_width",
            Self::RasterCommand => "raster_command",
            Self::RasterMaxRows => "raster_max_rows",
            Self::AutoCutter => "auto_cutter",
            Self::CutCommand => "cut_command",
            Self::CutFeedRows => "cut_feed_rows",
            Self::StatusProtocol => "status_protocol",
            Self::DrawerPulse => "drawer_pulse",
            Self::SupportsRasterArabic => "supports_raster_arabic",
        }
    }
}

impl fmt::Display for Parameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// Everything a register's driver needs from its printer's profile, in the
/// order [`PrinterProfile::bindable`] checks it.
///
/// How bytes reach the device and how wide its line is; how the raster is
/// framed and how tall one command may be; how the document ends, including
/// the feed the cutter needs so the footer is not cut off inside the machine;
/// whether paper-out can be read before the money is taken (§3, rule 1); how
/// the drawer opens; and whether the device prints the Arabic receipt this
/// product is sold on. The rest of the record — the maker, the firmware range,
/// the code pages, the drawer port's voltage — describes the device without
/// changing what the driver sends.
pub const BINDING: [Parameter; 10] = [
    Parameter::Transport,
    Parameter::DotWidth,
    Parameter::RasterCommand,
    Parameter::RasterMaxRows,
    Parameter::AutoCutter,
    Parameter::CutCommand,
    Parameter::CutFeedRows,
    Parameter::StatusProtocol,
    Parameter::DrawerPulse,
    Parameter::SupportsRasterArabic,
];

/// `ref/hardware-and-receipts.md` §6a's record, every field present, with the
/// receipt-printer parameters §6b.2 adds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrinterProfile {
    /// What a support call is about, and what `receipt_artifact.printer_profile`
    /// stores, so a reprint can refuse a register bound to another profile.
    pub profile_id: String,
    /// Which version of the profile this is. A changed value is a new version
    /// and the old one stays, because a capture, a fixture or a qualification
    /// may name it (§6b.2, rule 3).
    pub version: NonZeroU32,
    pub maker: Fact<String>,
    pub model: Fact<String>,
    /// A range, because behaviour changes under a firmware update.
    pub firmware_qualified: Fact<String>,
    pub transport: Fact<Transport>,
    pub paper_mm: Fact<u32>,
    /// The dots one line prints — the check on a page's width.
    pub dot_width: Fact<NonZeroU32>,
    pub raster_command: Fact<RasterCommand>,
    /// The most dot rows one raster command may carry on this device — 2303 on
    /// an Epson TM-T20III, 4095 on a TM-T88VII. A taller page is sent in strips.
    pub raster_max_rows: Fact<NonZeroU32>,
    /// The device's character code pages, for §2.1's text fallback only:
    /// receipts are raster.
    pub code_pages: Fact<Vec<String>>,
    /// Whether the device cuts the paper itself.
    pub auto_cutter: Fact<bool>,
    pub cut_command: Fact<CutCommand>,
    /// The dot rows to feed before the cut, so the cutter, which sits above
    /// the print head, does not sever the paper before the footer. Unknown on
    /// every device until one is measured: `crate::escpos` records why it emits
    /// no feed yet.
    pub cut_feed_rows: Fact<u32>,
    pub status_protocol: Fact<StatusProtocol>,
    /// The voltage the drawer port supplies, which a drawer's solenoid must
    /// match.
    pub drawer_port_volts: Fact<u32>,
    pub drawer_pulse: Fact<DrawerPulse>,
    /// `Known(false)` means the device is not qualified for Arabic and may not
    /// be sold to an Arabic-first store; `Unknown` means nobody has printed the
    /// Arabic goldens on it yet.
    pub supports_raster_arabic: Fact<bool>,
    pub qualification: Qualification,
}

/// Why a profile cannot be bound to a register.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    /// §6a.1 refuses a referent nobody measured, and a driver cannot work
    /// from one either.
    #[error(
        "profile {profile_id} does not know its {parameter}, which a register cannot be bound without"
    )]
    Unknown {
        profile_id: String,
        parameter: Parameter,
    },
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
    /// before milestone 5.8. It is code rather than a [`data`] file because it
    /// describes no device: every value is the simulator's own.
    #[must_use]
    pub fn simulator(paper: PaperWidth) -> Self {
        let (profile_id, paper_mm) = match paper {
            PaperWidth::Mm80 => ("simulator-80mm", 80),
            PaperWidth::Mm58 => ("simulator-58mm", 58),
        };
        Self {
            profile_id: profile_id.to_owned(),
            version: NonZeroU32::MIN,
            maker: Fact::Known("pos-hardware".to_owned()),
            model: Fact::Known("SimulatedPrinter".to_owned()),
            firmware_qualified: Fact::Known("not applicable: software".to_owned()),
            transport: Fact::Known(Transport::Simulated),
            paper_mm: Fact::Known(paper_mm),
            dot_width: NonZeroU32::new(paper.width_px()).map_or(Fact::Unknown, Fact::Known),
            raster_command: Fact::Known(RasterCommand::GsV0),
            raster_max_rows: NonZeroU32::new(SIMULATOR_RASTER_MAX_ROWS)
                .map_or(Fact::Unknown, Fact::Known),
            // Raster only: the simulator has no text mode to fall back to.
            code_pages: Fact::Known(Vec::new()),
            auto_cutter: Fact::Known(true),
            cut_command: Fact::Known(CutCommand::Partial),
            // No cutter above a print head, so nothing to feed past.
            cut_feed_rows: Fact::Known(0),
            status_protocol: Fact::Known(StatusProtocol::DleEot),
            // No electrical port, and nothing reads this to drive a document.
            drawer_port_volts: Fact::Unknown,
            drawer_pulse: Fact::Known(DrawerPulse {
                pin: DrawerPin::Pin2,
                on_ms: 50,
                off_ms: 500,
            }),
            supports_raster_arabic: Fact::Known(true),
            qualification: Qualification::NotQualified,
        }
    }

    /// Whether this profile states `parameter`.
    #[must_use]
    pub const fn states(&self, parameter: Parameter) -> bool {
        match parameter {
            Parameter::Transport => !self.transport.is_unknown(),
            Parameter::DotWidth => !self.dot_width.is_unknown(),
            Parameter::RasterCommand => !self.raster_command.is_unknown(),
            Parameter::RasterMaxRows => !self.raster_max_rows.is_unknown(),
            Parameter::AutoCutter => !self.auto_cutter.is_unknown(),
            Parameter::CutCommand => !self.cut_command.is_unknown(),
            Parameter::CutFeedRows => !self.cut_feed_rows.is_unknown(),
            Parameter::StatusProtocol => !self.status_protocol.is_unknown(),
            Parameter::DrawerPulse => !self.drawer_pulse.is_unknown(),
            Parameter::SupportsRasterArabic => !self.supports_raster_arabic.is_unknown(),
        }
    }

    /// Every parameter in [`BINDING`] this profile does not know, in order —
    /// what the gate must settle before a register may be bound to it
    /// (`ref/hardware-and-receipts.md` §6b.5, rule 4).
    #[must_use]
    pub fn unresolved(&self) -> Vec<Parameter> {
        BINDING
            .into_iter()
            .filter(|parameter| !self.states(*parameter))
            .collect()
    }

    /// Whether this profile can be bound to a register: every parameter in
    /// [`BINDING`] is known.
    ///
    /// # Errors
    ///
    /// [`ProfileError::Unknown`] naming the first parameter in [`BINDING`] the
    /// profile does not know; [`PrinterProfile::unresolved`] names them all.
    pub fn bindable(&self) -> Result<(), ProfileError> {
        match self.unresolved().first() {
            Some(&parameter) => Err(self.unknown(parameter)),
            None => Ok(()),
        }
    }

    /// The dots one line prints.
    ///
    /// # Errors
    ///
    /// [`ProfileError::Unknown`] when the profile does not know.
    pub fn dots_per_line(&self) -> Result<NonZeroU32, ProfileError> {
        self.dot_width
            .known()
            .copied()
            .ok_or_else(|| self.unknown(Parameter::DotWidth))
    }

    /// The command that carries the raster.
    ///
    /// # Errors
    ///
    /// [`ProfileError::Unknown`] when the profile does not know.
    pub fn raster_framing(&self) -> Result<RasterCommand, ProfileError> {
        self.raster_command
            .known()
            .copied()
            .ok_or_else(|| self.unknown(Parameter::RasterCommand))
    }

    /// The most dot rows one raster command may carry on this device.
    ///
    /// # Errors
    ///
    /// [`ProfileError::Unknown`] when the profile does not know.
    pub fn row_limit(&self) -> Result<NonZeroU32, ProfileError> {
        self.raster_max_rows
            .known()
            .copied()
            .ok_or_else(|| self.unknown(Parameter::RasterMaxRows))
    }

    fn unknown(&self, parameter: Parameter) -> ProfileError {
        ProfileError::Unknown {
            profile_id: self.profile_id.clone(),
            parameter,
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
            assert_eq!(
                profile.dots_per_line().map(NonZeroU32::get),
                Ok(paper.width_px()),
                "{paper:?}"
            );
            assert_eq!(profile.bindable(), Ok(()), "{paper:?}");
            assert_eq!(profile.unresolved(), Vec::new(), "{paper:?}");
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
            Err(ProfileError::Unknown {
                profile_id: "simulator-80mm".to_owned(),
                parameter: Parameter::RasterMaxRows,
            })
        );
    }

    /// `bindable` names the first gap; `unresolved` names every one, in
    /// [`BINDING`]'s order, which is the list the gate works from.
    #[test]
    fn unresolved_names_every_unknown_binding_parameter_in_order() {
        let mut profile = PrinterProfile::simulator(PaperWidth::Mm58);
        profile.drawer_pulse = Fact::Unknown;
        profile.transport = Fact::Unknown;
        profile.code_pages = Fact::Unknown;
        assert_eq!(
            profile.unresolved(),
            vec![Parameter::Transport, Parameter::DrawerPulse],
            "code pages describe the device without being needed to drive it"
        );
        assert_eq!(
            profile.bindable(),
            Err(ProfileError::Unknown {
                profile_id: "simulator-58mm".to_owned(),
                parameter: Parameter::Transport,
            })
        );
    }

    #[test]
    fn a_parameter_is_named_as_its_file_names_it() {
        assert_eq!(Parameter::RasterMaxRows.to_string(), "raster_max_rows");
        assert_eq!(
            ProfileError::Unknown {
                profile_id: "x".to_owned(),
                parameter: Parameter::CutFeedRows,
            }
            .to_string(),
            "profile x does not know its cut_feed_rows, which a register cannot be bound without"
        );
    }
}
