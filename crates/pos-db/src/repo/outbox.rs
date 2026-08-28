//! The transactional outbox: one business transaction, one delivery envelope
//! (I-9).
//!
//! Every local business transaction inserts one immutable `sync_commit`, the
//! complete permanent `fact_commit_member` manifest, and one `sync_outbox`
//! delivery row per member — beside the facts themselves, in the same SQLite
//! transaction. [`ref/sync-protocol.md`] §2 owns the protocol; this module owns
//! the write. Four rules it exists to keep:
//!
//! * **The caller owns the transaction.** [`OutboxRepository::write_commit`]
//!   takes an explicit `&Transaction` and never opens a connection, begins or
//!   commits (conventions §3). A sale without a manifest would be partial
//!   history; a delivery row without its member would be a phantom.
//! * **A header never survives without its lines.** The manifest is proved
//!   whole through the schema's own `sync_commit_ready` view *before* this
//!   writer reports success, so a partial envelope fails the business
//!   transaction rather than reaching the server as a sale header with no
//!   lines.
//! * **Ordering is owned, never observed.** Members are ordered by
//!   `commit_index`, assigned from the caller's slice position, and delivery is
//!   ordered by `sync_outbox.seq`, an `INTEGER PRIMARY KEY AUTOINCREMENT`.
//!   Nothing here sorts by a timestamp (I-7).
//! * **Nothing here reads a clock, mints an id, or logs.** Time and ids arrive
//!   as arguments (I-7, I-8). Payloads carry customer data, so this module
//!   emits no `tracing` output and puts no payload, id or hash input into an
//!   error (`.claude/rules/security.md`).
//!
//! No pusher exists yet — it is Phase 3 work — but complete envelopes
//! accumulate from the first sale, which is the point: the queue a pusher will
//! read must never contain a commit written before the envelope was versioned.
//!
//! **Canonical payload bytes are the caller's.** The shared canonical
//! projection named in [`ref/sync-protocol.md`] §"The canonical dump"
//! (`crates/pos-sync/src/canonical.rs`) arrives with the sync engine, and
//! `pos-db` does not depend on `pos-sync`. Until then a caller hands this
//! writer a payload it has already serialized canonically — sorted keys, no
//! whitespace, UTF-8 — and the writer hashes exactly the bytes it stores, so
//! `payload_hash` cannot describe anything other than the `payload` column
//! beside it.
//!
//! [`ref/sync-protocol.md`]: ../../../../docs/implementation/ref/sync-protocol.md

use rusqlite::{Connection, Transaction, params};

use crate::DbError;

/// Ids are `BLOB(16)` — a UUIDv7's bytes (conventions §2, I-7). Taking the
/// array rather than a slice makes "an id is sixteen bytes" a compile-time
/// fact, so no caller can write a truncated foreign key into the manifest.
const ID_BYTES: usize = 16;

/// `fact_commit_member.op` — the only operation a fact may carry.
///
/// Facts are inserted, never upserted: an upsert overwrites an immutable sale,
/// which is I-4 broken by the transport ([`ref/sync-protocol.md`] §2, "`INSERT`,
/// not upsert"). The schema agrees — `CHECK (op = 'insert')` — and the value is
/// bound into the commit hash, so a rewrite of it on the wire changes the
/// digest the server checks.
///
/// [`ref/sync-protocol.md`]: ../../../../docs/implementation/ref/sync-protocol.md
const MEMBER_OP: &str = "insert";

/// The delivery state a member is born in. `sync_outbox.state` is the
/// transport's truth from here on — `pending` → `in_flight` → `acknowledged`,
/// with `retry` and `dead` — and this writer never touches it again.
const INITIAL_DELIVERY_STATE: &str = "pending";

/// Version byte of the canonical commit encoding in [`canonical_commit_bytes`].
///
/// Every historical `commit_hash` was produced by exactly one layout, so a
/// change to that layout is a bump here and a new golden, never a silent edit.
const CANONICAL_COMMIT_VERSION: u8 = 1;

