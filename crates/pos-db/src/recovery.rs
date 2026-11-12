//! Key custody (microstep 1.8.5b): the merchant's recovery code, and the
//! envelope that wraps the register's data key under it.
//!
//! The data key lives in the OS credential store ([`crate::key`]). Lose the
//! store and the machine together and the database, and every backup of it,
//! is unreadable, unless a copy of the key was kept somewhere else. That copy
//! is the [`RecoveryEnvelope`]: the data key encrypted with XChaCha20-Poly1305
//! under a key Argon2id derives from a [`RecoveryCode`] the merchant holds on
//! paper. The envelope travels beside every backup; the code is never written
//! to a disk (`ref/security-compliance.md` §6a, E.4d).
//!
//! By the owner's decisions of 12 November 2026:
//!
//! - **The code is 30 digits in six groups of five.** Each group is four
//!   random digits and a Damm check digit, so a mistyped digit, or two
//!   neighbouring digits swapped, names its group. Digits only, so an Arabic
//!   or an English keyboard types it alike, and Arabic-Indic digits are read
//!   as the digits they are. Its 24 random digits carry about 80 bits, and
//!   Argon2id makes every guess cost a 64 MiB, three-pass derivation.
//! - **The wrap is XChaCha20-Poly1305,** under a 256-bit key Argon2id derives
//!   at RFC 9106's second recommended profile. Every other field of the
//!   envelope is bound in as associated data, so none can be changed without
//!   the unwrap failing.

use std::fmt;
use std::fs::{self, File};
use std::io::Write as _;
use std::path::Path;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::key::DatabaseKey;

/// The groups a recovery code is printed in.
pub const GROUPS: usize = 6;
/// The digits in each group: four random, then one check digit.
pub const GROUP_DIGITS: usize = 5;
/// The digits in a whole code.
pub const CODE_DIGITS: usize = GROUPS * GROUP_DIGITS;
const RANDOM_DIGITS_PER_GROUP: usize = GROUP_DIGITS - 1;

/// The envelope's own format version. A later format is a new number, read
/// alongside this one, never a change to it.
pub const ENVELOPE_FORMAT: u32 = 1;
/// The wrap, as an envelope names it.
pub const WRAP_ALGORITHM: &str = "xchacha20poly1305";
/// The key derivation, as an envelope names it.
pub const KDF_ALGORITHM: &str = "argon2id";
/// The envelope's file name, beside the database and beside every backup.
pub const ENVELOPE_FILE: &str = "recovery-envelope.json";

const NONCE_BYTES: usize = 24;
const TAG_BYTES: usize = 16;
const WRAPPING_KEY_BYTES: usize = 32;
const SALT_BYTES: usize = 16;

/// Argon2id's version 1.3, the only one an envelope is made or read with.
const ARGON2_VERSION: u32 = 0x13;

/// RFC 9106 §4's second recommended option: three passes over 64 MiB in four
/// lanes. A register is a modest machine, and a recovery is rare, so the
/// memory-hard option it can always afford is the right one.
const M_COST_KIB: u32 = 64 * 1024;
const T_COST: u32 = 3;
const P_COST: u32 = 4;

/// The bounds an envelope's parameters are read within. Below them the
/// derivation is weaker than this build ever writes; above them a forged
/// envelope could ask a register for gigabytes before failing.
const M_COST_RANGE: std::ops::RangeInclusive<u32> = M_COST_KIB..=1024 * 1024;
const T_COST_RANGE: std::ops::RangeInclusive<u32> = T_COST..=10;
const P_COST_RANGE: std::ops::RangeInclusive<u32> = 1..=16;
const SALT_RANGE: std::ops::RangeInclusive<usize> = SALT_BYTES..=64;

/// The Damm algorithm's table: a totally anti-symmetric quasigroup of order
/// ten. Running a group's digits through it from zero leaves zero exactly when
/// the group checks, and any one wrong digit, or any two neighbours swapped,
/// leaves something else.
const DAMM: [[u8; 10]; 10] = [
    [0, 3, 1, 7, 5, 9, 8, 6, 4, 2],
    [7, 0, 9, 2, 1, 5, 4, 8, 6, 3],
    [4, 2, 0, 6, 8, 7, 1, 3, 5, 9],
    [1, 7, 5, 0, 9, 8, 3, 4, 2, 6],
    [6, 1, 2, 3, 0, 4, 5, 9, 7, 8],
    [3, 6, 7, 4, 2, 0, 9, 5, 8, 1],
    [5, 8, 6, 9, 7, 2, 0, 1, 3, 4],
    [8, 9, 4, 5, 3, 6, 2, 0, 1, 7],
    [9, 4, 3, 8, 6, 1, 7, 2, 0, 5],
    [2, 5, 8, 1, 4, 3, 6, 7, 9, 0],
];

/// The Damm interim of `digits`, or `None` for a value that is not a digit.
/// A group checks when its interim is zero; a check digit is the interim of
/// the digits before it.
fn damm(digits: &[u8]) -> Option<u8> {
    digits.iter().try_fold(0u8, |interim, &digit| {
        DAMM.get(usize::from(interim))?
            .get(usize::from(digit))
            .copied()
    })
}

