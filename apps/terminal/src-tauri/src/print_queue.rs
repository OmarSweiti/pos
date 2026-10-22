//! The register's print worker (microstep 1.7.7, `ref/hardware-and-receipts.md`
//! §3): it drives a printer from `pos-db`'s receipt queue, and records a
//! person's answer when a print's outcome is ambiguous.
//!
//! The queue's rules are `pos-db`'s and `0005`'s; this module is the half that
//! touches a device, and it adds the rules only a device's side can keep:
//!
//! * **No transaction is open while the device is.** A job is leased and that
//!   lease committed, then the bytes are sent, then the outcome is appended in a
//!   second transaction. A printer that takes seconds to answer must not hold the
//!   register's one write lock while a sale waits behind it.
//! * **The bytes are checked before they leave.** A stored artifact whose bytes
//!   no longer match their BLAKE3 digest is never sent, and neither is one
//!   rendered for a printer other than the one this register is bound to (§3
//!   rule 7): either would put garbage in a customer's hand. Both are recorded
//!   as a failure with no retry, which leaves the job on the unprinted worklist
//!   for a person.
//! * **Only a refusal is retried** (§3 rule 4). `Failed` means the device took no
//!   byte, so no paper exists and the retry is free; `Partial` and `Unknown` may
//!   be paper, and are left for a person. The worker never decides otherwise —
//!   the queue would not offer it the job anyway.
//! * **A lapsed lease is ambiguous, not failed.** A worker that died holding a
//!   job may have sent its bytes, so [`PrintWorker::recover`] resolves it as
//!   `unknown`, never back into the queue.
//! * **The worker never opens the drawer** (§3 rule 6, §4). No stored document
//!   carries a pulse, and nothing here calls [`ReceiptPrinter::open_drawer`], so
//!   no retry and no restart can open a drawer a second time.
//! * **The sale is never touched** (I-4). Print state lives on the job and its
//!   attempts; a completed sale does not learn whether its receipt printed.
//!
//! [`Resolver::resolve_unknown`] is the person's half (§3 rule 5,
//! `ref/ipc-contract.md` §3): *did a readable receipt come out?* Yes closes the
//! job. No also writes a **duplicate** — the original document re-rendered with
//! the DUPLICATE watermark, citing the original, with its own job — and every
//! answer is an audit fact. The answer, its audit row, the duplicate and the one
//! envelope that carries them commit together (I-9).
//!
//! Time and ids come from the [`Clock`] and [`IdSource`] the shell hands in, so
//! every instant a test asserts on is one it chose.

use std::collections::BTreeMap;

use pos_db::DbError;
use pos_db::repo::audit::AuditRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::receipt::{
    ArtifactKind, AttemptOutcome, Id, JobState, Lease, PrintAttempt, ReceiptArtifact,
    ReceiptRepository, StoredArtifact,
};
use pos_domain::audit::VERSION as AUDIT_CANONICAL_VERSION;
use pos_domain::{
    AuditIntent, Authorized, Clock, IdSource, ReceiptModel, RegisterId, Timestamp, Watermark, cap,
};
use pos_hardware::escpos::{self, EscPosError};
use pos_hardware::profile::{Fact, PrinterProfile};
use pos_hardware::render::{PaperWidth, RenderError, render_receipt};
use pos_hardware::{FailReason, PrintOutcome, ReceiptPrinter, RenderedReceipt};
use rusqlite::{Connection, Transaction};
use serde_json::{Map, Value, json};
use uuid::Uuid;

/// The envelope shape every commit is written in. `ref/sync-protocol.md` §2
/// freezes it in Phase 1, before the first outbox row exists.
const PROTOCOL_VERSION: i64 = 1;

/// The build that produced a commit's payloads.
const PRODUCER_VERSION: &str = concat!("terminal ", env!("CARGO_PKG_VERSION"));

/// The audit action a person's answer is recorded under.
pub const RESOLVE_UNKNOWN_ACTION: &str = "print.resolve_unknown";

/// Stable codes for why an attempt ended as it did. A device's own message is
/// never recorded: it could carry anything.
pub mod code {
    pub const PAPER_OUT: &str = "paper_out";
    pub const COVER_OPEN: &str = "cover_open";
    pub const OFFLINE: &str = "offline";
    pub const UNREACHABLE: &str = "unreachable";
    /// Some bytes were taken, then the device failed.
    pub const PARTIAL: &str = "partial";
    /// The device went silent, or missed its deadline.
    pub const NO_ANSWER: &str = "no_answer";
    /// A worker held the job past its lease.
    pub const LEASE_EXPIRED: &str = "lease_expired";
    /// The stored bytes no longer match their digest. Never sent.
    pub const CONTENT_HASH_MISMATCH: &str = "content_hash_mismatch";
    /// Rendered for another printer than this register's. Never sent.
    pub const PROFILE_MISMATCH: &str = "profile_mismatch";
    /// A person said a readable receipt came out.
    pub const RECEIPT_CAME_OUT: &str = "receipt_came_out";
    /// A person said no readable receipt came out; a duplicate replaces it.
    pub const NO_READABLE_RECEIPT: &str = "no_readable_receipt";
}

/// Everything the print queue refuses or cannot do.
///
/// No variant carries a receipt's bytes, a payload or a hash: a receipt can
/// carry a buyer's name and tax number (`.claude/rules/security.md`).
#[derive(Debug, thiserror::Error)]
pub enum PrintQueueError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// An instant past the representable range: a lease or a retry time that
    /// cannot be written down.
    #[error("an instant {offset_ms} ms after {from} is outside the representable range")]
    InstantOutOfRange { from: Timestamp, offset_ms: i64 },
    /// A lease or a retry delay that is not strictly positive. A zero retry
    /// delay would make a refused job due again the instant it failed.
    #[error("the {what} must be at least one millisecond, not {ms}")]
    TimingInvalid { what: &'static str, ms: i64 },
    #[error("no print job has this id")]
    JobMissing,
    #[error("the print job's artifact is missing")]
    ArtifactMissing,
    /// Finalize queues an original; anything else has its own path.
    #[error("only an original is queued at finalize, and this artifact is a {kind:?}")]
    NotAnOriginal { kind: ArtifactKind },
    /// A person resolves an `unknown` job and nothing else (§3 rule 5).
    #[error("this print job is {state:?}; a person resolves only an unknown one")]
    NotUnknown { state: JobState },
    /// `print_resolve_unknown` is not escalatable (`ref/ipc-contract.md` §3), so
    /// it takes no approval, and a proof that carries one would leave a handle
    /// unconsumed.
    #[error("print_resolve_unknown takes no approval, and this proof carries one")]
    ApprovalNotTaken,
    /// The duplicate of a fiscal supplement is not defined: rule 5's duplicate
    /// cites an original, and the schema refuses one that cites anything else.
    #[error("a fiscal supplement's print has no duplicate yet")]
    NoDuplicateOfSupplement,
    /// The duplicate would be printed for another device than its original.
    #[error("the original was rendered for {artifact}, and this register's printer is {register}")]
    ProfileMismatch { artifact: String, register: String },
    #[error("printer profile {profile_id} states no paper width this renderer knows")]
    NoPaper { profile_id: String },
    /// The model the caller supplied does not re-render the original byte for
    /// byte, so a duplicate rendered from it would be a different document.
    #[error(
        "this receipt model does not reproduce the original, so a duplicate would differ from it"
    )]
    ModelDiffersFromOriginal,
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    Encode(#[from] EscPosError),
    #[error("a commit payload could not be serialized: {0}")]
    Payload(#[from] serde_json::Error),
}

/// How long a worker holds a job, and how long a refused job waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    lease_ms: i64,
    retry_after_ms: i64,
}

impl Timing {
    /// A lease and a retry delay, both strictly positive.
    ///
    /// The lease must outlast the driver's own deadlines — connect, write and
    /// status — or a slow but healthy print will be resolved as `unknown`.
    ///
    /// # Errors
    ///
    /// [`PrintQueueError::TimingInvalid`] for a value under one millisecond.
    pub fn new(lease_ms: i64, retry_after_ms: i64) -> Result<Self, PrintQueueError> {
        for (what, ms) in [("lease", lease_ms), ("retry delay", retry_after_ms)] {
            if ms < 1 {
                return Err(PrintQueueError::TimingInvalid { what, ms });
            }
        }
        Ok(Self {
            lease_ms,
            retry_after_ms,
        })
    }
}

impl Default for Timing {
    /// A minute's lease, and thirty seconds before a refused job is tried
    /// again. A refused job sends nothing, so retrying it costs one status read.
    fn default() -> Self {
        Self {
            lease_ms: 60_000,
            retry_after_ms: 30_000,
        }
    }
}

