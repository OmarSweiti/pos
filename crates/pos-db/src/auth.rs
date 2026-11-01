//! Staff PINs: how one is hashed, checked, and guessed at (microstep 1.6.2).
//!
//! # Why this lives in `pos-db`
//!
//! Hashing needs a salt from the operating system and a few dozen megabytes of
//! memory, and `pos-domain` takes neither (I-8). The rules around it are plain
//! data and are tested here beside it.
//!
//! # A four-digit PIN is not the defence
//!
//! A four-digit PIN has 10 000 candidates, so at a quarter of a second each, a
//! copied database falls to a serial search in under an hour. What stops that is
//! everything around the hash:
//!
//! - **a reviewed Argon2id profile** ([`PIN_PROFILE`]), asserted by reading its
//!   parameters back out of the stored PHC string, with a rehash when the floor
//!   rises ([`needs_rehash`]);
//! - **persisted attempt limiting** ([`ATTEMPT_POLICY`]), per user and per
//!   register, so restarting the register clears nothing;
//! - **longer PINs for anyone who can move money or administer people**
//!   ([`minimum_pin_digits`]).
//!
//! `ref/security-compliance.md` §5 is the full treatment.
//!
//! # What never leaves this module
//!
//! A PIN and a PHC string are on the never-log list (`pin`, `pin_hash`).
//! No error here carries either, nor any part of one.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use pos_domain::Timestamp;

/// Why a PIN could not be hashed, checked or accepted. No variant carries the
/// PIN or the hash.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    /// A new PIN that is not all digits, or not long enough for the account.
    #[error("a PIN is {min_digits} or more digits, and nothing else")]
    PinShape { min_digits: usize },
    /// The stored hash is not a PHC string this build can read.
    #[error("the stored PIN hash is not one this build can read")]
    UnreadableHash,
    /// The stored hash is readable but not Argon2id.
    #[error("the stored PIN hash is not Argon2id")]
    NotArgon2id,
    /// The hasher refused, or the OS could not supply a salt.
    #[error("the PIN could not be hashed: {0}")]
    Hashing(&'static str),
}

/// A named, versioned set of Argon2id parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinHashProfile {
    /// Raised when the parameters are: a stored hash below the current
    /// profile is rehashed at the next correct PIN.
    pub version: u32,
    /// Memory, in KiB.
    pub m_cost_kib: u32,
    /// Passes over that memory.
    pub t_cost: u32,
    /// Lanes.
    pub p_cost: u32,
    /// The digest's length, in bytes.
    pub output_len: usize,
    /// The salt's length, in bytes.
    pub salt_len: usize,
}

/// The reviewed profile: RFC 9106 §4's second recommended option, Argon2id
/// with three passes over 64 MiB in four lanes, a 128-bit salt and a 256-bit
/// tag. It is chosen before the reference register exists. Microstep 5.8.2
/// re-tunes it there to a verification of 200–350 ms median and 500 ms at the
/// 99th percentile. Raising it is a new `version`, and every stored hash below
/// it is rehashed at its owner's next correct PIN.
pub const PIN_PROFILE: PinHashProfile = PinHashProfile {
    version: 1,
    m_cost_kib: 65_536,
    t_cost: 3,
    p_cost: 4,
    output_len: 32,
    salt_len: 16,
};

/// The digits a cashier's PIN has at least (merchant decision 9.2).
pub const CASHIER_PIN_DIGITS: usize = 4;

/// The digits a PIN has at least when its account can move money or administer
/// people (merchant decision 9.5). Four digits fall to a search over a lunch
/// break; six take a hundred times as long.
pub const MANAGER_PIN_DIGITS: usize = 6;

/// The capabilities that make an account a manager's for its PIN's length
/// (`ref/security-compliance.md` §5): every `refund.` capability,
/// `settings.edit`, `user.admin` and `zreport.run`.
pub fn is_manager_capability(capability: &str) -> bool {
    capability.starts_with("refund.")
        || matches!(capability, "settings.edit" | "user.admin" | "zreport.run")
}

/// The digits a PIN needs for an account holding `capabilities`.
pub fn minimum_pin_digits<'a>(capabilities: impl IntoIterator<Item = &'a str>) -> usize {
    if capabilities.into_iter().any(is_manager_capability) {
        MANAGER_PIN_DIGITS
    } else {
        CASHIER_PIN_DIGITS
    }
}

/// Whether `pin` may be set on an account that needs `min_digits`: ASCII
/// digits only, at least that many, and no more than twelve, since a longer PIN
/// is a mistyped one.
///
/// # Errors
///
/// [`AuthError::PinShape`].
pub fn check_new_pin(pin: &str, min_digits: usize) -> Result<(), AuthError> {
    let digits = pin.len();
    if digits < min_digits || digits > 12 || !pin.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AuthError::PinShape { min_digits });
    }
    Ok(())
}

