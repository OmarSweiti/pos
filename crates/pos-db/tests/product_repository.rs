//! `ProductRepository` (microstep 1.2.3): every lookup a cashier makes, and the
//! one save path a product takes into the catalogue.
//!
//! Each test opens a fresh encrypted register database through `pos_db::open`,
//! seeds the registered chain for its tax category, and builds its catalogue
//! through the repository itself — so a test that reads a product also proves
//! the write that put it there. Rows go in by hand only where the repository
//! has no write of its own (PLU codes, a category) or where the point is a row
//! no repository would write.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::DbError;
use pos_db::fixtures::registered_chain::{AT, RegisteredChain};
use pos_db::repo::product::ProductRepository;
use pos_domain::{
    Barcode, BarcodeKind, CatalogError, CategoryId, Currency, MatchedOn, Money, Product,
    ProductHit, ProductId, Qty, RegulatedSaleForm, ScanLookup, TaxCategoryId, UnitOfMeasure,
    parse_scan, resolve_scan,
};
use pos_test_support::io_proptest_config;
use proptest::prelude::*;
use rusqlite::{Connection, params};
use uuid::Uuid;

const KEY: &str = "product-repository-test-key";
const COLA_CODE: &str = "6291041500213";
const COLA_SIX_PACK: &str = "6291041500220";

struct Store {
    _dir: tempfile::TempDir,
    conn: Connection,
    chain: RegisteredChain,
}

fn store() -> Store {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    Store {
        _dir: dir,
        conn,
        chain,
    }
}

fn pid(n: u128) -> ProductId {
    ProductId::from_uuid(Uuid::from_u128(n))
}

fn row_id(n: u128) -> [u8; 16] {
    Uuid::from_u128(n).into_bytes()
}

fn jod(minor: i64) -> Money {
    Money::from_minor(minor, Currency::JOD)
}

/// A plain product: counted, active, unregulated, one dinar.
fn product(s: &Store, n: u128, sku: &str, name_ar: &str, name_en: Option<&str>) -> Product {
    Product {
        id: pid(n),
        sku: sku.to_owned(),
        name_ar: name_ar.to_owned(),
        name_en: name_en.map(str::to_owned),
        category_id: None,
        tax_category_id: TaxCategoryId::from_uuid(Uuid::from_slice(&s.chain.tax_category).unwrap()),
        unit: UnitOfMeasure::Each,
        unit_price: jod(1_000),
        is_weighed: false,
        is_service: false,
        is_active: true,
        min_age: None,
        max_price: None,
        reorder_point: None,
        regulated_sale_form: RegulatedSaleForm::UNREGULATED_SEALED_PACK,
    }
}

fn save(s: &Store, product: &Product) -> Result<(), DbError> {
    let tx = s.conn.unchecked_transaction().unwrap();
    ProductRepository::new(&s.conn).upsert(&tx, product)?;
    tx.commit().unwrap();
    Ok(())
}

fn claim(s: &Store, row: u128, product: u128, code: &str, pack_qty: Qty) -> Result<(), DbError> {
    let tx = s.conn.unchecked_transaction().unwrap();
    ProductRepository::new(&s.conn).add_barcode(
        &tx,
        &row_id(row),
        &Barcode {
            product_id: pid(product),
            code: code.to_owned(),
            kind: BarcodeKind::Ean13,
            is_primary: pack_qty == Qty::ONE,
            pack_qty,
        },
    )?;
    tx.commit().unwrap();
    Ok(())
}

fn retire(s: &Store, table: &str, id: &[u8]) {
    let changed = s
        .conn
        .execute(
            &format!("UPDATE {table} SET deleted_at = ?1 WHERE id = ?2"),
            params![AT, id],
        )
        .unwrap();
    assert_eq!(changed, 1, "retired one {table} row");
}

fn skus(hits: &[ProductHit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.product.sku.as_str()).collect()
}

fn search(s: &Store, typed: &str) -> Vec<String> {
    let hits = ProductRepository::new(&s.conn)
        .search(typed, 20)
        .unwrap_or_else(|error| panic!("{typed:?} must not be an error: {error}"));
    hits.iter().map(|hit| hit.product.sku.clone()).collect()
}