/// The merchant's recovery code: thirty digits, printed in six groups of five.
///
/// It never prints itself: `Debug` says `RecoveryCode(redacted)`, nothing
/// implements `Display`, and it is wiped from memory when dropped. Only
/// [`RecoveryCode::formatted`] writes it out, for the one screen and the one
/// sheet that show it.
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryCode {
    /// Each digit's value, 0 to 9.
    digits: Zeroizing<[u8; CODE_DIGITS]>,
}

impl RecoveryCode {
    /// A fresh code: 24 digits from the OS CSPRNG, each group closed by its
    /// check digit.
    ///
    /// # Errors
    ///
    /// [`KeyError::Random`] when the OS supplies no randomness.
    pub fn generate() -> Result<Self, KeyError> {
        let mut source = RandomDigits::default();
        let mut random = Zeroizing::new([0u8; GROUPS * RANDOM_DIGITS_PER_GROUP]);
        for digit in random.iter_mut() {
            *digit = source.next()?;
        }
        Self::from_random_digits(&random)
    }

    /// The code whose random digits are `random`, four to a group, with each
    /// group's check digit appended.
    fn from_random_digits(
        random: &[u8; GROUPS * RANDOM_DIGITS_PER_GROUP],
    ) -> Result<Self, KeyError> {
        let mut digits = Zeroizing::new([0u8; CODE_DIGITS]);
        for (group, body) in digits
            .chunks_exact_mut(GROUP_DIGITS)
            .zip(random.chunks_exact(RANDOM_DIGITS_PER_GROUP))
        {
            let check = damm(body).ok_or(KeyError::Malformed("a recovery digit above nine"))?;
            for (slot, digit) in group.iter_mut().zip(body.iter().chain([&check])) {
                *slot = *digit;
            }
        }
        Ok(Self { digits })
    }

    /// The code `text` spells.
    ///
    /// Western digits, Arabic-Indic digits (٠ to ٩) and Eastern Arabic-Indic
    /// digits (۰ to ۹) are read alike. Spaces, hyphens, dashes and the
    /// invisible direction marks Arabic text carries are ignored wherever they
    /// fall, so a code typed with or without its separators reads the same.
    ///
    /// # Errors
    ///
    /// [`RecoveryCodeError::NotADigit`] for any other character, naming its
    /// position and never the character; [`RecoveryCodeError::WrongLength`]
    /// unless there are exactly thirty digits; and
    /// [`RecoveryCodeError::Mistyped`], naming every group that does not check.
    pub fn parse(text: &str) -> Result<Self, RecoveryCodeError> {
        let mut digits = Zeroizing::new([0u8; CODE_DIGITS]);
        let mut count = 0usize;
        for (index, character) in text.chars().enumerate() {
            if let Some(value) = digit_value(character) {
                if let Some(slot) = digits.get_mut(count) {
                    *slot = value;
                }
                count += 1;
            } else if !is_separator(character) {
                return Err(RecoveryCodeError::NotADigit {
                    position: index + 1,
                });
            }
        }
        if count != CODE_DIGITS {
            return Err(RecoveryCodeError::WrongLength { digits: count });
        }
        let mistyped: Vec<usize> = digits
            .chunks_exact(GROUP_DIGITS)
            .enumerate()
            .filter(|(_, group)| damm(group) != Some(0))
            .map(|(index, _)| index + 1)
            .collect();
        if mistyped.is_empty() {
            Ok(Self { digits })
        } else {
            Err(RecoveryCodeError::Mistyped { groups: mistyped })
        }
    }

    /// The code as it is shown and printed: six groups of five Western digits,
    /// joined by hyphens.
    pub fn formatted(&self) -> Zeroizing<String> {
        let mut text = Zeroizing::new(String::with_capacity(CODE_DIGITS + GROUPS - 1));
        for (index, group) in self.digits.chunks_exact(GROUP_DIGITS).enumerate() {
            if index > 0 {
                text.push('-');
            }
            text.extend(group.iter().map(|digit| char::from(b'0' + digit)));
        }
        text
    }

    /// The thirty digits as ASCII, the password Argon2id derives from.
    fn ascii(&self) -> Zeroizing<[u8; CODE_DIGITS]> {
        let mut ascii = Zeroizing::new([0u8; CODE_DIGITS]);
        for (slot, digit) in ascii.iter_mut().zip(self.digits.iter()) {
            *slot = b'0' + digit;
        }
        ascii
    }
}

impl fmt::Debug for RecoveryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryCode(redacted)")
    }
}

/// The value of `character` as a digit, in any of the three scripts a register
/// may be typed in.
fn digit_value(character: char) -> Option<u8> {
    let zero = match character {
        '0'..='9' => '0',
        '\u{0660}'..='\u{0669}' => '\u{0660}',
        '\u{06F0}'..='\u{06F9}' => '\u{06F0}',
        _ => return None,
    };
    u8::try_from(u32::from(character) - u32::from(zero)).ok()
}

/// What may stand between a code's digits: white space, hyphens and dashes,
/// a minus sign, and the direction marks pasted Arabic text carries.
fn is_separator(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '-' | '\u{2010}'..='\u{2015}' | '\u{2212}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
        )
}

