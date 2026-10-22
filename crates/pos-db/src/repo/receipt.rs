//! Receipts and their print queue (microstep 1.7.7,
//! `ref/hardware-and-receipts.md` §3).
//!
//! Three tables, three different natures, all defined by migration `0005`:
//!
//! * **`receipt_artifact`** is the document: immutable, synced, and the only
//!   thing a reprint ever sends. A correction is a new, linked artifact — a
//!   duplicate or a fiscal supplement — never an edit (I-4).
//! * **`print_job`** is the register-local promise to print one artifact. It is
//!   the one mutable table here, and `0005` lets it change in exactly two ways:
//!   a worker claims it under a lease, or an attempt is appended and projected
//!   onto it.
//! * **`print_attempt`** is the append-only history of what each attempt did.
//!
//! What this repository holds to, beyond the schema:
//!
//! * **Only `Failed` is retried** (§3 rule 4). [`ReceiptRepository::lease_next_job`]
//!   selects queued jobs and failed jobs whose retry time has come, and nothing
//!   else; the schema refuses to lease an `unknown` one in any case. An
//!   `Unknown` or `Partial` outcome may have put paper in a customer's hand, and
//!   replaying it would print a second original.
//! * **A lease that lapses is ambiguous, not failed.** A worker that died
//!   holding a job may have sent the bytes, so an expired lease is resolved by
//!   appending an `unknown` attempt — never by putting the job back in the
//!   queue, which `0005` refuses anyway.
//! * **Ordering is owned, not observed** (I-7). Due jobs are leased in the
//!   order they were queued, by `rowid`, never by a timestamp a device clock
//!   wrote.
//! * **Nothing here reads a clock or mints an id** (I-8). Every id and every
//!   instant is an argument, and instants are stored through
//!   [`Timestamp::to_iso8601`] so that the text comparisons `0005`'s lease
//!   triggers make are comparisons of one fixed format.
//! * **The content hash is the stored bytes' BLAKE3 digest, taken here**, so it
//!   cannot describe bytes other than the ones beside it. It is a plain content
//!   address — reproducible with any BLAKE3 tool — because what reads it
//!   (reprint verification, convergence) compares documents, not signatures.
//!
//! A receipt artifact is a fact: the caller writes it in the same transaction
//! as the commit envelope that names it (I-9). A print job and its attempts
//! are register-local and never synced, so they need none.

use pos_domain::Timestamp;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

use crate::DbError;

/// Ids are a UUIDv7's sixteen bytes, as everywhere in `pos-db`.
pub type Id = [u8; 16];

/// What a receipt artifact is to its sale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    /// The receipt the sale was finalized with. One per sale.
    Original,
    /// A watermarked copy, created only when a person said no readable
    /// receipt came out (§3 rule 5). Cites the original.
    Duplicate,
    /// The fiscal QR that cleared after the paper printed (§3 rule 8). Cites
    /// the original.
    FiscalSupplement,
}

impl ArtifactKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Duplicate => "duplicate",
            Self::FiscalSupplement => "fiscal_supplement",
        }
    }

    fn parse(value: &str) -> Result<Self, DbError> {
        match value {
            "original" => Ok(Self::Original),
            "duplicate" => Ok(Self::Duplicate),
            "fiscal_supplement" => Ok(Self::FiscalSupplement),
            other => Err(invalid(format!("unknown artifact kind {other:?}"))),
        }
    }
}

/// A receipt document, as the caller writes it. The content hash is not a
/// field: [`ReceiptRepository::append_artifact`] takes it from the bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct ReceiptArtifact {
    pub id: Id,
    pub sale_id: Id,
    pub kind: ArtifactKind,
    /// The original a duplicate or fiscal supplement cites; `None` for an
    /// original. The schema refuses any other combination.
    pub source_artifact_id: Option<Id>,
    /// The byte format — `escpos` for a receipt printer.
    pub format: String,
    pub template_version: String,
    /// The profile the bytes were rendered for. A register bound to another
    /// cannot reproduce them, and says so (§3 rule 7).
    pub printer_profile: String,
    pub fiscal_version: Option<String>,
    /// The document, byte for byte as a printer receives it. It can carry a
    /// buyer's name and tax number, so it never appears in `Debug`.
    pub content_bytes: Vec<u8>,
    pub generated_at: Timestamp,
}

