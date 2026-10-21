//! Hardware abstraction layer (blueprint §5).
//! Traits here; the network printer driver in `net`, over `transport`.

pub mod escpos;
pub mod font;
pub mod net;
pub mod profile;
pub mod render;
pub mod transport;

use std::sync::{Mutex, MutexGuard};

#[derive(Debug, thiserror::Error)]
pub enum HwError {
    #[error("device offline")]
    Offline,
    #[error("io: {0}")]
    Io(String),
    /// A profile whose device is reached another way, handed to a driver for
    /// this one.
    #[error("profile {profile_id} is reached by {transport}, not by this driver")]
    WrongTransport {
        profile_id: String,
        transport: String,
    },
    /// The profile does not know a value the command needs, so the command is
    /// refused rather than sent with a guess.
    #[error(transparent)]
    Profile(#[from] profile::ProfileError),
    /// A drawer pulse time that does not fit `ESC p`'s one byte of 2 ms units.
    #[error("a drawer pulse of {ms} ms does not fit ESC p, whose longest is 510 ms")]
    PulseOutOfRange { ms: u16 },
}

/// Receipt bytes already rendered to ESC/POS by the template engine.
/// Rendering lives in domain/templates; this layer only moves bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedReceipt {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrinterStatus {
    #[default]
    Ready,
    PaperOut,
    CoverOpen,
    Offline,
    /// The device answers no real-time status. Neither ready nor offline: a
    /// fact about the device rather than about this moment, and the reason §3
    /// rule 1 cannot warn before tender on it (1.7.10).
    Unsupported,
}

/// Why a device refused a document before accepting a byte of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailReason {
    PaperOut,
    CoverOpen,
    Offline,
    /// No connection could be made, or the device closed it before taking a
    /// byte (1.7.10).
    Unreachable,
}

/// What became of one document sent to a printer — the four outcomes of
/// `ref/hardware-and-receipts.md` §3, and only these (microstep 1.7.8).
///
/// The classification is the whole point. Only `Failed` proves that no paper
/// exists, so only `Failed` may be retried, and only by the print queue. A
/// byte-exact retry of `Partial` or `Unknown` could print a second **original**
/// — the same receipt number, with nothing marking it a copy — so those are
/// resolved by a person, into a duplicate (§3, rules 4 and 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintOutcome {
    /// Every byte was accepted, and the device reported no fault.
    Printed,
    /// The device refused before accepting a byte: no paper can exist.
    Failed(FailReason),
    /// The device accepted `acknowledged` bytes and then failed: some paper
    /// may exist, but not the whole document.
    Partial { acknowledged: usize },
    /// The device went silent or missed its deadline: paper may exist, and
    /// nothing inside the machine can say how much.
    Unknown,
}

pub trait ReceiptPrinter: Send + Sync {
    /// Send one document and classify what happened to it. Never retries:
    /// retrying is the print queue's decision, and only for `Failed`.
    fn print(&self, doc: &RenderedReceipt) -> PrintOutcome;
    /// ESC p pulse via the printer's drawer port (blueprint §5).
    fn open_drawer(&self) -> Result<(), HwError>;
    fn status(&self) -> PrinterStatus;
}

// pub trait BarcodeSource { .. }    // Phase 1: keyboard-wedge + serial modes
// pub trait PaymentTerminal { .. }  // Phase 2: semi-integrated PSP terminals (§6)

/// A fault the simulator injects into the next document it is sent, so every
/// outcome can be produced without a device (microstep 1.7.8). A status fault
/// — paper-out, cover-open or offline before the document starts — is
/// [`SimulatedPrinter::force_status`] instead, because it is a state the
/// printer is in rather than something that happens mid-document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fault {
    /// Every document prints.
    #[default]
    None,
    /// The paper runs out after this many bytes of the document. At zero the
    /// device refuses before accepting a byte; at or past the document's end
    /// the document fits. The printer reports paper-out afterwards either way
    /// the paper ran out mid-document.
    PaperOutAfter(usize),
    /// The device stops answering after this many bytes — a peer gone silent.
    SilentAfter(usize),
    /// The device prints the document, too slowly to say so before the
    /// deadline: the paper exists and the register cannot know it.
    Slow,
}

