//! Walk a register's audit hash chain and say what it found (microstep 1.6.6b).
//!
//! Gap G-7 again, from the other end. `ref/plan-validation.md:343` is the
//! sentence the whole of 1.6.x answers — *"the canonical serialization,
//! coverage, verifier, and chain-break behaviour are not [specified]. **An
//! unverifiable hash chain is decoration**."* — and the three pieces arrived in
//! order: `1.6.5` the arithmetic, `1.6.6` the writer and the reader, and this,
//! the thing a merchant, an auditor or a court can actually run. It exists in
//! Phase 1 rather than Phase 5, where `5.4.4` extends it, because the Phase-1
//! exit gate (`phase-1:1479`) demonstrates tamper detection, and a claim whose
//! tool arrives four phases later is a claim nobody can check.
//!
//! # It decides nothing
//!
//! Every judgement here belongs to [`pos_domain::verify_chain`], which is pure.
//! This binary supplies the arguments that a pure function cannot compute — the
//! file, the key, the anchor read off a disk — prints what came back, and turns
//! it into an exit code. That division is I-8, and it is why the verifier is a
//! binary in `pos-db`: the connection and the key provider already live here.
//!
//! # Four decisions the phase entry does not make
//!
//! **The anchor file is shaped like `audit_checkpoint`, not like
//! [`ChainAnchor`].** The flag is `--anchor <backup-manifest>` and `5.4.4`
//! extends it to the server's last accepted `audit_checkpoint` — a table that
//! already exists (`0004:478`) with the columns `register_id`, `last_seq`,
//! `last_hash`, `source_kind` and `anchored_at`. Those are the key names, so
//! that microstep is an extension rather than a rename. [`ChainAnchor`] does
//! derive `Deserialize` and is deliberately not used for the file: its `hash`
//! is a `[u8; 32]`, whose serde form in JSON is an array of thirty-two
//! integers that no backup tool or operator would ever write, and bending a
//! domain type's wire form to a CLI's convenience is the wrong repair.
//!
//! **An anchor recording `last_seq: 0` anchors nothing and is not tampering.**
//! `audit_checkpoint.last_seq` is `CHECK (last_seq >= 0)`, so a Z close on a
//! register that has written no audit row yet can honestly record zero. Handing
//! that to [`verify_chain`] returns `Broken { at_seq: 0 }` — no row is ever
//! numbered zero, so the anchored hash is never found — which is tampering
//! reported where none happened, the one answer `repo::audit` says a forensic
//! tool must never give. It is accepted, reported as covering nothing, and
//! passed on as `None`.
//!
//! **The register set is what the file holds *union* what the anchor names.**
//! Enumerating only the tills with rows leaves a hole exactly where the anchor
//! is meant to close one: delete *every* row of a register and it vanishes from
//! the enumeration, so the verifier walks nothing, reports nothing and exits 0
//! while holding an anchor that says there were four rows. Adding the anchor's
//! own register turns that back into `Truncated { anchored_seq: 4,
//! found_seq: 0 }`.
//!
//! **An anchor above a stopped read is withheld, not applied.** This is the
//! one combination where a correct anchor and a correct stop produce a wrong
//! answer between them, and no mutation of either half can find it: a read that
//! stops at seq 3 hands `verify_chain` the rows below it, and an anchor at
//! seq 5 then makes it answer `Truncated { anchored_seq: 5, found_seq: 2 }` —
//! *the rows between were removed*. They were not. Rows 3 to 5 are still on the
//! disk; this build could not rebuild one of them, which once there are two
//! versions in the field is an upgrade far more often than an attack. An anchor
//! **below** the stop is untouched: it is entirely inside the prefix that was
//! read.
//!
//! # Nothing raw from the anchor file reaches the report
//!
//! The anchor is supplied by whoever is being investigated and the report is a
//! document somebody reads as evidence, so every field is parsed and printed
//! back from the parsed value: the register as a `Uuid`, the digest through
//! this file's own `hex`, `source_kind` as the matched `&'static str` out of
//! `audit_checkpoint`'s three, and `anchored_at` through `Timestamp`. A field
//! echoed byte for byte would hand its author a terminal — one newline inside
//! `source_kind` is enough to forge a register block, verdict and all, into the
//! middle of the report.
//!
//! # What it does to the file
//!
//! It opens through [`pos_db::open`], which is read-write: it sets WAL mode and
//! applies any migration the file is behind. That is why the usage line says
//! `<copy>` and why `--help` says so in its own words. No register in the field
//! is below `0005` today, so the migration path cannot fire yet; the day it
//! can, what changes is the copy and never the original — which is also what
//! `phase-1:1479`'s drill assumes when it takes a snapshot first.
//!
//! [`ChainAnchor`]: pos_domain::ChainAnchor
//! [`verify_chain`]: pos_domain::verify_chain

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pos_db::key::KeySource;
use pos_db::repo::audit::{AuditRepository, ChainStop};
use pos_domain::{ChainAnchor, ChainVerdict, RegisterId, Timestamp};
use uuid::Uuid;