/// A small catalogue every search test can share: two coffees, two labnehs
/// that differ only by taa marbuta, a cola with a barcode, and salt and
/// pepper, whose English name holds the word *and*.
fn catalogue() -> Store {
    let s = store();
    for p in [
        product(&s, 1, "COFFEE-AR", "قَهْوَة عَرَبِيَّة", Some("Arabic coffee")),
        product(&s, 2, "COFFEE-TR", "قهوة تركية", Some("Turkish coffee")),
        product(&s, 3, "LABNEH-TM", "لبنة", Some("Labneh")),
        product(&s, 4, "LABNEH-HAA", "لبنه بلدية", None),
        product(&s, 5, "COLA-330", "كولا", Some("Cola 330ml")),
        product(&s, 6, "SALT-PEPPER", "ملح وفلفل", Some("Salt and pepper")),
    ] {
        save(&s, &p).unwrap();
    }
    claim(&s, 0x100, 5, COLA_CODE, Qty::ONE).unwrap();
    s
}

#[test]
fn a_second_live_barcode_claim_is_refused() {
    // E.36. A code is a lookup key with one live owner: a second product's
    // claim is refused and names the holder, and nothing lands.
    let s = store();
    save(&s, &product(&s, 1, "COLA-330", "كولا", None)).unwrap();
    save(&s, &product(&s, 2, "COLA-ZERO", "كولا زيرو", None)).unwrap();
    claim(&s, 0x100, 1, COLA_CODE, Qty::ONE).unwrap();

    match claim(&s, 0x101, 2, COLA_CODE, Qty::ONE) {
        Err(DbError::BarcodeAlreadyClaimed { code, holder }) => {
            assert_eq!(code, COLA_CODE);
            assert_eq!(holder, pid(1));
        }
        other => panic!("a second live claim must be refused by name, found {other:?}"),
    }
    // The same product claiming its own code twice is a second claim too.
    assert!(matches!(
        claim(&s, 0x102, 1, COLA_CODE, Qty::ONE),
        Err(DbError::BarcodeAlreadyClaimed { .. })
    ));
    let repo = ProductRepository::new(&s.conn);
    assert_eq!(skus(&repo.by_barcode(COLA_CODE).unwrap()), ["COLA-330"]);

    // Beneath the repository, the schema refuses it as well: the partial
    // unique index is the backstop, not the only line.
    let raw = s
        .conn
        .execute(
            "INSERT INTO barcode (id, product_id, code) VALUES (?1, ?2, ?3)",
            params![
                row_id(0x103).as_slice(),
                pid(2).as_uuid().as_bytes(),
                COLA_CODE
            ],
        )
        .unwrap_err();
    assert!(
        raw.to_string().contains("UNIQUE constraint failed"),
        "the index must refuse a second live code: {raw}"
    );

    // Retiring the old claim frees the code, and it now finds only its new
    // owner.
    retire(&s, "barcode", &row_id(0x100));
    claim(&s, 0x104, 2, COLA_CODE, Qty::ONE).unwrap();
    assert_eq!(skus(&repo.by_barcode(COLA_CODE).unwrap()), ["COLA-ZERO"]);
}

#[test]
fn a_multipack_barcode_adds_its_pack_quantity() {
    // E.78. One product, two codes: the can and the six-pack outer. The
    // quantity a scan means comes from the code that matched, and it carries
    // through the domain's scan resolution to the line it will become.
    let s = store();
    let cola = product(&s, 1, "COLA-330", "كولا", Some("Cola 330ml"));
    save(&s, &cola).unwrap();
    claim(&s, 0x100, 1, COLA_CODE, Qty::ONE).unwrap();
    claim(&s, 0x101, 1, COLA_SIX_PACK, Qty::from_units(6).unwrap()).unwrap();
    let repo = ProductRepository::new(&s.conn);

    let [can] = repo.by_barcode(COLA_CODE).unwrap().try_into().unwrap();
    let [case] = repo.by_barcode(COLA_SIX_PACK).unwrap().try_into().unwrap();
    assert_eq!(can.product, cola);
    assert_eq!(case.product, cola, "the same product behind both codes");
    assert_eq!(
        case.matched_on,
        MatchedOn::Barcode {
            code: COLA_SIX_PACK.to_owned(),
            kind: BarcodeKind::Ean13,
            pack_qty: Qty::from_milli(6_000),
        }
    );

    // The scan path end to end: parse, look the item code up, resolve.
    for (code, units) in [(COLA_CODE, 1), (COLA_SIX_PACK, 6)] {
        let parsed = parse_scan(code, &[]).unwrap();
        let hits = repo.by_barcode(parsed.item_code()).unwrap();
        let ScanLookup::Product { hit, pack_qty } =
            resolve_scan(parsed, hits, Currency::JOD).unwrap()
        else {
            panic!("{code} must resolve to one product");
        };
        assert_eq!(hit.product.id, cola.id);
        assert_eq!(pack_qty, Qty::from_units(units).unwrap(), "{code}");
    }
}