/// A uniformly drawn digit from `byte`, or `None` for the six byte values that
/// would bias the draw: 250 divides by ten, and 256 does not.
fn digit_from_byte(byte: u8) -> Option<u8> {
    (byte < 250).then_some(byte % 10)
}

/// Digits from the OS CSPRNG, drawn without bias, a buffer at a time.
struct RandomDigits {
    buffer: Zeroizing<[u8; 32]>,
    /// How much of the buffer has been drawn; a full count means refill.
    used: usize,
}

impl Default for RandomDigits {
    fn default() -> Self {
        Self {
            buffer: Zeroizing::new([0u8; 32]),
            used: 32,
        }
    }
}

impl RandomDigits {
    fn next(&mut self) -> Result<u8, KeyError> {
        loop {
            if self.used >= self.buffer.len() {
                getrandom::fill(self.buffer.as_mut_slice())
                    .map_err(|error| KeyError::Random(error.to_string()))?;
                self.used = 0;
            }
            let byte = self.buffer.get(self.used).copied();
            self.used += 1;
            if let Some(digit) = byte.and_then(digit_from_byte) {
                return Ok(digit);
            }
        }
    }
}

/// Why a typed recovery code was not read. Never the code, nor any digit of
/// it: a position or a group number only.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryCodeError {
    #[error("character {position} is not a digit")]
    NotADigit { position: usize },
    #[error("a recovery code has {CODE_DIGITS} digits, and this has {digits}")]
    WrongLength { digits: usize },
    #[error("{} does not check: a digit in it was mistyped", describe_groups(.groups))]
    Mistyped {
        /// Every group that does not check, counted from one.
        groups: Vec<usize>,
    },
}

fn describe_groups(groups: &[usize]) -> String {
    match groups {
        [] => "no group".to_owned(),
        [only] => format!("group {only}"),
        [before @ .., last] => {
            let before: Vec<String> = before.iter().map(usize::to_string).collect();
            format!("groups {} and {last}", before.join(", "))
        }
    }
}

/// Why a key could not be wrapped or unwrapped. Never the key or the code.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// The code is not the one the envelope was made with, or the envelope was
    /// altered: authenticated encryption cannot tell the two apart, and
    /// neither opens it.
    #[error("the recovery code does not open this envelope")]
    WrongRecoveryCode,
    #[error("this build cannot read the envelope: {0}")]
    UnsupportedEnvelope(&'static str),
    #[error("the envelope's key derivation parameters are outside what this build accepts")]
    KdfParamsOutOfBounds,
    #[error("the envelope is malformed: {0}")]
    Malformed(&'static str),
    #[error("the OS supplied no randomness: {0}")]
    Random(String),
    #[error("the key derivation failed")]
    Kdf,
    #[error("the key could not be wrapped")]
    Cipher,
    #[error("the envelope file could not be read or written: {0}")]
    EnvelopeIo(#[from] std::io::Error),
    /// The file is not an envelope. Its position only: a parser's message can
    /// quote the bytes it stopped at.
    #[error("the envelope file is not an envelope (line {line}, column {column})")]
    EnvelopeUnreadable { line: usize, column: usize },
}

/// Argon2id's parameters for one envelope, and its salt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KdfParams {
    /// Argon2's version: 19, which is 1.3.
    pub version: u32,
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    /// Sixteen bytes from the OS CSPRNG, in hex. Not a secret.
    #[serde(with = "hex_bytes")]
    pub salt: Vec<u8>,
}

impl KdfParams {
    /// This build's profile, under a fresh salt.
    ///
    /// # Errors
    ///
    /// [`KeyError::Random`] when the OS supplies no randomness.
    pub fn fresh() -> Result<Self, KeyError> {
        let mut salt = vec![0u8; SALT_BYTES];
        getrandom::fill(&mut salt).map_err(|error| KeyError::Random(error.to_string()))?;
        Ok(Self {
            version: ARGON2_VERSION,
            m_cost_kib: M_COST_KIB,
            t_cost: T_COST,
            p_cost: P_COST,
            salt,
        })
    }

    /// The 256-bit wrapping key `code` derives under these parameters, once
    /// they are known to be within bounds.
    fn derive(&self, code: &RecoveryCode) -> Result<Zeroizing<[u8; WRAPPING_KEY_BYTES]>, KeyError> {
        if self.version != ARGON2_VERSION {
            return Err(KeyError::UnsupportedEnvelope("its Argon2 version"));
        }
        if !M_COST_RANGE.contains(&self.m_cost_kib)
            || !T_COST_RANGE.contains(&self.t_cost)
            || !P_COST_RANGE.contains(&self.p_cost)
            || !SALT_RANGE.contains(&self.salt.len())
        {
            return Err(KeyError::KdfParamsOutOfBounds);
        }
        let params = Params::new(
            self.m_cost_kib,
            self.t_cost,
            self.p_cost,
            Some(WRAPPING_KEY_BYTES),
        )
        .map_err(|_| KeyError::KdfParamsOutOfBounds)?;
        let mut key = Zeroizing::new([0u8; WRAPPING_KEY_BYTES]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(code.ascii().as_slice(), &self.salt, key.as_mut_slice())
            .map_err(|_| KeyError::Kdf)?;
        Ok(key)
    }
}

/// The register's data key, wrapped under the merchant's recovery code.
///
/// Nothing in it is secret on its own: opening it takes the code. Its file is
/// [`ENVELOPE_FILE`], beside the database and beside every backup.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryEnvelope {
    /// [`ENVELOPE_FORMAT`].
    pub format: u32,
    /// [`DatabaseKey::id`] of the key inside.
    pub key_id: String,
    /// [`WRAP_ALGORITHM`].
    pub wrap_algorithm: String,
    /// [`KDF_ALGORITHM`].
    pub kdf_algorithm: String,
    pub kdf_params: KdfParams,
    /// The 24-byte nonce, then the sealed key and its 16-byte tag, in hex.
    #[serde(with = "hex_bytes")]
    pub wrapped_key: Vec<u8>,
}

impl fmt::Debug for RecoveryEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecoveryEnvelope")
            .field("format", &self.format)
            .field("key_id", &self.key_id)
            .field("wrap_algorithm", &self.wrap_algorithm)
            .field("kdf_algorithm", &self.kdf_algorithm)
            .field("kdf_params", &self.kdf_params)
            .field(
                "wrapped_key",
                &format_args!("{} bytes, redacted", self.wrapped_key.len()),
            )
            .finish()
    }
}