/// A BLAKE3 digest, and therefore twice that many hex characters.
const DIGEST_BYTES: usize = 32;
const DIGEST_HEX: usize = DIGEST_BYTES * 2;

const HELP: &str = "\
verify-audit — walk a register's audit hash chain and report what it found.

USAGE
    verify-audit --database <path> [--anchor <path>]
    verify-audit --help

OPTIONS
    --database <path>  The database to read. Give it a COPY. This opens the
                       file read-write through the application's own open path,
                       which sets WAL mode and applies any migration the copy
                       is behind; the original is never touched.
    --anchor <path>    A JSON anchor taken somewhere the register cannot
                       rewrite — a Z report, a verified backup manifest, or
                       (microstep 5.4.4) the server's last accepted checkpoint.
                       Without one, deleting the newest rows is
                       indistinguishable from nothing having happened.
    -h, --help         Print this and exit 0.

THE ANCHOR FILE
    One JSON object, named for the columns of `audit_checkpoint`:

        {
          \"register_id\": \"0192f3a4-5b6c-7d8e-9f01-23456789abcd\",
          \"last_seq\":    4,
          \"last_hash\":   \"<64 hex characters>\",
          \"source_kind\": \"verified_backup\",
          \"anchored_at\": \"2026-09-21T10:00:00.000Z\"
        }

    `register_id`, `last_seq` and `last_hash` are required; the other two are
    provenance. Every one of the five is parsed and printed back from the parsed
    value, never echoed: `source_kind` must be one of audit_checkpoint's own
    three — z_report, verified_backup, server — and `anchored_at` must be an
    instant. Keys this build does not know are ignored, so a later checkpoint
    format stays readable. `\"last_seq\": 0` records a register that had written
    no audit row when the anchor was taken — it is accepted, and it anchors
    nothing.

THE DATABASE KEY
    Read through the application's own provider: POS_DB_KEY in a debug build,
    the OS credential store otherwise. The report names the SOURCE, never the
    key.

EXIT CODES
    0  Every chain this build could read agreed with itself and with the anchor
       supplied. Read the report: with no anchor, 0 says nothing about whether
       the newest rows are all still there.
    1  Tamper evidence — a chain that disagrees with itself, or a tail deleted
       below an anchor.
    2  The tool could not run: bad arguments, an unreadable anchor, or a
       database that would not open.
    3  Inconclusive — a row this build cannot rebuild, or rows belonging to no
       chain at all. Not a verdict about the register.
";