/// What one pass did to one job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempted {
    pub job_id: Id,
    pub outcome: AttemptOutcome,
    pub error_code: Option<&'static str>,
}

/// What a drain did: the lapsed leases it resolved as `unknown`, then every
/// job it attempted, in the order it attempted them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drained {
    pub recovered: Vec<Id>,
    pub attempted: Vec<Attempted>,
}

/// The worker that sends queued receipts to this register's printer.
pub struct PrintWorker<'w> {
    printer: &'w dyn ReceiptPrinter,
    profile_id: &'w str,
    clock: &'w dyn Clock,
    ids: &'w dyn IdSource,
    name: &'w str,
    timing: Timing,
}

impl<'w> PrintWorker<'w> {
    /// A worker named `name`, printing on `printer`, which is bound to the
    /// profile `profile_id`.
    #[must_use]
    pub fn new(
        printer: &'w dyn ReceiptPrinter,
        profile_id: &'w str,
        clock: &'w dyn Clock,
        ids: &'w dyn IdSource,
        name: &'w str,
    ) -> Self {
        Self {
            printer,
            profile_id,
            clock,
            ids,
            name,
            timing: Timing::default(),
        }
    }

    #[must_use]
    pub const fn with_timing(mut self, timing: Timing) -> Self {
        self.timing = timing;
        self
    }

    /// Resolve every lease that lapsed before now as `unknown`: a worker
    /// stopped mid-job and may have sent the bytes. Run at startup, and before
    /// every drain.
    ///
    /// # Errors
    ///
    /// [`PrintQueueError`] when the queue cannot be read or written; nothing is
    /// recorded then.
    pub fn recover(&self, conn: &Connection) -> Result<Vec<Id>, PrintQueueError> {
        let now = self.clock.now();
        let lapsed = ReceiptRepository::new(conn).expired_leases(now)?;
        if lapsed.is_empty() {
            return Ok(Vec::new());
        }
        let tx = conn.unchecked_transaction()?;
        let receipts = ReceiptRepository::new(&tx);
        for job in &lapsed {
            receipts.append_attempt(
                &tx,
                &PrintAttempt {
                    id: next_id(self.ids),
                    job_id: job.id,
                    attempt_no: job.attempts + 1,
                    outcome: AttemptOutcome::Unknown,
                    error_code: Some(code::LEASE_EXPIRED.to_owned()),
                    started_at: now,
                    finished_at: Some(now),
                    sent_at: None,
                    retry_at: None,
                },
            )?;
        }
        tx.commit()?;
        Ok(lapsed.into_iter().map(|job| job.id).collect())
    }

    /// Recover lapsed leases, then attempt every job that is due, once each.
    ///
    /// It ends because every attempt takes its job out of the due set until a
    /// later instant: a print or an ambiguous outcome for good, a refusal until
    /// its retry time, which [`Timing`] keeps strictly in the future.
    ///
    /// # Errors
    ///
    /// The first [`PrintQueueError`]; attempts already appended stay appended.
    pub fn drain(&self, conn: &Connection) -> Result<Drained, PrintQueueError> {
        let recovered = self.recover(conn)?;
        let mut attempted = Vec::new();
        while let Some(attempt) = self.run_once(conn)? {
            attempted.push(attempt);
        }
        Ok(Drained {
            recovered,
            attempted,
        })
    }

    /// Lease the next due job, send it, and append what the device did.
    /// `None` when nothing is due.
    ///
    /// # Errors
    ///
    /// [`PrintQueueError`] when the queue cannot be read or written. A job
    /// leased and then not recorded keeps its lease until [`Self::recover`]
    /// resolves it as `unknown`, which is the safe direction: its bytes may
    /// have been sent.
    pub fn run_once(&self, conn: &Connection) -> Result<Option<Attempted>, PrintQueueError> {
        let now = self.clock.now();
        let until = later(now, self.timing.lease_ms)?;
        let tx = conn.unchecked_transaction()?;
        let leased = ReceiptRepository::new(&tx).lease_next_job(
            &tx,
            Lease {
                owner: self.name,
                at: now,
                until,
            },
        )?;
        tx.commit()?;
        let Some(job) = leased else {
            return Ok(None);
        };

        let stored = ReceiptRepository::new(conn)
            .artifact(&job.artifact_id)?
            .ok_or(PrintQueueError::ArtifactMissing)?;
        let started_at = self.clock.now();
        let sent = self.send(&stored);
        let finished_at = self.clock.now();
        let retry_at = match sent.outcome {
            AttemptOutcome::Failed if sent.retry => {
                Some(later(finished_at, self.timing.retry_after_ms)?)
            }
            _ => None,
        };

        let tx = conn.unchecked_transaction()?;
        ReceiptRepository::new(&tx).append_attempt(
            &tx,
            &PrintAttempt {
                id: next_id(self.ids),
                job_id: job.id,
                attempt_no: job.attempts + 1,
                outcome: sent.outcome,
                error_code: sent.error_code.map(str::to_owned),
                started_at,
                finished_at: Some(finished_at),
                sent_at: sent.bytes_left.then_some(started_at),
                retry_at,
            },
        )?;
        tx.commit()?;
        Ok(Some(Attempted {
            job_id: job.id,
            outcome: sent.outcome,
            error_code: sent.error_code,
        }))
    }

    /// Check the stored document, then hand it to the device and classify
    /// what came back.
    fn send(&self, stored: &StoredArtifact) -> Sent {
        let artifact = &stored.artifact;
        if *blake3::hash(&artifact.content_bytes).as_bytes() != stored.content_hash {
            return Sent::withheld(code::CONTENT_HASH_MISMATCH);
        }
        if artifact.printer_profile != self.profile_id {
            return Sent::withheld(code::PROFILE_MISMATCH);
        }
        let document = RenderedReceipt {
            bytes: artifact.content_bytes.clone(),
        };
        match self.printer.print(&document) {
            PrintOutcome::Printed => Sent::delivered(AttemptOutcome::Printed, None),
            PrintOutcome::Failed(reason) => Sent {
                outcome: AttemptOutcome::Failed,
                error_code: Some(refusal_code(reason)),
                bytes_left: false,
                retry: true,
            },
            PrintOutcome::Partial { .. } => {
                Sent::delivered(AttemptOutcome::Partial, Some(code::PARTIAL))
            }
            PrintOutcome::Unknown => {
                Sent::delivered(AttemptOutcome::Unknown, Some(code::NO_ANSWER))
            }
        }
    }
}

/// One send, classified.
struct Sent {
    outcome: AttemptOutcome,
    error_code: Option<&'static str>,
    /// Whether any byte may have reached the device.
    bytes_left: bool,
    /// Whether a refusal is worth trying again. A document that was never
    /// sent because it is wrong will be just as wrong in thirty seconds.
    retry: bool,
}

impl Sent {
    const fn delivered(outcome: AttemptOutcome, error_code: Option<&'static str>) -> Self {
        Self {
            outcome,
            error_code,
            bytes_left: true,
            retry: false,
        }
    }

    const fn withheld(error_code: &'static str) -> Self {
        Self {
            outcome: AttemptOutcome::Failed,
            error_code: Some(error_code),
            bytes_left: false,
            retry: false,
        }
    }
}

const fn refusal_code(reason: FailReason) -> &'static str {
    match reason {
        FailReason::PaperOut => code::PAPER_OUT,
        FailReason::CoverOpen => code::COVER_OPEN,
        FailReason::Offline => code::OFFLINE,
        FailReason::Unreachable => code::UNREACHABLE,
    }
}

/// Write a sale's original receipt and its one initial print job, in the
/// sale's own transaction — the unconditional write every finalize makes (§3
/// rule 3), whether or not a printer is there. `0005` refuses to complete the
/// sale without both. Returns the digest stored beside the bytes, for the
/// sale's commit envelope.
///
/// # Errors
///
/// [`PrintQueueError::NotAnOriginal`] for any other kind of artifact, and
/// [`PrintQueueError::Db`] for anything the schema refuses — a second original
/// for one sale above all.
pub fn queue_original(
    tx: &Transaction<'_>,
    artifact: &ReceiptArtifact,
    job_id: &Id,
    created_at: Timestamp,
) -> Result<[u8; 32], PrintQueueError> {
    if artifact.kind != ArtifactKind::Original {
        return Err(PrintQueueError::NotAnOriginal {
            kind: artifact.kind,
        });
    }
    let receipts = ReceiptRepository::new(tx);
    let content_hash = receipts.append_artifact(tx, artifact)?;
    receipts.queue_initial(tx, job_id, &artifact.id, created_at)?;
    Ok(content_hash)
}

