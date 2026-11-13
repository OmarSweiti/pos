//! Encrypted backup (microstep 1.8.6, its first part): verified generations of
//! the register's database, each with the recovery envelope beside it.
//!
//! A register holds the only copy of its sales in Phase 1, so a backup is the
//! difference between a lost machine and a lost business (gap G-1).
//!
//! **A generation** is a directory under the destination, named for the
//! instant it was taken:
//!
//! ```text
//! <destination>/2026-11-13T17-00-00.000Z/
//!     register.db              the database, SQLCipher, keyed with the live data key
//!     recovery-envelope.json   the register's envelope, which the recovery code opens
//!     backup.json              its manifest, which is also its audit anchor
//! ```
//!
//! **Taken, verified, then published.** [`snapshot`] copies the database
//! through SQLite's online backup API on the register's own connection into a
//! staging directory, keyed with the live data key before the first page is
//! written, so no plaintext copy ever exists. It reads the audit head from the
//! copy, writes the envelope and the manifest beside it, verifies the whole
//! generation with the live key ([`verify`]), makes every file and the
//! directory durable, and only then publishes it by renaming the staging
//! directory to its generation's name. A generation that does not verify is
//! never published, and nothing published is ever replaced: each run is a new
//! name.
//!
//! **One source connection.** Until 1.8.0's storage gate allows a second, the
//! copy runs on the connection that writes the register. It goes a few pages
//! at a time, and between steps asks its caller whether to go on
//! ([`Step`]). A write on the same connection between steps is carried into
//! the copy by SQLite itself; a caller that has a sale waiting answers
//! [`Step::GiveWay`], and the copy is abandoned, its staging removed, and
//! tried again later.
//!
//! **The manifest is the anchor.** It carries `register_id`, `last_seq`,
//! `last_hash`, `source_kind` and `anchored_at` under `audit_checkpoint`'s own
//! names, so `verify-audit --anchor <generation>/backup.json` reads it as it
//! stands (security-compliance §4, "the anchored checkpoint").
//!
//! **The live key, never the recovery code.** An hourly job has the data key
//! from the credential store and nothing else: [`snapshot`] and [`verify`]
//! take a [`DatabaseKey`]. Only a restore on a clean machine uses the code,
//! through the envelope that travels with every generation.
//!
//! **Retention** ([`prune`]) is merchant decision 13.3: one generation per hour
//! for a day, one per day for thirty days, and always the newest.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use pos_domain::{RegisterId, Timestamp};
use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::key::DatabaseKey;
use crate::recovery::{ENVELOPE_FILE, RecoveryEnvelope};
use crate::repo::audit::AuditRepository;
use crate::{DbError, migrations, verify_runtime_schema};

/// The database inside a generation.
pub const DATABASE_FILE: &str = "register.db";
/// A generation's manifest, which is also its audit anchor.
pub const MANIFEST_FILE: &str = "backup.json";
/// The manifest format this build writes and reads.
pub const BACKUP_FORMAT: u32 = 1;
/// The `audit_checkpoint.source_kind` a verified backup anchors under.
pub const SOURCE_KIND: &str = "verified_backup";
/// A generation being taken, not yet verified: `<prefix><generation>`.
pub const STAGING_PREFIX: &str = ".staging-";
/// SQLCipher 4's page size, which every register database is created with.
const PAGE_BYTES: u64 = 4096;
/// Pages copied between two questions to the caller. 64 pages of 4 KiB is a
/// quarter of a megabyte: a step a sale never notices waiting behind.
pub const PAGES_PER_STEP: i32 = 64;

const HOUR_MS: i64 = 60 * 60 * 1000;
const DAY_MS: i64 = 24 * HOUR_MS;
/// Merchant decision 13.3: hourly generations are kept for a day.
pub const HOURLY_FOR_MS: i64 = DAY_MS;
/// Merchant decision 13.3: daily generations are kept for thirty days.
pub const DAILY_FOR_MS: i64 = 30 * DAY_MS;

/// What the copy's caller answers between steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Copy the next pages.
    GoOn,
    /// A write is waiting for the connection: abandon this copy.
    GiveWay,
}

/// Why a backup was not taken, or does not verify. None of them carries a key,
/// a code or a path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackupError {
    /// The caller had a write waiting, and the copy gave way to it. Nothing was
    /// published; the next run starts again.
    #[error("the backup gave way to a write waiting for the database; nothing was published")]
    GaveWay,
    /// The envelope is for another key, so a generation it travelled with could
    /// not be opened with the recovery code.
    #[error("the recovery envelope is for another key than the register's")]
    EnvelopeForAnotherKey,
    /// A generation of this name already exists. Nothing published is replaced.
    #[error("a generation of this name already exists, and is never replaced")]
    GenerationExists,
    /// The generation does not verify, and why.
    #[error("the backup does not verify: {0}")]
    Unverified(&'static str),
    /// The file system refused.
    #[error("backup file system: {0}")]
    Io(String),
}

impl From<std::io::Error> for BackupError {
    fn from(error: std::io::Error) -> Self {
        BackupError::Io(error.kind().to_string())
    }
}