fn main() -> ExitCode {
    match parse(std::env::args_os().skip(1)) {
        Ok(Invocation::Help) => {
            print!("{HELP}");
            ExitCode::from(Verdict::Verified.code())
        }
        Ok(Invocation::Verify { database, anchor }) => {
            ExitCode::from(verify(&database, anchor.as_deref()).code())
        }
        Err(reason) => {
            eprintln!("verify-audit: {reason}");
            eprintln!("Run `verify-audit --help` for the usage.");
            ExitCode::from(Verdict::CouldNotRun.code())
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The verdict, and what each exit code means
// ─────────────────────────────────────────────────────────────────────────────

/// How a run ended, and through [`Verdict::code`] what the shell is told.
///
/// The number is written out in one `match` rather than carried as a
/// discriminant, because the four codes are a published interface — they are in
/// `--help`, `help_exits_zero_and_names_every_exit_code` reads them there, and
/// a reordered enum must not be able to renumber them silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Everything this build read agreed with itself and with the anchor.
    Verified,
    /// A chain that disagrees with itself, or a tail deleted below an anchor.
    Tampered,
    /// The tool could not run over some or all of what it was pointed at.
    CouldNotRun,
    /// A row this build cannot rebuild, or rows in no chain. Not a verdict.
    Inconclusive,
}

impl Verdict {
    const fn code(self) -> u8 {
        match self {
            Self::Verified => 0,
            Self::Tampered => 1,
            Self::CouldNotRun => 2,
            Self::Inconclusive => 3,
        }
    }

    /// The one a run reports when more than one happened.
    ///
    /// **Tamper evidence outranks a failure to read**, which is the ordering a
    /// reader is most likely to get backwards. "This register was altered" is
    /// a finding somebody must act on today; "one of its rows would not come
    /// back" is a reason to look again. A run that found both must not report
    /// only the weaker one, and the report prints both regardless of which
    /// number the shell sees.
    const fn worse(self, other: Self) -> Self {
        match (self, other) {
            (Self::Tampered, _) | (_, Self::Tampered) => Self::Tampered,
            (Self::CouldNotRun, _) | (_, Self::CouldNotRun) => Self::CouldNotRun,
            (Self::Inconclusive, _) | (_, Self::Inconclusive) => Self::Inconclusive,
            (Self::Verified, Self::Verified) => Self::Verified,
        }
    }

    const fn headline(self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::Tampered => "TAMPERED",
            Self::CouldNotRun => "COULD NOT RUN",
            Self::Inconclusive => "INCONCLUSIVE",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Arguments
// ─────────────────────────────────────────────────────────────────────────────

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Invocation {
    Help,
    Verify {
        database: PathBuf,
        anchor: Option<PathBuf>,
    },
}

/// Parse the command line, by hand.
///
/// Two flags and a help switch do not earn an argument-parser dependency: one
/// would enter the register's own crate graph through `pos-db`, move the
/// licence and advisory surface `just audit` reports, and buy nothing this
/// forty lines does not already do — including refusing a repeated flag, which
/// most parsers accept and silently resolve to the last value.
fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    let arguments: Vec<OsString> = arguments.into_iter().collect();

    // Help beats everything, a malformed rest of the line included: somebody
    // who has just been refused needs the page more than the refusal.
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        return Ok(Invocation::Help);
    }

    let mut database: Option<PathBuf> = None;
    let mut anchor: Option<PathBuf> = None;

    let mut remaining = arguments.into_iter();
    while let Some(argument) = remaining.next() {
        let flag = argument.to_string_lossy().into_owned();
        let slot = match flag.as_str() {
            "--database" => &mut database,
            "--anchor" => &mut anchor,
            other if other.starts_with('-') => {
                return Err(format!("unknown option `{other}`"));
            }
            // The token is not echoed. It is a value that arrived without its
            // flag, and argv is exactly where somebody types a key or a token
            // by mistake; naming the shape is as useful as naming the bytes.
            _ => {
                return Err("a value arrived without its flag; every path follows \
                     `--database` or `--anchor`"
                    .to_owned());
            }
        };
        if slot.is_some() {
            return Err(format!(
                "`{flag}` was given twice; it takes exactly one value"
            ));
        }
        let value = remaining
            .next()
            .ok_or_else(|| format!("`{flag}` needs a path after it"))?;
        *slot = Some(PathBuf::from(value));
    }

    let database = database.ok_or_else(|| "`--database` is required".to_owned())?;
    Ok(Invocation::Verify { database, anchor })
}

// ─────────────────────────────────────────────────────────────────────────────
// The anchor file
// ─────────────────────────────────────────────────────────────────────────────

/// One anchor, as read off a disk.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Anchor {
    /// The register this anchor is about. Known even when it anchors nothing,
    /// because that is what puts a register with no rows left back into the
    /// set of chains to walk — and, there, what lets that register's own block
    /// say the anchor was for it.
    register: RegisterId,
    /// What [`verify_chain`] is given. `None` when `last_seq` was zero.
    ///
    /// [`verify_chain`]: pos_domain::verify_chain
    chain_anchor: Option<ChainAnchor>,
    /// Provenance: which of `audit_checkpoint`'s three sources wrote it.
    source_kind: Option<&'static str>,
    /// Provenance: when. A parsed instant, never the file's own characters.
    anchored_at: Option<Timestamp>,
}

fn parse_anchor(text: &str) -> Result<Anchor, String> {
    let document: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("not JSON: {error}"))?;
    let object = document
        .as_object()
        .ok_or_else(|| "the anchor must be one JSON object".to_owned())?;

    let register_id = required_string(object, "register_id")?;
    let register = Uuid::parse_str(register_id)
        .map_err(|error| format!("`register_id` is not a UUID: {error}"))?;

    let last_seq = object
        .get("last_seq")
        .ok_or_else(|| "`last_seq` is required".to_owned())?
        .as_u64()
        .ok_or_else(|| "`last_seq` must be a whole number, zero or above".to_owned())?;

    let last_hash = parse_digest(required_string(object, "last_hash")?)?;

    let register = RegisterId::from_uuid(register);
    Ok(Anchor {
        register,
        // Zero is a register that had written nothing when the anchor was
        // taken. See this file's header for why passing it on would report
        // tampering that did not happen.
        chain_anchor: (last_seq > 0).then_some(ChainAnchor {
            register_id: register,
            seq: last_seq,
            hash: last_hash,
        }),
        source_kind: source_kind(object)?,
        anchored_at: anchored_at(object)?,
    })
}