/// A person's answer to *did a readable receipt come out?*
pub enum Answer<'a> {
    /// The paper in the customer's hand is the document.
    CameOut,
    /// Nothing readable came out. A duplicate is rendered from `model` — the
    /// original's own model — on this register's printer profile.
    NoReadableReceipt {
        model: &'a ReceiptModel,
        profile: &'a PrinterProfile,
    },
}

/// The duplicate a "no" answer created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Duplicate {
    pub artifact_id: Id,
    pub job_id: Id,
}

/// What a resolution wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    pub duplicate: Option<Duplicate>,
    pub audit_id: Uuid,
    pub commit_id: Id,
}

/// Records a person's answer for an ambiguous print.
pub struct Resolver<'r> {
    clock: &'r dyn Clock,
    ids: &'r dyn IdSource,
    register: RegisterId,
}

impl<'r> Resolver<'r> {
    #[must_use]
    pub const fn new(clock: &'r dyn Clock, ids: &'r dyn IdSource, register: RegisterId) -> Self {
        Self {
            clock,
            ids,
            register,
        }
    }

    /// Close an `unknown` job with the answer of a person holding
    /// `sale.reprint`, in one transaction: the `cancelled` attempt, the audit
    /// fact, and for a "no", the duplicate artifact and its queued job — with
    /// one commit envelope carrying both facts (I-9).
    ///
    /// The duplicate is the original document plus the DUPLICATE watermark and
    /// nothing else: `model` must re-render the original byte for byte without
    /// the watermark before a duplicate is rendered from it with one, so a
    /// model for another sale, another template or another printer is refused
    /// rather than printed as a copy of something it is not.
    ///
    /// # Errors
    ///
    /// [`PrintQueueError::NotUnknown`] for any job not `unknown`, checked again
    /// inside the transaction so two answers cannot both land;
    /// [`PrintQueueError::ApprovalNotTaken`] for a proof carrying an approval;
    /// the render refusals above; and [`PrintQueueError::Db`] for anything the
    /// schema refuses. Nothing is written on any error.
    pub fn resolve_unknown(
        &self,
        conn: &Connection,
        authorized: &Authorized<cap::SaleReprint>,
        job_id: &Id,
        answer: Answer<'_>,
    ) -> Result<Resolution, PrintQueueError> {
        if authorized.approval().is_some() {
            return Err(PrintQueueError::ApprovalNotTaken);
        }
        let receipts = ReceiptRepository::new(conn);
        let job = receipts.job(job_id)?.ok_or(PrintQueueError::JobMissing)?;
        if job.state != JobState::Unknown {
            return Err(PrintQueueError::NotUnknown { state: job.state });
        }
        // Rendering is pure work, done before the transaction opens.
        let duplicate_bytes = match answer {
            Answer::CameOut => None,
            Answer::NoReadableReceipt { model, profile } => {
                let original = original_of(&receipts, &job.artifact_id)?;
                let bytes = render_duplicate(&original.artifact, model, profile)?;
                Some((original.artifact, bytes))
            }
        };

        let now = self.clock.now();
        let tx = conn.unchecked_transaction()?;
        let receipts = ReceiptRepository::new(&tx);
        let job = receipts.job(job_id)?.ok_or(PrintQueueError::JobMissing)?;
        if job.state != JobState::Unknown {
            return Err(PrintQueueError::NotUnknown { state: job.state });
        }

        let commit_id = next_id(self.ids);
        let audit_id = self.ids.next();
        let mut members: Vec<(&'static str, Id, String)> = Vec::new();
        // The duplicate is described before anything is written: its envelope
        // must exist before the artifact may (0006's
        // `receipt_artifact_has_ready_commit`), and the envelope names it.
        let duplicate_artifact = match duplicate_bytes {
            None => None,
            Some((original, content_bytes)) => {
                let artifact = ReceiptArtifact {
                    id: next_id(self.ids),
                    sale_id: original.sale_id,
                    kind: ArtifactKind::Duplicate,
                    source_artifact_id: Some(original.id),
                    format: original.format.clone(),
                    template_version: original.template_version.clone(),
                    printer_profile: original.printer_profile.clone(),
                    fiscal_version: original.fiscal_version.clone(),
                    content_bytes,
                    generated_at: now,
                };
                let content_hash = *blake3::hash(&artifact.content_bytes).as_bytes();
                members.push((
                    "receipt_artifact",
                    artifact.id,
                    artifact_payload(&artifact, &content_hash)?,
                ));
                Some((artifact, content_hash, next_id(self.ids)))
            }
        };
        let duplicate = duplicate_artifact
            .as_ref()
            .map(|(artifact, _, job_id)| Duplicate {
                artifact_id: artifact.id,
                job_id: *job_id,
            });

        let intent = AuditIntent {
            actor: authorized.actor(),
            approver: None,
            approval: None,
            action: RESOLVE_UNKNOWN_ACTION,
            entity: "print_job",
            entity_id: Uuid::from_bytes(*job_id),
            reason: None,
            payload: json!({
                "duplicate_artifact_id": duplicate.map(|d| uuid_text(&d.artifact_id)),
                "receipt_came_out": duplicate.is_none(),
            }),
            at: now,
        };
        members.push((
            "audit_log",
            *audit_id.as_bytes(),
            audit_payload(audit_id, self.register, &intent)?,
        ));
        let change_ids: Vec<Id> = members.iter().map(|_| next_id(self.ids)).collect();
        let facts: Vec<FactMember<'_>> = members
            .iter()
            .zip(&change_ids)
            .map(|((entity, entity_id, payload), change_id)| FactMember {
                change_id,
                entity,
                entity_id,
                payload,
            })
            .collect();
        let created_at = now.to_iso8601();
        OutboxRepository::new(&tx).write_commit(
            &tx,
            &CommitEnvelope {
                commit_id: &commit_id,
                protocol_version: PROTOCOL_VERSION,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: PRODUCER_VERSION,
                created_at: &created_at,
            },
            &facts,
        )?;
        if let Some((artifact, content_hash, job_id)) = &duplicate_artifact {
            let stored = receipts.append_artifact(&tx, artifact)?;
            if stored != *content_hash {
                return Err(PrintQueueError::Db(DbError::StoredPrintRecordInvalid {
                    reason: "the stored duplicate's digest is not the one its envelope names"
                        .to_owned(),
                }));
            }
            receipts.queue_initial(&tx, job_id, &artifact.id, now)?;
        }
        AuditRepository::new(&tx).append(&tx, self.register, audit_id, &intent)?;
        receipts.append_attempt(
            &tx,
            &PrintAttempt {
                id: next_id(self.ids),
                job_id: *job_id,
                attempt_no: job.attempts + 1,
                outcome: AttemptOutcome::Cancelled,
                error_code: Some(
                    if duplicate.is_none() {
                        code::RECEIPT_CAME_OUT
                    } else {
                        code::NO_READABLE_RECEIPT
                    }
                    .to_owned(),
                ),
                started_at: now,
                finished_at: Some(now),
                sent_at: None,
                retry_at: None,
            },
        )?;
        tx.commit()?;
        Ok(Resolution {
            duplicate,
            audit_id,
            commit_id,
        })
    }
}

/// The original a duplicate of this artifact must cite: itself, or the one a
/// duplicate already cites.
fn original_of(
    receipts: &ReceiptRepository<'_>,
    artifact_id: &Id,
) -> Result<StoredArtifact, PrintQueueError> {
    let printed = receipts
        .artifact(artifact_id)?
        .ok_or(PrintQueueError::ArtifactMissing)?;
    match (printed.artifact.kind, printed.artifact.source_artifact_id) {
        (ArtifactKind::Original, _) => Ok(printed),
        (ArtifactKind::Duplicate, Some(source)) => receipts
            .artifact(&source)?
            .ok_or(PrintQueueError::ArtifactMissing),
        (ArtifactKind::Duplicate, None) => Err(PrintQueueError::ArtifactMissing),
        (ArtifactKind::FiscalSupplement, _) => Err(PrintQueueError::NoDuplicateOfSupplement),
    }
}

/// The original's document with the DUPLICATE watermark, after proving that
/// `model` is the original's document.
fn render_duplicate(
    original: &ReceiptArtifact,
    model: &ReceiptModel,
    profile: &PrinterProfile,
) -> Result<Vec<u8>, PrintQueueError> {
    if profile.profile_id != original.printer_profile {
        return Err(PrintQueueError::ProfileMismatch {
            artifact: original.printer_profile.clone(),
            register: profile.profile_id.clone(),
        });
    }
    let paper = paper_of(profile)?;
    let typeset = |watermark: Option<Watermark>| -> Result<Vec<u8>, PrintQueueError> {
        let mut document = model.clone();
        document.watermark = watermark;
        Ok(escpos::encode(&render_receipt(&document, paper)?, profile)?)
    };
    if typeset(None)? != original.content_bytes {
        return Err(PrintQueueError::ModelDiffersFromOriginal);
    }
    typeset(Some(Watermark::Duplicate))
}

/// The paper a profile prints on, read from the dots it states — one source
/// for the width, so a profile and a paper cannot be handed in disagreeing.
fn paper_of(profile: &PrinterProfile) -> Result<PaperWidth, PrintQueueError> {
    let dots = match &profile.dot_width {
        Fact::Known(dots) => Some(dots.get()),
        _ => None,
    };
    PaperWidth::ALL
        .into_iter()
        .find(|paper| Some(paper.width_px()) == dots)
        .ok_or_else(|| PrintQueueError::NoPaper {
            profile_id: profile.profile_id.clone(),
        })
}

/// A receipt artifact as it travels (`ref/sync-protocol.md` §3, "The payload a
/// `receipt_artifact` travels as"): its own columns in the canonical form —
/// sorted keys, no whitespace, ids as lower-case hyphenated UUIDs, instants as
/// ISO-8601 UTC with milliseconds, bytes as lower-case hex.
fn artifact_payload(
    artifact: &ReceiptArtifact,
    content_hash: &[u8; 32],
) -> Result<String, PrintQueueError> {
    canonical(&json!({
        "artifact_kind": artifact_kind_text(artifact.kind),
        "content_bytes": hex(&artifact.content_bytes),
        "content_hash": hex(content_hash),
        "fiscal_version": artifact.fiscal_version,
        "format": artifact.format,
        "generated_at": artifact.generated_at.to_iso8601(),
        "id": uuid_text(&artifact.id),
        "printer_profile": artifact.printer_profile,
        "sale_id": uuid_text(&artifact.sale_id),
        "source_artifact_id": artifact.source_artifact_id.as_ref().map(uuid_text),
        "template_version": artifact.template_version,
    }))
}

/// An audit row as it travels (`ref/sync-protocol.md` §3, "The payload an
/// `audit_log` row travels as"): every column known before the row is chained.
/// `seq`, `prev_hash` and `hash` are absent because `0004` refuses the row until
/// this envelope exists, so they cannot be known when it is written.
fn audit_payload(
    id: Uuid,
    register: RegisterId,
    intent: &AuditIntent,
) -> Result<String, PrintQueueError> {
    canonical(&json!({
        "action": intent.action,
        "actor_id": intent.actor.as_uuid().hyphenated().to_string(),
        "approval_handle_id": intent.approval.map(|a| a.as_uuid().hyphenated().to_string()),
        "approver_id": intent.approver.map(|a| a.as_uuid().hyphenated().to_string()),
        "at": intent.at.to_iso8601(),
        "canonical_version": AUDIT_CANONICAL_VERSION,
        "entity": intent.entity,
        "entity_id": intent.entity_id.hyphenated().to_string(),
        "id": id.hyphenated().to_string(),
        "payload": intent.payload,
        "reason": intent.reason,
        "register_id": register.as_uuid().hyphenated().to_string(),
    }))
}

/// Serialize with every object's keys sorted, at every depth, whatever order
/// `serde_json` keeps them in — no whitespace, UTF-8.
fn canonical(value: &Value) -> Result<String, PrintQueueError> {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(object) => {
                let ordered: BTreeMap<&String, Value> = object
                    .iter()
                    .map(|(key, item)| (key, sorted(item)))
                    .collect();
                let mut out = Map::new();
                for (key, item) in ordered {
                    out.insert(key.clone(), item);
                }
                Value::Object(out)
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    Ok(serde_json::to_string(&sorted(value))?)
}

const fn artifact_kind_text(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Original => "original",
        ArtifactKind::Duplicate => "duplicate",
        ArtifactKind::FiscalSupplement => "fiscal_supplement",
    }
}

fn uuid_text(id: &Id) -> String {
    Uuid::from_bytes(*id).hyphenated().to_string()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

fn next_id(ids: &dyn IdSource) -> Id {
    *ids.next().as_bytes()
}

/// `from` plus `offset_ms`, or a named refusal past the representable range.
fn later(from: Timestamp, offset_ms: i64) -> Result<Timestamp, PrintQueueError> {
    from.epoch_milliseconds()
        .checked_add(offset_ms)
        .and_then(|ms| Timestamp::from_epoch_milliseconds(ms).ok())
        .ok_or(PrintQueueError::InstantOutOfRange { from, offset_ms })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::cell::Cell;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use pos_db::fixtures::registered_chain::{self, Checkout, RegisteredChain};
    use pos_db::repo::audit::AuditRepository;
    use pos_db::repo::receipt::PrintJob;
    use pos_domain::catalog::UnitOfMeasure;
    use pos_domain::receipt::{
        DocKind, FiscalBlock, FooterBlock, Language, MerchantBlock, ReceiptHeader, ReceiptLine,
        ReceiptLocale, ReceiptTender, ReceiptTotals,
    };
    use pos_domain::tax::{TaxSummaryRow, TaxTreatment};
    use pos_domain::{
        ApprovalBinding, ApprovalHandle, ApprovalId, BusinessDate, Currency, EscalationPolicy,
        GrantSet, Money, Percent, Qty, Role, SaleId, SeqIdSource, UserId, authorize,
    };
    use pos_hardware::{Fault, PrinterStatus, SimulatedPrinter};
    use rusqlite::params;
    use tempfile::TempDir;

    use super::*;

    const KEY: &str = "print-queue-test-key";
    const WORKER: &str = "print-worker-1";
    /// The profile this register's printer is bound to, and every original
    /// here was rendered for.
    const PROFILE: &str = "simulator-80mm";
    const REGISTER: [u8; 16] = [0x0A; 16];

    /// A clock a test moves by hand. The worker reads it; nothing else does.
    struct TestClock(Cell<Timestamp>);

    impl TestClock {
        fn new() -> Self {
            Self(Cell::new(at(0)))
        }

        fn set(&self, seconds: i64) {
            self.0.set(at(seconds));
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> Timestamp {
            self.0.get()
        }
    }

    /// `seconds` after the fixture's own instant.
    fn at(seconds: i64) -> Timestamp {
        let base = Timestamp::parse_iso8601(registered_chain::AT).unwrap();
        Timestamp::from_epoch_milliseconds(base.epoch_milliseconds() + seconds * 1_000).unwrap()
    }

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, Currency::JOD)
    }

    /// One sale's receipt: a line, its tax, a cash tender and a cleared fiscal
    /// block, so a duplicate carries the original's fiscal identity. The
    /// block prints its UUID today; the QR raster arrives at 2.7.9.
    fn model() -> ReceiptModel {
        ReceiptModel {
            doc_kind: DocKind::Sale,
            watermark: None,
            merchant: MerchantBlock {
                legal_name: "مؤسسة النور التجارية".to_owned(),
                store_name: "النور".to_owned(),
                address: None,
                phone: None,
                tin: Some("123456789".to_owned()),
            },
            buyer: None,
            header: ReceiptHeader {
                sale_id: SaleId::from_uuid(Uuid::from_u128(0x10)),
                receipt_no: "REG01-000123".to_owned(),
                register_id: RegisterId::from_uuid(Uuid::from_bytes(REGISTER)),
                register_code: "REG01".to_owned(),
                cashier_name: "ليلى".to_owned(),
                issued_at: at(0),
                business_date: BusinessDate::new(2026, 8, 25).unwrap(),
            },
            lines: vec![ReceiptLine {
                name: "قهوة".to_owned(),
                qty: Qty::from_milli(1_000),
                unit: UnitOfMeasure::Each,
                is_weighed: false,
                unit_price: jod(2_500),
                line_total: jod(2_900),
                discounts: Vec::new(),
            }],
            totals: ReceiptTotals {
                subtotal: jod(2_900),
                discount_total: jod(0),
                tax_summary: vec![TaxSummaryRow {
                    code: "GST16".to_owned(),
                    treatment: TaxTreatment::Standard,
                    rate: Percent::from_ppm(160_000),
                    per_unit: None,
                    reason: None,
                    net: jod(2_500),
                    tax: jod(400),
                    gross: jod(2_900),
                }],
                rounding_adjustment: None,
                total: jod(2_900),
            },
            tenders: vec![ReceiptTender {
                label: "نقدًا".to_owned(),
                amount: jod(2_900),
                masked_pan: None,
                scheme: None,
            }],
            change: Some(jod(0)),
            loyalty: None,
            fiscal: Some(FiscalBlock {
                uuid: "9f1c0f7a-0000-4000-8000-000000000000".to_owned(),
                qr_payload: "AQIDBAU=".to_owned(),
            }),
            footer: FooterBlock {
                return_policy: None,
                thank_you: Some("شكرًا لتسوقكم".to_owned()),
            },
            locale: ReceiptLocale::new(Language::Arabic),
        }
    }

    fn profile() -> PrinterProfile {
        PrinterProfile::simulator(PaperWidth::Mm80)
    }

    /// The original's bytes, through the shipped pipeline, rendered once.
    fn original_bytes() -> &'static [u8] {
        static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
        BYTES.get_or_init(|| {
            escpos::encode(
                &render_receipt(&model(), PaperWidth::Mm80).unwrap(),
                &profile(),
            )
            .unwrap()
        })
    }