impl core::fmt::Debug for ReceiptArtifact {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ReceiptArtifact")
            .field("id", &self.id)
            .field("sale_id", &self.sale_id)
            .field("kind", &self.kind)
            .field("source_artifact_id", &self.source_artifact_id)
            .field("format", &self.format)
            .field("template_version", &self.template_version)
            .field("printer_profile", &self.printer_profile)
            .field("fiscal_version", &self.fiscal_version)
            .field(
                "content_bytes",
                &format_args!("<redacted, {} bytes>", self.content_bytes.len()),
            )
            .field("generated_at", &self.generated_at)
            .finish()
    }
}

/// An artifact read back, with the digest stored beside its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArtifact {
    pub artifact: ReceiptArtifact,
    pub content_hash: [u8; 32],
}

/// Where a print job stands. `0005` derives every change after a claim from
/// the attempt that caused it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Printing,
    /// The last attempt may have printed. Only a person resolves it.
    Unknown,
    Printed,
    /// Refused before a byte was accepted; retried when its time comes.
    Failed,
    Cancelled,
}

impl JobState {
    fn parse(value: &str) -> Result<Self, DbError> {
        match value {
            "queued" => Ok(Self::Queued),
            "printing" => Ok(Self::Printing),
            "unknown" => Ok(Self::Unknown),
            "printed" => Ok(Self::Printed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(invalid(format!("unknown print job state {other:?}"))),
        }
    }
}

/// The register's promise to print one artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintJob {
    pub id: Id,
    pub artifact_id: Id,
    pub state: JobState,
    pub attempts: u32,
    pub next_attempt_at: Option<Timestamp>,
    pub claimed_at: Option<Timestamp>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub last_error: Option<String>,
}

/// What one attempt to print did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// Every byte was taken.
    Printed,
    /// The device went silent, or a lease lapsed: paper may exist.
    Unknown,
    /// Some bytes were taken, then the device went away: paper may exist.
    Partial,
    /// Refused before a byte was taken: no paper can exist.
    Failed,
    /// Closed by a person, or superseded; no further attempt is made.
    Cancelled,
}

impl AttemptOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Printed => "printed",
            Self::Unknown => "unknown",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Result<Self, DbError> {
        match value {
            "printed" => Ok(Self::Printed),
            "unknown" => Ok(Self::Unknown),
            "partial" => Ok(Self::Partial),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(invalid(format!("unknown print attempt outcome {other:?}"))),
        }
    }
}

/// One appended attempt. `attempt_no` is the job's next one; the schema
/// refuses any other, and refuses an outcome the job's state cannot take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintAttempt {
    pub id: Id,
    pub job_id: Id,
    pub attempt_no: u32,
    pub outcome: AttemptOutcome,
    /// A stable code naming why — `paper_out`, `lease_expired` — never a
    /// device message, which could carry anything.
    pub error_code: Option<String>,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    pub sent_at: Option<Timestamp>,
    /// When a failed job may be tried again. Only a `Failed` attempt carries
    /// one; `None` there leaves the job for a person.
    pub retry_at: Option<Timestamp>,
}

/// A worker's claim on one job.
#[derive(Debug, Clone, Copy)]
pub struct Lease<'a> {
    /// Who holds it — one worker's stable name.
    pub owner: &'a str,
    /// When it is taken; also the instant a failed job's retry is due by.
    pub at: Timestamp,
    /// When it lapses unless renewed. Must be later than `at`.
    pub until: Timestamp,
}