impl RecoveryEnvelope {
    /// Write the envelope to `path`, whole or not at all: to a temporary file
    /// beside it, flushed to the disk, then renamed over it.
    ///
    /// # Errors
    ///
    /// [`KeyError::EnvelopeIo`].
    pub fn write(&self, path: &Path) -> Result<(), KeyError> {
        let mut json = serde_json::to_vec_pretty(self)
            .map_err(|_| KeyError::Malformed("the envelope could not be written as JSON"))?;
        json.push(b'\n');
        let staged = path.with_extension("json.partial");
        {
            let mut file = File::create(&staged)?;
            file.write_all(&json)?;
            file.sync_all()?;
        }
        fs::rename(&staged, path)?;
        sync_parent(path)?;
        Ok(())
    }

    /// The envelope at `path`.
    ///
    /// # Errors
    ///
    /// [`KeyError::EnvelopeIo`] when it cannot be read, and
    /// [`KeyError::EnvelopeUnreadable`] when it is not an envelope.
    pub fn read(path: &Path) -> Result<Self, KeyError> {
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(|error| KeyError::EnvelopeUnreadable {
            line: error.line(),
            column: error.column(),
        })
    }
}

/// Make a rename in `path`'s directory durable. Unix only: Windows does not
/// open a directory as a file, and its rename is flushed with the file.
#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), KeyError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> Result<(), KeyError> {
    Ok(())
}

/// What the wrap authenticates besides the key: every other field of the
/// envelope, so none of them can be changed without the unwrap failing.
fn associated_data(
    format: u32,
    key_id: &str,
    wrap_algorithm: &str,
    kdf_algorithm: &str,
    params: &KdfParams,
) -> Vec<u8> {
    format!(
        "pos-recovery-envelope\n{format}\n{key_id}\n{wrap_algorithm}\n{kdf_algorithm}\n{}:{}:{}:{}:{}",
        params.version,
        params.m_cost_kib,
        params.t_cost,
        params.p_cost,
        hex_bytes::encode(&params.salt),
    )
    .into_bytes()
}

/// `data_key`, wrapped under `recovery_code` with `params`.
///
/// # Errors
///
/// [`KeyError::Random`] when the OS supplies no nonce; whatever the
/// parameters' bounds or the derivation refuse; [`KeyError::Cipher`].
pub fn wrap_data_key(
    data_key: &DatabaseKey,
    recovery_code: &RecoveryCode,
    params: &KdfParams,
) -> Result<RecoveryEnvelope, KeyError> {
    let wrapping = params.derive(recovery_code)?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(wrapping.as_slice()).map_err(|_| KeyError::Cipher)?;
    let mut nonce = [0u8; NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(|error| KeyError::Random(error.to_string()))?;
    let key_id = data_key.id();
    let aad = associated_data(
        ENVELOPE_FORMAT,
        &key_id,
        WRAP_ALGORITHM,
        KDF_ALGORITHM,
        params,
    );
    let sealed = cipher
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: data_key.expose().as_bytes(),
                aad: &aad,
            },
        )
        .map_err(|_| KeyError::Cipher)?;
    let mut wrapped_key = Vec::with_capacity(NONCE_BYTES + sealed.len());
    wrapped_key.extend_from_slice(&nonce);
    wrapped_key.extend_from_slice(&sealed);
    Ok(RecoveryEnvelope {
        format: ENVELOPE_FORMAT,
        key_id,
        wrap_algorithm: WRAP_ALGORITHM.to_owned(),
        kdf_algorithm: KDF_ALGORITHM.to_owned(),
        kdf_params: params.clone(),
        wrapped_key,
    })
}

