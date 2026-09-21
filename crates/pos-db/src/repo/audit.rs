//! The register's audit hash chain, appended to and read back (microstep 1.6.6).
//!
//! Gap G-7. `ref/plan-validation.md:343` states the requirement in the words
//! that decide this module's surface: *"`prev_hash`/`hash` columns are
//! specified; the canonical serialization, coverage, verifier, and chain-break
//! behaviour are not. **An unverifiable hash chain is decoration**."* The
//! columns shipped in `0004` and the arithmetic shipped in
//! [`pos_domain::audit`] at 1.6.5. What was still missing is the half that
//! makes the other two evidence: something that writes a row whose `hash` is
//! the hash of that row, and something that reads the rows back so
//! [`verify_chain`] has a chain to walk. Both are here, and nothing else is.
//!
//! **Append-only, and the type system says so.** There is no update method and
//! no delete method — not private ones, none. `audit_log_no_update` and
//! `audit_log_no_delete` (`0004:516`, `:522`) refuse both from the other side,
//! and `audit_log_still_refuses_update_and_delete` asserts their exact
//! messages, because a guard nobody has seen fail is a guard nobody should
//! trust.
//!
//! **This table is a fact, so its delivery envelope comes first.**
//! `audit_log_has_ready_commit` (`0004:502`) refuses the insert unless a
//! `fact_commit_member` row naming `entity = 'audit_log'` and this row's `id`
//! already joins a *ready* `sync_commit`. So [`AuditRepository::append`] is not
//! self-sufficient and is not meant to be: the caller writes the envelope
//! through [`crate::repo::outbox::OutboxRepository::write_commit`] earlier in
//! the same transaction, exactly as `approval.rs` requires of an issuance. That
//! ordering is also what makes the chain safe to write, which is the next
//! section.
//!
//! # Why the chain cannot fork
//!
//! A fork is two rows claiming the same parent: two `prev_hash` values equal to
//! one row's `hash`. Nothing in SQL refuses it — there is no `CHECK`, no unique
//! index and no trigger on `prev_hash`, and an `INSERT` that duplicates a
//! parent is accepted. **The guarantee is therefore structural, not declared**,
//! and it is worth stating exactly where it comes from, because a reader who
//! assumes the schema is holding the line will remove the thing that is.
//!
//! [`AuditRepository::append`] reads the head and writes the row **through the
//! caller's `&Transaction`**, and `seq` is derived from that same read and
//! written explicitly. Three independent things then have to fail at once for
//! two appends to share a parent:
//!
//! 1. **The envelope is already written.** I-9 and the ready-commit trigger put
//!    the caller's `sync_commit`, `fact_commit_member` and `sync_outbox` writes
//!    *before* the audit insert in the same transaction — so by the time the
//!    head is read, this transaction already holds SQLite's single write lock
//!    and no second appender can be between its own read and its own write.
//! 2. **A stale snapshot cannot be written.** A transaction that read before
//!    another committed is refused at its write, *immediately* and without
//!    consulting `busy_timeout` — waiting cannot repair a snapshot. So the
//!    writer that read a stale head never commits; it errors.
//! 3. **`seq` is the primary key.** Two rows sharing a parent would have been
//!    computed from one head, and this module derives `seq` from that same
//!    head, so the second insert collides on `audit_log.seq` before it can
//!    store its hash.
//!
//! The requirement that made this microstep awkward — `seq` is inside the
//! hashed bytes and there is no `UPDATE` to repair it afterwards — is the same
//! thing that supplies the third guard. Writing `seq` explicitly is not a
//! workaround for AUTOINCREMENT; it is the backstop.
//!
//! `concurrent_appends_serialize` is the test, and it asserts the property
//! rather than the absence of an error: every appended row links to its
//! predecessor, and no two rows of one register share a `prev_hash`. Asserting
//! only that both appends returned `Ok` would pass against a forked chain.
//!
//! # Why `MAX(seq)` and not `sqlite_sequence`
//!
//! Both answer the same question and, for this table, always with the same
//! number: `audit_log_no_delete` means no committed row disappears, and
//! `sqlite_sequence` rolls back with its transaction, so its value and
//! `MAX(seq)` cannot diverge on their own. They differ in what an attacker can
//! do to them. `sqlite_sequence` is an ordinary table with **no trigger guard
//! of any kind** — `UPDATE sqlite_sequence SET seq = 9999` is accepted — and
//! after that rewrite the counter and the chain disagree. `MAX(seq)` reads the
//! hashed, delete-guarded column itself. The allocator reads the fact, never
//! the bookkeeping.
//!
//! `seq` is table-global because it is `INTEGER PRIMARY KEY`; `prev_hash` is
//! per register because [`verify_chain`] is per register. In the shipping shape
//! — one register per database — the two coincide, and the asymmetry only shows
//! up in a file holding two registers, where chaining globally would give every
//! single-register read a `Broken` at its first row.
//!
//! # Reading a row back, and the refusal this module will not make
//!
//! [`pos_domain::AuditIntent`] spells `action` and `entity` as `&'static str`
//! and SQLite returns `String`, so the read path has to produce a `'static`
//! borrow from a runtime value. With `unsafe_code` forbidden there is exactly
//! one way to do that, and it is a leak — so the leak is deduplicated: one per
//! *distinct* spelling this process has ever seen, never one per row.
//!
//! The alternative was to intern against a closed vocabulary and refuse
//! anything outside it. That is wrong twice. On the facts, there is no such
//! vocabulary: `action` is already a superset of `pos_domain::cap::ALL` —
//! `registered_chain.rs` writes `sale.complete`, which is not a capability —
//! and `entity` has no registry anywhere in the workspace. On the attack, a
//! read that failed on an unrecognised spelling would let **one inserted row
//! disable the whole verifier**, with no forensic signal and no located break;
//! and every routine upgrade that added an action would make an older verifier
//! cry tamper. [`pos_domain::canonical_bytes`] is deliberately total for the
//! same reason — *"a forensic tool must be able to hash a row that should never
//! have been written"* — and a read path in front of it that is not total
//! throws that away.
//!
//! So an unknown `action` or `entity` is **never** an error here.
//!
//! # No single row can silence a register, and the first draft got this wrong
//!
//! Some rows genuinely cannot be rebuilt: a malformed id, a negative `seq`, an
//! unparsable timestamp, a payload that is not JSON, a NULL `entity_id`, or a
//! `canonical_version` this build does not hash. The first version of this
//! module returned `Err` for those and threw the rest of the chain away with
//! them — which built, for six conditions, exactly the hole the section above
//! refuses to build for one.
//!
//! It is worth being precise about how bad that was, because the shape recurs.
//! `audit_log_no_update` and `audit_log_no_delete` guard mutation; **nothing
//! guards `INSERT`**. So a SQL console needed only to *add* one row — plus an
//! envelope the same console can write — and every honest row below it became
//! unreachable through the only public read. Neither trigger would then let
//! anyone remove the poison row or repair it. That is permanent, it is cheaper
//! than the re-chaining attack the chain exists to detect, and it is more
//! effective, because a re-chained history still verifies *somewhere* while an
//! `Err` locates nothing at all.
//!
//! [`AuditRepository::chain`] therefore returns an [`AuditChain`]: every row it
//! could rebuild, in order, **plus** an optional [`ChainStop`] naming the `seq`
//! it stopped at and why. A caller walks the prefix with [`verify_chain`] and
//! reports the stop beside the verdict; a poison row costs the rows above it
//! and nothing below. `DbError` is then reserved for what it should always have
//! meant here — the database itself would not answer.
//!
//! **A stop is not a tamper verdict**, and the two must not be printed as one.
//! `ref/ui-spec.md:133` gives a chain break an alarm that blocks nothing, and a
//! row this build cannot parse is at least as likely to be a newer build's row
//! as an attacker's.
//!
//! # What this module deliberately does not do
//!
//! It does not write `audit_checkpoint`. Anchoring the head belongs to Z close,
//! and [`verify_chain`] takes the anchor as an argument precisely so the
//! storage of one is somebody else's decision.
//!
//! It does not verify anything itself. [`AuditRepository::chain`] returns the
//! rows and the caller passes them to [`verify_chain`]; wrapping that would
//! bury a `ChainVerdict` inside a `Result` and invite a caller to treat a
//! `Broken` chain as a failed read. `ref/ui-spec.md:133` is explicit that a
//! chain break *"blocks nothing, hides from no one"* — no error returned here
//! should be one a register could only answer by refusing to sell.
//!
//! It does not read a clock or mint an id (I-8). `register`, `id` and
//! `intent.at` are arguments, supplied by the shell that owns them.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use pos_domain::audit::{DOMAIN, GENESIS, VERSION};
use pos_domain::{
    ApprovalId, AuditIntent, CanonicalAuditEntry, RegisterId, Timestamp, UserId, chain_hash,
    check_payload,
};
use rusqlite::{Connection, Transaction, params};
use uuid::Uuid;

