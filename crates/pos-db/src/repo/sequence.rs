//! Owned document counters — receipt, Z and fiscal ICV (microstep 1.9.2).
//!
//! Gap G-2. `ref/plan-validation.md:338` states the requirement in the words
//! that decide this module's shape: *"Per-register counters must be crash-safe
//! and **gap-detectable**. A gap in a receipt sequence is what an auditor asks
//! about first."* **Detectable, not impossible.** A crash between allocating a
//! number and writing the document it numbers is a gap, and the product's job
//! is to show it rather than to pretend it cannot happen — which is why
//! [`SequenceRepository::gaps`] exists at all.
//!
//! What *is* prevented is spending a number on nothing. [`SequenceRepository::next`]
//! takes the caller's `&Transaction`, so the bump and the document commit
//! together or neither does; `ref/schema.md:2676` puts it as "Receipt and Z
//! counters are bumped in the SAME transaction as the document they number".
//!
//! **This table is register-local, not a fact.** `ref/schema.md:36` names
//! `doc_sequence` in the `sync-authority-local-only` list, so unlike `outbox.rs`
//! this module writes no `sync_commit`, no `fact_commit_member` and no
//! `sync_outbox` row. `clock.rs` has the same shape for the same reason. It
//! still takes an explicit `&Transaction`, because `repo/mod.rs` gives the
//! boundary to the caller.
//!
//! Nothing here reads a clock (I-7): `ref/schema.md:2676` again — *"Counters,
//! never derived from time (E.6)."*
//!
//! ## Two refusals that are deliberate, and one hazard with no guard behind it
//!
//! **`next()` refuses `SeqKind::FiscalIcv`.** Not because the scope rule forbids
//! it — `0005`'s `CHECK` pairs `fiscal_icv` with `store` quite happily — but
//! because **the first `fiscal_icv` row that exists closes #113's reversal
//! window**. `0005` froze `scope_kind` as `store` knowingly, and the mistake
//! stays cheap only while no row has been allocated: after one, correcting the
//! namespace is a migration *plus* a data repair on a counter required to be
//! gapless. Allocation belongs to `2.7.4`, which must re-check merchant
//! decision 6.9 first. A `next()` that could create that row lets a stray test
//! spend a decision reserved for a later microstep, so it refuses by name and
//! `2.7.4` removes the refusal as part of doing the re-check.
//!
//! **Scope/kind legality is refused in Rust, and the `CHECK` is the second
//! line.** `0005`'s composite `CHECK` is table-level and unnamed, so it raises a
//! generic constraint failure with no message to map — unlike the triggers
//! `approval.rs` keys named errors off. The named error therefore has to come
//! from here.
//!
//! **Never `INSERT OR REPLACE`.** `REPLACE` deletes the row and inserts a new
//! one, so `doc_sequence_monotonic` — a `BEFORE UPDATE OF next_value` trigger —
//! never fires, and the counter silently resets to 1. `doc_sequence` has **no
//! delete guard** (issue #179's fourth finding, still open), so nothing in the
//! schema would catch it. The upsert below advances with
//! `next_value = next_value + 1`, which is the only step that trigger accepts.

use pos_domain::{RegisterId, StoreId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::DbError;

/// `scope_kind`'s two legal values, spelled as `0005`'s `CHECK` spells them.
const SCOPE_REGISTER: &str = "register";
const SCOPE_STORE: &str = "store";

/// `kind`'s three legal values, likewise.
const RECEIPT: &str = "receipt";
const ZREPORT: &str = "zreport";
const FISCAL_ICV: &str = "fiscal_icv";

/// Which counter. The storage encoding of the `kind` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeqKind {
    Receipt,
    ZReport,
    FiscalIcv,
}

impl SeqKind {
    /// The exact token `0005`'s `CHECK` admits.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SeqKind::Receipt => RECEIPT,
            SeqKind::ZReport => ZREPORT,
            SeqKind::FiscalIcv => FISCAL_ICV,
        }
    }
}

/// Whose counter. `0005` pairs each kind with exactly one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceScope {
    Register(RegisterId),
    Store(StoreId),
}

impl SequenceScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SequenceScope::Register(_) => SCOPE_REGISTER,
            SequenceScope::Store(_) => SCOPE_STORE,
        }
    }

    fn id_bytes(self) -> [u8; 16] {
        match self {
            SequenceScope::Register(id) => id.as_uuid().into_bytes(),
            SequenceScope::Store(id) => id.as_uuid().into_bytes(),
        }
    }

    /// `0005`'s composite `CHECK`, restated so the refusal can be named.
    ///
    /// Kept as one expression mirroring the SQL rather than a lookup table: the
    /// two must not drift, and a reader comparing them should be comparing like
    /// with like.
    const fn permits(self, kind: SeqKind) -> bool {
        matches!(
            (self, kind),
            (
                SequenceScope::Register(_),
                SeqKind::Receipt | SeqKind::ZReport
            ) | (SequenceScope::Store(_), SeqKind::FiscalIcv)
        )
    }
}

/// Reads and advances the register's owned counters.
pub struct SequenceRepository<'c> {
    conn: &'c Connection,
}