/// `audit_checkpoint.source_kind`'s own closed vocabulary (`0004:484`).
const SOURCE_KINDS: [&str; 3] = ["z_report", "verified_backup", "server"];

/// The anchor's `source_kind`, matched against that vocabulary.
///
/// **Nothing raw from the anchor file reaches the report**, and this is the
/// field that makes the rule necessary rather than tidy. The file is supplied
/// by whoever is being investigated, and the report is a document somebody
/// reads as evidence: a `source_kind` echoed byte for byte hands its author a
/// terminal, and a newline inside it is enough to forge a whole register block
/// — heading, rows read, and an `INTACT` verdict — into that document. The
/// column this field mirrors is already `CHECK (source_kind IN
/// ('z_report','verified_backup','server'))`, so the safe answer is also the
/// correct one: match it, and print the matched `&'static str`.
fn source_kind(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<&'static str>, String> {
    let Some(value) = object.get("source_kind") else {
        return Ok(None);
    };
    let text = value
        .as_str()
        .ok_or_else(|| "`source_kind` must be a string".to_owned())?;
    SOURCE_KINDS
        .into_iter()
        .find(|known| *known == text)
        .map(Some)
        .ok_or_else(|| {
            format!(
                "`source_kind` must be one of {}, which is what \
                 audit_checkpoint accepts",
                SOURCE_KINDS.join(", ")
            )
        })
}

