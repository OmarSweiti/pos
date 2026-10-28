//! A jurisdiction's tax pack, pinned, imported and read back (microstep 1.3.7,
//! `ref/tax-jordan.md` §3 rule 6 and §7, `ref/schema.md` §0003).
//!
//! Rates are data. Every band a store charges, the standard band included,
//! arrives in one [`PinnedTaxPack`]: the official source it was taken from,
//! that source's version, the categories and their rates with real effective
//! dates, and a hash fixed when the snapshot was reviewed.
//! [`import_tax_pack`] writes it only if its content still hashes to that pin,
//! and only with the accountant's dated approval, so what a store charges is
//! exactly what was reviewed and approved. [`TaxPackRepository::components_for`]
//! reads back what a store's approved pack charges on one category on one
//! business date, as the components `pos_domain::compute_line_tax` is handed.
//!
//! **No real pack ships.** Which categories and rates apply to the merchant's
//! actual assortment is `ref/schema.md` §0003's `⚠️ OPEN` item, and its answer
//! is the current official ISTD catalogue plus the merchant's accountant's
//! classification. Until both arrive no `tax_rule_pack` is approved, no store
//! can be provisioned, and the tests prove this path on a visibly synthetic
//! fixture instead.
//!
//! ## What a pack can say
//!
//! One component per category, a percentage of the line's net, effective over
//! store-local business dates. That is every Phase-1 band: standard, reduced,
//! zero-rated and exempt general sales tax. A fixed amount, a second component
//! and a tax on a tax are Special Sales Tax, which `ref/tax-jordan.md` §2 keeps
//! out until its schedule and the merchant's evidence exist and whose base is
//! `ref/domain-api.md` §5's `⚠️ OPEN` item, so this format cannot express
//! them. Their arrival is a new [`FORMAT`], hashed differently.
//!
//! Dates are business dates because `0003` stores them so and `0005`'s
//! completion gate compares them with `sale.business_date`. The reader filters
//! the same way, from `valid_from` inclusive to `valid_to` exclusive, so the
//! components it returns are the ones that gate will accept.
//!
//! ## Refused, by name
//!
//! Before anything is written, [`import_tax_pack`] refuses a pack whose
//! content no longer matches its pin, then any pack that does not hold
//! together on its own terms: a band listed with no rate (an unconfigured
//! band), a rate for a band not listed, a zero-rated or exempt band that
//! charges or a standard or reduced one that charges nothing, overlapping or
//! empty periods, a second component on one band. Then what it finds in the
//! database: an approver this register does not know, a version already
//! imported, a category whose code exists with other names or another
//! treatment, and a zone pack that does not cover every band the approved
//! standard pack charges (rule 3: a zone never inherits the mainland's rate).
//! Every refusal is a [`TaxPackRefusal`].

use std::collections::{BTreeMap, BTreeSet};

use pos_domain::{
    BusinessDate, IdSource, Percent, StoreId, StoreTaxProfile, TaxBase, TaxBasis, TaxCategoryId,
    TaxComponent, TaxTreatment, Timestamp, UserId,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::Value;

use crate::DbError;

/// Ids are a UUIDv7's sixteen bytes, as everywhere in `pos-db`.
pub type Id = [u8; 16];

/// The format a pack's content is hashed in. What a pack can say changes only
/// with this string, and so does every hash.
pub const FORMAT: &str = "pos-tax-pack/1";

/// A jurisdiction pack as reviewed: its source, its bands, and the hash that
/// pins them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedTaxPack {
    /// The profile the pack is for. `Unregistered` charges no GST and has no
    /// pack (1.3.8).
    pub profile: StoreTaxProfile,
    /// This pack's version within its profile. A change of rate is a new
    /// version, never an edit: `0003` freezes an approved pack.
    pub pack_version: String,
    /// The official source and its version: the pinned ISTD catalogue
    /// snapshot the bands were taken from.
    pub source_ref: String,
    /// BLAKE3 of the pack's canonical content, fixed when it was reviewed.
    /// [`content_hash`] computes it.
    pub content_hash: [u8; 32],
    pub categories: Vec<PackCategory>,
    pub rates: Vec<PackRate>,
}