impl<'c> SequenceRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Take the next number for `scope`/`kind`, inside the caller's transaction.
    ///
    /// The allocation is one statement. That is not only for speed: a
    /// `SELECT` followed by an `UPDATE` leaves a window in which a second
    /// writer reads the same value, and closing it in SQL is cheaper than
    /// reasoning about it in Rust. A caller that already holds read locks and
    /// expects contention should open its transaction with
    /// `TransactionBehavior::Immediate` — this module cannot choose, because
    /// `repo/mod.rs` hands the boundary to the caller.
    pub fn next(
        &self,
        tx: &Transaction<'_>,
        scope: SequenceScope,
        kind: SeqKind,
    ) -> Result<u64, DbError> {
        if kind == SeqKind::FiscalIcv {
            return Err(DbError::SequenceNotYetAllocatable {
                kind: kind.as_str(),
            });
        }
        self.check_scope(scope, kind)?;

        // `RETURNING` gives the value the same statement wrote, so no second
        // read can observe a different one. The insert branch seeds
        // `next_value = 2` and returns 1, which is why no migration seed row is
        // needed; the update branch advances by exactly one, which is the only
        // step `doc_sequence_monotonic` accepts.
        let issued: i64 = tx.query_row(
            "INSERT INTO doc_sequence (scope_kind, scope_id, kind, next_value)
             VALUES (?1, ?2, ?3, 2)
             ON CONFLICT (scope_kind, scope_id, kind)
             DO UPDATE SET next_value = next_value + 1
             RETURNING next_value - 1",
            params![scope.as_str(), scope.id_bytes(), kind.as_str()],
            |row| row.get(0),
        )?;

        u64::try_from(issued).map_err(|_| DbError::SequenceValueInvalid { found: issued })
    }

    /// The numbers this counter issued that no document carries.
    ///
    /// The counter alone cannot answer this. `doc_sequence` stores a high-water
    /// mark, and nothing in the schema couples a bump to the document it
    /// numbered, so the evidence has to come from the documents — and today it
    /// exists for exactly one kind.
    ///
    /// * **`Receipt`** is answerable: `sale.receipt_number` carries it.
    /// * **`ZReport`** has no table to read. `z_report` arrives with the Z
    ///   lifecycle in Phase 2.
    /// * **`FiscalIcv`** is unreachable while [`Self::next`] refuses the kind.
    ///
    /// For the two that cannot be answered this returns an **error**, not an
    /// empty vector — except where nothing was ever allocated, which is the one
    /// case where "no gaps" is true rather than merely unknown. An unconditional
    /// `Ok(vec![])` would read as "this counter is sound" to every caller and to
    /// every test, which is the absent-proof-behind-a-green-result this
    /// repository's gates exist to refuse.
    pub fn gaps(&self, scope: SequenceScope, kind: SeqKind) -> Result<Vec<u64>, DbError> {
        self.check_scope(scope, kind)?;
        let (next_value, prefix) = match self.row(scope, kind)? {
            // Never allocated, so nothing can be missing.
            None => return Ok(Vec::new()),
            Some(row) => row,
        };
        let allocated = u64::try_from(next_value - 1)
            .map_err(|_| DbError::SequenceValueInvalid { found: next_value })?;
        if allocated == 0 {
            return Ok(Vec::new());
        }
        if kind != SeqKind::Receipt {
            return Err(DbError::SequenceEvidenceUnavailable {
                kind: kind.as_str(),
            });
        }

        let observed = self.receipt_numbers(scope, &prefix)?;
        Ok((1..=allocated).filter(|n| !observed.contains(n)).collect())
    }

    /// The row for this counter, if it has ever been advanced.
    fn row(&self, scope: SequenceScope, kind: SeqKind) -> Result<Option<(i64, String)>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT next_value, prefix FROM doc_sequence
                  WHERE scope_kind = ?1 AND scope_id = ?2 AND kind = ?3",
                params![scope.as_str(), scope.id_bytes(), kind.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    /// Every receipt number this register has actually issued, as an integer.
    ///
    /// `receipt_number` is `TEXT` with no enforced format — `1.9.3` owns the
    /// `REG01-000123` shape and this module owns the `prefix` it is built from.
    /// A row that does not parse is a **named error rather than a skipped row**:
    /// skipping it manufactures a gap that is not there, and the whole point of
    /// this function is that an auditor believes its answer.
    fn receipt_numbers(&self, scope: SequenceScope, prefix: &str) -> Result<Vec<u64>, DbError> {
        let mut statement = self
            .conn
            .prepare("SELECT receipt_number FROM sale WHERE register_id = ?1")?;
        let rows = statement.query_map(params![scope.id_bytes()], |row| row.get::<_, String>(0))?;

        let mut issued = Vec::new();
        for row in rows {
            let number = row?;
            let tail =
                number
                    .strip_prefix(prefix)
                    .ok_or_else(|| DbError::SequenceNumberUnreadable {
                        found: number.clone(),
                    })?;
            issued.push(
                tail.parse::<u64>()
                    .map_err(|_| DbError::SequenceNumberUnreadable {
                        found: number.clone(),
                    })?,
            );
        }
        Ok(issued)
    }

    fn check_scope(&self, scope: SequenceScope, kind: SeqKind) -> Result<(), DbError> {
        if scope.permits(kind) {
            return Ok(());
        }
        Err(DbError::SequenceScopeInvalid {
            scope_kind: scope.as_str(),
            kind: kind.as_str(),
        })
    }
}
