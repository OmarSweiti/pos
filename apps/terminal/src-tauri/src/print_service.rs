//! The register's print worker, off the command path (microstep 1.7.14c,
//! `ref/hardware-and-receipts.md` §3).
//!
//! **A slow printer holds no command.** The finalize writes every original
//! receipt and its print job in the sale's own transaction, kicks the drawer,
//! and hands the receipt to this worker. It answers the cashier at once, and
//! never waits on the device (the owner's decision of 11 November 2026: hand
//! off, report later). What the device then does is the print queue's to
//! record, and a print that fails is raised on `health_status`'s strip.
//!
//! **The worker holds the register's database only to lease and to record.**
//! [`drain`] takes the register's connection to lease a job, lets it go while
//! the device is written to, and takes it again to append what the device did
//! ([`PrintWorker::lease`], [`PrintWorker::print`], [`PrintWorker::record`]).
//! So a sale, a scan or a sign-in proceeds while a long receipt prints.
//!
//! **It wakes when it is nudged, and on a timer.** A finalize nudges it; it
//! drains once as it starts, so receipts kept while no printer was bound print
//! as soon as one is; and it wakes every [`POLL`] for a refused receipt whose
//! retry has come due. With no printer bound, a wake does nothing.
//!
//! **It holds the register weakly.** The till owns the register; the worker
//! stops once the register is gone, or when its [`PrintService`] is stopped.
//!
//! Everything the queue's own rules require — only a refusal retried, a lapsed
//! lease resolved as unknown, the drawer never kicked, the bytes checked
//! against their digest and their profile before they leave — is
//! [`crate::print_queue`]'s, and holds here unchanged.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use pos_domain::{Clock, IdSource, Timestamp};
use uuid::Uuid;

use crate::print_queue::{Drained, PrintQueueError, PrintWorker};
use crate::state::Register;

/// How often the worker wakes with no nudge: often enough that a refused
/// receipt is tried again soon after its retry comes due (thirty seconds after
/// the refusal, by [`crate::print_queue::Timing`]'s default).
pub const POLL: Duration = Duration::from_secs(5);

/// The name the worker leases a job under.
pub const WORKER: &str = "print-worker";

/// The running worker. Dropping it stops the worker after its current pass.
pub struct PrintService {
    stopping: Arc<AtomicBool>,
    nudge: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

/// Why a print worker could not start.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The register already has a worker: two would race for its jobs.
    #[error("the register already has a print worker")]
    AlreadyRunning,
    /// The operating system refused the thread.
    #[error("the print worker's thread could not start: {0}")]
    Thread(#[from] std::io::Error),
}

/// Start the register's print worker, waking every `poll` with no nudge. It
/// drains once as it starts.
///
/// # Errors
///
/// [`StartError::AlreadyRunning`] when the register already has one, and
/// [`StartError::Thread`] when its thread cannot start.
pub fn start(register: &Arc<Register>, poll: Duration) -> Result<PrintService, StartError> {
    let (nudge, wake) = mpsc::channel();
    if !register.set_print_nudge(nudge.clone()) {
        return Err(StartError::AlreadyRunning);
    }
    let stopping = Arc::new(AtomicBool::new(false));
    let weak = Arc::downgrade(register);
    let stop = Arc::clone(&stopping);
    let thread = std::thread::Builder::new()
        .name(WORKER.to_owned())
        .spawn(move || run(&weak, &wake, poll, &stop))?;
    // Drain once at start: receipts kept while no printer was bound, and any
    // a crash left leased, are taken up now.
    let _ = nudge.send(());
    Ok(PrintService {
        stopping,
        nudge,
        thread: Some(thread),
    })
}

impl PrintService {
    /// Stop the worker after its current pass, and wait for it.
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        let _ = self.nudge.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for PrintService {
    fn drop(&mut self) {
        self.halt();
    }
}

fn run(register: &Weak<Register>, wake: &Receiver<()>, poll: Duration, stopping: &AtomicBool) {
    loop {
        match wake.recv_timeout(poll) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        // Many nudges while one pass ran call for one more pass, not many.
        while wake.try_recv().is_ok() {}
        if stopping.load(Ordering::SeqCst) {
            return;
        }
        let Some(register) = register.upgrade() else {
            return;
        };
        if let Err(error) = drain(&register) {
            tracing::error!(
                error = %error,
                "the print worker stopped this pass; it tries again on its next wake"
            );
        }
    }
}

/// One pass of the worker: resolve lapsed leases, then lease, print and
/// record every due job, once each. It holds the register's connection to
/// lease and to record, never while the device is written to. A register
/// with no printer attached has nothing to drain.
///
/// # Errors
///
/// The first [`PrintQueueError`]; attempts already recorded stay recorded,
/// and a job leased and not recorded is resolved as unknown on a later pass.
pub fn drain(register: &Register) -> Result<Drained, PrintQueueError> {
    let device = register.receipt_device();
    let Some(printer) = device.printer.as_deref() else {
        return Ok(Drained::default());
    };
    let ids = RegisterIds(register);
    let clock = RegisterClock(register);
    let worker = PrintWorker::new(printer, &device.profile.profile_id, &clock, &ids, WORKER);
    let recovered = register.with_db(|conn| worker.recover(conn))?;
    let mut attempted = Vec::new();
    while let Some(leased) = register.with_db(|conn| worker.lease(conn))? {
        // The device, with the register's connection free.
        let answer = worker.print(&leased);
        attempted.push(register.with_db(|conn| worker.record(conn, &leased, &answer))?);
    }
    Ok(Drained {
        recovered,
        attempted,
    })
}

/// The register's ids, for the print worker.
pub(crate) struct RegisterIds<'r>(pub(crate) &'r Register);

impl IdSource for RegisterIds<'_> {
    fn next(&self) -> Uuid {
        self.0.next_id()
    }
}

/// The register's clock, for the print worker.
pub(crate) struct RegisterClock<'r>(pub(crate) &'r Register);

impl Clock for RegisterClock<'_> {
    fn now(&self) -> Timestamp {
        self.0.now()
    }
}
