//! Typed ids, and the `IdSource` port that mints them.
//!
//! Two rules meet in this file. **Identity is typed** — a schema with fifteen id
//! columns hands the compiler fifteen chances to catch an argument swap, and a
//! bare `Uuid` throws all fifteen away. And **`pos-domain` is pure (I-8)**: this
//! crate may not generate a UUID, because generating one reads an RNG and, for
//! version 7, a clock. So `uuid` is here with `default-features = false` and only
//! `serde` enabled, `Uuid::now_v7` is unreachable by construction, and every id a
//! domain function needs arrives as an argument or comes from an injected
//! [`IdSource`].
//!
//! `Clock` and `FixedClock`, the other port named in `ref/domain-api.md` §2, land
//! with microstep 1.1.9 instead: `Clock::now` returns `Timestamp`, `Timestamp`
//! lives in `time.rs`, and a second definition of time written here to make a
//! trait compile early is worse than a trait that waits one microstep.
//!
//! Nothing in this module can fail, so there is no `IdsError`. The two places a
//! caller could hand over a value too wide for the field it lands in — the
//! millisecond anchor and the sequence number of [`SeqIdSource`] — mask rather
//! than refuse, which is what keeps `next` infallible and total.

use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Declare a typed id: a newtype over `Uuid` that no other id can be mistaken
/// for.
///
/// The derive list is the whole design, so each entry has a reason:
///
/// - `Debug` — an id appears inside every `Debug` domain struct, every
///   `thiserror` payload that names the row it refused, and every failing
///   `assert_eq!`.
/// - `Clone`, `Copy` — sixteen bytes with no allocation and no interior
///   mutability. `Copy` is what lets an id be read out of a struct and passed
///   on without a borrow dance, and it cannot be observed as a move.
/// - `PartialEq`, `Eq` — "is this the same row" is the only question this type
///   exists to answer. `Eq` is sound because a `Uuid` is sixteen bytes with no
///   unequal-to-itself value.
/// - `Hash` — ids are `HashMap` keys throughout: lines by `SaleLineId`, the
///   on-hand cache by `ProductId`, rate lookups by `TaxCategoryId`. Derived
///   beside `Eq`, so the `Hash`/`Eq` contract holds by construction.
/// - `PartialOrd`, `Ord` — ids are `BTreeMap` keys and sort keys, and a
///   deterministic total order is what makes a sorted report or a proration
///   input reproducible on every machine (`pricing.rs` sorts lines before it
///   calls `Money::split_proportional_by`). **The order is the UUID's sixteen
///   bytes, big-endian, and nothing else.** A UUIDv7 embeds a device timestamp,
///   so that order *looks* chronological and is not: I-7 gives causal order to
///   owned sequences — the server's `version`, `sync_outbox.seq` — never to an
///   id. Sort by it to get a stable order; never to learn what happened first.
/// - `Serialize`, `Deserialize` — ids cross the IPC boundary and the sync wire
///   in both directions, so the inbound half is as load-bearing as the outbound
///   one. `ref/domain-api.md` §2 names only `Serialize`; a snapshot coming back
///   from the webview, and a `PushBatch` arriving at the server, both need
///   `Deserialize`.
///
/// `#[serde(transparent)]` pins the wire form to the `Uuid`'s own — the string
/// `"019b76da-a800-7000-8000-000000000000"` in JSON, sixteen bytes in a compact
/// format — rather than leaving it to how a given format happens to treat a
/// newtype struct. Contrast `Currency`, which hand-writes both halves because
/// its derived form would put the minor-unit exponent on the wire (§1.1): here
/// the derived form is exactly right, so the derive stays.
///
/// There is deliberately no `From<Uuid>`. Turning an untyped id into a typed one
/// is the moment the type system stops helping, so it is spelled `from_uuid` and
/// is visible in review, not reachable through an inferred `.into()`.
macro_rules! typed_id {
    ($(#[$attribute:meta])* $name:ident) => {
        $(#[$attribute])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Adopt a UUID minted elsewhere — the shell's generator, a database
            /// row, a sync payload — as this kind of id.
            pub const fn from_uuid(id: Uuid) -> $name {
                $name(id)
            }

            /// Take the next id from an injected source. The one call site that
            /// mints an id inside pure code, and it mints nothing itself.
            pub fn next_from(source: &impl IdSource) -> $name {
                $name(source.next())
            }

            /// The wrapped UUID, for the boundaries that store or transmit
            /// sixteen bytes rather than a domain type.
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        /// The plain UUID, with no type name and no decoration: what a receipt
        /// footer, a log line and a support ticket all quote.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

typed_id! {
    /// A merchant. The tenant boundary every other row is scoped by.
    OrgId
}
typed_id! {
    /// One sale document, immutable once complete (I-4).
    SaleId
}
typed_id! {
    /// One line of one sale, carrying the price and name captured at the moment
    /// it was scanned (I-5).
    SaleLineId
}
typed_id! {
    /// A catalogue product.
    ProductId
}
typed_id! {
    /// A shop.
    StoreId
}
typed_id! {
    /// One till. Half of the push ordering key `(register_id, sync_outbox.seq)`
    /// (I-7).
    RegisterId
}
typed_id! {
    /// A person who signs in: cashier, shift lead, manager, owner.
    UserId
}
typed_id! {
    /// One cash-accountability period, from open to Z.
    ShiftId
}
typed_id! {
    /// A named customer. Optional on a sale, and never required to sell.
    CustomerId
}
typed_id! {
    /// One payment on one sale — cash, card, wallet, store credit.
    TenderId
}
typed_id! {
    /// A catalogue category. Named here rather than in `catalog.rs` because
    /// `Product.category_id` refers to it before that module exists.
    CategoryId
}
typed_id! {
    /// A tax treatment: standard, reduced, zero-rated, exempt.
    TaxCategoryId
}
typed_id! {
    /// A promotion or campaign.
    PromotionId
}
typed_id! {
    /// One append-only row of the stock ledger, whose sum is on-hand (I-6).
    StockEventId
}
typed_id! {
    /// One manager approval. It names the `ApprovalHandle` a privileged command
    /// consumes in the same transaction as its financial effect and audit row,
    /// and it is typed for the same reason as the rest:
    /// `consume(approval: ApprovalId, sale: SaleId)` cannot be called backwards.
    ApprovalId
}

/// Where a new id comes from, so the domain never generates one.
///
/// Production implements this in the shell over `Uuid::now_v7`, which is why the
/// trait exists at all: UUIDv7's timestamp prefix buys index locality on a
/// register that inserts all day, and reading the clock it needs is exactly what
/// this crate may not do. Tests and the server implement it with
/// [`SeqIdSource`].
///
/// `&self` rather than `&mut self`: a source is shared — one register mints a
/// sale id, several line ids and a tender id inside one transaction — so the
/// counter lives behind interior mutability instead of forcing every holder to
/// own a mutable borrow.
pub trait IdSource {
    /// The next id. Never the same value twice from one source.
    fn next(&self) -> Uuid;
}

/// The 48-bit `unix_ts_ms` field, and the mask that keeps an anchor inside it.
const MILLIS_MASK: u64 = (1 << 48) - 1;
/// `rand_b`'s low 58 bits, which is where the sequence number goes.
const SEQUENCE_MASK: u64 = (1 << 58) - 1;
/// The version nibble, at bits 76–79. `7` is the whole point.
const VERSION_7: u128 = 0x7 << 76;
/// The RFC 9562 variant bits `0b10`, at bits 62–63.
const VARIANT_RFC: u128 = 0b10 << 62;

/// A deterministic [`IdSource`]: v7-**shaped**, counter-driven, reproducible.
///
/// Not behind `#[cfg(test)]`, deliberately. `ref/domain-api.md` §2 puts the
/// deterministic doubles in the crate so the server's own tests and the
/// cross-crate integration tests can construct the same id stream a domain
/// property did; a `#[cfg(test)]` type is invisible to both.
///
/// # The layout, and why "shaped" is the honest word
///
/// Every id it hands out has a real UUIDv7 *layout* — RFC 9562 §5.7: a
/// big-endian 48-bit millisecond field, the version nibble `7`, the variant bits
/// `0b10` — so `Uuid::get_version_num` answers `7`, a database index sees the
/// same key distribution as production, and a sort behaves the same way. What
/// differs is where the bits come from:
///
/// ```text
/// │ 48 bits unix_ts_ms │ 7 │ 12 bits rand_a │ 10 │ 62 bits rand_b │
///   origin + sequence    ▲   stream tag           stream tag ▸ 4
///   (caller's anchor)    │   (low 12 bits)        sequence   ▸ 58
///                        └─ version, literal 7
/// ```
///
/// - **The millisecond field is the caller's anchor plus the sequence number**,
///   not a clock reading. One simulated millisecond per id, so the prefix
///   advances the way a real stream's does and index locality and sort order
///   behave like production — a frozen prefix would hide any ordering defect
///   that only appears once the prefix moves.
/// - **`rand_a` and `rand_b` hold no entropy.** They hold the stream tag and the
///   sequence number. RFC 9562 does permit a counter in `rand_a`, so the shape
///   is conformant; the content is not random and is not meant to be.
///
/// The consequence is worth stating plainly: **these ids are predictable, so a
/// production register must never mint them here.** A test double whose output
/// you can read is the point — `019b76da-a801-7000-8000-000000000001` says
/// "second id, stream 0" at a glance, which a real v7 never does.
///
/// # Reproducibility
///
/// Two sources built with the same `(origin_millis, stream)` produce the same
/// sequence, for as long as they are called the same number of times: the value
/// is a pure function of the construction and the call index. Concurrent callers
/// still get distinct ids — the counter is atomic — but which caller gets which
/// index depends on the interleaving, so the *set* is reproducible and the
/// per-thread assignment is not. Deliberately not `Clone`: two clones would
/// restart the same counter and hand out the same ids.
///
/// What a caller sees — the first two ids of `SeqIdSource::new(1_767_225_600_000, 0)`:
///
/// ```text
/// SaleId::next_from(&ids)  →  019b76da-a800-7000-8000-000000000000
/// SaleId::next_from(&ids)  →  019b76da-a801-7000-8000-000000000001
/// ```
///
/// Those exact values are asserted by `seq_id_source_is_reproducible`, not by a
/// doctest: `just test` runs `cargo nextest`, which does not execute doctests, so
/// an example that only rustdoc checks is an example nothing in this repository
/// runs.
#[derive(Debug)]
pub struct SeqIdSource {
    /// The first id's millisecond field, and the anchor every later one counts
    /// from. Only the low 48 bits reach an id; a wider value is masked, because
    /// refusing one would make `next` fallible for no benefit.
    origin_millis: u64,
    /// Separates two sources anchored at the same millisecond — two registers in
    /// one test, or a register and the server. All sixteen bits reach the id.
    stream: u16,
    /// Ids handed out so far. `Relaxed` is sufficient: the only requirement is
    /// that no two calls observe the same value, which `fetch_add` guarantees on
    /// its own, and no other memory is published through this counter.
    counter: AtomicU64,
}

impl SeqIdSource {
    /// A source anchored at `origin_millis`, tagged `stream`.
    ///
    /// `origin_millis` is a number the caller chose — a fixture's simulated
    /// instant — and never a clock reading, which is what keeps this
    /// constructor usable inside a pure crate.
    pub const fn new(origin_millis: u64, stream: u16) -> SeqIdSource {
        SeqIdSource {
            origin_millis,
            stream,
            counter: AtomicU64::new(0),
        }
    }

    /// How many ids this source has handed out. The next one will carry this
    /// sequence number.
    pub fn issued(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    /// The id for one sequence number. A pure function of the construction and
    /// `sequence`, which is what makes the whole type reproducible.
    fn compose(&self, sequence: u64) -> Uuid {
        let millis = u128::from(self.origin_millis.wrapping_add(sequence) & MILLIS_MASK);
        let rand_a = u128::from(self.stream & 0x0fff);
        let rand_b = (u128::from(self.stream >> 12) << 58) | u128::from(sequence & SEQUENCE_MASK);

        Uuid::from_u128((millis << 80) | VERSION_7 | (rand_a << 64) | VARIANT_RFC | rand_b)
    }
}

impl IdSource for SeqIdSource {
    fn next(&self) -> Uuid {
        self.compose(self.counter.fetch_add(1, Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;
    use std::collections::HashSet;
    use uuid::Variant;

    /// 2026-01-01T00:00:00Z in milliseconds. A fixture instant, written as the
    /// number it is: nothing in this crate may ask what time it is.
    const ORIGIN: u64 = 1_767_225_600_000;

    /// The first three ids of `SeqIdSource::new(ORIGIN, 0)`, as committed
    /// fixtures. They are review tripwires: the layout in `SeqIdSource`'s
    /// documentation is a promise about these exact bytes, so a change to either
    /// has to change this list too.
    const GOLDEN_SEQUENCE: [&str; 3] = [
        "019b76da-a800-7000-8000-000000000000",
        "019b76da-a801-7000-8000-000000000001",
        "019b76da-a802-7000-8000-000000000002",
    ];

    // Covers the whole 64-bit anchor space — including anchors wider than the 48
    // bits an id can hold, which the layout masks — every one of the 65,536
    // stream tags, and runs of 1 to 64 ids. It excludes runs long enough to
    // exhaust the 58-bit sequence field, because reaching one takes 2^58 calls,
    // and it excludes concurrency, which changes which caller sees which index
    // and not which ids exist.
    fn seq_source_cases() -> impl Strategy<Value = (u64, u16, u8)> {
        (any::<u64>(), any::<u16>(), 1u8..=64)
    }

    // The same space, twice, with the two stream tags never equal — the case
    // that matters is two sources anchored at the *same* millisecond, since two
    // different anchors are trivially apart. Equal tags are excluded here
    // because they are the reproducibility case, proved above them.
    fn two_stream_cases() -> impl Strategy<Value = (u64, u16, u16, u8)> {
        (any::<u64>(), any::<u16>(), any::<u16>(), 1u8..=64)
            .prop_filter("two distinct stream tags", |(_, first, second, _)| {
                first != second
            })
    }

    fn take(source: &SeqIdSource, count: u8) -> Vec<Uuid> {
        (0..count).map(|_| source.next()).collect()
    }

    #[test]
    fn seq_id_source_is_reproducible() {
        // The claim: the id stream is a function of the construction and the
        // call index. Two sources built alike agree id for id, so a fixture, a
        // failing property's seed and a support reproduction all describe the
        // same world.
        let first = SeqIdSource::new(ORIGIN, 0);
        let second = SeqIdSource::new(ORIGIN, 0);
        assert_eq!(take(&first, 8), take(&second, 8));
        assert_eq!(first.issued(), 8);
        assert_eq!(second.issued(), 8);

        // And the exact values, so "reproducible" is not merely "these two
        // agree with each other".
        let golden = SeqIdSource::new(ORIGIN, 0);
        let rendered: Vec<String> = take(&golden, 3).iter().map(Uuid::to_string).collect();
        assert_eq!(rendered, GOLDEN_SEQUENCE);

        // A different stream tag at the same anchor is a different world.
        let other_stream = SeqIdSource::new(ORIGIN, 1);
        assert_ne!(
            take(&other_stream, 3),
            take(&SeqIdSource::new(ORIGIN, 0), 3)
        );

        // Interleaving two sources does not disturb either: each carries its own
        // counter, so neither can consume the other's sequence number.
        let left = SeqIdSource::new(ORIGIN, 0);
        let right = SeqIdSource::new(ORIGIN, 0);
        let interleaved = [left.next(), right.next(), left.next(), right.next()];
        assert_eq!(interleaved[0], interleaved[1]);
        assert_eq!(interleaved[2], interleaved[3]);
        assert_ne!(interleaved[0], interleaved[2]);
    }

    #[test]
    fn seq_ids_carry_the_v7_layout() {
        // Version, variant and the millisecond prefix, read back through
        // `uuid`'s own accessors rather than the arithmetic that produced them.
        let ids = SeqIdSource::new(ORIGIN, 0x1234);
        for sequence in 0..4u64 {
            let id = ids.next();
            assert_eq!(id.get_version_num(), 7);
            assert_eq!(id.get_variant(), Variant::RFC4122);

            let millis = u64::try_from(id.as_u128() >> 80).unwrap();
            assert_eq!(
                millis,
                ORIGIN + sequence,
                "one simulated millisecond per id"
            );
        }

        // An anchor wider than the field is masked, not refused: `next` stays
        // total, and the id is still a well-formed v7 shape.
        let wide = SeqIdSource::new((1 << 48) | 5, 0);
        let id = wide.next();
        assert_eq!(id.get_version_num(), 7);
        assert_eq!(id.as_u128() >> 80, 5);
    }

    #[test]
    fn the_stream_tag_and_the_sequence_occupy_their_own_fields() {
        // The readable-id claim from `SeqIdSource`'s documentation: the tag sits
        // in `rand_a` and the sequence number in `rand_b`, so a failing test
        // says which source and which call produced the id it printed.
        assert_eq!(
            SeqIdSource::new(ORIGIN, 0x0fff).next().to_string(),
            "019b76da-a800-7fff-8000-000000000000",
        );
        // The tag's top four bits ride at the head of `rand_b`, so all 65,536
        // tags are distinguishable rather than folded into twelve bits.
        assert_eq!(
            SeqIdSource::new(ORIGIN, 0x1000).next().to_string(),
            "019b76da-a800-7000-8400-000000000000",
        );
        assert_eq!(
            SeqIdSource::new(ORIGIN, 0xffff).next().to_string(),
            "019b76da-a800-7fff-bc00-000000000000",
        );
    }

    #[test]
    fn all_fifteen_typed_ids_round_trip_through_json() {
        // Fifteen, counted rather than trusted: `ref/domain-api.md` §2 records
        // that an earlier revision listed thirteen while the phase file said
        // fourteen, and `OrgId` and `CategoryId` are the two a reader skips
        // because they are used before they are declared.
        let uuid = SeqIdSource::new(ORIGIN, 7).next();
        let expected = format!("\"{uuid}\"");

        macro_rules! round_trip {
            ($($name:ident),+ $(,)?) => {{
                let mut counted = 0usize;
                $(
                    let id = $name::from_uuid(uuid);
                    assert_eq!(id.as_uuid(), uuid);
                    let json = serde_json::to_string(&id).unwrap();
                    assert_eq!(json, expected, concat!(stringify!($name), " wire form"));
                    assert_eq!(serde_json::from_str::<$name>(&json).unwrap(), id);
                    counted += 1;
                )+
                counted
            }};
        }

        let counted = round_trip!(
            OrgId,
            SaleId,
            SaleLineId,
            ProductId,
            StoreId,
            RegisterId,
            UserId,
            ShiftId,
            CustomerId,
            TenderId,
            CategoryId,
            TaxCategoryId,
            PromotionId,
            StockEventId,
            ApprovalId,
        );
        assert_eq!(counted, 15, "every id in ref/domain-api.md §2, and no more");
    }

    #[test]
    fn a_typed_id_displays_as_the_plain_uuid() {
        // No type name, no braces: the string a cashier reads off a receipt
        // footer to a support line must match the string in the database.
        let uuid = SeqIdSource::new(ORIGIN, 0).next();
        let sale = SaleId::from_uuid(uuid);
        assert_eq!(sale.to_string(), uuid.to_string());
        assert_eq!(sale.to_string(), GOLDEN_SEQUENCE[0]);
        assert_eq!(format!("{sale}"), GOLDEN_SEQUENCE[0]);
        // `Debug` keeps the type name, which is what makes a failing assertion
        // readable; `Display` must not.
        assert_eq!(format!("{sale:?}"), format!("SaleId({uuid:?})"));
    }

    #[test]
    fn a_typed_id_costs_nothing_over_its_uuid() {
        assert_eq!(size_of::<SaleId>(), size_of::<Uuid>());
        assert_eq!(align_of::<SaleId>(), align_of::<Uuid>());
        // `Option<SaleId>` is one byte wider because a `Uuid` has no niche. Said
        // out loud so nobody discovers it while sizing a cache row.
        assert!(size_of::<Option<SaleId>>() > size_of::<SaleId>());
    }

    #[test]
    fn typed_ids_order_by_their_bytes_and_never_by_causality() {
        // `Ord` exists for `BTreeMap` keys and stable sorting. It orders the
        // sixteen bytes, so a v7 id sorts near-chronologically — and I-7 says
        // that resemblance is never the authority. This test pins the mechanical
        // claim; the causal claim belongs to owned sequences.
        let ids = SeqIdSource::new(ORIGIN, 0);
        let first = SaleId::next_from(&ids);
        let second = SaleId::next_from(&ids);
        assert!(first < second);

        // Two sources whose anchors run backwards: the later *call* produces the
        // smaller id, which is exactly why sort order is not evidence of order
        // of events.
        let earlier_anchor = SeqIdSource::new(ORIGIN - 1_000, 0);
        let issued_second = SaleId::next_from(&earlier_anchor);
        assert!(issued_second < second);
    }

    proptest! {
        // One shared configuration for every property in this crate: 4,096 cases,
        // the repository's recorded seed, and a minimized failing case persisted
        // under crates/pos-domain/proptest-regressions/ids.txt to be committed.
        // `PROPTEST_CASES` raises the count and can never lower it, which is what
        // makes the scheduled PROPTEST_CASES=100000 lane mean what it says.
        // Owned by microstep 1.1.0; conventions §5.1 is the rule.
        #![proptest_config(domain_proptest_config())]

        /// Two sources constructed alike hand out the same ids, in the same
        /// order, whatever the anchor and the tag.
        #[test]
        fn prop_seq_id_sources_agree_when_constructed_alike(
            (origin, stream, count) in seq_source_cases()
        ) {
            let first = SeqIdSource::new(origin, stream);
            let second = SeqIdSource::new(origin, stream);
            prop_assert_eq!(take(&first, count), take(&second, count));
            prop_assert_eq!(first.issued(), u64::from(count));
        }

        /// One source never repeats an id, and two sources tagged differently at
        /// the same anchor never produce one in common.
        #[test]
        fn prop_seq_ids_never_collide(
            (origin, first_stream, second_stream, count) in two_stream_cases()
        ) {
            let mine = take(&SeqIdSource::new(origin, first_stream), count);
            let theirs = take(&SeqIdSource::new(origin, second_stream), count);

            let unique: HashSet<&Uuid> = mine.iter().collect();
            prop_assert_eq!(unique.len(), mine.len(), "a source repeated an id");

            let overlap: HashSet<&Uuid> = unique
                .intersection(&theirs.iter().collect())
                .copied()
                .collect();
            prop_assert!(overlap.is_empty(), "two streams shared {:?}", overlap);
        }

        /// Every id keeps the v7 layout: version 7, the RFC variant bits, and a
        /// millisecond field that is the anchor plus the call index, masked to
        /// the 48 bits the field holds.
        #[test]
        fn prop_seq_ids_keep_the_v7_layout(
            (origin, stream, count) in seq_source_cases()
        ) {
            let source = SeqIdSource::new(origin, stream);
            for sequence in 0..u64::from(count) {
                let id = source.next();
                prop_assert_eq!(id.get_version_num(), 7);
                prop_assert_eq!(id.get_variant(), Variant::RFC4122);
                prop_assert_eq!(
                    id.as_u128() >> 80,
                    u128::from(origin.wrapping_add(sequence) & MILLIS_MASK)
                );
            }
        }
    }
}
