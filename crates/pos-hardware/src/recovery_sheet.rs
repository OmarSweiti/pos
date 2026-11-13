//! The recovery sheet (microstep 1.8.5b, its third part): a new register's
//! recovery code, printed once on the receipt printer.
//!
//! **A hardware effect, never a document.** A receipt is rendered, kept as an
//! artifact and queued, so it can be printed again and proved later. The sheet
//! is none of those. It is rendered for one printer and sent to it once, and
//! its dots and its bytes are wiped as soon as the printer has answered.
//! Nothing here returns the bytes, retries a print or writes anything down.
//! Whether a person may press Print again is the till's to decide, from the
//! answer: only a refusal before the first byte proves that no paper exists
//! (`ref/hardware-and-receipts.md` §3).
//!
//! **Two languages on one sheet,** by the owner's decision of 13 November
//! 2026, because no store, and so no store language, exists when a code is
//! issued. Arabic comes first, then English, paragraph by paragraph, each in
//! its own direction. The code is set large and left to right, in three lines
//! of two groups, hyphenated as the screen shows it. Under it are the first
//! digits of the key's id, which match a sheet to its register's backups, and
//! when it was printed, in UTC.
//!
//! **What is wiped, and what is not.** The code's own lines, the page's
//! glyphs, the coverage they were drawn at, the page's dots and the bytes sent
//! are wiped. The shaper's and the line breaker's working copies of a line are
//! freed without being wiped: they belong to code this crate does not own.

use std::fmt;

use pos_domain::receipt::TextDirection;
use pos_domain::time::Timestamp;
use zeroize::{Zeroize, Zeroizing};

use crate::escpos::{self, EscPosError};
use crate::font::Weight;
use crate::profile::{Fact, PrinterProfile};
use crate::render::layout::{self, Alignment, Block, Faces, Layout, TextStyle};
use crate::render::raster::{self, Bitmap};
use crate::render::{BODY_PX_PER_EM, PaperWidth, RenderError};
use crate::{PrintOutcome, ReceiptPrinter, RenderedReceipt};

/// Groups in a recovery code.
pub const GROUPS: usize = 6;
/// Digits in each group.
pub const GROUP_DIGITS: usize = 5;
/// Groups set on each line of the sheet.
pub const GROUPS_PER_LINE: usize = 2;
/// Lines the code takes on the sheet.
pub const CODE_LINES: usize = GROUPS / GROUPS_PER_LINE;
/// How many of the key id's hexadecimal digits the sheet prints: enough to
/// tell one register's sheet from another's, as a disk-recovery key's short
/// identifier does.
pub const KEY_ID_DIGITS: usize = 8;

/// The code's type size: twice the body's, so each digit reads from arm's
/// length and every line still fits 58 mm paper whole.
pub const CODE_PX_PER_EM: i32 = BODY_PX_PER_EM * 2;

/// Every fixed word the sheet prints, in both languages. A native reader
/// reviews them on the evidence page, as 1.7.5's goldens are reviewed.
mod words {
    pub(super) const TITLE_AR: &str = "رمز استرداد الصندوق";
    pub(super) const TITLE_EN: &str = "Register recovery code";
    pub(super) const KEY_ID: &str = "معرّف المفتاح · Key id";
    pub(super) const PRINTED: &str = "تاريخ الطباعة · Printed";
    pub(super) const KEEP_AR: &str =
        "احفظ هذه الورقة في مكان آمن بعيدًا عن الصندوق. لن يُطبع هذا الرمز مرة أخرى.";
    pub(super) const KEEP_EN: &str =
        "Keep this sheet somewhere safe, away from the register. This code is never printed again.";
    pub(super) const LOST_AR: &str =
        "إذا فُقد هذا الجهاز أو تلف، فلا تُفتح نسخه الاحتياطية إلا بهذا الرمز.";
    pub(super) const LOST_EN: &str =
        "If this machine is lost or fails, its backups open only with this code.";
    pub(super) const HOLDER_AR: &str =
        "من يحمل هذا الرمز ونسخة احتياطية يستطيع قراءة بيانات الصندوق.";
    pub(super) const HOLDER_EN: &str =
        "Anyone holding this code and a backup can read the register's data.";
}

