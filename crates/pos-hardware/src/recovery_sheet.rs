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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::sync::Mutex;
    use std::time::Duration;

    use super::*;
    use crate::emulator::{ConnectionFault, Emulator, EmulatorConfig};
    use crate::net::NetworkPrinter;
    use crate::{FailReason, HwError, PrinterStatus};

    /// A code of the screen's shape. Its check digits are not this crate's
    /// to know, so any digits will do.
    const CODE: &str = "12340-56781-90122-34563-78904-13575";
    const KEY_ID: &str = "0123456789abcdef0123456789abcdef";

    fn at() -> Timestamp {
        Timestamp::from_epoch_milliseconds(1_794_590_717_123).unwrap()
    }

    fn sheet() -> RecoverySheet {
        RecoverySheet::new(CODE, KEY_ID, at()).unwrap()
    }

    /// The text of every block, in order.
    fn texts(sheet: &RecoverySheet) -> Vec<(String, TextStyle, bool)> {
        sheet
            .blocks()
            .into_iter()
            .filter_map(|block| match block {
                Block::Text {
                    text, style, rtl, ..
                } => Some((text.to_owned(), style, rtl)),
                Block::Gap | Block::Rule => None,
            })
            .collect()
    }

    /// A printer that answers every document with one outcome, and counts
    /// what it was sent.
    struct Answers {
        outcome: PrintOutcome,
        sent: Mutex<Vec<Vec<u8>>>,
    }

    impl Answers {
        fn new(outcome: PrintOutcome) -> Self {
            Self {
                outcome,
                sent: Mutex::new(Vec::new()),
            }
        }
    }

    impl ReceiptPrinter for Answers {
        fn print(&self, doc: &RenderedReceipt) -> PrintOutcome {
            self.sent.lock().unwrap().push(doc.bytes.clone());
            self.outcome
        }
        fn open_drawer(&self) -> Result<(), HwError> {
            Ok(())
        }
        fn status(&self) -> PrinterStatus {
            PrinterStatus::Ready
        }
    }

    /// The sheet takes the code exactly as the screen shows it, and refuses
    /// any other shape without repeating what it was given.
    #[test]
    fn recovery_sheet_takes_the_code_as_the_screen_shows_it() {
        assert!(RecoverySheet::new(CODE, KEY_ID, at()).is_ok());
        assert!(RecoverySheet::new(CODE, KEY_ID.get(..KEY_ID_DIGITS).unwrap(), at()).is_ok());

        for code in [
            "",
            "12340-56781-90122-34563-78904",
            "12340-56781-90122-34563-78904-13575-24686",
            "1234-56781-90122-34563-78904-13575",
            "123400-56781-90122-34563-78904-13575",
            "12340 56781 90122 34563 78904 13575",
            "123405678190122345637890413575",
            "12340-56781-90122-34563-78904-13575-",
            "-12340-56781-90122-34563-78904-13575",
            "12340--56781-90122-34563-78904-13575",
            "12340-56781-90122-34563-78904-1357a",
            "١٢٣٤٠-56781-90122-34563-78904-13575",
        ] {
            let refused = RecoverySheet::new(code, KEY_ID, at()).unwrap_err();
            assert_eq!(refused, SheetError::NotACode, "{code:?}");
            assert!(
                !refused.to_string().contains("12340"),
                "a refusal never repeats the code"
            );
        }
        for key_id in ["", "0123abc", "0123ABCD", "0123abcg", "g123abcd"] {
            assert_eq!(
                RecoverySheet::new(CODE, key_id, at()).unwrap_err(),
                SheetError::NotAKeyId,
                "{key_id:?}"
            );
        }
    }

    /// The code is set in three lines of two groups, in order, large, bold
    /// and left to right, and nowhere else on the sheet. Under it, the key
    /// id's first eight digits and the minute it was printed, in UTC; every
    /// fixed sentence in Arabic, then in English.
    #[test]
    fn recovery_sheet_sets_the_code_in_three_lines_of_two_groups() {
        let sheet = sheet();
        let texts = texts(&sheet);
        let code_style = TextStyle {
            px_per_em: CODE_PX_PER_EM,
            weight: Weight::Bold,
        };
        let code_lines: Vec<&str> = texts
            .iter()
            .filter(|(_, style, _)| *style == code_style)
            .map(|(text, _, rtl)| {
                assert!(!rtl, "the code is set left to right");
                text.as_str()
            })
            .collect();
        assert_eq!(
            code_lines,
            ["12340-56781", "90122-34563", "78904-13575"],
            "three lines of two groups, in the order they are typed"
        );
        const { assert!(CODE_PX_PER_EM > BODY_PX_PER_EM) };
        for group in CODE.split('-') {
            let carrying = texts.iter().filter(|(text, ..)| text.contains(group));
            assert_eq!(carrying.count(), 1, "group {group} is printed once");
        }

        let all: Vec<&str> = texts.iter().map(|(text, ..)| text.as_str()).collect();
        assert!(all.contains(&"01234567"), "the key id's first eight digits");
        assert!(!all.iter().any(|text| text.contains("89abcdef")));
        assert!(all.contains(&"2026-11-13 17:25 UTC"), "{all:?}");
        let titles = [words::TITLE_AR, words::TITLE_EN];
        assert_eq!(
            all.get(..2),
            Some(titles.as_slice()),
            "Arabic first, then English"
        );
        for (arabic, english) in [
            (words::KEEP_AR, words::KEEP_EN),
            (words::LOST_AR, words::LOST_EN),
            (words::HOLDER_AR, words::HOLDER_EN),
        ] {
            let at = all.iter().position(|text| *text == arabic).unwrap();
            assert_eq!(all.get(at + 1), Some(&english));
            let rtl = |wanted: &str| texts.iter().find(|(text, ..)| text == wanted).unwrap().2;
            assert!(rtl(arabic) && !rtl(english), "each in its own direction");
        }
    }

    /// The sheet fits both papers: no glyph outside the page, and each line
    /// of the code on one row of its own, never broken.
    #[test]
    fn recovery_sheet_fits_both_papers_with_each_code_line_whole() {
        let faces = Faces::load().unwrap();
        let sheet = sheet();
        for paper in PaperWidth::ALL {
            let page = sheet.lay_out(&faces, paper).unwrap();
            let width = i32::try_from(paper.width_px()).unwrap();
            for glyph in page.lines.iter().flat_map(|line| &line.glyphs) {
                assert!((0..width).contains(&glyph.x_px), "{paper:?}: {glyph:?}");
            }
            for line in &sheet.lines {
                let alone = layout::lay_out_blocks(
                    &faces,
                    &[Block::Text {
                        text: line.as_str(),
                        style: TextStyle {
                            px_per_em: CODE_PX_PER_EM,
                            weight: Weight::Bold,
                        },
                        rtl: false,
                        alignment: Alignment::Center,
                    }],
                    paper,
                    TextDirection::RightToLeft,
                )
                .unwrap();
                assert_eq!(alone.lines.len(), 1, "{paper:?}: a code line is whole");
            }

            let rendered = sheet.render(paper).unwrap();
            assert_eq!(rendered.bitmap().width_px, paper.width_px());
            assert!(rendered.bitmap().ink() > 0, "{paper:?}");
        }
    }

    /// Whatever the printer answers, the sheet is sent to it exactly once,
    /// whole, and its answer is returned as it was given: a refusal is never
    /// tried again here.
    #[test]
    fn recovery_sheet_is_sent_once_whatever_the_printer_answers() {
        let profile = PrinterProfile::simulator(PaperWidth::Mm80);
        for outcome in [
            PrintOutcome::Printed,
            PrintOutcome::Failed(FailReason::PaperOut),
            PrintOutcome::Failed(FailReason::CoverOpen),
            PrintOutcome::Failed(FailReason::Offline),
            PrintOutcome::Failed(FailReason::Unreachable),
            PrintOutcome::Partial { acknowledged: 7 },
            PrintOutcome::Unknown,
        ] {
            let printer = Answers::new(outcome);
            assert_eq!(sheet().print(&printer, &profile).unwrap(), outcome);
            let sent = printer.sent.lock().unwrap();
            assert_eq!(sent.len(), 1, "{outcome:?}: sent once");
            let document = sent.first().unwrap();
            assert!(document.starts_with(&[0x1B, 0x40]), "it begins ESC @");
            assert!(
                document.ends_with(&[0x1D, 0x56, 0x01]),
                "and ends with a cut"
            );
        }
    }

    /// A profile whose paper the build does not render for is refused before
    /// anything is sent.
    #[test]
    fn recovery_sheet_is_not_sent_for_a_paper_it_cannot_render() {
        let mut profile = PrinterProfile::simulator(PaperWidth::Mm80);
        profile.dot_width = Fact::Unknown;
        let printer = Answers::new(PrintOutcome::Printed);
        assert_eq!(
            sheet().print(&printer, &profile).unwrap_err(),
            SheetError::NoPaper {
                profile_id: profile.profile_id.clone()
            }
        );
        assert!(printer.sent.lock().unwrap().is_empty());
    }

    /// The sheet reaches 1.7.12's emulated printer over TCP 9100 as one
    /// document, cut, whose page is the sheet's own, dot for dot; and a
    /// printer out of paper is refused before a byte, with nothing printed.
    #[test]
    fn recovery_sheet_reaches_the_emulated_printer_as_rendered() {
        let profile = PrinterProfile::emulator(PaperWidth::Mm80);
        let emulator = Emulator::start(EmulatorConfig {
            printer: profile.clone(),
            drawer: None,
            listen: "127.0.0.1:0".parse().unwrap(),
            out_dir: None,
        })
        .unwrap();
        let printer = NetworkPrinter::tcp(emulator.address(), profile.clone()).unwrap();

        emulator.script(ConnectionFault::PaperOut);
        assert_eq!(
            sheet().print(&printer, &profile).unwrap(),
            PrintOutcome::Failed(FailReason::PaperOut)
        );
        assert!(emulator.settled(1, Duration::from_secs(5)));
        assert!(emulator.jobs().is_empty(), "nothing reached paper");

        assert_eq!(
            sheet().print(&printer, &profile).unwrap(),
            PrintOutcome::Printed
        );
        assert!(emulator.settled(2, Duration::from_secs(5)));
        let jobs = emulator.jobs();
        assert_eq!(jobs.len(), 1, "one sheet");
        let job = jobs.first().unwrap();
        assert!(job.cut);
        let expected = sheet().render(PaperWidth::Mm80).unwrap();
        assert_eq!(
            &job.page,
            expected.bitmap(),
            "the page printed is the sheet"
        );
    }

    /// Neither the sheet nor its page shows the code in a log.
    #[test]
    fn recovery_sheet_never_shows_its_code_in_a_log() {
        let sheet = sheet();
        assert_eq!(format!("{sheet:?}"), "RecoverySheet(redacted)");
        let page = sheet.render(PaperWidth::Mm58).unwrap();
        let shown = format!("{page:?}");
        assert!(shown.starts_with("SheetPage(384x") && shown.ends_with(", redacted)"));
        for group in CODE.split('-') {
            assert!(!shown.contains(group));
        }
    }
}
