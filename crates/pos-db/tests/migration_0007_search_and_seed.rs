//! Migration `0007`: catalogue search, trade-scale commissioning, PLU codes and
//! tiles (microstep 1.2.5).
//!
//! Search is the fallback for every unbarcoded item and the only path a cashier
//! has when the scanner fails, and `0007` is forward-only: a mistake here costs
//! a compensating migration and a full reindex on every register. So the
//! fixture does not start from an empty catalogue. It builds a database at v6,
//! puts products and barcodes in it, and only then opens it, so `0007` runs over
//! a catalogue that already exists — the backfill — and every test then adds to
//! it through the triggers.
//!
//! Queries are built the way 1.2.3's `search()` will build them: the typed text
//! put through `pos_db::search::{exact, fold}`, then the precision branch and
//! the recall branch together, so the spelling the cashier typed ranks first.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::fixtures::registered_chain::{AT, RegisteredChain};
use pos_db::search;
use pos_test_support::io_proptest_config;
use proptest::prelude::*;
use rusqlite::{Connection, params};
use uuid::Uuid;

const KEY: &str = "migration-0007-test-key";

fn id(value: u128) -> Vec<u8> {
    Uuid::from_u128(value).as_bytes().to_vec()
}

fn message(error: rusqlite::Error) -> String {
    match error {
        rusqlite::Error::SqliteFailure(_, Some(message)) => message,
        other => panic!("expected a refusal with a message, found {other:?}"),
    }
}

struct Db {
    _dir: tempfile::TempDir,
    conn: Connection,
    chain: RegisteredChain,
}

fn insert_product(
    conn: &Connection,
    chain: &RegisteredChain,
    product: u128,
    sku: &str,
    name_ar: &str,
    name_en: Option<&str>,
) {
    conn.execute(
        "INSERT INTO product (id, sku, name, name_ar, name_en, price_minor, currency, is_active,
                              tax_category_id)
         VALUES (?1, ?2, ?3, ?3, ?4, 1000, 'JOD', 1, ?5)",
        params![&id(product), sku, name_ar, name_en, &chain.tax_category],
    )
    .unwrap();
}

fn insert_barcode(conn: &Connection, barcode: u128, product: u128, code: &str) {
    conn.execute(
        "INSERT INTO barcode (id, product_id, code, kind) VALUES (?1, ?2, ?3, 'ean13')",
        params![&id(barcode), &id(product), code],
    )
    .unwrap();
}

/// A register database built at v6 with a small catalogue in it, then opened —
/// which applies 0007 over a catalogue that already exists.
fn migrated() -> Db {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("register.db");
    let chain = {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "key", KEY).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        for (index, sql) in [
            include_str!("../migrations/0001_init.sql"),
            include_str!("../migrations/0002_sale_integrity.sql"),
            include_str!("../migrations/0003_strict_rebuild_and_catalog_depth.sql"),
            include_str!("../migrations/0004_people_and_audit.sql"),
            include_str!("../migrations/0005_sale_columns_and_sequences.sql"),
            include_str!("../migrations/0006_stock_ledger.sql"),
        ]
        .iter()
        .enumerate()
        {
            let tx = conn.unchecked_transaction().unwrap();
            tx.execute_batch(sql).unwrap();
            tx.pragma_update(None, "user_version", i64::try_from(index + 1).unwrap())
                .unwrap();
            tx.commit().unwrap();
        }
        let chain = RegisteredChain::seed(&conn);
        // Before 0007: a vocalised name, a taa-marbuta spelling, a tatweel, and
        // an English-named product with a barcode.
        insert_product(
            &conn,
            &chain,
            0x10,
            "COFFEE-AR",
            "قَهْوَة عَرَبِيَّة",
            Some("Arabic coffee"),
        );
        insert_product(&conn, &chain, 0x11, "LABAN-TM", "لبنة", None);
        insert_product(&conn, &chain, 0x12, "TEA-TATWEEL", "شـــاي", Some("Tea"));
        insert_product(&conn, &chain, 0x13, "COLA-330", "كولا", Some("Cola 330ml"));
        insert_barcode(&conn, 0x20, 0x13, "6291041500213");
        conn.close().map_err(|(_, e)| e).unwrap();
        chain
    };
    let conn = pos_db::open(&path, KEY).unwrap();
    Db {
        _dir: dir,
        conn,
        chain,
    }
}

