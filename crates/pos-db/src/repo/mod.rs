//! Repositories — the storage seam (conventions §3).
//!
//! `pos-domain` knows nothing about storage and `pos-db` knows nothing about
//! business rules, so everything in here reads and writes rows and computes no
//! total, tax or discount. Two rules hold across every module below:
//!
//! * a repository is a struct holding `&Connection` and returns owned values
//!   and [`crate::DbError`] — never a `rusqlite::Row`, never a
//!   `rusqlite::Error`;
//! * **every write that produces a fact takes an explicit `&Transaction`**, so
//!   the caller — never the repository — decides where the transaction begins
//!   and ends. That is how I-9 stays true: a sale's facts, its `sync_commit`,
//!   its complete `fact_commit_member` manifest and its `sync_outbox` delivery
//!   rows reach disk in one `BEGIN`/`COMMIT` or not at all.
//!
//! This module arrived with microstep 1.8.9, the outbox writer, because the
//! commit-manifest writer is scheduled before any repository that appends a
//! fact.

pub mod approval;
pub mod outbox;