/// A generation's manifest: what it holds, and the audit head it anchors,
/// under `audit_checkpoint`'s column names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupManifest {
    /// [`BACKUP_FORMAT`].
    pub format: u32,
    /// The generation's name, which is its directory's.
    pub generation: String,
    /// The register whose audit head this anchors.
    pub register_id: String,
    /// The register's audit head in the copy: `0` when it had written none.
    pub last_seq: u64,
    /// That row's hash, in hex; the genesis digest when `last_seq` is `0`.
    pub last_hash: String,
    /// [`SOURCE_KIND`].
    pub source_kind: String,
    /// When the copy was taken.
    pub anchored_at: String,
    /// [`DatabaseKey::id`] of the key the copy is encrypted with.
    pub key_id: String,
    /// The migrations the copy's schema carries.
    pub schema_version: u64,
    /// The size of [`DATABASE_FILE`], in bytes.
    pub database_bytes: u64,
    /// The BLAKE3 digest of [`DATABASE_FILE`], in hex.
    pub database_blake3: String,
}

impl BackupManifest {
    /// The audit head this manifest anchors, as bytes: `None` when its hash is
    /// not a digest.
    #[must_use]
    pub fn head(&self) -> Option<(u64, [u8; 32])> {
        let text = self.last_hash.as_bytes();
        if text.len() != 64 {
            return None;
        }
        let mut hash = [0u8; 32];
        for (byte, pair) in hash.iter_mut().zip(text.chunks_exact(2)) {
            let pair = std::str::from_utf8(pair).ok()?;
            *byte = u8::from_str_radix(pair, 16).ok()?;
        }
        Some((self.last_seq, hash))
    }
}

/// A published, verified generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupInfo {
    /// Its directory.
    pub path: PathBuf,
    pub manifest: BackupManifest,
}

/// The generation name for `at`: its instant, with the colons a Windows file
/// name cannot hold made hyphens. Names of one width sort as their instants.
#[must_use]
pub fn generation_name(at: Timestamp) -> String {
    at.to_iso8601().replace(':', "-")
}

/// The instant a generation name records, or `None` when it is not one.
#[must_use]
pub fn generation_instant(name: &str) -> Option<Timestamp> {
    let bytes = name.as_bytes();
    if bytes.get(13) != Some(&b'-') || bytes.get(16) != Some(&b'-') {
        return None;
    }
    let mut iso = name.to_owned();
    iso.replace_range(13..14, ":");
    iso.replace_range(16..17, ":");
    Timestamp::parse_iso8601(&iso)
        .ok()
        .filter(|at| generation_name(*at) == name)
}

/// Take a backup of the database on `conn` into a new generation under
/// `destination`, taken at `at`, for `register`, encrypted with `data_key`
/// and carrying `envelope`: [`copy`], then [`StagedCopy::publish`].
/// `between_steps` is asked after every [`PAGES_PER_STEP`] pages whether to
/// go on.
///
/// A caller that shares `conn` with others, as the till does, calls the two
/// halves itself, and lets the connection go between them: only the copy
/// needs it.
///
/// # Errors
///
/// As [`copy`] and [`StagedCopy::publish`]. Nothing is published on any of
/// them, and no staging is left.
pub fn snapshot(
    conn: &Connection,
    register: RegisterId,
    data_key: &DatabaseKey,
    envelope: &RecoveryEnvelope,
    destination: &Path,
    at: Timestamp,
    between_steps: &mut dyn FnMut() -> Step,
) -> Result<BackupInfo, DbError> {
    copy(
        conn,
        register,
        data_key,
        envelope,
        destination,
        at,
        between_steps,
    )?
    .publish(data_key, envelope)
}

/// A copy of the database in its staging directory, not yet verified or
/// published. Dropped unpublished, its staging is removed.
#[derive(Debug)]
pub struct StagedCopy {
    staging: PathBuf,
    published: PathBuf,
    generation: String,
    register: RegisterId,
    at: Timestamp,
    done: bool,
}

impl Drop for StagedCopy {
    fn drop(&mut self) {
        if !self.done {
            let _ = fs::remove_dir_all(&self.staging);
        }
    }
}

/// Copy the database on `conn` into the staging directory of a new
/// generation under `destination`, keyed with `data_key` before the first
/// page is written. Only this half reads `conn`.
///
/// # Errors
///
/// [`BackupError::EnvelopeForAnotherKey`] before anything is copied;
/// [`BackupError::GaveWay`] when `between_steps` answered [`Step::GiveWay`];
/// [`BackupError::GenerationExists`]; [`BackupError::Io`]; and the
/// database's own refusals.
pub fn copy(
    conn: &Connection,
    register: RegisterId,
    data_key: &DatabaseKey,
    envelope: &RecoveryEnvelope,
    destination: &Path,
    at: Timestamp,
    between_steps: &mut dyn FnMut() -> Step,
) -> Result<StagedCopy, DbError> {
    if envelope.key_id != data_key.id() {
        return Err(BackupError::EnvelopeForAnotherKey.into());
    }
    let generation = generation_name(at);
    let published = destination.join(&generation);
    if published.try_exists().map_err(BackupError::from)? {
        return Err(BackupError::GenerationExists.into());
    }
    fs::create_dir_all(destination).map_err(BackupError::from)?;
    let staging = destination.join(format!("{STAGING_PREFIX}{generation}"));
    if staging.try_exists().map_err(BackupError::from)? {
        fs::remove_dir_all(&staging).map_err(BackupError::from)?;
    }
    fs::create_dir(&staging).map_err(BackupError::from)?;
    let staged = StagedCopy {
        staging,
        published,
        generation,
        register,
        at,
        done: false,
    };

    let mut copy = Connection::open(staged.staging.join(DATABASE_FILE))?;
    // SQLCipher: the key is the first statement, so not one page of the copy
    // is ever written in the clear.
    copy.pragma_update(None, "key", data_key.expose())?;
    {
        let backup = Backup::new(conn, &mut copy)?;
        loop {
            match backup.step(PAGES_PER_STEP)? {
                StepResult::Done => break,
                StepResult::More => {}
                // The register's one connection is the source, so another
                // holder of a lock is a caller's own write in progress.
                StepResult::Busy | StepResult::Locked => {
                    return Err(BackupError::GaveWay.into());
                }
                _ => return Err(BackupError::Unverified("the copy stopped part-way").into()),
            }
            if between_steps() == Step::GiveWay {
                return Err(BackupError::GaveWay.into());
            }
        }
    }
    // A backup is one file: no write-ahead log beside it.
    let mode: String = copy.query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("delete") {
        return Err(BackupError::Unverified("the copy kept a write-ahead log").into());
    }
    Ok(staged)
}