/// The data key `envelope` wraps, opened with `recovery_code`.
///
/// # Errors
///
/// [`KeyError::UnsupportedEnvelope`] for a format or algorithm this build does
/// not read; [`KeyError::KdfParamsOutOfBounds`]; [`KeyError::Malformed`] for a
/// wrapped key too short to hold one, or one that does not match the
/// envelope's key id; and [`KeyError::WrongRecoveryCode`] when the code does
/// not open it, or the envelope was altered.
pub fn unwrap_data_key(
    envelope: &RecoveryEnvelope,
    recovery_code: &RecoveryCode,
) -> Result<DatabaseKey, KeyError> {
    if envelope.format != ENVELOPE_FORMAT {
        return Err(KeyError::UnsupportedEnvelope("its format version"));
    }
    if envelope.wrap_algorithm != WRAP_ALGORITHM {
        return Err(KeyError::UnsupportedEnvelope("its wrap algorithm"));
    }
    if envelope.kdf_algorithm != KDF_ALGORITHM {
        return Err(KeyError::UnsupportedEnvelope("its key derivation"));
    }
    let (nonce, sealed) = envelope
        .wrapped_key
        .split_at_checked(NONCE_BYTES)
        .ok_or(KeyError::Malformed("the wrapped key is too short"))?;
    if sealed.len() <= TAG_BYTES {
        return Err(KeyError::Malformed("the wrapped key is too short"));
    }
    let nonce: [u8; NONCE_BYTES] = nonce
        .try_into()
        .map_err(|_| KeyError::Malformed("the wrapped key is too short"))?;
    let wrapping = envelope.kdf_params.derive(recovery_code)?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(wrapping.as_slice()).map_err(|_| KeyError::Cipher)?;
    let aad = associated_data(
        envelope.format,
        &envelope.key_id,
        &envelope.wrap_algorithm,
        &envelope.kdf_algorithm,
        &envelope.kdf_params,
    );
    let opened = Zeroizing::new(
        cipher
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: sealed,
                    aad: &aad,
                },
            )
            .map_err(|_| KeyError::WrongRecoveryCode)?,
    );
    let passphrase = std::str::from_utf8(&opened)
        .map_err(|_| KeyError::Malformed("the wrapped key is not a passphrase"))?;
    let key = DatabaseKey::new(passphrase.to_owned());
    if key.id() != envelope.key_id {
        return Err(KeyError::Malformed(
            "the key inside is not the one the envelope names",
        ));
    }
    Ok(key)
}

/// Bytes as lowercase hex in the envelope's JSON.
mod hex_bytes {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn encode(bytes: &[u8]) -> String {
        bytes
            .iter()
            .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
                use std::fmt::Write as _;
                // Writing to a String is infallible; the Result is the trait's.
                let _ = write!(text, "{byte:02x}");
                text
            })
    }

    fn decode(text: &str) -> Option<Vec<u8>> {
        if !text.len().is_multiple_of(2) {
            return None;
        }
        // `from_str_radix` alone would read a leading `+` as a sign.
        if !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        text.as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let pair = std::str::from_utf8(pair).ok()?;
                u8::from_str_radix(pair, 16).ok()
            })
            .collect()
    }

    pub(super) fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&encode(bytes))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        decode(&text).ok_or_else(|| D::Error::custom("expected hex"))
    }
}

#[cfg(test)]
mod key_custody {
    //! Microstep 1.8.5b's tests for the envelope and the key. The step's
    //! `Done when` runs them as `cargo nextest run -p pos-db key_custody::`.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::path::Path;

    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;

    use super::*;
    use crate::DbError;
    use crate::key::{self, KeySource, MemoryCredentialStore};

    const DATABASE: &str = "register.db";

