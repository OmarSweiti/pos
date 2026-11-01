//! PIN verification under the reviewed Argon2id profile (microstep 1.6.2).
//!
//! `cargo bench -p pos-db --bench pin_verify` times [`verify_pin`] on a PHC
//! string [`hash_pin`] made under [`PIN_PROFILE`]; `cargo test -p pos-db
//! --bench pin_verify` runs it once instead, which is how a gate knows the
//! benchmark still runs. The wrong PIN is timed beside the right one, because
//! the two must cost the same: a faster refusal would tell a guesser it was
//! wrong before the hash was done.
//!
//! **This file claims no budget.** Conventions §7 targets 250 ms, with a median
//! of 200–350 ms and a 99th percentile under 500 ms, on the reference register,
//! and no reference register exists before milestone 5.8. A figure this prints
//! on a laptop is indicative and is never committed as a baseline; 5.8.2
//! records the first one and proves the band with `just bench-gate pin-verify`,
//! re-tuning [`PIN_PROFILE`] if it falls outside it.
#![allow(clippy::expect_used)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use pos_db::auth::{PIN_PROFILE, hash_pin, verify_pin};

/// §7: "50 measured samples after warm-up".
const SAMPLES: usize = 50;

fn pin_verify(c: &mut Criterion) {
    let hash = hash_pin("482916").expect("a PIN hashes under the reviewed profile");
    assert_eq!(
        pos_db::auth::profile_of(&hash).expect("the hash reads back"),
        PIN_PROFILE
    );
    let mut group = c.benchmark_group("pin_verify");
    group.sample_size(SAMPLES);
    for (label, pin) in [("right", "482916"), ("wrong", "482917")] {
        group.bench_function(label, |b| {
            b.iter(|| verify_pin(black_box(pin), black_box(&hash)).expect("a verdict"));
        });
    }
    group.finish();
}

criterion_group!(benches, pin_verify);
criterion_main!(benches);