#[test]
fn by_id_ignores_tombstones() {
    // A retired product is gone from every lookup at once — by id, by code,
    // by PLU, by name — while an inactive one is still found, flagged, for the
    // cart to refuse by name (E.38).
    let s = store();
    let cheese = Product {
        unit: UnitOfMeasure::Kilogram,
        is_weighed: true,
        ..product(&s, 1, "CHEESE-AKKAWI", "جبنة عكاوي", Some("Akkawi cheese"))
    };
    save(&s, &cheese).unwrap();
    claim(&s, 0x100, 1, COLA_CODE, Qty::ONE).unwrap();
    s.conn
        .execute(
            "INSERT INTO plu_code (code, product_id) VALUES ('10234', ?1)",
            [pid(1).as_uuid().as_bytes()],
        )
        .unwrap();
    let repo = ProductRepository::new(&s.conn);
    assert_eq!(repo.by_id(pid(1)).unwrap(), Some(cheese.clone()));
    assert_eq!(repo.by_plu("10234").unwrap(), Some(cheese));
    assert_eq!(search(&s, "عكاوي"), ["CHEESE-AKKAWI"]);

    retire(&s, "product", pid(1).as_uuid().as_bytes());
    assert_eq!(repo.by_id(pid(1)).unwrap(), None);
    assert!(repo.by_barcode(COLA_CODE).unwrap().is_empty());
    assert_eq!(repo.by_plu("10234").unwrap(), None);
    assert!(search(&s, "عكاوي").is_empty());

    let discontinued = Product {
        is_active: false,
        ..product(&s, 2, "OLD-TEA", "شاي قديم", None)
    };
    save(&s, &discontinued).unwrap();
    assert_eq!(repo.by_id(pid(2)).unwrap(), Some(discontinued));
    assert_eq!(search(&s, "شاي"), ["OLD-TEA"]);
}

#[test]
fn catalog_save_above_ceiling_is_rejected() {
    // E.71: a controlled staple priced above the ministry ceiling never
    // reaches the catalogue — not on insert, and not by a later price rise —
    // and the refusal is the domain's, carried up unchanged.
    let s = store();
    let bread = product(&s, 1, "BREAD-ARABIC", "خبز عربي", Some("Arabic bread"));
    let over = Product {
        unit_price: jod(1_001),
        max_price: Some(jod(1_000)),
        ..bread.clone()
    };
    match save(&s, &over) {
        Err(DbError::CatalogSaveRefused(CatalogError::AboveMaxPrice { price, ceiling })) => {
            assert_eq!((price, ceiling), (jod(1_001), jod(1_000)));
        }
        other => panic!("a price above the ceiling must be refused, found {other:?}"),
    }
    let repo = ProductRepository::new(&s.conn);
    assert_eq!(repo.by_id(pid(1)).unwrap(), None, "nothing was written");

    let at_ceiling = Product {
        max_price: Some(jod(1_000)),
        ..bread
    };
    save(&s, &at_ceiling).unwrap();
    assert!(matches!(
        save(
            &s,
            &Product {
                unit_price: jod(1_001),
                ..at_ceiling.clone()
            }
        ),
        Err(DbError::CatalogSaveRefused(
            CatalogError::AboveMaxPrice { .. }
        ))
    ));
    assert_eq!(
        repo.by_id(pid(1)).unwrap(),
        Some(at_ceiling),
        "the stored price stands"
    );
}

/// One row of E.39b's corpus: what the text finds alone, what it finds inside
/// a real query, and what FTS5 would have done had it read the text as syntax.
struct MetacharacterCase {
    alone: &'static str,
    finds_alone: &'static [&'static str],
    in_context: &'static str,
    finds_in_context: &'static [&'static str],
    as_syntax: &'static str,
}

const COFFEES: &[&str] = &["COFFEE-AR", "COFFEE-TR"];