/// Reads and writes of receipt artifacts, print jobs and print attempts.
pub struct ReceiptRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ReceiptRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Write one immutable artifact in the caller's transaction, and return
    /// the BLAKE3 digest stored beside its bytes.
    ///
    /// The caller names the artifact in the same transaction's commit envelope
    /// (I-9); nothing here writes one.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] for anything the schema refuses: a second original
    /// for one sale, a duplicate that does not cite this sale's original, an
    /// original that cites anything.
    pub fn append_artifact(
        &self,
        tx: &Transaction<'_>,
        artifact: &ReceiptArtifact,
    ) -> Result<[u8; 32], DbError> {
        let content_hash = *blake3::hash(&artifact.content_bytes).as_bytes();
        tx.execute(
            "INSERT INTO receipt_artifact
               (id, sale_id, artifact_kind, source_artifact_id, format, template_version,
                printer_profile, fiscal_version, content_bytes, content_hash, generated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                &artifact.id,
                &artifact.sale_id,
                artifact.kind.as_str(),
                artifact.source_artifact_id.as_ref(),
                &artifact.format,
                &artifact.template_version,
                &artifact.printer_profile,
                artifact.fiscal_version.as_deref(),
                &artifact.content_bytes,
                &content_hash,
                artifact.generated_at.to_iso8601(),
            ],
        )?;
        Ok(content_hash)
    }

    /// Queue an artifact's first print job: `queued`, never attempted — the
    /// shape `0005` requires before a sale may complete.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] when the artifact does not exist or already has a
    /// job: one artifact, one promise to print it.
    pub fn queue_initial(
        &self,
        tx: &Transaction<'_>,
        job_id: &Id,
        artifact_id: &Id,
        created_at: Timestamp,
    ) -> Result<(), DbError> {
        let at = created_at.to_iso8601();
        tx.execute(
            "INSERT INTO print_job (id, artifact_id, state, attempts, created_at, updated_at)
             VALUES (?1, ?2, 'queued', 0, ?3, ?3)",
            params![job_id, artifact_id, &at],
        )?;
        Ok(())
    }

    /// Claim the next job that is due — the oldest queued job, or a failed one
    /// whose retry time has come — under `lease`, and return it.
    ///
    /// `Unknown` jobs are never returned: their outcome may be paper, and only
    /// a person may say it is not. `None` means nothing is due.
    ///
    /// # Errors
    ///
    /// [`DbError::PrintLeaseInvalid`] when `lease.until` is not after
    /// `lease.at`; [`DbError::Sqlite`] if the claim is refused.
    pub fn lease_next_job(
        &self,
        tx: &Transaction<'_>,
        lease: Lease<'_>,
    ) -> Result<Option<PrintJob>, DbError> {
        if lease.until <= lease.at {
            return Err(DbError::PrintLeaseInvalid);
        }
        let at = lease.at.to_iso8601();
        let due: Option<Id> = tx
            .query_row(
                "SELECT id FROM print_job
                  WHERE state = 'queued'
                     OR (state = 'failed' AND next_attempt_at IS NOT NULL
                         AND next_attempt_at <= ?1)
                  ORDER BY rowid
                  LIMIT 1",
                [&at],
                |row| row.get(0),
            )
            .optional()?;
        let Some(id) = due else {
            return Ok(None);
        };
        tx.execute(
            "UPDATE print_job
                SET state = 'printing', claimed_at = ?1, lease_owner = ?2,
                    lease_expires_at = ?3, next_attempt_at = NULL, updated_at = ?1
              WHERE id = ?4 AND state IN ('queued','failed')",
            params![&at, lease.owner, lease.until.to_iso8601(), &id],
        )?;
        read_job(tx, &id)
    }

    /// Append one attempt. `0005` projects it onto the job: its state, its
    /// count, its retry time, and the end of its lease.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] when the attempt is not the job's next one, or the
    /// job's state cannot take this outcome — a result on a job nobody
    /// claimed, a retry time on anything but a failure.
    pub fn append_attempt(
        &self,
        tx: &Transaction<'_>,
        attempt: &PrintAttempt,
    ) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO print_attempt
               (id, job_id, attempt_no, outcome, error_code, started_at, finished_at,
                sent_at, retry_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &attempt.id,
                &attempt.job_id,
                attempt.attempt_no,
                attempt.outcome.as_str(),
                attempt.error_code.as_deref(),
                attempt.started_at.to_iso8601(),
                attempt.finished_at.map(Timestamp::to_iso8601),
                attempt.sent_at.map(Timestamp::to_iso8601),
                attempt.retry_at.map(Timestamp::to_iso8601),
            ],
        )?;
        Ok(())
    }

    /// Jobs whose lease lapsed before `now` while they were printing: a worker
    /// that stopped mid-job. Each is resolved as `unknown`, never re-queued.
    ///
    /// # Errors
    ///
    /// [`DbError`] when a stored job cannot be read.
    pub fn expired_leases(&self, now: Timestamp) -> Result<Vec<PrintJob>, DbError> {
        self.jobs_where(
            "state = 'printing' AND lease_expires_at < ?1",
            Some(now.to_iso8601()),
        )
    }

    /// Every job still owed a print, in the order it was queued: queued,
    /// claimed, failed, or awaiting a person's answer. This is the unprinted
    /// worklist; a completed sale carries no flag of its own (§3 rule 3).
    ///
    /// # Errors
    ///
    /// [`DbError`] when a stored job cannot be read.
    pub fn unprinted(&self) -> Result<Vec<PrintJob>, DbError> {
        self.jobs_where("state IN ('queued','printing','failed','unknown')", None)
    }

    /// The jobs only a person can close (§3 rule 5).
    ///
    /// # Errors
    ///
    /// [`DbError`] when a stored job cannot be read.
    pub fn awaiting_an_answer(&self) -> Result<Vec<PrintJob>, DbError> {
        self.jobs_where("state = 'unknown'", None)
    }

    /// One job.
    ///
    /// # Errors
    ///
    /// [`DbError`] when the stored job cannot be read.
    pub fn job(&self, id: &Id) -> Result<Option<PrintJob>, DbError> {
        read_job(self.conn, id)
    }

    /// One artifact, with its stored digest.
    ///
    /// # Errors
    ///
    /// [`DbError`] when the stored artifact cannot be read.
    pub fn artifact(&self, id: &Id) -> Result<Option<StoredArtifact>, DbError> {
        self.conn
            .query_row(
                "SELECT id, sale_id, artifact_kind, source_artifact_id, format,
                        template_version, printer_profile, fiscal_version, content_bytes,
                        content_hash, generated_at
                   FROM receipt_artifact WHERE id = ?1",
                [id],
                |row| Ok(artifact_row(row)),
            )
            .optional()?
            .transpose()
    }

    /// A job's attempts, in the order they were appended.
    ///
    /// # Errors
    ///
    /// [`DbError`] when a stored attempt cannot be read.
    pub fn attempts(&self, job_id: &Id) -> Result<Vec<PrintAttempt>, DbError> {
        let mut statement = self.conn.prepare(
            "SELECT id, job_id, attempt_no, outcome, error_code, started_at, finished_at,
                    sent_at, retry_at
               FROM print_attempt WHERE job_id = ?1 ORDER BY attempt_no",
        )?;
        let rows = statement.query_map([job_id], |row| Ok(attempt_row(row)))?;
        rows.map(|row| row.map_err(DbError::from).and_then(core::convert::identity))
            .collect()
    }

    fn jobs_where(&self, filter: &str, now: Option<String>) -> Result<Vec<PrintJob>, DbError> {
        let sql = format!("SELECT {JOB_COLUMNS} FROM print_job WHERE {filter} ORDER BY rowid");
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(now.iter()), |row| {
            Ok(job_row(row))
        })?;
        rows.map(|row| row.map_err(DbError::from).and_then(core::convert::identity))
            .collect()
    }
}