use crate::DbError;

/// The width of a chain digest, in bytes. BLAKE3, as `ref/security-compliance.md` §4 fixes it.
const DIGEST_BYTES: usize = 32;

/// The width of every id in this schema (conventions §2).
const ID_BYTES: usize = 16;

/// `audit_log_has_ready_commit`'s message, matched exactly.
///
/// The trigger is the one refusal a correct caller still meets — it fires when
/// the delivery envelope was not written first, which is an ordering mistake
/// rather than a corrupt database. Mapping it by its own words, the way
/// `approval.rs` maps `approval_consumption`'s, is what turns
/// `DbError::Sqlite(SqliteFailure(…))` into a sentence that names the fix.
const ENVELOPE_MISSING_MESSAGE: &str = "audit fact requires its complete delivery envelope";

/// What an append computed and stored: this row's place in the chain.
///
/// Returned rather than discarded because the caller needs all three. `seq` is
/// what an anchor records, `hash` is the register's new head, and a caller that
/// wanted them from a re-read would be taking a second snapshot of a row it
/// just wrote.
///
/// No field here is a payload, so the derived `Debug` is safe — unlike
/// [`StoredAuditEntry`]'s, below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppendedAudit {
    /// `audit_log.seq`, fixed before the hash was taken over it.
    pub seq: u64,
    /// The register's previous head, or [`GENESIS`] for the first row.
    pub prev_hash: [u8; DIGEST_BYTES],
    /// This row's `hash`, and the register's new head.
    pub hash: [u8; DIGEST_BYTES],
}