    fn id(value: u128) -> Vec<u8> {
        Uuid::from_u128(value).as_bytes().to_vec()
    }

    fn id16(bytes: &[u8]) -> Id {
        Id::try_from(bytes).unwrap()
    }

    /// A register database with a store a sale can complete in, on disk so a
    /// test can close it and open it again.
    struct World {
        _dir: TempDir,
        path: PathBuf,
        conn: Connection,
        chain: RegisteredChain,
    }

    impl World {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("register.db");
            let conn = pos_db::open(&path, KEY).unwrap();
            let chain = RegisteredChain::seed(&conn);
            chain.add_register(&conn, &REGISTER, "REG01");
            Self {
                _dir: dir,
                path,
                conn,
                chain,
            }
        }

        fn reopen(&mut self) {
            let fresh = pos_db::open(&self.path, KEY).unwrap();
            let old = std::mem::replace(&mut self.conn, fresh);
            old.close().map_err(|(_, error)| error).unwrap();
        }

        /// A completed sale whose original receipt and initial job are written
        /// by [`queue_original`], in the transaction that writes them for a
        /// finalize.
        fn completed_sale(&self, slot: u8, printer_profile: &str) -> Sale {
            let checkout = self.parked_sale(slot);
            checkout.attach(&self.conn, &self.chain);
            self.chain
                .write_envelope(&self.conn, &checkout.commit, &checkout.members());
            checkout.write_line_taxes(&self.conn);
            let tx = self.conn.unchecked_transaction().unwrap();
            queue_original(
                &tx,
                &ReceiptArtifact {
                    id: id16(&checkout.artifact),
                    sale_id: id16(&checkout.sale),
                    kind: ArtifactKind::Original,
                    source_artifact_id: None,
                    format: "escpos".to_owned(),
                    template_version: "receipt-v1".to_owned(),
                    printer_profile: printer_profile.to_owned(),
                    fiscal_version: None,
                    content_bytes: original_bytes().to_vec(),
                    generated_at: at(0),
                },
                &id16(&checkout.print_job),
                at(0),
            )
            .unwrap();
            tx.commit().unwrap();
            checkout.write_facts_after_envelope(&self.conn, &self.chain);
            checkout.complete(&self.conn);
            Sale::of(checkout)
        }