const JOB_COLUMNS: &str = "id, artifact_id, state, attempts, next_attempt_at, claimed_at, \
                           lease_owner, lease_expires_at, created_at, updated_at, last_error";

fn read_job(conn: &Connection, id: &Id) -> Result<Option<PrintJob>, DbError> {
    conn.query_row(
        &format!("SELECT {JOB_COLUMNS} FROM print_job WHERE id = ?1"),
        [id],
        |row| Ok(job_row(row)),
    )
    .optional()?
    .transpose()
}

fn invalid(reason: String) -> DbError {
    DbError::StoredPrintRecordInvalid { reason }
}

fn instant(text: &str) -> Result<Timestamp, DbError> {
    Timestamp::parse_iso8601(text).map_err(|error| invalid(format!("a stored instant: {error}")))
}

fn optional_instant(text: Option<String>) -> Result<Option<Timestamp>, DbError> {
    text.as_deref().map(instant).transpose()
}

fn id(bytes: &[u8]) -> Result<Id, DbError> {
    Id::try_from(bytes).map_err(|_| invalid(format!("an id of {} bytes", bytes.len())))
}

fn count(value: i64) -> Result<u32, DbError> {
    u32::try_from(value).map_err(|_| invalid(format!("a count of {value}")))
}

fn job_row(row: &Row<'_>) -> Result<PrintJob, DbError> {
    Ok(PrintJob {
        id: id(&row.get::<_, Vec<u8>>(0)?)?,
        artifact_id: id(&row.get::<_, Vec<u8>>(1)?)?,
        state: JobState::parse(&row.get::<_, String>(2)?)?,
        attempts: count(row.get(3)?)?,
        next_attempt_at: optional_instant(row.get(4)?)?,
        claimed_at: optional_instant(row.get(5)?)?,
        lease_owner: row.get(6)?,
        lease_expires_at: optional_instant(row.get(7)?)?,
        created_at: instant(&row.get::<_, String>(8)?)?,
        updated_at: instant(&row.get::<_, String>(9)?)?,
        last_error: row.get(10)?,
    })
}