/// Why a sheet could not be made or sent. None of them repeats the code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SheetError {
    /// The code is not six groups of five ASCII digits joined by hyphens,
    /// the form `pos_db::recovery::RecoveryCode::formatted` gives.
    #[error("the recovery code is not six groups of five digits")]
    NotACode,
    /// The key id is not lowercase hexadecimal, or is too short to print.
    #[error("the key id is not at least {KEY_ID_DIGITS} lowercase hexadecimal digits")]
    NotAKeyId,
    /// The profile names no paper this build renders for.
    #[error("profile {profile_id} names no paper width this build renders for")]
    NoPaper { profile_id: String },
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    Encode(#[from] EscPosError),
}

/// One register's recovery sheet, before it is printed. It never shows its
/// code in a log, and its code is wiped when it is dropped.
pub struct RecoverySheet {
    /// The code, two groups to a line, hyphenated as the screen shows it.
    lines: [Zeroizing<String>; CODE_LINES],
    /// The first [`KEY_ID_DIGITS`] of the key's id.
    key_id: String,
    /// When it is printed, in UTC, to the minute.
    printed_at: String,
}

impl fmt::Debug for RecoverySheet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoverySheet(redacted)")
    }
}

impl RecoverySheet {
    /// The sheet for `code`, as the screen shows it, for the key whose id is
    /// `key_id`, printed at `printed_at`.
    ///
    /// # Errors
    ///
    /// [`SheetError::NotACode`] for a code of any other shape, and
    /// [`SheetError::NotAKeyId`] for a key id that is not lowercase
    /// hexadecimal of at least [`KEY_ID_DIGITS`] digits.
    pub fn new(code: &str, key_id: &str, printed_at: Timestamp) -> Result<Self, SheetError> {
        let mut lines: [Zeroizing<String>; CODE_LINES] = std::array::from_fn(|_| {
            Zeroizing::new(String::with_capacity(
                GROUPS_PER_LINE * GROUP_DIGITS + GROUPS_PER_LINE - 1,
            ))
        });
        let mut groups = 0;
        for (index, group) in code.split('-').enumerate() {
            let line = lines
                .get_mut(index / GROUPS_PER_LINE)
                .ok_or(SheetError::NotACode)?;
            if group.len() != GROUP_DIGITS || !group.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(SheetError::NotACode);
            }
            if !line.is_empty() {
                line.push('-');
            }
            line.push_str(group);
            groups += 1;
        }
        if groups != GROUPS {
            return Err(SheetError::NotACode);
        }