/// One `audit_log` row, rebuilt into the five values [`verify_chain`] walks.
///
/// [`verify_chain`]: pos_domain::verify_chain
#[derive(Clone, PartialEq)]
pub struct StoredAuditEntry {
    /// `audit_log.seq`, inside the hash.
    pub seq: u64,
    /// `audit_log.id`, inside the hash.
    pub id: Uuid,
    /// Every remaining hashed field, as the domain models them.
    pub intent: AuditIntent,
    /// The parent this row claims.
    pub prev_hash: [u8; DIGEST_BYTES],
    /// This row's stored digest, as read — not as recomputed.
    pub hash: [u8; DIGEST_BYTES],
}

impl StoredAuditEntry {
    /// The tuple [`verify_chain`] takes, in its order.
    ///
    /// It exists so the column order is fixed once, here, rather than at every
    /// call site: a test that assembled the tuple itself would be asserting
    /// against its own ordering rather than against the reader's.
    ///
    /// [`verify_chain`]: pos_domain::verify_chain
    #[must_use]
    pub const fn as_verifier_row(
        &self,
    ) -> (
        u64,
        Uuid,
        &AuditIntent,
        &[u8; DIGEST_BYTES],
        &[u8; DIGEST_BYTES],
    ) {
        (self.seq, self.id, &self.intent, &self.prev_hash, &self.hash)
    }
}