    /// One way to forge an envelope, and what it changes.
    type Alteration = (&'static str, fn(&mut RecoveryEnvelope));

    /// A passphrase as production mints one, 64 hex characters, written out.
    const PASSPHRASE: &str = "3f1c9a7e0b2d4c6e8f1a3b5c7d9e0f2a4b6c8d0e1f3a5b7c9d0e2f4a6b8c0d1e";

    /// Something only this database holds: one setting row.
    fn keep_a_row(database: &Path, key: &DatabaseKey) {
        let conn = crate::open(database, key.expose()).unwrap();
        conn.execute(
            "INSERT INTO setting (scope, scope_id, key, value_json)
             VALUES ('register', x'01', 'recovery.drill', '\"kept\"')",
            [],
        )
        .unwrap();
    }

    fn the_row(database: &Path, key: &DatabaseKey) -> String {
        crate::open(database, key.expose())
            .unwrap()
            .query_row(
                "SELECT value_json FROM setting WHERE key = 'recovery.drill'",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    /// The code's thirty digits, with no separator.
    fn digits_of(code: &RecoveryCode) -> String {
        code.formatted()
            .chars()
            .filter(char::is_ascii_digit)
            .collect()
    }

    /// A code from 24 random digits, four to a group.
    fn code_from(random: [u8; 24]) -> RecoveryCode {
        RecoveryCode::from_random_digits(&random).unwrap()
    }

    /// The step's `Done when`: a fixture backup opens after its credential-store
    /// entry is deleted, with only its envelope and the once-displayed code. The
    /// register mints its key on a clean machine and writes a row; the backup is
    /// the database file copied beside the envelope. Then the machine and its
    /// credential store are lost, and the code, typed back from what was
    /// printed, opens the copy, whose row is there.
    #[test]
    fn a_backup_opens_with_the_recovery_code_alone() {
        let machine = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let store = MemoryCredentialStore::new();
        let database = machine.path().join(DATABASE);

        let (key, source) = key::resolve(None, &store, &database).unwrap();
        assert_eq!(source, KeySource::CredentialStore);
        assert_eq!(store.held().as_ref(), Some(&key), "the key was kept");
        keep_a_row(&database, &key);
        let code = RecoveryCode::generate().unwrap();
        let printed = code.formatted();
        let envelope = wrap_data_key(&key, &code, &KdfParams::fresh().unwrap()).unwrap();
        envelope.write(&backups.path().join(ENVELOPE_FILE)).unwrap();
        fs::copy(&database, backups.path().join(DATABASE)).unwrap();
        drop((key, code, envelope));

        // The machine and its credential store are lost together.
        store.forget();
        drop(machine);

        let envelope = RecoveryEnvelope::read(&backups.path().join(ENVELOPE_FILE)).unwrap();
        let typed = RecoveryCode::parse(&printed).unwrap();
        let key = unwrap_data_key(&envelope, &typed).unwrap();
        assert_eq!(the_row(&backups.path().join(DATABASE), &key), "\"kept\"");
        assert_eq!(store.held(), None, "nothing was minted along the way");
    }

    /// A code that is not the envelope's opens nothing, and so does an envelope
    /// with any field altered: authenticated encryption binds them all. A
    /// derivation outside the bounds this build accepts, and a format or
    /// algorithm it does not read, are refused before anything is derived.
    #[test]
    fn wrong_recovery_code_is_refused() {
        let key = DatabaseKey::new(PASSPHRASE.to_owned());
        let code = code_from([
            1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4,
        ]);
        let other = code_from([
            1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 5,
        ]);
        let envelope = wrap_data_key(&key, &code, &KdfParams::fresh().unwrap()).unwrap();

        assert_eq!(unwrap_data_key(&envelope, &code).unwrap(), key);
        assert!(matches!(
            unwrap_data_key(&envelope, &other),
            Err(KeyError::WrongRecoveryCode)
        ));

        let altered: [Alteration; 4] = [
            ("the key id", |e| {
                e.key_id = DatabaseKey::new("another".to_owned()).id();
            }),
            ("a byte of the sealed key", |e| {
                if let Some(byte) = e.wrapped_key.last_mut() {
                    *byte ^= 1;
                }
            }),
            ("a byte of the salt", |e| {
                if let Some(byte) = e.kdf_params.salt.first_mut() {
                    *byte ^= 1;
                }
            }),
            ("the passes", |e| e.kdf_params.t_cost += 1),
        ];
        for (what, alter) in altered {
            let mut forged = envelope.clone();
            alter(&mut forged);
            assert!(
                matches!(
                    unwrap_data_key(&forged, &code),
                    Err(KeyError::WrongRecoveryCode)
                ),
                "{what}"
            );
        }

        let refused_first: [Alteration; 7] = [
            ("4 GiB asked of the register", |e| {
                e.kdf_params.m_cost_kib = 4 * 1024 * 1024;
            }),
            ("less memory than this build writes", |e| {
                e.kdf_params.m_cost_kib = 1024
            }),
            ("one pass", |e| e.kdf_params.t_cost = 1),
            ("a short salt", |e| e.kdf_params.salt.truncate(8)),
            ("a later format", |e| e.format = 2),
            ("another wrap", |e| {
                e.wrap_algorithm = "aes256gcm".to_owned()
            }),
            ("another derivation", |e| {
                e.kdf_algorithm = "scrypt".to_owned()
            }),
        ];
        for (what, alter) in refused_first {
            let mut forged = envelope.clone();
            alter(&mut forged);
            assert!(
                matches!(
                    unwrap_data_key(&forged, &code),
                    Err(KeyError::KdfParamsOutOfBounds | KeyError::UnsupportedEnvelope(_))
                ),
                "{what}"
            );
        }
        let mut short = envelope.clone();
        short.wrapped_key.truncate(NONCE_BYTES + TAG_BYTES);
        assert!(matches!(
            unwrap_data_key(&short, &code),
            Err(KeyError::Malformed(_))
        ));
    }

    /// A database whose key is gone is never given a new one: the key would open
    /// nothing, and keeping it would leave the store holding the wrong key. The
    /// store that kept its key still answers with it, and a clean machine, with
    /// neither a key nor a database, mints one once and keeps it.
    #[test]
    fn key_generation_refuses_when_a_database_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join(DATABASE);
        let kept = MemoryCredentialStore::new();
        let (key, _) = key::resolve(None, &kept, &database).unwrap();
        keep_a_row(&database, &key);

        let wiped = MemoryCredentialStore::new();
        assert!(matches!(
            key::resolve(None, &wiped, &database),
            Err(DbError::KeyMissingForExistingDatabase)
        ));
        assert_eq!(wiped.held(), None, "no key was minted");
        assert!(matches!(
            key::resolve_existing(None, &wiped),
            Err(DbError::KeyMissingForExistingDatabase)
        ));

        let (again, source) = key::resolve(None, &kept, &database).unwrap();
        assert_eq!((again, source), (key.clone(), KeySource::CredentialStore));

        let clean = MemoryCredentialStore::new();
        let (minted, _) = key::resolve(None, &clean, &dir.path().join("another.db")).unwrap();
        assert_eq!(clean.held().as_ref(), Some(&minted));
        assert_ne!(minted, key);
        assert_eq!(minted.expose().len(), 64);
        assert!(minted.expose().chars().all(|c| c.is_ascii_hexdigit()));

        // Whether a database is there cannot be told through a file: no key.
        #[cfg(unix)]
        {
            let file = dir.path().join("a-file");
            fs::write(&file, b"").unwrap();
            assert!(matches!(
                key::resolve(None, &MemoryCredentialStore::new(), &file.join(DATABASE)),
                Err(DbError::DatabasePresenceUnknown(_))
            ));
        }
    }

    /// The envelope is a file a later process reads back exactly, written whole:
    /// nothing staged is left beside it, and a rewrite replaces it, after which
    /// the old code opens nothing there. Its shape is written out here.
    #[test]
    fn recovery_envelope_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(ENVELOPE_FILE);
        let key = DatabaseKey::new(PASSPHRASE.to_owned());
        let code = RecoveryCode::generate().unwrap();
        let envelope = wrap_data_key(&key, &code, &KdfParams::fresh().unwrap()).unwrap();
        envelope.write(&path).unwrap();

        let read = RecoveryEnvelope::read(&path).unwrap();
        assert_eq!(read, envelope);
        assert_eq!(unwrap_data_key(&read, &code).unwrap(), key);
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, [ENVELOPE_FILE]);

        let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let field = |name: &str| json.get(name).cloned().unwrap();
        assert_eq!(field("format"), serde_json::json!(1));
        assert_eq!(field("key_id"), serde_json::json!(key.id()));
        assert_eq!(
            field("wrap_algorithm"),
            serde_json::json!("xchacha20poly1305")
        );
        assert_eq!(field("kdf_algorithm"), serde_json::json!("argon2id"));
        let params = field("kdf_params");
        for (name, value) in [
            ("version", 19),
            ("m_cost_kib", 65_536),
            ("t_cost", 3),
            ("p_cost", 4),
        ] {
            assert_eq!(params.get(name), Some(&serde_json::json!(value)), "{name}");
        }
        let hex_len = |value: Option<&serde_json::Value>| {
            value.and_then(serde_json::Value::as_str).map(str::len)
        };
        assert_eq!(hex_len(params.get("salt")), Some(32), "a 16-byte salt");
        assert_eq!(
            hex_len(json.get("wrapped_key")),
            Some(2 * (24 + 64 + 16)),
            "a nonce, the sealed passphrase and its tag"
        );

        let reissued = RecoveryCode::generate().unwrap();
        let replacement = wrap_data_key(&key, &reissued, &KdfParams::fresh().unwrap()).unwrap();
        replacement.write(&path).unwrap();
        let read = RecoveryEnvelope::read(&path).unwrap();
        assert_eq!(read, replacement);
        assert!(matches!(
            unwrap_data_key(&read, &code),
            Err(KeyError::WrongRecoveryCode)
        ));

        for not_an_envelope in [&b"{\"format\": 1}"[..], b"not json", b""] {
            fs::write(&path, not_an_envelope).unwrap();
            assert!(matches!(
                RecoveryEnvelope::read(&path),
                Err(KeyError::EnvelopeUnreadable { .. })
            ));
        }
        let mut extra = serde_json::to_value(&envelope).unwrap();
        if let Some(object) = extra.as_object_mut() {
            object.insert("note".to_owned(), serde_json::json!("x"));
        }
        fs::write(&path, serde_json::to_vec(&extra).unwrap()).unwrap();
        assert!(matches!(
            RecoveryEnvelope::read(&path),
            Err(KeyError::EnvelopeUnreadable { .. })
        ));
    }

