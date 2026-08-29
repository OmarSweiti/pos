//! The audit hash chain: tamper *evidence* for `audit_log`, and the sentence
//! that keeps it evidence rather than a claim.
//!
//! Every audit row carries `prev_hash` and `hash`. `hash` is
//! `BLAKE3(prev_hash ‖ canonical_bytes(entry))`, the first row chains from
//! [`GENESIS`], and [`verify_chain`] walks the whole chain and reports the
//! **first** place the arithmetic stops agreeing
//! ([`ref/security-compliance.md`] §4).
//!
//! # What is hashed, and why it is the row rather than the intent
//!
//! [`AuditIntent`] is what a pure domain function returns — actor, approver,
//! action, entity, payload. Hashing only that leaves `audit_log.id`,
//! `audit_log.seq` and `audit_log.register_id` outside the chain, and those
//! three are precisely the columns an insider would rewrite: move a refund to
//! another register, renumber it, or re-point its identity, and every hash still
//! verifies because none of it was ever hashed. [`CanonicalAuditEntry`] binds
//! the persisted row — identity columns included — plus a domain tag and a
//! chain version, so a digest computed here can never be replayed into another
//! chain and a future serialization change has to announce itself.
//!
//! # What the chain proves — and what it does not
//!
//! This is the part that was overclaimed once, and the mechanism is worth
//! keeping precisely because the honest version is still valuable.
//!
//! BLAKE3 is unkeyed and the chain lives in a database whose key the merchant
//! holds. Anyone with that key can edit a row and recompute every hash after it,
//! or delete the newest rows and leave a shorter chain that verifies perfectly.
//! **A local hash chain cannot detect deletion of its own tail** — a truncation
//! removes the evidence of itself. A keyed hash would not help: the key would
//! have to live on the same machine, in the same custody.
//!
//! Without an external anchor the chain proves that no row was changed,
//! reordered or removed *by anything that does not recompute the chain* — which
//! covers every bug, every crash, every partial write and every casual edit
//! through a SQL console — and it proves **where** the first inconsistency is,
//! which is what a forensic investigation needs to start. It does not prove that
//! a determined holder of the database key did not rewrite history.
//!
//! That is why [`verify_chain`] takes a [`ChainAnchor`]: the head `(seq, hash)`
//! written somewhere the register does not own it. Every Z close and every
//! verified backup exports the head in Phase 1, and from Phase 3 the server
//! stores it too. Deletion at or below the last anchor is then
//! [`ChainVerdict::Truncated`] or [`ChainVerdict::Broken`], and everything above
//! it is reported as [`ChainVerdict::IntactUnanchoredFrom`] rather than counted
//! as verified. `prop_chain_detects_tail_deletion_against_an_anchor` is the test
//! that proves the anchor is what closes the hole, and it asserts both halves:
//! the same truncated chain is *not* detectable without one.
//!
//! **Residual risk, disclosed.** Entries written after the last anchor — at most
//! one shift's worth, or one backup interval — remain deletable without
//! detection on a register whose database key the attacker holds. Shortening
//! that window is what anchoring at Z and at backup is for; closing it entirely
//! needs the Phase-3 server head.
//!
//! # What this module may not carry
//!
//! An audit row names a cashier, an approver and a document. It must never carry
//! a value under a sensitive field name — `audit_log.payload`'s own DDL says
//! "NEVER PII or card data" and [`ref/security-compliance.md`] §6 owns the
//! canonical registry of those names. This module deliberately does **not**
//! restate that list: microstep 1.6.8 defines `SENSITIVE_FIELD_RULES` once and
//! every scrubber, assertion and fixture iterates it, and a second
//! hand-maintained copy here is exactly how the two come to disagree. Nothing in
//! this file logs, and no error it returns carries a payload value, an id or a
//! hash input.
//!
//! # Purity
//!
//! Nothing here reads a clock, mints an id or performs I/O (I-8). `register_id`,
//! `seq`, `id` and every timestamp are **arguments**, supplied by the shell that
//! wrote the row.
//!
//! [`ref/security-compliance.md`]: ../../../docs/implementation/ref/security-compliance.md

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::ids::{ApprovalId, RegisterId, UserId};
use crate::time::Timestamp;

/// The domain tag bound into every audit digest.
///
/// A hash is only meaningful inside the context it was computed for. Binding the
/// tag means a digest produced here can never be presented as a `sync_commit`
/// commit hash or a prepared-intent `content_hash`, and vice versa
/// ([`ref/security-compliance.md`] §4).
///
/// [`ref/security-compliance.md`]: ../../../docs/implementation/ref/security-compliance.md
pub const DOMAIN: &str = "pos.audit";

/// The canonical-serialization version, and `audit_log.canonical_version`.
///
/// Adding a field to [`CanonicalAuditEntry`], or changing how one is written, is
/// a bump here plus a new golden — never a silent edit. Every historical hash
/// was produced by exactly one layout, and the version is what lets a verifier
/// know which.
pub const VERSION: u16 = 1;

/// The chain's zero: what the first entry's `prev_hash` is.
pub const GENESIS: [u8; 32] = [0u8; 32];

/// How deeply a payload may nest before [`check_payload`] refuses it.
///
/// `serde_json`'s own parser stops at 128 levels, so a payload that arrived as
/// text can never exceed this; the limit exists for one built programmatically,
/// because [`canonical_bytes`] walks a payload's structure recursively and a
/// register must not be able to store a row that later overflows the stack of
/// the forensic tool sent to read it.
pub const MAX_PAYLOAD_DEPTH: usize = 128;

/// Why a payload cannot be chained as written.
///
/// Neither variant carries a value out of the payload. An audit payload must not
/// contain PII or card data in the first place, and an error type is not the
/// place to test that assumption (`.claude/rules/security.md`); the JSON pointer
/// names the position, which is what a caller needs to fix it.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuditError {
    /// A number in the payload is not an integer.
    ///
    /// Canonical audit bytes carry "integers as integers, never floats"
    /// ([`ref/security-compliance.md`] §4), and `approval_consumption`'s own
    /// trigger requires `json_type(payload, '$.amount_minor') = 'integer'`. A
    /// float in a money path is I-1 broken one layer down, and a float in a
    /// hashed document is a byte-stability question nobody wants to answer in a
    /// dispute.
    ///
    /// [`ref/security-compliance.md`]: ../../../docs/implementation/ref/security-compliance.md
    #[error("audit payload at {pointer} holds a number that is not an integer")]
    PayloadNumberIsNotAnInteger {
        /// RFC 6901 JSON pointer to the offending position — key names and array
        /// indexes only, never a value.
        pointer: String,
    },
    /// The payload nests deeper than [`MAX_PAYLOAD_DEPTH`].
    #[error("audit payload nests deeper than the {limit}-level canonical limit")]
    PayloadNestedTooDeeply {
        /// [`MAX_PAYLOAD_DEPTH`], restated so the message is self-contained.
        limit: usize,
    },
}