/// What a payload renders as, so a derived `Debug` cannot print one.
///
/// Length rather than nothing, for the reason `outbox.rs` records: a mismatch
/// is diagnosed by *which* row disagreed and by how big its payload was, and a
/// bare marker throws that away for no additional protection. Bytes, not
/// characters — the canonical form is UTF-8 and an audit payload may carry
/// Arabic.
fn redacted_payload(payload: &serde_json::Value) -> String {
    format!("<redacted, {} bytes>", payload.to_string().len())
}

/// `Debug` is hand-written, and the reason differs from `outbox.rs`'s.
///
/// There, the argument was that a canonical sale payload *does* carry
/// `buyer_name`, `buyer_id_value` and a customer `phone`. That argument does
/// not transfer: `audit_log.payload`'s own DDL says "NEVER PII or card data"
/// and `ref/security-compliance.md` §2 makes it an invariant, so a conforming
/// payload here holds nothing on the registry in §6.
///
/// This is therefore a **latent** guard on a column whose contract already
/// forbids the data, and saying so is the point — `.claude/rules/security.md`
/// names "test fixtures that print" among the surfaces the never-list reaches,
/// and [`StoredAuditEntry`] is a public type handed to callers this crate does
/// not control, carrying a payload that some future caller will get wrong.
/// `pos-db` emits no `tracing` at all, so nothing prints this today; that is
/// not the guarantee.
///
/// **`reason` and the digests are deliberately legible.** `reason` is the
/// operator's own stated words, which is the field an investigator reads first,
/// and it is on no registry. `prev_hash` and `hash` end in `_hash`, which the
/// never-log list covers literally — whether a content digest is in scope for
/// that rule or explicitly out of it is the open question on #174, and nothing
/// here depends on the answer, because both readings agree about `payload`.
///
/// `AuditIntent`'s own derived `Debug` in `pos-domain` still prints a payload.
/// That is #174's third crate, it is out of this microstep's files, and it is
/// named here rather than left silently uncovered — which is the omission the
/// #179 audit found in the first sweep.
impl core::fmt::Debug for StoredAuditEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StoredAuditEntry")
            .field("seq", &self.seq)
            .field("id", &self.id)
            .field("actor", &self.intent.actor)
            .field("approver", &self.intent.approver)
            .field("approval", &self.intent.approval)
            .field("action", &self.intent.action)
            .field("entity", &self.intent.entity)
            .field("entity_id", &self.intent.entity_id)
            .field("reason", &self.intent.reason)
            .field("payload", &redacted_payload(&self.intent.payload))
            .field("at", &self.intent.at)
            .field("prev_hash", &self.prev_hash)
            .field("hash", &self.hash)
            .finish()
    }
}

/// Where a chain read stopped, and why. Everything below it still came back.
///
/// A stop is a statement about **this build's** ability to rebuild one row, not
/// a verdict about the register. The commonest cause once this product has more
/// than one version in the field is a row written under a canonical layout this
/// binary does not know, which is an upgrade rather than a tamper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainStop {
    /// The `audit_log.seq` that could not be rebuilt — where a forensic
    /// investigation starts.
    pub seq: i64,
    /// What defeated the rebuild. Key names and widths only, never a value.
    pub reason: String,
}

/// One register's chain, as far as this build could read it.
///
/// The prefix and the stop are one value on purpose: a caller cannot obtain the
/// rows without also being handed the reason the list may be short, which is
/// the mistake the first version of this module made by returning a bare
/// `Result<Vec<_>, _>`.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditChain {
    entries: Vec<StoredAuditEntry>,
    stopped: Option<ChainStop>,
}

impl AuditChain {
    /// Every row rebuilt, in ascending `seq`.
    ///
    /// **This is a prefix, not necessarily the whole log.** Check
    /// [`Self::stopped`] before describing a verdict over it as covering the
    /// register.
    #[must_use]
    pub fn entries(&self) -> &[StoredAuditEntry] {
        &self.entries
    }