fn attempt_row(row: &Row<'_>) -> Result<PrintAttempt, DbError> {
    Ok(PrintAttempt {
        id: id(&row.get::<_, Vec<u8>>(0)?)?,
        job_id: id(&row.get::<_, Vec<u8>>(1)?)?,
        attempt_no: count(row.get(2)?)?,
        outcome: AttemptOutcome::parse(&row.get::<_, String>(3)?)?,
        error_code: row.get(4)?,
        started_at: instant(&row.get::<_, String>(5)?)?,
        finished_at: optional_instant(row.get(6)?)?,
        sent_at: optional_instant(row.get(7)?)?,
        retry_at: optional_instant(row.get(8)?)?,
    })
}

fn artifact_row(row: &Row<'_>) -> Result<StoredArtifact, DbError> {
    let source: Option<Vec<u8>> = row.get(3)?;
    let hash: Vec<u8> = row.get(9)?;
    Ok(StoredArtifact {
        artifact: ReceiptArtifact {
            id: id(&row.get::<_, Vec<u8>>(0)?)?,
            sale_id: id(&row.get::<_, Vec<u8>>(1)?)?,
            kind: ArtifactKind::parse(&row.get::<_, String>(2)?)?,
            source_artifact_id: source.as_deref().map(id).transpose()?,
            format: row.get(4)?,
            template_version: row.get(5)?,
            printer_profile: row.get(6)?,
            fiscal_version: row.get(7)?,
            content_bytes: row.get(8)?,
            generated_at: instant(&row.get::<_, String>(10)?)?,
        },
        content_hash: <[u8; 32]>::try_from(hash.as_slice())
            .map_err(|_| invalid(format!("a content hash of {} bytes", hash.len())))?,
    })
}
