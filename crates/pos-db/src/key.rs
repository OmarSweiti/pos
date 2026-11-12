//! DB-key management.
//!
//! Production path: the OS credential store via `keyring` (Windows Credential
//! Manager / macOS Keychain / Secret Service).
//!
//! Dev/CI path: the `POS_DB_KEY` environment variable, so a laptop and a CI
//! runner need no vault. **A release build refuses to honour it** (conventions
//! §12, microstep 1.8.5): an environment variable is readable by every process
//! running as that user and lands in crash dumps and process listings, which is
//! precisely what the credential store exists to avoid.
//!
//! The refusal is *ignore-and-continue*, not *error*. Falling through to the
//! credential store is the more secure outcome, and a stray variable inherited
//! from a shell must never be able to stop a register from opening its till.
//!
//! **A key is never minted beside an existing database** (microstep 1.8.5b). A
//! database whose credential-store entry is gone is opened with the merchant's
//! recovery code ([`crate::recovery`]), not with a fresh key: a fresh key opens
//! nothing, and writing it to the store would leave the store holding the
//! wrong key for the one database it serves.

use std::fmt;
use std::path::Path;

use keyring::Entry;
use zeroize::Zeroizing;

const SERVICE: &str = "pos-terminal";
const USER: &str = "sqlcipher-db-key";

/// Bytes of entropy behind a generated key. SQLCipher stretches whatever it is
/// given with PBKDF2, but the passphrase should carry full strength on its own.
const KEY_BYTES: usize = 32;

/// The BLAKE3 context a key's id is derived under. A context string names one
/// purpose, so the id can never be mistaken for any other hash of the key.
const KEY_ID_CONTEXT: &str = "pos-db 2026-11-12 data key id";

/// The register's database key: the passphrase SQLCipher opens its file with.
///
/// It never prints itself: `Debug` says `DatabaseKey(redacted)`, and nothing
/// implements `Display`. It is wiped from memory when dropped. Only
/// [`DatabaseKey::expose`] hands the passphrase out, to the one call that keys
/// a connection.
#[derive(Clone, PartialEq, Eq)]
pub struct DatabaseKey(Zeroizing<String>);

impl DatabaseKey {
    /// The key whose passphrase is `passphrase`.
    pub fn new(passphrase: String) -> Self {
        Self(Zeroizing::new(passphrase))
    }

    /// The passphrase, for keying a connection. Never for a log line.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// A name for this key that reveals nothing about it: the first 16 bytes
    /// of a BLAKE3 key derivation from it, in hex. A recovery envelope carries
    /// it, so the key a restore unwraps can be checked against the one the
    /// envelope was made for.
    pub fn id(&self) -> String {
        let derived = blake3::derive_key(KEY_ID_CONTEXT, self.0.as_bytes());
        derived
            .iter()
            .take(16)
            .fold(String::with_capacity(32), |mut id, byte| {
                use std::fmt::Write as _;
                // Writing to a String is infallible; the Result is the trait's.
                let _ = write!(id, "{byte:02x}");
                id
            })
    }
}

impl fmt::Debug for DatabaseKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DatabaseKey(redacted)")
    }
}

/// Where a key came from. Returned alongside the key so a caller can log the
/// *source* — never the key — on the diagnostics screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// `POS_DB_KEY`. Debug builds only.
    Environment,
    /// The OS credential store.
    CredentialStore,
}

/// Where the register's key is kept between runs.
///
/// The OS credential store in production ([`OsCredentialStore`]). The seam
/// exists so the rules about the key are tested without touching the machine's
/// real Keychain: a test that provisions a Keychain entry is worse than the gap
/// it closes.
pub trait CredentialStore {
    /// The stored key, or `None` when the store holds none.
    ///
    /// # Errors
    ///
    /// The store could not be read.
    fn read(&self) -> Result<Option<DatabaseKey>, crate::DbError>;

    /// Keep `key`, replacing any key held.
    ///
    /// # Errors
    ///
    /// The store could not be written.
    fn write(&self, key: &DatabaseKey) -> Result<(), crate::DbError>;
}

/// The OS credential store: Windows Credential Manager, the macOS Keychain or
/// the Secret Service.
#[derive(Debug, Clone, Copy, Default)]
pub struct OsCredentialStore;