    /// The row the read stopped at, when it stopped short.
    #[must_use]
    pub const fn stopped(&self) -> Option<&ChainStop> {
        self.stopped.as_ref()
    }

    /// The prefix in the shape [`verify_chain`] takes.
    ///
    /// [`verify_chain`]: pos_domain::verify_chain
    pub fn verifier_rows(
        &self,
    ) -> impl Iterator<
        Item = (
            u64,
            Uuid,
            &AuditIntent,
            &[u8; DIGEST_BYTES],
            &[u8; DIGEST_BYTES],
        ),
    > {
        self.entries.iter().map(StoredAuditEntry::as_verifier_row)
    }
}

/// Appends to, and reads back, one register's hash-chained audit log.
pub struct AuditRepository<'c> {
    conn: &'c Connection,
}

impl<'c> AuditRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Append one audit fact inside the caller's transaction.
    ///
    /// **The caller must already have written a ready delivery envelope naming
    /// `id`.** `audit_log_has_ready_commit` refuses the fact without one, and
    /// that ordering is also what serializes the head read — see the module
    /// documentation on why the chain cannot fork.
    ///
    /// `id` is an argument because the manifest has already named it and
    /// [`crate::repo::approval::ApprovalRepository::consume`] will name it
    /// again; `register` is an argument because [`AuditIntent`] is a pure value
    /// and a till is a shell fact (I-8).
    ///
    /// # Errors
    ///
    /// [`DbError::AuditPayloadRefused`] when [`check_payload`] refuses the
    /// payload. It is checked **before** anything is written, because
    /// `audit_log` has no `UPDATE` and a bad row is permanent.
    /// [`DbError::AuditEnvelopeMissing`] when the envelope was not written
    /// first. [`DbError::AuditSeqInvalid`] when the stored head is not a chain
    /// position, and [`DbError::AuditRowUnreadable`] when it is not a digest.
    pub fn append(
        &self,
        tx: &Transaction<'_>,
        register: RegisterId,
        id: Uuid,
        intent: &AuditIntent,
    ) -> Result<AppendedAudit, DbError> {
        check_payload(&intent.payload).map_err(refused)?;

        // One statement, through the caller's transaction, for both halves of
        // the link. Through `tx` rather than `self.conn` so the read is on the
        // connection that holds the write lock whatever connection this
        // repository was constructed over.
        // Three scalars, not two: the head row's own `seq` is selected as well
        // as its `hash`, so a refusal names the row that is actually corrupt.
        // `MAX(seq)` is table-global and would name whichever register wrote
        // the highest number in the file — sending an investigation to a
        // healthy row belonging to another till.
        let (table_head, head_seq, head_hash): (i64, Option<i64>, Option<Vec<u8>>) = tx.query_row(
            "SELECT (SELECT IFNULL(MAX(seq), 0) FROM audit_log),
                        (SELECT seq FROM audit_log
                          WHERE register_id = ?1
                          ORDER BY seq DESC
                          LIMIT 1),
                        (SELECT hash FROM audit_log
                          WHERE register_id = ?1
                          ORDER BY seq DESC
                          LIMIT 1)",
            [register.as_uuid().as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        let seq = u64::try_from(table_head)
            .map_err(|_| DbError::AuditSeqInvalid { found: table_head })?
            .saturating_add(1);
        let prev_hash = match head_hash {
            None => GENESIS,
            Some(bytes) => {
                let at = head_seq.unwrap_or(table_head);
                bytes
                    .try_into()
                    .map_err(|bytes: Vec<u8>| DbError::AuditHeadUnreadable {
                        seq: at,
                        reason: format!(
                            "hash holds {} bytes; a chain digest is BLAKE3's {DIGEST_BYTES}",
                            bytes.len()
                        ),
                    })?
            }
        };

        let hash = chain_hash(
            &prev_hash,
            &CanonicalAuditEntry {
                domain: DOMAIN,
                version: VERSION,
                register_id: register,
                seq,
                id,
                intent,
            },
        );

        // `canonical_version` is written explicitly rather than left to the
        // column's `DEFAULT 1`. `VERSION` is already inside the hashed bytes,
        // and the default lives in a committed migration that can never be
        // edited while `VERSION` is the constant a layout change bumps —
        // sourcing the column from the default makes the two agree only by
        // coincidence.
        //
        // **No test can tell the difference today**, and the mutation sweep
        // says so: omitting the column leaves every test green, because
        // `VERSION` is 1 and so is the default. The divergence arrives with the
        // first bump, which is exactly when nobody will be looking here.
        let result = tx.execute(
            "INSERT INTO audit_log
               (seq, id, canonical_version, register_id, actor_id, approver_id,
                approval_handle_id, action, entity, entity_id, reason, payload,
                prev_hash, hash, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                i64::try_from(seq).map_err(|_| DbError::AuditSeqInvalid { found: table_head })?,
                id.as_bytes().as_slice(),
                i64::from(VERSION),
                register.as_uuid().as_bytes().as_slice(),
                intent.actor.as_uuid().as_bytes().as_slice(),
                intent.approver.map(|who| who.as_uuid().into_bytes()),
                intent.approval.map(|which| which.as_uuid().into_bytes()),
                intent.action,
                intent.entity,
                intent.entity_id.as_bytes().as_slice(),
                intent.reason.as_deref(),
                payload_text(&intent.payload),
                prev_hash.as_slice(),
                hash.as_slice(),
                intent.at.to_iso8601(),
            ],
        );

        match result {
            Ok(_) => Ok(AppendedAudit {
                seq,
                prev_hash,
                hash,
            }),
            Err(rusqlite::Error::SqliteFailure(_, Some(message)))
                if message == ENVELOPE_MISSING_MESSAGE =>
            {
                Err(DbError::AuditEnvelopeMissing)
            }
            Err(error) => Err(DbError::Sqlite(error)),
        }
    }

    /// Every row of `register`'s chain, in ascending `seq`.
    ///
    /// Materialised rather than streamed because [`verify_chain`] borrows every
    /// entry under one lifetime; there is no streaming shape that type-checks.
    ///
    /// The `ORDER BY` is load-bearing and is the one line here that no test can
    /// prove: [`verify_chain`] checks hash linkage and never that `seq`
    /// ascends, so an unordered read would report `Broken` at an honest row —
    /// but `seq` is the rowid and no index applies to `WHERE register_id = ?`,
    /// so a scan returns ascending order with or without the clause. Deleting
    /// it survived the mutation sweep, as predicted. It is a reviewed line, the
    /// same standing `lib.rs`'s `assert_fts5` call site records for itself, and
    /// it becomes load-bearing in fact the day a migration indexes
    /// `register_id`.
    ///
    /// A row this build cannot rebuild ends the walk and is reported as
    /// [`AuditChain::stopped`] — it is **not** an error, and it does not take
    /// the rows below it with it. See the module documentation for the attack
    /// that shape exists to refuse.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] only: the statement would not run, or a column came
    /// back as a type `STRICT` should have made impossible. Everything that is
    /// a property of one row is a stop.
    pub fn chain(&self, register: RegisterId) -> Result<AuditChain, DbError> {
        let mut statement = self.conn.prepare(
            "SELECT seq, id, canonical_version, actor_id, approver_id,
                    approval_handle_id, action, entity, entity_id, reason,
                    payload, prev_hash, hash, at
               FROM audit_log
              WHERE register_id = ?1
              ORDER BY seq",
        )?;
        let rows = statement.query_map([register.as_uuid().as_bytes().as_slice()], |row| {
            Ok(RawAuditRow {
                seq: row.get(0)?,
                id: row.get(1)?,
                canonical_version: row.get(2)?,
                actor: row.get(3)?,
                approver: row.get(4)?,
                approval: row.get(5)?,
                action: row.get(6)?,
                entity: row.get(7)?,
                entity_id: row.get(8)?,
                reason: row.get(9)?,
                payload: row.get(10)?,
                prev_hash: row.get(11)?,
                hash: row.get(12)?,
                at: row.get(13)?,
            })
        })?;

        let mut entries = Vec::new();
        let mut stopped = None;
        for row in rows {
            match row?.restore() {
                Ok(entry) => entries.push(entry),
                Err(stop) => {
                    stopped = Some(stop);
                    break;
                }
            }
        }
        Ok(AuditChain { entries, stopped })
    }
}

/// `audit_log`'s columns exactly as SQLite hands them over, before any of them
/// has been shown to be a domain value.
struct RawAuditRow {
    seq: i64,
    id: Vec<u8>,
    canonical_version: i64,
    actor: Vec<u8>,
    approver: Option<Vec<u8>>,
    approval: Option<Vec<u8>>,
    action: String,
    entity: String,
    entity_id: Option<Vec<u8>>,
    reason: Option<String>,
    payload: String,
    prev_hash: Vec<u8>,
    hash: Vec<u8>,
    at: String,
}

impl RawAuditRow {
    fn restore(self) -> Result<StoredAuditEntry, ChainStop> {
        let seq = self.seq;
        let stored_seq = u64::try_from(seq)
            .map_err(|_| stop(seq, "seq is negative, and no append can have written it"))?;

        // Stopped before anything else is rebuilt. A row written under a layout
        // this build does not know would verify against the wrong bytes, and
        // reporting tampering that did not happen is the one answer a forensic
        // tool must never give.
        //
        // The comparison is deliberately `!=` and not `>`: a row from *below*
        // this build's `VERSION` is equally unhashable, because
        // `canonical_bytes` emits exactly one layout — today's. That is why
        // this is a **stop and not an error**. At the first `VERSION` bump
        // every row already on disk becomes unrebuildable by the new binary,
        // and `audit_log_no_update` means they can never be re-encoded; an
        // all-or-nothing read would turn that upgrade into the permanent loss
        // of every register's history. As a stop it costs the rows above the
        // boundary and reports where it is. Whoever bumps `VERSION` owes
        // `pos-domain` a verifier that can hash both layouts; this line is what
        // keeps that debt survivable rather than fatal.
        if self.canonical_version != i64::from(VERSION) {
            return Err(stop(
                seq,
                &format!(
                    "written under canonical version {} and this build hashes \
                     version {VERSION}; hashing it against the wrong layout \
                     would report tampering that did not happen",
                    self.canonical_version
                ),
            ));
        }

        // `audit_log.entity_id` is nullable and `AuditIntent.entity_id` is not.
        // Every row this module writes fills it; a NULL is a row from somewhere
        // else, and there is no honest `Uuid` to invent for it.
        let entity_id = self
            .entity_id
            .ok_or_else(|| stop(seq, "entity_id is NULL and an intent names a row"))?;

        Ok(StoredAuditEntry {
            seq: stored_seq,
            id: uuid_at(seq, "id", self.id)?,
            intent: AuditIntent {
                actor: UserId::from_uuid(uuid_at(seq, "actor_id", self.actor)?),
                approver: self
                    .approver
                    .map(|bytes| uuid_at(seq, "approver_id", bytes))
                    .transpose()?
                    .map(UserId::from_uuid),
                approval: self
                    .approval
                    .map(|bytes| uuid_at(seq, "approval_handle_id", bytes))
                    .transpose()?
                    .map(ApprovalId::from_uuid),
                action: intern(&self.action),
                entity: intern(&self.entity),
                entity_id: uuid_at(seq, "entity_id", entity_id)?,
                reason: self.reason,
                payload: serde_json::from_str(&self.payload)
                    .map_err(|error| stop(seq, &format!("payload is not JSON: {error}")))?,
                at: Timestamp::parse_iso8601(&self.at)
                    .map_err(|error| stop(seq, &format!("at is unreadable: {error}")))?,
            },
            prev_hash: digest(seq, "prev_hash", self.prev_hash)?,
            hash: digest(seq, "hash", self.hash)?,
        })
    }
}

/// The distinct `action` and `entity` spellings this process has read back.
///
/// A `HashSet` of leaked strings rather than a map: the key *is* the value, and
/// looking one up is what avoids leaking a second copy of it.
static VOCABULARY: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();

/// A `&'static str` with the same content as `value`, leaking at most once per
/// distinct spelling.
///
/// The hash depends on a string's *content* and never on its identity, so this
/// is a lifetime device and not a claim about which spellings are legitimate —
/// see the module documentation for why refusing an unknown one would be a
/// denial-of-verification hole rather than a safety check.
///
/// The memory it holds is bounded by the register's vocabulary — a few dozen
/// actions and a handful of entity names — and never by the number of rows. A
/// hostile database with a million distinct spellings costs a million small
/// strings, which is proportional to a file the caller has already chosen to
/// read, and the process that reads one is `verify-audit`, which then exits.
///
/// A poisoned lock is recovered from rather than propagated: the only thing
/// this mutex guards is a set of `'static` strings, so a panic mid-insert can
/// leave it at worst missing one entry, which costs one extra leak and no
/// correctness.
fn intern(value: &str) -> &'static str {
    let vocabulary = VOCABULARY.get_or_init(|| Mutex::new(HashSet::new()));
    let mut seen = vocabulary
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(known) = seen.get(value) {
        return known;
    }
    let leaked: &'static str = Box::leak(value.to_owned().into_boxed_str());
    seen.insert(leaked);
    leaked
}

