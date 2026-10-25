//! The search benchmark's fixture and queries (microstep 1.2.7): what
//! `benches/search.rs` claims to measure, checked where a test can fail.
//!
//! `cargo bench` reports a time for whatever it was given. A catalogue that
//! silently held 500 products, or a query that matched nothing, would report a
//! fast till and be wrong about it. These two tests read the same
//! [`search_catalogue`] the benchmark reads.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::fixtures::search_catalogue::{self, PRODUCTS, QUERIES, RESULT_LIMIT};
use pos_db::repo::product::ProductRepository;
use pos_db::search::match_expression;
use rusqlite::Connection;

const KEY: &str = "search-benchmark-test-key";

fn catalogue() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("search.db"), KEY).unwrap();
    search_catalogue::seed(&conn);
    (dir, conn)
}

fn count(conn: &Connection, sql: &str) -> usize {
    let n: i64 = conn.query_row(sql, [], |row| row.get(0)).unwrap();
    usize::try_from(n).unwrap()
}

#[test]
fn search_benchmark_fixture_contains_50000_products() {
    // The literal, not the constant: `PRODUCTS` is derived from the
    // vocabulary, and §7's budget is over 50 000 SKUs whatever it holds.
    assert_eq!(PRODUCTS, 50_000);
    let (_dir, conn) = catalogue();

    // 50 000 live products, every one a distinct name, every one indexed:
    // `0007`'s insert trigger maps a product to its index row, so a product
    // missing from the map is a product no search can return.
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM product WHERE deleted_at IS NULL"
        ),
        PRODUCTS
    );
    assert_eq!(
        count(&conn, "SELECT count(DISTINCT name_ar) FROM product"),
        PRODUCTS
    );
    assert_eq!(
        count(&conn, "SELECT count(*) FROM product_fts_map"),
        PRODUCTS
    );

    // Arabic names, and both projections with work to do: tashkeel for
    // `name_ar_exact` to strip, spellings for `name_ar_fold` to collapse.
    let mut names = conn.prepare("SELECT name_ar FROM product").unwrap();
    let arabic = names
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .filter(|name| name.chars().any(|c| ('\u{0621}'..='\u{064A}').contains(&c)))
        .count();
    assert_eq!(arabic, PRODUCTS, "every name is Arabic");
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM product WHERE name_ar <> name_ar_exact"
        ),
        PRODUCTS / 10,
        "every tenth name carries tashkeel"
    );
    // A name escapes the fold only if its item (28 of 40), brand (15 of 25)
    // and qualifier (7 of 10) all carry no hamza, alef maqsura or taa marbuta:
    // 28 · 15 · 7 · 5 sizes = 14 700 names.
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM product WHERE name_ar_exact <> name_ar_fold"
        ),
        PRODUCTS - 14_700,
        "names with a spelling the fold collapses"
    );
}

#[test]
fn search_benchmark_exercises_every_query_length() {
    // The lengths 1.2.7 names, typed in order: each query is the last one and
    // one more keystroke (two, from 3 to 5).
    let lengths: Vec<usize> = QUERIES.iter().map(|q| q.chars().count()).collect();
    assert_eq!(lengths, [1, 2, 3, 5]);
    for (earlier, later) in QUERIES.iter().zip(QUERIES.iter().skip(1)) {
        assert!(later.starts_with(earlier), "{earlier:?} then {later:?}");
    }

    let (_dir, conn) = catalogue();
    let repository = ProductRepository::new(&conn);
    let matches = |typed: &str| {
        let expression = match_expression(typed).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM product_fts WHERE product_fts MATCH ?1",
                [expression],
                |row| row.get(0),
            )
            .unwrap();
        usize::try_from(n).unwrap()
    };

    // How much each length asks of the index. The first keystroke matches
    // every product, so no first letter costs more; the fifth narrows to one
    // brand's 2 000.
    let expected = [PRODUCTS, PRODUCTS, 4_000, 2_000];
    for (typed, expected) in QUERIES.iter().zip(expected) {
        assert_eq!(matches(typed), expected, "{typed:?} matches");

        // And no length measures the empty path: each fills its page.
        let hits = repository.search(typed, RESULT_LIMIT).unwrap();
        assert_eq!(
            hits.len(),
            usize::try_from(RESULT_LIMIT).unwrap(),
            "{typed:?} fills a page"
        );
    }
}