    /// The code, the key and the wrapped key never print themselves, a typed
    /// code's refusal names a place and never a digit, and the envelope's file
    /// holds neither the code nor the key.
    #[test]
    fn recovery_code_and_wrapped_key_are_redacted() {
        let dir = tempfile::tempdir().unwrap();
        let key = DatabaseKey::new(PASSPHRASE.to_owned());
        let code = RecoveryCode::generate().unwrap();
        let printed = code.formatted();
        let envelope = wrap_data_key(&key, &code, &KdfParams::fresh().unwrap()).unwrap();

        assert_eq!(format!("{code:?}"), "RecoveryCode(redacted)");
        assert_eq!(format!("{key:?}"), "DatabaseKey(redacted)");
        let shown = format!("{envelope:?}");
        assert!(
            shown.contains("wrapped_key: 104 bytes, redacted"),
            "{shown}"
        );
        assert!(!shown.contains(&hex_bytes::encode(&envelope.wrapped_key)));
        assert!(!shown.contains(PASSPHRASE));

        let mut typo: Vec<char> = printed.chars().collect();
        if let Some(first) = typo.first_mut() {
            *first = if *first == '9' { '0' } else { '9' };
        }
        let typo: String = typo.into_iter().collect();
        let refusals = [
            RecoveryCode::parse(&typo).unwrap_err(),
            RecoveryCode::parse(&format!("{}x", printed.as_str())).unwrap_err(),
            RecoveryCode::parse(&format!("{}1", printed.as_str())).unwrap_err(),
        ];
        assert_eq!(
            refusals.clone().map(|refusal| refusal.to_string()),
            [
                "group 1 does not check: a digit in it was mistyped".to_owned(),
                "character 36 is not a digit".to_owned(),
                "a recovery code has 30 digits, and this has 31".to_owned(),
            ]
        );

        envelope.write(&dir.path().join(ENVELOPE_FILE)).unwrap();
        let file = fs::read_to_string(dir.path().join(ENVELOPE_FILE)).unwrap();
        assert!(!file.contains(printed.as_str()));
        assert!(!file.contains(&digits_of(&code)));
        assert!(!file.contains(PASSPHRASE));
    }