/// The payload as it is stored: canonical JSON, by construction.
///
/// `serde_json::Map` is a `BTreeMap` unless the `preserve_order` feature is on,
/// and it is off across this workspace, so `to_string` emits keys sorted at
/// every level with no whitespace — which is what `ref/security-compliance.md`
/// §4 means by canonical and what the column's own DDL comment claims.
/// `the_stored_payload_is_canonical_json` pins that, so the day a crate three
/// levels away turns the feature on, a test says so rather than the claim
/// quietly becoming false.
///
/// The stored bytes are not what is hashed — [`pos_domain::canonical_bytes`]
/// re-serializes the parsed value and sorts it itself — so verification
/// survives the drift either way. The column's honesty is what would not.
fn payload_text(payload: &serde_json::Value) -> String {
    payload.to_string()
}

fn uuid_at(seq: i64, column: &'static str, bytes: Vec<u8>) -> Result<Uuid, ChainStop> {
    if bytes.len() != ID_BYTES {
        return Err(stop(
            seq,
            &format!(
                "{column} holds a {}-byte id; ids are BLOB(16) (conventions §2)",
                bytes.len()
            ),
        ));
    }
    Uuid::from_slice(&bytes).map_err(|error| stop(seq, &format!("{column}: {error}")))
}

fn digest(seq: i64, column: &'static str, bytes: Vec<u8>) -> Result<[u8; DIGEST_BYTES], ChainStop> {
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        stop(
            seq,
            &format!(
                "{column} holds {} bytes; a chain digest is BLAKE3's {DIGEST_BYTES}",
                bytes.len()
            ),
        )
    })
}

/// Every reason carries a column name, a width or a parser message — never a
/// value out of the row. A stop is read by whoever is investigating the
/// register, and `.claude/rules/security.md` reaches anything that prints.
fn stop(seq: i64, reason: &str) -> ChainStop {
    ChainStop {
        seq,
        reason: reason.to_owned(),
    }
}

/// Flatten a domain refusal, the way `approval.rs` flattens a `PermissionError`.
///
/// Deliberately not a `#[from]`: 1.6.4 recorded that there is no automatic
/// conversion from a domain policy error, and the shell maps one explicitly.
/// The text is safe to carry — `AuditError` holds an RFC 6901 pointer of key
/// names and array indexes and never a value.
fn refused(error: impl core::fmt::Display) -> DbError {
    DbError::AuditPayloadRefused {
        reason: error.to_string(),
    }
}
