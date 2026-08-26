# `pos-domain` — the complete public API

The crown jewel. Pure, no I/O, shared by register and server so the two can never disagree about what a total is.

This document is the **target shape**, assembled across Phases 1–4. Each item is annotated with the microstep that creates it. Signatures here are normative: if a phase file shows something different, this file wins.

**Purity is enforced mechanically.** `pos-domain/Cargo.toml` may depend only on `serde`,
`thiserror`, `uuid`, `rust_decimal`, and (dev) `proptest`, `criterion`, `trybuild`,
`serde_json`, `pos-test-support`. `uuid` is an identity/serialization type here: its default and
version-generation features stay disabled, and generated IDs are injected by the shell.
Adding anything capable of I/O, clock access, or randomness is a design review, not a commit.
`scripts/check-domain-purity.py` audits the resolved normal dependency features and direct
call sites.

> `trybuild`, `serde_json` and `pos-test-support` are dev-dependencies and ship nothing, which is
> why they are allowed: purity governs what reaches a register, and a `[dev-dependencies]` entry
> never does.
>
> `trybuild` is here because 1.1.8's claim — that two id types cannot be interchanged — is only
> provable by code that **fails** to compile, and `cargo nextest` does not run doctests, so a
> ` ```compile_fail ` block would be invisible to both `just test` and CI. `trybuild` runs as an
> ordinary integration test.
>
> `serde_json` is here because `Currency` hand-writes `Serialize`/`Deserialize` to keep the
> minor-unit exponent off the wire (§1.1), and an encoding cannot be tested without an encoder.
>
> `pos-test-support` is here because [`01-conventions.md`](../01-conventions.md) §5.1 requires
> every property in this crate to take its case count, seed and regression-persistence policy from
> one shared helper (microstep 1.1.0). That helper reads `PROPTEST_CASES`, which this crate may not
> do — so it lives in its own crate on the dev side of the boundary rather than in a module here.
> It never depends on `pos-domain`, so a property can never be checked against a value the harness
> produced.

```
crates/pos-domain/src/
├── lib.rs           module tree + re-exports
├── money.rs         Money, Currency, Qty, Percent      [exists, extended 1.1.x]
├── ids.rs           typed ids, the IdSource port       [1.1.8]
├── time.rs          BusinessDate, day cutover, ClockState, the Clock port [1.1.9]
├── catalog.rs       Product, Barcode, PriceSource, ScanLookup [1.2.x]
├── tax.rs           categories, rates, extraction      [1.3.x]
├── cart.rs          the state machine, in-flight state [1.4.x]
├── pricing.rs       discounts, overrides, proration    [1.4.x]
├── tender.rs        tenders, change, cash rounding     [1.5.x]
├── receipt.rs       ReceiptModel (render input)        [1.7.x]
├── permissions.rs   capabilities, Authorized<C>, ApprovalHandle [1.6.x]
├── audit.rs         hash-chain construction            [1.6.x]
├── refund.rs        refundable balances, credit docs   [2.3.x]
├── shift.rs         shift lifecycle, over/short, Z     [2.4.x]
├── stock.rs         ledger kinds, on-hand, WAC         [1.10.x, 4.2.x]
├── loyalty.rs       points ledger                      [3.4.x]
└── promo.rs         promotion engine                   [4.4.x]
```

---

## 1 · `money.rs` — Money, Currency, Qty, Percent

Today `Money` is a bare `i64` with no currency. That makes JOD's three-decimal minor unit (master plan B.5) unrepresentable and lets a USD amount be added to a JOD amount silently. Fixed first, before anything depends on it (**G-11**).

### 1.1 `Currency` — [1.1.1]

```rust
/// ISO 4217 code plus its minor-unit exponent. The exponent is DATA:
/// JOD = 3 (1 dinar = 1000 fils), USD/EUR = 2, JPY = 0.
///
/// `Serialize`/`Deserialize` are NOT in this derive. Deriving them alongside
/// the hand-written impls below is `error[E0119]: conflicting implementations`,
/// so a reader who copies the derive gets a crate that does not compile — and a
/// reader who deletes the impls instead gets the wrong wire format. One of the
/// two had to go, and it is the derive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Currency {
    code: [u8; 3],
    exponent: u8,
}

impl Currency {
    pub const JOD: Currency = Currency { code: *b"JOD", exponent: 3 };
    pub const USD: Currency = Currency { code: *b"USD", exponent: 2 };

    pub fn from_code(code: &str) -> Result<Currency, MoneyError>;
    pub fn code(self) -> &'static str;      // interned; no allocation
    pub fn exponent(self) -> u8;
    pub fn minor_per_major(self) -> i64;    // 10^exponent
}
```

`Currency` is `Copy` and three bytes plus one. It rides on every `Money`, so it must stay small.

`code()` returns `&'static str` with no allocation because the known currencies are a private `const`
table and `code()` returns a slice of it. `from_code` is the only constructor for an arbitrary code
and it rejects anything not in that table, so the table is total: there is no `Currency` whose code
is unknown.

**`Serialize`/`Deserialize` are implemented by hand, as the ISO string `"JOD"`** — not derived. The
derive over private fields would emit `{"code":[74,79,68],"exponent":3}`, which is unreadable over
IPC and, worse, puts the exponent on the wire as data a client could disagree with. Deserialisation
goes through `from_code`, so an unknown currency is a parse error rather than a struct with a
plausible-looking exponent. `Money` therefore serialises as `{"minor":1250,"currency":"JOD"}`.

```rust
impl serde::Serialize for Currency {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

impl<'de> serde::Deserialize<'de> for Currency {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Currency, D::Error> {
        // `Cow` rather than `&str`: a JSON string containing an escape cannot be
        // borrowed, and `&str` would fail on it at runtime for no reason.
        let code = <std::borrow::Cow<'_, str>>::deserialize(d)?;
        Currency::from_code(&code).map_err(serde::de::Error::custom)
    }
}
```