/// One band, as `tax_category` stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackCategory {
    /// The band's code: `STD16`, `REDUCED_<rate_ppm>`, `ZERO`, `EXEMPT` in a
    /// real pack (`ref/tax-jordan.md` §7).
    pub code: String,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub treatment: TaxTreatment,
}

/// One rate of one band: a percentage of the line's net, effective from
/// `valid_from` (inclusive) to `valid_to` (exclusive), in store-local
/// business dates. It takes its treatment from its band.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackRate {
    pub category: String,
    pub component: String,
    pub rate: Percent,
    pub valid_from: BusinessDate,
    pub valid_to: Option<BusinessDate>,
}

/// The accountant's dated approval of one pack.
///
/// `0003` stores who and when on `tax_rule_pack`. The accountant is not a
/// user of the register, so `recorded_by` is the user who records their
/// sign-off; the date is the accountant's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountantApproval {
    pub recorded_by: UserId,
    pub approved_at: Timestamp,
}

/// An imported, approved pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxPackVersion {
    pub id: Id,
    pub profile: StoreTaxProfile,
    pub pack_version: String,
    pub content_hash: [u8; 32],
}

/// Why a pack was not imported. Nothing was written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TaxPackRefusal {
    #[error(
        "the pack's content hashes to {found}, not to the {pinned} it was pinned to when \
         reviewed, so it is not the pack that was approved"
    )]
    PinMismatch { pinned: String, found: String },
    #[error("an unregistered store charges no general sales tax and has no pack")]
    UnregisteredProfile,
    #[error("the pack carries no {0}, which an enabled pack must name")]
    EvidenceMissing(&'static str),
    #[error("the pack lists no bands, or no rates")]
    Empty,
    #[error("band {0} is listed twice")]
    DuplicateCategory(String),
    #[error("band {0} is listed with no code, name or component")]
    BlankField(String),
    #[error("a rate names band {0}, which the pack does not list")]
    UnknownCategory(String),
    #[error(
        "band {0} is listed with no rate: an unconfigured band fails closed rather than \
         being sold at a familiar rate"
    )]
    BandWithoutRate(String),
    #[error(
        "band {0} carries more than one component, which is Special Sales Tax and not \
         importable yet"
    )]
    SeveralComponents(String),
    #[error(
        "band {0}'s rate contradicts its treatment: zero-rated and exempt bands charge 0%, \
         standard and reduced bands more"
    )]
    RateContradictsTreatment(String),
    #[error("band {0} has a period that ends on or before the day it begins")]
    EmptyPeriod(String),
    #[error("band {0} has two rates in effect on one day")]
    OverlappingRates(String),
    #[error("the approval names user {0}, whom this register does not know")]
    ApproverUnknown(UserId),
    #[error("version {pack_version} of the {profile:?} pack is already imported")]
    VersionExists {
        profile: StoreTaxProfile,
        pack_version: String,
    },
    #[error(
        "band {0} already exists with other names or another treatment; a band never \
         changes what it is"
    )]
    CategoryConflict(String),
    #[error(
        "a zone pack is complete only against the approved standard pack, and there is \
         none to check it against"
    )]
    NoStandardPack,
    #[error(
        "the zone pack has no rate for band {0}, which the approved standard pack charges; \
         a zone never inherits the mainland's rate"
    )]
    ZonePackIncomplete(String),
}

/// BLAKE3 of the pack's canonical content: everything but the pin itself,
/// as sorted-key JSON with no whitespace, bands sorted by code and rates by
/// band, component and start date, so the order a pack lists them in does
/// not change its hash.
#[must_use]
pub fn content_hash(pack: &PinnedTaxPack) -> [u8; 32] {
    *blake3::hash(canonical_content(pack).as_bytes()).as_bytes()
}