fn hasher() -> Result<Argon2<'static>, AuthError> {
    let params = Params::new(
        PIN_PROFILE.m_cost_kib,
        PIN_PROFILE.t_cost,
        PIN_PROFILE.p_cost,
        Some(PIN_PROFILE.output_len),
    )
    .map_err(|_| AuthError::Hashing("the reviewed profile is not a valid Argon2 configuration"))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// `pin`, hashed under [`PIN_PROFILE`] with a fresh salt from the operating
/// system, as a PHC string. Two hashes of one PIN differ, because their salts
/// do.
///
/// # Errors
///
/// [`AuthError::Hashing`] when the OS has no randomness to give or the hasher
/// refuses.
pub fn hash_pin(pin: &str) -> Result<String, AuthError> {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|_| AuthError::Hashing("the OS supplied no salt"))?;
    let salt = SaltString::encode_b64(&salt).map_err(|_| AuthError::Hashing("salt"))?;
    hasher()?
        .hash_password(pin.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::Hashing("the hasher refused the PIN"))
}

/// Whether `pin` is the one `hash` was made from. The comparison runs in
/// constant time, inside the hasher, under the parameters the hash itself
/// records, so a hash made under an older profile still verifies.
///
/// # Errors
///
/// [`AuthError::UnreadableHash`] for a stored value that is no PHC string, and
/// [`AuthError::NotArgon2id`] for one made by another algorithm. A wrong PIN
/// is `Ok(false)`, never an error.
pub fn verify_pin(pin: &str, hash: &str) -> Result<bool, AuthError> {
    let parsed = argon2id(hash)?;
    match Argon2::default().verify_password(pin.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(_) => Err(AuthError::UnreadableHash),
    }
}

fn argon2id(hash: &str) -> Result<PasswordHash<'_>, AuthError> {
    let parsed = PasswordHash::new(hash).map_err(|_| AuthError::UnreadableHash)?;
    if parsed.algorithm != Algorithm::Argon2id.ident() {
        return Err(AuthError::NotArgon2id);
    }
    Ok(parsed)
}

/// The parameters a stored hash was made under, read back out of its PHC
/// string. `version` is the profile's if the parameters are exactly its, and
/// zero otherwise.
///
/// # Errors
///
/// As [`verify_pin`].
pub fn profile_of(hash: &str) -> Result<PinHashProfile, AuthError> {
    let parsed = argon2id(hash)?;
    let params = Params::try_from(&parsed).map_err(|_| AuthError::UnreadableHash)?;
    let mut found = PinHashProfile {
        version: 0,
        m_cost_kib: params.m_cost(),
        t_cost: params.t_cost(),
        p_cost: params.p_cost(),
        output_len: parsed.hash.map_or(0, |h| h.len()),
        salt_len: parsed
            .salt
            .and_then(|s| {
                let mut buf = [0u8; 64];
                s.decode_b64(&mut buf).ok().map(<[u8]>::len)
            })
            .unwrap_or(0),
    };
    let reviewed = PinHashProfile {
        version: 0,
        ..PIN_PROFILE
    };
    if found == reviewed {
        found.version = PIN_PROFILE.version;
    }
    Ok(found)
}

/// Whether a stored hash falls below [`PIN_PROFILE`] anywhere, and is rehashed
/// at its owner's next correct PIN.
///
/// # Errors
///
/// As [`verify_pin`].
pub fn needs_rehash(hash: &str) -> Result<bool, AuthError> {
    let found = profile_of(hash)?;
    Ok(found.m_cost_kib < PIN_PROFILE.m_cost_kib
        || found.t_cost < PIN_PROFILE.t_cost
        || found.p_cost < PIN_PROFILE.p_cost
        || found.output_len < PIN_PROFILE.output_len
        || found.salt_len < PIN_PROFILE.salt_len)
}

/// How wrong PINs are answered (the owner's decision, 1 November 2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptPolicy {
    /// Wrong PINs that cost nothing.
    pub free_attempts: u32,
    /// The wait after the first wrong PIN beyond those, in milliseconds. Each
    /// later one doubles it.
    pub first_delay_ms: i64,
    /// The longest wait, in milliseconds.
    pub max_delay_ms: i64,
    /// Wrong PINs in a row that lock the user out on the register until a
    /// manager releases them.
    pub lockout_after: u32,
}