/// What a pure domain function returns when an operation must be audited.
///
/// A pure function cannot write an audit row — that is I/O. It returns the
/// *intent*, and the shell persists it inside the same transaction as the effect
/// ([`ref/domain-api.md`] §6.5, §9). That is how "every action that reverses
/// money writes the audit log" becomes structural rather than remembered.
///
/// `action` and `entity` are `&'static str`, which makes the derived
/// `Deserialize` carry an implicit `'de: 'static` bound. That costs nothing
/// here: a stored row is rebuilt from `audit_log`'s columns, not parsed back out
/// of JSON.
///
/// [`ref/domain-api.md`]: ../../../docs/implementation/ref/domain-api.md
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditIntent {
    /// Who performed the action.
    pub actor: UserId,
    /// Who approved it, when it needed approval. Always a different person from
    /// `actor` on every escalation path (E.52).
    pub approver: Option<UserId>,
    /// Which approval paid for this ([`ref/domain-api.md`] §8.1), so the audit
    /// row and the one-use `ApprovalHandle` name each other.
    ///
    /// [`ref/domain-api.md`]: ../../../docs/implementation/ref/domain-api.md
    pub approval: Option<ApprovalId>,
    /// `"sale.void"`, `"price.override"` — the capability spelling, which is
    /// what `approval_consumption`'s trigger compares against
    /// `approval_handle.capability`.
    pub action: &'static str,
    /// The table the action happened to: `"sale"`, `"shift"`, `"user"`.
    pub entity: &'static str,
    /// Which row of it.
    pub entity_id: Uuid,
    /// The operator's stated reason, where one is required.
    pub reason: Option<String>,
    /// The action's own facts — amounts, counts, codes. **Never PII, never card
    /// data** (conventions §12); see the module documentation for why the
    /// sensitive-name registry is not restated here.
    pub payload: Value,
    /// When the action happened, supplied by the shell (I-8).
    pub at: Timestamp,
}

/// What is actually hashed: the intent **plus** the identity the row is stored
/// under, plus the tag and version that place the digest in one context.
///
/// Hashing the intent alone leaves `id`, `seq` and `register_id` outside the
/// chain, so all three could be rewritten without breaking a single hash — which
/// is enough to reattribute a drawer-open or a refund to another register while
/// [`verify_chain`] still answers [`ChainVerdict::Intact`].
///
/// # The derived `Serialize` is not the canonical form
///
/// It is here because [`ref/domain-api.md`] §9 has it and because a `Debug`-like
/// dump is occasionally useful. It emits **declaration order**, and the canonical
/// bytes are **sorted at every level** — so the two differ, deliberately, and
/// `the_derived_serialization_is_not_the_canonical_form` fails if they ever stop
/// differing. Hash [`canonical_bytes`] and nothing else.
///
/// `Deserialize` is absent: serde cannot deserialize the `&'a AuditIntent`
/// borrow, and a verifier rebuilds an entry from database columns rather than
/// from JSON.
///
/// [`ref/domain-api.md`]: ../../../docs/implementation/ref/domain-api.md
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CanonicalAuditEntry<'a> {
    /// [`DOMAIN`].
    pub domain: &'static str,
    /// [`VERSION`], and the row's `canonical_version`.
    pub version: u16,
    /// The till this row belongs to. Half of the push ordering key (I-7).
    pub register_id: RegisterId,
    /// `audit_log.seq` — the register's own append order.
    pub seq: u64,
    /// `audit_log.id`.
    pub id: Uuid,
    /// The action, as the domain reported it.
    pub intent: &'a AuditIntent,
}

/// A `(seq, hash)` pair recorded somewhere the register cannot rewrite: on a Z
/// report, in a verified backup manifest, and from Phase 3 on the server.
///
/// This is the whole difference between "the chain is self-consistent" and "the
/// chain is what it was". `audit_checkpoint` stores one row per export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainAnchor {
    /// The register whose chain this anchors. An anchor for another register
    /// anchors nothing here — see [`verify_chain`].
    pub register_id: RegisterId,
    /// The `audit_log.seq` that was the head when the anchor was written.
    pub seq: u64,
    /// That row's `hash`.
    pub hash: [u8; 32],
}

/// What a walk of the chain found.
///
/// Four verdicts rather than a `bool`, because "we checked what we could" and
/// "it is intact" are different sentences and a merchant, an auditor and a court
/// all need the difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainVerdict {
    /// Every entry verified, and the anchor covers the head. Nothing is
    /// unverified.
    Intact {
        /// How many entries were walked.
        entries: u64,
    },
    /// The chain stops agreeing with itself at `at_seq`, or the anchored entry
    /// is not the one the anchor recorded. `at_seq` is where a forensic
    /// investigation starts.
    Broken {
        /// The `seq` the offending row carries, as read.
        at_seq: u64,
    },
    /// The chain is internally consistent but ends **before** the last anchored
    /// entry: rows were removed from the tail.
    Truncated {
        /// The head the anchor recorded.
        anchored_seq: u64,
        /// The highest `seq` still present, or `0` when nothing is.
        found_seq: u64,
    },
    /// Verified up to the anchor, and everything above it is unverifiable —
    /// deletion there is indistinguishable from nothing having happened.
    IntactUnanchoredFrom {
        /// How many entries were walked.
        entries: u64,
        /// The first `seq` no anchor covers. `0` when no usable anchor was
        /// supplied at all, which is every sequence number.
        unanchored_from: u64,
    },
}