/// E.39b's corpus, exactly: `"`, `(`, `)`, `:`, `*`, `^`, `-`, AND, OR, NOT,
/// NEAR, empty and whitespace-only. Each in-context query is chosen so that
/// reading the text as syntax would have raised an error or found something
/// else.
const METACHARACTERS: [MetacharacterCase; 13] = [
    MetacharacterCase {
        alone: "\"",
        finds_alone: &[],
        in_context: "قهوة\"",
        finds_in_context: COFFEES,
        as_syntax: "an unterminated string: an error",
    },
    MetacharacterCase {
        alone: "(",
        finds_alone: &[],
        in_context: "(قهوة",
        finds_in_context: COFFEES,
        as_syntax: "an unclosed group: a syntax error",
    },
    MetacharacterCase {
        alone: ")",
        finds_alone: &[],
        in_context: "قهوة)",
        finds_in_context: COFFEES,
        as_syntax: "a stray close: a syntax error",
    },
    MetacharacterCase {
        alone: ":",
        finds_alone: &[],
        in_context: "sku:COLA",
        finds_in_context: &[],
        as_syntax: "a column filter: the cola",
    },
    MetacharacterCase {
        alone: "*",
        finds_alone: &[],
        in_context: "*قهوة",
        finds_in_context: COFFEES,
        as_syntax: "an unknown special query: an error",
    },
    MetacharacterCase {
        alone: "^",
        finds_alone: &[],
        in_context: "كولا^",
        finds_in_context: &["COLA-330"],
        as_syntax: "a misplaced initial-token marker: a syntax error",
    },
    MetacharacterCase {
        alone: "-",
        finds_alone: &[],
        in_context: "cola-330",
        finds_in_context: &["COLA-330"],
        as_syntax: "a column exclusion: an error, no such column 330",
    },
    MetacharacterCase {
        alone: "AND",
        finds_alone: &["SALT-PEPPER"],
        in_context: "cola AND",
        finds_in_context: &[],
        as_syntax: "a dangling AND: a syntax error",
    },
    MetacharacterCase {
        alone: "OR",
        finds_alone: &[],
        in_context: "قهوة OR كولا",
        finds_in_context: &[],
        as_syntax: "a disjunction: both coffees and the cola",
    },
    MetacharacterCase {
        alone: "NOT",
        finds_alone: &[],
        in_context: "coffee NOT turkish",
        finds_in_context: &[],
        as_syntax: "a difference: the Arabic coffee alone",
    },
    MetacharacterCase {
        alone: "NEAR",
        finds_alone: &[],
        in_context: "NEAR(قهوة عربية)",
        finds_in_context: &[],
        as_syntax: "a proximity group: the Arabic coffee",
    },
    MetacharacterCase {
        alone: "",
        finds_alone: &[],
        in_context: "",
        finds_in_context: &[],
        as_syntax: "an empty expression: a syntax error",
    },
    MetacharacterCase {
        alone: " \t\n ",
        finds_alone: &[],
        in_context: "   ",
        finds_in_context: &[],
        as_syntax: "a blank expression: a syntax error",
    },
];

#[test]
fn search_survives_every_fts5_metacharacter() {
    // E.39b: none of the corpus may raise an error, and none may change the
    // query — so each row is searched alone and inside a real query, and each
    // result is the one the text means as words.
    let s = catalogue();
    for case in &METACHARACTERS {
        assert_eq!(
            search(&s, case.alone),
            case.finds_alone,
            "{:?} alone",
            case.alone
        );
        assert_eq!(
            search(&s, case.in_context),
            case.finds_in_context,
            "{:?} — as FTS5 syntax it would have been {}",
            case.in_context,
            case.as_syntax
        );
    }
    // The words are words: AND finds *and* in "Salt and pepper", as a term.
    assert_eq!(search(&s, "salt AND pepper"), ["SALT-PEPPER"]);
}