impl StagedCopy {
    /// The generation this copy will be published as.
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }

    /// Read the copy's schema and audit head, write the envelope and the
    /// manifest beside it, verify the whole generation with the live key
    /// ([`verify`]'s checks), make it durable, and publish it under its own
    /// name. Needs no connection to the register.
    ///
    /// # Errors
    ///
    /// [`BackupError::Unverified`] when the copy does not verify;
    /// [`BackupError::GenerationExists`]; [`BackupError::Io`]; and the
    /// database's own refusals. Nothing is published on any of them, and the
    /// staging is removed.
    pub fn publish(
        mut self,
        data_key: &DatabaseKey,
        envelope: &RecoveryEnvelope,
    ) -> Result<BackupInfo, DbError> {
        let database = self.staging.join(DATABASE_FILE);
        let (schema, (last_seq, last_hash)) = {
            let copy = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            copy.pragma_update(None, "key", data_key.expose())?;
            let schema = verify_runtime_schema(&copy, migrations())?;
            let head = AuditRepository::new(&copy)
                .head(self.register)?
                .unwrap_or((0, pos_domain::audit::GENESIS));
            (schema, head)
        };
        let (database_bytes, database_blake3) = measure(&database)?;
        File::open(&database)
            .and_then(|file| file.sync_all())
            .map_err(BackupError::from)?;
        envelope
            .write(&self.staging.join(ENVELOPE_FILE))
            .map_err(|_| {
                BackupError::Io("the recovery envelope could not be written".to_owned())
            })?;
        let manifest = BackupManifest {
            format: BACKUP_FORMAT,
            generation: self.generation.clone(),
            register_id: self.register.as_uuid().to_string(),
            last_seq,
            last_hash: hex(&last_hash),
            source_kind: SOURCE_KIND.to_owned(),
            anchored_at: self.at.to_iso8601(),
            key_id: data_key.id(),
            schema_version: u64::try_from(schema.applied()).unwrap_or(u64::MAX),
            database_bytes,
            database_blake3,
        };
        write_durably(&self.staging.join(MANIFEST_FILE), &manifest)?;

        let manifest = check(&self.staging, data_key, envelope)?;
        sync_dir(&self.staging)?;
        if self.published.try_exists().map_err(BackupError::from)? {
            return Err(BackupError::GenerationExists.into());
        }
        fs::rename(&self.staging, &self.published).map_err(BackupError::from)?;
        self.done = true;
        if let Some(destination) = self.published.parent() {
            sync_dir(destination)?;
        }
        Ok(BackupInfo {
            path: self.published.clone(),
            manifest,
        })
    }
}

/// Verify the generation at `path` with the live `data_key`: the file is the
/// one its manifest records, whole; it opens with the live key and every page
/// authenticates; its schema is its version's; its audit head is the one its
/// manifest anchors; and it carries `envelope`, for the same key.
///
/// # Errors
///
/// [`BackupError::Unverified`], naming what does not hold; and
/// [`DbError::SchemaShapeMismatch`] and its kin for a schema that is not
/// its version's.
pub fn verify(
    path: &Path,
    data_key: &DatabaseKey,
    envelope: &RecoveryEnvelope,
) -> Result<BackupInfo, DbError> {
    let manifest = check(path, data_key, envelope)?;
    let name = path.file_name().and_then(|name| name.to_str());
    if name != Some(manifest.generation.as_str()) {
        return Err(
            BackupError::Unverified("the directory is not its manifest's generation").into(),
        );
    }
    Ok(BackupInfo {
        path: path.to_path_buf(),
        manifest,
    })
}