/// The first three wrong PINs cost nothing; each later one waits 30 s,
/// doubling, to at most 15 minutes; the tenth in a row locks the user out on
/// that register until a manager releases them, audited. A correct PIN resets
/// the count.
pub const ATTEMPT_POLICY: AttemptPolicy = AttemptPolicy {
    free_attempts: 3,
    first_delay_ms: 30_000,
    max_delay_ms: 900_000,
    lockout_after: 10,
};

impl AttemptPolicy {
    /// The wait that follows the `failed`-th wrong PIN in a row, if any.
    #[must_use]
    pub fn delay_after(&self, failed: u32) -> Option<i64> {
        if failed <= self.free_attempts || self.locks_out(failed) {
            return None;
        }
        let doublings = failed - self.free_attempts - 1;
        let delay = self
            .first_delay_ms
            .checked_mul(1_i64.checked_shl(doublings).unwrap_or(i64::MAX))
            .unwrap_or(i64::MAX);
        Some(delay.min(self.max_delay_ms))
    }

    /// Whether `failed` wrong PINs in a row lock the user out.
    #[must_use]
    pub fn locks_out(&self, failed: u32) -> bool {
        failed >= self.lockout_after
    }
}

/// One user's wrong PINs on one register, as `auth_attempt_state` keeps them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttemptState {
    pub failed_attempts: u32,
    /// No PIN is checked before this moment.
    pub delay_until: Option<Timestamp>,
    pub last_attempt_at: Option<Timestamp>,
}

/// Whether a PIN may be checked now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Allowed,
    /// Not before this moment.
    Delayed {
        until: Timestamp,
    },
    /// Not until a manager releases the user, audited.
    LockedOut,
}