/// In-memory printer: captures everything, fails on demand.
/// This is what CI and laptop development run against.
#[derive(Default)]
pub struct SimulatedPrinter {
    /// Every whole document that reached paper.
    pub printed: Mutex<Vec<RenderedReceipt>>,
    /// The bytes of every document that reached paper only in part — a torn
    /// receipt, which is paper the queue must never print a second original
    /// over.
    pub torn: Mutex<Vec<usize>>,
    pub drawer_opens: Mutex<u32>,
    pub force_status: Mutex<PrinterStatus>,
    /// The fault the next document meets. It is spent by that document.
    pub fault: Mutex<Fault>,
}

impl SimulatedPrinter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Script the fault the next document meets.
    pub fn script(&self, fault: Fault) {
        *lock(&self.fault) = fault;
    }
}

/// Recover from a poisoned lock rather than panicking: a printer simulator
/// must never be the reason a register dies (conventions §4).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl ReceiptPrinter for SimulatedPrinter {
    fn print(&self, doc: &RenderedReceipt) -> PrintOutcome {
        let refused = match self.status() {
            PrinterStatus::Ready | PrinterStatus::Unsupported => None,
            PrinterStatus::PaperOut => Some(FailReason::PaperOut),
            PrinterStatus::CoverOpen => Some(FailReason::CoverOpen),
            PrinterStatus::Offline => Some(FailReason::Offline),
        };
        if let Some(reason) = refused {
            return PrintOutcome::Failed(reason);
        }

        let length = doc.bytes.len();
        let fault = std::mem::take(&mut *lock(&self.fault));
        match fault {
            Fault::None => {
                lock(&self.printed).push(doc.clone());
                PrintOutcome::Printed
            }
            Fault::PaperOutAfter(0) => {
                *lock(&self.force_status) = PrinterStatus::PaperOut;
                PrintOutcome::Failed(FailReason::PaperOut)
            }
            Fault::PaperOutAfter(at) | Fault::SilentAfter(at) if at >= length => {
                lock(&self.printed).push(doc.clone());
                PrintOutcome::Printed
            }
            Fault::PaperOutAfter(at) => {
                *lock(&self.force_status) = PrinterStatus::PaperOut;
                lock(&self.torn).push(at);
                PrintOutcome::Partial { acknowledged: at }
            }
            Fault::SilentAfter(at) => {
                if at > 0 {
                    lock(&self.torn).push(at);
                }
                PrintOutcome::Unknown
            }
            Fault::Slow => {
                lock(&self.printed).push(doc.clone());
                PrintOutcome::Unknown
            }
        }
    }

    fn open_drawer(&self) -> Result<(), HwError> {
        *lock(&self.drawer_opens) += 1;
        Ok(())
    }

    fn status(&self) -> PrinterStatus {
        *lock(&self.force_status)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::escpos::encode;
    use crate::profile::PrinterProfile;
    use crate::render::tests_support::model;
    use crate::render::{PaperWidth, render_receipt};

    #[test]
    fn simulator_captures_prints_and_drawer_kicks() {
        let p = SimulatedPrinter::new();
        assert_eq!(
            p.print(&RenderedReceipt {
                bytes: b"\x1b@RECEIPT".to_vec(),
            }),
            PrintOutcome::Printed
        );
        p.open_drawer().unwrap();

        assert_eq!(p.printed.lock().unwrap().len(), 1);
        assert_eq!(*p.drawer_opens.lock().unwrap(), 1);
    }

    #[test]
    fn simulator_fails_when_paper_out() {
        let p = SimulatedPrinter::new();
        *p.force_status.lock().unwrap() = PrinterStatus::PaperOut;
        assert_eq!(
            p.print(&RenderedReceipt { bytes: vec![] }),
            PrintOutcome::Failed(FailReason::PaperOut)
        );
    }

    /// A status the printer is in refuses the document before its first byte,
    /// naming the reason, and nothing reaches paper.
    #[test]
    fn a_printer_that_is_not_ready_fails_before_a_byte() {
        for (status, reason) in [
            (PrinterStatus::PaperOut, FailReason::PaperOut),
            (PrinterStatus::CoverOpen, FailReason::CoverOpen),
            (PrinterStatus::Offline, FailReason::Offline),
        ] {
            let p = SimulatedPrinter::new();
            *p.force_status.lock().unwrap() = status;
            let doc = RenderedReceipt {
                bytes: b"\x1b@".to_vec(),
            };
            assert_eq!(p.print(&doc), PrintOutcome::Failed(reason), "{status:?}");
            assert!(p.printed.lock().unwrap().is_empty());
            assert!(p.torn.lock().unwrap().is_empty());
        }
    }

    /// A slow device prints the document after its deadline: the register sees
    /// `Unknown` while the paper exists — which is exactly why `Unknown` is
    /// never retried automatically.
    #[test]
    fn a_slow_printer_is_unknown_though_its_paper_exists() {
        let p = SimulatedPrinter::new();
        p.script(Fault::Slow);
        let doc = RenderedReceipt {
            bytes: b"\x1b@RECEIPT".to_vec(),
        };
        assert_eq!(p.print(&doc), PrintOutcome::Unknown);
        assert_eq!(p.printed.lock().unwrap().len(), 1, "the paper exists");
        // The fault is spent: the next document prints.
        assert_eq!(p.print(&doc), PrintOutcome::Printed);
    }

    /// What one fault at byte `at` of an `length`-byte document must produce.
    fn expected(fault: Fault, length: usize) -> PrintOutcome {
        match fault {
            Fault::PaperOutAfter(0) => PrintOutcome::Failed(FailReason::PaperOut),
            Fault::PaperOutAfter(at) | Fault::SilentAfter(at) if at >= length => {
                PrintOutcome::Printed
            }
            Fault::PaperOutAfter(at) => PrintOutcome::Partial { acknowledged: at },
            Fault::SilentAfter(_) | Fault::Slow => PrintOutcome::Unknown,
            Fault::None => PrintOutcome::Printed,
        }
    }

    /// The named test (§3, "a failure is injected at every byte boundary, not
    /// at a convenient one"): paper-out and silence are scripted at every byte
    /// of a real receipt's stream, and each yields exactly its one outcome,
    /// never moves the drawer, and leaves at most one document's paper — a
    /// whole one only when the outcome says `Printed`.
    ///
    /// The stream is the 80 mm receipt the goldens will be rendered from, until
    /// 1.7.5 commits them; the other half of the rule — that no second
    /// *original artifact* is created — needs 1.7.7's queue, which is why
    /// 1.7.8 carries a `Full-step status:` marker.
    #[test]
    fn simulator_fails_midway_when_scripted() {
        let bitmap = render_receipt(&model(), PaperWidth::Mm80).expect("renders");
        let profile = PrinterProfile::simulator(PaperWidth::Mm80);
        let doc = RenderedReceipt {
            bytes: encode(&bitmap, &profile).expect("encodes"),
        };
        let length = doc.bytes.len();
        assert!(length > 1_000, "a real receipt, not a token stream");

        let mut seen = [0_usize; 4];
        for at in 0..=length + 1 {
            for fault in [Fault::PaperOutAfter(at), Fault::SilentAfter(at)] {
                let p = SimulatedPrinter::new();
                p.script(fault);
                let outcome = p.print(&doc);
                assert_eq!(outcome, expected(fault, length), "{fault:?}");

                assert_eq!(*lock(&p.drawer_opens), 0, "{fault:?} moved the drawer");
                let whole = lock(&p.printed).len();
                let torn = lock(&p.torn).len();
                assert!(whole + torn <= 1, "{fault:?} left two documents' paper");
                assert_eq!(
                    whole == 1,
                    outcome == PrintOutcome::Printed,
                    "{fault:?}: a whole document exists exactly when the outcome is Printed"
                );

                let slot = match outcome {
                    PrintOutcome::Printed => 0,
                    PrintOutcome::Failed(_) => 1,
                    PrintOutcome::Partial { .. } => 2,
                    PrintOutcome::Unknown => 3,
                };
                if let Some(count) = seen.get_mut(slot) {
                    *count += 1;
                }
            }
        }
        assert!(
            seen.iter().all(|count| *count > 0),
            "the sweep reached all four outcomes: {seen:?}"
        );
    }

    /// Paper running out mid-document leaves the printer out of paper, so the
    /// queue's retry of the next `Failed` document cannot print over a torn
    /// one by accident.
    #[test]
    fn paper_out_midway_leaves_the_printer_out_of_paper() {
        let p = SimulatedPrinter::new();
        p.script(Fault::PaperOutAfter(3));
        let doc = RenderedReceipt {
            bytes: b"\x1b@RECEIPT".to_vec(),
        };
        assert_eq!(p.print(&doc), PrintOutcome::Partial { acknowledged: 3 });
        assert_eq!(p.status(), PrinterStatus::PaperOut);
        assert_eq!(p.print(&doc), PrintOutcome::Failed(FailReason::PaperOut));
    }
}