    /// The Damm table's published example: 572 is closed by 4, and 5724 checks.
    #[test]
    fn the_check_digit_is_damms() {
        assert_eq!(damm(&[5, 7, 2]), Some(4));
        assert_eq!(damm(&[5, 7, 2, 4]), Some(0));
        assert_eq!(damm(&[5, 10]), None, "ten is not a digit");
    }

    /// Bytes become digits without bias: each digit from exactly 25 of the 250
    /// bytes kept, and the six above them drawn again.
    #[test]
    fn recovery_digits_are_drawn_without_bias() {
        let mut counts = [0usize; 10];
        for byte in u8::MIN..=u8::MAX {
            if let Some(digit) = digit_from_byte(byte)
                && let Some(count) = counts.get_mut(usize::from(digit))
            {
                *count += 1;
            }
        }
        assert_eq!(counts, [25; 10]);
        assert!((250..=255).all(|byte| digit_from_byte(byte).is_none()));
    }

    /// Two generated codes read back from their printed form, and differ.
    #[test]
    fn generated_codes_check_and_never_repeat() {
        let a = RecoveryCode::generate().unwrap();
        let b = RecoveryCode::generate().unwrap();
        assert_eq!(RecoveryCode::parse(&a.formatted()).unwrap(), a);
        assert_eq!(RecoveryCode::parse(&b.formatted()).unwrap(), b);
        assert_ne!(a, b);
        assert_eq!(a.formatted().len(), 35, "six groups of five, five hyphens");
    }

    const ARABIC_INDIC: [char; 10] = ['٠', '١', '٢', '٣', '٤', '٥', '٦', '٧', '٨', '٩'];
    const EASTERN_ARABIC_INDIC: [char; 10] = ['۰', '۱', '۲', '۳', '۴', '۵', '۶', '۷', '۸', '۹'];

    /// `text` with its Western digits written from `script`.
    fn in_script(text: &str, script: &[char; 10]) -> String {
        text.chars()
            .map(|character| {
                character
                    .to_digit(10)
                    .and_then(|digit| script.get(usize::try_from(digit).ok()?).copied())
                    .unwrap_or(character)
            })
            .collect()
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// Any code reads back from its printed form, from its digits run
        /// together, from spaces and a right-to-left mark in place of hyphens,
        /// and from Arabic-Indic or Eastern Arabic-Indic digits.
        #[test]
        fn prop_a_code_reads_back_however_it_is_typed(random in proptest::array::uniform24(0u8..10)) {
            let code = code_from(random);
            let printed = code.formatted();
            let spaced = printed.replace('-', " \u{200F}");
            for typed in [
                printed.to_string(),
                digits_of(&code),
                spaced,
                in_script(&printed, &ARABIC_INDIC),
                in_script(&printed, &EASTERN_ARABIC_INDIC),
            ] {
                prop_assert_eq!(&RecoveryCode::parse(&typed).unwrap(), &code);
            }
        }

        /// Any one digit typed wrong is refused, naming its group.
        #[test]
        fn prop_one_mistyped_digit_names_its_group(
            random in proptest::array::uniform24(0u8..10),
            position in 0usize..30,
            shift in 1u32..10,
        ) {
            let code = code_from(random);
            let typed: String = digits_of(&code)
                .chars()
                .enumerate()
                .map(|(index, digit)| {
                    if index == position {
                        char::from_digit((digit.to_digit(10).unwrap() + shift) % 10, 10).unwrap()
                    } else {
                        digit
                    }
                })
                .collect();
            prop_assert_eq!(
                RecoveryCode::parse(&typed),
                Err(RecoveryCodeError::Mistyped { groups: vec![position / 5 + 1] })
            );
        }

        /// Any two neighbouring digits of a group swapped are refused, naming
        /// the group; two equal digits swapped are the same code.
        #[test]
        fn prop_two_swapped_neighbours_name_their_group(
            random in proptest::array::uniform24(0u8..10),
            group in 0usize..6,
            offset in 0usize..4,
        ) {
            let code = code_from(random);
            let digits: Vec<char> = digits_of(&code).chars().collect();
            let first = group * 5 + offset;
            let mut swapped = digits.clone();
            swapped.swap(first, first + 1);
            let typed: String = swapped.into_iter().collect();
            if digits.get(first) == digits.get(first + 1) {
                prop_assert_eq!(RecoveryCode::parse(&typed), Ok(code));
            } else {
                prop_assert_eq!(
                    RecoveryCode::parse(&typed),
                    Err(RecoveryCodeError::Mistyped { groups: vec![group + 1] })
                );
            }
        }
    }
}