        /// A sale, a line and a cash tender, parked: everything the caller
        /// owns before the fixture attaches the rest.
        fn parked_sale(&self, slot: u8) -> Checkout {
            let base = u128::from(slot) << 8;
            let (sale, line, tender, product) =
                (id(base), id(base + 1), id(base + 2), id(base + 3));
            self.conn
                .execute(
                    "INSERT INTO product (id, sku, name, price_minor, currency, is_active,
                                          tax_category_id)
                     VALUES (?1, ?2, 'Coffee', 2500, 'JOD', 1, ?3)",
                    params![&product, format!("SKU-{slot}"), &self.chain.tax_category],
                )
                .unwrap();
            self.conn
                .execute(
                    "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                                       tax_minor, total_minor, currency, business_date,
                                       completed_at)
                     VALUES (?1, ?2, ?3, 'parked', 2500, 400, 2900, 'JOD', '2026-08-25', ?4)",
                    params![
                        &sale,
                        format!("REG01-{slot:06}"),
                        &REGISTER,
                        registered_chain::AT
                    ],
                )
                .unwrap();
            self.conn
                .execute(
                    "INSERT INTO sale_line (id, sale_id, product_id, qty_milli, unit_price_minor,
                                            tax_minor, total_minor, line_no, name_snapshot,
                                            net_minor, tax_category_id)
                     VALUES (?1, ?2, ?3, 1000, 2500, 400, 2900, 1, 'Coffee', 2500, ?4)",
                    params![&line, &sale, &product, &self.chain.tax_category],
                )
                .unwrap();
            self.conn
                .execute(
                    "INSERT INTO sale_tender (id, sale_id, method, amount_minor, change_minor)
                     VALUES (?1, ?2, 'cash', 2900, 0)",
                    params![&tender, &sale],
                )
                .unwrap();
            Checkout::new(slot, &sale, &REGISTER, &[&line], &[&tender])
        }

        fn job(&self, job: &Id) -> PrintJob {
            ReceiptRepository::new(&self.conn)
                .job(job)
                .unwrap()
                .unwrap()
        }

        fn attempts(&self, job: &Id) -> Vec<PrintAttempt> {
            ReceiptRepository::new(&self.conn).attempts(job).unwrap()
        }

        fn artifact(&self, artifact: &Id) -> StoredArtifact {
            ReceiptRepository::new(&self.conn)
                .artifact(artifact)
                .unwrap()
                .unwrap()
        }

        /// Every column of the sale's row, as SQLite stores it.
        fn sale_row(&self, sale: &[u8]) -> Vec<rusqlite::types::Value> {
            let mut statement = self
                .conn
                .prepare("SELECT * FROM sale WHERE id = ?1")
                .unwrap();
            let width = statement.column_count();
            statement
                .query_row([sale], |row| (0..width).map(|i| row.get(i)).collect())
                .unwrap()
        }

        fn sale_status(&self, sale: &[u8]) -> String {
            self.conn
                .query_row("SELECT status FROM sale WHERE id = ?1", [sale], |row| {
                    row.get(0)
                })
                .unwrap()
        }

        fn worker<'w>(
            &self,
            printer: &'w SimulatedPrinter,
            clock: &'w TestClock,
            ids: &'w SeqIdSource,
        ) -> PrintWorker<'w> {
            PrintWorker::new(printer, PROFILE, clock, ids, WORKER)
        }

        fn cashier(&self) -> UserId {
            UserId::from_uuid(Uuid::from_slice(&self.chain.cashier).unwrap())
        }
    }

    /// One completed sale's ids.
    struct Sale {
        sale: Vec<u8>,
        artifact: Id,
        job: Id,
    }

    impl Sale {
        fn of(checkout: Checkout) -> Self {
            Self {
                artifact: id16(&checkout.artifact),
                job: id16(&checkout.print_job),
                sale: checkout.sale,
            }
        }
    }

    fn ids() -> SeqIdSource {
        SeqIdSource::new(1_787_000_000_000, 7)
    }

    /// A cashier's proof of `sale.reprint`, as the command handler will
    /// produce it: no approval, because the command takes none.
    fn reprint_proof(actor: UserId, now: Timestamp) -> Authorized<cap::SaleReprint> {
        authorize::<cap::SaleReprint>(
            actor,
            &GrantSet::of_role(Role::Cashier),
            None,
            &ApprovalBinding {
                entity_id: Uuid::nil(),
                amount_minor: 0,
                content_hash: None,
            },
            &EscalationPolicy::empty(),
            now,
        )
        .unwrap()
    }

    fn resolver<'r>(clock: &'r TestClock, ids: &'r SeqIdSource) -> Resolver<'r> {
        Resolver::new(
            clock,
            ids,
            RegisterId::from_uuid(Uuid::from_bytes(REGISTER)),
        )
    }

    fn manifest(world: &World, commit: &Id) -> Vec<pos_db::repo::outbox::ManifestEntry> {
        OutboxRepository::new(&world.conn).manifest(commit).unwrap()
    }

    fn unhex(text: &str) -> Vec<u8> {
        text.as_bytes()
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    /// The one item of a list that must hold exactly one. Patterns rather than
    /// `[0]`: `clippy::indexing_slicing` is denied in tests too.
    fn only<T: std::fmt::Debug>(items: &[T]) -> &T {
        match items {
            [item] => item,
            other => panic!("expected exactly one, found {other:?}"),
        }
    }

    fn field<'v>(value: &'v Value, key: &str) -> &'v Value {
        value
            .get(key)
            .unwrap_or_else(|| panic!("no {key:?} in the payload"))
    }

    // ── the six named tests ────────────────────────────────────────────────

    /// Finalize writes the original and its one job unconditionally, and the
    /// schema will not complete a sale without them: every completed sale has
    /// exactly one initial job, queued and never attempted, and nothing else
    /// may be queued in its place.
    #[test]
    fn every_finalized_sale_has_one_initial_print_job() {
        let world = World::new();
        let sales = [
            world.completed_sale(1, PROFILE),
            world.completed_sale(2, PROFILE),
        ];
        for sale in &sales {
            assert_eq!(world.sale_status(&sale.sale), "completed");
            let jobs: i64 = world
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM print_job j JOIN receipt_artifact a ON a.id = j.artifact_id
                      WHERE a.sale_id = ?1",
                    [&sale.sale],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(jobs, 1);
            let job = world.job(&sale.job);
            assert_eq!((job.state, job.attempts), (JobState::Queued, 0));
            assert_eq!(job.artifact_id, sale.artifact);
        }

        // Nothing may be queued in the original's place: not a second
        // original, and not another kind of artifact at finalize.
        let tx = world.conn.unchecked_transaction().unwrap();
        let mut second = world.artifact(&sales[0].artifact).artifact;
        second.id = [0x51; 16];
        assert!(matches!(
            queue_original(&tx, &second, &[0x52; 16], at(1)),
            Err(PrintQueueError::Db(_))
        ));
        second.kind = ArtifactKind::Duplicate;
        second.source_artifact_id = Some(sales[0].artifact);
        assert!(matches!(
            queue_original(&tx, &second, &[0x52; 16], at(1)),
            Err(PrintQueueError::NotAnOriginal {
                kind: ArtifactKind::Duplicate
            })
        ));
        drop(tx);

        // And a sale whose original has no job cannot complete at all.
        let checkout = world.parked_sale(3);
        checkout.attach(&world.conn, &world.chain);
        world
            .chain
            .write_envelope(&world.conn, &checkout.commit, &checkout.members());
        checkout.write_line_taxes(&world.conn);
        let tx = world.conn.unchecked_transaction().unwrap();
        let mut orphan = world.artifact(&sales[0].artifact).artifact;
        orphan.id = id16(&checkout.artifact);
        orphan.sale_id = id16(&checkout.sale);
        ReceiptRepository::new(&tx)
            .append_artifact(&tx, &orphan)
            .unwrap();
        tx.commit().unwrap();
        checkout.write_facts_after_envelope(&world.conn, &world.chain);
        assert!(checkout.try_complete(&world.conn).is_err());
        assert_eq!(world.sale_status(&checkout.sale), "parked");

        // The two initial jobs are exactly what the worker finds due, in the
        // order they were queued.
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        let drained = world
            .worker(&printer, &clock, &ids)
            .drain(&world.conn)
            .unwrap();
        let printed: Vec<Id> = drained.attempted.iter().map(|a| a.job_id).collect();
        assert_eq!(printed, vec![sales[0].job, sales[1].job]);
    }

    /// The device refuses, tears the paper, or goes silent: the money moved and
    /// the sale stands, every column of it as it completed (§3 rule 2, I-4).
    #[test]
    fn print_failure_after_finalize_leaves_sale_complete() {
        let world = World::new();
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        let worker = world.worker(&printer, &clock, &ids);
        let refused = world.completed_sale(1, PROFILE);
        let torn = world.completed_sale(2, PROFILE);
        let silent = world.completed_sale(3, PROFILE);
        let before: Vec<_> = [&refused, &torn, &silent]
            .iter()
            .map(|sale| world.sale_row(&sale.sale))
            .collect();

        // Paper out before a byte: a failure, retried in thirty seconds.
        *printer.force_status.lock().unwrap() = PrinterStatus::PaperOut;
        let attempt = worker.run_once(&world.conn).unwrap().unwrap();
        assert_eq!(
            (attempt.job_id, attempt.outcome, attempt.error_code),
            (refused.job, AttemptOutcome::Failed, Some(code::PAPER_OUT))
        );
        let attempts = world.attempts(&refused.job);
        let failure = only(&attempts);
        assert_eq!(failure.retry_at, Some(at(30)));
        assert_eq!(failure.sent_at, None, "nothing left for the device");
        assert_eq!(world.job(&refused.job).next_attempt_at, Some(at(30)));

        // The paper runs out mid-document, then the device goes silent.
        *printer.force_status.lock().unwrap() = PrinterStatus::Ready;
        printer.script(Fault::PaperOutAfter(64));
        let attempt = worker.run_once(&world.conn).unwrap().unwrap();
        assert_eq!(
            (attempt.job_id, attempt.outcome),
            (torn.job, AttemptOutcome::Partial)
        );
        *printer.force_status.lock().unwrap() = PrinterStatus::Ready;
        printer.script(Fault::SilentAfter(64));
        let attempt = worker.run_once(&world.conn).unwrap().unwrap();
        assert_eq!(
            (attempt.job_id, attempt.outcome),
            (silent.job, AttemptOutcome::Unknown)
        );

        for (sale, row) in [&refused, &torn, &silent].iter().zip(&before) {
            assert_eq!(world.sale_status(&sale.sale), "completed");
            assert_eq!(&world.sale_row(&sale.sale), row, "the sale changed");
        }
        assert!(printer.printed.lock().unwrap().is_empty());
    }

    /// No answer came back, so paper may exist: the duplicate is the
    /// original's document plus the DUPLICATE watermark, cites the original,
    /// gets its own job, travels in its own envelope, and the answer that
    /// created it is an audit fact.
    #[test]
    fn duplicate_artifact_links_the_original_and_adds_the_duplicate_watermark() {
        let world = World::new();
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        let sale = world.completed_sale(1, PROFILE);
        printer.script(Fault::SilentAfter(64));
        world
            .worker(&printer, &clock, &ids)
            .drain(&world.conn)
            .unwrap();
        assert_eq!(world.job(&sale.job).state, JobState::Unknown);

        clock.set(90);
        let (model, profile) = (model(), profile());
        let resolution = resolver(&clock, &ids)
            .resolve_unknown(
                &world.conn,
                &reprint_proof(world.cashier(), at(90)),
                &sale.job,
                Answer::NoReadableReceipt {
                    model: &model,
                    profile: &profile,
                },
            )
            .unwrap();
        let duplicate = resolution.duplicate.expect("a no creates a duplicate");

        // It cites this sale's original and was set for the same device.
        let stored = world.artifact(&duplicate.artifact_id);
        let original = world.artifact(&sale.artifact).artifact;
        assert_eq!(stored.artifact.kind, ArtifactKind::Duplicate);
        assert_eq!(stored.artifact.source_artifact_id, Some(sale.artifact));
        assert_eq!(stored.artifact.sale_id, original.sale_id);
        assert_eq!(stored.artifact.printer_profile, original.printer_profile);
        assert_eq!(stored.artifact.generated_at, at(90));

        // Its bytes are the original's document with the watermark, and only
        // that: the same model, the same pipeline, `Watermark::Duplicate`.
        let mut watermarked = model.clone();
        watermarked.watermark = Some(Watermark::Duplicate);
        let expected = escpos::encode(
            &render_receipt(&watermarked, PaperWidth::Mm80).unwrap(),
            &profile,
        )
        .unwrap();
        assert_eq!(stored.artifact.content_bytes, expected);
        assert_ne!(stored.artifact.content_bytes, original.content_bytes);
        assert_eq!(
            stored.content_hash,
            *blake3::hash(&stored.artifact.content_bytes).as_bytes()
        );

        // The original job is closed by the answer; the duplicate is queued.
        let closed = world.job(&sale.job);
        assert_eq!((closed.state, closed.attempts), (JobState::Cancelled, 2));
        assert_eq!(
            closed.last_error.as_deref(),
            Some(code::NO_READABLE_RECEIPT)
        );
        let queued = world.job(&duplicate.job_id);
        assert_eq!((queued.state, queued.attempts), (JobState::Queued, 0));
        assert_eq!(queued.artifact_id, duplicate.artifact_id);

        // One envelope carries the duplicate and the audit fact, and the
        // duplicate travels as ref/sync-protocol.md §3 defines it.
        let members = manifest(&world, &resolution.commit_id);
        let entities: Vec<&str> = members.iter().map(|m| m.entity.as_str()).collect();
        assert_eq!(entities, ["receipt_artifact", "audit_log"]);
        let artifact_member = members.first().unwrap();
        assert_eq!(artifact_member.entity_id, duplicate.artifact_id);
        let payload: Value = serde_json::from_str(&artifact_member.payload).unwrap();
        assert_eq!(field(&payload, "artifact_kind"), "duplicate");
        assert_eq!(
            field(&payload, "source_artifact_id"),
            &Value::from(uuid_text(&sale.artifact))
        );
        assert_eq!(
            unhex(field(&payload, "content_bytes").as_str().unwrap()),
            expected
        );
        assert_eq!(
            field(&payload, "content_hash"),
            &Value::from(hex(&stored.content_hash))
        );
        assert_eq!(
            canonical(&payload).unwrap(),
            artifact_member.payload,
            "not canonical"
        );

        // The answer is the audit fact: who, which job, what they said.
        let chain = AuditRepository::new(&world.conn)
            .chain(RegisterId::from_uuid(Uuid::from_bytes(REGISTER)))
            .unwrap();
        let entry = chain.entries().last().unwrap();
        assert_eq!(entry.id, resolution.audit_id);
        assert_eq!(entry.intent.action, RESOLVE_UNKNOWN_ACTION);
        assert_eq!(entry.intent.entity, "print_job");
        assert_eq!(entry.intent.entity_id, Uuid::from_bytes(sale.job));
        assert_eq!(entry.intent.actor, world.cashier());
        assert_eq!(
            entry.intent.payload,
            json!({
                "duplicate_artifact_id": uuid_text(&duplicate.artifact_id),
                "receipt_came_out": false,
            })
        );

        // The worker prints the duplicate, and nothing opens the drawer.
        let drained = world
            .worker(&printer, &clock, &ids)
            .drain(&world.conn)
            .unwrap();
        assert_eq!(only(&drained.attempted).job_id, duplicate.job_id);
        let printed = printer.printed.lock().unwrap();
        assert_eq!(only(&printed).bytes, expected);
        assert_eq!(*printer.drawer_opens.lock().unwrap(), 0);
    }

    /// A silent device and a torn page may both be paper. Neither is ever sent
    /// again by the worker, however long it runs — and nothing it does opens
    /// the drawer, so no retry can open it a second time (§3 rules 4 and 6).
    #[test]
    fn an_unknown_print_outcome_never_auto_retries_the_drawer_pulse() {
        let world = World::new();
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        let worker = world.worker(&printer, &clock, &ids);
        let silent = world.completed_sale(1, PROFILE);
        let torn = world.completed_sale(2, PROFILE);
        printer.script(Fault::SilentAfter(64));
        assert_eq!(
            worker.run_once(&world.conn).unwrap().unwrap().outcome,
            AttemptOutcome::Unknown
        );
        printer.script(Fault::PaperOutAfter(64));
        assert_eq!(
            worker.run_once(&world.conn).unwrap().unwrap().outcome,
            AttemptOutcome::Partial
        );
        let torn_pages = printer.torn.lock().unwrap().len();

        // The printer is healthy again, and a day of passes goes by.
        *printer.force_status.lock().unwrap() = PrinterStatus::Ready;
        for hour in 1..=24 {
            clock.set(hour * 3_600);
            let drained = worker.drain(&world.conn).unwrap();
            assert_eq!(drained, Drained::default(), "hour {hour}");
        }

        assert!(printer.printed.lock().unwrap().is_empty());
        assert_eq!(printer.torn.lock().unwrap().len(), torn_pages);
        assert_eq!(*printer.drawer_opens.lock().unwrap(), 0);
        for sale in [&silent, &torn] {
            let job = world.job(&sale.job);
            assert_eq!((job.state, job.attempts), (JobState::Unknown, 1));
        }
        let waiting: Vec<Id> = ReceiptRepository::new(&world.conn)
            .awaiting_an_answer()
            .unwrap()
            .into_iter()
            .map(|job| job.id)
            .collect();
        assert_eq!(waiting, vec![silent.job, torn.job]);
    }

    /// The queue is the database: a restart loses nothing and replays nothing.
    /// A refused job is retried when due, a lapsed lease is resolved as
    /// unknown rather than re-sent, and the bytes, job states and attempt
    /// history read back exactly as they were written.
    #[test]
    fn queue_survives_restart() {
        let mut world = World::new();
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        let refused = world.completed_sale(1, PROFILE);
        let silent = world.completed_sale(2, PROFILE);
        let held = world.completed_sale(3, PROFILE);
        let waiting = world.completed_sale(4, PROFILE);
        {
            let worker = world.worker(&printer, &clock, &ids);
            *printer.force_status.lock().unwrap() = PrinterStatus::PaperOut;
            worker.run_once(&world.conn).unwrap().unwrap();
            *printer.force_status.lock().unwrap() = PrinterStatus::Ready;
            printer.script(Fault::SilentAfter(64));
            worker.run_once(&world.conn).unwrap().unwrap();
        }
        // A worker takes the third job and dies before it can say what
        // happened.
        let tx = world.conn.unchecked_transaction().unwrap();
        let leased = ReceiptRepository::new(&tx)
            .lease_next_job(
                &tx,
                Lease {
                    owner: "a-worker-that-died",
                    at: at(5),
                    until: at(65),
                },
            )
            .unwrap()
            .unwrap();
        tx.commit().unwrap();
        assert_eq!(leased.id, held.job);

        let sales = [&refused, &silent, &held, &waiting];
        let snapshot = |world: &World| -> Vec<(PrintJob, Vec<PrintAttempt>, StoredArtifact)> {
            sales
                .iter()
                .map(|sale| {
                    (
                        world.job(&sale.job),
                        world.attempts(&sale.job),
                        world.artifact(&sale.artifact),
                    )
                })
                .collect()
        };
        let before = snapshot(&world);
        world.reopen();
        assert_eq!(snapshot(&world), before, "the restart changed the queue");
        for (_, _, artifact) in &before {
            assert_eq!(artifact.artifact.content_bytes, original_bytes());
        }

        // The register comes back a minute and six seconds later.
        clock.set(66);
        let drained = world
            .worker(&printer, &clock, &ids)
            .drain(&world.conn)
            .unwrap();
        assert_eq!(drained.recovered, vec![held.job]);
        let attempted: Vec<(Id, AttemptOutcome)> = drained
            .attempted
            .iter()
            .map(|a| (a.job_id, a.outcome))
            .collect();
        assert_eq!(
            attempted,
            vec![
                (refused.job, AttemptOutcome::Printed),
                (waiting.job, AttemptOutcome::Printed),
            ]
        );
        let attempts = world.attempts(&held.job);
        let lapsed = only(&attempts);
        assert_eq!(lapsed.outcome, AttemptOutcome::Unknown);
        assert_eq!(lapsed.error_code.as_deref(), Some(code::LEASE_EXPIRED));
        assert_eq!(
            world.job(&silent.job).attempts,
            1,
            "an unknown is never re-sent"
        );

        // History is appended to, never rewritten.
        for (sale, (_, attempts, artifact)) in sales.iter().zip(&before) {
            let now: Vec<PrintAttempt> = world
                .attempts(&sale.job)
                .into_iter()
                .take(attempts.len())
                .collect();
            assert_eq!(&now, attempts);
            assert_eq!(&world.artifact(&sale.artifact), artifact);
        }
        assert_eq!(printer.printed.lock().unwrap().len(), 2);
    }

    /// A print that works appends one attempt and changes nothing else: the job
    /// is printed and the sale is every column what it was (I-4).
    #[test]
    fn successful_print_appends_an_attempt_without_updating_sale() {
        let world = World::new();
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        let sale = world.completed_sale(1, PROFILE);
        let before = world.sale_row(&sale.sale);

        clock.set(2);
        let drained = world
            .worker(&printer, &clock, &ids)
            .drain(&world.conn)
            .unwrap();
        assert_eq!(
            drained.attempted,
            vec![Attempted {
                job_id: sale.job,
                outcome: AttemptOutcome::Printed,
                error_code: None,
            }]
        );
        let attempts = world.attempts(&sale.job);
        let printed = only(&attempts);
        assert_eq!(printed.attempt_no, 1);
        assert_eq!(printed.outcome, AttemptOutcome::Printed);
        assert_eq!(
            (printed.sent_at, printed.finished_at),
            (Some(at(2)), Some(at(2)))
        );
        assert_eq!(printed.retry_at, None);
        assert_eq!(world.job(&sale.job).state, JobState::Printed);

        assert_eq!(world.sale_row(&sale.sale), before);
        assert_eq!(world.sale_status(&sale.sale), "completed");
        let paper = printer.printed.lock().unwrap();
        assert_eq!(only(&paper).bytes, original_bytes());
        assert_eq!(*printer.drawer_opens.lock().unwrap(), 0);
        assert!(
            ReceiptRepository::new(&world.conn)
                .unprinted()
                .unwrap()
                .is_empty()
        );
    }

    // ── what the worker will not send ──────────────────────────────────────

    /// A stored document whose bytes no longer match their digest is never
    /// sent: it is recorded as a failure with no retry and waits for a person.
    #[test]
    fn a_document_whose_bytes_do_not_match_their_digest_is_never_sent() {
        let world = World::new();
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        // The fixture's own receipt stores a digest that is not its bytes'.
        let checkout = world.parked_sale(1);
        checkout.seal(&world.conn, &world.chain);
        let job = id16(&checkout.print_job);

        let worker = world.worker(&printer, &clock, &ids);
        let attempt = worker.run_once(&world.conn).unwrap().unwrap();
        assert_eq!(
            (attempt.outcome, attempt.error_code),
            (AttemptOutcome::Failed, Some(code::CONTENT_HASH_MISMATCH))
        );
        assert_eq!(only(&world.attempts(&job)).retry_at, None);
        clock.set(86_400);
        assert_eq!(worker.drain(&world.conn).unwrap(), Drained::default());
        assert!(printer.printed.lock().unwrap().is_empty());
        assert_eq!(world.job(&job).state, JobState::Failed);
    }

    /// A document rendered for another printer would print at the wrong width
    /// (§3 rule 7), so it is never sent either.
    #[test]
    fn a_document_rendered_for_another_printer_is_never_sent() {
        let world = World::new();
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        let sale = world.completed_sale(1, "simulator-58mm");
        let attempt = world
            .worker(&printer, &clock, &ids)
            .run_once(&world.conn)
            .unwrap()
            .unwrap();
        assert_eq!(
            (attempt.job_id, attempt.outcome, attempt.error_code),
            (
                sale.job,
                AttemptOutcome::Failed,
                Some(code::PROFILE_MISMATCH)
            )
        );
        assert_eq!(only(&world.attempts(&sale.job)).retry_at, None);
        assert!(printer.printed.lock().unwrap().is_empty());
    }

    // ── what a person's answer may and may not do ──────────────────────────

    fn unknown_sale(
        world: &World,
        printer: &SimulatedPrinter,
        clock: &TestClock,
        ids: &SeqIdSource,
    ) -> Sale {
        let sale = world.completed_sale(1, PROFILE);
        printer.script(Fault::SilentAfter(64));
        world
            .worker(printer, clock, ids)
            .drain(&world.conn)
            .unwrap();
        assert_eq!(world.job(&sale.job).state, JobState::Unknown);
        sale
    }

    fn artifact_count(world: &World) -> i64 {
        world
            .conn
            .query_row("SELECT COUNT(*) FROM receipt_artifact", [], |row| {
                row.get(0)
            })
            .unwrap()
    }

    /// Yes closes the job, writes no artifact, and is an audit fact in an
    /// envelope of its own.
    #[test]
    fn a_receipt_that_came_out_closes_the_job_without_a_duplicate() {
        let world = World::new();
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        let sale = unknown_sale(&world, &printer, &clock, &ids);
        let artifacts = artifact_count(&world);

        let resolution = resolver(&clock, &ids)
            .resolve_unknown(
                &world.conn,
                &reprint_proof(world.cashier(), at(0)),
                &sale.job,
                Answer::CameOut,
            )
            .unwrap();
        assert_eq!(resolution.duplicate, None);
        assert_eq!(artifact_count(&world), artifacts);
        let job = world.job(&sale.job);
        assert_eq!(job.state, JobState::Cancelled);
        assert_eq!(job.last_error.as_deref(), Some(code::RECEIPT_CAME_OUT));
        let members = manifest(&world, &resolution.commit_id);
        let member = only(&members);
        assert_eq!(member.entity, "audit_log");
        let payload: Value = serde_json::from_str(&member.payload).unwrap();
        assert_eq!(field(&payload, "action"), RESOLVE_UNKNOWN_ACTION);
        assert_eq!(
            field(field(&payload, "payload"), "receipt_came_out"),
            &Value::Bool(true)
        );
        assert_eq!(
            field(&payload, "id"),
            &Value::from(resolution.audit_id.hyphenated().to_string())
        );
        assert_eq!(
            canonical(&payload).unwrap(),
            member.payload,
            "not canonical"
        );
    }

    /// Only an unknown job is a person's to close, and it is closed once.
    #[test]
    fn only_an_unknown_job_is_resolved_and_only_once() {
        let world = World::new();
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        let sale = unknown_sale(&world, &printer, &clock, &ids);
        let queued = world.completed_sale(2, PROFILE);
        let proof = reprint_proof(world.cashier(), at(0));
        let resolver = resolver(&clock, &ids);
        assert!(matches!(
            resolver.resolve_unknown(&world.conn, &proof, &queued.job, Answer::CameOut),
            Err(PrintQueueError::NotUnknown {
                state: JobState::Queued
            })
        ));
        assert_eq!(world.job(&queued.job).state, JobState::Queued);

        resolver
            .resolve_unknown(&world.conn, &proof, &sale.job, Answer::CameOut)
            .unwrap();
        assert!(matches!(
            resolver.resolve_unknown(&world.conn, &proof, &sale.job, Answer::CameOut),
            Err(PrintQueueError::NotUnknown {
                state: JobState::Cancelled
            })
        ));
    }

    /// A model that is not the original's document, or a printer that is not
    /// the original's, would make a "duplicate" of something else. Both are
    /// refused, and nothing is written.
    #[test]
    fn a_duplicate_that_would_differ_from_its_original_is_refused() {
        let world = World::new();
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        let sale = unknown_sale(&world, &printer, &clock, &ids);
        let proof = reprint_proof(world.cashier(), at(0));
        let resolver = resolver(&clock, &ids);
        let artifacts = artifact_count(&world);

        let mut another = model();
        another.header.receipt_no = "REG01-000999".to_owned();
        let profile = profile();
        assert!(matches!(
            resolver.resolve_unknown(
                &world.conn,
                &proof,
                &sale.job,
                Answer::NoReadableReceipt {
                    model: &another,
                    profile: &profile,
                },
            ),
            Err(PrintQueueError::ModelDiffersFromOriginal)
        ));

        let (model, narrow) = (model(), PrinterProfile::simulator(PaperWidth::Mm58));
        assert!(matches!(
            resolver.resolve_unknown(
                &world.conn,
                &proof,
                &sale.job,
                Answer::NoReadableReceipt {
                    model: &model,
                    profile: &narrow,
                },
            ),
            Err(PrintQueueError::ProfileMismatch { .. })
        ));

        assert_eq!(artifact_count(&world), artifacts);
        assert_eq!(world.job(&sale.job).state, JobState::Unknown);
    }

    /// `print_resolve_unknown` takes no approval (`ref/ipc-contract.md` §3).
    /// A proof that carries one — a merchant policy escalating reprints — is
    /// refused rather than leaving the manager's handle unconsumed.
    #[test]
    fn a_proof_carrying_an_approval_is_refused() {
        let world = World::new();
        let (printer, clock, ids) = (SimulatedPrinter::new(), TestClock::new(), ids());
        let sale = unknown_sale(&world, &printer, &clock, &ids);
        let manager = UserId::from_uuid(Uuid::from_u128(0x4D));
        let binding = ApprovalBinding {
            entity_id: Uuid::from_bytes(sale.job),
            amount_minor: 0,
            content_hash: None,
        };
        let approver = authorize::<cap::SaleReprint>(
            manager,
            &GrantSet::of_role(Role::Manager),
            None,
            &binding,
            &EscalationPolicy::empty(),
            at(0),
        )
        .unwrap();
        let handle = ApprovalHandle::issue(
            ApprovalId::from_uuid(Uuid::from_u128(0xA1)),
            world.cashier(),
            &approver,
            &binding,
            "reprint".to_owned(),
            at(0),
            60_000,
            // Any fixed value serves; the job's own id keeps the nonce a
            // test fact rather than a literal a scanner reads as a key.
            sale.job,
        )
        .unwrap();
        let escalated = authorize::<cap::SaleReprint>(
            world.cashier(),
            &GrantSet::of_role(Role::Cashier),
            Some(&handle),
            &binding,
            &EscalationPolicy::empty().with_escalation::<cap::SaleReprint>(),
            at(0),
        )
        .unwrap();
        assert!(escalated.approval().is_some());

        assert!(matches!(
            resolver(&clock, &ids).resolve_unknown(
                &world.conn,
                &escalated,
                &sale.job,
                Answer::CameOut
            ),
            Err(PrintQueueError::ApprovalNotTaken)
        ));
        assert_eq!(world.job(&sale.job).state, JobState::Unknown);
    }

    #[test]
    fn a_retry_delay_of_zero_is_refused() {
        assert!(matches!(
            Timing::new(60_000, 0),
            Err(PrintQueueError::TimingInvalid { ms: 0, .. })
        ));
        assert!(matches!(
            Timing::new(0, 30_000),
            Err(PrintQueueError::TimingInvalid { ms: 0, .. })
        ));
        assert!(Timing::new(1, 1).is_ok());
    }

    #[test]
    fn the_canonical_form_sorts_every_object_at_every_depth() {
        let value = json!({"b": 1, "a": {"d": [{"f": 1, "e": 2}], "c": null}});
        assert_eq!(
            canonical(&value).unwrap(),
            r#"{"a":{"c":null,"d":[{"e":2,"f":1}]},"b":1}"#
        );
    }
}