fn add_product(db: &Db, product: u128, sku: &str, name_ar: &str, name_en: Option<&str>) {
    insert_product(&db.conn, &db.chain, product, sku, name_ar, name_en);
}

/// The SKUs an FTS5 query matches, best first.
fn matches(db: &Db, query: &str) -> Vec<String> {
    db.conn
        .prepare(
            "SELECT p.sku
               FROM product_fts f
               JOIN product_fts_map m ON m.rowid = f.rowid
               JOIN product p ON p.id = m.product_id
              WHERE product_fts MATCH ?1
              ORDER BY f.rank, p.sku",
        )
        .unwrap()
        .query_map([query], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// What a cashier's Arabic keystrokes become, as 1.2.3 will build it: the
/// precision branch over `name_ar_exact` and the recall branch over
/// `name_ar_fold`, each with the typed text put through the same transform its
/// column was.
fn arabic(typed: &str, prefix: bool) -> String {
    let star = if prefix { "*" } else { "" };
    format!(
        r#"name_ar_exact : "{}"{star} OR name_ar_fold : "{}"{star}"#,
        search::exact(typed),
        search::fold(typed)
    )
}

fn arabic_matches(db: &Db, typed: &str) -> Vec<String> {
    matches(db, &arabic(typed, false))
}

// ── search ──────────────────────────────────────────────────────────────────

/// A name stored with tashkeel is found by the plain spelling a cashier types,
/// and a query typed with tashkeel finds a name stored without.
#[test]
fn fts_matches_arabic_with_and_without_diacritics() {
    let db = migrated();
    add_product(&db, 0x30, "COFFEE-TR", "قهوة تركية", None);
    // Both names carry the word as typed, so both match both branches and the
    // order between them is BM25's to choose; this test is about matching.
    let found = |typed: &str| {
        let mut skus = arabic_matches(&db, typed);
        skus.sort();
        skus
    };
    assert_eq!(found("قهوة"), vec!["COFFEE-AR", "COFFEE-TR"]);
    assert_eq!(found("قَهْوَة"), vec!["COFFEE-AR", "COFFEE-TR"]);
}

/// Hamza forms of alef find each other, and alef maqsura finds yaa.
#[test]
fn fts_matches_alef_and_yaa_spelling_variants() {
    let db = migrated();
    add_product(&db, 0x30, "KETTLE", "إبريق", None);
    add_product(&db, 0x31, "HOSPITAL-KIT", "مستشفى", None);
    for typed in ["ابريق", "أبريق", "إبريق", "آبريق"] {
        assert_eq!(arabic_matches(&db, typed), vec!["KETTLE"], "{typed}");
    }
    assert_eq!(arabic_matches(&db, "مستشفي"), vec!["HOSPITAL-KIT"]);
}

#[test]
fn fts_matches_taa_marbuta_spelled_as_haa() {
    let db = migrated();
    assert_eq!(arabic_matches(&db, "لبنه"), vec!["LABAN-TM"]);
    assert_eq!(arabic_matches(&db, "لبنة"), vec!["LABAN-TM"]);
}

/// Tatweel stretches a word for display; it is never part of the word.
#[test]
fn fts_ignores_tatweel() {
    let db = migrated();
    assert_eq!(arabic_matches(&db, "شاي"), vec!["TEA-TATWEEL"]);
    assert_eq!(arabic_matches(&db, "شــاي"), vec!["TEA-TATWEEL"]);
}

/// Search-as-you-type from two characters, served by the `prefix='2 3'` index.
#[test]
fn fts_prefix_search_works_at_two_characters() {
    let db = migrated();
    assert_eq!(matches(&db, &arabic("قه", true)), vec!["COFFEE-AR"]);
    assert_eq!(matches(&db, &arabic("لب", true)), vec!["LABAN-TM"]);
    let mut english = matches(&db, r#"name_en : "co"*"#);
    english.sort();
    assert_eq!(english, vec!["COFFEE-AR", "COLA-330"]);
}

/// Two products whose names differ only by taa marbuta and haa: whichever
/// spelling the cashier types, both are found, and that spelling ranks first.
#[test]
fn exact_spelling_outranks_a_folded_variant() {
    let db = migrated();
    add_product(&db, 0x30, "LABAN-HAA", "لبنه", None);
    assert_eq!(arabic_matches(&db, "لبنة"), vec!["LABAN-TM", "LABAN-HAA"]);
    assert_eq!(arabic_matches(&db, "لبنه"), vec!["LABAN-HAA", "LABAN-TM"]);
}

/// Raw `name_ar` is not indexed, so a vocalised name does not shred into
/// single letters that a one-letter query would match.
#[test]
fn a_single_letter_query_does_not_match_a_vocalised_name() {
    let db = migrated();
    for letter in ["ق", "ه", "و", "ة"] {
        assert!(
            arabic_matches(&db, letter).is_empty(),
            "{letter} matched a vocalised name"
        );
    }
}

#[test]
fn fts_matches_english_and_sku() {
    let db = migrated();
    assert_eq!(matches(&db, r#"name_en : "coffee""#), vec!["COFFEE-AR"]);
    assert_eq!(matches(&db, r#"sku : "laban""#), vec!["LABAN-TM"]);
    assert_eq!(
        matches(&db, r#"barcodes : "6291041500213""#),
        vec!["COLA-330"]
    );
}

/// E.41: an Arabic name — with tashkeel, mixed with Latin, digits and an emoji —
/// is stored and read back byte for byte, and found by its folded word.
#[test]
fn unicode_names_roundtrip_through_db_and_fts() {
    let db = migrated();
    let name = "حَلِيب Nido® ٤٠٠ غرام 🥛";
    add_product(&db, 0x30, "NIDO-400", name, Some("Nido milk 400g"));
    let stored: String = db
        .conn
        .query_row(
            "SELECT name_ar FROM product WHERE id = ?1",
            [&id(0x30)],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored.as_bytes(), name.as_bytes());
    assert_eq!(arabic_matches(&db, "حليب"), vec!["NIDO-400"]);
    assert_eq!(matches(&db, r#"name_ar_exact : "nido""#), vec!["NIDO-400"]);
}

/// A rename re-indexes the product, and moving a barcode re-indexes both
/// products: the old owner stops answering to the code and the new one starts.
#[test]
fn fts_survives_product_update() {
    let db = migrated();
    db.conn
        .execute(
            "UPDATE product SET name_ar = 'شاي أخضر' WHERE id = ?1",
            [&id(0x12)],
        )
        .unwrap();
    assert!(arabic_matches(&db, "شاي").contains(&"TEA-TATWEEL".to_owned()));
    assert_eq!(arabic_matches(&db, "اخضر"), vec!["TEA-TATWEEL"]);

    add_product(&db, 0x30, "COLA-ZERO", "كولا زيرو", None);
    db.conn
        .execute(
            "UPDATE barcode SET product_id = ?1 WHERE id = ?2",
            params![&id(0x30), &id(0x20)],
        )
        .unwrap();
    assert_eq!(
        matches(&db, r#"barcodes : "6291041500213""#),
        vec!["COLA-ZERO"]
    );
    db.conn
        .execute(
            "UPDATE product SET price_minor = 900 WHERE id = ?1",
            [&id(0x13)],
        )
        .unwrap();
    assert_eq!(arabic_matches(&db, "كولا"), vec!["COLA-330", "COLA-ZERO"]);
}

/// A tombstoned product leaves the index, a tombstoned barcode stops answering,
/// and a restored product comes back under the same map row.
#[test]
fn fts_row_removed_on_tombstone() {
    let db = migrated();
    let rowid = |db: &Db| -> i64 {
        db.conn
            .query_row(
                "SELECT rowid FROM product_fts_map WHERE product_id = ?1",
                [&id(0x11)],
                |row| row.get(0),
            )
            .unwrap()
    };
    let before = rowid(&db);
    db.conn
        .execute(
            "UPDATE product SET deleted_at = ?1 WHERE id = ?2",
            params![AT, &id(0x11)],
        )
        .unwrap();
    assert!(arabic_matches(&db, "لبنة").is_empty());
    db.conn
        .execute(
            "UPDATE product SET deleted_at = NULL WHERE id = ?1",
            [&id(0x11)],
        )
        .unwrap();
    assert_eq!(arabic_matches(&db, "لبنة"), vec!["LABAN-TM"]);
    assert_eq!(rowid(&db), before, "a restore reuses the stable rowid");

    db.conn
        .execute(
            "UPDATE barcode SET deleted_at = ?1 WHERE id = ?2",
            params![AT, &id(0x20)],
        )
        .unwrap();
    assert!(matches(&db, r#"barcodes : "6291041500213""#).is_empty());
    assert_eq!(
        arabic_matches(&db, "كولا"),
        vec!["COLA-330"],
        "the product stays"
    );
}

proptest! {
    // The workspace's shared I/O configuration (conventions §5.1).
    #![proptest_config(io_proptest_config())]

    /// For any text, the query-side fold in `pos_db::search` produces exactly
    /// what 0003's generated columns store. Two implementations of one rule
    /// drift unless something holds them together; this is that something.
    #[test]
    fn prop_sql_and_rust_folding_agree(
        text in prop::collection::vec(
            prop_oneof![
                3 => prop::sample::select(vec![
                    '\u{064B}', '\u{064C}', '\u{064D}', '\u{064E}', '\u{064F}', '\u{0650}',
                    '\u{0651}', '\u{0652}', '\u{0653}', '\u{0654}', '\u{0655}', '\u{0670}',
                    '\u{0640}', '\u{0623}', '\u{0625}', '\u{0622}', '\u{0671}', '\u{0649}',
                    '\u{0629}', '\u{0624}', '\u{0626}', '\u{0627}', '\u{064A}', '\u{0647}',
                    '\u{0648}', ' ', 'a', '7',
                ]),
                2 => proptest::char::range('\u{0621}', '\u{0670}'),
                1 => any::<char>().prop_filter("SQLite text cannot hold NUL", |c| *c != '\0'),
            ],
            0..24,
        ).prop_map(|chars| chars.into_iter().collect::<String>())
    ) {
        thread_local! {
            static DB: Db = migrated();
        }
        DB.with(|db| {
            let tx = db.conn.unchecked_transaction().unwrap();
            tx.execute(
                "INSERT INTO product (id, sku, name, name_ar, price_minor, currency, is_active,
                                      tax_category_id)
                 VALUES (?1, 'PROP', ?2, ?2, 0, 'JOD', 1, ?3)",
                params![&id(0xF0), &text, &db.chain.tax_category],
            )
            .unwrap();
            let (exact, fold): (String, String) = tx
                .query_row(
                    "SELECT name_ar_exact, name_ar_fold FROM product WHERE id = ?1",
                    [&id(0xF0)],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            drop(tx);
            prop_assert_eq!(search::exact(&text), exact);
            prop_assert_eq!(search::fold(&text), fold);
            Ok(())
        })?;
    }
}

// ── trade-scale commissioning ───────────────────────────────────────────────

const UNVERIFIED: &str = "embedded pricing requires verified trade-scale evidence";

fn commission_scale(db: &Db, scale: u128, store: &[u8], serial: &str) {
    db.conn
        .execute(
            "INSERT INTO trade_scale (id, store_id, maker, model, serial_number)
             VALUES (?1, ?2, 'Bizerba', 'K-Class', ?3)",
            params![&id(scale), store, serial],
        )
        .unwrap();
}

fn verification(
    db: &Db,
    event: u128,
    scale: u128,
    event_no: i64,
    state: &str,
) -> rusqlite::Result<usize> {
    db.conn.execute(
        "INSERT INTO trade_scale_verification
           (id, trade_scale_id, event_no, state, evidence_ref, evidence_hash, effective_at)
         VALUES (?1, ?2, ?3, ?4, 'JSMO-CERT-SYNTHETIC', ?5, ?6)",
        params![
            &id(event),
            &id(scale),
            event_no,
            state,
            vec![0x5Au8; 32],
            AT
        ],
    )
}

fn rule(db: &Db, rule: u128, scale: u128, active: bool) -> rusqlite::Result<usize> {
    db.conn.execute(
        "INSERT INTO embedded_barcode_rule
           (id, store_id, trade_scale_id, prefix, item_code_start, item_code_len,
            value_start, value_len, value_kind, is_active)
         VALUES (?1, ?2, ?3, '2', 1, 5, 7, 5, 'price_minor', ?4)",
        params![&id(rule), &db.chain.store, &id(scale), active],
    )
}

fn is_active(db: &Db, rule: u128) -> bool {
    db.conn
        .query_row(
            "SELECT is_active FROM embedded_barcode_rule WHERE id = ?1",
            [&id(rule)],
            |row| row.get(0),
        )
        .unwrap()
}

/// A rule may be switched on only for a scale of the same store whose newest
/// evidence says `verified`: not one with no evidence, and not another store's.
#[test]
fn embedded_rule_requires_verified_trade_scale() {
    let db = migrated();
    commission_scale(&db, 0x40, &db.chain.store, "SN-0001");
    assert_eq!(
        message(rule(&db, 0x50, 0x40, true).unwrap_err()),
        UNVERIFIED
    );
    rule(&db, 0x50, 0x40, false).unwrap();
    let activate = |db: &Db| {
        db.conn.execute(
            "UPDATE embedded_barcode_rule SET is_active = 1 WHERE id = ?1",
            [&id(0x50)],
        )
    };
    assert_eq!(message(activate(&db).unwrap_err()), UNVERIFIED);

    verification(&db, 0x60, 0x40, 1, "verified").unwrap();
    activate(&db).unwrap();
    assert!(is_active(&db, 0x50));

    // A verified scale of another store prices nothing here.
    db.conn
        .execute(
            "INSERT INTO store (id, org_id, code, name_ar, fiscal_obligation,
                                fiscal_obligation_evidence_ref, fiscal_profile,
                                tax_rule_pack_id, tax_computation_policy_id)
             SELECT ?1, org_id, 'OTHER', 'متجر آخر', fiscal_obligation,
                    fiscal_obligation_evidence_ref, fiscal_profile, tax_rule_pack_id,
                    tax_computation_policy_id
               FROM store WHERE id = ?2",
            params![&id(0x41), &db.chain.store],
        )
        .unwrap();
    commission_scale(&db, 0x42, &id(0x41), "SN-0002");
    verification(&db, 0x61, 0x42, 1, "verified").unwrap();
    assert_eq!(
        message(rule(&db, 0x51, 0x42, true).unwrap_err()),
        UNVERIFIED
    );
}

/// Identity is immutable and evidence is history: neither is edited or
/// removed, and evidence events are numbered without a gap.
#[test]
fn scale_verification_evidence_is_append_only() {
    let db = migrated();
    commission_scale(&db, 0x40, &db.chain.store, "SN-0001");
    verification(&db, 0x60, 0x40, 1, "verified").unwrap();
    let refused = |sql: &str| message(db.conn.execute(sql, []).unwrap_err());
    assert_eq!(
        refused("UPDATE trade_scale_verification SET state = 'revoked'"),
        "scale verification evidence is append-only"
    );
    assert_eq!(
        refused("DELETE FROM trade_scale_verification"),
        "scale verification history cannot be deleted"
    );
    assert_eq!(
        refused("UPDATE trade_scale SET serial_number = 'SN-9999'"),
        "trade-scale identity is immutable — append verification evidence"
    );
    assert_eq!(
        refused("DELETE FROM trade_scale"),
        "commissioned trade-scale identity cannot be deleted"
    );
    assert_eq!(
        message(verification(&db, 0x62, 0x40, 3, "revoked").unwrap_err()),
        "scale verification events must be contiguous"
    );
}

/// The moment a scale's newest evidence stops saying `verified`, every rule
/// bound to it switches off, and none can be switched back on until a later
/// event verifies the scale again.
#[test]
fn status_loss_disables_an_active_embedded_rule() {
    let db = migrated();
    commission_scale(&db, 0x40, &db.chain.store, "SN-0001");
    verification(&db, 0x60, 0x40, 1, "verified").unwrap();
    rule(&db, 0x50, 0x40, true).unwrap();
    assert!(is_active(&db, 0x50));

    verification(&db, 0x61, 0x40, 2, "maintenance_pending").unwrap();
    assert!(!is_active(&db, 0x50), "maintenance switched the rule off");
    let reactivate = |db: &Db| {
        db.conn.execute(
            "UPDATE embedded_barcode_rule SET is_active = 1 WHERE id = ?1",
            [&id(0x50)],
        )
    };
    assert_eq!(message(reactivate(&db).unwrap_err()), UNVERIFIED);

    verification(&db, 0x62, 0x40, 3, "verified").unwrap();
    reactivate(&db).unwrap();
    assert!(is_active(&db, 0x50));
}
