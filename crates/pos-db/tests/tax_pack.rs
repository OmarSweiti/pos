//! A jurisdiction's tax pack (microstep 1.3.7): pinned, imported with the
//! accountant's dated approval, and read back as what a store charges.
//!
//! **Every pack here is synthetic.** Which bands the merchant actually enables
//! is `ref/schema.md` §0003's `⚠️ OPEN` item, so the fixture stands in for the
//! real pack: its codes are `SYNTH_*`, its source says it is not an official
//! catalogue, its dates are in 2001, and its rates are arithmetic vectors, not
//! a statement of any Jordanian band. When the accountant's classification
//! arrives, the real pinned pack replaces it, and [`ENABLED_BANDS`] becomes
//! the merchant's list.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::DbError;
use pos_db::tax_pack::{
    AccountantApproval, PackCategory, PackRate, PinnedTaxPack, TaxPackRefusal, TaxPackRepository,
    content_hash, import_tax_pack,
};
use pos_domain::{
    BusinessDate, Percent, SeqIdSource, StoreId, StoreTaxProfile, TaxBase, TaxBasis, TaxCategoryId,
    TaxComponent, TaxTreatment, Timestamp, UserId,
};
use rusqlite::{Connection, params};
use uuid::Uuid;

const KEY: &str = "tax-pack-test-key";
const SOURCE: &str =
    "SYNTHETIC-CATALOGUE 2001.1, a test fixture and not an official ISTD catalogue";
const STANDARD: &str = "SYNTH_STANDARD";
const REDUCED: &str = "SYNTH_REDUCED_40000";
const ZERO: &str = "SYNTH_ZERO";
const EXEMPT: &str = "SYNTH_EXEMPT";

/// The bands the fixture stands in for: one of each treatment. The merchant's
/// own list replaces this one when their classification arrives.
const ENABLED_BANDS: [(&str, TaxTreatment); 4] = [
    (STANDARD, TaxTreatment::Standard),
    (REDUCED, TaxTreatment::Reduced),
    (ZERO, TaxTreatment::Zero),
    (EXEMPT, TaxTreatment::Exempt),
];

const ORG: u128 = 0x10;
const RECORDER: u128 = 0x11;
const POLICY: u128 = 0x12;

struct World {
    _dir: tempfile::TempDir,
    conn: Connection,
    ids: SeqIdSource,
}