#[test]
fn search_ranks_the_typed_spelling_first_and_finds_codes() {
    let s = catalogue();
    // Two labnehs that differ only by taa marbuta: both are found whichever is
    // typed, and the spelling typed comes first.
    assert_eq!(search(&s, "لبنة"), ["LABNEH-TM", "LABNEH-HAA"]);
    assert_eq!(search(&s, "لبنه"), ["LABNEH-HAA", "LABNEH-TM"]);
    // Tashkeel typed or not, the vocalised name is found.
    assert_eq!(search(&s, "قَهْوَة"), ["COFFEE-AR", "COFFEE-TR"]);
    // A label that will not scan: the digits under the bars, whole or begun,
    // and the SKU and the English name.
    assert_eq!(search(&s, COLA_CODE), ["COLA-330"]);
    assert_eq!(search(&s, "629104"), ["COLA-330"]);
    assert_eq!(search(&s, "COLA-330"), ["COLA-330"]);
    assert_eq!(search(&s, "turkish cof"), ["COFFEE-TR"]);
    // A search hit is a name match: it is never a scan's, so it carries no
    // pack quantity.
    let repo = ProductRepository::new(&s.conn);
    let [hit] = repo.search(COLA_CODE, 20).unwrap().try_into().unwrap();
    assert_eq!(hit.matched_on, MatchedOn::Name);
    // At most `limit`, best first; zero asks for nothing.
    assert_eq!(skus(&repo.search("قهوة", 1).unwrap()), ["COFFEE-AR"]);
    assert!(repo.search("قهوة", 0).unwrap().is_empty());
}