/// The exact bytes an audit row's `hash` is taken over.
///
/// Canonical JSON, as [`ref/security-compliance.md`] §4 specifies it: keys
/// sorted lexicographically **at every level**, no whitespace, UTF-8 with no
/// BOM, integers as integers and never floats, timestamps ISO-8601 UTC with
/// milliseconds. Written by hand rather than by `serde_json::to_vec`, because
/// serde emits declaration order and a serde version that reordered anything
/// would silently invalidate every historical hash — which is the failure the
/// golden test `golden_canonical_bytes_are_stable` exists to prevent.
///
/// ```text
/// {"domain":…,"id":…,"intent":{"action":…,"actor":…,"approval":…,"approver":…,
///  "at":…,"entity":…,"entity_id":…,"payload":…,"reason":…},
///  "register_id":…,"seq":…,"version":…}
/// ```
///
/// Every field [`ref/security-compliance.md`] §4 lists is bound — the domain
/// tag, the chain version, `register_id`, `id`, `seq`, and the intent's `actor`,
/// `approver`, `action`, `entity`, `entity_id`, `reason`, `payload` and `at` —
/// plus `approval`, which §9's type carries and which names the handle the row
/// consumed. §4 enumerates those fields flat; they are nested under `"intent"`
/// here because §9's type nests them, and because nesting keeps a payload key
/// from ever colliding with an entry key. The *set* of bound fields is the same
/// either way, and the set is what the tamper evidence rests on.
///
/// This function is **total**: it encodes any [`AuditIntent`], including one
/// whose payload [`check_payload`] would refuse. That is deliberate. A verifier
/// walking a database it did not write must be able to hash a row that should
/// never have been written, or the forensic tool crashes on exactly the row it
/// was sent to report on.
///
/// [`ref/security-compliance.md`]: ../../../docs/implementation/ref/security-compliance.md
#[must_use]
pub fn canonical_bytes(entry: &CanonicalAuditEntry<'_>) -> Vec<u8> {
    let intent = entry.intent;
    let mut bytes = Vec::new();

    // The keys below are written in sorted order, once, in one place. Reading
    // them top to bottom is how a human checks that order; the sorted-keys test
    // beside the golden is how the build checks it.
    bytes.push(b'{');
    push_key(&mut bytes, "domain");
    push_json_string(&mut bytes, entry.domain);

    push_separator(&mut bytes, "id");
    push_uuid(&mut bytes, entry.id);

    push_separator(&mut bytes, "intent");
    bytes.push(b'{');
    push_key(&mut bytes, "action");
    push_json_string(&mut bytes, intent.action);
    push_separator(&mut bytes, "actor");
    push_uuid(&mut bytes, intent.actor.as_uuid());
    push_separator(&mut bytes, "approval");
    push_optional_uuid(&mut bytes, intent.approval.map(ApprovalId::as_uuid));
    push_separator(&mut bytes, "approver");
    push_optional_uuid(&mut bytes, intent.approver.map(UserId::as_uuid));
    push_separator(&mut bytes, "at");
    push_json_string(&mut bytes, &intent.at.to_iso8601());
    push_separator(&mut bytes, "entity");
    push_json_string(&mut bytes, intent.entity);
    push_separator(&mut bytes, "entity_id");
    push_uuid(&mut bytes, intent.entity_id);
    push_separator(&mut bytes, "payload");
    push_json_value(&mut bytes, &intent.payload);
    push_separator(&mut bytes, "reason");
    match &intent.reason {
        Some(reason) => push_json_string(&mut bytes, reason),
        None => bytes.extend_from_slice(b"null"),
    }
    bytes.push(b'}');

    push_separator(&mut bytes, "register_id");
    push_uuid(&mut bytes, entry.register_id.as_uuid());
    push_separator(&mut bytes, "seq");
    push_unsigned(&mut bytes, entry.seq);
    push_separator(&mut bytes, "version");
    push_unsigned(&mut bytes, u64::from(entry.version));
    bytes.push(b'}');

    bytes
}

/// `hash = BLAKE3(prev_hash ‖ canonical_bytes(entry))`.
///
/// BLAKE3 rather than SHA-256: faster on the low-end CPUs registers actually
/// run, and 32 bytes either way. `prev` is a fixed 32 bytes, so the
/// concatenation is unambiguous without a length prefix — the one place this
/// digest may differ in shape from the length-prefixed binary encodings used for
/// `sync_commit.commit_hash` and prepared-intent digests, which concatenate
/// variable-width fields.
#[must_use]
pub fn chain_hash(prev: &[u8; 32], entry: &CanonicalAuditEntry<'_>) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(prev);
    hasher.update(&canonical_bytes(entry));
    *hasher.finalize().as_bytes()
}

/// Walk one register's chain and say what it found.
///
/// `entries` yields `(seq, id, intent, prev_hash, hash)` — the stored columns —
/// in **ascending `seq` order**, which is the order `audit_log` is read in. The
/// walk stops at the first disagreement, because after one the rest is noise.
///
/// Two independent checks run per entry, and each answers a different tamper:
///
/// 1. the row's stored `prev_hash` must equal the previous row's `hash`
///    ([`GENESIS`] for the first) — this is what a deletion, an insertion or a
///    reordering breaks, and it breaks at the surviving row *after* the hole,
///    which is where the evidence actually is;
/// 2. `chain_hash(prev, entry)` must equal the row's stored `hash` — this is
///    what any edit to any bound field breaks, identity columns included, and it
///    breaks at the edited row itself.
///
/// `anchor` closes the one hole the chain cannot close on its own. An anchor for
/// a **different** register anchors nothing here: it is discarded, and the
/// verdict reports everything as unanchored rather than quietly claiming a
/// coverage the anchor cannot give.
#[must_use]
pub fn verify_chain<'a>(
    register_id: RegisterId,
    entries: impl Iterator<Item = (u64, Uuid, &'a AuditIntent, &'a [u8; 32], &'a [u8; 32])>,
    anchor: Option<ChainAnchor>,
) -> ChainVerdict {
    let anchor = anchor.filter(|candidate| candidate.register_id == register_id);

    let mut expected_prev = GENESIS;
    let mut entry_count: u64 = 0;
    let mut last_seq: u64 = 0;
    let mut anchored_hash: Option<[u8; 32]> = None;

    for (seq, id, intent, stored_prev, stored_hash) in entries {
        if *stored_prev != expected_prev {
            return ChainVerdict::Broken { at_seq: seq };
        }
        let entry = CanonicalAuditEntry {
            domain: DOMAIN,
            version: VERSION,
            register_id,
            seq,
            id,
            intent,
        };
        if chain_hash(&expected_prev, &entry) != *stored_hash {
            return ChainVerdict::Broken { at_seq: seq };
        }
        if anchor.is_some_and(|candidate| candidate.seq == seq) {
            anchored_hash = Some(*stored_hash);
        }
        expected_prev = *stored_hash;
        entry_count = entry_count.saturating_add(1);
        last_seq = seq;
    }

    let Some(anchor) = anchor else {
        return ChainVerdict::IntactUnanchoredFrom {
            entries: entry_count,
            unanchored_from: 0,
        };
    };

    if anchor.seq > last_seq {
        // The chain is self-consistent and stops below the anchor: the tail was
        // removed. This is the verdict the anchor exists to make possible.
        return ChainVerdict::Truncated {
            anchored_seq: anchor.seq,
            found_seq: last_seq,
        };
    }

    match anchored_hash {
        // The anchored row is not where the anchor says it is, or it is a
        // different row now — history was rewritten and re-chained, which the
        // walk alone cannot see because a re-chained history is self-consistent.
        None => ChainVerdict::Broken { at_seq: anchor.seq },
        Some(hash) if hash != anchor.hash => ChainVerdict::Broken { at_seq: anchor.seq },
        Some(_) if anchor.seq == last_seq => ChainVerdict::Intact {
            entries: entry_count,
        },
        Some(_) => ChainVerdict::IntactUnanchoredFrom {
            entries: entry_count,
            unanchored_from: anchor.seq.saturating_add(1),
        },
    }
}