/// Whether `state` lets a PIN be checked at `now` under `policy`.
#[must_use]
pub fn admission(state: &AttemptState, now: Timestamp, policy: &AttemptPolicy) -> Admission {
    if policy.locks_out(state.failed_attempts) {
        return Admission::LockedOut;
    }
    match state.delay_until {
        Some(until) if now < until => Admission::Delayed { until },
        Some(_) | None => Admission::Allowed,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn hash_verify_roundtrip() {
        let hash = hash_pin("1234").unwrap();
        assert!(hash.starts_with("$argon2id$v=19$"));
        assert_eq!(verify_pin("1234", &hash), Ok(true));
        assert!(!hash.contains("1234"));
    }

    #[test]
    fn wrong_pin_rejected() {
        let hash = hash_pin("482916").unwrap();
        for wrong in ["482917", "48291", "4829160", "", "abcdef"] {
            assert_eq!(verify_pin(wrong, &hash), Ok(false), "{wrong:?}");
        }
        assert_eq!(
            verify_pin("1234", "placeholder-not-a-hash"),
            Err(AuthError::UnreadableHash)
        );
        let pbkdf2 = "$pbkdf2-sha256$i=1000$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
        assert_eq!(verify_pin("1234", pbkdf2), Err(AuthError::NotArgon2id));
    }

    #[test]
    fn hash_is_salted_and_differs_per_call() {
        let (a, b) = (hash_pin("1234").unwrap(), hash_pin("1234").unwrap());
        assert_ne!(a, b);
        let salt = |h: &str| PasswordHash::new(h).unwrap().salt.unwrap().to_string();
        assert_ne!(salt(&a), salt(&b));
        assert_eq!(verify_pin("1234", &a), Ok(true));
        assert_eq!(verify_pin("1234", &b), Ok(true));
    }

    /// The parameters are read back out of the stored PHC string, not timed:
    /// a profile is a property of the hash a register keeps.
    #[test]
    fn argon2_parameters_match_the_reviewed_profile() {
        let hash = hash_pin("1234").unwrap();
        assert_eq!(profile_of(&hash), Ok(PIN_PROFILE));
        assert!(hash.contains("$m=65536,t=3,p=4$"));
        assert_eq!(needs_rehash(&hash), Ok(false));
        // A hash made under a weaker profile verifies, and is flagged for a
        // rehash at its owner's next correct PIN.
        let weak = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(19_456, 2, 1, Some(32)).unwrap(),
        )
        .hash_password(b"1234", &SaltString::encode_b64(&[7u8; 16]).unwrap())
        .unwrap()
        .to_string();
        assert_eq!(verify_pin("1234", &weak), Ok(true));
        assert_eq!(needs_rehash(&weak), Ok(true));
        assert_eq!(profile_of(&weak).unwrap().version, 0);
    }

    /// Each floor of the profile holds on its own: a hash below it in any one
    /// parameter, and in that one only, is flagged for a rehash.
    #[test]
    fn a_hash_below_the_profile_in_any_one_parameter_is_rehashed() {
        let p = PIN_PROFILE;
        let made = |m: u32, t: u32, l: u32, out: usize, salt: &[u8]| {
            Argon2::new(
                Algorithm::Argon2id,
                Version::V0x13,
                Params::new(m, t, l, Some(out)).unwrap(),
            )
            .hash_password(b"1234", &SaltString::encode_b64(salt).unwrap())
            .unwrap()
            .to_string()
        };
        let full_salt = [9u8; 16];
        assert_eq!(
            needs_rehash(&made(
                p.m_cost_kib,
                p.t_cost,
                p.p_cost,
                p.output_len,
                &full_salt
            )),
            Ok(false)
        );
        for (below, hash) in [
            (
                "memory",
                made(
                    p.m_cost_kib / 2,
                    p.t_cost,
                    p.p_cost,
                    p.output_len,
                    &full_salt,
                ),
            ),
            (
                "passes",
                made(
                    p.m_cost_kib,
                    p.t_cost - 1,
                    p.p_cost,
                    p.output_len,
                    &full_salt,
                ),
            ),
            (
                "lanes",
                made(
                    p.m_cost_kib,
                    p.t_cost,
                    p.p_cost - 1,
                    p.output_len,
                    &full_salt,
                ),
            ),
            (
                "tag",
                made(p.m_cost_kib, p.t_cost, p.p_cost, 16, &full_salt),
            ),
            (
                "salt",
                made(p.m_cost_kib, p.t_cost, p.p_cost, p.output_len, &[9u8; 8]),
            ),
        ] {
            assert_eq!(needs_rehash(&hash), Ok(true), "{below}");
            assert_eq!(verify_pin("1234", &hash), Ok(true), "{below}");
        }
    }

    #[test]
    fn a_new_pin_has_the_digits_its_account_needs() {
        assert_eq!(minimum_pin_digits(["sale.create", "line.void"]), 4);
        for manager in [
            "refund.receipted",
            "settings.edit",
            "user.admin",
            "zreport.run",
        ] {
            assert_eq!(minimum_pin_digits(["sale.create", manager]), 6, "{manager}");
        }
        assert_eq!(check_new_pin("1234", 4), Ok(()));
        assert_eq!(check_new_pin("123456", 6), Ok(()));
        for refused in ["123", "12a4", "12 34", "١٢٣٤", "1234567890123", ""] {
            assert_eq!(
                check_new_pin(refused, 4),
                Err(AuthError::PinShape { min_digits: 4 }),
                "{refused:?}"
            );
        }
        assert_eq!(
            check_new_pin("12345", 6),
            Err(AuthError::PinShape { min_digits: 6 })
        );
    }

    /// The owner's decision: three free, then 30 s doubling to 15 minutes, and
    /// the tenth in a row locks the user out.
    #[test]
    fn wrong_pins_wait_longer_then_lock_out() {
        let p = ATTEMPT_POLICY;
        let delays: Vec<Option<i64>> = (0..=11).map(|n| p.delay_after(n)).collect();
        assert_eq!(
            delays,
            [
                None,
                None,
                None,
                None,
                Some(30_000),
                Some(60_000),
                Some(120_000),
                Some(240_000),
                Some(480_000),
                Some(900_000),
                None,
                None,
            ]
        );
        assert!(!p.locks_out(9) && p.locks_out(10) && p.locks_out(11));
        assert_eq!(p.delay_after(u32::MAX), None);
        let at = |ms| Timestamp::from_epoch_milliseconds(ms).unwrap();
        let waiting = AttemptState {
            failed_attempts: 4,
            delay_until: Some(at(1_030_000)),
            last_attempt_at: Some(at(1_000_000)),
        };
        assert_eq!(
            admission(&waiting, at(1_029_999), &p),
            Admission::Delayed {
                until: at(1_030_000)
            }
        );
        assert_eq!(admission(&waiting, at(1_030_000), &p), Admission::Allowed);
        let locked = AttemptState {
            failed_attempts: 10,
            delay_until: None,
            last_attempt_at: Some(at(1_000_000)),
        };
        assert_eq!(
            admission(&locked, at(9_000_000_000), &p),
            Admission::LockedOut
        );
        assert_eq!(
            admission(&AttemptState::default(), at(0), &p),
            Admission::Allowed
        );
    }

    #[test]
    fn auth_errors_carry_no_pin_and_no_hash() {
        let hash = hash_pin("918273").unwrap();
        for e in [
            check_new_pin("918273x", 6).unwrap_err(),
            verify_pin("918273", "garbage").unwrap_err(),
            verify_pin("918273", "$pbkdf2-sha256$i=1$c2FsdA$aGFzaA").unwrap_err(),
        ] {
            let shown = format!("{e} {e:?}");
            assert!(
                !shown.contains("918273") && !shown.contains(&hash),
                "{shown}"
            );
        }
    }
}
