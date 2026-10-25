//! Search-as-you-type over 50 000 products (microstep 1.2.7, gap G-9).
//!
//! `cargo bench -p pos-db --bench search` times
//! [`ProductRepository::search`] at each length in [`QUERIES`] — 1, 2, 3 and 5
//! characters — over the catalogue in [`search_catalogue`], on an encrypted
//! register database [`pos_db::open`] built. `cargo test -p pos-db --bench
//! search` runs each length once instead, which is how a gate knows the
//! benchmark still runs. `tests/search_benchmark.rs` checks the catalogue and
//! the queries this file reads: the size, and that every length fills a page.
//!
//! **This file claims no budget.** §7's limit is a p99 under 50 ms on the
//! reference register, and no reference register exists before milestone 5.8
//! (conventions §7.1–§7.2). A figure this prints on a laptop or a hosted
//! runner is indicative and is never committed as a baseline: 1.12.3 compares
//! a pull request against its base on one runner, and 5.8.2 records the first
//! baseline and proves the limit with `just bench-gate search`.
#![allow(clippy::expect_used)]

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use pos_db::fixtures::search_catalogue::{self, QUERIES, RESULT_LIMIT};
use pos_db::repo::product::ProductRepository;

const KEY: &str = "search-benchmark-key";

/// §7: "50 measured samples after warm-up". criterion's warm-up runs first
/// and is not counted among them.
const SAMPLES: usize = 50;

fn search_as_you_type(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("a directory for the register database");
    let conn = pos_db::open(&dir.path().join("search.db"), KEY)
        .expect("an encrypted register database at the current schema");
    search_catalogue::seed(&conn);
    let repository = ProductRepository::new(&conn);

    let mut group = c.benchmark_group("search");
    group.sample_size(SAMPLES);
    for typed in QUERIES {
        let length = typed.chars().count();
        group.bench_with_input(BenchmarkId::new("chars", length), typed, |b, typed| {
            b.iter(|| {
                repository
                    .search(black_box(typed), RESULT_LIMIT)
                    .expect("a search is a list, never an error")
            });
        });
    }
    group.finish();
}

criterion_group!(benches, search_as_you_type);
criterion_main!(benches);
