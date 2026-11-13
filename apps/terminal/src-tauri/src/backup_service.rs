//! The register's backup worker, off the command path (microstep 1.8.6, its
//! first part).
//!
//! **The till takes its own backups,** by the owner's decision of 13 November
//! 2026: no step of the plan owned the hourly job, and a backup that waits for
//! a person to remember it is the one that does not exist on the day it is
//! needed. It runs beside the print worker, once as it starts and then every
//! [`EVERY`].
//!
//! **A sale comes first.** Until 1.8.0's storage gate allows a second source
//! connection, the copy runs on the register's own, a few pages at a time
//! ([`pos_db::backup::PAGES_PER_STEP`]). Between steps it asks whether a
//! command is waiting for the connection, and gives way to it if one is
//! ([`Register::with_db_giving_way`]): the copy is abandoned and tried again
//! after [`AFTER_GIVING_WAY`]. The connection is held for the copy alone;
//! verifying and publishing the generation need none.
//!
//! **What it needs:** the register's confirmed recovery envelope, beside the
//! database (1.8.5b), and the data key from the credential store. A register
//! with no confirmed envelope, such as a seeded demonstration register, is not
//! backed up, and the log says why: every backup carries the envelope the
//! merchant's recovery code opens.
//!
//! **Where:** [`BACKUPS_DIR`], beside the database. 1.8.6b adds the
//! off-machine destination and reports each one's age to device health.
//!
//! **The anchor.** Each verified generation's audit head is recorded in
//! `audit_checkpoint` under the generation's name, with its delivery envelope
//! in one transaction (I-9), unless an earlier backup already anchored that
//! same head: an hour with no audit row leaves the head where it was.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use pos_db::DbError;
use pos_db::backup::{self, BackupError, BackupInfo, Step};
use pos_db::recovery::{ENVELOPE_FILE, RecoveryEnvelope};
use pos_db::repo::checkpoint::{AuditCheckpoint, CheckpointRepository, CheckpointSource};
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_domain::Timestamp;
use rusqlite::Connection;

use crate::payload::checkpoint_payload;
use crate::provisioning::KeyCustody;
use crate::state::{PRODUCER_VERSION, Register, SYNC_PROTOCOL_VERSION};

/// How often a backup is taken: merchant decision 13.3's hourly generation.
pub const EVERY: Duration = Duration::from_secs(60 * 60);

/// How soon a backup that gave way to a sale is tried again.
pub const AFTER_GIVING_WAY: Duration = Duration::from_secs(30);

/// The local destination, beside the database.
pub const BACKUPS_DIR: &str = "backups";

/// The worker's thread name.
pub const WORKER: &str = "backup-worker";

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken {
    /// The generation published.
    pub generation: String,
    /// Whether its audit head was recorded as a new checkpoint; `false` when
    /// an earlier backup had already anchored the same head.
    pub anchored: bool,
    /// The generations retention removed.
    pub pruned: Vec<String>,
}

