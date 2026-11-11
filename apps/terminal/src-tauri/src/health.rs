//! The register's device health: `health_status` (`ref/ipc-contract.md` §3,
//! "Health and diagnostics"; `ref/hardware-and-receipts.md` §3a; microstep
//! 1.7.6b).
//!
//! **Selling never stops for a printer.** A printer unplugged at shift open,
//! dead at 09:00 on a Saturday or never bought is the common failure, and a
//! merchant who cannot trade because of it loses a day's takings to a cable.
//! So with no printer, or one that would not print now, a sale is still paid
//! for and completes. Its receipt is kept and queued in the sale's own
//! transaction exactly as always (1.8.3), and the queued receipts print
//! unchanged once a printer can take them. The missing printer is raised
//! here, as an alarm the status strip carries, never as a modal, and an
//! undrained queue raises an alarm and never refuses a sale.
//!
//! **The lock screen reads it before anyone signs in,** so it proves no
//! capability. It answers what the register knows about its own devices: the
//! printer, read now as Pay reads it, and how many receipts wait. It carries
//! no grant, PIN or key, and nothing of any receipt but a count.
//!
//! **A printer binding the register could not honour is the loudest alarm**
//! (1.7.14b). The till opened anyway, with no device, by the owner's decision
//! of 11 November 2026, so the binding's refusal is said here, with why,
//! rather than by a till that will not open.
//!
//! **It holds what exists today.** Backup age (1.8.6), disk space (1.8.8),
//! the fiscal queue (Phase 2) and sync (Phase 3) join this snapshot as the
//! steps that build them land.

use pos_db::DbError;
use pos_db::repo::receipt::{JobState, ReceiptRepository};
use pos_hardware::PrinterAtPay;
use serde::Serialize;
use tauri::State;

use crate::ipc::{IpcError, IpcResult};
use crate::state::{PrinterBindingState, Register, Till};

/// The register's devices as the lock screen and the status strip show them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealthSnapshot {
    pub printer: PrinterHealth,
    /// Everything that needs a person, the loudest first. Each is a line on
    /// the status strip, never a dialogue: none of them stops a sale.
    pub alarms: Vec<Alarm>,
}

/// The receipt printer, and the receipts waiting for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrinterHealth {
    /// What the register's printer binding came to when the till opened.
    pub binding: PrinterBindingState,
    /// The printer as read now, the reading Pay takes (1.7.6).
    pub reading: PrinterAtPay,
    /// Receipts kept and not yet printed, which print once a printer can
    /// take them: queued, being sent, or refused and waiting to be tried
    /// again.
    pub receipts_waiting: usize,
    /// Receipts whose print may have produced paper, which only a person
    /// closes: *did a readable receipt come out?* (§3 rule 5).
    pub receipts_unconfirmed: usize,
}

/// Something a person should see, by its code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Alarm {
    /// A printer binding is set and the register could not honour it, so it
    /// opened with no device. The binding says why.
    PrinterBindingRefused,
    /// No printer, or one that would not print now. Receipts are kept, and
    /// print later.
    PrinterUnavailable,
    /// A print may have produced paper, and a person must say whether it did.
    ReceiptsUnconfirmed,
    /// Receipts wait to be printed.
    ReceiptsWaiting,
}

#[tauri::command]
pub fn health_status(till: State<'_, Till>) -> IpcResult<HealthSnapshot> {
    health(till.register()?)
}

/// `health_status`: the printer, read now, and the receipts waiting for it.
/// The printer is read before the database is, and without any lock, so a
/// network printer's deadlines hold no command.
///
/// # Errors
///
/// `health_unreadable` when the receipt queue cannot be read.
pub fn health(register: &Register) -> IpcResult<HealthSnapshot> {
    let reading = PrinterAtPay::poll(register.receipt_device().printer.as_deref());
    let (waiting, unconfirmed) = register
        .with_db(|conn| -> Result<(usize, usize), DbError> {
            let unprinted = ReceiptRepository::new(conn).unprinted()?;
            let unconfirmed = unprinted
                .iter()
                .filter(|job| job.state == JobState::Unknown)
                .count();
            Ok((unprinted.len() - unconfirmed, unconfirmed))
        })
        .map_err(|error| {
            IpcError::from_source("health_unreadable", "error.health.unreadable", &error)
        })?;
    let binding = register.printer_binding().clone();
    let refused = matches!(binding, PrinterBindingState::Refused { .. });
    let alarms = [
        (refused, Alarm::PrinterBindingRefused),
        (reading.warns(), Alarm::PrinterUnavailable),
        (unconfirmed > 0, Alarm::ReceiptsUnconfirmed),
        (waiting > 0, Alarm::ReceiptsWaiting),
    ]
    .into_iter()
    .filter_map(|(raised, alarm)| raised.then_some(alarm))
    .collect();
    Ok(HealthSnapshot {
        printer: PrinterHealth {
            binding,
            reading,
            receipts_waiting: waiting,
            receipts_unconfirmed: unconfirmed,
        },
        alarms,
    })
}