#[test]
fn every_unit_round_trips_through_the_unit_column() {
    // The bijection 1.2.3 chose: five units, five stored dimensions, and a
    // quantity step the cart's rule implies — whole units for counted goods,
    // any milli-unit for a measure.
    let s = store();
    let units = [
        (UnitOfMeasure::Each, "each", 1_000),
        (UnitOfMeasure::Package, "package", 1_000),
        (UnitOfMeasure::Kilogram, "weight", 1),
        (UnitOfMeasure::Litre, "volume", 1),
        (UnitOfMeasure::Metre, "length", 1),
    ];
    let repo = ProductRepository::new(&s.conn);
    for (n, (unit, token, step)) in (1_u128..).zip(units) {
        let saved = Product {
            unit,
            is_weighed: unit.is_divisible(),
            ..product(&s, n, &format!("UNIT-{n}"), "منتج", None)
        };
        save(&s, &saved).unwrap();
        assert_eq!(
            repo.by_id(saved.id).unwrap(),
            Some(saved.clone()),
            "{unit:?}"
        );
        let stored: (String, i64) = s
            .conn
            .query_row(
                "SELECT unit, qty_step_milli FROM product WHERE id = ?1",
                [saved.id.as_uuid().as_bytes()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (token.to_owned(), step), "{unit:?}");
    }
}

#[test]
fn a_product_round_trips_through_upsert_and_by_id() {
    // Every field, set: a category, an English name, an age, a ceiling, a
    // reorder point and the regulated pair. Then an edit replaces every field,
    // and search follows the new name.
    let s = store();
    let category = row_id(0x200);
    s.conn
        .execute(
            "INSERT INTO category (id, name_ar, name_en) VALUES (?1, 'تبغ', 'Tobacco')",
            [category.as_slice()],
        )
        .unwrap();
    let cigarettes = Product {
        name_en: Some("Cigarettes, 20".to_owned()),
        category_id: Some(CategoryId::from_uuid(Uuid::from_bytes(category))),
        unit_price: jod(2_750),
        min_age: Some(18),
        max_price: Some(jod(2_750)),
        reorder_point: Some(Qty::from_units(10).unwrap()),
        regulated_sale_form: RegulatedSaleForm::Tobacco,
        ..product(&s, 1, "CIG-20", "سجائر", None)
    };
    save(&s, &cigarettes).unwrap();
    let repo = ProductRepository::new(&s.conn);
    assert_eq!(repo.by_id(pid(1)).unwrap(), Some(cigarettes.clone()));

    let renamed = Product {
        name_ar: "سجائر عشرين".to_owned(),
        unit_price: jod(2_500),
        max_price: None,
        min_age: Some(19),
        reorder_point: None,
        category_id: None,
        ..cigarettes
    };
    save(&s, &renamed).unwrap();
    assert_eq!(repo.by_id(pid(1)).unwrap(), Some(renamed));
    assert_eq!(search(&s, "عشرين"), ["CIG-20"]);
    let count: i64 = s
        .conn
        .query_row("SELECT count(*) FROM product", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1, "an edit is an update, not a second row");
}

#[test]
fn by_plu_finds_a_live_code_only() {
    let s = store();
    let tomatoes = Product {
        unit: UnitOfMeasure::Kilogram,
        is_weighed: true,
        ..product(&s, 1, "TOMATO", "بندورة", Some("Tomatoes"))
    };
    save(&s, &tomatoes).unwrap();
    s.conn
        .execute(
            "INSERT INTO plu_code (code, product_id) VALUES ('4087', ?1)",
            [pid(1).as_uuid().as_bytes()],
        )
        .unwrap();
    let repo = ProductRepository::new(&s.conn);
    assert_eq!(repo.by_plu("4087").unwrap(), Some(tomatoes));
    assert_eq!(repo.by_plu("4088").unwrap(), None);
    s.conn
        .execute(
            "UPDATE plu_code SET deleted_at = ?1 WHERE code = '4087'",
            [AT],
        )
        .unwrap();
    assert_eq!(repo.by_plu("4087").unwrap(), None);
}

#[test]
fn a_retired_product_is_not_revived_by_an_upsert() {
    let s = store();
    let tea = product(&s, 1, "TEA", "شاي", None);
    save(&s, &tea).unwrap();
    retire(&s, "product", pid(1).as_uuid().as_bytes());
    assert!(matches!(save(&s, &tea), Err(DbError::ProductRetired(id)) if id == pid(1)));
    let deleted_at: Option<String> = s
        .conn
        .query_row(
            "SELECT deleted_at FROM product WHERE id = ?1",
            [pid(1).as_uuid().as_bytes()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(deleted_at.as_deref(), Some(AT), "still retired");
}

#[test]
fn a_product_without_a_tax_category_is_reported_not_guessed() {
    // Storage allows an inactive product with no tax category; a `Product`
    // cannot hold one. Asked for it by id, the repository says so. A search
    // passes over it, because one such row must not turn the cashier's
    // fallback into an error.
    let s = catalogue();
    s.conn
        .execute(
            "INSERT INTO product (id, sku, name, name_ar, price_minor, currency, is_active)
             VALUES (?1, 'OLD-COFFEE', 'قهوة قديمة', 'قهوة قديمة', 500, 'JOD', 0)",
            [pid(9).as_uuid().as_bytes()],
        )
        .unwrap();
    assert!(matches!(
        ProductRepository::new(&s.conn).by_id(pid(9)),
        Err(DbError::StoredProductInvalid { .. })
    ));
    assert_eq!(search(&s, "قهوة"), ["COFFEE-AR", "COFFEE-TR"]);
}

/// What a cashier might type: anything at all, and — weighted, so the
/// interesting cases are not left to chance — runs of FTS5's own operators
/// and punctuation mixed with Arabic, Latin and digits.
fn typed_text() -> impl Strategy<Value = String> {
    prop_oneof![
        1 => any::<String>(),
        3 => prop::collection::vec(
            prop_oneof![
                3 => prop::sample::select(vec![
                    "\"", "(", ")", ":", "*", "^", "-", "+", "{", "}", ",", ".", "'",
                    "AND", "OR", "NOT", "NEAR", "NEAR(", " ", "\t", "\n", "\u{0640}", "\u{064E}",
                ])
                .prop_map(str::to_owned),
                2 => "[\u{0621}-\u{064A}]{1,6}",
                2 => "[a-zA-Z0-9]{1,6}",
            ],
            0..12,
        )
        .prop_map(|parts| parts.concat()),
    ]
}

proptest! {
    // The workspace's shared I/O configuration (conventions §5.1).
    #![proptest_config(io_proptest_config())]

    /// A cashier's keystrokes reach an FTS5 `MATCH` expression, and none of
    /// them may throw: whatever is typed, search answers with a list of at
    /// most `limit` live products, never an error (E.39b).
    #[test]
    fn prop_no_query_string_produces_a_database_error(typed in typed_text(), limit in 0_u32..12) {
        thread_local! {
            static CATALOGUE: Store = catalogue();
        }
        CATALOGUE.with(|s| {
            let found = ProductRepository::new(&s.conn).search(&typed, limit);
            prop_assert!(found.is_ok(), "{:?} raised {:?}", typed, found);
            let hits = found.unwrap();
            prop_assert!(hits.len() <= limit as usize);
            prop_assert!(hits.iter().all(|hit| hit.matched_on == MatchedOn::Name));
            Ok(())
        })?;
    }
}