fn canonical_content(pack: &PinnedTaxPack) -> String {
    let mut categories: Vec<&PackCategory> = pack.categories.iter().collect();
    categories.sort_by(|a, b| a.code.cmp(&b.code));
    let mut rates: Vec<&PackRate> = pack.rates.iter().collect();
    rates.sort_by(|a, b| {
        (&a.category, &a.component, a.valid_from).cmp(&(&b.category, &b.component, b.valid_from))
    });
    let categories: Vec<Value> = categories
        .into_iter()
        .map(|c| {
            object([
                ("code", Value::from(c.code.clone())),
                ("name_ar", Value::from(c.name_ar.clone())),
                (
                    "name_en",
                    c.name_en.clone().map_or(Value::Null, Value::from),
                ),
                ("treatment", Value::from(treatment_token(c.treatment))),
            ])
        })
        .collect();
    let rates: Vec<Value> = rates
        .into_iter()
        .map(|r| {
            object([
                ("category", Value::from(r.category.clone())),
                ("component", Value::from(r.component.clone())),
                ("rate_ppm", Value::from(r.rate.ppm())),
                ("valid_from", Value::from(r.valid_from.to_iso())),
                (
                    "valid_to",
                    r.valid_to.map_or(Value::Null, |d| Value::from(d.to_iso())),
                ),
            ])
        })
        .collect();
    object([
        ("categories", Value::Array(categories)),
        ("format", Value::from(FORMAT)),
        ("pack_version", Value::from(pack.pack_version.clone())),
        (
            "profile",
            Value::from(profile_token(pack.profile).unwrap_or("unregistered")),
        ),
        ("rates", Value::Array(rates)),
        ("source_ref", Value::from(pack.source_ref.clone())),
    ])
    .to_string()
}

