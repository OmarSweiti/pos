//! What the finalize and a shift's opening read of a store (`ref/schema.md`
//! §0003, §0005).
//!
//! A completed sale's receipt names its merchant, and 1.8.3's finalize must
//! know before it writes anything whether the store may complete a sale at
//! all. Both come from the store's row and its organisation's.
//!
//! **A shift opens on the store's trading day** (1.9.5, conventions §11): its
//! float is counted in the store's currency, and its business date, when the
//! clock is trusted to derive one, turns at the store's own cutover in its
//! own zone. [`TradingDay`] is those three columns, read as stored. The zone
//! stays its IANA name: resolving it to an offset at an instant is the
//! shell's, with the time-zone database it ships.
//!
//! **The fiscal decision is the schema's own rule.** `0005`'s
//! `sale_completed_requires_fiscal_decision_*` triggers refuse a live sale
//! unless the store has an evidenced decision: obliged to report with
//! JoFotara enabled and the taxpayer type known, or exempt with JoFotara
//! disabled. [`FiscalDecision`] is that rule, read in the same SQL terms, so
//! the finalize can refuse by name what the trigger would refuse as a bare
//! constraint failure.

use std::fmt;

use pos_domain::{Currency, Language, StoreId};
use rusqlite::{Connection, OptionalExtension};

use crate::DbError;

/// Whether a store may complete a live sale, and what it owes once it does
/// (`0005`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiscalDecision {
    /// No evidenced decision: `pending_evidence`, or a stated obligation the
    /// rest of the row does not support. No live sale completes.
    Undecided,
    /// The store reports each sale to JoFotara.
    Required,
    /// The store is evidenced exempt from e-invoicing.
    Exempt,
}

/// A store as the finalize and its receipt read it.
///
/// `Debug` leaves out the phone (`.claude/rules/security.md`).
#[derive(Clone, PartialEq, Eq)]
pub struct StoreProfile {
    /// The organisation's legal name, as registered.
    pub legal_name: String,
    /// The organisation's tax number, printed on every receipt.
    pub tin: Option<String>,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    /// The language its receipts are set in.
    pub receipt_language: Language,
    /// Whether it is registered for GST. A store that is not charges no tax.
    pub gst_registered: bool,
    pub fiscal: FiscalDecision,
}

impl StoreProfile {
    /// The store's name in `language`: its Arabic name when it has no other.
    #[must_use]
    pub fn name_in(&self, language: Language) -> &str {
        match language {
            Language::English => self.name_en.as_deref().unwrap_or(&self.name_ar),
            Language::Arabic => &self.name_ar,
        }
    }
}

impl fmt::Debug for StoreProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoreProfile")
            .field("legal_name", &self.legal_name)
            .field("tin", &self.tin)
            .field("name_ar", &self.name_ar)
            .field("name_en", &self.name_en)
            .field("receipt_language", &self.receipt_language)
            .field("gst_registered", &self.gst_registered)
            .field("fiscal", &self.fiscal)
            .finish_non_exhaustive()
    }
}

/// The day a store trades on, as a shift's opening reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradingDay {
    /// The currency the store sells in, and a shift's float is counted in.
    pub currency: Currency,
    /// Its IANA time zone, `Asia/Amman`, unresolved.
    pub time_zone: String,
    /// The local minute its business day turns at: `240` is 04:00.
    pub cutover_minutes: u16,
}

/// The minutes in a day: a cutover is a minute before the next midnight.
const MINUTES_PER_DAY: u16 = 1_440;

/// Reads stores. It writes nothing.
pub struct StoreRepository<'c> {
    conn: &'c Connection,
}

impl<'c> StoreRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// `store`, and its organisation's name and tax number.
    ///
    /// # Errors
    ///
    /// [`DbError::StoreUnknown`] for a store this database does not hold;
    /// [`DbError::StoredStoreInvalid`] for a receipt language this build does
    /// not print in; and [`DbError::Sqlite`] if the read fails.
    pub fn profile(&self, store: StoreId) -> Result<StoreProfile, DbError> {
        let row = self
            .conn
            .query_row(
                "SELECT o.legal_name, o.tin, s.name_ar, s.name_en, s.address, s.phone,
                        s.receipt_locale, s.gst_profile <> 'unregistered',
                        CASE
                          WHEN s.fiscal_obligation = 'required'
                               AND s.fiscal_profile = 'jordan_jofotara'
                               AND s.fiscal_taxpayer_type IS NOT NULL
                               AND s.fiscal_obligation_evidence_ref IS NOT NULL
                            THEN 'required'
                          WHEN s.fiscal_obligation = 'exempt'
                               AND s.fiscal_profile = 'disabled'
                               AND s.fiscal_obligation_evidence_ref IS NOT NULL
                            THEN 'exempt'
                          ELSE 'undecided'
                        END
                   FROM store s JOIN org o ON o.id = s.org_id
                  WHERE s.id = ?1",
                [store.as_uuid().as_bytes().as_slice()],
                |row| {
                    Ok((
                        (
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ),
                        (
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, bool>(7)?,
                            row.get::<_, String>(8)?,
                        ),
                    ))
                },
            )
            .optional()?;
        let Some(((legal_name, tin, name_ar, name_en), (address, phone, locale, gst, fiscal))) =
            row
        else {
            return Err(DbError::StoreUnknown(store));
        };
        let receipt_language = match locale.as_str() {
            "ar" => Language::Arabic,
            "en" => Language::English,
            other => {
                return Err(DbError::StoredStoreInvalid {
                    reason: format!("receipt_locale {other:?} is not a language receipts print in"),
                });
            }
        };
        let fiscal = match fiscal.as_str() {
            "required" => FiscalDecision::Required,
            "exempt" => FiscalDecision::Exempt,
            _ => FiscalDecision::Undecided,
        };
        Ok(StoreProfile {
            legal_name,
            tin,
            name_ar,
            name_en,
            address,
            phone,
            receipt_language,
            gst_registered: gst,
            fiscal,
        })
    }

    /// The day `store` trades on: its currency, its time zone and its cutover.
    ///
    /// # Errors
    ///
    /// [`DbError::StoreUnknown`] for a store this database does not hold;
    /// [`DbError::StoredStoreInvalid`] for a currency this build does not
    /// know, or a cutover that is not a minute of the day; and
    /// [`DbError::Sqlite`] if the read fails.
    pub fn trading_day(&self, store: StoreId) -> Result<TradingDay, DbError> {
        let row = self
            .conn
            .query_row(
                "SELECT currency, time_zone, day_cutover_minutes FROM store WHERE id = ?1",
                [store.as_uuid().as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((currency, time_zone, cutover)) = row else {
            return Err(DbError::StoreUnknown(store));
        };
        let currency = Currency::from_code(&currency).map_err(|_| DbError::StoredStoreInvalid {
            reason: format!("currency {currency:?} is not one this build knows"),
        })?;
        let cutover_minutes = u16::try_from(cutover)
            .ok()
            .filter(|minutes| *minutes < MINUTES_PER_DAY)
            .ok_or_else(|| DbError::StoredStoreInvalid {
                reason: format!("day_cutover_minutes {cutover} is not a minute of the day"),
            })?;
        Ok(TradingDay {
            currency,
            time_zone,
            cutover_minutes,
        })
    }
}
