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

#[cfg(test)]
mod tests {
    //! The till's half of 1.8.6's first part. The register is the suites'
    //! shop, its credential store held in memory: the machine's real
    //! Keychain is never touched.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::sync::atomic::AtomicI64;
    use std::time::Instant;

    use pos_db::key::{CredentialStore, DatabaseKey, MemoryCredentialStore};
    use pos_db::recovery::{ENVELOPE_FORMAT, KDF_ALGORITHM, KdfParams, WRAP_ALGORITHM};
    use pos_domain::Clock;

    use super::*;
    use crate::commands::auth::{login_pin, logout};
    use crate::shop::{AT, CASHIER_CODE, CASHIER_PIN, KEY, seed};
    use crate::state::V7Ids;

    /// A clock the test moves.
    #[derive(Clone)]
    struct Hands(Arc<AtomicI64>);

    impl Hands {
        fn new() -> Self {
            Hands(Arc::new(AtomicI64::new(
                Timestamp::parse_iso8601(AT).unwrap().epoch_milliseconds(),
            )))
        }

        fn forward(&self, minutes: i64) {
            self.0.fetch_add(minutes * 60 * 1_000, Ordering::SeqCst);
        }
    }

    impl Clock for Hands {
        fn now(&self) -> Timestamp {
            Timestamp::from_epoch_milliseconds(self.0.load(Ordering::SeqCst)).unwrap()
        }
    }

    fn key() -> DatabaseKey {
        DatabaseKey::new(KEY.to_owned())
    }

    /// The register's key in a credential store held in memory.
    fn keys() -> KeyCustody {
        let store = Arc::new(MemoryCredentialStore::new());
        store.write(&key()).unwrap();
        KeyCustody::in_store(store)
    }

    /// A confirmed envelope for the shop's key. Nothing here unwraps it: what
    /// travels is what is checked, and 1.8.6's `pos-db` half opens one with
    /// its recovery code.
    fn confirm_envelope(dir: &Path) -> RecoveryEnvelope {
        let envelope = RecoveryEnvelope {
            format: ENVELOPE_FORMAT,
            key_id: key().id(),
            wrap_algorithm: WRAP_ALGORITHM.to_owned(),
            kdf_algorithm: KDF_ALGORITHM.to_owned(),
            kdf_params: KdfParams::fresh().unwrap(),
            wrapped_key: vec![0x5A; 72],
        };
        envelope.write(&dir.join(ENVELOPE_FILE)).unwrap();
        envelope
    }

    /// The shop's register file, opened at `hands`.
    fn till(dir: &Path, hands: &Hands) -> Arc<Register> {
        Arc::new(
            Register::open_with(
                dir,
                KEY,
                Box::new(hands.clone()),
                Box::new(V7Ids::default()),
            )
            .unwrap(),
        )
    }

    fn checkpoints(register: &Register) -> Vec<(String, i64, String)> {
        register.with_db(|conn| {
            conn.prepare(
                "SELECT anchor_ref, last_seq, source_kind FROM audit_checkpoint ORDER BY anchored_at",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
        })
    }

    fn enveloped_checkpoints(register: &Register) -> i64 {
        register.with_db(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM fact_commit_member m
                   JOIN audit_checkpoint c ON c.id = m.entity_id
                  WHERE m.entity = 'audit_checkpoint'",
                [],
                |row| row.get(0),
            )
            .unwrap()
        })
    }

    /// The till backs itself up with its envelope, verified, and records the
    /// generation's audit head as a checkpoint with its delivery envelope. An
    /// hour with no audit row leaves the head where it was, and the next
    /// backup records nothing new; a sign-in moves the head, and the backup
    /// after it anchors the new one.
    #[test]
    fn the_till_backs_up_and_anchors_each_new_audit_head_once() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), true);
        let envelope = confirm_envelope(dir.path());
        let hands = Hands::new();
        let register = till(dir.path(), &hands);
        login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();

        let first = take(&register, dir.path(), &keys()).unwrap();
        assert!(first.anchored);
        let generation = dir.path().join(BACKUPS_DIR).join(&first.generation);
        let info = backup::verify(&generation, &key(), &envelope).unwrap();
        assert!(
            info.manifest.last_seq > 0,
            "the sign-in's audit row is anchored"
        );
        assert_eq!(
            checkpoints(&register),
            [(
                first.generation.clone(),
                i64::try_from(info.manifest.last_seq).unwrap(),
                "verified_backup".to_owned()
            )]
        );
        assert_eq!(
            enveloped_checkpoints(&register),
            1,
            "with its delivery envelope"
        );

        hands.forward(60);
        let quiet = take(&register, dir.path(), &keys()).unwrap();
        assert!(!quiet.anchored, "the same head is anchored once");
        assert_eq!(checkpoints(&register).len(), 1);

        hands.forward(60);
        logout(&register).unwrap();
        login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
        let moved = take(&register, dir.path(), &keys()).unwrap();
        assert!(moved.anchored);
        assert_eq!(checkpoints(&register).len(), 2);
        assert_eq!(enveloped_checkpoints(&register), 2);
        assert_eq!(
            backup::generations(&dir.path().join(BACKUPS_DIR))
                .unwrap()
                .len(),
            3,
            "three hours, three generations"
        );
    }

    /// A register with no confirmed recovery envelope beside its database,
    /// such as a seeded demonstration register, is not backed up.
    #[test]
    fn a_register_with_no_confirmed_envelope_is_not_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), true);
        let register = till(dir.path(), &Hands::new());
        assert!(matches!(
            take(&register, dir.path(), &keys()),
            Err(TakeError::NoEnvelope)
        ));
        assert!(!dir.path().join(BACKUPS_DIR).exists());
        assert!(checkpoints(&register).is_empty());
    }

    /// A command waiting for the database is seen by the work holding it,
    /// which is how a backup knows to give way, and is served once that work
    /// lets go.
    #[test]
    fn a_waiting_command_is_seen_by_the_backup_and_served_after_it() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), true);
        let register = till(dir.path(), &Hands::new());
        let other = Arc::clone(&register);
        let saw = register.with_db_giving_way(|_, someone_waits| {
            assert!(!someone_waits(), "nobody waits yet");
            let waiter = std::thread::spawn(move || other.with_db(|_| "served"));
            let start = Instant::now();
            while !someone_waits() && start.elapsed() < Duration::from_secs(5) {
                std::thread::yield_now();
            }
            (someone_waits(), waiter)
        });
        assert!(saw.0, "the waiting command was seen");
        assert_eq!(saw.1.join().unwrap(), "served");
        register.with_db_giving_way(|_, someone_waits| assert!(!someone_waits()));
    }

    /// The worker takes a backup as it starts, and stops with its service.
    #[test]
    fn the_backup_worker_takes_one_as_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), true);
        let envelope = confirm_envelope(dir.path());
        let register = till(dir.path(), &Hands::new());
        let worker = start(&register, dir.path(), keys(), EVERY).unwrap();
        let destination = dir.path().join(BACKUPS_DIR);
        let start = Instant::now();
        while backup::generations(&destination).unwrap().is_empty()
            && start.elapsed() < Duration::from_secs(20)
        {
            std::thread::sleep(Duration::from_millis(50));
        }
        worker.stop();
        let taken = backup::generations(&destination).unwrap();
        assert_eq!(
            taken.len(),
            1,
            "one as it starts, and the next an hour away"
        );
        backup::verify(&taken.first().unwrap().path, &key(), &envelope).unwrap();
    }
}