/// The anchor's `anchored_at`, parsed and re-rendered for the same reason.
///
/// A timestamp is not a free-text field, so nothing is lost by refusing one
/// that is not an instant — and what is printed is [`Timestamp`]'s own
/// rendering of the value, not the characters that produced it.
fn anchored_at(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<Timestamp>, String> {
    let Some(value) = object.get("anchored_at") else {
        return Ok(None);
    };
    let text = value
        .as_str()
        .ok_or_else(|| "`anchored_at` must be a string".to_owned())?;
    Timestamp::parse_iso8601(text)
        .map(Some)
        .map_err(|error| format!("`anchored_at` is not an instant: {error}"))
}

fn required_string<'j>(
    object: &'j serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<&'j str, String> {
    object
        .get(key)
        .ok_or_else(|| format!("`{key}` is required"))?
        .as_str()
        .ok_or_else(|| format!("`{key}` must be a string"))
}

/// Hex in, digest out — and the rejection is what makes it a digest.
///
/// `u8::from_str_radix` accepts a leading sign, so `"+f"` is fifteen to it and
/// a string of thirty-two of those would parse into a perfectly plausible
/// digest that is not the one anybody wrote down. Every byte is checked against
/// [`u8::is_ascii_hexdigit`] before any of it is converted, which also makes
/// the two-byte slicing below total: after that check the string is ASCII, so
/// its byte length is its character count.
fn parse_digest(text: &str) -> Result<[u8; DIGEST_BYTES], String> {
    if text.len() != DIGEST_HEX {
        return Err(format!(
            "`last_hash` is {} bytes; a chain digest is {DIGEST_HEX} hex characters",
            text.len()
        ));
    }
    if !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("`last_hash` holds a character that is not a hex digit".to_owned());
    }

    let mut digest = [0u8; DIGEST_BYTES];
    for (index, slot) in digest.iter_mut().enumerate() {
        let pair = text
            .get(index * 2..index * 2 + 2)
            .ok_or_else(|| "`last_hash` is not a run of hex pairs".to_owned())?;
        *slot = u8::from_str_radix(pair, 16)
            .map_err(|_| "`last_hash` holds a character that is not a hex digit".to_owned())?;
    }
    Ok(digest)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        use std::fmt::Write as _;
        // Writing to a String is infallible; the Result belongs to the trait.
        let _ = write!(text, "{byte:02x}");
        text
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// The run
// ─────────────────────────────────────────────────────────────────────────────

fn refuse(reason: &str) -> Verdict {
    eprintln!("verify-audit: {reason}");
    Verdict::CouldNotRun
}

fn verify(database: &Path, anchor_path: Option<&Path>) -> Verdict {
    let anchor = match anchor_path {
        None => None,
        Some(path) => match std::fs::read_to_string(path) {
            Err(error) => {
                return refuse(&format!("cannot read {}: {error}", path.display()));
            }
            Ok(text) => match parse_anchor(&text) {
                Err(reason) => {
                    return refuse(&format!(
                        "{} is not a usable anchor: {reason}",
                        path.display()
                    ));
                }
                Ok(anchor) => Some(anchor),
            },
        },
    };

    // A verifier must never create the thing it was asked to verify.
    // `Connection::open` creates a missing file, `pos_db::open` then migrates
    // the empty result to the current schema, and the report that comes back
    // reads "no audit rows" and exits 0 — so a mistyped path would answer
    // *verified* about a database that has never existed. That is the worst
    // wrong answer this tool can give, and it costs one question to refuse.
    //
    // Deliberately not a security boundary: the file can be removed between
    // this check and the open. It closes a typo, not a race.
    if !database.is_file() {
        return refuse(&format!(
            "{} is not an existing file, and a verifier does not create the \
             database it was asked to read",
            database.display()
        ));
    }

    let (key, key_source) = match pos_db::key::get_or_create_with_source() {
        Ok(pair) => pair,
        Err(error) => return refuse(&format!("no database key: {error}")),
    };

    let connection = match pos_db::open(database, &key) {
        Ok(connection) => connection,
        Err(error) => {
            return refuse(&format!("cannot open {}: {error}", database.display()));
        }
    };

    println!("verify-audit — audit hash chain report");
    println!("  database    {}", database.display());
    println!("  key source  {}", describe_key_source(key_source));
    print_anchor(anchor.as_ref());

    let repository = AuditRepository::new(&connection);
    let found = match repository.registers() {
        Ok(found) => found,
        Err(error) => return refuse(&format!("cannot list the registers: {error}")),
    };

    let mut tills: BTreeSet<RegisterId> = found.registers().iter().copied().collect();
    if let Some(anchor) = anchor.as_ref() {
        tills.insert(anchor.register);
    }

    let mut verdict = Verdict::Verified;

    if found.orphan_rows() > 0 {
        println!();
        println!(
            "  unattributed  {} row(s) carry a register_id that is not an id; \
             they belong to no chain and were not verified",
            found.orphan_rows()
        );
        verdict = verdict.worse(Verdict::Inconclusive);
    }

    if tills.is_empty() {
        println!();
        println!("  no audit rows, and no anchor to say whether there ever were any");
    }

    for till in tills {
        println!();
        println!("register {till}");
        verdict = verdict.worse(report_register(&repository, till, anchor.as_ref()));
    }

    println!();
    println!("summary: {} (exit {})", verdict.headline(), verdict.code());
    verdict
}

fn report_register(
    repository: &AuditRepository<'_>,
    till: RegisterId,
    anchor: Option<&Anchor>,
) -> Verdict {
    let chain = match repository.chain(till) {
        Ok(chain) => chain,
        Err(error) => {
            println!("  read        FAILED: {error}");
            return Verdict::CouldNotRun;
        }
    };

    println!("  rows read   {}", chain.entries().len());

    // Only an anchor for *this* register says anything about it. `verify_chain`
    // discards a foreign one itself; saying so here is what stops a reader
    // assuming the anchor line at the top of the report covered every register
    // under it.
    let names_this_register = anchor.is_some_and(|anchor| anchor.register == till);
    let applicable = anchor
        .filter(|anchor| anchor.register == till)
        .and_then(|anchor| anchor.chain_anchor);
    let withheld =
        applicable.is_some_and(|candidate| above_the_stop(chain.stopped(), candidate.seq));
    let applied = if withheld { None } else { applicable };
    println!(
        "  anchor      {}",
        match applied {
            Some(anchor) => format!("last_seq {}", anchor.seq),
            None if withheld => "WITHHELD — it records a row at or above the stop below, which \
                 this read never reached, so applying it would report a deleted \
                 tail over rows that are still on the disk"
                .to_owned(),
            // "no anchor" and "an anchor that anchors nothing here" are
            // different sentences, and only this block can tell them apart —
            // which is also why the header no longer repeats the register the
            // anchor names. One line says what was supplied; this one says what
            // it was worth against this chain.
            None if names_this_register =>
                "for this register, recording it as having written no audit row \
                 — so it anchors nothing here"
                    .to_owned(),
            None => "none for this register".to_owned(),
        }
    );

    let verdict = pos_domain::verify_chain(till, chain.verifier_rows(), applied);
    println!("  verdict     {}", describe_verdict(verdict));

    let mut outcome = match verdict {
        ChainVerdict::Intact { .. } | ChainVerdict::IntactUnanchoredFrom { .. } => {
            Verdict::Verified
        }
        ChainVerdict::Broken { .. } | ChainVerdict::Truncated { .. } => Verdict::Tampered,
    };

    // The stop comes last and never replaces the verdict above it: the rows
    // below the stop were read and walked, and their verdict stands over them.
    if let Some(stop) = chain.stopped() {
        println!(
            "  stopped     at seq {}: {} — every row above it is unread, \
             and the verdict above covers only those below",
            stop.seq, stop.reason
        );
        outcome = outcome.worse(Verdict::Inconclusive);
    }

    outcome
}

/// Whether `seq` names a row the read never reached.
///
/// This is the one place where a *correct* anchor and a *correct* stop combine
/// into a wrong answer, and it is worth spelling out because no mutation of
/// either half can find it. A read that stops at seq 3 hands `verify_chain` the
/// rows below it. Given an anchor at seq 5, `verify_chain` sees a
/// self-consistent chain ending at 2, does not find the anchored row, and
/// answers `Truncated { anchored_seq: 5, found_seq: 2 }` — *the rows between
/// were removed*. They were not. Rows 3, 4 and 5 are still there; this build
/// could not rebuild one of them, which is an upgrade far more often than an
/// attack.
///
/// So the anchor is withheld, and the run stays inconclusive rather than
/// becoming a tamper verdict. An anchor **below** the stop is untouched: it is
/// entirely inside the prefix that was read, and it still says exactly what it
/// always said. A stop at a row numbered zero or less sorts first, so the walk
/// read nothing and no anchor can be below it.
fn above_the_stop(stop: Option<&ChainStop>, seq: u64) -> bool {
    let Some(stop) = stop else { return false };
    match u64::try_from(stop.seq) {
        Ok(stopped_at) => seq >= stopped_at,
        Err(_) => true,
    }
}

fn print_anchor(anchor: Option<&Anchor>) {
    let Some(anchor) = anchor else {
        println!(
            "  anchor      none supplied — a chain cannot see its own tail, so \
             deletion of the newest rows is indistinguishable from nothing \
             having happened"
        );
        return;
    };

    // Which register it is for is stated once, in that register's own block
    // below, beside the verdict it did or did not contribute to — and every
    // register the anchor names is walked, so there is always such a block.
    // Naming it here as well invited a reader to match a top-line anchor
    // against the wrong section of a two-register report.
    match anchor.chain_anchor {
        Some(chain_anchor) => println!(
            "  anchor      last_seq {}, last_hash {}",
            chain_anchor.seq,
            hex(&chain_anchor.hash)
        ),
        None => println!(
            "  anchor      last_seq 0 — it records a register that had written \
             no audit row, so it anchors nothing"
        ),
    }
    if let Some(kind) = anchor.source_kind {
        println!("  anchor from {kind}");
    }
    if let Some(at) = anchor.anchored_at {
        println!("  anchor at   {}", at.to_iso8601());
    }
}

fn describe_key_source(source: KeySource) -> &'static str {
    match source {
        KeySource::Environment => "POS_DB_KEY (a debug build only; never a release one)",
        KeySource::CredentialStore => "the OS credential store",
    }
}

