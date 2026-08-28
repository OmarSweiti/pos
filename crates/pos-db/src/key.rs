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

use keyring::Entry;

const SERVICE: &str = "pos-terminal";
const USER: &str = "sqlcipher-db-key";

/// Bytes of entropy behind a generated key. SQLCipher stretches whatever it is
/// given with PBKDF2, but the passphrase should carry full strength on its own.
const KEY_BYTES: usize = 32;

/// Where a key came from. Returned alongside the key so a caller can log the
/// *source* — never the key — on the diagnostics screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// `POS_DB_KEY`. Debug builds only.
    Environment,
    /// The OS credential store.
    CredentialStore,
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

pub fn get_or_create() -> Result<String, crate::DbError> {
    Ok(get_or_create_with_source()?.0)
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
/// without a credential store. `get_or_create_with_source` cannot be called in a
/// test without touching the machine's real Keychain and, on a clean machine,
/// writing a key into it.
fn env_key_from(raw: Option<&str>) -> Option<String> {
    if !honours_env_key() {
        return None;
    }
    raw.filter(|key| !key.is_empty()).map(str::to_owned)
}

fn env_key() -> Option<String> {
    env_key_from(std::env::var("POS_DB_KEY").ok().as_deref())
}

pub fn get_or_create_with_source() -> Result<(String, KeySource), crate::DbError> {
    if let Some(key) = env_key() {
        return Ok((key, KeySource::Environment));
    }

    let entry = Entry::new(SERVICE, USER)?;
    match entry.get_password() {
        Ok(key) => Ok((key, KeySource::CredentialStore)),
        Err(keyring::Error::NoEntry) => {
            let key = generate_key()?;
            entry.set_password(&key)?;
            Ok((key, KeySource::CredentialStore))
        }
        Err(e) => Err(e.into()),
    }
}

/// A fresh 256-bit key, hex-encoded, straight from the OS CSPRNG.
///
/// Deliberately not a UUIDv4: a v4 spends 6 of its 128 bits on version and
/// variant markers, so it carries 122 bits, and a key is the wrong place to be
/// approximate about that.
fn generate_key() -> Result<String, crate::DbError> {
    let mut bytes = [0u8; KEY_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| crate::DbError::KeyGeneration(e.to_string()))?;
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
    /// What it deliberately does not do is call `get_or_create_with_source`.
    /// That would reach the machine's real credential store and, on a clean
    /// machine, write a key into it — a test that provisions a Keychain entry is
    /// worse than the gap it closes. The keyring lookup itself is covered by the
    /// key-custody work at 1.8.5b.
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