/// Every check [`verify`] makes, on a published or a staged generation.
fn check(
    path: &Path,
    data_key: &DatabaseKey,
    envelope: &RecoveryEnvelope,
) -> Result<BackupManifest, DbError> {
    let unverified = |reason| DbError::from(BackupError::Unverified(reason));
    let manifest = read_manifest(&path.join(MANIFEST_FILE))?;
    if manifest.format != BACKUP_FORMAT || manifest.source_kind != SOURCE_KIND {
        return Err(unverified("its manifest is not one this build writes"));
    }
    if generation_instant(&manifest.generation).is_none() {
        return Err(unverified("its manifest names no generation"));
    }
    let key_id = data_key.id();
    if manifest.key_id != key_id {
        return Err(unverified("it is encrypted with another key"));
    }
    let carried = RecoveryEnvelope::read(&path.join(ENVELOPE_FILE))
        .map_err(|_| unverified("it carries no readable recovery envelope"))?;
    if carried != *envelope || carried.key_id != key_id {
        return Err(unverified("it carries another recovery envelope"));
    }

    let database = path.join(DATABASE_FILE);
    let (bytes, digest) = measure(&database)?;
    if bytes != manifest.database_bytes {
        return Err(unverified(
            "its database is not the size its manifest records",
        ));
    }
    if digest != manifest.database_blake3 {
        return Err(unverified(
            "its database is not the one its manifest records",
        ));
    }

    let copy = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    copy.pragma_update(None, "key", data_key.expose())?;
    // Every page's authentication first, before the schema is read: a wrong
    // key fails every page, and damage fails only the pages it touched, so
    // the two are told apart rather than both called a wrong key.
    let failures: Vec<String> = copy
        .prepare("PRAGMA cipher_integrity_check")
        .and_then(|mut check| {
            check
                .query_map([], |row| row.get(0))?
                .collect::<Result<_, _>>()
        })
        .map_err(|_| unverified("it does not open with the live key"))?;
    let pages = bytes.div_ceil(PAGE_BYTES);
    if !failures.is_empty() {
        let every_page = u64::try_from(failures.len()).is_ok_and(|failed| failed >= pages);
        return Err(unverified(if every_page {
            "it does not open with the live key"
        } else {
            "a page of it fails its authentication"
        }));
    }
    copy.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(|_| unverified("it does not open with the live key"))?;
    let quick: String = copy.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if quick != "ok" {
        return Err(unverified("its database is not well formed"));
    }
    let schema = verify_runtime_schema(&copy, migrations())?;
    if u64::try_from(schema.applied()).ok() != Some(manifest.schema_version) {
        return Err(unverified(
            "its schema is not the version its manifest records",
        ));
    }
    let register = uuid::Uuid::parse_str(&manifest.register_id)
        .map(RegisterId::from_uuid)
        .map_err(|_| unverified("its manifest names no register"))?;
    let (last_seq, last_hash) = AuditRepository::new(&copy)
        .head(register)?
        .unwrap_or((0, pos_domain::audit::GENESIS));
    if last_seq != manifest.last_seq || hex(&last_hash) != manifest.last_hash {
        return Err(unverified(
            "its audit head is not the one its manifest anchors",
        ));
    }
    Ok(manifest)
}

/// The published generations under `destination`, oldest first: every
/// directory whose name is a generation and whose manifest reads. Anything
/// else there is not this module's, and is left alone.
///
/// # Errors
///
/// [`BackupError::Io`] when the destination cannot be read.
pub fn generations(destination: &Path) -> Result<Vec<BackupInfo>, DbError> {
    let mut found = Vec::new();
    let entries = match fs::read_dir(destination) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(error) => return Err(BackupError::from(error).into()),
    };
    for entry in entries {
        let entry = entry.map_err(BackupError::from)?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if generation_instant(&name).is_none() || !entry.path().is_dir() {
            continue;
        }
        let Ok(manifest) = read_manifest(&entry.path().join(MANIFEST_FILE)) else {
            continue;
        };
        if manifest.generation == name {
            found.push(BackupInfo {
                path: entry.path(),
                manifest,
            });
        }
    }
    found.sort_by(|a, b| a.manifest.generation.cmp(&b.manifest.generation));
    Ok(found)
}

/// Apply merchant decision 13.3's retention to `destination` at `now`: keep
/// the newest generation of each hour of the last day, the newest of each day
/// of the last thirty, any taken after `now` (a clock that moved back is not a
/// reason to delete), and always the newest; remove every other generation,
/// and any staging a stopped run left. Returns the names removed.
///
/// # Errors
///
/// [`BackupError::Io`] when a generation cannot be listed or removed.
pub fn prune(destination: &Path, now: Timestamp) -> Result<Vec<String>, DbError> {
    let all = generations(destination)?;
    let kept = retained(
        &all.iter()
            .filter_map(|info| {
                generation_instant(&info.manifest.generation)
                    .map(|at| (info.manifest.generation.clone(), at))
            })
            .collect::<Vec<_>>(),
        now,
    );
    let mut removed = Vec::new();
    for info in &all {
        if !kept.contains(&info.manifest.generation) {
            fs::remove_dir_all(&info.path).map_err(BackupError::from)?;
            removed.push(info.manifest.generation.clone());
        }
    }
    if let Ok(entries) = fs::read_dir(destination) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let is_staging = name
                .to_str()
                .and_then(|name| name.strip_prefix(STAGING_PREFIX))
                .is_some_and(|generation| generation_instant(generation).is_some());
            if is_staging && entry.path().is_dir() {
                fs::remove_dir_all(entry.path()).map_err(BackupError::from)?;
            }
        }
    }
    if !removed.is_empty() {
        sync_dir(destination)?;
    }
    Ok(removed)
}

/// The generations of `all` that retention keeps at `now`.
fn retained(all: &[(String, Timestamp)], now: Timestamp) -> std::collections::BTreeSet<String> {
    let now_ms = now.epoch_milliseconds();
    let mut hourly: BTreeMap<i64, &String> = BTreeMap::new();
    let mut daily: BTreeMap<i64, &String> = BTreeMap::new();
    let mut kept = std::collections::BTreeSet::new();
    // Oldest first, so the newest of each bucket is the one left standing.
    let mut ordered: Vec<&(String, Timestamp)> = all.iter().collect();
    ordered.sort_by_key(|(_, at)| at.epoch_milliseconds());
    for (name, at) in &ordered {
        let at_ms = at.epoch_milliseconds();
        let age = now_ms.saturating_sub(at_ms);
        if age < 0 {
            kept.insert(name.clone());
            continue;
        }
        if age < HOURLY_FOR_MS {
            hourly.insert(at_ms.div_euclid(HOUR_MS), name);
        }
        if age < DAILY_FOR_MS {
            daily.insert(at_ms.div_euclid(DAY_MS), name);
        }
    }
    kept.extend(hourly.into_values().cloned());
    kept.extend(daily.into_values().cloned());
    if let Some((newest, _)) = ordered.last() {
        kept.insert(newest.clone());
    }
    kept
}