        let hex = |byte: u8| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte);
        if key_id.len() < KEY_ID_DIGITS || !key_id.bytes().all(hex) {
            return Err(SheetError::NotAKeyId);
        }
        let key_id = key_id.get(..KEY_ID_DIGITS).ok_or(SheetError::NotAKeyId)?;

        Ok(Self {
            lines,
            key_id: key_id.to_owned(),
            printed_at: to_the_minute(printed_at),
        })
    }

    /// The sheet's blocks, top to bottom.
    pub(crate) fn blocks(&self) -> Vec<Block<'_>> {
        let body = TextStyle {
            px_per_em: BODY_PX_PER_EM,
            weight: Weight::Regular,
        };
        let strong = TextStyle {
            px_per_em: BODY_PX_PER_EM,
            weight: Weight::Bold,
        };
        let code = TextStyle {
            px_per_em: CODE_PX_PER_EM,
            weight: Weight::Bold,
        };
        let centred = |text, style, rtl| Block::Text {
            text,
            style,
            rtl,
            alignment: Alignment::Center,
        };
        let paragraph = |text, rtl| Block::Text {
            text,
            style: body,
            rtl,
            alignment: Alignment::Start,
        };

        let mut blocks = vec![
            centred(words::TITLE_AR, strong, true),
            centred(words::TITLE_EN, strong, false),
            Block::Rule,
        ];
        blocks.extend(
            self.lines
                .iter()
                .map(|line| centred(line.as_str(), code, false)),
        );
        blocks.extend([
            Block::Rule,
            centred(words::KEY_ID, body, true),
            centred(&self.key_id, strong, false),
            Block::Gap,
            centred(words::PRINTED, body, true),
            centred(&self.printed_at, body, false),
            Block::Rule,
            paragraph(words::KEEP_AR, true),
            paragraph(words::KEEP_EN, false),
            Block::Gap,
            paragraph(words::LOST_AR, true),
            paragraph(words::LOST_EN, false),
            Block::Gap,
            paragraph(words::HOLDER_AR, true),
            paragraph(words::HOLDER_EN, false),
        ]);
        blocks
    }

    /// The sheet laid out for `paper`. Its glyphs spell the code, so the
    /// caller wipes them ([`wipe_glyphs`]).
    fn lay_out(&self, faces: &Faces<'_>, paper: PaperWidth) -> Result<Layout, SheetError> {
        Ok(layout::lay_out_blocks(
            faces,
            &self.blocks(),
            paper,
            TextDirection::RightToLeft,
        )?)
    }

    /// The sheet's page for `paper`, wiped when it is dropped.
    ///
    /// # Errors
    ///
    /// [`SheetError::Render`] when the embedded faces cannot be read or the
    /// page cannot be drawn.
    pub fn render(&self, paper: PaperWidth) -> Result<SheetPage, SheetError> {
        let faces = Faces::load()?;
        let mut page = self.lay_out(&faces, paper)?;
        let drawn = raster::rasterise_wiped(&faces, &page, paper);
        wipe_glyphs(&mut page);
        Ok(SheetPage(drawn?))
    }

    /// Send the sheet to `printer`, rendered for `profile`'s paper, once.
    /// What the printer did is returned as it said it, and nothing is tried
    /// again: whether a person may print it again is the caller's to decide.
    /// The sheet is consumed, and its page and bytes are wiped once the
    /// printer has answered.
    ///
    /// # Errors
    ///
    /// [`SheetError::NoPaper`] for a profile whose paper this build does not
    /// render for, and [`SheetError::Render`] or [`SheetError::Encode`] when
    /// the page cannot be made. Nothing is sent then.
    pub fn print(
        self,
        printer: &dyn ReceiptPrinter,
        profile: &PrinterProfile,
    ) -> Result<PrintOutcome, SheetError> {
        let paper = paper_of(profile)?;
        let page = self.render(paper)?;
        let document = Sent(RenderedReceipt {
            bytes: escpos::encode(&page.0, profile)?,
        });
        drop(page);
        Ok(printer.print(&document.0))
    }
}

/// The sheet's page, wiped when it is dropped.
pub struct SheetPage(Bitmap);

impl SheetPage {
    /// The page's dots.
    #[must_use]
    pub fn bitmap(&self) -> &Bitmap {
        &self.0
    }
}

impl fmt::Debug for SheetPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SheetPage({}x{}, redacted)",
            self.0.width_px, self.0.height_px
        )
    }
}

impl Drop for SheetPage {
    fn drop(&mut self) {
        self.0.bits.zeroize();
    }
}

/// The bytes sent, wiped once the printer has answered.
struct Sent(RenderedReceipt);

impl Drop for Sent {
    fn drop(&mut self) {
        self.0.bytes.zeroize();
    }
}

/// Wipe every glyph of a page: their ids spell what it says.
fn wipe_glyphs(page: &mut Layout) {
    for line in &mut page.lines {
        for glyph in &mut line.glyphs {
            glyph.glyph_id.zeroize();
        }
    }
}

/// The paper `profile` prints, by its dots across.
fn paper_of(profile: &PrinterProfile) -> Result<PaperWidth, SheetError> {
    let dots = match profile.dot_width {
        Fact::Known(dots) => Some(dots.get()),
        _ => None,
    };
    PaperWidth::ALL
        .into_iter()
        .find(|paper| Some(paper.width_px()) == dots)
        .ok_or_else(|| SheetError::NoPaper {
            profile_id: profile.profile_id.clone(),
        })
}

/// `2026-11-13 16:25 UTC`: the date and the minute, which is all a person
/// comparing two sheets needs.
fn to_the_minute(instant: Timestamp) -> String {
    let iso = instant.to_iso8601();
    match (iso.get(..10), iso.get(11..16)) {
        (Some(date), Some(time)) => format!("{date} {time} UTC"),
        _ => iso,
    }
}