/// An org, the user who records approvals, and an approved computation
/// policy: everything a store needs except its tax pack.
fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
    conn.execute(
        "INSERT INTO org (id, legal_name) VALUES (?1, 'Synthetic Org')",
        [id(ORG).as_slice()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
         VALUES (?1, ?2, 'OWN-1', 'Synthetic Owner', 'placeholder-not-a-hash',
                 '2001-01-01T00:00:00.000Z')",
        params![id(RECORDER).as_slice(), id(ORG).as_slice()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tax_computation_policy
           (id, jurisdiction, policy_version, rounding_rule, cash_round_step_minor,
            cash_round_direction, cash_round_tax_treatment, source_ref, content_hash,
            approved_at)
         VALUES (?1, 'JO', 'synthetic-v1', 'half_away_from_zero', 5, 'nearest', 'none',
                 'synthetic', ?2, '2001-01-01T00:00:00.000Z')",
        params![id(POLICY).as_slice(), vec![0x09u8; 32]],
    )
    .unwrap();
    World {
        _dir: dir,
        conn,
        // 1 January 2001, the fixture's own year.
        ids: SeqIdSource::new(978_307_200_000, 1),
    }
}

fn id(value: u128) -> [u8; 16] {
    Uuid::from_u128(value).into_bytes()
}

fn date(iso: &str) -> BusinessDate {
    BusinessDate::parse(iso).unwrap()
}

fn approval() -> AccountantApproval {
    AccountantApproval {
        recorded_by: UserId::from_uuid(Uuid::from_u128(RECORDER)),
        approved_at: Timestamp::parse_iso8601("2001-01-15T09:00:00.000Z").unwrap(),
    }
}

fn band(code: &str, treatment: TaxTreatment) -> PackCategory {
    PackCategory {
        code: code.to_owned(),
        name_ar: format!("فئة اصطناعية {code}"),
        name_en: Some(format!("Synthetic {code}")),
        treatment,
    }
}

fn rate(category: &str, ppm: i64, from: &str, to: Option<&str>) -> PackRate {
    PackRate {
        category: category.to_owned(),
        component: "GST".to_owned(),
        rate: Percent::from_ppm(ppm),
        valid_from: date(from),
        valid_to: to.map(date),
    }
}

/// `pack` with its pin set to what it hashes to, as a reviewer would fix it.
fn pinned(mut pack: PinnedTaxPack) -> PinnedTaxPack {
    pack.content_hash = content_hash(&pack);
    pack
}

/// The synthetic standard pack: one band of each treatment, and a reduced
/// band whose rate changes on 1 July 2001.
fn standard_pack(version: &str) -> PinnedTaxPack {
    pinned(PinnedTaxPack {
        profile: StoreTaxProfile::Standard,
        pack_version: version.to_owned(),
        source_ref: SOURCE.to_owned(),
        content_hash: [0; 32],
        categories: ENABLED_BANDS
            .iter()
            .map(|&(code, t)| band(code, t))
            .collect(),
        rates: vec![
            rate(STANDARD, 160_000, "2001-01-01", None),
            rate(REDUCED, 40_000, "2001-01-01", Some("2001-07-01")),
            rate(REDUCED, 50_000, "2001-07-01", None),
            rate(ZERO, 0, "2001-01-01", None),
            rate(EXEMPT, 0, "2001-01-01", None),
        ],
    })
}

/// The same bands for the ASEZ profile, at rates of their own.
fn asez_pack(version: &str) -> PinnedTaxPack {
    pinned(PinnedTaxPack {
        profile: StoreTaxProfile::Asez,
        pack_version: version.to_owned(),
        source_ref: SOURCE.to_owned(),
        content_hash: [0; 32],
        categories: ENABLED_BANDS
            .iter()
            .map(|&(code, t)| band(code, t))
            .collect(),
        rates: vec![
            rate(STANDARD, 70_000, "2001-01-01", None),
            rate(REDUCED, 20_000, "2001-01-01", None),
            rate(ZERO, 0, "2001-01-01", None),
            rate(EXEMPT, 0, "2001-01-01", None),
        ],
    })
}

fn import(w: &mut World, pack: &PinnedTaxPack) -> Result<[u8; 16], DbError> {
    import_with(w, pack, &approval())
}

/// Import in a transaction of its own, committed only on success, as a
/// caller would.
fn import_with(
    w: &mut World,
    pack: &PinnedTaxPack,
    approval: &AccountantApproval,
) -> Result<[u8; 16], DbError> {
    let tx = w.conn.transaction().unwrap();
    let version = import_tax_pack(&tx, pack, approval, &w.ids)?;
    tx.commit().unwrap();
    Ok(version.id)
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}

/// A store on `pack`, with the profile it is for.
fn store(w: &World, n: u128, profile: &str, pack: &[u8; 16]) -> rusqlite::Result<StoreId> {
    w.conn.execute(
        "INSERT INTO store
           (id, org_id, code, name_ar, tax_profile, fiscal_obligation,
            fiscal_obligation_evidence_ref, fiscal_profile,
            tax_rule_pack_id, tax_computation_policy_id)
         VALUES (?1, ?2, ?3, 'متجر اصطناعي', ?4, 'exempt', 'synthetic-evidence', 'disabled',
                 ?5, ?6)",
        params![
            id(n).as_slice(),
            id(ORG).as_slice(),
            format!("S-{n}"),
            profile,
            pack.as_slice(),
            id(POLICY).as_slice()
        ],
    )?;
    Ok(StoreId::from_uuid(Uuid::from_u128(n)))
}

fn category(conn: &Connection, code: &str) -> TaxCategoryId {
    let bytes: Vec<u8> = conn
        .query_row(
            "SELECT id FROM tax_category WHERE code = ?1",
            [code],
            |row| row.get(0),
        )
        .unwrap();
    TaxCategoryId::from_uuid(Uuid::from_slice(&bytes).unwrap())
}

/// What `store` charges on band `code` on `day`.
fn charges(w: &World, store: StoreId, code: &str, day: &str) -> Result<Vec<TaxComponent>, DbError> {
    TaxPackRepository::new(&w.conn).components_for(store, category(&w.conn, code), date(day))
}

fn percentage(code: &str, treatment: TaxTreatment, ppm: i64) -> TaxComponent {
    TaxComponent {
        code: code.to_owned(),
        treatment,
        basis: TaxBasis::Percentage {
            rate: Percent::from_ppm(ppm),
        },
        sequence: 0,
        base: TaxBase::LineNet,
        is_inclusive_capable: true,
    }
}

const TABLES: [&str; 3] = ["tax_category", "tax_rule_pack", "tax_rate"];

/// The refusal, checked to have written nothing before it was found: the rows
/// are counted inside the import's own transaction, before it rolls back, so
/// a refusal reached after a write would show here even though the rollback
/// would hide it from every later reader.
fn refused(w: &mut World, pack: &PinnedTaxPack, approval: &AccountantApproval) -> TaxPackRefusal {
    let tx = w.conn.transaction().unwrap();
    let before = TABLES.map(|table| count(&tx, table));
    let result = import_tax_pack(&tx, pack, approval, &w.ids);
    let after = TABLES.map(|table| count(&tx, table));
    assert_eq!(before, after, "{result:?} wrote rows before it was refused");
    match result {
        Err(DbError::TaxPackRefused(refusal)) => refusal,
        other => panic!("expected a refused pack, got {other:?}"),
    }
}

#[test]
fn imported_rule_records_source_version_hash_and_approval() {
    let mut w = world();
    let pack = standard_pack("synthetic-2001.1");
    let tx = w.conn.transaction().unwrap();
    let version = import_tax_pack(&tx, &pack, &approval(), &w.ids).unwrap();
    tx.commit().unwrap();
    assert_eq!(version.pack_version, "synthetic-2001.1");
    assert_eq!(version.content_hash, content_hash(&pack));

    let mut statement = w
        .conn
        .prepare(
            "SELECT c.code, r.treatment, r.rate_ppm, r.valid_from, r.valid_to, r.profile_scope,
                    p.profile_scope, p.pack_version, p.source_ref, p.content_hash,
                    p.approved_by, p.approved_at, p.status
               FROM tax_rate r
               JOIN tax_rule_pack p ON p.id = r.rule_pack_id
               JOIN tax_category c ON c.id = r.tax_category_id
              ORDER BY c.code, r.valid_from",
        )
        .unwrap();
    let rows: Vec<(String, String, i64, String, Option<String>, String)> = statement
        .query_map([], |row| {
            // Every rule carries its pack's evidence, identically.
            assert_eq!(row.get::<_, String>(6)?, "standard");
            assert_eq!(row.get::<_, String>(7)?, "synthetic-2001.1");
            assert_eq!(row.get::<_, String>(8)?, SOURCE);
            assert_eq!(row.get::<_, Vec<u8>>(9)?, content_hash(&pack).to_vec());
            assert_eq!(row.get::<_, Vec<u8>>(10)?, id(RECORDER).to_vec());
            assert_eq!(row.get::<_, String>(11)?, "2001-01-15T09:00:00.000Z");
            assert_eq!(row.get::<_, String>(12)?, "approved");
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    // Each rule as the pack states it: its real dates, never a go-live date.
    let at = |c: &str, t: &str, ppm: i64, from: &str, to: Option<&str>| {
        (
            c.to_owned(),
            t.to_owned(),
            ppm,
            from.to_owned(),
            to.map(str::to_owned),
            "standard".to_owned(),
        )
    };
    assert_eq!(
        rows,
        vec![
            at(EXEMPT, "exempt", 0, "2001-01-01", None),
            at(REDUCED, "reduced", 40_000, "2001-01-01", Some("2001-07-01")),
            at(REDUCED, "reduced", 50_000, "2001-07-01", None),
            at(STANDARD, "standard", 160_000, "2001-01-01", None),
            at(ZERO, "zero", 0, "2001-01-01", None),
        ]
    );
}

#[test]
fn standard_rate_records_the_same_pack_evidence_as_reduced_rates() {
    let mut w = world();
    let pack = import(&mut w, &standard_pack("synthetic-2001.1")).unwrap();
    // The standard band is not a seed of its own: its rule belongs to the
    // same pinned, approved pack as the reduced band's, so it carries the
    // same source, version, hash and approval.
    let packs_of = |code: &str| -> Vec<Vec<u8>> {
        let mut statement = w
            .conn
            .prepare(
                "SELECT DISTINCT r.rule_pack_id FROM tax_rate r
                   JOIN tax_category c ON c.id = r.tax_category_id
                  WHERE c.code = ?1",
            )
            .unwrap();
        statement
            .query_map([code], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(packs_of(STANDARD), vec![pack.to_vec()]);
    assert_eq!(packs_of(REDUCED), vec![pack.to_vec()]);
    // And nothing else charges anything: no rate outside the one pack.
    let outside: i64 = w
        .conn
        .query_row(
            "SELECT COUNT(*) FROM tax_rate WHERE rule_pack_id <> ?1",
            [pack.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(outside, 0);
    assert_eq!(count(&w.conn, "tax_rule_pack"), 1);
}

#[test]
fn unconfigured_reduced_band_fails_closed() {
    let mut w = world();

    // A band listed with no rate is not imported at all.
    let mut unconfigured = standard_pack("synthetic-2001.1");
    unconfigured
        .categories
        .push(band("SYNTH_REDUCED_80000", TaxTreatment::Reduced));
    assert_eq!(
        refused(&mut w, &pinned(unconfigured), &approval()),
        TaxPackRefusal::BandWithoutRate("SYNTH_REDUCED_80000".to_owned())
    );
    // Nor is a rate for a band the pack does not list.
    let mut stray = standard_pack("synthetic-2001.1");
    stray
        .rates
        .push(rate("SYNTH_REDUCED_80000", 80_000, "2001-01-01", None));
    assert_eq!(
        refused(&mut w, &pinned(stray), &approval()),
        TaxPackRefusal::UnknownCategory("SYNTH_REDUCED_80000".to_owned())
    );
    assert_eq!(count(&w.conn, "tax_rule_pack"), 0);
    assert_eq!(count(&w.conn, "tax_category"), 0);

    // A store on the pack charges each band its rate on the day, from the
    // pack's own dates: nothing before the band's first day, and the reduced
    // rate's change on 1 July.
    let first = import(&mut w, &standard_pack("synthetic-2001.1")).unwrap();
    let on_first = store(&w, 0x100, "standard", &first).unwrap();
    let reduced = |ppm| vec![percentage("GST", TaxTreatment::Reduced, ppm)];
    assert_eq!(
        charges(&w, on_first, REDUCED, "2001-06-30").unwrap(),
        reduced(40_000)
    );
    assert_eq!(
        charges(&w, on_first, REDUCED, "2001-07-01").unwrap(),
        reduced(50_000)
    );
    assert!(matches!(
        charges(&w, on_first, REDUCED, "2000-12-31"),
        Err(DbError::TaxBandUnconfigured { .. })
    ));

    // A later version drops the reduced band. A store on it still has the
    // category, from the first pack, but no rate for it: the band fails
    // closed by name, and is never sold at the standard rate instead.
    let mut without = standard_pack("synthetic-2001.2");
    without.categories.retain(|c| c.code != REDUCED);
    without.rates.retain(|r| r.category != REDUCED);
    let second = import(&mut w, &pinned(without)).unwrap();
    let on_second = store(&w, 0x101, "standard", &second).unwrap();
    assert!(matches!(
        charges(&w, on_second, REDUCED, "2001-08-01"),
        Err(DbError::TaxBandUnconfigured { .. })
    ));
    assert_eq!(
        charges(&w, on_second, STANDARD, "2001-08-01").unwrap(),
        vec![percentage("GST", TaxTreatment::Standard, 160_000)]
    );
}

#[test]
fn asez_profile_without_complete_pack_fails_closed() {
    let mut w = world();

    // A zone pack is complete only against the approved standard pack, so
    // with none to check against it is refused.
    assert_eq!(
        refused(&mut w, &asez_pack("synthetic-asez-2001.1"), &approval()),
        TaxPackRefusal::NoStandardPack
    );
    let standard = import(&mut w, &standard_pack("synthetic-2001.1")).unwrap();

    // A zone pack missing a band the standard pack charges is refused: the
    // zone would otherwise sell that band at whatever the mainland charges.
    let mut partial = asez_pack("synthetic-asez-2001.1");
    partial.categories.retain(|c| c.code != EXEMPT);
    partial.rates.retain(|r| r.category != EXEMPT);
    assert_eq!(
        refused(&mut w, &pinned(partial), &approval()),
        TaxPackRefusal::ZonePackIncomplete(EXEMPT.to_owned())
    );
    assert_eq!(count(&w.conn, "tax_rule_pack"), 1);

    // And an ASEZ store cannot fall through to the standard pack: `0003`
    // refuses to provision it on a pack for another profile.
    let refused = store(&w, 0x200, "asez", &standard).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("every store tax profile requires its approved rule pack"),
        "{refused}"
    );

    // With its own complete pack it charges the zone's rates, not the
    // mainland's.
    let zone = import(&mut w, &asez_pack("synthetic-asez-2001.1")).unwrap();
    let asez = store(&w, 0x201, "asez", &zone).unwrap();
    assert_eq!(
        charges(&w, asez, STANDARD, "2001-03-01").unwrap(),
        vec![percentage("GST", TaxTreatment::Standard, 70_000)]
    );
}

/// Whether a pack, imported with `approval` into a fresh register, gives a
/// store exactly one component of the right treatment for every enabled band.
fn covers(pack: &PinnedTaxPack, approval: &AccountantApproval) -> Result<(), String> {
    let mut w = world();
    let imported = import_with(&mut w, pack, approval).map_err(|e| e.to_string())?;
    let shop = store(&w, 0x300, "standard", &imported).map_err(|e| e.to_string())?;
    for (code, treatment) in ENABLED_BANDS {
        let exists: bool = w
            .conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM tax_category WHERE code = ?1)",
                [code],
                |row| row.get(0),
            )
            .unwrap();
        if !exists {
            return Err(format!("band {code} is not in the pack"));
        }
        let components = charges(&w, shop, code, "2001-03-01").map_err(|e| e.to_string())?;
        match components.as_slice() {
            [one] if one.treatment == treatment => {}
            other => return Err(format!("band {code} charges {other:?}")),
        }
    }
    Ok(())
}

#[test]
fn fixture_covers_every_enabled_band() {
    let fixture = standard_pack("synthetic-2001.1");
    assert_eq!(covers(&fixture, &approval()), Ok(()));

    // It fails without any one band. A band dropped from the pack is not
    // charged at all.
    for (code, _) in ENABLED_BANDS {
        let mut short = fixture.clone();
        short.categories.retain(|c| c.code != code);
        short.rates.retain(|r| r.category != code);
        let result = covers(&pinned(short), &approval());
        assert_eq!(
            result,
            Err(format!("band {code} is not in the pack")),
            "the fixture without {code} must not cover the enabled bands"
        );
    }
    // And without its approval record: an approval that names no user the
    // register knows approves nothing, so the pack is never imported.
    let unrecorded = AccountantApproval {
        recorded_by: UserId::from_uuid(Uuid::from_u128(0xDEAD)),
        ..approval()
    };
    let result = covers(&fixture, &unrecorded);
    assert!(
        result
            .as_ref()
            .is_err_and(|e| e.contains("whom this register does not know")),
        "{result:?}"
    );
}

#[test]
fn a_pack_that_is_not_what_was_pinned_is_refused() {
    let mut w = world();
    // One rate changed after the pack was pinned.
    let mut altered = standard_pack("synthetic-2001.1");
    for r in altered.rates.iter_mut().filter(|r| r.category == STANDARD) {
        r.rate = Percent::from_ppm(170_000);
    }
    assert!(matches!(
        refused(&mut w, &altered, &approval()),
        TaxPackRefusal::PinMismatch { .. }
    ));
    assert_eq!(count(&w.conn, "tax_rule_pack"), 0);
    assert_eq!(count(&w.conn, "tax_rate"), 0);
}

#[test]
fn a_pack_hashes_the_same_whatever_order_it_lists_its_bands_in() {
    let pack = standard_pack("synthetic-2001.1");
    let mut reordered = pack.clone();
    reordered.categories.reverse();
    reordered.rates.reverse();
    assert_eq!(content_hash(&reordered), content_hash(&pack));
    // Everything else it says is pinned: the version, the source, a name, a
    // treatment, a rate and a date each move the hash.
    let changed: [fn(&mut PinnedTaxPack); 6] = [
        |p| p.pack_version.push('x'),
        |p| p.source_ref.push('x'),
        |p| p.categories.iter_mut().for_each(|c| c.name_ar.push('x')),
        |p| p.profile = StoreTaxProfile::Asez,
        |p| {
            p.rates
                .iter_mut()
                .for_each(|r| r.rate = Percent::from_ppm(r.rate.ppm() + 1))
        },
        |p| {
            p.rates
                .iter_mut()
                .for_each(|r| r.valid_from = r.valid_from.succ())
        },
    ];
    for change in changed {
        let mut other = pack.clone();
        change(&mut other);
        assert_ne!(content_hash(&other), content_hash(&pack));
    }
}

/// A pack that has been changed after it was built, then pinned again.
type Spoil = fn(&mut PinnedTaxPack);

#[test]
fn a_malformed_pack_is_refused_and_writes_nothing() {
    let cases: [(Spoil, TaxPackRefusal); 11] = [
        (
            |p| p.profile = StoreTaxProfile::Unregistered,
            TaxPackRefusal::UnregisteredProfile,
        ),
        (
            |p| p.source_ref.clear(),
            TaxPackRefusal::EvidenceMissing("source reference"),
        ),
        (|p| p.rates.clear(), TaxPackRefusal::Empty),
        (
            |p| p.categories.push(band(STANDARD, TaxTreatment::Standard)),
            TaxPackRefusal::DuplicateCategory(STANDARD.to_owned()),
        ),
        (
            |p| p.categories.iter_mut().for_each(|c| c.name_ar.clear()),
            TaxPackRefusal::BlankField(STANDARD.to_owned()),
        ),
        (
            |p| p.rates.push(rate(ZERO, 10_000, "2002-01-01", None)),
            TaxPackRefusal::RateContradictsTreatment(ZERO.to_owned()),
        ),
        (
            |p| p.rates.push(rate(STANDARD, 0, "2002-01-01", None)),
            TaxPackRefusal::RateContradictsTreatment(STANDARD.to_owned()),
        ),
        (
            |p| {
                p.rates
                    .push(rate(STANDARD, 160_000, "2002-01-01", Some("2002-01-01")))
            },
            TaxPackRefusal::EmptyPeriod(STANDARD.to_owned()),
        ),
        (
            |p| p.rates.push(rate(STANDARD, 170_000, "2001-06-01", None)),
            TaxPackRefusal::OverlappingRates(STANDARD.to_owned()),
        ),
        (
            |p| {
                let mut second = rate(STANDARD, 10_000, "2001-01-01", None);
                second.component = "SST".to_owned();
                p.rates.push(second);
            },
            TaxPackRefusal::SeveralComponents(STANDARD.to_owned()),
        ),
        (
            |p| p.rates.iter_mut().for_each(|r| r.component.clear()),
            TaxPackRefusal::BlankField(STANDARD.to_owned()),
        ),
    ];
    for (spoil, expected) in cases {
        let mut w = world();
        let mut pack = standard_pack("synthetic-2001.1");
        spoil(&mut pack);
        assert_eq!(refused(&mut w, &pinned(pack), &approval()), expected);
        for table in ["tax_rule_pack", "tax_rate", "tax_category"] {
            assert_eq!(count(&w.conn, table), 0, "{expected}: {table} was written");
        }
    }
}

#[test]
fn a_pack_that_disagrees_with_the_register_is_refused() {
    let mut w = world();
    import(&mut w, &standard_pack("synthetic-2001.1")).unwrap();

    // The same version twice.
    assert_eq!(
        refused(&mut w, &standard_pack("synthetic-2001.1"), &approval()),
        TaxPackRefusal::VersionExists {
            profile: StoreTaxProfile::Standard,
            pack_version: "synthetic-2001.1".to_owned(),
        }
    );
    // A band that already exists, now claimed to be another treatment: a
    // product in it would change its tax without anyone reclassifying it.
    let mut reclassified = standard_pack("synthetic-2001.2");
    for c in reclassified
        .categories
        .iter_mut()
        .filter(|c| c.code == ZERO)
    {
        c.treatment = TaxTreatment::Exempt;
    }
    assert_eq!(
        refused(&mut w, &pinned(reclassified), &approval()),
        TaxPackRefusal::CategoryConflict(ZERO.to_owned())
    );
    // An approval recorded by nobody this register knows.
    let unrecorded = AccountantApproval {
        recorded_by: UserId::from_uuid(Uuid::from_u128(0xDEAD)),
        ..approval()
    };
    assert_eq!(
        refused(&mut w, &standard_pack("synthetic-2001.2"), &unrecorded),
        TaxPackRefusal::ApproverUnknown(unrecorded.recorded_by)
    );
    assert_eq!(count(&w.conn, "tax_rule_pack"), 1);
    assert_eq!(count(&w.conn, "tax_category"), 4);
}

#[test]
fn a_store_rate_this_build_cannot_read_is_refused_not_guessed() {
    let mut w = world();
    let unknown = TaxPackRepository::new(&w.conn).components_for(
        StoreId::from_uuid(Uuid::from_u128(0x404)),
        TaxCategoryId::from_uuid(Uuid::from_u128(0x405)),
        date("2001-03-01"),
    );
    assert!(matches!(unknown, Err(DbError::TaxPackStoreUnknown(_))));

    // A fixed amount per quantity, written by hand into a pending pack: the
    // Special Sales Tax shape, which no `pos-tax-pack/1` pack can carry and
    // this build therefore does not read.
    import(&mut w, &standard_pack("synthetic-2001.1")).unwrap();
    let fixed_pack = id(0x500);
    w.conn
        .execute(
            "INSERT INTO tax_rule_pack
               (id, profile_scope, pack_version, source_ref, content_hash, status)
             VALUES (?1, 'standard', 'hand-written', 'synthetic', ?2, 'pending')",
            params![fixed_pack.as_slice(), vec![0x07u8; 32]],
        )
        .unwrap();
    let standard = category(&w.conn, STANDARD);
    w.conn
        .execute(
            "INSERT INTO tax_rate
               (id, rule_pack_id, tax_category_id, component_code, treatment,
                calculation_kind, fixed_amount_minor, fixed_currency, fixed_basis_qty_milli,
                calculation_order, base_kind, valid_from, profile_scope)
             VALUES (?1, ?2, ?3, 'SST', 'standard', 'fixed_per_quantity', 100, 'JOD', 1000,
                     0, 'quantity', '2001-01-01', 'standard')",
            params![
                id(0x501).as_slice(),
                fixed_pack.as_slice(),
                standard.as_uuid().as_bytes().as_slice()
            ],
        )
        .unwrap();
    w.conn
        .execute(
            "UPDATE tax_rule_pack SET status = 'approved', approved_by = ?1,
                    approved_at = '2001-01-15T09:00:00.000Z'
              WHERE id = ?2",
            params![id(RECORDER).as_slice(), fixed_pack.as_slice()],
        )
        .unwrap();
    let shop = store(&w, 0x502, "standard", &fixed_pack).unwrap();
    assert!(matches!(
        charges(&w, shop, STANDARD, "2001-03-01"),
        Err(DbError::StoredTaxPackInvalid { .. })
    ));
}