fn describe_verdict(verdict: ChainVerdict) -> String {
    match verdict {
        ChainVerdict::Intact { entries } => {
            format!("INTACT — {entries} entries, anchored through the head")
        }
        ChainVerdict::Broken { at_seq } => format!(
            "BROKEN at seq {at_seq} — this row does not agree with the chain, \
             and a forensic investigation starts here"
        ),
        ChainVerdict::Truncated {
            anchored_seq,
            found_seq,
        } => format!(
            "TRUNCATED — the anchor records seq {anchored_seq} as the head and \
             the highest row present is {found_seq}; the rows between them were \
             removed"
        ),
        ChainVerdict::IntactUnanchoredFrom {
            entries,
            unanchored_from: 0,
        } => format!(
            "INTACT SO FAR — {entries} entries agree with each other, and no \
             anchor covers any of them, so nothing here rules out a deleted tail"
        ),
        ChainVerdict::IntactUnanchoredFrom {
            entries,
            unanchored_from,
        } => format!(
            "INTACT SO FAR — {entries} entries, anchored below seq \
             {unanchored_from}; nothing covers seq {unanchored_from} and above"
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::{Anchor, DIGEST_BYTES, Invocation, Timestamp, parse, parse_anchor, parse_digest};
    use std::ffi::OsString;

    fn words(line: &[&str]) -> Vec<OsString> {
        line.iter().map(OsString::from).collect()
    }

    fn anchor(text: &str) -> Result<Anchor, String> {
        parse_anchor(text)
    }

    const REGISTER: &str = "f0f0f0f0-f0f0-f0f0-f0f0-f0f0f0f0f0f0";
    const DIGEST: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    #[test]
    fn an_unknown_argument_is_refused() {
        let unknown = parse(words(&["--database", "a.db", "--verbose"])).unwrap_err();
        assert!(unknown.contains("--verbose"), "{unknown}");

        // A bare value is refused too, and the message names the shape rather
        // than the token: argv is where a key gets typed by mistake.
        let bare = parse(words(&["a.db"])).unwrap_err();
        assert!(bare.contains("without its flag"), "{bare}");
        assert!(
            !bare.contains("a.db"),
            "the token must not be echoed: {bare}"
        );
    }

    #[test]
    fn a_flag_without_its_value_is_refused() {
        for line in [vec!["--database"], vec!["--database", "a.db", "--anchor"]] {
            let refusal = parse(words(&line)).unwrap_err();
            assert!(refusal.contains("needs a path"), "{refusal}");
        }
        // And the whole thing is refused when `--database` never appears.
        let missing = parse(words(&["--anchor", "a.json"])).unwrap_err();
        assert!(missing.contains("`--database` is required"), "{missing}");
    }

    #[test]
    fn a_repeated_flag_is_refused() {
        // The failure this closes is silent: most parsers take the last value,
        // so `--database evidence.db --database scratch.db` would verify the
        // wrong file and report on it in the name of the first.
        let twice = parse(words(&["--database", "a.db", "--database", "b.db"])).unwrap_err();
        assert!(twice.contains("given twice"), "{twice}");
    }

    #[test]
    fn help_beats_every_other_argument() {
        for line in [
            vec!["--help"],
            vec!["-h"],
            vec!["--database", "a.db", "--help"],
            // Including a line that is otherwise a refusal.
            vec!["--nonsense", "--help"],
        ] {
            assert_eq!(parse(words(&line)).unwrap(), Invocation::Help, "{line:?}");
        }

        let verify = parse(words(&["--database", "a.db"])).unwrap();
        assert_eq!(
            verify,
            Invocation::Verify {
                database: "a.db".into(),
                anchor: None
            }
        );
    }

    #[test]
    fn a_digest_must_be_sixty_four_hex_characters() {
        assert_eq!(parse_digest(DIGEST).unwrap().len(), DIGEST_BYTES);
        assert!(parse_digest("").is_err());
        assert!(parse_digest(&DIGEST[..62]).is_err());
        assert!(parse_digest(&format!("{DIGEST}00")).is_err());
        assert!(parse_digest(&DIGEST.replace('9', "z")).is_err());
        // Sixty-four bytes of two-byte characters is not sixty-four hex
        // characters, and the byte-length check alone would let it through.
        assert!(parse_digest(&"é".repeat(32)).is_err());
        // Upper case is the same digest. A manifest written by another tool
        // must not be refused over a case convention.
        assert_eq!(
            parse_digest(&DIGEST.to_uppercase()).unwrap(),
            parse_digest(DIGEST).unwrap()
        );
    }

    #[test]
    fn a_signed_hex_pair_is_not_a_digest() {
        // `u8::from_str_radix("+f", 16)` is fifteen, so without the hex-digit
        // check this would parse into a plausible digest nobody wrote down.
        let signed = "+f".repeat(32);
        assert_eq!(signed.len(), 64);
        let refusal = parse_digest(&signed).unwrap_err();
        assert!(refusal.contains("not a hex digit"), "{refusal}");
    }

    #[test]
    fn an_anchor_needs_all_three_required_keys() {
        let complete =
            format!(r#"{{"register_id":"{REGISTER}","last_seq":4,"last_hash":"{DIGEST}"}}"#);
        let parsed = anchor(&complete).unwrap();
        assert_eq!(parsed.chain_anchor.unwrap().seq, 4);
        assert_eq!(parsed.source_kind, None);
        assert_eq!(parsed.anchored_at, None);

        for (missing, without) in [
            (
                "register_id",
                format!(r#"{{"last_seq":4,"last_hash":"{DIGEST}"}}"#),
            ),
            (
                "last_seq",
                format!(r#"{{"register_id":"{REGISTER}","last_hash":"{DIGEST}"}}"#),
            ),
            (
                "last_hash",
                format!(r#"{{"register_id":"{REGISTER}","last_seq":4}}"#),
            ),
        ] {
            let refusal = anchor(&without).unwrap_err();
            assert!(refusal.contains(missing), "{missing}: {refusal}");
        }

        // A negative or fractional sequence is not a chain position.
        for bad in ["-1", "1.5"] {
            let text = format!(
                r#"{{"register_id":"{REGISTER}","last_seq":{bad},"last_hash":"{DIGEST}"}}"#
            );
            assert!(anchor(&text).is_err(), "last_seq {bad} must be refused");
        }

        // Zero is accepted and anchors nothing — see the header.
        let empty =
            format!(r#"{{"register_id":"{REGISTER}","last_seq":0,"last_hash":"{DIGEST}"}}"#);
        let parsed = anchor(&empty).unwrap();
        assert_eq!(parsed.chain_anchor, None);
        assert_eq!(parsed.register.to_string(), REGISTER);

        // Provenance is read and carried; an unknown key is ignored, so a later
        // checkpoint format stays readable by this build.
        let provenanced = format!(
            r#"{{"register_id":"{REGISTER}","last_seq":4,"last_hash":"{DIGEST}",
                 "source_kind":"verified_backup","anchored_at":"2026-09-21T10:00:00.000Z",
                 "anchor_ref":"backup-0007"}}"#
        );
        let parsed = anchor(&provenanced).unwrap();
        assert_eq!(parsed.source_kind, Some("verified_backup"));
        assert_eq!(
            parsed.anchored_at.map(Timestamp::to_iso8601).as_deref(),
            Some("2026-09-21T10:00:00.000Z")
        );

        // Provenance is validated, not echoed. A newline inside `source_kind`
        // would otherwise let the anchor's author forge a register block and an
        // INTACT verdict into the middle of a report read as evidence.
        for forged in [
            "verified_backup\n\nregister f0f0\n  verdict     INTACT",
            "\u{1b}[2Jz_report",
            "Z_REPORT",
            "",
        ] {
            let text = format!(
                r#"{{"register_id":"{REGISTER}","last_seq":4,"last_hash":"{DIGEST}",
                     "source_kind":{}}}"#,
                serde_json::Value::String(forged.to_owned())
            );
            let refusal = anchor(&text).unwrap_err();
            assert!(
                refusal.contains("`source_kind` must be one of"),
                "{refusal}"
            );
        }

        let not_an_instant = format!(
            r#"{{"register_id":"{REGISTER}","last_seq":4,"last_hash":"{DIGEST}",
                 "anchored_at":"yesterday\nregister f0f0"}}"#
        );
        let refusal = anchor(&not_an_instant).unwrap_err();
        assert!(
            refusal.contains("`anchored_at` is not an instant"),
            "{refusal}"
        );

        // And a document that is not one object is refused before any key is.
        assert!(anchor("[]").is_err());
        assert!(anchor("not json at all").is_err());
    }
}