/// Refuse a payload the canonical form cannot carry honestly, before it is
/// chained.
///
/// [`canonical_bytes`] is total by design, so this is the gate: the shell calls
/// it before appending an audit row, and a refusal is a bug in the caller that
/// built the payload rather than a runtime condition to recover from.
///
/// Two refusals, both from [`ref/security-compliance.md`] §4's canonical
/// serialization: a number that is not an integer, and nesting deeper than
/// [`MAX_PAYLOAD_DEPTH`]. The walk is iterative rather than recursive, so the
/// checker itself cannot be the thing that overflows, and it descends in
/// canonical document order so a payload with two faults always names the same
/// one.
///
/// # Errors
///
/// [`AuditError::PayloadNumberIsNotAnInteger`] and
/// [`AuditError::PayloadNestedTooDeeply`].
///
/// [`ref/security-compliance.md`]: ../../../docs/implementation/ref/security-compliance.md
pub fn check_payload(payload: &Value) -> Result<(), AuditError> {
    // (value, depth, pointer to it). Depth 1 is the payload itself.
    let mut pending: Vec<(&Value, usize, String)> = vec![(payload, 1, String::new())];

    while let Some((value, depth, pointer)) = pending.pop() {
        if depth > MAX_PAYLOAD_DEPTH {
            return Err(AuditError::PayloadNestedTooDeeply {
                limit: MAX_PAYLOAD_DEPTH,
            });
        }
        match value {
            Value::Number(number) => {
                if !number.is_i64() && !number.is_u64() {
                    return Err(AuditError::PayloadNumberIsNotAnInteger {
                        pointer: if pointer.is_empty() {
                            "/".to_owned()
                        } else {
                            pointer
                        },
                    });
                }
            }
            // Children go on the stack in reverse so they come *off* it in
            // canonical order — array index ascending, object key sorted — which
            // is what makes the reported pointer the first fault rather than
            // whichever one the traversal happened to reach.
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate().rev() {
                    pending.push((item, depth.saturating_add(1), format!("{pointer}/{index}")));
                }
            }
            Value::Object(members) => {
                let mut sorted: Vec<(&String, &Value)> = members.iter().collect();
                sorted.sort_unstable_by(|left, right| right.0.cmp(left.0));
                for (key, member) in sorted {
                    pending.push((
                        member,
                        depth.saturating_add(1),
                        format!("{pointer}/{}", escape_json_pointer(key)),
                    ));
                }
            }
            Value::Null | Value::Bool(_) | Value::String(_) => {}
        }
    }
    Ok(())
}