/// A JSON object whose keys are in sorted order however `serde_json` keeps
/// its maps, because they are inserted from a `BTreeMap`.
fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    let sorted: BTreeMap<&str, Value> = BTreeMap::from(fields);
    Value::Object(
        sorted
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Import `pack`, approved by `approval`, inside the caller's transaction.
///
/// The pack is assembled `pending` and approved last, the one order `0003`'s
/// triggers allow: no rate may join an approved pack, and an approved one is
/// frozen. A band whose code is already a `tax_category` is reused when it is
/// the same band, so a product keeps its category across pack versions. New
/// ids come from `ids`, because this crate mints none of its own.
///
/// # Errors
///
/// [`DbError::TaxPackRefused`] with the [`TaxPackRefusal`] that stopped it,
/// found before anything is written, and [`DbError::Sqlite`] for what the
/// schema refuses after that, which the caller's transaction rolls back.
pub fn import_tax_pack(
    tx: &Transaction<'_>,
    pack: &PinnedTaxPack,
    approval: &AccountantApproval,
    ids: &impl IdSource,
) -> Result<TaxPackVersion, DbError> {
    let found = content_hash(pack);
    if found != pack.content_hash {
        return Err(TaxPackRefusal::PinMismatch {
            pinned: hex(&pack.content_hash),
            found: hex(&found),
        }
        .into());
    }
    let profile = profile_token(pack.profile).ok_or(TaxPackRefusal::UnregisteredProfile)?;
    let treatments = check_pack(pack)?;
    check_against_database(tx, pack, profile, approval, &treatments)?;

    let mut category_ids: BTreeMap<&str, Id> = BTreeMap::new();
    for category in &pack.categories {
        let id = match existing_category(tx, &category.code)? {
            Some((id, _)) => id,
            None => {
                let id = ids.next().into_bytes();
                tx.execute(
                    "INSERT INTO tax_category (id, code, name_ar, name_en, treatment)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        id.as_slice(),
                        category.code,
                        category.name_ar,
                        category.name_en,
                        treatment_token(category.treatment),
                    ],
                )?;
                id
            }
        };
        category_ids.insert(category.code.as_str(), id);
    }

    let pack_id = ids.next().into_bytes();
    tx.execute(
        "INSERT INTO tax_rule_pack
           (id, profile_scope, pack_version, source_ref, content_hash, status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
        params![
            pack_id.as_slice(),
            profile,
            pack.pack_version,
            pack.source_ref,
            pack.content_hash.as_slice(),
        ],
    )?;
    for rate in &pack.rates {
        let (Some(category), Some(treatment)) = (
            category_ids.get(rate.category.as_str()),
            treatments.get(rate.category.as_str()),
        ) else {
            return Err(TaxPackRefusal::UnknownCategory(rate.category.clone()).into());
        };
        tx.execute(
            "INSERT INTO tax_rate
               (id, rule_pack_id, tax_category_id, component_code, treatment,
                calculation_kind, rate_ppm, calculation_order, base_kind,
                valid_from, valid_to, profile_scope)
             VALUES (?1, ?2, ?3, ?4, ?5, 'ad_valorem', ?6, 0, 'line_net', ?7, ?8, ?9)",
            params![
                ids.next().into_bytes().as_slice(),
                pack_id.as_slice(),
                category.as_slice(),
                rate.component,
                treatment_token(*treatment),
                rate.rate.ppm(),
                rate.valid_from.to_iso(),
                rate.valid_to.map(BusinessDate::to_iso),
                profile,
            ],
        )?;
    }
    tx.execute(
        "UPDATE tax_rule_pack SET status = 'approved', approved_by = ?1, approved_at = ?2
          WHERE id = ?3",
        params![
            approval.recorded_by.as_uuid().as_bytes().as_slice(),
            approval.approved_at.to_iso8601(),
            pack_id.as_slice(),
        ],
    )?;
    Ok(TaxPackVersion {
        id: pack_id,
        profile: pack.profile,
        pack_version: pack.pack_version.clone(),
        content_hash: pack.content_hash,
    })
}

/// Everything a pack must be on its own terms. Returns each band's treatment.
fn check_pack(pack: &PinnedTaxPack) -> Result<BTreeMap<&str, TaxTreatment>, TaxPackRefusal> {
    if pack.source_ref.trim().is_empty() {
        return Err(TaxPackRefusal::EvidenceMissing("source reference"));
    }
    if pack.pack_version.trim().is_empty() {
        return Err(TaxPackRefusal::EvidenceMissing("version"));
    }
    if pack.categories.is_empty() || pack.rates.is_empty() {
        return Err(TaxPackRefusal::Empty);
    }
    let mut treatments: BTreeMap<&str, TaxTreatment> = BTreeMap::new();
    for category in &pack.categories {
        if category.code.trim().is_empty() || category.name_ar.trim().is_empty() {
            return Err(TaxPackRefusal::BlankField(category.code.clone()));
        }
        if treatments
            .insert(category.code.as_str(), category.treatment)
            .is_some()
        {
            return Err(TaxPackRefusal::DuplicateCategory(category.code.clone()));
        }
    }
    let mut by_band: BTreeMap<&str, Vec<&PackRate>> = BTreeMap::new();
    for rate in &pack.rates {
        let Some(&treatment) = treatments.get(rate.category.as_str()) else {
            return Err(TaxPackRefusal::UnknownCategory(rate.category.clone()));
        };
        if rate.component.trim().is_empty() {
            return Err(TaxPackRefusal::BlankField(rate.category.clone()));
        }
        let untaxed = matches!(treatment, TaxTreatment::Zero | TaxTreatment::Exempt);
        if (untaxed && rate.rate != Percent::ZERO) || (!untaxed && rate.rate <= Percent::ZERO) {
            return Err(TaxPackRefusal::RateContradictsTreatment(
                rate.category.clone(),
            ));
        }
        if rate.valid_to.is_some_and(|to| to <= rate.valid_from) {
            return Err(TaxPackRefusal::EmptyPeriod(rate.category.clone()));
        }
        by_band
            .entry(rate.category.as_str())
            .or_default()
            .push(rate);
    }
    for &band in treatments.keys() {
        let Some(rates) = by_band.get_mut(band) else {
            return Err(TaxPackRefusal::BandWithoutRate(band.to_owned()));
        };
        let first = rates.first().map(|r| r.component.as_str());
        if rates.iter().any(|r| Some(r.component.as_str()) != first) {
            return Err(TaxPackRefusal::SeveralComponents(band.to_owned()));
        }
        rates.sort_by_key(|r| r.valid_from);
        for pair in rates.windows(2) {
            if let [earlier, later] = pair
                && earlier.valid_to.is_none_or(|to| to > later.valid_from)
            {
                return Err(TaxPackRefusal::OverlappingRates(band.to_owned()));
            }
        }
    }
    Ok(treatments)
}

/// What the pack must agree with in this database. Reads only.
fn check_against_database(
    tx: &Transaction<'_>,
    pack: &PinnedTaxPack,
    profile: &str,
    approval: &AccountantApproval,
    treatments: &BTreeMap<&str, TaxTreatment>,
) -> Result<(), DbError> {
    let known: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM app_user WHERE id = ?1)",
        [approval.recorded_by.as_uuid().as_bytes().as_slice()],
        |row| row.get(0),
    )?;
    if !known {
        return Err(TaxPackRefusal::ApproverUnknown(approval.recorded_by).into());
    }
    let imported: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM tax_rule_pack
                         WHERE profile_scope = ?1 AND pack_version = ?2)",
        params![profile, pack.pack_version],
        |row| row.get(0),
    )?;
    if imported {
        return Err(TaxPackRefusal::VersionExists {
            profile: pack.profile,
            pack_version: pack.pack_version.clone(),
        }
        .into());
    }
    for category in &pack.categories {
        if let Some((_, stored)) = existing_category(tx, &category.code)?
            && stored != *category
        {
            return Err(TaxPackRefusal::CategoryConflict(category.code.clone()).into());
        }
    }
    if pack.profile != StoreTaxProfile::Standard {
        let mut statement = tx.prepare(
            "SELECT DISTINCT c.code
               FROM tax_rate r
               JOIN tax_rule_pack p ON p.id = r.rule_pack_id
               JOIN tax_category c ON c.id = r.tax_category_id
              WHERE p.profile_scope = 'standard' AND p.status = 'approved'
                AND r.deleted_at IS NULL
              ORDER BY c.code",
        )?;
        let standard: BTreeSet<String> = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        if standard.is_empty() {
            return Err(TaxPackRefusal::NoStandardPack.into());
        }
        if let Some(missing) = standard
            .iter()
            .find(|c| !treatments.contains_key(c.as_str()))
        {
            return Err(TaxPackRefusal::ZonePackIncomplete(missing.clone()).into());
        }
    }
    Ok(())
}