/// Why a pass took no backup.
#[derive(Debug, thiserror::Error)]
pub enum TakeError {
    /// No confirmed recovery envelope beside the database.
    #[error("the register has no confirmed recovery envelope, and no backup is taken without one")]
    NoEnvelope,
    /// The data key could not be read from the credential store.
    #[error("the database key could not be had: {0}")]
    Key(DbError),
    /// A command was waiting for the connection, and the copy gave way.
    #[error("the backup gave way to a command waiting for the database")]
    GaveWay,
    /// The backup, its anchor or its retention failed.
    #[error(transparent)]
    Backup(#[from] DbError),
}

/// Take one backup of `register`, whose files are in `data_dir`, with the key
/// `keys` holds: copy it under the register's connection, giving way to any
/// command, then verify and publish it, record its anchor, and apply
/// retention.
///
/// # Errors
///
/// [`TakeError`], naming why. Nothing is published unless it verified.
pub fn take(register: &Register, data_dir: &Path, keys: &KeyCustody) -> Result<Taken, TakeError> {
    let envelope =
        RecoveryEnvelope::read(&data_dir.join(ENVELOPE_FILE)).map_err(|_| TakeError::NoEnvelope)?;
    let key = keys.existing().map_err(TakeError::Key)?;
    let destination = data_dir.join(BACKUPS_DIR);
    let at = register.now();
    let staged = register
        .with_db_giving_way(|conn, someone_waits| {
            backup::copy(
                conn,
                register.register(),
                &key,
                &envelope,
                &destination,
                at,
                &mut || {
                    if someone_waits() {
                        Step::GiveWay
                    } else {
                        Step::GoOn
                    }
                },
            )
        })
        .map_err(|error| match error {
            DbError::Backup(BackupError::GaveWay) => TakeError::GaveWay,
            other => TakeError::Backup(other),
        })?;
    let published = staged.publish(&key, &envelope)?;
    let anchored = register.with_db(|conn| anchor(register, conn, &published, at))?;
    let pruned = backup::prune(&destination, at)?;
    Ok(Taken {
        generation: published.manifest.generation,
        anchored,
        pruned,
    })
}

/// Record `published`'s audit head in `audit_checkpoint`, with its delivery
/// envelope, in one transaction. `false` when that head is already anchored
/// by a verified backup.
fn anchor(
    register: &Register,
    conn: &mut Connection,
    published: &BackupInfo,
    at: Timestamp,
) -> Result<bool, DbError> {
    let (last_seq, last_hash) =
        published
            .manifest
            .head()
            .ok_or(DbError::Backup(BackupError::Unverified(
                "its manifest's audit head is not a digest",
            )))?;
    let tx = conn.transaction()?;
    if CheckpointRepository::new(&tx).anchored(
        register.register(),
        last_seq,
        &last_hash,
        CheckpointSource::VerifiedBackup,
    )? {
        return Ok(false);
    }
    let checkpoint = AuditCheckpoint {
        id: register.next_id(),
        register: register.register(),
        last_seq,
        last_hash,
        source: CheckpointSource::VerifiedBackup,
        anchor_ref: published.manifest.generation.clone(),
        anchored_at: at,
    };
    let payload = checkpoint_payload(&checkpoint).map_err(|_| {
        DbError::Backup(BackupError::Io(
            "the checkpoint could not be written as JSON".to_owned(),
        ))
    })?;
    let commit_id = register.next_id();
    let change_id = register.next_id();
    let created_at = at.to_iso8601();
    OutboxRepository::new(&tx).write_commit(
        &tx,
        &CommitEnvelope {
            commit_id: commit_id.as_bytes(),
            protocol_version: SYNC_PROTOCOL_VERSION,
            schema_version: pos_db::SCHEMA_VERSION,
            producer_version: PRODUCER_VERSION,
            created_at: &created_at,
        },
        &[FactMember {
            change_id: change_id.as_bytes(),
            entity: "audit_checkpoint",
            entity_id: checkpoint.id.as_bytes(),
            payload: &payload,
        }],
    )?;
    CheckpointRepository::new(&tx).record(&tx, &checkpoint)?;
    tx.commit()?;
    Ok(true)
}

/// The running worker. Dropping it stops the worker after its current pass.
pub struct BackupService {
    stopping: Arc<AtomicBool>,
    nudge: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

/// Start the register's backup worker: a pass now, then one every `every`,
/// and one [`AFTER_GIVING_WAY`] after a pass that gave way.
///
/// # Errors
///
/// The operating system refused the thread.
pub fn start(
    register: &Arc<Register>,
    data_dir: &Path,
    keys: KeyCustody,
    every: Duration,
) -> Result<BackupService, std::io::Error> {
    let (nudge, wake) = mpsc::channel();
    let stopping = Arc::new(AtomicBool::new(false));
    let weak = Arc::downgrade(register);
    let stop = Arc::clone(&stopping);
    let data_dir = data_dir.to_path_buf();
    let thread = std::thread::Builder::new()
        .name(WORKER.to_owned())
        .spawn(move || run(&weak, &data_dir, &keys, &wake, every, &stop))?;
    Ok(BackupService {
        stopping,
        nudge,
        thread: Some(thread),
    })
}

impl BackupService {
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

impl Drop for BackupService {
    fn drop(&mut self) {
        self.halt();
    }
}

fn run(
    register: &Weak<Register>,
    data_dir: &Path,
    keys: &KeyCustody,
    wake: &Receiver<()>,
    every: Duration,
    stopping: &AtomicBool,
) {
    loop {
        if stopping.load(Ordering::SeqCst) {
            return;
        }
        let Some(till) = register.upgrade() else {
            return;
        };
        let next = match take(&till, data_dir, keys) {
            Ok(taken) => {
                tracing::info!(
                    generation = %taken.generation,
                    anchored = taken.anchored,
                    pruned = taken.pruned.len(),
                    "a verified backup was published"
                );
                every
            }
            Err(TakeError::GaveWay) => AFTER_GIVING_WAY,
            Err(error) => {
                tracing::warn!(error = %error, "no backup was taken; the worker tries again on its next pass");
                every
            }
        };
        drop(till);
        match wake.recv_timeout(next) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

