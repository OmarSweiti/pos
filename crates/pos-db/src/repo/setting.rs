//! The merchant's settings a command reads (`setting`, `0003`): one row per
//! scope, key and value, the value as JSON.
//!
//! **A setting is read at its store.** `setting` also holds org and register
//! rows, and no precedence among the three scopes is decided yet, so nothing
//! here reads org rows: a value set for the whole organisation is not mistaken
//! for one store's. The one register row read here is the printer binding
//! (1.7.14), which is a register's own and has no store or organisation form,
//! so no precedence is needed to read it.
//!
//! **A value is checked as it is read.** A stored value that is not the shape
//! its key promises is refused as [`DbError::StoredSettingInvalid`], naming
//! the key and never quoting the value. It is never read as the default
//! either: a store that narrowed a list must not find it widened again by a
//! typo.

use std::net::SocketAddr;
use std::num::NonZeroU32;

use pos_domain::{RegisterId, StoreId};
use rusqlite::{Connection, OptionalExtension, params};

use crate::DbError;

/// The setting that lists the reasons a price may be overridden for, as a
/// JSON array of codes (merchant decision 3.7).
pub const PRICE_OVERRIDE_REASONS: &str = "price.override_reasons";

/// The reasons a price may be overridden for at a store that has set none:
/// merchant decision 3.7's default, which the owner adopted on 6 November
/// 2026.
pub const DEFAULT_PRICE_OVERRIDE_REASONS: &[&str] =
    &[DISPLAYED_PRICE, "damaged", "price_match", "catalogue_error"];

/// The shelf's price (J.3, E.70), which no store's list can leave out: the
/// customer pays what the shelf showed. `pos_domain::OverrideReason`'s own
/// code for it.
const DISPLAYED_PRICE: &str = "displayed_price";

/// The setting that binds a register to its receipt printer (1.7.14): a
/// register-scope row whose value is
/// `{ "profile_id": "…", "version": 1, "address": "192.0.2.10:9100" }`.
pub const PRINTER_BINDING: &str = "printer.binding";

/// A register's printer binding, as its row states it: which shipped profile,
/// by id and version, and where the device is. Whether the build ships that
/// profile, and whether the driver can work from it, is
/// `pos_hardware::profile::data::printer`'s to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterBinding {
    pub profile_id: String,
    pub version: NonZeroU32,
    /// An IP address and a port, never a host name: the register opens without
    /// a network lookup, which could hold the till for as long as a resolver
    /// takes to give up.
    pub address: SocketAddr,
}

/// The longest reason code, in bytes.
pub const REASON_CODE_MAX_LEN: usize = 32;

/// Whether `code` is shaped like a reason code: a lowercase ASCII letter, then
/// lowercase letters, digits and underscores, at most
/// [`REASON_CODE_MAX_LEN`] bytes. A code is what a report groups by, so free
/// text never is one, and nothing a person types about a customer can reach
/// the audit chain through it.
#[must_use]
pub fn is_reason_code(code: &str) -> bool {
    let mut bytes = code.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && code.len() <= REASON_CODE_MAX_LEN
}

/// Reads of the merchant's settings.
pub struct SettingRepository<'c> {
    conn: &'c Connection,
}

impl<'c> SettingRepository<'c> {
    #[must_use]
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// The reasons a price may be overridden for at `store`: the store's own
    /// [`PRICE_OVERRIDE_REASONS`] list, or [`DEFAULT_PRICE_OVERRIDE_REASONS`]
    /// when it set none. `displayed_price` is always among them, first, and
    /// the rest keep the order the store gave them.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredSettingInvalid`] for a stored value that is not a
    /// JSON array of distinct reason codes ([`is_reason_code`]), and
    /// [`DbError::Sqlite`] if the read fails.
    pub fn price_override_reasons(&self, store: StoreId) -> Result<Vec<String>, DbError> {
        let Some(value) = self.store_value(store, PRICE_OVERRIDE_REASONS)? else {
            return Ok(DEFAULT_PRICE_OVERRIDE_REASONS
                .iter()
                .map(|code| (*code).to_owned())
                .collect());
        };
        let invalid = |reason: &'static str| DbError::StoredSettingInvalid {
            key: PRICE_OVERRIDE_REASONS,
            reason,
        };
        let serde_json::Value::Array(items) = value else {
            return Err(invalid("not a list"));
        };
        let mut reasons = vec![DISPLAYED_PRICE.to_owned()];
        let mut listed = Vec::with_capacity(items.len());
        for item in items {
            let serde_json::Value::String(code) = item else {
                return Err(invalid("an entry is not text"));
            };
            if !is_reason_code(&code) {
                return Err(invalid("an entry is not a reason code"));
            }
            if listed.contains(&code) {
                return Err(invalid("a code is listed twice"));
            }
            listed.push(code);
        }
        reasons.extend(listed.into_iter().filter(|code| code != DISPLAYED_PRICE));
        Ok(reasons)
    }

    /// `register`'s printer binding, or `None` when none is set. Only the
    /// register's own row is read: a binding is never a store's or an
    /// organisation's.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredSettingInvalid`] for a value that is not an object of
    /// exactly `profile_id` (text), `version` (a whole number from 1) and
    /// `address` (an IP address and a port), and [`DbError::Sqlite`] if the
    /// read fails.
    pub fn printer_binding(&self, register: RegisterId) -> Result<Option<PrinterBinding>, DbError> {
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT value_json FROM setting
                  WHERE scope = 'register' AND scope_id = ?1 AND key = ?2",
                params![register.as_uuid().as_bytes().as_slice(), PRINTER_BINDING],
                |row| row.get(0),
            )
            .optional()?;
        let Some(json) = stored else {
            return Ok(None);
        };
        let invalid = |reason: &'static str| DbError::StoredSettingInvalid {
            key: PRINTER_BINDING,
            reason,
        };
        let Ok(serde_json::Value::Object(fields)) = serde_json::from_str(&json) else {
            return Err(invalid("not a JSON object"));
        };
        if fields.len() != 3 {
            return Err(invalid("not exactly profile_id, version and address"));
        }
        let Some(serde_json::Value::String(profile_id)) = fields.get("profile_id") else {
            return Err(invalid("profile_id is not text"));
        };
        let version = fields
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .and_then(NonZeroU32::new)
            .ok_or_else(|| invalid("version is not a whole number from 1"))?;
        let address = fields
            .get("address")
            .and_then(serde_json::Value::as_str)
            .and_then(|address| address.parse::<SocketAddr>().ok())
            .ok_or_else(|| invalid("address is not an IP address and a port"))?;
        Ok(Some(PrinterBinding {
            profile_id: profile_id.clone(),
            version,
            address,
        }))
    }

    /// `key`'s value at `store`, parsed, or `None` when the store set none.
    fn store_value(
        &self,
        store: StoreId,
        key: &'static str,
    ) -> Result<Option<serde_json::Value>, DbError> {
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT value_json FROM setting
                  WHERE scope = 'store' AND scope_id = ?1 AND key = ?2",
                params![store.as_uuid().as_bytes().as_slice(), key],
                |row| row.get(0),
            )
            .optional()?;
        stored
            .map(|json| {
                serde_json::from_str(&json).map_err(|_| DbError::StoredSettingInvalid {
                    key,
                    reason: "not JSON",
                })
            })
            .transpose()
    }
}
