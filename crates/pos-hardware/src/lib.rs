//! Hardware abstraction layer (blueprint §5).
//! Traits here; drivers (ESC/POS over TCP/serial/USB) in submodules later.

use std::sync::{Mutex, MutexGuard};

#[derive(Debug, thiserror::Error)]
pub enum HwError {
    #[error("device offline")]
    Offline,
    #[error("io: {0}")]
    Io(String),
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
}

pub trait ReceiptPrinter: Send + Sync {
    fn print(&self, doc: &RenderedReceipt) -> Result<(), HwError>;
    /// ESC p pulse via the printer's drawer port (blueprint §5).
    fn open_drawer(&self) -> Result<(), HwError>;
    fn status(&self) -> PrinterStatus;
}

// pub trait BarcodeSource { .. }    // Phase 1: keyboard-wedge + serial modes
// pub trait PaymentTerminal { .. }  // Phase 2: semi-integrated PSP terminals (§6)

/// In-memory printer: captures everything, fails on demand.
/// This is what CI and laptop development run against.
#[derive(Default)]
pub struct SimulatedPrinter {
    pub printed: Mutex<Vec<RenderedReceipt>>,
    pub drawer_opens: Mutex<u32>,
    pub force_status: Mutex<PrinterStatus>,
}

impl SimulatedPrinter {
    pub fn new() -> Self {
        Self {
            printed: Mutex::new(Vec::new()),
            drawer_opens: Mutex::new(0),
            force_status: Mutex::new(PrinterStatus::Ready),
        }
    }
}

/// Recover from a poisoned lock rather than panicking: a printer simulator
/// must never be the reason a register dies (conventions §4).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl ReceiptPrinter for SimulatedPrinter {
    fn print(&self, doc: &RenderedReceipt) -> Result<(), HwError> {
        match self.status() {
            PrinterStatus::Ready => {
                lock(&self.printed).push(doc.clone());
                Ok(())
            }
            _ => Err(HwError::Offline),
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

    #[test]
    fn simulator_captures_prints_and_drawer_kicks() {
        let p = SimulatedPrinter::new();
        p.print(&RenderedReceipt {
            bytes: b"\x1b@RECEIPT".to_vec(),
        })
        .unwrap();
        p.open_drawer().unwrap();

        assert_eq!(p.printed.lock().unwrap().len(), 1);
        assert_eq!(*p.drawer_opens.lock().unwrap(), 1);
    }

    #[test]
    fn simulator_fails_when_paper_out() {
        let p = SimulatedPrinter::new();
        *p.force_status.lock().unwrap() = PrinterStatus::PaperOut;
        assert!(p.print(&RenderedReceipt { bytes: vec![] }).is_err());
    }
}