/// The `tax_category` row with this code, if any, as a band.
fn existing_category(conn: &Connection, code: &str) -> Result<Option<(Id, PackCategory)>, DbError> {
    let row: Option<(Vec<u8>, String, Option<String>, String)> = conn
        .query_row(
            "SELECT id, name_ar, name_en, treatment FROM tax_category
              WHERE code = ?1 AND deleted_at IS NULL",
            [code],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((id, name_ar, name_en, treatment)) = row else {
        return Ok(None);
    };
    let found = id.len();
    let id: Id = id.try_into().map_err(|_| DbError::IdWidthInvalid {
        table: "tax_category",
        column: "id",
        found,
    })?;
    let treatment =
        treatment_from_token(&treatment).ok_or_else(|| DbError::StoredTaxPackInvalid {
            reason: format!("tax_category {code} has treatment {treatment:?}"),
        })?;
    Ok(Some((
        id,
        PackCategory {
            code: code.to_owned(),
            name_ar,
            name_en,
            treatment,
        },
    )))
}

/// Reads what a store's approved pack charges.
pub struct TaxPackRepository<'c> {
    conn: &'c Connection,
}

impl<'c> TaxPackRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// The components `store`'s approved pack charges on `category` on the
    /// business date `on`, in application order: exactly the rows `0005`'s
    /// completion gate holds a finished line's tax to.
    ///
    /// They come back as the engine's components, ready for
    /// `pos_domain::compute_line_tax`. The pack is already the store's
    /// profile's and its periods were checked when it was imported, so there
    /// is nothing left for `resolve_components` to choose between.
    ///
    /// # Errors
    ///
    /// - [`DbError::TaxPackStoreUnknown`]: this register holds no such store.
    /// - [`DbError::TaxPackMissing`] and [`DbError::TaxPackUnapproved`]: the
    ///   store has no pack this register holds, or its pack is not approved.
    /// - [`DbError::TaxBandUnconfigured`]: the pack charges nothing on that
    ///   category that day. An unconfigured band fails closed; it is never
    ///   sold at a familiar rate.
    /// - [`DbError::StoredTaxPackInvalid`] and [`DbError::IdWidthInvalid`]: a
    ///   row this build cannot read, including any Special Sales Tax shape.
    pub fn components_for(
        &self,
        store: StoreId,
        category: TaxCategoryId,
        on: BusinessDate,
    ) -> Result<Vec<TaxComponent>, DbError> {
        let pinned: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT tax_rule_pack_id FROM store WHERE id = ?1",
                [store.as_uuid().as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(DbError::TaxPackStoreUnknown(store))?;
        let pack = pinned.ok_or(DbError::TaxPackMissing(store))?;
        let status: Option<String> = self
            .conn
            .query_row(
                "SELECT status FROM tax_rule_pack WHERE id = ?1",
                [pack.as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        match status.as_deref() {
            None => return Err(DbError::TaxPackMissing(store)),
            Some("approved") => {}
            Some(_) => return Err(DbError::TaxPackUnapproved(store)),
        }

        let day = on.to_iso();
        let mut statement = self.conn.prepare(
            "SELECT component_code, treatment, calculation_kind, rate_ppm,
                    calculation_order, base_kind
               FROM tax_rate
              WHERE rule_pack_id = ?1 AND tax_category_id = ?2
                AND valid_from <= ?3 AND (valid_to IS NULL OR valid_to > ?3)
                AND deleted_at IS NULL
              ORDER BY calculation_order, component_code",
        )?;
        let rows = statement
            .query_map(
                params![
                    pack.as_slice(),
                    category.as_uuid().as_bytes().as_slice(),
                    day
                ],
                |row| {
                    Ok(StoredRate {
                        component: row.get(0)?,
                        treatment: row.get(1)?,
                        kind: row.get(2)?,
                        rate_ppm: row.get(3)?,
                        order: row.get(4)?,
                        base: row.get(5)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.is_empty() {
            return Err(DbError::TaxBandUnconfigured {
                store,
                category,
                on: day,
            });
        }
        rows.into_iter().map(StoredRate::into_component).collect()
    }
}

/// A `tax_rate` row as stored, before anything is believed.
struct StoredRate {
    component: String,
    treatment: String,
    kind: String,
    rate_ppm: Option<i64>,
    order: i64,
    base: String,
}

impl StoredRate {
    fn into_component(self) -> Result<TaxComponent, DbError> {
        let invalid = |reason: String| DbError::StoredTaxPackInvalid { reason };
        let treatment = treatment_from_token(&self.treatment).ok_or_else(|| {
            invalid(format!(
                "component {} has treatment {:?}",
                self.component, self.treatment
            ))
        })?;
        // Only what a `pos-tax-pack/1` pack can say is read. A fixed amount
        // or a tax on a tax is Special Sales Tax, whose stored form 1.3.5
        // recorded as not yet reconciled with the engine's.
        let (Some(ppm), "ad_valorem", "line_net") =
            (self.rate_ppm, self.kind.as_str(), self.base.as_str())
        else {
            return Err(invalid(format!(
                "component {} is {} on {}, which this build does not read",
                self.component, self.kind, self.base
            )));
        };
        let sequence = u8::try_from(self.order).map_err(|_| {
            invalid(format!(
                "component {} has order {}",
                self.component, self.order
            ))
        })?;
        Ok(TaxComponent {
            code: self.component,
            treatment,
            basis: TaxBasis::Percentage {
                rate: Percent::from_ppm(ppm),
            },
            sequence,
            base: TaxBase::LineNet,
            is_inclusive_capable: true,
        })
    }
}

// ── The schema's spelling, mapped explicitly (`ref/domain-api.md` §1.2) ──

fn treatment_token(treatment: TaxTreatment) -> &'static str {
    match treatment {
        TaxTreatment::Standard => "standard",
        TaxTreatment::Reduced => "reduced",
        TaxTreatment::Zero => "zero",
        TaxTreatment::Exempt => "exempt",
    }
}

fn treatment_from_token(token: &str) -> Option<TaxTreatment> {
    match token {
        "standard" => Some(TaxTreatment::Standard),
        "reduced" => Some(TaxTreatment::Reduced),
        "zero" => Some(TaxTreatment::Zero),
        "exempt" => Some(TaxTreatment::Exempt),
        _ => None,
    }
}

/// `tax_rule_pack.profile_scope`'s spelling. An unregistered store has none.
fn profile_token(profile: StoreTaxProfile) -> Option<&'static str> {
    match profile {
        StoreTaxProfile::Standard => Some("standard"),
        StoreTaxProfile::Asez => Some("asez"),
        StoreTaxProfile::DevelopmentArea => Some("development_area"),
        StoreTaxProfile::Unregistered => None,
    }
}