Tests — [1.1.1]: `currency_serialises_as_its_iso_code` · `unknown_currency_code_is_a_deserialisation_error` ·
`the_exponent_never_appears_on_the_wire` · `golden_money_json_is_stable` (a committed fixture, so a
future refactor that reinstates the derive fails on the golden rather than on a merchant's receipt).

### 1.2 `Money` — extended [1.1.2]

Existing methods (`from_minor`, `minor`, `checked_add`, `checked_sub`, `split_evenly`) keep their behaviour but gain currency. **The `ZERO` associated constant does not survive.** There is no currency to give it, and inventing a default currency to keep it is exactly the silent coercion this type exists to prevent, so `zero(currency)` replaces it; microstep 1.1.2a removed the constant. `split_evenly`'s largest-remainder implementation and its property test are already correct — do not rewrite them, only thread `Currency` through.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money { minor: i64, currency: Currency }

impl Money {
    pub const fn from_minor(minor: i64, currency: Currency) -> Self;
    pub fn zero(currency: Currency) -> Self;
    pub const fn minor(self) -> i64;
    pub const fn currency(self) -> Currency;
    pub const fn is_zero(self) -> bool;
    pub const fn is_negative(self) -> bool;

    // Currency-checked arithmetic. Mismatch is an error, never a panic,
    // never a silent coercion.
    pub fn checked_add(self, o: Money) -> Result<Money, MoneyError>;
    pub fn checked_sub(self, o: Money) -> Result<Money, MoneyError>;
    pub fn checked_neg(self) -> Result<Money, MoneyError>;
    pub fn sum<I: IntoIterator<Item = Money>>(iter: I, c: Currency) -> Result<Money, MoneyError>;

    /// Ordering is CHECKED, because `PartialOrd`/`Ord` are deliberately NOT
    /// derived: over `(minor, currency)` a derive would rank a JOD amount
    /// against a USD one and answer confidently. Mixed currencies are an error
    /// here too, so `<` and `sort` on a Money do not compile at all.
    pub fn checked_cmp(self, o: Money) -> Result<core::cmp::Ordering, MoneyError>;

    /// Multiply by a quantity in milli-units, rounding ONCE by `rule`.
    /// This is the only path from (unit price × qty) to a line amount.
    pub fn mul_qty(self, qty: Qty, rule: RoundingRule) -> Result<Money, MoneyError>;

    /// Apply a percentage, rounding ONCE by `rule`.
    pub fn mul_percent(self, pct: Percent, rule: RoundingRule) -> Result<Money, MoneyError>;

    /// Largest-remainder split. Sum of parts == self, exactly. Parts differ
    /// by at most one minor unit. The primitive under split tender AND under
    /// basket-discount proration (master plan B.2, C.9).
    pub fn split_evenly(self, parts: u32) -> Result<Vec<Money>, MoneyError>;

    /// Largest-remainder split PROPORTIONAL to `weights`. Sum == self, exactly.
    /// This is what prorates a basket discount across lines by line value.
    pub fn split_proportional(self, weights: &[Money]) -> Result<Vec<Money>, MoneyError>;

    /// The same algorithm over arbitrary non-negative integer weights.
    /// `split_proportional` delegates here with each weight's `minor()`.
    /// Proration weights by line VALUE; a partial refund weights by
    /// QUANTITY (§10). Writing largest-remainder twice is how the two come to
    /// disagree by a fil on the one document where they must not — a credit
    /// note against its invoice.
    pub fn split_proportional_by(self, weights: &[i64]) -> Result<Vec<Money>, MoneyError>;

    /// Round to a coin step (cash rounding, master plan B.5).
    /// step_minor = 10 for the 1-qirsh default.
    pub fn round_to_step(self, step_minor: i64, dir: RoundingDirection) -> Result<Money, MoneyError>;

    /// Exact conversion for intermediate math. NEVER for storage or display.
    pub fn to_decimal(self) -> Decimal;
    pub fn from_decimal(d: Decimal, c: Currency, rule: RoundingRule) -> Result<Money, MoneyError>;

    /// SHELF AND CATALOGUE display only, at the store's `money_decimals` (B.5).
    /// Fails when the amount is not exactly representable at that precision.
    /// `format(2)` on a 3-exponent currency is not cosmetic: the card is charged
    /// 1.259 and the screen says 1.25, and a 3-fil cash-rounding line renders as
    /// `0.00`. A shorter display is only honest when nothing is hidden by it.
    pub fn format(self, decimals: u8) -> Result<String, MoneyError>;

    /// The currency's own exponent, always, with no setting in the path.
    /// Every amount the customer pays or the books carry renders through this:
    /// line totals, tax, the grand total, tenders, change, the cash-rounding
    /// adjustment, refunds, and every value in a fiscal document.
    pub fn format_exact(self) -> String;

    pub fn parse(s: &str, c: Currency) -> Result<Money, MoneyError>;
}
```

Both enums, and the one rounding primitive they exist to parameterise, arrive at **[1.1.6]** —
before the arithmetic above, because rounding is an argument to it.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoundingRule { HalfAwayFromZero, HalfEven, Floor, Ceil }

impl RoundingRule {
    /// THE rounding point (I-1): one exact `Decimal` in, one whole `i64` out.
    ///
    /// `mul_qty`, `mul_percent` and `from_decimal` all end by reducing a
    /// `rust_decimal` intermediate to integer units, so that reduction exists
    /// exactly once. "Rounds once" is only a claim about the system if there is
    /// one place to round in; four callers each spelling their own conversion is
    /// how two of them come to disagree by a fil.
    ///
    /// The caller passes the value already in the units it wants back — minor
    /// units for money, milli-units for a quantity. That is why the name says
    /// `i64` and not `minor`: the primitive carries no currency and must not
    /// imply one. Out of `i64` range is `MoneyError::Overflow`, never a panic
    /// and never a saturating cast.
    pub fn round_to_i64(self, value: Decimal) -> Result<i64, MoneyError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoundingDirection { Nearest, Up, Down }
```

`RoundingRule` maps onto `rust_decimal::RoundingStrategy` — `MidpointAwayFromZero`,
`MidpointNearestEven`, `ToNegativeInfinity`, `ToPositiveInfinity`, in declaration order — rather
than hand-rolling four roundings, so the mapping is the only thing in `round_to_i64` that can be
wrong, and a wrong one misprices every line in the system.

> **The derive is the IPC spelling, not the storage spelling — and that is a decision, not an
> oversight.** A bare `Serialize` emits `"HalfAwayFromZero"` and `"Nearest"`, while
> [`schema.md`](schema.md) §0003 persists the same values as
> `CHECK (rounding_rule IN ('half_away_from_zero','half_even','floor','ceil'))` and
> `cash_round_direction IN ('nearest','up','down')`. The two do not match, on purpose: `pos-domain`
> is pure and never touches SQL, so serde here describes the IPC wire and nothing else. **The
> `pos-db` layer converts to the schema's snake_case explicitly**, as a visible mapping with its own
> test, rather than by adding `#[serde(rename_all = "snake_case")]` and making a derive silently
> responsible for a `CHECK` constraint. No domain enum in this document carries `rename_all`
> — `BarcodeKind`, `ApprovalRequirement` and `ScanLookup` included — and this is the convention for
> every persisted enum that follows: **one explicit mapping per enum in `pos-db`, never a derive
> attribute that couples the JSON wire format to a column constraint.** Whoever writes the first such
> mapping (group 1.3) owns establishing the pattern.

**There is no `Default` impl, deliberately.** `HalfAwayFromZero` is the jurisdiction default, not a
type-level fallback: `unwrap_or_default()` is precisely how an unapproved tax rule would reach a
real sale, and microstep `1.3.4` exists to block a finalization that has no approved policy behind
it. The rule is threaded from the policy or it does not arrive.

**`Money::round_to_step` lands in [1.1.2b].** It is the primitive on the *cash* axis, with mechanics
defined by numeric order: `Up` means toward positive infinity and `Down` toward negative infinity,
so below zero `Up` moves toward zero and `Down` away from it. `Nearest` chooses the closer multiple
and breaks an exact half-step tie away from zero. Microstep [1.5.3] owns `compute_cash_rounding`,
the policy that applies this primitive only to a final cash tender's remaining amount, using the
selected direction. The direction for a cash refund payout remains the separate open question in
[`tax-jordan.md`](tax-jordan.md) §5; defining honest primitive mechanics here does not decide it.

> **Default is `HalfAwayFromZero`, not banker's rounding.** The blueprint suggests banker's; the
> arithmetic a merchant's accountant does by hand expects half-away-from-zero, and the default a
> merchant never changes must be the one that matches their hand-check.
>
> **The tie rule is not a per-store preference.** It changes tax facts, not presentation: a 13-fil
> 4%-inclusive line has an exact net of 12.5 fils, so half-away records net 13 and tax 0 while
> half-even records net 12 and tax 1. Two stores in one org, or a register and the server, computing
> the same sale differently is a filing error with no diagnosis. `RoundingRule` therefore belongs to a
> **versioned jurisdiction policy**, pinned per store, and once fiscal issuance is enabled for a store
> it may not change without a new policy version and a recorded reason — a mid-year change would make
> the merchant's own filing history internally inconsistent. `store.rounding_rule` names the policy;
> it does not offer four options to a settings screen.

> ⚠️ **OPEN — blocks 2.7.0.** Which tie rule does ISTD's own validator apply when it recomputes a
> line, and is a merchant free to choose? Default until answered: `HalfAwayFromZero`, pinned as
> jurisdiction policy version 1 for Jordan, with the 12.5-fil boundary as a committed fixture.
> Owner: 2.7.0. Source that settles it: the official ISTD Technical Integration Guide and business
> rules obtained and pinned at 2.7.0, plus a credentialed boundary submission.

### 1.3 `Qty` — [1.1.3]

```rust
/// Quantity in milli-units (3 dp). 1 unit = 1000. 0.347 kg = 347.
/// One representation for discrete and weighed goods so arithmetic
/// never branches on which kind it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Qty(i64);

impl Qty {
    pub const ZERO: Qty;
    pub const ONE: Qty;                      // 1000
    pub const fn from_milli(m: i64) -> Qty;
    pub const fn milli(self) -> i64;
    pub fn from_units(units: i64) -> Result<Qty, MoneyError>;
    pub fn checked_add(self, o: Qty) -> Result<Qty, MoneyError>;
    pub fn checked_sub(self, o: Qty) -> Result<Qty, MoneyError>;
    pub fn is_whole_units(self) -> bool;     // milli % 1000 == 0
    pub fn to_decimal(self) -> Decimal;
    pub fn format(self, weighed: bool) -> String;  // "2" vs "0.347"
}
```

### 1.4 `Percent` — [1.1.4]

```rust
/// A rate in parts-per-million. 16% = 160_000. 0.5% = 5_000.
/// Used for tax rates, discount percentages, and margin floors.
/// See conventions §2 for why ppm and not basis points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Percent(i64);

impl Percent {
    pub const ZERO: Percent;
    pub const fn from_ppm(ppm: i64) -> Percent;
    pub const fn ppm(self) -> i64;

    /// Reads a PERCENTAGE, not a fraction: `16` is 16% and `0.5` is 0.5%.
    /// `to_percent_decimal` is the exact inverse; `to_decimal` is not.
    pub fn from_percent_decimal(percent: Decimal) -> Result<Percent, MoneyError>;

    /// The FRACTION the arithmetic multiplies by:  160_000 -> 0.16 (scale 6)
    pub fn to_decimal(self) -> Decimal;

    /// The PERCENTAGE a human reads and types:     160_000 -> 16   (scale 4)
    pub fn to_percent_decimal(self) -> Decimal;

    pub fn format(self) -> String;           // "16%", "0.5%", "0.0001%"
}
```

**The two decimal projections point in opposite directions, on purpose, and `to_percent_decimal`
is 1.1.4's one addition to this signature list.** `to_decimal` is the fraction because `net × r`
and `gross / (1 + r)` (§5) need `r` and not `100 r`, so collapsing the two would hide a ÷100 at
every tax site. `from_percent_decimal` reads the percentage because that is what a Cabinet decree, a
settings row and a cashier all write — the method name says *percent* decimal and means it. Those
two are therefore **not** inverses, and the round-trip property 1.1.4 owes cannot be stated over
them at all. `to_percent_decimal` is the inverse, and the property is the claim that this pair round
trips over every representable rate, from either end.

**`from_percent_decimal` refuses rather than rounds**, with the two `MoneyError` variants §1.5
already carries. A value finer than one ppm — `0.00001%` — is
`NotRepresentableAtPrecision(value, 4)`: there is no `RoundingRule` argument here, and I-1 puts
rounding only where a rule was passed in, so a silently truncated rate is not on offer. A value
beyond ±`i64` ppm — or one so large that scaling it to ppm overflows `Decimal` — is `OutOfRange`.
Exactness is the test rather than scale, so `16.000000` is accepted as 16%, which is what a JSON
number or a SQL decimal arrives looking like; and because that test runs first, a value that is both
imprecise and out of range reports the precision error. Both answers are a refusal, which is the
part a caller may rely on.

**A negative rate is representable and nothing in this type forbids one.** `from_ppm` is `const`
and infallible, so a refusal here would be a comment rather than a rule; the sign restriction on a
*tax* rate lives where one is built and stored — `CHECK (rate_ppm >= 0)` in
[`schema.md`](schema.md), and §5's rate resolution — and a discount stays a positive rate with the
direction at the call site, which is how `mul_percent` reads. `format` keeps the sign, and the
derived `Ord` is the signed numeric order of the ppm.

`format` trims trailing zeros and changes nothing else: 160_000 ppm carries four zeros the rate does
not have, so a literal render reads `"16.0000%"`, while `0.0001%` — the smallest representable rate
— keeps all four places. A rate display that rounds is a rate display that lies.

### 1.5 `MoneyError` — extended [1.1.2]

```rust
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("arithmetic overflow")]                       Overflow,
    #[error("cannot split into zero parts")]              ZeroParts,
    #[error("negative amount not allowed here")]          Negative,
    #[error("currency mismatch: {0} vs {1}")]             CurrencyMismatch(&'static str, &'static str),
    #[error("unknown currency code {0}")]                 UnknownCurrency(String),
    #[error("cannot parse {0:?} as an amount")]           Parse(String),
    #[error("weights sum to zero; cannot prorate")]       ZeroWeights,
    #[error("negative weight in a proration")]            NegativeWeight,
    #[error("rounding step must be positive, got {0}")]   InvalidStep(i64),
    #[error("{0} is not exact at {1} decimals")]          NotRepresentableAtPrecision(String, u8),
    #[error("value out of representable range")]          OutOfRange,
}
```

The last two variants landed early, at **[1.1.4]**, because `Percent::from_percent_decimal` refuses
both an inexact and an unrepresentable rate and neither is `Overflow`. The heading keeps `[1.1.2]`
because that is where the rest of the enum arrives; `Parse`, `ZeroWeights`, `NegativeWeight` and
`InvalidStep` are still 1.1.2b's.

### 1.6 Properties — [1.1.5]

| Test | Invariant |
|---|---|
| `prop_split_preserves_total` | *(exists)* Σ parts == original; parts differ by ≤ 1 |
| `prop_split_proportional_preserves_total` | Σ parts == original for **any** weights; a zero weight gets zero |
| `prop_split_proportional_by_preserves_total` | the same over integer weights, so the value-weighted and quantity-weighted callers cannot drift |
| `prop_add_sub_roundtrip` | *(exists)* `(a+b)-b == a` |
| `prop_currency_mismatch_never_silently_coerces` | *(exists)* Mixed-currency ops always `Err`, never a wrong number |
| `prop_round_to_step_is_idempotent` | Rounding an already-rounded amount changes nothing |
| `prop_round_to_step_within_half_step` | `\|rounded − original\| ≤ step/2` for `Nearest` |
| `prop_format_exact_parse_roundtrip` | `parse(format_exact(m)) == m` at the currency's own exponent |
| `format_truncating_a_fil_is_refused` | `Money::from_minor(1259, JOD).format(2)` is `Err`, not `"1.25"` |
| `prop_mul_qty_whole_units_is_repeated_add` | `price.mul_qty(n units) == price + … + price` — the sanity check a cashier would do |

---

## 2 · `ids.rs` — typed ids and the `IdSource` port — [1.1.8]

Purity means `pos-domain` cannot call `Uuid::now_v7()` or read a clock. Both are injected.

**There are two ports, and they land one microstep apart.** `IdSource` is 1.1.8's, in `ids.rs`.
`Clock` is **1.1.9's, in `time.rs`** (§3), with `FixedClock`: `Clock::now` returns `Timestamp`,
`Timestamp` is defined by that microstep, and a `Clock` written into `ids.rs` at 1.1.8 does not
compile — the same dependency that made the phase file split 1.1.2 into a and b. This section
described both as 1.1.8's until the ids landed and it read as one step's work; the marker on each
declaration below is now the authority.

```rust
macro_rules! typed_id { … }
// A newtype over Uuid, one per id kind. Derives, in full, and each one load-bearing:
//   Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize
// plus #[serde(transparent)] and a hand-written Display that renders the plain UUID.

typed_id!(OrgId); typed_id!(SaleId); typed_id!(SaleLineId); typed_id!(ProductId);
typed_id!(StoreId); typed_id!(RegisterId); typed_id!(UserId); typed_id!(ShiftId);
typed_id!(CustomerId); typed_id!(TenderId); typed_id!(CategoryId); typed_id!(TaxCategoryId);
typed_id!(PromotionId); typed_id!(StockEventId); typed_id!(ApprovalId);

impl SaleId {                                    // and every other id, from the macro
    pub const fn from_uuid(id: Uuid) -> SaleId;  // adopt a UUID minted elsewhere
    pub fn next_from(source: &impl IdSource) -> SaleId;
    pub const fn as_uuid(self) -> Uuid;          // for the storage and wire boundaries
}

/// Injected so domain functions stay pure and tests stay deterministic.  [1.1.8]
pub trait IdSource { fn next(&self) -> Uuid; }   // UUIDv7 in production, minted by the shell

/// Deterministic test double — lives in the crate, not behind #[cfg(test)],
/// so the server and integration tests can use it too.                    [1.1.8]
pub struct SeqIdSource { … }    // v7-shaped, counter-driven, reproducible
impl SeqIdSource {
    /// `origin_millis` is a caller-supplied anchor, never a clock reading;
    /// `stream` separates two sources anchored at the same millisecond.
    pub const fn new(origin_millis: u64, stream: u16) -> SeqIdSource;
    pub fn issued(&self) -> u64;                 // ids handed out so far
}

// pub trait Clock { fn now(&self) -> Timestamp; }   →  §3, [1.1.9]
// pub struct FixedClock { … }                       →  §3, [1.1.9]
```

**Why typed ids.** `fn refund(sale: SaleId, line: SaleLineId)` cannot be called with the arguments swapped. Over a schema with fifteen id columns, that is worth the boilerplate.

Fifteen types, listed in full above. `OrgId` and `CategoryId` are easy to miss because they are used
before they are declared — `CategoryId` by `Product.category_id` in §4, `OrgId` by `store.org_id` in
schema §0003 — and an earlier revision of this section omitted both while the phase file already said
"fourteen". `ApprovalId` is the fifteenth and names one `ApprovalHandle` (§8), which is the row a
privileged command consumes; it is a typed id and not a bare `Uuid` for the same reason as the rest —
`consume(approval: ApprovalId, sale: SaleId)` cannot be called with its arguments swapped.

**`Ord` is on the derive list, and it is not chronology.** Ids are `BTreeMap` keys and sort keys, and
a report or a proration input has to order the same way on every machine, so the derive stays — but it
orders the UUID's sixteen bytes and nothing else. A UUIDv7 embeds a device timestamp, so that order
*resembles* time and I-7 says the resemblance is never the authority: causal order comes from owned
sequences, the server's `version` and `sync_outbox.seq`. This is the opposite call from `Money`
(§1.2), which lost `Ord` because a derived comparison over two currencies answers *wrongly*; a
derived comparison over two ids answers correctly and can only be *misread*.

**`SeqIdSource` is v7-shaped, which is a narrower claim than v7.** The layout is RFC 9562 §5.7's — a
big-endian 48-bit millisecond field, version nibble `7`, variant bits `0b10`, so `get_version_num()`
answers 7 and an index sees production's key distribution — while the content is a counter:

```text
│ 48 bits unix_ts_ms │ 7 │ 12 bits rand_a │ 10 │ 62 bits rand_b │
  origin + sequence        stream tag           stream tag ▸ 4, sequence ▸ 58
```

The millisecond field is the caller's anchor plus the call index, one simulated millisecond per id, so
the prefix advances the way a real stream's does rather than freezing and hiding an ordering defect.
`rand_a` and `rand_b` carry the stream tag and the sequence number, not entropy — RFC 9562 permits a
counter in `rand_a`, so the shape is conformant and the content is deliberately not random. Two
consequences, both stated rather than discovered: the ids are **predictable**, so nothing that ships
to a register may mint them here; and they are **readable** —
`019b76da-a801-7000-8000-000000000001` is "second id, stream 0" at a glance.

Purity is what forces the construction. `pos-domain` may not add a `uuid` `v1`–`v8`, `rng`,
`fast-rng` or `js` feature and may not call a generating constructor — `scripts/check-domain-purity.py`
refuses both by name, including through an alias — so `SeqIdSource` composes the bytes itself and
hands them to `Uuid::from_u128`. The step adds no `uuid` feature and no runtime dependency at all;
its one manifest addition is the dev-dependency in the allowlist at the top of this file.

Tests — [1.1.8]: `typed_ids_do_not_interconvert` (compile-fail, via trybuild) ·
`seq_id_source_is_reproducible` · `seq_ids_carry_the_v7_layout` ·
`the_stream_tag_and_the_sequence_occupy_their_own_fields` ·
`all_fifteen_typed_ids_round_trip_through_json` · `a_typed_id_displays_as_the_plain_uuid` ·
`a_typed_id_costs_nothing_over_its_uuid` · `typed_ids_order_by_their_bytes_and_never_by_causality` ·
`prop_seq_id_sources_agree_when_constructed_alike` · `prop_seq_ids_never_collide` ·
`prop_seq_ids_keep_the_v7_layout`

---

## 3 · `time.rs` — business date and the `Clock` port — [1.1.9] *(gap G-4)*

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp(i64);      // UTC milliseconds since epoch

/// The second injected port. It lives here, beside the type it returns, rather
/// than in `ids.rs` with `IdSource`: `Timestamp` is this microstep's, so a
/// `Clock` declared at 1.1.8 has no return type. [moved from §2]
pub trait Clock { fn now(&self) -> Timestamp; }   // UTC

/// The deterministic double, for the same reason `SeqIdSource` exists: not
/// behind #[cfg(test)], so the server and integration tests share it.
pub struct FixedClock { … }

impl Timestamp {
    pub const MIN: Timestamp;
    pub const MAX: Timestamp;
    /// The accepted range is the range Jiff can resolve in the shell. The
    /// constructor and Deserialize both validate it, so formatting and zone
    /// conversion stay total.
    pub fn from_epoch_milliseconds(ms: i64) -> Result<Timestamp, TimeError>;
    pub fn epoch_milliseconds(self) -> i64;
    pub fn to_iso8601(self) -> String;                 // 2026-08-20T07:15:22.418Z
    pub fn parse_iso8601(s: &str) -> Result<Timestamp, TimeError>;
}

impl Display for Timestamp { … }                            // canonical UTC form
impl From<Timestamp> for i64 { … }                          // epoch milliseconds

impl FixedClock {
    pub fn new(now: Timestamp) -> FixedClock;
    pub fn set(&mut self, now: Timestamp);
}

/// A store-local trading day. NOT a wall-clock date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BusinessDate { y: i16, m: u8, d: u8 }

impl BusinessDate {
    pub fn new(y: i16, m: u8, d: u8) -> Result<BusinessDate, TimeError>;
    pub fn year(self) -> i16;
    pub fn month(self) -> u8;
    pub fn day(self) -> u8;
    pub fn to_iso(self) -> String;                     // 2026-08-20
    pub fn parse(s: &str) -> Result<BusinessDate, TimeError>;
    pub fn succ(self) -> BusinessDate;
}

#[derive(Debug, Clone, Copy)]
pub struct DayBoundary {
    /// Resolved BY THE SHELL, from the store's IANA zone id, FOR THE INSTANT
    /// being converted. Not a configured constant, and not a configured DST
    /// rule — see the note below.
    utc_offset_minutes: i16,
    cutover_minutes: u16,        // minutes past local midnight; default 240 = 04:00
}

impl DayBoundary {
    pub const DEFAULT_CUTOVER_MINUTES: u16 = 240;
    pub fn new(utc_offset_minutes: i16, cutover_minutes: u16)
        -> Result<DayBoundary, TimeError>;
    pub fn utc_offset_minutes(self) -> i16;
    pub fn cutover_minutes(self) -> u16;
}

/// A shift opened at 00:30 local belongs to YESTERDAY's trading day.
pub fn business_date_of(opened_at: Timestamp, b: DayBoundary) -> BusinessDate;

/// Monotonic guard (E.6): never emit a timestamp before the last one seen.
pub struct MonotonicClock<C: Clock> { … }
impl<C: Clock> MonotonicClock<C> {
    pub fn new(source: C) -> MonotonicClock<C>;
    pub fn with_high_water(source: C, high_water: Timestamp) -> MonotonicClock<C>;
    pub fn source_mut(&mut self) -> &mut C;
    pub fn now(&mut self) -> (Timestamp, Option<ClockAnomaly>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockAnomaly {                                   // → audit entry
    JumpedBack     { by_ms: i64, at: Timestamp },
    JumpedForward  { by_ms: i64, at: Timestamp },         // a typo, or a deliberate skip past a rate change
    MonotonicReset { at: Timestamp },                     // rebooted with no trust anchor to re-anchor to
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TimeError {
    #[error("cannot parse {0:?} as an ISO-8601 UTC timestamp")]  Parse(String),
    #[error("cannot parse {0:?} as a YYYY-MM-DD date")]          ParseDate(String),
    #[error("timestamp {0} is outside the representable range")] OutOfRange(i64),
    #[error("{0}-{1}-{2} is not a real calendar date")]          NotACalendarDate(i16, u8, u8),
    #[error("utc offset {0} minutes is not a real offset")]      BadOffset(i16),
    #[error("cutover {0} minutes is not inside a day")]          BadCutover(u16),
}
```

`TimeError` is exhaustive and carries what a UI needs to say what was wrong. Note `NotACalendarDate`
and `BadOffset`/`BadCutover`: a `BusinessDate` is three integers and a `DayBoundary` is two, so both
are constructible with nonsense unless construction is validated. Their fields are private and their
constructors validate; `BusinessDate` deserialisation also goes through `new` rather than letting a
derive bypass it. 31 February and an offset of 4 000 minutes are the kind of input that arrives from
a corrupted settings row, not from a keyboard. `Timestamp` construction and deserialisation likewise
enforce the range the shell's Jiff instant can represent, keeping its infallible formatter and the
shell conversion on one definition of valid time.

`BusinessDate::succ` is total because the business-date API is used in invariant checks: at the
constructible `i16::MAX-12-31` ceiling, which no bounded `Timestamp` can reach, it saturates at that
date rather than wrapping or panicking. `DayBoundary::new` accepts Jiff's complete whole-minute
offset range; historical second-granularity offsets are rejected by the shell before construction.

### 3.1 The store keeps a zone, not an offset — [1.1.9]

`pos-domain` receives a *resolved offset* and stays pure. That half of the design is right and must
not be undone. What changes is where the offset comes from.

**The store stores an IANA zone identifier** (`Asia/Amman`), and the shell resolves it **for the
instant being converted**, with `jiff`. It does not store an offset, and it does not store a DST rule.

A configured offset-plus-DST-rule pair looks equivalent and is not. Jordan abolished seasonal time in
October 2022 and has been UTC+3 year-round since; a register configured with the old rule shifts
`business_date` by an hour every winter, which moves the 04:00 cutover to 03:00 and puts a scripted
day's last sales on the wrong trading day — on `sale` rows that are immutable by I-4. Storing the rule
also means every future government decision to change it is a release. Storing the zone means it is
a `tzdata` update, which the shell already ships.

The offset is resolved per instant, not per store, so a shift that spans a transition in some future
zone still converts each timestamp with the offset in force at that timestamp. The terminal enables
Jiff's bundled IANA database and addresses that database explicitly, so registers on different host
operating systems use the shipped rules rather than whichever tzdata revision the host happens to
carry. Jiff offsets have second precision; an historical offset that is not an exact minute is a
named shell error, never silently truncated into the `i16` minute value.

Tests: `business_date_uses_the_offset_in_force_at_the_instant` · `a_january_sale_and_a_july_sale_agree_in_asia_amman` ·
`resolving_an_unknown_zone_id_is_a_named_error_not_a_default_offset`.

### 3.2 `ClockState` — what the register actually knows — [1.1.9]

"Device time is recorded for humans to read, never branched on" is false, and stating it prevents the
guard that is actually needed. Business date branches on time. Tax-rate effective dates branch on
time. Shift boundaries and fiscal issue dates branch on time. UUIDv7 *embeds* a device timestamp, so
even identity inherits the clock's errors. The honest position is that the register branches on time
constantly and must know how much to trust it.

```rust
/// The register's own assessment of its clock. Persisted, so it survives a
/// restart — a reboot is exactly when a wrong clock arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockState {
    /// The last instant a server authenticated its clock to this register:
    /// a sync response [3.x], or provisioning. `None` before first contact.
    pub last_trusted_at: Option<Timestamp>,
    /// The device clock's own reading at that same instant, retained for
    /// initial-skew audit and to make a partial anchor detectable.
    pub device_at_trust: Option<Timestamp>,
    /// The boot-monotonic counter reading captured at the same trusted instant.
    /// `monotonic_now_ms - monotonic_since_trust_ms` is elapsed trusted time;
    /// a smaller current reading proves that boot continuity was lost. A
    /// cashier can set the wall clock; they cannot set this counter.
    pub monotonic_since_trust_ms: Option<i64>,
    /// The largest timestamp this register has ever issued (E.6).
    pub high_water: Timestamp,
    pub anomaly: Option<ClockAnomaly>,
}

impl ClockState {
    /// Called when the shell observes that the boot identity changed.
    pub fn note_monotonic_reset(&mut self, at: Timestamp);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockConfidence {
    /// Device time agrees with monotonic-projected trusted time inside tolerance.
    Trusted,
    /// Trusted once, projected forward past `max_trust_age_ms`, still consistent.
    Stale   { age_ms: i64 },
    /// Device time and projected time disagree beyond tolerance, or the
    /// monotonic counter reset with no anchor to re-establish from.
    Suspect { skew_ms: i64 },
    /// Never contacted a server. A register provisioned offline this morning.
    Untrusted,
}

impl ClockConfidence {
    pub fn permits_sale(self) -> bool;                 // true for every variant
    pub fn permits_shift_open(self) -> bool;            // true for every variant
    pub fn requires_business_date_confirmation(self) -> bool;
    pub fn raises_time_alarm(self) -> bool;
    /// The current 2.7.0-owned default, not a settled fiscal claim.
    pub fn defers_fiscal_issue_date(self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockPolicy {
    pub tolerance_ms: i64,      // default 120_000 — two minutes
    pub max_trust_age_ms: i64,  // default 604_800_000 — seven days
}

pub fn clock_confidence(state: &ClockState, device_now: Timestamp,
                        monotonic_now_ms: i64, policy: &ClockPolicy) -> ClockConfidence;

/// The timestamp everything else should use: monotonic-projected trusted time
/// where one exists, device time otherwise, never below `high_water`.
pub fn effective_now(state: &ClockState, device_now: Timestamp,
                     monotonic_now_ms: i64) -> Timestamp;
```

The three trust-anchor readings are captured together. With a continuous boot counter, elapsed time
is `monotonic_now_ms - monotonic_since_trust_ms`, and authenticated UTC projects from
`last_trusted_at`. Confidence compares the supplied device reading with that authenticated
projection. `device_at_trust` preserves the wall-clock reading that accompanied the authentication
for audit and makes a partial anchor detectable; it is not a substitute UTC baseline that could make
an already-wrong device appear trusted.

A numeric monotonic counter alone cannot identify its boot: after a reboot, a new counter can
eventually exceed an old anchor. The shell must therefore call `note_monotonic_reset` when its boot
identity changes; the deferred 1.9.1 integration persists and compares the shell-owned continuity
token beside this domain value. The transition clears the monotonic anchor and retains
`MonotonicReset` for audit.
A current reading below the captured counter is also a reset. Either reset, a missing component of a
previously established anchor, or another retained anomaly is `Suspect`. With no authenticated
instant at all the result is `Untrusted`. A consistent projection older than
`max_trust_age_ms` is `Stale`; skew outside `tolerance_ms` takes precedence and is `Suspect`.
`effective_now` uses the trusted projection whenever boot continuity exists, otherwise device time,
and clamps either candidate at `high_water`. That high water is an E.6 timestamp guard, never a
substitute for the owned sequences required by I-7.

**What each confidence permits.** The first column is the one that matters: a register with a wrong
clock is still a register, and a shop with a queue does not care what the clock thinks.

| Confidence | Sell | Open a shift | Resolve a tax rate | Stamp a fiscal issue date |
|---|---|---|---|---|
| `Trusted` | ✓ | ✓ | at `effective_now` | at sale time |
| `Stale` | ✓ | ✓ | at `effective_now`; alarm | at sale time; alarm |
| `Suspect` | ✓ | ✓, business date **confirmed by the operator**, audited | at `effective_now`; alarm | **deferred** — wait for a new authenticated time anchor |
| `Untrusted` | ✓ | ✓, business date **confirmed by the operator**, audited | at `effective_now`; alarm | **deferred** — wait for the first authenticated time anchor |

Deferring the fiscal issue date rather than blocking the sale is the same trade the ICV allocation
makes ([`fiscal-jofotara.md`](fiscal-jofotara.md) §5, and the errata in
[`00-master-plan.md`](../00-master-plan.md) §4a): clearance waits, selling never does. A fiscal
document carrying a date the register invented while its clock was wrong is worse than one that
clears an hour later with the right date.

> ⚠️ **OPEN — blocks 2.7.0.** May a fiscal document's issue date differ from the sale date when a
> `Suspect` or `Untrusted` register clears later, and which source may establish that store-local
> date? Default until answered: complete the sale with `issue_date IS NULL`; reaching the clearance
> endpoint alone does not authenticate time, and ICV allocation plus payload freeze wait for a new
> authenticated time anchor. A never-synchronised register remains visibly queued rather than
> inventing a date. `Trusted`/`Stale` registers stamp at sale time as tabled above.
> Owner: 2.7.0. Source that settles it: the pinned official ISTD Technical Integration Guide and
> outage procedure, or a written ruling from the ISTD E-Invoicing Directorate.

Tests: `reaching_the_clearance_endpoint_does_not_make_device_time_trusted` ·
`a_never_synchronised_register_keeps_issue_date_null_and_sale_complete`.

Properties: `prop_business_date_stable_across_shift`, `prop_cutover_boundary_never_skips_a_day`,
`prop_monotonic_clock_never_decreases`, `prop_effective_now_never_precedes_high_water`,
`prop_clock_confidence_is_monotone_in_skew`.
Tests: `a_never_synced_register_is_untrusted_not_trusted` · `wall_clock_moved_forward_a_year_is_suspect` ·
`a_reboot_without_an_anchor_is_a_monotonic_reset` · `clock_state_survives_restart` ·
`no_clock_confidence_refuses_a_sale` — the one that would be quietly deleted the first time a
register's clock drifted, so it is named and kept.

---

## 4 · `catalog.rs` — products and lookup — [1.2.x]

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Product {
    pub id: ProductId,
    pub sku: String,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub category_id: Option<CategoryId>,
    pub tax_category_id: TaxCategoryId,
    pub unit: UnitOfMeasure,
    /// THE price. It lives here so `PriceSource::from_catalog` can take no
    /// amount at all — see §4.2. An earlier revision left `Product` priceless,
    /// which is why the only way to price a line was for the caller to supply
    /// a number.
    pub unit_price: Money,
    pub is_weighed: bool,
    pub is_service: bool,          // e-recharge, fees — no stock events (J.1)
    pub is_active: bool,
    pub min_age: Option<u8>,       // age-restricted (J.1, E.69)
    pub max_price: Option<Money>,  // ministry price ceiling (J.3, E.71)
    pub reorder_point: Option<Qty>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitOfMeasure { Each, Kilogram, Gram, Litre, Millilitre, Metre, Package }

impl UnitOfMeasure {
    /// `Each` and `Package` are counted, not measured. Selling 0.001 of a can
    /// is not a smaller sale; it is a mispriced one (§6.3).
    pub const fn is_divisible(self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BarcodeKind { Ean13, Ean8, Upca, Code128, Internal, PriceEmbedded, WeightEmbedded }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Barcode {
    pub product_id: ProductId,
    pub code: String,
    pub kind: BarcodeKind,
    pub is_primary: bool,
    /// How many units this code means. `Qty::ONE` for a single unit, `6000` for
    /// a six-pack outer. Both the master plan and the schema name multipacks as
    /// the reason a product carries several codes, and neither carried the
    /// multiplier: a case of cola scanned on its outer barcode charged one can's
    /// price and decremented one unit. The default is one unit, so every code
    /// configured before this existed behaves exactly as it did.
    pub pack_qty: Qty,
}
```

### 4.1 Price-embedded barcode parsing — [1.2.4]

Deli scales print EAN-13 codes whose prefix means "the digits that follow are a weight" or "…a price". Getting this wrong charges the wrong amount, so it is pure, table-driven, and property-tested.

```rust
/// A store-configured rule, e.g. prefix "2", 5 digits of item code,
/// 5 digits of value, value is weight in grams.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedBarcodeRule {
    pub prefix: String,
    pub item_code_span: (usize, usize),
    pub value_span: (usize, usize),
    pub value_kind: EmbeddedValue,
    pub value_scale: u32,          // divisor applied to the raw digits
    pub verify_checksum: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddedValue { WeightMilli, PriceMinor }

/// Opaque evidence returned only by the pure parser. Dependent crates can read
/// the extracted lookup key but cannot construct or alter the private parsed
/// kind, value, matched rule or raw-code evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedScan {
    item_code: String,
    raw_code: String,
    kind: ParsedScanKind,
}

#[derive(Debug, Clone, PartialEq)]
enum ParsedScanKind {
    Plain,
    Weight { qty_milli: i64, rule: EmbeddedBarcodeRule },
    Price { price_minor: i64, rule: EmbeddedBarcodeRule },
}

pub fn parse_scan_bytes(input: &[u8], rules: &[EmbeddedBarcodeRule])
    -> Result<ParsedScan, ScanError>;
pub fn parse_scan(raw_code: &str, rules: &[EmbeddedBarcodeRule])
    -> Result<ParsedScan, ScanError>;
impl ParsedScan {
    pub fn item_code(&self) -> &str;
}
pub fn ean13_checksum_ok(code: &str) -> bool;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScanError {
    #[error("checksum failed")]                ChecksumFailed,   // E.40: reject, never guess
    #[error("length {0} invalid for {1:?}")]   BadLength(usize, BarcodeKind),
    #[error("embedded value out of range")]    ValueOutOfRange,
    #[error("non-numeric in numeric span")]    NonNumeric,
}
```

Properties: `prop_ean13_checksum_matches_reference`, `prop_embedded_parse_roundtrip`, `prop_corrupt_digit_never_parses_clean` (**E.40 — a checksum error is a rejected scan and an honest error, never a guess**).

### 4.2 `PriceSource` — where a price is allowed to come from — [1.2.4]

The register's own threat model ranks *"a cashier discounts for friends"* first, and the plan answers
it with `price.override`: a capability, a reason code, a margin floor, the ministry ceiling, an audit
row and a manager escalation. All of that is optional as long as any other path can set a line's
price. `cart_add_line` used to take `unit_price_minor?`, under the base `sale.create` capability, with
no audit and no escalation — so the whole control was a suggestion.

The fix is not a check inside the handler. It is that **no type in this crate can be built from a
number the caller chose.**

```rust
/// A price and where it came from. `amount` is private, so the ONLY way to get
/// one is a constructor — and of the three, two take no amount at all: they
/// require an artefact the webview cannot fabricate, a catalogue `Product` or an
/// `EmbeddedBarcodeRule` plus the raw code that matched it. The third is the
/// department sale, and it is the one `add_line` refuses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceSource { amount: Money, origin: PriceOrigin }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PriceOrigin {
    Catalog    { product_id: ProductId },
    Label      { rule_prefix: String, raw_code: String },
    Department { tax_category_id: TaxCategoryId },        // §6.5, and only §6.5
}

impl PriceSource {
    /// Takes no amount. That is the entire point.
    pub fn from_catalog(p: &Product) -> PriceSource;

    /// Derives the amount from the parsed scan; refuses a rule whose
    /// `value_kind` is not `PriceMinor`.
    fn from_label(scan: &ParsedScan, c: Currency)
        -> Result<PriceSource, ScanError>;

    /// The one constructor that takes a typed amount. `add_line` REFUSES a
    /// `Department` origin; only `add_department_line` accepts it, and that
    /// function requires `Authorized<cap::DepartmentSale>` and returns an
    /// `AuditIntent` (§6.5).
    pub fn from_department(amount: Money, tax_category_id: TaxCategoryId) -> PriceSource;

    pub fn amount(&self) -> Money;
    pub fn origin(&self) -> &PriceOrigin;
}
```

Tests: `add_line_refuses_a_department_price_source` · `from_label_refuses_a_weight_rule` ·
`client_supplied_price_cannot_bypass_override` — the last one is an IPC-layer test
([`ipc-contract.md`](ipc-contract.md) §5) and belongs to the boundary, because the boundary is what
makes the guarantee: `cart_add_line` has no field a price could arrive in.

### 4.3 `ScanLookup` — the one place a scan becomes a line — [1.2.4]

The `catalog_by_barcode` handler parses the raw scan, reads only `ParsedScan::item_code()` for the
repository lookup, then gives the opaque parse evidence and all matching product hits to
`resolve_scan`. That domain handler returns this type, and `cart_add_scan` consumes it. Every variant
is a decision the UI must render differently, which is why it is an enum and not an
`Option<Product>`.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScanLookup {
    /// One live product. `pack_qty` is the BARCODE's multiplier, not the
    /// product's: the same product's single-unit and six-pack codes differ.
    Product        { hit: ProductHit, pack_qty: Qty },
    /// The label carries a weight. Quantity is that weight; the price is the
    /// catalogue's, and `mul_qty` rounds once, exactly as for any other line.
    WeightEmbedded { hit: ProductHit, qty: Qty },
    /// The label carries a PRICE. Dependent crates may match this variant but
    /// cannot construct it; only `resolve_scan` can call the private label
    /// constructor in §4.2. That keeps the scan path from becoming a general
    /// price-entry path.
    #[non_exhaustive]
    PriceEmbedded  { hit: ProductHit, price: PriceSource, derived: Option<DerivedWeight> },
    /// More than one live product claims this code (E.36). Newest active first,
    /// and the UI says that it chose rather than choosing silently.
    Ambiguous      { hits: Vec<ProductHit> },
    ChecksumFailed { code: String },       // E.40
    Unknown        { code: String },       // E.39 — the queue must not stall (§6.5)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductHit { pub product: Product, pub matched_on: MatchedOn }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MatchedOn { Barcode { code: String, kind: BarcodeKind }, Plu(String), Name, Tile }

/// The weight a label's price implies at TODAY's catalogue price per unit.
/// ADVISORY, and never money: a label printed at 09:00 for 0.416 JOD says
/// nothing about the price per kilo at 21:00 (E.80). It feeds the stock ledger
/// with `is_weight_derived` set, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DerivedWeight { pub qty: Qty, pub catalog_unit_price: Money }

pub fn resolve_scan(parsed: ParsedScan, candidates: Vec<ProductHit>, c: Currency)
    -> Result<ScanLookup, ScanError>;
```

There is no public `ScanResult`, `EmbeddedAmount` or label-price constructor. The repository never
returns a resolved price-bearing value: it returns product hits for the opaque parse's item code.
Only `resolve_scan` can inspect `ParsedScanKind` and call `PriceSource::from_label`, so dependent Rust
code cannot manufacture `PriceEmbedded { price }` from a caller-chosen integer and hand it to the
cart.

**What a price-embedded label does to the line.** The label is the contract with the customer, so its
amount is what is charged: the line is **one unit at the label's price**, and the derived weight is
recorded as an estimated stock basis, not as the line quantity.

The alternative — quantity = derived weight, unit price = the catalogue's — was rejected twice over.
It charges an amount that differs from the sticker the customer is holding whenever the price per
kilo has moved since the label printed, and it breaks the per-line arithmetic identity every fiscal
document is checked against, because `unit_price × qty` no longer equals the line amount. A line of
`1 × 0.416` is honest: the unit of sale was one labelled package.

Tests: `price_embedded_line_total_equals_the_label` · `price_embedded_line_is_one_unit_not_a_weight` ·
`price_embedded_stock_event_carries_the_derived_weight_flagged_estimated` ·
`price_embedded_after_a_price_per_kilo_change_still_charges_the_label` (E.80) ·
`a_multipack_barcode_adds_its_pack_quantity` (E.78).

---

## 5 · `tax.rs` — the tax engine — [1.3.x]

Full jurisdiction detail in [`tax-jordan.md`](tax-jordan.md). This is the shape.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaxTreatment { Standard, Reduced, Zero, Exempt }

/// One tax component on a line. v1 ships GST only, but the schema and this
/// type allow >1 so Special Sales Tax (tobacco, fuel…) is a data change,
/// not a migration of the engine (master plan B.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaxComponent {
    pub code: String,               // "GST", "SST"
    pub treatment: TaxTreatment,
    pub basis: TaxBasis,
    /// Application order, ascending. Components sharing a `sequence` are
    /// computed on the same base and cannot depend on one another.
    pub sequence: u8,
    pub base: TaxBase,
    pub is_inclusive_capable: bool,
}

/// A tax is a percentage, a fixed amount per unit, or both.
///
/// A component list that holds only a rate cannot express a per-unit excise,
/// and Jordan's Special Sales Tax schedules contain both forms — the plan's
/// claim that "enabling SST is data plus a rate rule, never an engine
/// migration" was not true of a `rate: Percent` field. It is true of this one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaxBasis {
    Percentage { rate: Percent },
    PerUnit    { amount: Money },                    // × the line's quantity
    Compound   { rate: Percent, per_unit: Money },
}

/// What a component is charged ON. `LineNet` is the ordinary case. A general
/// tax charged on a base that already contains an excise is `NetPlusComponents`,
/// naming the components it sits on top of — never "all of them", because the
/// order has to be readable from the persisted row six months later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaxBase {
    LineNet,
    NetPlusComponents { codes: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriceMode { Inclusive, Exclusive }     // Jordan default: Inclusive

/// The store's jurisdiction profile (master plan B.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoreTaxProfile { Standard, Asez, DevelopmentArea, Unregistered }

/// Why THIS supply is treated the way it is. Zero-rating is not always a
/// property of the product: the same standard-rated SKU can be zero-rated
/// because this particular transaction is an export or a supply to an eligible
/// body. Category, profile and date cannot express that, so without this the
/// engine either charges 16% on a documented zero-rated supply or zero-rates
/// every domestic sale of the SKU, and reports it in the wrong return box.
///
/// Snapshotted onto the sale (I-5) — the evidence reference is what an
/// inspection asks for, and today's customer record is not it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupplyTaxContext {
    pub destination: SupplyDestination,
    pub reason: Option<ZeroRatingReason>,
    pub evidence_ref: Option<String>,     // export declaration, eligibility authority
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupplyDestination { Domestic, Export, FreeZone, DevelopmentArea, EligibleBody }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZeroRatingReason { Export, FreeZoneSupply, EligibleEntity, ProductCategory }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineTax {
    pub net: Money,
    pub components: Vec<ComponentTax>,
    pub tax_total: Money,
    pub gross: Money,
    /// Carried so `summarize_tax` can split the zero-rated box by reason
    /// without being handed the cart again.
    pub supply_reason: Option<ZeroRatingReason>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentTax {
    pub code: String,
    pub treatment: TaxTreatment,
    pub basis: TaxBasis,
    /// What this component was charged on, persisted. Without it a compound
    /// line cannot be re-derived from its own row, and the filing report has to
    /// guess which component sat on which base.
    pub base_amount: Money,
    pub amount: Money,
}

/// THE function. Everything about tax is here.
///   Inclusive: net = gross / (1+r);  tax = gross − net
///   Exclusive: tax = net × r;        gross = net + tax
/// Computed in rust_decimal, rounded ONCE per line by `rule`, returned as fils.
///
/// With more than one component, they are applied in `sequence` order, each on
/// the base its `TaxBase` names, and the whole line still rounds once. `qty` is
/// an argument because a `PerUnit` component is charged per unit and there is
/// no way back from a line total to a quantity.
pub fn compute_line_tax(
    taxable: Money,          // line gross (inclusive) or net (exclusive), post-discount
    qty: Qty,
    mode: PriceMode,
    components: &[TaxComponent],
    profile: StoreTaxProfile,
    supply: &SupplyTaxContext,
    rule: RoundingRule,
) -> Result<LineTax, TaxError>;

/// Receipt tax summary, grouped by rate. This is the EXACT SUM of line taxes.
/// It is never re-derived from the total — that is how JoFotara total checks fail
/// (master plan C.3, and correction C-3 in plan-validation.md).
pub fn summarize_tax(lines: &[LineTax]) -> Vec<TaxSummaryRow>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaxSummaryRow {
    pub code: String, pub treatment: TaxTreatment, pub rate: Percent,
    /// `Some` for a per-unit or compound component. It is part of the grouping
    /// key: two lines at the same percentage but different fixed amounts are
    /// two rows on the filing report, because they are two rates in law.
    pub per_unit: Option<Money>,
    pub reason: Option<ZeroRatingReason>,   // splits the zero-rated box by why
    pub net: Money, pub tax: Money, pub gross: Money,
}

/// Rate resolution is time-effective DATA. Rates change by Cabinet decree;
/// a rate in code is a re-release (master plan B.1).
pub fn resolve_components(
    category: TaxCategoryId,
    rules: &[TaxRateRule],
    profile: StoreTaxProfile,
    at: Timestamp,
) -> Result<Vec<TaxComponent>, TaxError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaxRateRule {
    pub tax_category_id: TaxCategoryId,
    pub component_code: String,
    pub treatment: TaxTreatment,
    pub basis: TaxBasis,
    pub sequence: u8,
    pub base: TaxBase,
    pub valid_from: Timestamp,
    pub valid_to: Option<Timestamp>,
    /// `None` is scoped to `Standard` ONLY. A rule that applies to every
    /// profile lets an ASEZ store silently inherit mainland rates, which is a
    /// different regime with a different return — see rule 3 in
    /// [`tax-jordan.md`](tax-jordan.md) §3.
    pub profile_scope: Option<StoreTaxProfile>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TaxError {
    #[error("no rate rule for category at {0:?}")]  NoRuleInEffect(Timestamp),
    #[error("overlapping rate rules for {0}")]      OverlappingRules(String),
    #[error("inclusive pricing with a component that cannot be inclusive")] NotInclusiveCapable,
    #[error("component {0} names a base component {1} that is not on this line")] UnknownBaseComponent(String, String),
    #[error("components {0} and {1} depend on each other")] CircularComponentBase(String, String),
    #[error("profile {0:?} has no complete rate pack; refusing to fall back")] ProfilePackIncomplete(StoreTaxProfile),
    #[error("supply destination {0:?} has no reason code")] SupplyReasonMissing(SupplyDestination),
    #[error(transparent)]                           Money(#[from] MoneyError),
}
```

> ⚠️ **OPEN — blocks 1.3.5.** For a line carrying both General Sales Tax and Special Sales Tax: what
> is each component's base, in what order, and is the fixed part per unit or per line? Default until
> answered: `SST` at `sequence: 0` on `LineNet`, `GST` at `sequence: 1` on
> `NetPlusComponents { codes: ["SST"] }`, fixed parts charged **per unit**. No SST rate rule ships
> seeded, and a store whose assortment needs one fails closed on `ProfilePackIncomplete` rather than
> selling at GST only. Owner: 1.3.5. Source that settles it: the consolidated Special Tax Regulation
> and the General Sales Tax Law as read by the merchant's tax advisor, recorded in
> [`merchant-decisions.md`](merchant-decisions.md).

> ⚠️ **OPEN — blocks 1.3.2.** Which `ZeroRatingReason` values does the filing return distinguish, and
> what evidence must `evidence_ref` point at for each? Default until answered: the four listed above,
> `evidence_ref` mandatory for `Export` and `FreeZoneSupply`, and `SupplyDestination` other than
> `Domestic` refused until a reason and evidence are supplied. Owner: 1.3.2, with the fiscal code
> list confirmed by 2.7.0. Source that settles it: the official ISTD declaration form and its filing
> instructions, plus the merchant's tax advisor.

Properties — the ones that matter most in the whole crate:

| Test | Invariant |
|---|---|
| `prop_inclusive_net_plus_tax_equals_gross` | Exactly, at fils, for every rate and amount |
| `prop_line_tax_sum_equals_receipt_tax` | Σ line taxes == summary total, **exactly** |
| `prop_exempt_and_zero_produce_zero_tax_but_differ_in_reporting` | Exempt ≠ zero-rated — they must not collapse (master plan B.1) |
| `prop_tax_never_exceeds_gross` | |
| `prop_rate_resolution_is_deterministic_at_boundaries` | `valid_from` inclusive, `valid_to` exclusive, no gap, no overlap |
| `prop_unregistered_profile_yields_no_tax` | The tax-disabled merchant configuration (C-4) |
| `prop_multi_component_line_sums_correctly` | Σ `ComponentTax.amount` == `tax_total`, at any sequence and any base |
| `prop_per_unit_component_scales_with_quantity` | A fixed excise on 3 units is three times the excise on 1 |
| `an_incomplete_profile_pack_fails_closed` | An `asez` store with no ASEZ-scoped rule refuses; it does not inherit 16% |

---

## 6 · `cart.rs` — the checkout state machine — [1.4.x]

Blueprint §8's diagram as a Rust enum. Illegal transitions do not compile.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Sale {
    Building(Cart),
    Tendering(Tendering),
    Finalizing(Finalizing),
    Complete(CompletedSale),
    Parked(Cart),
    Voided(VoidedSale),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cart {
    pub id: SaleId,
    pub register_id: RegisterId,
    pub shift_id: ShiftId,
    pub cashier_id: UserId,
    pub currency: Currency,
    pub lines: Vec<CartLine>,
    pub basket_discounts: Vec<BasketDiscount>,
    pub customer_id: Option<CustomerId>,
    pub buyer_tin: Option<String>,      // B2B fiscal invoices (master plan B.2)
    pub buyer_name: Option<String>,
    pub supply: SupplyTaxContext,       // §5 — defaults to Domestic
    pub is_training: bool,              // checked EVERYWHERE, incl. the fiscal queue
    pub opened_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartLine {
    pub id: SaleLineId,
    pub product_id: Option<ProductId>,  // None for a department sale (§6.5)
    pub name_snapshot: String,          // I-5: copied at capture, never re-read
    pub unit_price: Money,              // I-5
    pub price_origin: PriceOrigin,      // §4.2 — where that number came from
    pub qty: Qty,
    pub unit: UnitOfMeasure,
    pub is_weighed: bool,
    pub tax_category_id: TaxCategoryId,
    pub discounts: Vec<LineDiscount>,
    pub price_override: Option<PriceOverride>,
    pub min_age: Option<u8>,            // captured, so a catalogue edit mid-cart cannot clear the gate
    pub age_confirmed: bool,
    pub label: Option<DerivedWeight>,   // §4.3 — advisory stock basis, never money
    pub promo_group: Option<PromoGroupRef>,   // §14, and the requalification input in §10
}
```

### 6.1 Transitions — [1.4.3 … 1.4.8]

Every one is a free function taking the state by value and returning the next state or an error. No method mutates in place; no transition is reachable from the wrong state because the wrong state is a different type.

```rust
/// The only way to describe a new line, and it carries no number the caller
/// chose: `price` is a `PriceSource` (§4.2), and both of its ordinary
/// constructors require a catalogue `Product` or a matched barcode rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddLine {
    pub line_id: SaleLineId,
    pub product_id: Option<ProductId>,
    pub name_snapshot: String,
    pub price: PriceSource,
    pub qty: Qty,
    pub unit: UnitOfMeasure,
    pub is_weighed: bool,
    pub tax_category_id: TaxCategoryId,
    pub min_age: Option<u8>,
    pub max_price: Option<Money>,
    pub age_confirmed: bool,
    pub label: Option<DerivedWeight>,
}

impl AddLine {
    pub fn from_product(line_id: SaleLineId, p: &Product, qty: Qty) -> Result<AddLine, CartError>;
    /// Applies the barcode's `pack_qty` and the label's price or weight. The
    /// one function that turns a `ScanLookup` into a line, so the pack
    /// multiplier and the label rule cannot be applied in two places.
    pub fn from_scan(line_id: SaleLineId, hit: ScanLookup) -> Result<AddLine, CartError>;
    /// `product_id: None`, quantity one, the department's tax category, and a
    /// `PriceOrigin::Department` price that `add_line` will refuse (§6.5).
    pub fn department(line_id: SaleLineId, d: &Department, amount: Money,
                      scanned_code: Option<String>) -> Result<AddLine, CartError>;
}

pub fn open(id: SaleId, ctx: &CartContext) -> Cart;

/// Refuses a `PriceOrigin::Department` price (use `add_department_line`), an
/// inactive product (E.38), an unconfirmed age-restricted line (E.69), and a
/// fractional quantity of an indivisible unit (§6.3).
pub fn add_line(cart: Cart, req: AddLine)          -> Result<Cart, CartError>;
pub fn set_qty(cart: Cart, line: SaleLineId, q: Qty) -> Result<Cart, CartError>;
pub fn void_line(cart: Cart, line: SaleLineId, reason: VoidReason, by: UserId)
                                                    -> Result<(Cart, AuditIntent), CartError>;
pub fn apply_line_discount(cart: Cart, line: SaleLineId, d: DiscountRequest,
                           auth: &Authorized<cap::DiscountManual>)
                                                    -> Result<(Cart, AuditIntent), CartError>;
pub fn apply_basket_discount(cart: Cart, d: DiscountRequest,
                           auth: &Authorized<cap::DiscountManual>)
                                                    -> Result<(Cart, AuditIntent), CartError>;
pub fn override_price(cart: Cart, line: SaleLineId, to: Money, reason: OverrideReason,
                      auth: &Authorized<cap::PriceOverride>)
                                                    -> Result<(Cart, AuditIntent), CartError>;
/// A non-catalogue line for goods with no SKU (§6.5). The ONLY transition that
/// accepts an operator-entered amount, and it is capability-gated, capped,
/// audited, and reported beside price overrides — which is exactly what
/// `add_line`'s old optional price was not.
pub fn add_department_line(cart: Cart, req: AddLine, policy: &DepartmentPolicy,
                           auth: &Authorized<cap::DepartmentSale>)
                                                    -> Result<(Cart, AuditIntent), CartError>;

pub fn attach_customer(cart: Cart, c: CustomerId)   -> Result<Cart, CartError>;
pub fn set_buyer_tin(cart: Cart, tin: String, name: Option<String>)
                                                    -> Result<Cart, CartError>;
pub fn clear_buyer_tin(cart: Cart)                  -> Result<Cart, CartError>;
pub fn set_supply_context(cart: Cart, supply: SupplyTaxContext) -> Result<Cart, CartError>;

pub fn park(cart: Cart)                              -> Result<Sale, CartError>;
pub fn resume(parked: Cart)                          -> Result<Cart, CartError>;

pub fn begin_tender(cart: Cart, priced: PricedCart)  -> Result<Tendering, CartError>;
pub fn back_to_building(t: Tendering)                -> Result<Cart, CartError>; // only if no tender collected
pub fn add_tender(t: Tendering, tender: Tender)      -> Result<Tendering, CartError>;
pub fn remove_tender(t: Tendering, id: TenderId)     -> Result<Tendering, CartError>;
pub fn begin_finalize(t: Tendering)                  -> Result<Finalizing, CartError>;
pub fn complete(f: Finalizing, effects: FinalizeEffects) -> Result<CompletedSale, CartError>;

pub fn void_sale(sale: Sale, reason: VoidReason,
                 auth: &Authorized<cap::SaleVoid>)
                                                     -> Result<(VoidedSale, AuditIntent), CartError>;
```

> **`AuditIntent`.** A pure domain function cannot write an audit row — that is I/O. It returns the *intent*: actor, approver, action, entity, and a canonical payload. The shell writes it inside the same transaction as the effect. This is how "every ✓ that reverses money writes the audit log" (master plan C.10) becomes structural rather than remembered.

> **`set_supply_context` has no IPC command in v1.** The transition exists so the engine and its tests
> can express an export or a free-zone supply, and the register always sends `Domestic`. Exposing it
> before the reason codes, the evidence capture and the return-box mapping exist would let a cashier
> zero-rate a supply that is not zero-rated, which is a filing error the merchant pays for. See the
> open item in §5 and [`ipc-contract.md`](ipc-contract.md) §3.

### 6.2 Pricing the cart — [1.4.9]

```rust
/// The ONE function that turns a cart into money. Pure, deterministic,
/// and the only source of every number on the receipt AND in the fiscal
/// document — never recomputed separately (master plan B.2).
pub fn price_cart(
    cart: &Cart,
    tax_rules: &[TaxRateRule],
    settings: &PricingSettings,
    now: Timestamp,
) -> Result<PricedCart, CartError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricedCart {
    pub lines: Vec<PricedLine>,
    pub subtotal: Money,          // Σ line gross before discounts
    pub discount_total: Money,
    pub tax_summary: Vec<TaxSummaryRow>,
    pub tax_total: Money,
    pub total: Money,             // what the customer owes, pre cash-rounding
    pub currency: Currency,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricedLine {
    pub line_id: SaleLineId,
    pub gross_before_discount: Money,
    pub discount_attributions: Vec<DiscountAttribution>,  // promo id or manual actor + amount
    pub taxable: Money,
    pub tax: LineTax,
    pub line_total: Money,
}
```

`DiscountAttribution` is not an optimisation. Campaign cost reporting (master plan C.9) and JoFotara's per-line discount requirement (correction **C-2**, as revised in [`00-master-plan.md`](../00-master-plan.md) §4a) both read it. A basket discount that has not been attributed to lines cannot be turned into a fiscal document at all.

**The largest-remainder tie-break is a function of line content, not of line position.** Where two
lines are owed the same fractional share, the leftover fil goes to the line with the lower
`(unit_price, product_id, line_id)` — never to "whichever was scanned first". Position-based
tie-breaking is invisible on a single-rate basket, because the total is unchanged. On a multi-rate
basket it moves a fil of discount between a 16% line and an exempt line, which changes the taxable
base, the tax, and the total: two cashiers scanning the same three items in a different order charge
different amounts, the filing report stops reconciling to a hand-checked day by a fil, and the
difference is visible to ISTD because the fil becomes a per-line percentage in the fiscal document.
`prop_price_cart_is_deterministic` does not catch this — same inputs in the same order is not the
same claim as same inputs in any order.

### 6.3 `CartError` — [1.4.2]

```rust
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CartError {
    #[error("no line {0}")]                                    LineNotFound(SaleLineId),
    #[error("cannot modify a completed sale")]                 SaleIsComplete,     // I-4
    #[error("cannot go back to building: {0} tender(s) collected")] TenderCollected(usize),
    #[error("quantity must be positive")]                      NonPositiveQty,
    #[error("{0} is sold in whole units; {1} milli is not one")] FractionalQtyForDiscrete(ProductId, i64),
    #[error("a caller-supplied price needs the department-sale capability")] DepartmentPriceOutsideDepartmentSale,
    #[error("department sale of {0} exceeds the cap {1}")]      DepartmentAmountAboveCap(String, String),
    #[error("negative line amounts are not allowed")]           NegativeLine,      // E.19
    #[error("discount {0} exceeds line value")]                DiscountExceedsLine(String),
    #[error("price {0} is below the floor {1}")]               BelowPriceFloor(String, String),
    #[error("price {0} exceeds the regulated ceiling {1}")]     AboveMaxPrice(String, String), // E.71
    #[error("age confirmation required for {0}")]              AgeConfirmationRequired(ProductId),
    #[error("product {0} is not sellable")]                    ProductInactive(ProductId), // E.38
    #[error("collected {0} is less than due {1}")]             Underpaid(String, String),
    #[error("overtender is only allowed for cash")]            OvertenderNotCash,
    #[error(transparent)] Tax(#[from] TaxError),
    #[error(transparent)] Money(#[from] MoneyError),
}
```

### 6.4 Properties — [1.4.10]

| Test | Invariant |
|---|---|
| `prop_total_equals_lines_minus_discounts_plus_tax` | The master identity, for every cart |
| `prop_line_tax_sum_equals_receipt_tax` | Exactly |
| `prop_no_operation_mutates_a_complete_sale` | I-4, by construction |
| `prop_basket_discount_prorates_to_the_fil` | Σ attributions == the basket discount, exactly |
| `prop_discount_never_makes_a_line_negative` | E.19 |
| `prop_price_cart_is_deterministic` | Same inputs, same order → byte-identical output |
| `prop_price_cart_is_invariant_under_line_reordering` | Same inputs, **any** order → same total, same summary rows, same multiset of line values (E.19b) |
| `prop_discrete_products_never_carry_a_fractional_quantity` | `Each` and `Package` lines are whole units, from every entry path |
| `prop_park_resume_roundtrip_is_identity` | E.3 |
| `prop_zero_total_cart_is_valid` | E.18 — 100% discount is a legal sale |

The 16 ms recompute budget is a **criterion benchmark** (`benches/price_cart.rs`, microstep 1.4.9),
not a property. A wall-clock duration is not an invariant, `proptest` cannot attack it, and a
`prop_`-prefixed timing test lands inside the verification filter
`cargo nextest run -p pos-domain -E 'test(prop_)'` — which then passes or fails with machine load.
Conventions §7 owns the budget; this table owns the invariants.

### 6.5 The unknown barcode, and the department sale — [1.4.12]

The default unknown-barcode policy is quick-add under `product.edit`, which is manager-only. At 22:00
in a one-person shop there is no manager, so the cashier's remaining options are to abandon the item
or to ring it up as something else — a wrong product on the line, the wrong tax category, the wrong
stock movement, and a receipt that misdescribes the goods. The plan's own five-second rule fails by
default, and `queue_never_stalls_on_unknown_code` cannot pass against the configuration it ships with.

The department sale is the path that was named in four places and designed in none.

```rust
/// A department is a sellable bucket with a tax category and a cap. It is not a
/// product: it has no SKU, no barcode, and no stock.
///
/// It IS a `category` — same id space, same rows — carrying a tax category and
/// a flag. A separate table would need its own sync direction, its own
/// back-office screen and its own tree, to hold the same three fields the
/// taxonomy already holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Department {
    pub id: CategoryId,
    pub name_ar: String,
    pub name_en: Option<String>,
    pub tax_category_id: TaxCategoryId,
    pub is_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DepartmentPolicy {
    /// Hard ceiling per line. A department sale is an open-price line, so it is
    /// capped low by default and raised deliberately.
    pub max_line_amount: Money,
    /// Above this, the line escalates like any other privileged action.
    pub escalate_above: Option<Money>,
}
```

Rules, all of them load-bearing:

1. The line carries the department's `tax_category_id`. There is no untaxed department, and no
   department without one — that is the whole reason this is not just "an open-price line".
2. `product_id` is `None` and no stock event is written. Inventing a product to hold the sale is how
   a catalogue fills with `MISC-0007`.
3. Every department line is **audited**, with the operator, the amount, and the code that was scanned
   when there was one. It appears on the price-override report beside overrides, because it is the
   same class of exception: an amount a person typed.
4. The receipt prints the department name, not "unknown item". A customer's proof of purchase has to
   describe what they bought.
5. `sale.department` is granted to the cashier by default (§8) — a control the queue cannot use is
   not a control, it is an outage.

Tests: `department_line_carries_its_department_tax_category` · `department_line_writes_no_stock_event` ·
`department_line_is_always_audited` · `department_above_cap_is_refused` ·
`department_above_escalation_threshold_requires_approval` ·
`queue_never_stalls_on_unknown_code` (E.39, E.83).

### 6.6 The in-flight sale — [1.8.4]

`sale.status` admits only `completed`, `voided` and `parked`, and the finalize transaction writes
everything at once — so before it commits, nothing about a `Tendering` or `Finalizing` sale exists on
disk. The recovery step promises to "re-run idempotently from persisted state" and there is no
persisted state to re-run from. The consequence is the one the `Unknown` protocol exists to prevent,
arriving by a different door: the terminal approved, the customer's card is charged, the register lost
power before commit, and after restart there is no sale, no tender, no `psp_ref` and nothing to query.
The money surfaces days later as an unmatched line in the PSP settlement report, with no document to
attach it to.

```rust
/// The durable projection of a sale that has left `Building`. Written BEFORE
/// the first irreversible side effect — before `tender_start_card`, and before
/// `begin_finalize` — and deleted inside the finalize transaction itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InFlightSale {
    pub sale_id: SaleId,
    pub register_id: RegisterId,
    pub state: Sale,                  // the exact state-machine value, serialized
    pub card_op: Option<CardOpRef>,   // E.2's status-query input
    pub idempotency_key: Uuid,        // one per checkout attempt, never reused
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardOpRef { pub sale_ref: String, pub amount: Money, pub started_at: Timestamp }

#[derive(Debug, Clone, PartialEq)]
pub enum Resumption {
    /// Return to the tender screen with the tenders already collected.
    ResumeTendering(Tendering),
    /// A card operation was outstanding: query its status BEFORE anything else,
    /// then attach or discard (E.2).
    QueryCardFirst { t: Tendering, op: CardOpRef },
    /// Finalize had begun. Replay it under the same idempotency key.
    ReplayFinalize(Finalizing),
    /// Nothing was collected and nothing is outstanding. Discard, audited.
    Abandon { reason: &'static str },
}

pub fn resume_in_flight(row: InFlightSale, now: Timestamp) -> Result<Resumption, CartError>;
```

A parked cart is not this. A park is a deliberate cashier action with a label and an expiry, and it
never syncs; an in-flight row is machinery that exists for at most the length of one checkout.

Tests: `an_interrupted_tendering_is_recovered_and_status_queried` (E.2) ·
`a_checkout_operation_row_never_outlives_its_commit` · `finalize_replays_under_the_same_idempotency_key` ·
`a_card_approval_before_a_power_cut_is_found_and_attached` ·
`prop_resume_never_produces_a_second_authorisation`.

---

## 7 · `tender.rs` — payment collection — [1.5.x]

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TenderType {
    pub code: String,                 // see the table below
    pub opens_drawer: bool,
    pub allows_change: bool,
    pub is_cash_counted: bool,        // counts toward expected drawer cash (§11)
    pub refundable_to: RefundRouting,
    /// An internal tender moves value between two documents and never between
    /// the customer and the drawer. It is excluded from every takings figure,
    /// from the PSP reconciliation, and from expected cash.
    pub is_internal: bool,
}

/// Where a refund of this tender is allowed to go. `Same` routes back to the
/// original instrument; `None` refuses — an internal or non-refundable tender
/// has no destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefundRouting { Same, Cash, StoreCredit, None }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tender {
    pub id: TenderId,
    pub type_code: String,
    pub amount: Money,
    pub psp_ref: Option<String>,      // card: reconciliation + refunds (master plan C.4)
    pub masked_pan: Option<String>,   // receipt only. Nothing else from the card. Ever.
    pub scheme: Option<String>,
    pub state: TenderState,
}

/// A PROJECTION, not a column anyone updates. A tender is inserted `Pending` or
/// `Collected` and later settles or reverses, and `sale_tender` is a fact table
/// on a completed sale — so the transition is an appended `tender_status_event`
/// and this value is the fold over that tender's events. Modelling it as a
/// mutable field is what left the local register updating a row the server had
/// revoked `UPDATE` on ([`00-master-plan.md`](../00-master-plan.md) §4a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TenderState {
    Collected,
    Pending,
    Unknown,
    Failed,
    Reversed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tendering {
    pub cart: Cart,
    pub priced: PricedCart,
    pub tenders: Vec<Tender>,
    pub cash_rounding: Option<CashRounding>,
}

pub fn remaining_due(t: &Tendering) -> Result<Money, MoneyError>;
pub fn change_due(t: &Tendering)    -> Result<Money, MoneyError>;
pub fn is_settled(t: &Tendering)    -> bool;

/// Cash rounding (master plan B.5) applies ONLY when the FINAL tender is cash,
/// and only to the remaining cash amount (E.14). Card charges the exact total.
/// The adjustment is an explicit field so books and fiscal totals reconcile.
pub fn compute_cash_rounding(
    remaining: Money, step_minor: i64, dir: RoundingDirection,
) -> Result<CashRounding, MoneyError>;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CashRounding { pub original: Money, pub rounded: Money, pub adjustment: Money }

/// A cash PAYOUT rounds to the same coin step. Refunding a cash-rounded sale
/// otherwise produces an amount that cannot physically be handed over: a 1.247
/// basket collected as 1.250 refunds 1.247, and the smallest coin in everyday
/// circulation is 10 fils. The cashier then pays some other number that the
/// ledger does not record, and the drawer is quietly short every time.
///
/// The default direction is the customer's favour — round the payout UP — so
/// the merchant absorbs the fils rather than arguing about three of them at the
/// counter. The difference is recorded on the refund document's
/// `rounding_adj_minor`, exactly as on a sale, so the books still reconcile.
pub fn compute_refund_rounding(
    payout: Money, step_minor: i64, dir: RoundingDirection,
) -> Result<CashRounding, MoneyError>;
```

### 7.1 The tender-type table — [1.5.1]

| `code` | `opens_drawer` | `allows_change` | `is_cash_counted` | `is_internal` | `refundable_to` |
|---|---|---|---|---|---|
| `cash` | ✓ | ✓ | ✓ | | `same` |
| `card` | | | | | `same` |
| `cliq` | | | | | `same` |
| `voucher` | | | | | `none` |
| `store_credit` | | | | | `store_credit` |
| **`exchange`** | | | | ✓ | `none` |

**`exchange` is the instrument the exchange flow was missing.** "Return + new sale, settling only the
difference" requires value to pass from the refund document to the sale document without cash or card
moving, and every document must still balance on its own — a sale is settled when `collected ≥ due`,
and a refund routes its full amount. With no tender that can carry the offset, the only representable
exchange is a full refund followed by a full payment: two PSP round trips, two lines on the customer's
card statement, two fees, and a Phase-2 gate item that is not implementable as written.

How the pair settles:

- The refund document is settled by an `exchange` tender for the offset portion, and by
  `route_refund_tenders` for any excess — so if the new item is **cheaper**, the difference goes back
  to the original card or cash exactly as an ordinary refund would.
- The new sale is settled by a matching `exchange` tender plus real tenders for the balance — so if
  the new item is **dearer**, the customer pays only the difference.
- The two `exchange` tenders are equal and opposite by construction, and the pair is linked through
  `document_link` with `link_kind = 'exchange'`.
- `is_cash_counted = 0` and `is_internal = 1`, or the offset would appear in expected drawer cash on
  both documents and the shift would close short by twice the exchanged value.

Properties: `prop_split_tender_sums_to_total`, `prop_cash_rounding_only_on_final_cash_tender` (E.14),
`prop_rounding_adjustment_keeps_total_exact`, `prop_change_never_negative`,
`prop_card_tender_never_exceeds_remaining_due`,
`prop_exchange_pair_nets_to_the_customer_facing_difference`,
`prop_internal_tenders_never_reach_expected_cash`,
`prop_cash_refund_is_payable_in_circulating_coin`.
Tests: `exchange_with_a_negative_difference_routes_to_the_original_card` (E.81) ·
`cash_refund_is_rounded_to_the_coin_step` (E.73).

---

## 8 · `permissions.rs` — capabilities with teeth — [1.6.x] *(gap G-6)*

"RBAC enforced in Rust, not in the UI" needs a mechanism, or the twentieth command ships without a check.

```rust
/// A capability is a marker TYPE, not a const-generic string.
///
/// `Authorized<const C: &'static str>` does not compile, and never has:
/// rustc answers "`&'static str` is forbidden as the type of a const generic
/// parameter — the only supported types are integers, `bool`, and `char`".
/// String const generics remain unstable. Do NOT "fix" this back into a
/// runtime `&str` field: that discards the entire compile-time property this
/// design exists for, silently. The marker type below keeps that property,
/// on stable, and is what 1.6.4 builds.
pub trait Capability {
    const NAME: &'static str;
}

/// Declares each capability exactly once: its marker type, its wire name, and
/// its membership in `ALL`. One source, so a name and a type cannot drift —
/// which is how `sale.park`, `sale.resume` and the cash-movement capability
/// came to be used by the IPC catalogue while missing from the list.
macro_rules! capabilities {
    ($($ident:ident => $name:literal),+ $(,)?) => {
        pub mod cap {
            use super::Capability;
            $(
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub struct $ident;
                impl Capability for $ident { const NAME: &'static str = $name; }
            )+
            /// Derived from the types themselves, never hand-maintained.
            pub const ALL: &[&str] = &[$(<$ident as Capability>::NAME),+];
        }
    };
}

capabilities! {
    SaleCreate           => "sale.create",
    SalePark             => "sale.park",
    SaleResume           => "sale.resume",
    SaleVoid             => "sale.void",
    SaleReprint          => "sale.reprint",        // any document, not only your own [1.9.3]
    DepartmentSale       => "sale.department",     // the open-price line (§6.5)
    LineVoid             => "line.void",
    DiscountManual       => "discount.manual",
    PriceOverride        => "price.override",
    RefundReceipted      => "refund.receipted",
    RefundAboveThreshold => "refund.above_threshold",
    RefundReceiptless    => "refund.receiptless",
    RefundCashForCard    => "refund.cash_for_card",
    RefundOutsideWindow  => "refund.outside_window",  // §10 — a defect claim on day 20
    DrawerOpen           => "drawer.open",
    CashMovement         => "cash.movement",       // every kind (schema §cash_movement)
    ShiftOpen            => "shift.open",
    ShiftClose           => "shift.close",
    XReportRun           => "xreport.run",         // split from zreport.run — see §8.3
    ZReportRun           => "zreport.run",
    JournalView          => "journal.view",        // find Tuesday's receipt in ten seconds
    StockAdjust          => "stock.adjust",
    ProductEdit          => "product.edit",
    TaxRateEdit          => "tax.rate.edit",       // a rate is a legal fact, not a setting
    FiscalRemediate      => "fiscal.remediate",    // rebuild a failed fiscal payload (§8.2)
    CustomerLookup       => "customer.lookup",     // PII: name, phone (PDPL) [3.x]
    TrainingToggle       => "training_mode.toggle",
    SettingsEdit         => "settings.edit",
    UserAdmin            => "user.admin",
    BackupRestore        => "backup.restore",      // see the note in §8.2
    ReportsOwn           => "reports.own",         // your own shift and day
    ReportsAll           => "reports.all",         // anyone's shift, any day, any cashier
}

/// A proof-carrying token. `authorize` is the ONLY way to obtain one, and
/// domain functions that reverse money REQUIRE one. You cannot forget the
/// check, because you cannot call the function without it.
///
/// Every field is PRIVATE. Public fields would make the token a struct literal
/// anyone can write — `Authorized { actor, approver, at }` — which is not a
/// proof of anything. Read them through the accessors.
pub struct Authorized<C: Capability> {
    actor: UserId,
    approver: Option<UserId>,        // distinct on escalation (E.52)
    approval: Option<ApprovalId>,    // the handle that was spent; the shell consumes it
    at: Timestamp,
    _capability: PhantomData<fn() -> C>,
}

impl<C: Capability> Authorized<C> {
    pub fn actor(&self) -> UserId { self.actor }
    pub fn approver(&self) -> Option<UserId> { self.approver }
    /// The handle to mark consumed, in the same transaction as the effect.
    pub fn approval(&self) -> Option<ApprovalId> { self.approval }
    pub fn at(&self) -> Timestamp { self.at }
    /// The capability this token proves, for the audit row.
    pub const fn capability() -> &'static str { C::NAME }
}

pub fn authorize<C: Capability>(
    actor: UserId, grants: &GrantSet,
    approval: Option<&ApprovalHandle>,
    binding: &ApprovalBinding,
    policy: &EscalationPolicy, at: Timestamp,
) -> Result<Authorized<C>, PermissionError>;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PermissionError {
    #[error("{0} lacks {1}")]                              Denied(UserId, &'static str),
    #[error("{0} requires manager approval")]              EscalationRequired(&'static str),
    #[error("self-approval is not permitted for {0}")]     SelfApprovalBanned(&'static str), // E.52
    #[error("user {0} is deactivated")]                    UserInactive(UserId),
    #[error("offline authorization window expired")]       OfflineAuthExpired,               // E.55
    #[error("approval authorises {0}, not {1}")]           ApprovalCapabilityMismatch(String, &'static str),
    #[error("approval was issued to {0}, not {1}")]        ApprovalActorMismatch(UserId, UserId),
    #[error("approval names {0}, not {1}")]                ApprovalEntityMismatch(Uuid, Uuid),
    #[error("approval covers {0}, not {1}")]               ApprovalAmountMismatch(String, String),
    #[error("approval expired at {0:?}")]                  ApprovalExpired(Timestamp),
    #[error("approval {0} has already been used")]         ApprovalAlreadyUsed(ApprovalId),
}
```

**Two properties, both proven by the compiler rather than by review.** A token for
the wrong capability is a *type* error — `expected &Authorized<SaleVoid>, found
&Authorized<DiscountManual>` — even though it was validly obtained. And a token
cannot be forged outside the module, because `_capability` is private: attempting
the struct literal is `error[E0451]: field `_capability` ... is private`. Both are
`trybuild` cases in 1.6.4.

**The exhaustiveness test** (`ipc_commands_all_declare_a_capability`, microstep 1.6.7) walks the IPC command registry and fails if any command has no capability entry. Adding a command without declaring one breaks CI.

What neither of those reaches is *which operation* an approval was for, which is §8.1.

### 8.1 `ApprovalHandle` — binding an approval to one operation — [1.6.4]

`Authorized<C>` proves that *somebody* was allowed to do *this class of thing*. That is the half the
compiler can check, and it is worth having. The half it cannot check is which operation the manager
actually looked at — and until that is bound at runtime, an implementer has two options and both are
bad. Hidden global approval state means the next command picks up an approval it was never granted.
A reusable bearer proof means one manager PIN, typed once at 09:00 and watched over a shoulder,
authorises refunds all day, each of them attributed to the manager.

The handle is the missing half: **one approval, one operation, one use.**

```rust
/// Issued by `auth_verify_pin` after the approver authenticates, and consumed
/// by exactly one privileged command. Every field is private and `issue` is the
/// only constructor, so an `ApprovalHandle` cannot be written as a struct
/// literal any more than an `Authorized<C>` can.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalHandle {
    id: ApprovalId,
    capability: String,          // what was approved
    actor: UserId,               // who asked — NOT transferable to another cashier
    approver: UserId,            // who approved; always differs from `actor`
    entity_id: Uuid,             // which sale, which line, which shift
    amount_minor: i64,           // exact; zero means exactly zero, never wildcard
    content_hash: Option<PreparedIntentHash>, // Some only for a prepared intent
    reason: String,
    issued_at: Timestamp,
    expires_at: Timestamp,       // default: issued_at + 120 s
    nonce: [u8; 16],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedIntentHash([u8; 32]);

/// What the operation about to happen actually is. `authorize` compares the
/// handle against this and refuses on any difference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ApprovalBinding {
    pub entity_id: Uuid,
    pub amount_minor: i64,
    pub content_hash: Option<PreparedIntentHash>,
}

impl ApprovalHandle {
    /// The only constructor. It takes the APPROVER's own proof of the
    /// capability, so an approval can never be issued for something the
    /// approver may not do themselves. It unconditionally refuses an actor who
    /// is also the approver. `ban_self_approval` decides whether an operation
    /// requires escalation at all; it never weakens this handle invariant.
    ///
    /// `nonce` and `now` are arguments, not read from the environment: this
    /// crate has no randomness and no clock (I-8).
    pub fn issue<C: Capability>(
        id: ApprovalId, actor: UserId, approver: &Authorized<C>,
        binding: &ApprovalBinding, reason: String,
        now: Timestamp, ttl_ms: i64, nonce: [u8; 16],
    ) -> Result<ApprovalHandle, PermissionError>;

    pub fn id(&self) -> ApprovalId;
    pub fn approver(&self) -> UserId;
    pub fn matches<C: Capability>(&self, actor: UserId, binding: &ApprovalBinding,
                                  now: Timestamp) -> Result<(), PermissionError>;
}
```

**Single use is a persistence property, not a type property**, and pretending otherwise is how it
gets lost. The domain checks capability, actor, entity, exact amount, optional content hash and
expiry. A non-money effect binds `amount_minor = 0`; zero is an exact value and never a wildcard.
`content_hash` is `None` unless the registry names a prepared intent. For
`product_quick_add_request` and `stock_adjustment_request`, the shell loads the row and computes
`PreparedIntentHash` itself before issue; neither `auth_verify_pin` nor any other webview command may
supply that digest. The shell inserts the immutable handle into `approval_handle` at issue. The
privileged transaction then inserts one
`approval_consumption` beside the financial effect and audit row; it never deletes or updates the
handle, because the approval itself is audit evidence. The unique consumption row makes a second
attempt fail after a restart, while the retained handle proves who approved which operation.

Prepared-intent canonical bytes have version `1` and the domain separator
`pos-prepared-intent\0product_quick_add` or `pos-prepared-intent\0stock_adjustment`. Every column
other than `content_hash` follows in declaration order: BLOB and UTF-8 TEXT values are unsigned
32-bit big-endian length-prefixed bytes, INTEGER values are signed 64-bit big-endian, and a nullable
value starts with a `0`/`1` presence byte. The digest is BLAKE3 of those bytes. Both the issue path
and the commit path recompute the digest from the loaded row; commit refuses unless the recomputed
value equals both the row and the handle. The schema additionally refuses `UPDATE` once a matching
approval row exists. That second layer matters because a repository bug must not turn an approved
quantity, reason, barcode, name, price or tax category into a different effect while keeping the same
UUID.

Required tests — [1.6.4]:

| Test | Refuses |
|---|---|
| `a_handle_used_twice_is_refused` | the second `return_commit` with the same handle |
| `an_altered_amount_is_refused` | approved 20.000, committed 200.000 |
| `a_different_sale_is_refused` | approved on sale A, spent on sale B |
| `a_different_actor_is_refused` | the handle is not a bearer token |
| `a_consumed_handle_is_still_consumed_after_restart` | a committed consumption remains one-use after reboot |
| `an_expired_handle_is_refused` | typed at 09:00, spent at 17:00 |
| `the_effect_and_the_consumption_commit_together_or_not_at_all` | roll the transaction back; the handle remains spendable and the effect did not happen |
| `altering_a_stock_request_after_approval_is_refused` | every prepared stock field is varied; the recomputed hash and the database trigger each refuse it |
| `altering_a_quick_add_request_after_approval_is_refused` | every prepared product field is varied; the recomputed hash and the database trigger each refuse it |

### 8.2 The default grant matrix — normative here — [1.6.3]

Master plan C.10's table is fifteen rows, several of which bundle three capability strings, and it
names no owner for several capabilities the IPC catalogue requires. An unspecified capability becomes
an accidental grant or an accidental block, and the seeding test cannot be written from a fixture
that does not enumerate what the test asserts. **This grid is the fixture.** It is derived against
`cap::ALL`, so the test is exhaustive rather than counted — a hard-coded count drifts the moment a
capability is added, and it already had.

| Capability | Cashier | Shift lead | Manager | Owner |
|---|---|---|---|---|
| `sale.create` | ✓ | ✓ | ✓ | — |
| `sale.park` | ✓ | ✓ | ✓ | — |
| `sale.resume` | ✓ | ✓ | ✓ | — |
| `sale.department` | ✓ | ✓ | ✓ | — |
| `sale.reprint` | ✓ | ✓ | ✓ | ✓ |
| `sale.void` | — | ✓ | ✓ | — |
| `line.void` | ✓ | ✓ | ✓ | — |
| `discount.manual` | ✓ *(role cap)* | ✓ | ✓ | sets the caps |
| `price.override` | — | ✓ | ✓ | sets floor and ceiling |
| `refund.receipted` | ✓ *(≤ threshold)* | ✓ | ✓ | sets the threshold |
| `refund.above_threshold` | — | — | ✓ | — |
| `refund.receiptless` | — | — | ✓ | — |
| `refund.cash_for_card` | — | — | ✓ | — |
| `refund.outside_window` | — | — | ✓ | — |
| `drawer.open` | — | ✓ | ✓ | — |
| `cash.movement` | — | ✓ | ✓ | — |
| `shift.open` | ✓ *(own)* | ✓ | ✓ | — |
| `shift.close` | ✓ *(own)* | ✓ | ✓ | — |
| `xreport.run` | — | ✓ | ✓ | — |
| `zreport.run` | — | ✓ | ✓ | — |
| `journal.view` | ✓ *(own shift)* | ✓ | ✓ | ✓ |
| `stock.adjust` | — | — | ✓ | ✓ |
| `product.edit` | — | — | ✓ | ✓ |
| `tax.rate.edit` | — | — | — | ✓ |
| `fiscal.remediate` | — | — | ✓ | ✓ |
| `customer.lookup` | ✓ *(exact match only)* | ✓ | ✓ | ✓ |
| `training_mode.toggle` | — | ✓ | ✓ | — |
| `settings.edit` | — | — | ✓ *(store)* | ✓ |
| `user.admin` | — | — | ✓ *(store)* | ✓ |
| `backup.restore` | — | — | — | ✓ |
| `reports.own` | ✓ | ✓ | ✓ | ✓ |
| `reports.all` | — | — | ✓ *(store)* | ✓ |

The parenthetical qualifiers are enforced, not decorative:

- **`journal.view` (own shift)** is what makes *"a customer is at the counter with a receipt from
  Tuesday"* a ten-second job. Putting the journal behind `reports.all` means fetching a manager to
  read back a receipt, which is why the qualifier exists rather than the capability being withheld.
  Another cashier's sales need `reports.all` — that is the answer to "who can see whose takings".
- **`customer.lookup` (exact match only)** returns a customer for an exact phone, card or loyalty
  number and **never lists or prefix-searches**. PDPL minimisation is a search-shape decision, not a
  disclaimer.
- **`backup.restore`** governs the back-office and settings-screen restore of a register whose
  database opens. It does **not** govern recovery after credential-store loss: the capability tables
  live inside the database that cannot be opened, so that path is authorised by the merchant recovery
  code issued at provisioning (microstep 1.8.5b), not by a session. See
  [`ipc-contract.md`](ipc-contract.md) §3.
- **`shift.close` (own)** permits an authenticated user to append the minimal or counted close for
  the shift they opened. That ordinary path takes no `ApprovalHandle`: ending your own work is not
  an escalation. `shift_force_close_stale` is the separate cross-user path; it always requires a
  reason and a different authorised approver, under the same `shift.close` capability.
- **Owner has no `sale.create`, and no `xreport.run` or `zreport.run`.** That is master plan C.10's
  deliberate split of till roles from back-office roles, kept here so a reader does not read the
  blanks as omissions. An owner reads the day's takings through `reports.all`, which is a report over
  facts; running a Z *closes a shift* on a register they are not standing at.

Tests: `default_matrix_covers_every_capability_in_cap_all` — an exhaustive iteration over `cap::ALL`
that fails when a capability has no row, replacing the counted assertion. ·
`journal_view_is_scoped_to_the_holders_own_shift_without_reports_all` ·
`customer_lookup_refuses_a_prefix_query`.

### 8.3 The things nobody may do — [1.6.3]

Capabilities answer "who may". These answer "nobody, ever", and they are written down because an
unstated prohibition becomes a feature request with an obvious implementation.

| Prohibited | Enforced by |
|---|---|
| Reopen a closed shift | no capability, no command, no repository method — a new shift is a new shift |
| Void or re-take a Z report | `z_report` is append-only in the schema; re-running produces a **new** numbered document |
| Edit a closed shift's counted or over/short figures | frozen once `closed_at` is set |
| Mutate a completed sale | I-4 triggers, and no command exists (§6, [`ipc-contract.md`](ipc-contract.md) §5) |
| Update or delete an audit row | append-only; a correction is a new entry |
| Spend an approval twice | §8.1 |
| Approve your own escalation | every `ApprovalHandle` constructor and row enforces `actor != approver`; `ban_self_approval` selects whether the operation enters escalation, never whether a self-issued handle is valid (E.52) |

**`xreport.run` is split from `zreport.run` for a reason.** The blind close is a wire-level
guarantee — the expected figure is never sent before the count is submitted — and an X report defeats
it, because totals by tender plus the opening float *is* the expected figure. Both capabilities were
one, and both are held by shift lead and manager, who are also the roles that close shifts. In a small
store the shift lead is the person counting their own drawer. Splitting the capability is half the
fix; the other half is that the X report withholds the cash figure from a holder of `shift.close` on
the currently open shift ([`ipc-contract.md`](ipc-contract.md) §3).

---

## 9 · `audit.rs` — hash chain — [1.6.x] *(gap G-7)*

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditIntent {
    pub actor: UserId,
    pub approver: Option<UserId>,
    pub approval: Option<ApprovalId>,  // which approval paid for this (§8.1)
    pub action: &'static str,          // "sale.void", "price.override"
    pub entity: &'static str,
    pub entity_id: Uuid,
    pub reason: Option<String>,
    pub payload: serde_json::Value,    // NEVER PII, NEVER card data (conventions §12)
    pub at: Timestamp,
}

/// What is actually hashed: the intent PLUS the identity the row is stored
/// under. Hashing the intent alone leaves `id`, `seq` and `register_id` outside
/// the chain, so those three can be rewritten without breaking a single hash —
/// which is enough to reattribute a drawer-open or a refund to another register
/// while `verify_chain` still answers `Intact`. The domain tag and version
/// prevent a hash from one context being replayed in another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalAuditEntry<'a> {
    pub domain: &'static str,          // "pos.audit"
    pub version: u16,                  // 1
    pub register_id: RegisterId,
    pub seq: u64,
    pub id: Uuid,
    pub intent: &'a AuditIntent,
}

/// Canonical serialization: JSON with keys sorted, no whitespace, UTF-8.
/// The hash is only reproducible if this is byte-stable, so it is pinned
/// by a golden test, not left to serde_json's default ordering.
pub fn canonical_bytes(entry: &CanonicalAuditEntry<'_>) -> Vec<u8>;

/// hash = BLAKE3(prev_hash ‖ canonical_bytes(entry))
pub fn chain_hash(prev: &[u8; 32], entry: &CanonicalAuditEntry<'_>) -> [u8; 32];

pub const GENESIS: [u8; 32] = [0u8; 32];

/// A `(seq, hash)` pair recorded somewhere the register cannot rewrite: on a
/// Z report, in a verified backup manifest, and from Phase 3 on the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainAnchor { pub register_id: RegisterId, pub seq: u64, pub hash: [u8; 32] }

#[derive(Debug, PartialEq)]
pub enum ChainVerdict {
    Intact  { entries: u64 },
    Broken  { at_seq: u64 },
    /// The chain is internally consistent but ends BEFORE the last anchored
    /// entry: rows were removed from the tail.
    Truncated { anchored_seq: u64, found_seq: u64 },
    /// Verified up to the anchor, and everything above it is unverifiable —
    /// deletion there is indistinguishable from nothing having happened.
    /// `unanchored_from` is 0 when no anchor was supplied at all. Reported as
    /// its own verdict rather than folded into `Intact`, because "we checked
    /// what we could" and "it is intact" are different sentences.
    IntactUnanchoredFrom { entries: u64, unanchored_from: u64 },
}

pub fn verify_chain<'a>(
    register_id: RegisterId,
    entries: impl Iterator<Item = (u64, Uuid, &'a AuditIntent, &'a [u8;32], &'a [u8;32])>,
    anchor: Option<ChainAnchor>,
) -> ChainVerdict;
```

**A local hash chain cannot detect deletion of its own tail**, and saying so is the difference between
a control and a claim. Remove the newest rows and what is left is a shorter, perfectly valid chain —
so a user with database access can erase the drawer-opens, refunds and overrides from the last hour
and the verifier reports `Intact`. `prop_chain_detects_deletion`, as named, asserts something the
mechanism cannot do.

The answer is an **anchor**: the head `(seq, hash)` written where the register does not own it. At
every Z report and every verified backup the head is recorded in the document; from Phase 3 it is sent
to the server, which rejects a fork or a rollback below the last head it saw. Deletion at or below the
last anchor is then detectable, and everything after it is honestly reported as unanchored rather
than counted as verified.

**On a broken chain the register does not stop selling.** It raises an alarm, records the break, and surfaces it in back-office device health. A tamper-evidence mechanism that halts trade converts a forensic signal into an outage.

Properties: `prop_chain_detects_any_single_entry_mutation` ·
`prop_chain_detects_deletion_before_the_anchor` ·
`prop_chain_detects_tail_deletion_against_an_anchor` ·
`prop_chain_detects_reordering` · `mutating_an_identity_column_breaks_the_chain` ·
`golden_canonical_bytes_are_stable`.
The identity mutation varies `register_id`, `seq` and `id`; each must return `Broken`, which the old
canonical input did not give.

> ⚠️ **Residual risk, disclosed.** Entries written after the last anchor — at most one shift's worth,
> or one backup interval — remain deletable without detection on a register whose database key the
> attacker holds. Shortening that window is what anchoring at Z and at backup is for; closing it
> entirely needs the Phase-3 server head. Recorded here rather than left for a reader to discover.

---

## 10 · `refund.rs` — refundable balances — [2.3.x]

The anti-abuse core (master plan C.5).

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefundableLine {
    pub original_line_id: SaleLineId,
    pub product_id: Option<ProductId>,
    pub name_snapshot: String,
    pub sold_qty: Qty,
    pub already_refunded_qty: Qty,
    pub remaining_qty: Qty,                // sold − already_refunded
    /// Display only — what the receipt showed the customer per unit.
    pub unit_price: Money,
    /// The price the line would have carried with no promotion applied,
    /// captured at sale time (I-5). `RequalifyPolicy::DealBreak` reprices the
    /// kept quantity at this.
    pub list_unit_price: Money,
    /// THE allocation base: the original line total including every discount
    /// prorated onto it (E.34). Money is allocated from here, never from
    /// `unit_price`.
    pub line_total: Money,
    pub already_refunded_value: Money,
    pub remaining_value: Money,            // line_total − already_refunded_value
    /// The ORIGINAL line's tax, at the ORIGINAL rate. A refund six months after
    /// a rate change uses the rate the customer paid, for free, because of I-5.
    pub line_tax: LineTax,
    pub promo_group: Option<PromoGroupRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReturnReason { ChangeOfMind, Defective, Damaged, WrongItem }

/// What happens to the rest of a multibuy group when part of it comes back.
/// With "3 for 1.000" on an item normally 0.500, refunding one unit at its
/// prorated share hands back 0.333 and leaves the customer holding two units
/// for 0.667 — when two units at the shelf price are 1.000. That is 0.333 per
/// abuse on a trivial promotion, and it scales with the depth of the offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequalifyPolicy {
    /// DEFAULT. The remaining group no longer qualifies: reprice what the
    /// customer keeps at `list_unit_price` and refund the difference.
    DealBreak,
    /// Refund the prorated share and let the customer keep the deal.
    ProportionalShare,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefundRequest {
    pub lines: Vec<RefundLineRequest>,      // { original_line_id, qty, restock }
    pub reason: ReturnReason,
    pub requalify: RequalifyPolicy,         // default DealBreak
    pub note: Option<String>,
}

pub fn refundable_lines(original: &CompletedSale, prior: &[CompletedSale])
    -> Result<Vec<RefundableLine>, RefundError>;

pub fn build_refund(
    original: &CompletedSale, req: &RefundRequest,
    auth: &Authorized<cap::RefundReceipted>,
    outside_window: Option<&Authorized<cap::RefundOutsideWindow>>,
    ctx: &RefundContext,
) -> Result<RefundDocument, RefundError>;

/// Cards refund to the original card via the PSP against `psp_ref`.
/// Cash-for-card is a separate capability with a threshold —
/// it is a money-laundering vector (master plan C.5).
/// An `exchange` tender settles the offset portion of a linked pair (§7.1).
pub fn route_refund_tenders(original: &[Tender], amount: Money, policy: &RefundPolicy)
    -> Result<Vec<RefundTenderPlan>, RefundError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefundDocument {
    pub lines: Vec<RefundDocumentLine>,
    pub tax_summary: Vec<TaxSummaryRow>,
    pub total: Money,
    /// Set when the payout is cash. `total + adjustment` is what leaves the
    /// drawer, and `adjustment` is stored on the document's
    /// `rounding_adj_minor` so the books reconcile (§7).
    pub cash_rounding: Option<CashRounding>,
    pub reason: ReturnReason,
    pub tenders: Vec<RefundTenderPlan>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RefundError {
    #[error("refund of {0} exceeds remaining refundable {1}")] ExceedsRefundable(String, String),
    #[error("original sale not found")]                        OriginalNotFound,
    #[error("original sale was voided")]                       OriginalVoided,
    #[error("return window of {0} days has expired")]          WindowExpired(u16),
    #[error("a {0:?} claim outside the window requires {1}")]  OutsideWindowNotAuthorized(ReturnReason, &'static str),
    #[error("cash refund for a card sale requires {0}")]       CashForCardNotPermitted(&'static str),
    #[error("training-mode sales cannot be refunded")]         TrainingSale,
    #[error("line {0} belongs to a promotion group; requalification policy is required")]
                                                               RequalifyPolicyRequired(SaleLineId),
    #[error(transparent)] Money(#[from] MoneyError),
}
```

### 10.1 How much money comes back — [2.3.2]

The old rule allocated from a per-unit price, and a per-unit price cannot represent a line whose
discount was prorated onto the **line**. Three units at 0.500 carrying a 0.500 line discount have a
true per-unit value of 0.3333…; stored as fils that is 0.333, and refunding all three returns 0.999
against a line total of 1.000. The fil is destroyed in the one direction that matters, on a document
that is immutable by I-4, and the credit note then cannot net its invoice to zero — which is exactly
what fiscal certification checks against the merchant's real ISTD record.

Allocation is from `remaining_value`, in one rule with no special cases:

```
refund_value(line, q) =
    q == line.remaining_qty  →  line.remaining_value          exactly, no arithmetic
    otherwise                →  line.remaining_value
                                   .split_proportional_by(&[q.milli(),
                                                            (line.remaining_qty − q).milli()])[0]
```

The first branch is what makes a full refund exact. The second is largest-remainder, so a sequence of
partial refunds sums to the line total whatever order they arrive in, and the last one absorbs the
remainder rather than dropping it.

**Requalification**, applied before allocation when the line carries a `promo_group`:

```
DealBreak (default):
    kept        = line.remaining_qty − q
    kept_at_list = line.list_unit_price.mul_qty(kept, rule)
    refund      = max(line.remaining_value − kept_at_list, ZERO)

ProportionalShare:
    refund      = refund_value(line, q)
```

The `max(…, ZERO)` clamp is deliberate and not defensive: on a deep offer the un-promoted price of
what the customer keeps can exceed what they paid for the whole group, and the answer to that is
"nothing comes back", never "the customer owes us money at the returns counter".

**Cash payouts round to the coin step** (§7), in the customer's favour by default, with the
difference on the document.

### 10.2 Defective goods and the window — [2.3.2]

`refund_policy.window_days` is a **goodwill** rule. Until counsel closes the OPEN item below, the
interim default treats a defective-goods claim differently: the flag
`is_defective_claim` and the intent to honour it both already exist in the schema, and there was no
rule, no reason argument, no capability, and therefore no path at all — a customer returning a faulty
kettle on day 20 against a 14-day window was refused by the domain with no override available to
anyone, including the owner. The workaround every shop then uses is a receiptless return or a faked
same-day refund, which puts the transaction outside every control the plan built.

- `ReturnReason::Defective` alone skips the `window_days` check under the interim default.
- Skipping it requires `refund.outside_window` (manager, §8.2) and writes
  `is_defective_claim = 1`.
- `ReturnReason::ChangeOfMind`, `Damaged`, and `WrongItem` use the configured window. `Damaged`
  records the condition in which goods were returned; it is not evidence that the merchant supplied
  a defective product, so treating it as a defect would bypass the control on an ambiguous claim.

> ⚠️ **OPEN — blocks 2.3.2.** For how long, and on what terms, must a defective-goods refund be honoured in Jordan, and may repair or replacement be offered instead of a refund? Default until answered: `ReturnReason::Defective` is not time-barred by `window_days`, refund-to-original-value is offered on request, and repair or replacement is recorded only where the customer chose it.
> Owner: 2.3.2. Source that settles it: Consumer Protection Law No. 7 of 2017 as read by Jordanian counsel, recorded in [`merchant-decisions.md`](merchant-decisions.md).

> ⚠️ **OPEN — blocks 2.3.3.** What payout direction, customer disclosure, and tax/fiscal treatment apply when a cash refund is not divisible by the configured coin step? Default until answered: round the cash payout in the customer's favour, persist and print the signed refund adjustment, and never alter the credited line or tax facts.
> Owner: 2.3.3. Source that settles it: current ISTD cash/credit-note rules plus written Jordanian consumer and tax counsel advice for the merchant's refund policy.

### 10.3 Properties — [2.3.2]

**The invariant that must never break:** `prop_cumulative_refunds_never_exceed_sold_qty` — across
*any* sequence of partial refunds, in any order, including refunds of exchanges (E.30, E.16).

| Property | Invariant |
|---|---|
| `prop_cumulative_refunds_never_exceed_sold_qty` | the anti-abuse core, in any order |
| `prop_refunding_every_unit_returns_the_line_total_exactly` | to the fil, on a discounted multi-unit line (E.75) |
| `prop_partial_refunds_sum_to_the_line_total` | no fil created, none destroyed, whatever the order |
| `prop_refunding_every_unit_returns_the_line_total_exactly` | the credit note against its original nets the carried line to zero |
| `prop_refund_never_leaves_the_customer_better_off_than_not_buying` | requalification, at any policy (E.74) |
| `prop_refund_uses_original_rate` | E.34, after a rate change |
| `prop_refund_rounding_keeps_expected_cash_exact` | the drawer still reconciles (§11) |

Tests: `partial_return_of_a_multibuy_reprices_the_remainder` (E.74) ·
`defective_claim_bypasses_the_window_with_manager_approval` (E.82) ·
`change_of_mind_outside_the_window_is_still_refused` ·
`cash_refund_is_rounded_to_the_coin_step` (E.73) ·
`refund_uses_original_price_after_a_price_change`.

---

## 11 · `shift.rs` — cash accountability — [2.4.x]

**This section is the normative home of the expected-cash formula.** Master plan C.6 and microstep
2.4.4 both carry a version of it and both are wrong; the corrections are recorded in
[`00-master-plan.md`](../00-master-plan.md) §4a, and the arithmetic below is what the code implements.

```rust
/// What should physically be in THIS drawer right now.
pub fn expected_cash(s: &ShiftTotals) -> Result<Money, MoneyError>;

pub fn over_short(expected: Money, counted: Money) -> Result<Money, MoneyError>;

pub fn build_z_report(shift: &Shift, sales: &[CompletedSale], movements: &[CashMovement],
                      z_number: u64) -> Result<ZReport, ShiftError>;
```

```
expected_drawer_cash
    =   opening float
      + Σ (tender.amount − tender.change)   over tenders whose type is_cash_counted
      − Σ (cash paid out on refund documents, including their rounding adjustment)
      + Σ (movement.amount)                 where to_location   is this drawer
      − Σ (movement.amount)                 where from_location is this drawer
```

Four things about it, each of which the previous formula got wrong and each of which is worth a
sentence, because a false variance is worse than no variance at all — the merchant learns to ignore
the number, and over/short is the second-ranked anti-theft control in the whole threat model.

1. **Change is subtracted.** `sale_tender.change_minor` exists, so a cash tender row records the note
   handed over *and* the change returned. A 5.000 note against a 1.250 basket leaves 1.250 in the
   drawer, not 5.000. Omitting the term makes expected cash exceed the drawer by the total change
   given out — hundreds of dinars on an ordinary minimarket day, from the very first close.
2. **Cash rounding carries no separate term.** The cash tender's `amount` already *is* the rounded
   amount the cashier took. Subtracting the rounding again double-counts it, and "given away" names
   only one sign when the default direction is `nearest` and roughly half of all roundings go to the
   merchant. [`tax-jordan.md`](tax-jordan.md) §5 states the true relation —
   `Σ tenders − change == total + rounding_adj` — and it is that relation, not a separate term, that
   makes this formula close.
3. **Every movement kind is a signed transfer between two locations**, so there is no kind without a
   term. The old formula enumerated three of five: a manager banking 300 JOD mid-shift created a
   300 JOD phantom shortage, and `float_add` — a different kind from `paid_in` — was unaccounted
   while the test that was supposed to cover it used `paid_in`. Kinds are not enumerated in the
   formula at all now; direction is read from `from_location_id` / `to_location_id`.
4. **A movement that does not touch this drawer does not appear.** A deposit taken from the safe to
   the bank moves the register's expected cash by nothing, which is only expressible once cash has
   locations. Where the money then is — and whether the bank got Thursday's deposit — is the safe and
   bank-in-transit balances, not this figure.

`ZReport` carries totals by tender, by tax rate, by category, **and the fraud tells**: counts of voids, refunds, price overrides, no-sale drawer opens, training transactions, and over/short (master plan C.6, E.35).

Properties: `prop_expected_cash_matches_movement_replay`,
`prop_expected_cash_equals_physical_drawer_replay` — simulate the coins in and out and assert the
formula reproduces the count, which the replay property alone does not, because a replay of a wrong
formula is order-independently wrong —
`prop_internal_tenders_never_reach_expected_cash` (§7.1),
`prop_z_totals_equal_sum_of_sales`, `prop_z_number_is_gap_free`.
Tests: `every_movement_kind_declares_its_location_pair` — an exhaustive match over the movement-kind
list, so a sixth kind cannot be added without saying where the money came from and where it went,
which is the same guarantee the old "every kind has a term" would have given had every kind had one ·
`change_given_leaves_the_drawer` ·
`a_bank_deposit_from_the_safe_does_not_move_the_drawer` (E.77) ·
`a_float_add_and_a_paid_in_are_not_the_same_movement` ·
`a_cash_rounded_sale_reconciles_without_a_rounding_term`.

---

## 12 · `stock.rs` — ledger and WAC — [1.10.x, 4.2.x]

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StockEventKind {
    Sale, RefundRestock, RefundDamage, Receive, Adjust, CountCorrection,
    TransferOut, TransferIn, Waste, Rtv, KitExplode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockEvent {
    pub id: StockEventId,
    pub product_id: ProductId,
    pub store_id: StoreId,
    pub kind: StockEventKind,
    pub qty_delta: Qty,
    /// The cost basis in force AT THIS EVENT, on EVERY kind including `Sale`.
    /// Not only on receipts: margin derived later from today's weighted average
    /// changes every time a delivery arrives, so January's reported profit moves
    /// in June and the merchant cannot reconcile their own gross margin. This is
    /// I-5 applied to cost, and the ledger is append-only, so there is no second
    /// chance to add it.
    /// `None` means no cost basis existed. A real zero is `Some(Money::zero(c))`;
    /// collapsing the two would make an unknown margin look exact.
    pub unit_cost: Option<Money>,
    /// The cost was projected rather than observed, or no basis existed. It is
    /// true whenever `unit_cost` is `None`, and may also accompany a projected
    /// non-zero value. Reported with a count, never silently zero.
    pub is_cost_estimated: bool,
    /// The stock weight was derived, not measured — a price-embedded label (§4.3).
    pub is_weight_derived: bool,
    pub ref_doc: Option<Uuid>,
    pub actor_id: Option<UserId>,
    pub occurred_at: Timestamp,
}

pub fn on_hand(events: &[StockEvent]) -> Qty;   // = Σ qty_delta

/// new_wac = (on_hand×wac + qty_in×unit_cost) / (on_hand + qty_in)
///
/// When `on_hand <= 0` the new WAC is the receipt's `unit_cost`, full stop.
/// A negative on-hand carries no cost basis to average: blending a phantom
/// negative inventory into the mean produces a figure far from any real cost,
/// and inventory valuation is an audited balance-sheet number. "Handled, not
/// panicked" is a licence to choose, and two stores choosing differently report
/// different values for the same purchase history (master plan C.7).
pub fn recompute_wac(on_hand: Qty, wac: Money, qty_in: Qty, unit_cost: Money,
                     rule: RoundingRule) -> Result<Money, StockError>;

/// Cost deviation guard — a 10× fat-fingered cost must ask (E.43).
pub fn cost_deviation_exceeds(new: Money, last: Money, tolerance: Percent) -> bool;
```

Properties: `prop_cache_rebuild_matches_ledger` · `prop_wac_never_negative` ·
`prop_wac_stable_under_zero_qty_receipt` ·
`prop_wac_is_between_the_min_and_max_cost_ever_received`. The final property constrains the
arithmetic; `prop_wac_never_negative` alone accepts any nonsense above zero.
Tests: `wac_on_zero_on_hand_takes_the_receipt_cost` ·
`wac_with_negative_on_hand_takes_the_receipt_cost` · `a_sale_event_records_the_cost_basis_at_capture_time` ·
`margin_report_is_stable_when_wac_changes_later`.
`wac_with_negative_on_hand_takes_the_receipt_cost` replaces the old
`wac_with_negative_on_hand_is_handled_not_panicked`, which asserted no behaviour.

---

## 13 · `receipt.rs` — the render model — [1.7.x]

```rust
/// Everything a receipt needs, in one pure struct. The ESC/POS rasteriser,
/// the PDF renderer, and the email renderer all consume THIS — so an emailed
/// receipt can never disagree with the printed one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReceiptModel {
    pub doc_kind: DocKind,               // Sale | Refund | XReport | ZReport
    pub watermark: Option<Watermark>,    // Duplicate | Training
    pub merchant: MerchantBlock,         // legal name, address, TIN (master plan B.6)
    /// The B2B buyer's name and TIN. The capture path is complete — a command,
    /// two `sale` columns, and a fiscal conformance rule — and the output path
    /// was not, so the one customer who explicitly asked for something got a
    /// receipt without it and could not use it for their input-tax deduction.
    pub buyer: Option<BuyerBlock>,
    pub header: ReceiptHeader,           // receipt no, register, cashier, datetime
    pub lines: Vec<ReceiptLine>,
    pub totals: ReceiptTotals,           // subtotal, discounts, tax summary BY RATE,
                                         // cash-rounding line, grand total
    pub tenders: Vec<ReceiptTender>,
    pub change: Option<Money>,
    pub loyalty: Option<LoyaltyBlock>,
    pub fiscal: Option<FiscalBlock>,     // JoFotara UUID + QR payload, once cleared
    pub footer: FooterBlock,             // return policy, thank-you, ar/en
    pub locale: ReceiptLocale,           // language, direction, money decimals
}

pub fn build_receipt_model(sale: &CompletedSale, ctx: &ReceiptContext) -> ReceiptModel;
```

Every money field on a `ReceiptModel` renders through `Money::format_exact` (§1.2). The store's
`money_decimals` governs shelf and catalogue display; it does not govern a document the customer is
handed, because a receipt whose visible rows do not add to its own total is not proof of anything.

---

## 14 · `promo.rs` — the promotions engine — [4.4.x]

Deliberately last. Manual discounts cover Phases 1–3 (master plan C.9).

```rust
/// Pure: (cart, active promotions, now, customer?) → priced cart.
/// STRICT SIMPLE STACKING, documented and never improvised:
///   • per line, the single best promotion wins — no stacking
///   • basket promotions apply after line promotions
///   • a manual discount excludes automatic ones on the same line
///     unless `settings.allow_manual_with_auto`
pub fn apply_promotions(
    cart: &Cart, promos: &[Promotion], now: Timestamp,
    customer: Option<&CustomerContext>, settings: &PromoSettings,
) -> Result<Vec<DiscountAttribution>, PromoError>;

/// Which quantity-threshold group a line's discount came from, snapshotted onto
/// the line. This is the input `RequalifyPolicy` needs (§10.1): without it a
/// refund cannot tell a "3 for 1.000" line from an ordinary discounted one, and
/// the deal-break rule has nothing to break.
///
/// It is captured at sale time, not looked up at refund time, for the same
/// reason as price and name (I-5): the promotion may have ended, or been
/// edited, by the time the customer comes back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromoGroupRef {
    pub promotion_id: PromotionId,
    pub promotion_version: u32,     // the exact terms that were applied
    pub group_key: String,          // lines sharing this key formed one qualifying group
    pub threshold_qty: Qty,
    pub group_price: Money,
}
```

Even though the engine is Phase 4, `PromoGroupRef` is captured from Phase 2, because `refund.rs`
ships in Phase 2 and the shape of a refund document is what freezes. Adding the requalification input
after the first promoted sale means rewriting refund arithmetic, `refund_line_link`, the credit-note
builder and their properties, inside the phase the ordering exists to protect.

Properties: `prop_promotions_never_increase_total`, `prop_promotion_proration_conserves_to_the_fil`, `prop_best_single_promotion_is_chosen`, `prop_promotions_are_order_independent`.

---

## 15 · Module dependency rule

```
money ─┬─→ tax ──┬─→ cart ──┬─→ tender ──→ receipt
       │         │          ├─→ refund
       │         │          └─→ promo
       ├─→ stock │
       └─→ ids ──┴─→ time
                     permissions ──→ audit
```

The diagram omits two modules that the tree at the top of this file lists. `catalog` sits beside
`tax` — it depends on `money` (since `Product` carries its price, §4.1) and on `ids`, and `cart`
depends on it. `pricing` sits between `money` and `cart`, and depends on `permissions` for the
`Authorized<C>` arguments its override and discount functions take. Neither adds a cycle, and
`just acyclic` is what establishes that rather than this diagram.

Arrows point one way. `money` depends on nothing but `rust_decimal`. Nothing depends on `receipt`. A
cycle here is a design error, and `scripts/check-domain-acyclic.py` — `just acyclic`, in `just lint`
and the `rust` CI job — fails the build on one. Not `cargo-modules`: its graph is item-level, so any
constructor returning `Self` reports as a circular dependency, and cycle detection runs before the
filters that would exclude it. The tool cannot express "modules must be acyclic".