/// Domain separator, so a digest computed here can never be replayed as an
/// audit-chain entry hash or a prepared-intent `content_hash`. Same shape as
/// the prepared-intent separator in `ref/domain-api.md` §9: a NUL-terminated
/// ASCII tag naming the thing being hashed.
const COMMIT_DOMAIN_SEPARATOR: &[u8] = b"pos-sync-commit\0";

/// The header of one business transaction's delivery envelope.
///
/// `commit_size`, `commit_hash` and the member rows are the writer's to
/// compute; everything a caller must decide is here, and every field of it is
/// an argument rather than something the writer observes.
#[derive(Debug, Clone, Copy)]
pub struct CommitEnvelope<'a> {
    /// Identity of this commit. A UUIDv7's bytes: index locality and a device
    /// timestamp, never causal order (I-7).
    pub commit_id: &'a [u8; ID_BYTES],
    /// The wire shape this envelope was produced for
    /// ([`ref/sync-protocol.md`] §2, "Envelope versioning").
    ///
    /// [`ref/sync-protocol.md`]: ../../../../docs/implementation/ref/sync-protocol.md
    pub protocol_version: i64,
    /// The migration `user_version` the payloads were produced by — normally
    /// [`crate::SCHEMA_VERSION`].
    pub schema_version: i64,
    /// The build that produced the payloads.
    pub producer_version: &'a str,
    /// ISO-8601 UTC with milliseconds, supplied by the shell's clock rather
    /// than read here (I-8). One value for the header and every member: they
    /// are one business transaction, and the writer must not be able to claim
    /// otherwise.
    pub created_at: &'a str,
}

/// One constituent fact of a business transaction, as it travels.
#[derive(Debug, Clone, Copy)]
pub struct FactMember<'a> {
    /// Identity of this change on the wire.
    pub change_id: &'a [u8; ID_BYTES],
    /// The fact table this row belongs to, spelled exactly as `ref/schema.md`'s
    /// `<!-- fact-tables: … -->` marker spells it — `sale`, `sale_line`,
    /// `stock_ledger`.
    pub entity: &'a str,
    /// The fact's own primary key.
    pub entity_id: &'a [u8; ID_BYTES],
    /// The canonical serialization of the fact: sorted keys, no whitespace,
    /// UTF-8 ([`ref/sync-protocol.md`] §2, "`INSERT`, not upsert"). `&str` is
    /// UTF-8 by construction; the other two are the caller's to hold, and the
    /// stored `payload_hash` is taken over exactly these bytes.
    ///
    /// [`ref/sync-protocol.md`]: ../../../../docs/implementation/ref/sync-protocol.md
    pub payload: &'a str,
}

/// What the writer computed, for a caller that wants to assert on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReceipt {
    /// Lower-case hex of the BLAKE3 digest over [`canonical_commit_bytes`],
    /// exactly as stored in `sync_commit.commit_hash`.
    pub commit_hash: String,
    /// How many facts this envelope claims. Equal to the number of members
    /// written, and to `sync_commit.commit_size`.
    pub commit_size: i64,
}

/// One permanent manifest row, read back.
///
/// Delivery rows may be pruned once durably acknowledged; these rows may not.
/// They are the financial evidence of what the register committed, and the
/// convergence oracle the server reconciles against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub change_id: [u8; ID_BYTES],
    pub commit_index: i64,
    pub entity: String,
    pub entity_id: [u8; ID_BYTES],
    pub op: String,
    pub payload: String,
    pub payload_hash: String,
    pub created_at: String,
}

/// A member with its payload digest, paired once so the digest can never drift
/// away from the member it belongs to between the hash and the `INSERT`.
struct PreparedMember<'a> {
    member: &'a FactMember<'a>,
    payload_digest: blake3::Hash,
}

/// Reads and writes of the durable delivery envelope.
pub struct OutboxRepository<'c> {
    conn: &'c Connection,
}