/// RFC 6901: `~` becomes `~0` and `/` becomes `~1`, so a pointer is unambiguous.
fn escape_json_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// One JSON string, escaped exactly one way.
///
/// `"` and `\` are escaped, the five short forms are used where they exist,
/// every other C0 control becomes `\u00xx` with lower-case hex, and everything
/// else — including every non-ASCII character — is written as raw UTF-8. The
/// rule is spelled out here rather than delegated because these bytes are the
/// hash: "whatever the serializer does today" is not a specification a dispute
/// can be settled against.
fn push_json_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.push(b'"');
    for character in value.chars() {
        match character {
            '"' => bytes.extend_from_slice(b"\\\""),
            '\\' => bytes.extend_from_slice(b"\\\\"),
            '\u{08}' => bytes.extend_from_slice(b"\\b"),
            '\u{0c}' => bytes.extend_from_slice(b"\\f"),
            '\n' => bytes.extend_from_slice(b"\\n"),
            '\r' => bytes.extend_from_slice(b"\\r"),
            '\t' => bytes.extend_from_slice(b"\\t"),
            control if control < '\u{20}' => {
                let code = u32::from(control);
                bytes.extend_from_slice(&[
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    hex_digit(((code >> 4) & 0x0f) as u8),
                    hex_digit((code & 0x0f) as u8),
                ]);
            }
            other => {
                let mut buffer = [0u8; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    bytes.push(b'"');
}

/// `"key":` — one object member's name and the colon after it.
fn push_key(bytes: &mut Vec<u8>, key: &str) {
    push_json_string(bytes, key);
    bytes.push(b':');
}

/// `,"key":` — the same, after a member that has already been written.
fn push_separator(bytes: &mut Vec<u8>, key: &str) {
    bytes.push(b',');
    push_key(bytes, key);
}

/// One lower-case hex digit for a value already masked to four bits.
const fn hex_digit(nibble: u8) -> u8 {
    if nibble < 10 {
        b'0' + nibble
    } else {
        b'a' + nibble - 10
    }
}

/// A UUID as the lower-case hyphenated string every id in this system stores and
/// transmits (`ids::typed_id!`'s `#[serde(transparent)]` wire form).
fn push_uuid(bytes: &mut Vec<u8>, id: Uuid) {
    let mut buffer = [0u8; uuid::fmt::Hyphenated::LENGTH];
    bytes.push(b'"');
    bytes.extend_from_slice(id.hyphenated().encode_lower(&mut buffer).as_bytes());
    bytes.push(b'"');
}

/// The same, or JSON `null`. An absent approver is a fact about the row, and it
/// is bound so that adding one later cannot pass as the original.
fn push_optional_uuid(bytes: &mut Vec<u8>, id: Option<Uuid>) {
    match id {
        Some(id) => push_uuid(bytes, id),
        None => bytes.extend_from_slice(b"null"),
    }
}

/// A JSON integer. `u64::to_string` is decimal digits with no separators, no
/// sign and no exponent, which is the only form this encoder emits.
fn push_unsigned(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(value.to_string().as_bytes());
}

/// One payload value, canonically.
///
/// Object keys are sorted here rather than trusted to come out sorted.
/// `serde_json::Map` is a `BTreeMap` today and iterates in order — but only
/// while the `preserve_order` feature stays off, and a feature flag enabled
/// three crates away must not be able to change what a historical hash was taken
/// over.
fn push_json_value(bytes: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Null => bytes.extend_from_slice(b"null"),
        Value::Bool(true) => bytes.extend_from_slice(b"true"),
        Value::Bool(false) => bytes.extend_from_slice(b"false"),
        // `Number`'s `Display` is decimal digits for an integer. A non-integer
        // reaches here only on a payload `check_payload` would have refused, and
        // is written in serde_json's shortest round-trip form rather than
        // panicking — see `canonical_bytes` on why this function is total.
        Value::Number(number) => bytes.extend_from_slice(number.to_string().as_bytes()),
        Value::String(text) => push_json_string(bytes, text),
        Value::Array(items) => {
            bytes.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    bytes.push(b',');
                }
                push_json_value(bytes, item);
            }
            bytes.push(b']');
        }
        Value::Object(object) => {
            let mut members: Vec<(&String, &Value)> = object.iter().collect();
            members.sort_unstable_by(|left, right| left.0.cmp(right.0));
            bytes.push(b'{');
            for (index, (key, member)) in members.into_iter().enumerate() {
                if index > 0 {
                    bytes.push(b',');
                }
                push_json_string(bytes, key);
                bytes.push(b':');
                push_json_value(bytes, member);
            }
            bytes.push(b'}');
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;

    // ── fixtures ────────────────────────────────────────────────────────────

    /// Two tills, so "this chain belongs to *this* register" is something the
    /// tests establish rather than assume.
    const REGISTER: RegisterId = RegisterId::from_uuid(Uuid::from_u128(0xa1));
    const OTHER_REGISTER: RegisterId = RegisterId::from_uuid(Uuid::from_u128(0xa2));

    /// Action/entity pairs drawn from the coverage list in
    /// `ref/security-compliance.md` §4 — a void, an override, a drawer open, a
    /// shift close, a PIN reset, and the chain break reporting itself. `static`
    /// rather than `const` because `prop::sample::select` wants a slice that
    /// really does live for `'static`.
    static AUDITED_ACTIONS: [(&str, &str); 6] = [
        ("sale.void", "sale"),
        ("price.override", "sale_line"),
        ("drawer.open", "drawer_event"),
        ("shift.close", "shift"),
        ("user.admin", "user"),
        ("audit.chain_break", "audit_log"),
    ];

    /// One audit row before it is chained.
    #[derive(Debug, Clone)]
    struct Row {
        seq: u64,
        id: Uuid,
        intent: AuditIntent,
    }

    /// One audit row as `audit_log` holds it: the identity columns, the intent,
    /// and the two hash columns.
    #[derive(Debug, Clone)]
    struct Stored {
        seq: u64,
        id: Uuid,
        intent: AuditIntent,
        prev: [u8; 32],
        hash: [u8; 32],
    }

    fn at(text: &str) -> Timestamp {
        Timestamp::parse_iso8601(text).unwrap()
    }

    fn uuid(text: &str) -> Uuid {
        Uuid::parse_str(text).unwrap()
    }

    /// Append `rows` to a fresh chain for `register`, exactly as the repository
    /// will at 1.6.6: read the previous hash, hash the entry, store both.
    fn chain(register: RegisterId, rows: &[Row]) -> Vec<Stored> {
        let mut prev = GENESIS;
        let mut stored = Vec::new();
        for row in rows {
            let entry = CanonicalAuditEntry {
                domain: DOMAIN,
                version: VERSION,
                register_id: register,
                seq: row.seq,
                id: row.id,
                intent: &row.intent,
            };
            let hash = chain_hash(&prev, &entry);
            stored.push(Stored {
                seq: row.seq,
                id: row.id,
                intent: row.intent.clone(),
                prev,
                hash,
            });
            prev = hash;
        }
        stored
    }

    fn verify(register: RegisterId, rows: &[Stored], anchor: Option<ChainAnchor>) -> ChainVerdict {
        verify_chain(
            register,
            rows.iter()
                .map(|row| (row.seq, row.id, &row.intent, &row.prev, &row.hash)),
            anchor,
        )
    }

    /// The anchor a Z close or a verified backup would have exported for this
    /// chain's current head.
    fn head_anchor(register: RegisterId, rows: &[Stored]) -> Option<ChainAnchor> {
        rows.last().map(|row| ChainAnchor {
            register_id: register,
            seq: row.seq,
            hash: row.hash,
        })
    }

    fn intent(action_index: usize, actor: u128, amount_minor: i64) -> AuditIntent {
        let (action, entity) = AUDITED_ACTIONS
            .get(action_index % AUDITED_ACTIONS.len())
            .copied()
            .expect("the modulus keeps the index in range");
        AuditIntent {
            actor: UserId::from_uuid(Uuid::from_u128(actor)),
            approver: None,
            approval: None,
            action,
            entity,
            entity_id: Uuid::from_u128(actor.wrapping_add(1_000)),
            reason: None,
            payload: json!({ "amount_minor": amount_minor }),
            at: at("2026-08-29T09:15:00.250Z"),
        }
    }

    fn three_rows() -> Vec<Row> {
        vec![
            Row {
                seq: 1,
                id: Uuid::from_u128(0x11),
                intent: intent(0, 1, -2_900),
            },
            Row {
                seq: 2,
                id: Uuid::from_u128(0x12),
                intent: intent(1, 2, 500),
            },
            // A gap: `audit_log.seq` is AUTOINCREMENT, and a rolled-back insert
            // consumes a number. A gap is not tampering.
            Row {
                seq: 7,
                id: Uuid::from_u128(0x13),
                intent: intent(2, 3, 0),
            },
        ]
    }

    // ── the golden ──────────────────────────────────────────────────────────

    /// The golden entry's `seq`, kept beside the bytes it appears in.
    const GOLDEN_SEQ: u64 = 42;

    /// The exact bytes an `audit_log.hash` is taken over.
    ///
    /// **Changing this constant is a protocol change, not a test fix.** Every
    /// hash ever written by every register was taken over bytes of this shape,
    /// so a diff here invalidates every historical chain unless [`VERSION`] is
    /// bumped in the same change and the verifier learns both layouts. It has
    /// the same standing as the trybuild golden in
    /// `crates/pos-domain/tests/ui/typed_ids_do_not_interconvert.stderr`: read
    /// the diff, do not accept it.
    ///
    /// Four claims are visible in it without running anything. Keys are sorted
    /// at both levels — `domain` before `id` before `intent`, and inside the
    /// payload `amount_minor` before `lines` although the fixture writes them
    /// the other way round. There is no whitespace. The Arabic reason is raw
    /// UTF-8, never `\u`-escaped. And `-2900` is an integer, because
    /// `approval_consumption`'s trigger reads it with
    /// `json_type(payload, '$.amount_minor') = 'integer'`.
    const GOLDEN_CANONICAL_BYTES: &str = concat!(
        r#"{"domain":"pos.audit","#,
        r#""id":"01998a2e-0001-7000-8000-0000000000b2","#,
        r#""intent":{"#,
        r#""action":"sale.void","#,
        r#""actor":"01998a2e-0002-7000-8000-0000000000c3","#,
        r#""approval":"01998a2e-0004-7000-8000-0000000000e5","#,
        r#""approver":"01998a2e-0003-7000-8000-0000000000d4","#,
        r#""at":"2026-08-29T09:15:00.250Z","#,
        r#""entity":"sale","#,
        r#""entity_id":"01998a2e-0005-7000-8000-0000000000f6","#,
        r#""payload":{"amount_minor":-2900,"lines":3},"#,
        r#""reason":"طلب العميل""#,
        r#"},"#,
        r#""register_id":"01998a2e-0000-7000-8000-0000000000a1","#,
        r#""seq":42,"#,
        r#""version":1}"#,
    );

    /// `BLAKE3(GENESIS ‖ GOLDEN_CANONICAL_BYTES)` — the first link of a chain
    /// whose first entry is the golden one. Same standing as the bytes above.
    const GOLDEN_CHAIN_HASH: &str =
        "c4b66775f2cefbcd7415875be666a873a7917a8b2309ee00b81054ea1ffb72a2";

    fn golden_register() -> RegisterId {
        RegisterId::from_uuid(uuid("01998a2e-0000-7000-8000-0000000000a1"))
    }

    fn golden_intent() -> AuditIntent {
        AuditIntent {
            actor: UserId::from_uuid(uuid("01998a2e-0002-7000-8000-0000000000c3")),
            approver: Some(UserId::from_uuid(uuid(
                "01998a2e-0003-7000-8000-0000000000d4",
            ))),
            approval: Some(ApprovalId::from_uuid(uuid(
                "01998a2e-0004-7000-8000-0000000000e5",
            ))),
            action: "sale.void",
            entity: "sale",
            entity_id: uuid("01998a2e-0005-7000-8000-0000000000f6"),
            reason: Some("طلب العميل".to_owned()),
            // Deliberately written out of order: the canonical form sorts, and
            // the golden proves it rather than describing it.
            payload: json!({ "lines": 3, "amount_minor": -2_900 }),
            at: at("2026-08-29T09:15:00.250Z"),
        }
    }

    fn golden_entry(intent: &AuditIntent) -> CanonicalAuditEntry<'_> {
        CanonicalAuditEntry {
            domain: DOMAIN,
            version: VERSION,
            register_id: golden_register(),
            seq: GOLDEN_SEQ,
            id: uuid("01998a2e-0001-7000-8000-0000000000b2"),
            intent,
        }
    }

    #[test]
    fn golden_canonical_bytes_are_stable() {
        let intent = golden_intent();
        let entry = golden_entry(&intent);

        assert_eq!(
            String::from_utf8(canonical_bytes(&entry)).unwrap(),
            GOLDEN_CANONICAL_BYTES,
            "the canonical audit encoding changed. That is a protocol change: \
             bump VERSION and teach the verifier both layouts, or revert."
        );
        assert_eq!(
            blake3::Hash::from_bytes(chain_hash(&GENESIS, &entry))
                .to_hex()
                .as_str(),
            GOLDEN_CHAIN_HASH,
            "the chain hash of the golden entry changed"
        );
    }

    #[test]
    fn canonical_bytes_sort_object_keys_at_every_level() {
        // The golden pins one encoding by eye; this pins the *rule* it is an
        // instance of. Parsing into `serde_json::Value` sorts every object into
        // a BTreeMap, and re-encoding through this module's own writer produces
        // the sorted form — so a canonical encoding that had emitted any key out
        // of order would not survive the round trip. It also proves the bytes
        // are valid JSON, which `audit_log.payload`'s `json_extract` callers
        // depend on.
        let intent = golden_intent();
        let bytes = canonical_bytes(&golden_entry(&intent));

        let parsed: Value = serde_json::from_slice(&bytes).expect("canonical bytes are JSON");
        let mut round_tripped = Vec::new();
        push_json_value(&mut round_tripped, &parsed);
        assert_eq!(
            String::from_utf8(round_tripped).unwrap(),
            String::from_utf8(bytes).unwrap(),
        );
    }

    #[test]
    fn canonical_bytes_escape_a_string_exactly_one_way() {
        // These bytes are the hash, so "whatever the serializer does today" is
        // not a specification. Quote and backslash escaped, the five short forms
        // used, every other C0 control as lower-case `\u00xx`, and everything
        // non-ASCII written raw.
        let mut bytes = Vec::new();
        push_json_string(&mut bytes, "a\"b\\c\nd\te\rf\u{08}g\u{0c}h\u{01}i…ي");
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "\"a\\\"b\\\\c\\nd\\te\\rf\\bg\\fh\\u0001i…ي\"",
        );
    }

    #[test]
    fn the_derived_serialization_is_not_the_canonical_form() {
        // `CanonicalAuditEntry` derives `Serialize` because ref/domain-api.md §9
        // does, and the derived form emits declaration order. Hashing it would
        // be a silent, permanent divergence, so the difference is asserted
        // rather than left as a comment nobody reads.
        let intent = golden_intent();
        let derived = serde_json::to_string(&golden_entry(&intent)).unwrap();

        assert_ne!(derived, GOLDEN_CANONICAL_BYTES);
        assert!(
            derived.starts_with(r#"{"domain":"pos.audit","version":1"#),
            "the derive emits declaration order: {derived}"
        );
    }

    // ── the chain ───────────────────────────────────────────────────────────

    #[test]
    fn an_intact_chain_verifies_against_its_head_anchor() {
        let stored = chain(REGISTER, &three_rows());
        assert_eq!(
            verify(REGISTER, &stored, head_anchor(REGISTER, &stored)),
            ChainVerdict::Intact { entries: 3 },
        );
    }

    #[test]
    fn an_unanchored_chain_says_so_rather_than_claiming_intact() {
        // "We checked what we could" and "it is intact" are different sentences.
        let stored = chain(REGISTER, &three_rows());
        assert_eq!(
            verify(REGISTER, &stored, None),
            ChainVerdict::IntactUnanchoredFrom {
                entries: 3,
                unanchored_from: 0,
            },
        );

        // An anchor below the head covers what is below it and nothing above.
        let anchored = stored.first().unwrap();
        assert_eq!(
            verify(
                REGISTER,
                &stored,
                Some(ChainAnchor {
                    register_id: REGISTER,
                    seq: anchored.seq,
                    hash: anchored.hash,
                }),
            ),
            ChainVerdict::IntactUnanchoredFrom {
                entries: 3,
                unanchored_from: 2,
            },
        );

        // An empty chain with no anchor is the honest zero, not `Intact`.
        assert_eq!(
            verify(REGISTER, &[], None),
            ChainVerdict::IntactUnanchoredFrom {
                entries: 0,
                unanchored_from: 0,
            },
        );
    }

    #[test]
    fn an_anchor_from_another_register_anchors_nothing() {
        // Silently accepting it would let a verifier report `Intact` on the
        // strength of a document about a different till.
        let stored = chain(REGISTER, &three_rows());
        let foreign = head_anchor(OTHER_REGISTER, &stored);
        assert_eq!(
            verify(REGISTER, &stored, foreign),
            ChainVerdict::IntactUnanchoredFrom {
                entries: 3,
                unanchored_from: 0,
            },
        );
    }

    #[test]
    fn a_rechained_history_verifies_alone_and_fails_against_the_anchor() {
        // The attack the anchor exists for: someone with the database key edits
        // a row and recomputes every hash after it. The result is a perfectly
        // valid chain — the walk cannot object — and the anchor is the only
        // thing left that remembers what the head used to be.
        let rows = three_rows();
        let honest = chain(REGISTER, &rows);
        let anchor = head_anchor(REGISTER, &honest).unwrap();

        let mut rewritten = rows;
        rewritten.get_mut(1).unwrap().intent.payload = json!({ "amount_minor": 999_999 });
        let forged = chain(REGISTER, &rewritten);

        assert_eq!(
            verify(REGISTER, &forged, None),
            ChainVerdict::IntactUnanchoredFrom {
                entries: 3,
                unanchored_from: 0,
            },
            "a re-chained history is internally consistent; saying otherwise \
             would be a claim this mechanism cannot support"
        );
        assert_eq!(
            verify(REGISTER, &forged, Some(anchor)),
            ChainVerdict::Broken { at_seq: anchor.seq },
        );
    }

    #[test]
    fn mutating_an_identity_column_breaks_the_chain() {
        // `register_id`, `id` and `seq` are the three columns the first design
        // left outside the hash — which is exactly enough to reattribute a
        // refund to another till, renumber it, or re-point its identity, with
        // every hash still verifying. One case per column.
        let stored = chain(REGISTER, &three_rows());
        let anchor = head_anchor(REGISTER, &stored);
        assert_eq!(
            verify(REGISTER, &stored, anchor),
            ChainVerdict::Intact { entries: 3 },
            "the untampered chain must verify, or the three cases below prove nothing"
        );

        // register_id — the rows are read as another register's chain. Every
        // entry rehashes, so the first one already refuses.
        assert_eq!(
            verify(OTHER_REGISTER, &stored, None),
            ChainVerdict::Broken {
                at_seq: stored.first().unwrap().seq,
            },
        );

        // seq — renumbering the middle row.
        let mut renumbered = stored.clone();
        renumbered.get_mut(1).unwrap().seq = 99;
        assert_eq!(
            verify(REGISTER, &renumbered, anchor),
            ChainVerdict::Broken { at_seq: 99 },
        );

        // id — re-pointing the middle row's identity.
        let mut repointed = stored.clone();
        let at_seq = {
            let row = repointed.get_mut(1).unwrap();
            row.id = Uuid::from_u128(row.id.as_u128() ^ 1);
            row.seq
        };
        assert_eq!(
            verify(REGISTER, &repointed, anchor),
            ChainVerdict::Broken { at_seq },
        );
    }

    // ── payload policy ──────────────────────────────────────────────────────

    #[test]
    fn a_non_integer_payload_number_is_refused_before_it_is_chained() {
        assert_eq!(check_payload(&json!({ "amount_minor": -2_900 })), Ok(()));
        assert_eq!(
            check_payload(&json!({ "amount_minor": 29.0 })),
            Err(AuditError::PayloadNumberIsNotAnInteger {
                pointer: "/amount_minor".to_owned(),
            }),
        );
        assert_eq!(
            check_payload(&json!({ "lines": [{ "qty_milli": 0.5 }] })),
            Err(AuditError::PayloadNumberIsNotAnInteger {
                pointer: "/lines/0/qty_milli".to_owned(),
            }),
        );
        // A bare non-integer payload has no key to point at.
        assert_eq!(
            check_payload(&json!(1.5)),
            Err(AuditError::PayloadNumberIsNotAnInteger {
                pointer: "/".to_owned(),
            }),
        );
        // RFC 6901: `/` and `~` inside a key are escaped, so the pointer is not
        // ambiguous about where the offending number is.
        assert_eq!(
            check_payload(&json!({ "a/b~c": 0.25 })),
            Err(AuditError::PayloadNumberIsNotAnInteger {
                pointer: "/a~1b~0c".to_owned(),
            }),
        );
        // Two faults: the pointer names the first in canonical order, whatever
        // order the payload was written in. A refusal that moved between runs
        // would send whoever has to fix it to a different place each time.
        assert_eq!(
            check_payload(&json!({ "z": 0.5, "a": 0.25 })),
            Err(AuditError::PayloadNumberIsNotAnInteger {
                pointer: "/a".to_owned(),
            }),
        );
    }

    #[test]
    fn a_payload_nested_past_the_canonical_limit_is_refused() {
        let mut deep = json!(0);
        for _ in 0..MAX_PAYLOAD_DEPTH {
            deep = Value::Array(vec![deep]);
        }
        assert_eq!(
            check_payload(&deep),
            Err(AuditError::PayloadNestedTooDeeply {
                limit: MAX_PAYLOAD_DEPTH,
            }),
        );

        let mut allowed = json!(0);
        for _ in 0..(MAX_PAYLOAD_DEPTH - 1) {
            allowed = Value::Array(vec![allowed]);
        }
        assert_eq!(check_payload(&allowed), Ok(()));
    }

    // ── the properties ──────────────────────────────────────────────────────

    /// One entry's variable parts.
    ///
    /// Covers: six real action/entity pairs; an approver and an approval each
    /// present or absent, which is the escalation/no-escalation split; a reason
    /// present or absent; and negative, zero and positive `amount_minor`.
    /// Excludes payload *shape* and string escaping — the golden and
    /// `canonical_bytes_escape_a_string_exactly_one_way` own the encoder, and
    /// these properties attack the chain.
    fn audit_intents() -> impl Strategy<Value = AuditIntent> {
        (
            0u128..64,
            proptest::option::of(64u128..128),
            proptest::option::of(128u128..192),
            prop::sample::select(AUDITED_ACTIONS.as_slice()),
            192u128..256,
            proptest::option::of("[a-z ]{0,16}"),
            -1_000_000i64..1_000_000,
            0i64..1_000_000_000_000,
        )
            .prop_map(
                |(
                    actor,
                    approver,
                    approval,
                    (action, entity),
                    entity_id,
                    reason,
                    amount,
                    millis,
                )| {
                    AuditIntent {
                        actor: UserId::from_uuid(Uuid::from_u128(actor)),
                        approver: approver.map(|id| UserId::from_uuid(Uuid::from_u128(id))),
                        approval: approval.map(|id| ApprovalId::from_uuid(Uuid::from_u128(id))),
                        action,
                        entity,
                        entity_id: Uuid::from_u128(entity_id),
                        reason,
                        payload: json!({ "amount_minor": amount }),
                        at: Timestamp::from_epoch_milliseconds(millis).unwrap(),
                    }
                },
            )
    }

    /// A chain of `min..=8` entries for one register.
    ///
    /// Sequence numbers ascend with gaps of one to three, because
    /// `audit_log.seq` is `AUTOINCREMENT` and a rolled-back insert consumes a
    /// number: a gap is ordinary history, not tampering, and a verifier that
    /// treated it as tampering would alarm on every crash. Excludes two
    /// registers in one chain, which cannot exist — a chain belongs to one till.
    fn audit_chains_of(min: usize) -> impl Strategy<Value = Vec<Row>> {
        proptest::collection::vec((1u64..=3, 0u128..1_024, audit_intents()), min..=8).prop_map(
            |generated| {
                let mut seq = 0u64;
                generated
                    .into_iter()
                    .map(|(gap, id, intent)| {
                        seq = seq.saturating_add(gap);
                        Row {
                            seq,
                            id: Uuid::from_u128(id),
                            intent,
                        }
                    })
                    .collect()
            },
        )
    }

    /// Every bound field of a stored row, one mutation each — the nine intent
    /// fields, the two identity columns, and the two hash columns themselves.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Mutation {
        Seq,
        Id,
        Actor,
        Approver,
        Approval,
        Action,
        Entity,
        EntityId,
        Reason,
        PayloadAmount,
        At,
        StoredPrev,
        StoredHash,
    }

    static MUTATIONS: [Mutation; 13] = [
        Mutation::Seq,
        Mutation::Id,
        Mutation::Actor,
        Mutation::Approver,
        Mutation::Approval,
        Mutation::Action,
        Mutation::Entity,
        Mutation::EntityId,
        Mutation::Reason,
        Mutation::PayloadAmount,
        Mutation::At,
        Mutation::StoredPrev,
        Mutation::StoredHash,
    ];

    /// Change exactly one thing, and always change it: a mutation that happened
    /// to be a no-op would make this property assert nothing.
    fn mutate(row: &mut Stored, mutation: Mutation) {
        match mutation {
            Mutation::Seq => row.seq = row.seq.wrapping_add(1),
            Mutation::Id => row.id = Uuid::from_u128(row.id.as_u128() ^ 1),
            Mutation::Actor => {
                row.intent.actor =
                    UserId::from_uuid(Uuid::from_u128(row.intent.actor.as_uuid().as_u128() ^ 1));
            }
            Mutation::Approver => {
                row.intent.approver = match row.intent.approver {
                    Some(_) => None,
                    None => Some(UserId::from_uuid(Uuid::from_u128(0xbeef))),
                };
            }
            Mutation::Approval => {
                row.intent.approval = match row.intent.approval {
                    Some(_) => None,
                    None => Some(ApprovalId::from_uuid(Uuid::from_u128(0xcafe))),
                };
            }
            Mutation::Action => {
                row.intent.action = if row.intent.action == "sale.void" {
                    "price.override"
                } else {
                    "sale.void"
                };
            }
            Mutation::Entity => {
                row.intent.entity = if row.intent.entity == "sale" {
                    "shift"
                } else {
                    "sale"
                };
            }
            Mutation::EntityId => {
                row.intent.entity_id = Uuid::from_u128(row.intent.entity_id.as_u128() ^ 1);
            }
            Mutation::Reason => {
                row.intent.reason = match row.intent.reason {
                    Some(_) => None,
                    None => Some("tampered".to_owned()),
                };
            }
            Mutation::PayloadAmount => {
                let amount = row
                    .intent
                    .payload
                    .get("amount_minor")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                row.intent.payload = json!({ "amount_minor": amount.wrapping_add(1) });
            }
            Mutation::At => {
                row.intent.at =
                    Timestamp::from_epoch_milliseconds(row.intent.at.epoch_milliseconds() + 1)
                        .unwrap();
            }
            Mutation::StoredPrev => row.prev[0] ^= 1,
            Mutation::StoredHash => row.hash[0] ^= 1,
        }
    }

    /// A chain, a row in it, and one mutation to apply to that row.
    fn mutation_cases() -> impl Strategy<Value = (Vec<Row>, usize, Mutation)> {
        audit_chains_of(1).prop_flat_map(|rows| {
            let len = rows.len();
            (
                Just(rows),
                0..len,
                prop::sample::select(MUTATIONS.as_slice()),
            )
        })
    }

    /// A chain of at least two, and a victim that is never the tail — tail
    /// deletion is a different property, because it is the one the chain alone
    /// cannot see.
    fn deletion_cases() -> impl Strategy<Value = (Vec<Row>, usize)> {
        audit_chains_of(2).prop_flat_map(|rows| {
            let last = rows.len().saturating_sub(1);
            (Just(rows), 0..last)
        })
    }

    /// A chain of at least two, an anchor somewhere above its first entry, and
    /// how many entries survive below that anchor.
    fn truncation_cases() -> impl Strategy<Value = (Vec<Row>, usize, usize)> {
        audit_chains_of(2)
            .prop_flat_map(|rows| {
                let len = rows.len();
                (Just(rows), 1..len)
            })
            .prop_flat_map(|(rows, anchor_index)| (Just(rows), Just(anchor_index), 0..anchor_index))
    }

    /// A chain of at least two and two distinct positions in it.
    fn reordering_cases() -> impl Strategy<Value = (Vec<Row>, usize, usize)> {
        audit_chains_of(2)
            .prop_flat_map(|rows| {
                let len = rows.len();
                (Just(rows), 0..len, 0..len)
            })
            .prop_filter("two distinct positions", |(_, first, second)| {
                first != second
            })
    }

    proptest! {
        // One shared configuration for every property in this crate: 4,096
        // cases, the repository's recorded seed, and a minimized failing case
        // persisted under crates/pos-domain/proptest-regressions/audit.txt to be
        // committed. Owned by microstep 1.1.0; conventions §5.1 is the rule.
        #![proptest_config(domain_proptest_config())]

        /// Change any bound field of any entry — an identity column, an intent
        /// field, or one of the two hash columns — and the verifier says
        /// `Broken` at that entry's sequence number. Not merely "an error": the
        /// number is what a forensic investigation starts from.
        #[test]
        fn prop_chain_detects_any_single_entry_mutation(
            (rows, target, mutation) in mutation_cases()
        ) {
            let mut stored = chain(REGISTER, &rows);
            let anchor = head_anchor(REGISTER, &stored);
            let at_seq = {
                let row = stored.get_mut(target).expect("the strategy bounds the index");
                mutate(row, mutation);
                row.seq
            };

            prop_assert_eq!(
                verify(REGISTER, &stored, anchor),
                ChainVerdict::Broken { at_seq }
            );
        }

        /// Remove any entry that is not the tail and the chain says so, at the
        /// surviving entry above the hole — which is where the evidence is. The
        /// retained anchor is not what catches this one: the chain is, and the
        /// second assertion proves the anchor is not doing the work.
        #[test]
        fn prop_chain_detects_deletion_before_the_anchor((rows, victim) in deletion_cases()) {
            let stored = chain(REGISTER, &rows);
            let anchor = head_anchor(REGISTER, &stored);
            let at_seq = stored
                .get(victim + 1)
                .map(|row| row.seq)
                .expect("the strategy never chooses the tail");

            let mut kept = stored;
            kept.remove(victim);

            prop_assert_eq!(verify(REGISTER, &kept, anchor), ChainVerdict::Broken { at_seq });
            prop_assert_eq!(verify(REGISTER, &kept, None), ChainVerdict::Broken { at_seq });
        }

        /// Delete the tail and what is left is a shorter, perfectly valid chain
        /// — the truncation removed the evidence of itself. The anchor is what
        /// turns that into a verdict, and this property asserts both halves:
        /// `Truncated` with the anchor, and nothing at all without it. The
        /// second assertion is the honest statement, not a weakness to hide.
        #[test]
        fn prop_chain_detects_tail_deletion_against_an_anchor(
            (rows, anchor_index, keep) in truncation_cases()
        ) {
            let stored = chain(REGISTER, &rows);
            let anchored = stored.get(anchor_index).expect("the strategy bounds the index");
            let anchor = ChainAnchor {
                register_id: REGISTER,
                seq: anchored.seq,
                hash: anchored.hash,
            };

            let kept: Vec<Stored> = stored.iter().take(keep).cloned().collect();
            let found_seq = kept.last().map_or(0, |row| row.seq);

            prop_assert_eq!(
                verify(REGISTER, &kept, Some(anchor)),
                ChainVerdict::Truncated { anchored_seq: anchor.seq, found_seq }
            );
            prop_assert_eq!(
                verify(REGISTER, &kept, None),
                ChainVerdict::IntactUnanchoredFrom {
                    entries: keep as u64,
                    unanchored_from: 0,
                }
            );
        }

        /// Swap any two entries and the chain refuses at the earlier of the two
        /// positions, because the entry that landed there carries a `prev_hash`
        /// belonging to somewhere else.
        #[test]
        fn prop_chain_detects_reordering((rows, first, second) in reordering_cases()) {
            let stored = chain(REGISTER, &rows);
            let anchor = head_anchor(REGISTER, &stored);

            let mut swapped = stored;
            swapped.swap(first, second);
            let at_seq = swapped
                .get(first.min(second))
                .map(|row| row.seq)
                .expect("the strategy bounds both positions");

            prop_assert_eq!(verify(REGISTER, &swapped, anchor), ChainVerdict::Broken { at_seq });
        }
    }
}