fn read_manifest(path: &Path) -> Result<BackupManifest, DbError> {
    let bytes =
        fs::read(path).map_err(|_| BackupError::Unverified("it has no readable manifest"))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| BackupError::Unverified("its manifest is not one this build reads").into())
}

/// Write `manifest` to `path` whole, and make it durable.
fn write_durably(path: &Path, manifest: &BackupManifest) -> Result<(), DbError> {
    let mut json = serde_json::to_vec_pretty(manifest)
        .map_err(|_| BackupError::Io("the manifest could not be written as JSON".to_owned()))?;
    json.push(b'\n');
    let mut file = File::create(path).map_err(BackupError::from)?;
    file.write_all(&json).map_err(BackupError::from)?;
    file.sync_all().map_err(BackupError::from)?;
    Ok(())
}

/// The size and BLAKE3 digest of the file at `path`.
fn measure(path: &Path) -> Result<(u64, String), DbError> {
    let mut file = File::open(path)
        .map_err(|_| BackupError::Unverified("its database file cannot be read"))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut bytes: u64 = 0;
    loop {
        let read = file.read(&mut buffer).map_err(BackupError::from)?;
        if read == 0 {
            break;
        }
        let chunk = buffer.get(..read).unwrap_or_default();
        hasher.update(chunk);
        bytes = bytes.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
    }
    Ok((bytes, hasher.finalize().to_hex().to_string()))
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Make the entries of the directory at `path` durable. Unix only: Windows
/// does not open a directory as a file, and flushes a rename with its file.
#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<(), DbError> {
    File::open(path)
        .and_then(|dir| dir.sync_all())
        .map_err(|error| BackupError::from(error).into())
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> Result<(), DbError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    //! 1.8.6's first part. Its `Done when` runs these as
    //! `cargo nextest run -p pos-db backup::`.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::collections::BTreeSet;

    use pos_domain::{AuditIntent, UserId};
    use rusqlite::{Transaction, params};
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::fixtures::registered_chain::RegisteredChain;
    use crate::recovery::{KdfParams, RecoveryCode, unwrap_data_key, wrap_data_key};
    use crate::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};

    const REGISTER: [u8; 16] = [0xF0; 16];
    const AT: &str = "2026-11-13T17:00:00.000Z";

    fn register() -> RegisterId {
        RegisterId::from_uuid(Uuid::from_bytes(REGISTER))
    }

    fn at(ms: i64) -> Timestamp {
        Timestamp::from_epoch_milliseconds(ms).unwrap()
    }

    /// 2026-11-13T17:00:00.000Z.
    fn noon() -> Timestamp {
        Timestamp::parse_iso8601(AT).unwrap()
    }

    fn key() -> DatabaseKey {
        DatabaseKey::new("the-live-data-key-of-this-register".to_owned())
    }

    /// An envelope for `key` that nothing here unwraps: every test but the
    /// one about the recovery code needs only its key id, and a real wrap
    /// costs Argon2id's 64 MiB each time.
    fn stand_in_envelope(key: &DatabaseKey) -> RecoveryEnvelope {
        RecoveryEnvelope {
            format: crate::recovery::ENVELOPE_FORMAT,
            key_id: key.id(),
            wrap_algorithm: crate::recovery::WRAP_ALGORITHM.to_owned(),
            kdf_algorithm: crate::recovery::KDF_ALGORITHM.to_owned(),
            kdf_params: KdfParams::fresh().unwrap(),
            wrapped_key: vec![0x5A; 72],
        }
    }

    /// A register's database, migrated, with its store and register rows.
    struct Register {
        conn: Connection,
        dir: tempfile::TempDir,
    }

    impl Register {
        fn new(key: &DatabaseKey) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let conn = crate::open(&dir.path().join("register.db"), key.expose()).unwrap();
            let chain = RegisteredChain::seed(&conn);
            chain.add_register(&conn, &REGISTER, "REG01");
            Self { conn, dir }
        }

        fn destination(&self) -> PathBuf {
            self.dir.path().join("backups")
        }

        /// Rows of synthetic settings, each about two kilobytes: enough pages
        /// that a copy takes many steps.
        fn fill(&self, from: usize, rows: usize) {
            let tx = self.conn.unchecked_transaction().unwrap();
            for row in from..from + rows {
                tx.execute(
                    "INSERT INTO setting (scope, scope_id, key, value_json) VALUES ('register', ?1, ?2, ?3)",
                    params![REGISTER.as_slice(), format!("bulk.{row}"), json!("x".repeat(2_000)).to_string()],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }

        fn settings(conn: &Connection) -> i64 {
            conn.query_row("SELECT COUNT(*) FROM setting", [], |row| row.get(0))
                .unwrap()
        }

        /// One audit row, with the delivery envelope its trigger demands.
        fn audit(&self, tag: u8) {
            let row = Uuid::from_bytes([tag; 16]);
            let tx = self.conn.unchecked_transaction().unwrap();
            let mut commit = *row.as_bytes();
            commit[0] = 0xCC;
            let mut change = *row.as_bytes();
            change[2] = 0xC0;
            write_envelope(&self.conn, &tx, &commit, &change, row);
            AuditRepository::new(&self.conn)
                .append(
                    &tx,
                    register(),
                    row,
                    &AuditIntent {
                        actor: UserId::from_uuid(Uuid::from_bytes([0xA1; 16])),
                        approver: None,
                        approval: None,
                        action: "drawer.open",
                        entity: "drawer_event",
                        entity_id: row,
                        reason: None,
                        payload: json!({ "tag": i64::from(tag) }),
                        at: noon(),
                    },
                )
                .unwrap();
            tx.commit().unwrap();
        }

        fn snapshot(
            &self,
            key: &DatabaseKey,
            envelope: &RecoveryEnvelope,
            at: Timestamp,
        ) -> Result<BackupInfo, DbError> {
            snapshot(
                &self.conn,
                register(),
                key,
                envelope,
                &self.destination(),
                at,
                &mut || Step::GoOn,
            )
        }
    }

    fn write_envelope(
        conn: &Connection,
        tx: &Transaction<'_>,
        commit: &[u8; 16],
        change: &[u8; 16],
        row: Uuid,
    ) {
        let entity_id = *row.as_bytes();
        OutboxRepository::new(conn)
            .write_commit(
                tx,
                &CommitEnvelope {
                    commit_id: commit,
                    protocol_version: 1,
                    schema_version: crate::SCHEMA_VERSION,
                    producer_version: "test",
                    created_at: AT,
                },
                &[FactMember {
                    change_id: change,
                    entity: "audit_log",
                    entity_id: &entity_id,
                    payload: "{}",
                }],
            )
            .unwrap();
    }

    /// The names of the entries in `dir`, sorted.
    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    fn unverified(result: Result<BackupInfo, DbError>) -> &'static str {
        match result {
            Err(DbError::Backup(BackupError::Unverified(reason))) => reason,
            other => panic!("a refusal was expected, not {other:?}"),
        }
    }

    /// 1.8.5b's held test, which needed a backup to travel with: every
    /// generation carries the register's envelope, and the recovery code
    /// alone, typed back from paper, opens the generation's database with
    /// nothing from the machine or its credential store.
    #[test]
    fn the_wrapped_envelope_travels_with_every_backup() {
        let key = key();
        let code = RecoveryCode::generate().unwrap();
        let envelope = wrap_data_key(&key, &code, &KdfParams::fresh().unwrap()).unwrap();
        let till = Register::new(&key);
        till.fill(0, 10);
        let first = till.snapshot(&key, &envelope, noon()).unwrap();
        till.fill(10, 5);
        let second = till
            .snapshot(&key, &envelope, at(noon().epoch_milliseconds() + HOUR_MS))
            .unwrap();

        let paper = code.formatted();
        for (generation, rows) in [(&first, 10), (&second, 15)] {
            assert_eq!(
                entries(&generation.path),
                [MANIFEST_FILE, ENVELOPE_FILE, DATABASE_FILE]
                    .map(str::to_owned)
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
            );
            let carried = RecoveryEnvelope::read(&generation.path.join(ENVELOPE_FILE)).unwrap();
            assert_eq!(carried, envelope);
            let opened = unwrap_data_key(&carried, &RecoveryCode::parse(&paper).unwrap()).unwrap();
            let copy = Connection::open_with_flags(
                generation.path.join(DATABASE_FILE),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            copy.pragma_update(None, "key", opened.expose()).unwrap();
            assert_eq!(
                Register::settings(&copy),
                rows,
                "the copy holds the register's rows"
            );
        }
    }

    /// The copy runs on the register's own connection a few pages at a time.
    /// Writes made on that connection between its steps reach the copy, which
    /// verifies and holds every row written before it finished. A caller with
    /// a write waiting makes it give way: nothing is published, no staging is
    /// left, and the next run is whole.
    #[test]
    fn snapshot_serializes_with_source_writes_and_is_consistent() {
        let key = key();
        let envelope = stand_in_envelope(&key);
        let till = Register::new(&key);
        till.fill(0, 600);

        let mut steps = 0;
        let mut written = 600;
        let taken = snapshot(
            &till.conn,
            register(),
            &key,
            &envelope,
            &till.destination(),
            noon(),
            &mut || {
                steps += 1;
                till.fill(written, 3);
                written += 3;
                Step::GoOn
            },
        )
        .unwrap();
        assert!(
            steps > 4,
            "the copy took {steps} steps; a test of interleaving needs several"
        );
        let copy = Connection::open_with_flags(
            taken.path.join(DATABASE_FILE),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        copy.pragma_update(None, "key", key.expose()).unwrap();
        assert_eq!(
            Register::settings(&copy),
            Register::settings(&till.conn),
            "every row written before it finished"
        );
        verify(&taken.path, &key, &envelope).unwrap();

        let mut asked = 0;
        let gave_way = snapshot(
            &till.conn,
            register(),
            &key,
            &envelope,
            &till.destination(),
            at(noon().epoch_milliseconds() + 1),
            &mut || {
                asked += 1;
                if asked == 2 {
                    Step::GiveWay
                } else {
                    Step::GoOn
                }
            },
        );
        assert!(matches!(
            gave_way,
            Err(DbError::Backup(BackupError::GaveWay))
        ));
        assert_eq!(
            entries(&till.destination()),
            std::slice::from_ref(&taken.manifest.generation),
            "nothing published, no staging left"
        );
        till.snapshot(&key, &envelope, at(noon().epoch_milliseconds() + 2))
            .unwrap();
    }

    /// The hourly job verifies with the data key the credential store holds,
    /// and nothing else: [`verify`] takes no recovery code. Another key does
    /// not verify a generation, whatever its manifest claims, and a generation
    /// carrying another envelope does not verify either.
    #[test]
    fn scheduled_verify_uses_the_live_key_not_the_recovery_code() {
        let key = key();
        let envelope = stand_in_envelope(&key);
        let till = Register::new(&key);
        till.fill(0, 20);
        let taken = till.snapshot(&key, &envelope, noon()).unwrap();
        assert_eq!(verify(&taken.path, &key, &envelope).unwrap(), taken);

        // Another envelope for the same key, as a second wrap would make: not
        // the one this register's code opens.
        assert_eq!(
            unverified(verify(&taken.path, &key, &stand_in_envelope(&key))),
            "it carries another recovery envelope"
        );

        let other = DatabaseKey::new("another-register's-key".to_owned());
        assert_eq!(
            unverified(verify(&taken.path, &other, &stand_in_envelope(&other))),
            "it is encrypted with another key"
        );

        // A manifest rewritten to claim the other key: the database itself
        // still does not open with it.
        let mut forged = taken.manifest.clone();
        forged.key_id = other.id();
        write_durably(&taken.path.join(MANIFEST_FILE), &forged).unwrap();
        let mut carried = stand_in_envelope(&other);
        carried.kdf_params = envelope.kdf_params.clone();
        carried.write(&taken.path.join(ENVELOPE_FILE)).unwrap();
        assert_eq!(
            unverified(verify(&taken.path, &other, &carried)),
            "it does not open with the live key"
        );
        assert_eq!(
            unverified(verify(&taken.path, &key, &envelope)),
            "it is encrypted with another key"
        );

        assert!(matches!(
            till.snapshot(
                &key,
                &stand_in_envelope(&other),
                at(noon().epoch_milliseconds() + 1)
            ),
            Err(DbError::Backup(BackupError::EnvelopeForAnotherKey))
        ));
    }

    /// A generation cut short, or changed at the same length, is refused
    /// before it is opened; and one whose manifest was rewritten to match a
    /// changed file is refused by the pages themselves.
    #[test]
    fn verify_detects_truncation() {
        let key = key();
        let envelope = stand_in_envelope(&key);
        let till = Register::new(&key);
        till.fill(0, 40);
        let taken = till.snapshot(&key, &envelope, noon()).unwrap();
        let database = taken.path.join(DATABASE_FILE);
        let whole = fs::read(&database).unwrap();

        fs::write(&database, whole.get(..whole.len() - 4096).unwrap()).unwrap();
        assert_eq!(
            unverified(verify(&taken.path, &key, &envelope)),
            "its database is not the size its manifest records"
        );

        let mut flipped = whole.clone();
        let middle = flipped.len() / 2;
        *flipped.get_mut(middle).unwrap() ^= 0x01;
        fs::write(&database, &flipped).unwrap();
        assert_eq!(
            unverified(verify(&taken.path, &key, &envelope)),
            "its database is not the one its manifest records"
        );

        let mut forged = taken.manifest.clone();
        (forged.database_bytes, forged.database_blake3) = measure(&database).unwrap();
        write_durably(&taken.path.join(MANIFEST_FILE), &forged).unwrap();
        assert_eq!(
            unverified(verify(&taken.path, &key, &envelope)),
            "a page of it fails its authentication"
        );

        fs::write(&database, &whole).unwrap();
        write_durably(&taken.path.join(MANIFEST_FILE), &taken.manifest).unwrap();
        verify(&taken.path, &key, &envelope).unwrap();
    }

    /// A copy that does not verify is never published: the staging is
    /// removed, and the last verified generation stands, unchanged. A name
    /// already taken is never written over.
    #[test]
    fn unverified_copy_never_replaces_the_last_verified_generation() {
        let key = key();
        let envelope = stand_in_envelope(&key);
        let till = Register::new(&key);
        till.fill(0, 20);
        let last = till.snapshot(&key, &envelope, noon()).unwrap();
        let before = fs::read(last.path.join(DATABASE_FILE)).unwrap();

        // Copy the next one, damage it before it is published, then publish it.
        let later = at(noon().epoch_milliseconds() + HOUR_MS);
        let staged = copy(
            &till.conn,
            register(),
            &key,
            &envelope,
            &till.destination(),
            later,
            &mut || Step::GoOn,
        )
        .unwrap();
        let staging = till
            .destination()
            .join(format!("{STAGING_PREFIX}{}", staged.generation()));
        let published = till.destination().join(staged.generation());
        let database = staging.join(DATABASE_FILE);
        let mut bytes = fs::read(&database).unwrap();
        bytes.truncate(bytes.len() - 1);
        fs::write(&database, &bytes).unwrap();
        assert!(matches!(
            staged.publish(&key, &envelope),
            Err(DbError::Backup(BackupError::Unverified(_)))
        ));
        assert!(!staging.exists(), "its staging is removed");
        assert!(!published.exists(), "an unverified copy is never published");

        assert!(matches!(
            till.snapshot(&key, &envelope, noon()),
            Err(DbError::Backup(BackupError::GenerationExists))
        ));
        assert_eq!(fs::read(last.path.join(DATABASE_FILE)).unwrap(), before);
        assert_eq!(verify(&last.path, &key, &envelope).unwrap(), last);
        assert_eq!(generations(&till.destination()).unwrap(), [last]);
    }

    /// The manifest anchors the register's audit head as the copy holds it,
    /// under `audit_checkpoint`'s own names, so `verify-audit --anchor` reads
    /// it as it stands. A register that has written nothing anchors seq 0.
    #[test]
    fn backup_manifest_anchors_the_audit_head_of_the_copy() {
        let key = key();
        let envelope = stand_in_envelope(&key);
        let till = Register::new(&key);
        let empty = till.snapshot(&key, &envelope, noon()).unwrap();
        assert_eq!(
            (empty.manifest.last_seq, empty.manifest.last_hash.as_str()),
            (0, "0".repeat(64).as_str())
        );

        till.audit(0x11);
        till.audit(0x12);
        let head = AuditRepository::new(&till.conn)
            .head(register())
            .unwrap()
            .unwrap();
        let taken = till
            .snapshot(&key, &envelope, at(noon().epoch_milliseconds() + HOUR_MS))
            .unwrap();
        assert_eq!(
            (taken.manifest.last_seq, taken.manifest.last_hash.clone()),
            (head.0, hex(&head.1))
        );
        assert_eq!(taken.manifest.register_id, register().as_uuid().to_string());

        let anchor: serde_json::Value =
            serde_json::from_slice(&fs::read(taken.path.join(MANIFEST_FILE)).unwrap()).unwrap();
        for column in [
            "register_id",
            "last_seq",
            "last_hash",
            "source_kind",
            "anchored_at",
        ] {
            assert!(anchor.get(column).is_some(), "{column}");
        }
        assert_eq!(anchor.get("source_kind").unwrap(), "verified_backup");
        assert_eq!(
            anchor.get("anchored_at").unwrap().as_str().unwrap(),
            at(noon().epoch_milliseconds() + HOUR_MS)
                .to_iso8601()
                .as_str()
        );

        // A head the copy does not hold is refused.
        let mut forged = taken.manifest.clone();
        forged.last_seq += 1;
        write_durably(&taken.path.join(MANIFEST_FILE), &forged).unwrap();
        assert_eq!(
            unverified(verify(&taken.path, &key, &envelope)),
            "its audit head is not the one its manifest anchors"
        );
    }

    /// Merchant decision 13.3: the newest generation of each hour of the last
    /// day and of each day of the last thirty, any taken after now, and the
    /// newest always.
    #[test]
    fn backup_retention_keeps_hourly_for_a_day_and_daily_for_a_month() {
        let now = noon().epoch_milliseconds();
        let named = |ms: i64| (generation_name(at(ms)), at(ms));
        let mut all = Vec::new();
        // Every quarter-hour for two days, then one every day back to 40 days.
        for quarter in 0..(2 * 24 * 4) {
            all.push(named(now - quarter * HOUR_MS / 4));
        }
        for day in 2..40 {
            all.push(named(now - day * DAY_MS - 5 * HOUR_MS));
        }
        all.push(named(now + 10 * 60 * 1000));
        let kept = retained(&all, at(now));

        let within_day = all.iter().filter(|(name, at)| {
            let age = now - at.epoch_milliseconds();
            (0..HOURLY_FOR_MS).contains(&age) && kept.contains(name)
        });
        assert_eq!(
            within_day.count(),
            24 + 1,
            "one per hour of the last day, and the hour of now"
        );
        assert!(kept.contains(&named(now).0), "the newest");
        assert!(
            kept.contains(&named(now - HOUR_MS / 4).0)
                && !kept.contains(&named(now - HOUR_MS / 2).0),
            "the newest of its hour, not the oldest"
        );
        assert!(
            kept.contains(&named(now + 10 * 60 * 1000).0),
            "one taken after now"
        );
        assert!(
            !kept.contains(&named(now - 35 * DAY_MS - 5 * HOUR_MS).0),
            "older than thirty days"
        );
        assert!(
            kept.contains(&named(now - 29 * DAY_MS - 5 * HOUR_MS).0),
            "a day within thirty"
        );
        let days: BTreeSet<i64> = kept
            .iter()
            .filter_map(|name| generation_instant(name))
            .map(|at| at.epoch_milliseconds().div_euclid(DAY_MS))
            .collect();
        assert!(days.len() <= 31 + 1);
        assert_eq!(
            retained(&[named(now - 90 * DAY_MS)], at(now)).len(),
            1,
            "the newest, however old"
        );

        // On the disk: a stale generation goes, a staging a stopped run left
        // goes, and what is not a generation stays.
        let key = key();
        let envelope = stand_in_envelope(&key);
        let till = Register::new(&key);
        let old = till
            .snapshot(&key, &envelope, at(now - 40 * DAY_MS))
            .unwrap();
        let recent = till
            .snapshot(&key, &envelope, at(now - HOUR_MS / 2))
            .unwrap();
        let newest = till.snapshot(&key, &envelope, at(now)).unwrap();
        fs::create_dir(
            till.destination()
                .join(format!("{STAGING_PREFIX}{}", generation_name(at(now + 1)))),
        )
        .unwrap();
        fs::create_dir(till.destination().join("notes")).unwrap();
        assert_eq!(
            prune(&till.destination(), at(now)).unwrap(),
            std::slice::from_ref(&old.manifest.generation)
        );
        assert_eq!(
            entries(&till.destination()),
            [
                recent.manifest.generation,
                newest.manifest.generation,
                "notes".to_owned()
            ]
        );
    }

    /// A generation's name is its instant, and only a name that is one reads
    /// back.
    #[test]
    fn backup_generation_name_is_its_instant() {
        assert_eq!(generation_name(noon()), "2026-11-13T17-00-00.000Z");
        assert_eq!(generation_instant("2026-11-13T17-00-00.000Z"), Some(noon()));
        for name in [
            "2026-11-13T17:00:00.000Z",
            "notes",
            "2026-11-13T17-00-00Z",
            "",
            ".staging-2026-11-13T17-00-00.000Z",
        ] {
            assert_eq!(generation_instant(name), None, "{name:?}");
        }
    }
}