impl<'c> OutboxRepository<'c> {
    #[must_use]
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Write one commit envelope: the immutable `sync_commit`, the complete
    /// `fact_commit_member` manifest, and one `sync_outbox` delivery row per
    /// member.
    ///
    /// The caller passes the transaction its facts were written in, and passes
    /// `members` in the order the server must apply them — parents before
    /// children within the group ([`ref/sync-protocol.md`] §2, rule 1). That
    /// position becomes `commit_index`.
    ///
    /// Reads in this repository go through the connection it holds; this write
    /// goes through the caller's transaction, and nothing here commits it. On
    /// any `Err` the caller rolls back, and the facts roll back with the
    /// envelope.
    ///
    /// # Errors
    ///
    /// [`DbError::EmptyCommitRefused`] when `members` is empty — an envelope
    /// with nothing in it is a header with no lines, which is the failure I-9
    /// exists to prevent, and `commit_size` has `CHECK (commit_size > 0)` to
    /// say so.
    ///
    /// [`DbError::CommitEnvelopeIncomplete`] when the schema's
    /// `sync_commit_ready` view does not recognise the envelope after it is
    /// written. That is a bug in this writer rather than a caller error, and it
    /// fails the business transaction on purpose.
    ///
    /// [`DbError::Sqlite`] for a duplicate `change_id`, a second commit
    /// claiming the same `(entity, entity_id)`, or any other constraint the
    /// schema refuses.
    ///
    /// [`ref/sync-protocol.md`]: ../../../../docs/implementation/ref/sync-protocol.md
    pub fn write_commit(
        &self,
        tx: &Transaction<'_>,
        envelope: &CommitEnvelope<'_>,
        members: &[FactMember<'_>],
    ) -> Result<CommitReceipt, DbError> {
        if members.is_empty() {
            return Err(DbError::EmptyCommitRefused);
        }
        let commit_size = members.len() as i64;

        // Each payload is hashed exactly as it will be stored, so the column
        // and its digest cannot describe different bytes.
        let prepared: Vec<PreparedMember<'_>> = members
            .iter()
            .map(|member| PreparedMember {
                member,
                payload_digest: blake3::hash(member.payload.as_bytes()),
            })
            .collect();
        let commit_hash = blake3::hash(&canonical_commit_bytes(commit_size, &prepared))
            .to_hex()
            .to_string();

        tx.execute(
            "INSERT INTO sync_commit
               (id, commit_size, commit_hash, protocol_version, schema_version,
                producer_version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                envelope.commit_id.as_slice(),
                commit_size,
                commit_hash,
                envelope.protocol_version,
                envelope.schema_version,
                envelope.producer_version,
                envelope.created_at,
            ],
        )?;

        for (index, prepared_member) in prepared.iter().enumerate() {
            let member = prepared_member.member;
            let payload_hash = prepared_member.payload_digest.to_hex();
            tx.execute(
                "INSERT INTO fact_commit_member
                   (change_id, commit_id, commit_index, entity, entity_id, op,
                    payload, payload_hash, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    member.change_id.as_slice(),
                    envelope.commit_id.as_slice(),
                    index as i64,
                    member.entity,
                    member.entity_id.as_slice(),
                    MEMBER_OP,
                    member.payload,
                    payload_hash.as_str(),
                    envelope.created_at,
                ],
            )?;
            // `seq` is the autoincrement the pusher will order by; `state` and
            // `attempts` carry their schema defaults, written here so a reader
            // of this code can see what a fresh row means. `created_at` is
            // passed rather than defaulted, because the column's default is
            // `strftime('now')` and nothing in a fact commit may come from a
            // device clock (I-7).
            tx.execute(
                "INSERT INTO sync_outbox (change_id, state, attempts, created_at)
                 VALUES (?1, ?2, 0, ?3)",
                params![
                    member.change_id.as_slice(),
                    INITIAL_DELIVERY_STATE,
                    envelope.created_at,
                ],
            )?;
        }

        // I-9, asked rather than assumed. `sync_commit_ready` is checkout's
        // stricter view: every member present, indexed `0..commit_size`, and a
        // delivery row for each. Asking it here — inside the caller's
        // transaction, before returning success — is what turns "this writer
        // intends to write a whole envelope" into "an incomplete envelope
        // cannot be committed".
        if count_ready(tx, envelope.commit_id)? != 1 {
            return Err(DbError::CommitEnvelopeIncomplete {
                commit_size,
                members: count_members(tx, envelope.commit_id)?,
                delivery_rows: count_delivery_rows(tx, envelope.commit_id)?,
            });
        }

        Ok(CommitReceipt {
            commit_hash,
            commit_size,
        })
    }

    /// The permanent manifest of one commit, in `commit_index` order.
    ///
    /// Unaffected by delivery-row pruning: `fact_commit_member` is the record
    /// of what the register committed, and the schema refuses to update or
    /// delete a row of it.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] if the read fails, or
    /// [`DbError::IdWidthInvalid`] if a stored id is not `BLOB(16)`.
    pub fn manifest(&self, commit_id: &[u8; ID_BYTES]) -> Result<Vec<ManifestEntry>, DbError> {
        let mut statement = self.conn.prepare(
            "SELECT change_id, commit_index, entity, entity_id, op, payload,
                    payload_hash, created_at
               FROM fact_commit_member
              WHERE commit_id = ?1
              ORDER BY commit_index",
        )?;
        let rows = statement.query_map([commit_id.as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;

        let mut manifest = Vec::new();
        for row in rows {
            let (change_id, commit_index, entity, entity_id, op, payload, payload_hash, created_at) =
                row?;
            manifest.push(ManifestEntry {
                change_id: id_bytes("change_id", change_id)?,
                commit_index,
                entity,
                entity_id: id_bytes("entity_id", entity_id)?,
                op,
                payload,
                payload_hash,
                created_at,
            });
        }
        Ok(manifest)
    }

    /// Whether the permanent manifest of this commit is whole — every member
    /// present and indexed `0..commit_size`.
    ///
    /// This stays true for the life of the database. It is what survives
    /// pruning, and what lets the server validate a whole sale graph as one
    /// atomic commit.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] if the read fails.
    pub fn is_complete(&self, commit_id: &[u8; ID_BYTES]) -> Result<bool, DbError> {
        Ok(count_complete(self.conn, commit_id)? == 1)
    }

    /// Whether this commit is also deliverable — every member still has its
    /// `sync_outbox` row.
    ///
    /// False after acknowledged delivery rows are pruned, which is ordinary
    /// queue retention rather than a defect. [`Self::is_complete`] is the
    /// question to ask about history.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] if the read fails.
    pub fn is_ready(&self, commit_id: &[u8; ID_BYTES]) -> Result<bool, DbError> {
        Ok(count_ready(self.conn, commit_id)? == 1)
    }
}

/// The bytes hashed into `sync_commit.commit_hash`.
///
/// [`ref/sync-protocol.md`] line 84 pins the algorithm and the subject —
/// `"<blake3 of the commit's canonical members>"` — and rule 2 of "The commit
/// group" pins its job: a commit "that is the right size and the wrong
/// contents" must be caught by the hash. The reference does not pin a byte
/// layout, so this is the layout, and it is the one this repository already
/// uses for domain-separated digests (`ref/schema.md` §`product_quick_add_request`,
/// `ref/domain-api.md` §9): a version byte, a domain separator, then
/// length-prefixed canonical encodings, big-endian throughout.
///
/// ```text
/// version byte                    1 byte    CANONICAL_COMMIT_VERSION
/// domain separator               16 bytes   "pos-sync-commit\0"
/// commit_size                     8 bytes   i64 big-endian
/// then, per member, in ascending commit_index order:
///   commit_index                  8 bytes   i64 big-endian
///   change_id                     4 + 16    u32 big-endian length, then bytes
///   entity                        4 + n     u32 big-endian length, then UTF-8
///   entity_id                     4 + 16    u32 big-endian length, then bytes
///   op                            4 + 6     u32 big-endian length, then UTF-8
///   payload digest               32 bytes   BLAKE3 of the payload bytes
/// ```
///
/// The payload is bound by its digest rather than by its bytes because that is
/// how the protocol already compares it: `payload_hash` travels with the change
/// so the cheap path is a hash comparison, and canonical bytes are read only on
/// a mismatch ([`ref/sync-protocol.md`] §2, "`INSERT`, not upsert").
///
/// **Deliberately absent: every header field.** Not `commit_id`, not
/// `protocol_version` or `schema_version`, not `producer_version`, not
/// `created_at`. The reference says *members*, and this digest answers exactly
/// one question — are these the same facts, in the same order? Binding
/// `created_at` or `producer_version` would make two registers that produced
/// the same fact graph disagree about it, and each header field is validated on
/// its own terms (§2, "Envelope versioning and compatibility"). `commit_size`
/// *is* bound, even though it equals the member count, so that the size a
/// header claims is inside the digest rather than only beside it.
///
/// [`ref/sync-protocol.md`]: ../../../../docs/implementation/ref/sync-protocol.md
fn canonical_commit_bytes(commit_size: i64, prepared: &[PreparedMember<'_>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.push(CANONICAL_COMMIT_VERSION);
    bytes.extend_from_slice(COMMIT_DOMAIN_SEPARATOR);
    bytes.extend_from_slice(&commit_size.to_be_bytes());
    for (index, prepared_member) in prepared.iter().enumerate() {
        let member = prepared_member.member;
        bytes.extend_from_slice(&(index as i64).to_be_bytes());
        push_field(&mut bytes, member.change_id.as_slice());
        push_field(&mut bytes, member.entity.as_bytes());
        push_field(&mut bytes, member.entity_id.as_slice());
        push_field(&mut bytes, MEMBER_OP.as_bytes());
        bytes.extend_from_slice(prepared_member.payload_digest.as_bytes());
    }
    bytes
}

/// One field, length-prefixed, so that `("ab", "c")` and `("a", "bc")` cannot
/// hash to the same digest.
///
/// Every field prefixed here is a sixteen-byte id, a table name or the word
/// `insert`, so the `u32` prefix cannot overflow. The payload — the only
/// unbounded value in a commit — is bound by its fixed 32-byte digest instead.
fn push_field(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
    bytes.extend_from_slice(value);
}

/// `1` when the whole manifest is present and each member has a delivery row.
fn count_ready(conn: &Connection, commit_id: &[u8; ID_BYTES]) -> Result<i64, DbError> {
    Ok(conn.query_row(
        "SELECT count(*) FROM sync_commit_ready WHERE id = ?1",
        [commit_id.as_slice()],
        |row| row.get(0),
    )?)
}

/// `1` when the whole manifest is present, whatever became of delivery.
fn count_complete(conn: &Connection, commit_id: &[u8; ID_BYTES]) -> Result<i64, DbError> {
    Ok(conn.query_row(
        "SELECT count(*) FROM fact_commit_complete WHERE id = ?1",
        [commit_id.as_slice()],
        |row| row.get(0),
    )?)
}

fn count_members(conn: &Connection, commit_id: &[u8; ID_BYTES]) -> Result<i64, DbError> {
    Ok(conn.query_row(
        "SELECT count(*) FROM fact_commit_member WHERE commit_id = ?1",
        [commit_id.as_slice()],
        |row| row.get(0),
    )?)
}

fn count_delivery_rows(conn: &Connection, commit_id: &[u8; ID_BYTES]) -> Result<i64, DbError> {
    Ok(conn.query_row(
        "SELECT count(*)
           FROM sync_outbox o
           JOIN fact_commit_member m ON m.change_id = o.change_id
          WHERE m.commit_id = ?1",
        [commit_id.as_slice()],
        |row| row.get(0),
    )?)
}

/// A stored id, which this writer only ever wrote as sixteen bytes.
fn id_bytes(column: &'static str, value: Vec<u8>) -> Result<[u8; ID_BYTES], DbError> {
    let found = value.len();
    <[u8; ID_BYTES]>::try_from(value).map_err(|_| DbError::IdWidthInvalid {
        table: "fact_commit_member",
        column,
        found,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::{
        CANONICAL_COMMIT_VERSION, COMMIT_DOMAIN_SEPARATOR, FactMember, PreparedMember,
        canonical_commit_bytes,
    };

    static SALE: [u8; 16] = [0x11; 16];
    static LINE: [u8; 16] = [0x22; 16];
    static CHANGE_A: [u8; 16] = [0xa1; 16];
    static CHANGE_B: [u8; 16] = [0xb2; 16];
    static OTHER_ID: [u8; 16] = [0x33; 16];
    static OTHER_CHANGE: [u8; 16] = [0xc3; 16];

    const SALE_PAYLOAD: &str = r#"{"id":"sale","total_minor":2900}"#;
    const LINE_PAYLOAD: &str = r#"{"id":"line","total_minor":2500}"#;

    fn member<'a>(
        change_id: &'a [u8; 16],
        entity: &'a str,
        entity_id: &'a [u8; 16],
        payload: &'a str,
    ) -> FactMember<'a> {
        FactMember {
            change_id,
            entity,
            entity_id,
            payload,
        }
    }

    fn digest(members: &[FactMember<'_>]) -> String {
        let prepared: Vec<PreparedMember<'_>> = members
            .iter()
            .map(|member| PreparedMember {
                member,
                payload_digest: blake3::hash(member.payload.as_bytes()),
            })
            .collect();
        blake3::hash(&canonical_commit_bytes(members.len() as i64, &prepared))
            .to_hex()
            .to_string()
    }

    fn two_members() -> [FactMember<'static>; 2] {
        [
            member(&CHANGE_A, "sale", &SALE, SALE_PAYLOAD),
            member(&CHANGE_B, "sale_line", &LINE, LINE_PAYLOAD),
        ]
    }

    #[test]
    fn the_same_members_always_produce_the_same_commit_hash() {
        assert_eq!(digest(&two_members()), digest(&two_members()));
    }

    #[test]
    fn a_commit_hash_is_sixty_four_lower_case_hex_characters() {
        let hash = digest(&two_members());
        assert_eq!(hash.len(), 64, "BLAKE3 is 32 bytes, hex is 64 characters");
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "the column is TEXT and stores lower-case hex"
        );
    }

    /// Every field the server would use to decide "same facts?" must move the
    /// digest. A field that does not is a field a corrupted or malicious change
    /// could alter without the hash noticing.
    #[test]
    fn changing_any_bound_member_field_changes_the_commit_hash() {
        let baseline = digest(&two_members());
        let [sale_member, line_member] = two_members();

        let variants = [
            // a different payload, same everything else
            [
                member(&CHANGE_A, "sale", &SALE, r#"{"id":"sale","total_minor":1}"#),
                line_member,
            ],
            // a different entity
            [
                member(&CHANGE_A, "sale_tender", &SALE, SALE_PAYLOAD),
                line_member,
            ],
            // a different entity id
            [
                member(&CHANGE_A, "sale", &OTHER_ID, SALE_PAYLOAD),
                line_member,
            ],
            // a different change id
            [
                member(&OTHER_CHANGE, "sale", &SALE, SALE_PAYLOAD),
                line_member,
            ],
            // the same members, applied in the other order
            [line_member, sale_member],
        ];

        for variant in &variants {
            assert_ne!(
                digest(variant),
                baseline,
                "a bound member field changed and the commit hash did not"
            );
        }
    }

    /// A truncated commit is a different commit, and rule 2 of the commit group
    /// wants the hash — not the count — to say so.
    #[test]
    fn a_shorter_commit_hashes_differently() {
        let members = two_members();
        let [sale_member, _] = members;
        assert_ne!(digest(&members), digest(&[sale_member]));
    }

    /// The digest is domain-separated and versioned, so it can never be
    /// replayed as an audit-chain or prepared-intent digest, and a future
    /// layout change is visible rather than silent.
    #[test]
    fn canonical_bytes_open_with_the_version_byte_and_the_domain_separator() {
        let members = two_members();
        let prepared: Vec<PreparedMember<'_>> = members
            .iter()
            .map(|member| PreparedMember {
                member,
                payload_digest: blake3::hash(member.payload.as_bytes()),
            })
            .collect();
        let bytes = canonical_commit_bytes(2, &prepared);

        let mut expected = vec![CANONICAL_COMMIT_VERSION];
        expected.extend_from_slice(COMMIT_DOMAIN_SEPARATOR);
        expected.extend_from_slice(&2_i64.to_be_bytes());
        assert!(
            bytes.starts_with(&expected),
            "canonical commit bytes must open with the version byte, the domain \
             separator and the commit size"
        );
    }
}