impl CredentialStore for OsCredentialStore {
    fn read(&self) -> Result<Option<DatabaseKey>, crate::DbError> {
        match Entry::new(SERVICE, USER)?.get_password() {
            Ok(key) => Ok(Some(DatabaseKey::new(key))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write(&self, key: &DatabaseKey) -> Result<(), crate::DbError> {
        Entry::new(SERVICE, USER)?.set_password(key.expose())?;
        Ok(())
    }
}

/// A credential store held in memory, for tests in this crate and in the
/// crates that build on it. Never product: nothing a register ships enables
/// the `fixtures` feature.
#[cfg(any(test, feature = "fixtures"))]
#[derive(Debug, Default)]
pub struct MemoryCredentialStore {
    key: std::sync::Mutex<Option<DatabaseKey>>,
}

#[cfg(any(test, feature = "fixtures"))]
impl MemoryCredentialStore {
    /// A store holding no key, as a clean machine's does.
    pub fn new() -> Self {
        Self::default()
    }

    /// The key held, if any.
    pub fn held(&self) -> Option<DatabaseKey> {
        self.lock().clone()
    }

    /// Lose the key, as a wiped credential store does.
    pub fn forget(&self) {
        *self.lock() = None;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<DatabaseKey>> {
        self.key
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(any(test, feature = "fixtures"))]
impl CredentialStore for MemoryCredentialStore {
    fn read(&self) -> Result<Option<DatabaseKey>, crate::DbError> {
        Ok(self.held())
    }

    fn write(&self, key: &DatabaseKey) -> Result<(), crate::DbError> {
        *self.lock() = Some(key.clone());
        Ok(())
    }
}

/// The whole of the release-build policy, as a pure function of the build
/// profile, so a debug test can prove what a release build does (conventions
/// §5: the rule is tested, not asserted in a comment).
const fn env_key_permitted(debug_build: bool) -> bool {
    debug_build
}

/// True when this build will read `POS_DB_KEY`. Exposed so the shell can show it
/// on a diagnostics screen and so the guard is observable from a test.
pub const fn honours_env_key() -> bool {
    env_key_permitted(cfg!(debug_assertions))
}

/// The key for the database at `database`, from the OS credential store, or a
/// new one when there is neither a key nor a database. See
/// [`get_or_create_in`].
///
/// # Errors
///
/// As [`get_or_create_in`].
pub fn get_or_create(database: &Path) -> Result<(DatabaseKey, KeySource), crate::DbError> {
    get_or_create_in(&OsCredentialStore, database)
}

/// The key for the database at `database`.
///
/// `POS_DB_KEY` in a debug build, else the key `store` holds. With no key
/// held, a new key is minted and kept **only when no database exists at
/// `database`**: a database without its key is the recovery code's to open,
/// and a fresh key would open nothing.
///
/// # Errors
///
/// [`crate::DbError::KeyMissingForExistingDatabase`] when a database exists and
/// `store` holds no key; [`crate::DbError::DatabasePresenceUnknown`] when
/// whether one exists cannot be told, since minting then risks the same;
/// [`crate::DbError::KeyGeneration`] when the OS supplies no randomness; and
/// whatever `store` returns.
pub fn get_or_create_in(
    store: &dyn CredentialStore,
    database: &Path,
) -> Result<(DatabaseKey, KeySource), crate::DbError> {
    resolve(env_key(), store, database)
}

/// [`get_or_create_in`], with the environment's key passed in, so a test
/// states it rather than inheriting whatever the shell that ran it exported.
pub(crate) fn resolve(
    env: Option<String>,
    store: &dyn CredentialStore,
    database: &Path,
) -> Result<(DatabaseKey, KeySource), crate::DbError> {
    if let Some(key) = env {
        return Ok((DatabaseKey::new(key), KeySource::Environment));
    }
    if let Some(key) = store.read()? {
        return Ok((key, KeySource::CredentialStore));
    }
    if database_exists(database)? {
        return Err(crate::DbError::KeyMissingForExistingDatabase);
    }
    let key = DatabaseKey::new(generate_key()?);
    store.write(&key)?;
    Ok((key, KeySource::CredentialStore))
}

/// The key for an existing database, never a new one: for a tool that reads a
/// register's file and must not provision the machine it runs on.
///
/// # Errors
///
/// [`crate::DbError::KeyMissingForExistingDatabase`] when no key is held, and
/// whatever the credential store returns.
pub fn get_existing() -> Result<(DatabaseKey, KeySource), crate::DbError> {
    get_existing_in(&OsCredentialStore)
}

/// [`get_existing`], from `store`.
///
/// # Errors
///
/// As [`get_existing`].
pub fn get_existing_in(
    store: &dyn CredentialStore,
) -> Result<(DatabaseKey, KeySource), crate::DbError> {
    resolve_existing(env_key(), store)
}

/// [`get_existing_in`], with the environment's key passed in.
pub(crate) fn resolve_existing(
    env: Option<String>,
    store: &dyn CredentialStore,
) -> Result<(DatabaseKey, KeySource), crate::DbError> {
    if let Some(key) = env {
        return Ok((DatabaseKey::new(key), KeySource::Environment));
    }
    store
        .read()?
        .map(|key| (key, KeySource::CredentialStore))
        .ok_or(crate::DbError::KeyMissingForExistingDatabase)
}

/// Whether a database file is at `database`. An answer the file system cannot
/// give is an error, never "no": "no" is what licenses a new key.
fn database_exists(database: &Path) -> Result<bool, crate::DbError> {
    database
        .try_exists()
        .map_err(|error| crate::DbError::DatabasePresenceUnknown(error.to_string()))
}

/// The environment key, if this build is allowed one and one is set.
///
/// `None` means **continue to the credential store**, and it is the only way
/// this function declines — it has no error case. That is the whole of the
/// ignore-and-continue rule in one signature: a release build, or an unset or
/// empty variable, all produce "keep going" rather than "stop". A register whose
/// shell happened to export `POS_DB_KEY` must still open its till.
///
/// It exists as a separate function so the release-build behaviour is testable
/// without a credential store.
fn env_key_from(raw: Option<&str>) -> Option<String> {
    if !honours_env_key() {
        return None;
    }
    raw.filter(|key| !key.is_empty()).map(str::to_owned)
}

fn env_key() -> Option<String> {
    env_key_from(std::env::var("POS_DB_KEY").ok().as_deref())
}

/// A fresh 256-bit key, hex-encoded, straight from the OS CSPRNG.
///
/// Deliberately not a UUIDv4: a v4 spends 6 of its 128 bits on version and
/// variant markers, so it carries 122 bits, and a key is the wrong place to be
/// approximate about that.
fn generate_key() -> Result<String, crate::DbError> {
    let mut bytes = Zeroizing::new([0u8; KEY_BYTES]);
    getrandom::fill(bytes.as_mut_slice())
        .map_err(|e| crate::DbError::KeyGeneration(e.to_string()))?;
    Ok(bytes
        .iter()
        .fold(String::with_capacity(KEY_BYTES * 2), |mut acc, b| {
            use std::fmt::Write as _;
            // Writing to a String is infallible; the Result is the trait's, not ours.
            let _ = write!(acc, "{b:02x}");
            acc
        }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Conventions §12: `POS_DB_KEY` is dev/CI only. This is the assertion that
    /// the release build refuses it — checkable from a debug test because the
    /// policy is a function of the profile rather than a `#[cfg]` block.
    #[test]
    fn a_release_build_refuses_the_environment_key() {
        assert!(!env_key_permitted(false), "release must ignore POS_DB_KEY");
        assert!(env_key_permitted(true), "debug/CI must still honour it");
    }

    /// Microstep 1.8.5's named test, and the only one that runs in the profile
    /// the rule is about. `#[cfg(not(debug_assertions))]` is why its `Done when`
    /// command carries `--release`: in a debug build this test does not exist.
    ///
    /// It asserts both halves of *ignore-and-continue*. Ignore: with
    /// `POS_DB_KEY` set to a sentinel, a release build does not read it.
    /// Continue: the decline is `None` rather than an error, so the caller falls
    /// through to the credential store instead of refusing to open.
    ///
    /// The value is a parameter rather than a real environment variable because
    /// `std::env::set_var` is `unsafe` in edition 2024 and `unsafe_code` is
    /// **forbid** workspace-wide — deliberately, so it cannot be lifted by a
    /// local attribute. A test may not mutate the process environment here, so
    /// the rule is a pure function of the build profile and the raw value, which
    /// is the shape `env_key_permitted` already uses one line above.
    #[cfg(not(debug_assertions))]
    #[test]
    fn release_build_ignores_env_key_and_falls_through() {
        const SENTINEL: &str = "release-build-must-not-read-this";

        assert!(
            !honours_env_key(),
            "a release build must not honour POS_DB_KEY"
        );
        assert_eq!(
            env_key_from(Some(SENTINEL)),
            None,
            "a release build read POS_DB_KEY instead of ignoring it"
        );
    }

    /// The debug half of the same seam: the dev/CI path still works, and an
    /// empty variable is treated as absent rather than as a key.
    #[cfg(debug_assertions)]
    #[test]
    fn a_debug_build_reads_the_env_key_but_treats_empty_as_absent() {
        assert_eq!(env_key_from(Some("deadbeef")).as_deref(), Some("deadbeef"));
        assert_eq!(
            env_key_from(Some("")),
            None,
            "an empty variable is not a key"
        );
        assert_eq!(env_key_from(None), None);
    }

    #[test]
    fn this_build_matches_its_profile() {
        assert_eq!(honours_env_key(), cfg!(debug_assertions));
    }

    #[test]
    fn generated_keys_are_256_bit_hex_and_never_repeat() {
        let a = generate_key().unwrap();
        let b = generate_key().unwrap();
        assert_eq!(a.len(), KEY_BYTES * 2, "64 hex characters = 256 bits");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
