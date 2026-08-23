# `pos-domain` — the complete public API

The crown jewel. Pure, no I/O, shared by register and server so the two can never disagree about what a total is.

This document is the **target shape**, assembled across Phases 1–4. Each item is annotated with the microstep that creates it. Signatures here are normative: if a phase file shows something different, this file wins.

**Purity is enforced mechanically.** `pos-domain/Cargo.toml` may depend only on `serde`,
`thiserror`, `uuid`, `rust_decimal`, and (dev) `proptest`, `criterion`. `uuid` is an
identity/serialization type here: its default and version-generation features stay disabled,
and generated IDs are injected by the shell. Adding anything capable of I/O, clock access, or
randomness is a design review, not a commit. `scripts/check-domain-purity.py` audits the resolved
normal dependency features and direct call sites.

```
crates/pos-domain/src/
├── lib.rs           module tree + re-exports
├── money.rs         Money, Currency, Qty, Percent      [exists, extended 1.1.x]
├── ids.rs           typed ids, Clock & IdSource ports  [1.1.8]
├── time.rs          BusinessDate, day cutover          [1.1.9]
├── catalog.rs       Product, Barcode, price-embedded   [1.2.x]
├── tax.rs           categories, rates, extraction      [1.3.x]
├── cart.rs          the state machine                  [1.4.x]
├── pricing.rs       discounts, overrides, proration    [1.4.x]
├── tender.rs        tenders, change, cash rounding     [1.5.x]
├── receipt.rs       ReceiptModel (render input)        [1.7.x]
├── permissions.rs   capability strings + Authorized<C> [1.6.x]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

### 1.2 `Money` — extended [1.1.2]

Existing methods (`from_minor`, `minor`, `checked_add`, `checked_sub`, `split_evenly`, `ZERO`) keep their behaviour but gain currency. `split_evenly`'s largest-remainder implementation and its property test are already correct — do not rewrite them, only thread `Currency` through.

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

    /// Round to a coin step (cash rounding, master plan B.5).
    /// step_minor = 10 for the 1-qirsh default.
    pub fn round_to_step(self, step_minor: i64, dir: RoundingDirection) -> Result<Money, MoneyError>;

    /// Exact conversion for intermediate math. NEVER for storage or display.
    pub fn to_decimal(self) -> Decimal;
    pub fn from_decimal(d: Decimal, c: Currency, rule: RoundingRule) -> Result<Money, MoneyError>;

    /// Display precision is a STORE SETTING (B.5): JOD shows 2 or 3 decimals.
    /// Storage is always fils.
    pub fn format(self, decimals: u8) -> String;
    pub fn parse(s: &str, c: Currency) -> Result<Money, MoneyError>;
}
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoundingRule { HalfAwayFromZero, HalfEven, Floor, Ceil }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoundingDirection { Nearest, Up, Down }
```

> **Default is `HalfAwayFromZero`, not banker's rounding.** The blueprint suggests banker's; retail tax practice in the region and the arithmetic a merchant's accountant will do by hand both expect half-away-from-zero. It is a setting either way, but the default a merchant never changes must be the one that matches their hand-check.

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
    pub fn from_percent_decimal(d: Decimal) -> Result<Percent, MoneyError>;
    pub fn to_decimal(self) -> Decimal;      // 160_000 -> 0.16
    pub fn format(self) -> String;           // "16%", "0.5%"
}
```

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
    #[error("value out of representable range")]          OutOfRange,
}
```

### 1.6 Properties — [1.1.5]

| Test | Invariant |
|---|---|
| `prop_split_preserves_total` | *(exists)* Σ parts == original; parts differ by ≤ 1 |
| `prop_split_proportional_preserves_total` | Σ parts == original for **any** weights; a zero weight gets zero |
| `prop_add_sub_roundtrip` | *(exists)* `(a+b)-b == a` |
| `prop_currency_mismatch_never_silently_coerces` | Mixed-currency ops always `Err`, never a wrong number |
| `prop_round_to_step_is_idempotent` | Rounding an already-rounded amount changes nothing |
| `prop_round_to_step_within_half_step` | `\|rounded − original\| ≤ step/2` for `Nearest` |
| `prop_format_parse_roundtrip` | `parse(format(m, e)) == m` at the currency's own exponent |
| `prop_mul_qty_whole_units_is_repeated_add` | `price.mul_qty(n units) == price + … + price` — the sanity check a cashier would do |

---

## 2 · `ids.rs` — typed ids and the two ports — [1.1.8]

Purity means `pos-domain` cannot call `Uuid::now_v7()` or read a clock. Both are injected.

```rust
macro_rules! typed_id { … }   // newtype over Uuid, Copy, Serialize, Display

typed_id!(SaleId); typed_id!(SaleLineId); typed_id!(ProductId); typed_id!(StoreId);
typed_id!(RegisterId); typed_id!(UserId); typed_id!(ShiftId); typed_id!(CustomerId);
typed_id!(TenderId); typed_id!(TaxCategoryId); typed_id!(PromotionId); typed_id!(StockEventId);

/// Injected so domain functions stay pure and tests stay deterministic.
pub trait IdSource { fn next(&self) -> Uuid; }        // UUIDv7 in production
pub trait Clock     { fn now(&self) -> Timestamp; }   // UTC

/// Deterministic test doubles — live in the crate, not behind #[cfg(test)],
/// so the server and integration tests can use them too.
pub struct SeqIdSource { … }    // v7-shaped, counter-driven, reproducible
pub struct FixedClock  { … }
```

**Why typed ids.** `fn refund(sale: SaleId, line: SaleLineId)` cannot be called with the arguments swapped. Over a schema with fourteen id columns, that is worth the boilerplate.

---

## 3 · `time.rs` — business date — [1.1.9] *(gap G-4)*

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp(i64);      // UTC milliseconds since epoch

impl Timestamp {
    pub fn to_iso8601(self) -> String;                 // 2026-08-20T07:15:22.418Z
    pub fn parse_iso8601(s: &str) -> Result<Timestamp, TimeError>;
}

/// A store-local trading day. NOT a wall-clock date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BusinessDate { y: i16, m: u8, d: u8 }

impl BusinessDate {
    pub fn to_iso(self) -> String;                     // 2026-08-20
    pub fn parse(s: &str) -> Result<BusinessDate, TimeError>;
    pub fn succ(self) -> BusinessDate;
}

#[derive(Debug, Clone, Copy)]
pub struct DayBoundary {
    pub utc_offset_minutes: i16,     // Asia/Amman: +180 (or +120 outside DST)
    pub cutover_minutes: u16,        // minutes past local midnight; default 240 = 04:00
}

/// A shift opened at 00:30 local belongs to YESTERDAY's trading day.
pub fn business_date_of(opened_at: Timestamp, b: DayBoundary) -> BusinessDate;

/// Monotonic guard (E.6): never emit a timestamp before the last one seen.
pub struct MonotonicClock<C: Clock> { … }
impl<C: Clock> MonotonicClock<C> {
    pub fn now(&mut self) -> (Timestamp, Option<ClockAnomaly>);
}
pub struct ClockAnomaly { pub jumped_back_by_ms: i64 }   // → audit entry
```

> **Timezone handling deliberately does not pull in a tz database.** The store stores a fixed UTC offset and a DST rule chosen at configuration time, refreshed from the server. `pos-domain` receiving a *resolved offset* keeps it pure; resolving Asia/Amman → offset happens in the shell with `jiff`.

Properties: `prop_business_date_stable_across_shift`, `prop_cutover_boundary_never_skips_a_day`, `prop_monotonic_clock_never_decreases`.

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
    pub is_weighed: bool,
    pub is_service: bool,          // e-recharge, fees — no stock events (J.1)
    pub is_active: bool,
    pub min_age: Option<u8>,       // age-restricted (J.1, E.69)
    pub max_price_minor: Option<i64>, // ministry price ceiling (J.3, E.71)
    pub reorder_point_milli: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitOfMeasure { Each, Kilogram, Gram, Litre, Millilitre, Metre, Package }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BarcodeKind { Ean13, Ean8, Upca, Code128, Internal, PriceEmbedded, WeightEmbedded }
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

#[derive(Debug, Clone, PartialEq)]
pub enum ScanResult {
    Plain { code: String },
    Embedded { item_code: String, value: EmbeddedAmount },
}

pub fn parse_scan(code: &str, rules: &[EmbeddedBarcodeRule]) -> Result<ScanResult, ScanError>;
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
    pub rate: Percent,
    pub is_inclusive_capable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriceMode { Inclusive, Exclusive }     // Jordan default: Inclusive

/// The store's jurisdiction profile (master plan B.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoreTaxProfile { Standard, Asez, DevelopmentArea, Unregistered }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineTax {
    pub net: Money,
    pub components: Vec<ComponentTax>,   // { code, rate, amount }
    pub tax_total: Money,
    pub gross: Money,
}

/// THE function. Everything about tax is here.
///   Inclusive: net = gross / (1+r);  tax = gross − net
///   Exclusive: tax = net × r;        gross = net + tax
/// Computed in rust_decimal, rounded ONCE per line by `rule`, returned as fils.
pub fn compute_line_tax(
    taxable: Money,          // line gross (inclusive) or net (exclusive), post-discount
    mode: PriceMode,
    components: &[TaxComponent],
    profile: StoreTaxProfile,
    rule: RoundingRule,
) -> Result<LineTax, TaxError>;

/// Receipt tax summary, grouped by rate. This is the EXACT SUM of line taxes.
/// It is never re-derived from the total — that is how JoFotara total checks fail
/// (master plan C.3, and correction C-3 in plan-validation.md).
pub fn summarize_tax(lines: &[LineTax]) -> Vec<TaxSummaryRow>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaxSummaryRow {
    pub code: String, pub treatment: TaxTreatment, pub rate: Percent,
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
    pub rate: Percent,
    pub valid_from: Timestamp,
    pub valid_to: Option<Timestamp>,
    pub profile_scope: Option<StoreTaxProfile>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TaxError {
    #[error("no rate rule for category at {0:?}")]  NoRuleInEffect(Timestamp),
    #[error("overlapping rate rules for {0}")]      OverlappingRules(String),
    #[error("inclusive pricing with a component that cannot be inclusive")] NotInclusiveCapable,
    #[error(transparent)]                           Money(#[from] MoneyError),
}
```

Properties — the ones that matter most in the whole crate:

| Test | Invariant |
|---|---|
| `prop_inclusive_net_plus_tax_equals_gross` | Exactly, at fils, for every rate and amount |
| `prop_line_tax_sum_equals_receipt_tax` | Σ line taxes == summary total, **exactly** |
| `prop_exempt_and_zero_produce_zero_tax_but_differ_in_reporting` | Exempt ≠ zero-rated — they must not collapse (master plan B.1) |
| `prop_tax_never_exceeds_gross` | |
| `prop_rate_resolution_is_deterministic_at_boundaries` | `valid_from` inclusive, `valid_to` exclusive, no gap, no overlap |
| `prop_unregistered_profile_yields_no_tax` | The tax-disabled merchant configuration (C-4) |

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
    pub is_training: bool,              // checked EVERYWHERE, incl. the fiscal queue
    pub opened_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartLine {
    pub id: SaleLineId,
    pub product_id: ProductId,
    pub name_snapshot: String,          // I-5: copied at capture, never re-read
    pub unit_price: Money,              // I-5
    pub qty: Qty,
    pub is_weighed: bool,
    pub tax_category_id: TaxCategoryId,
    pub discounts: Vec<LineDiscount>,
    pub price_override: Option<PriceOverride>,
    pub age_confirmed: bool,
}
```

### 6.1 Transitions — [1.4.3 … 1.4.8]

Every one is a free function taking the state by value and returning the next state or an error. No method mutates in place; no transition is reachable from the wrong state because the wrong state is a different type.

```rust
pub fn open(id: SaleId, ctx: &CartContext) -> Cart;

pub fn add_line(cart: Cart, req: AddLine)          -> Result<Cart, CartError>;
pub fn set_qty(cart: Cart, line: SaleLineId, q: Qty) -> Result<Cart, CartError>;
pub fn void_line(cart: Cart, line: SaleLineId, reason: VoidReason, by: UserId)
                                                    -> Result<(Cart, AuditIntent), CartError>;
pub fn apply_line_discount(cart: Cart, line: SaleLineId, d: DiscountRequest,
                           auth: &Authorized<{cap::DISCOUNT_MANUAL}>)
                                                    -> Result<(Cart, AuditIntent), CartError>;
pub fn apply_basket_discount(cart: Cart, d: DiscountRequest,
                           auth: &Authorized<{cap::DISCOUNT_MANUAL}>)
                                                    -> Result<(Cart, AuditIntent), CartError>;
pub fn override_price(cart: Cart, line: SaleLineId, to: Money, reason: OverrideReason,
                      auth: &Authorized<{cap::PRICE_OVERRIDE}>)
                                                    -> Result<(Cart, AuditIntent), CartError>;
pub fn attach_customer(cart: Cart, c: CustomerId)   -> Result<Cart, CartError>;
pub fn set_buyer_tin(cart: Cart, tin: String)       -> Result<Cart, CartError>;

pub fn park(cart: Cart)                              -> Result<Sale, CartError>;
pub fn resume(parked: Cart)                          -> Result<Cart, CartError>;

pub fn begin_tender(cart: Cart, priced: PricedCart)  -> Result<Tendering, CartError>;
pub fn back_to_building(t: Tendering)                -> Result<Cart, CartError>; // only if no tender collected
pub fn add_tender(t: Tendering, tender: Tender)      -> Result<Tendering, CartError>;
pub fn remove_tender(t: Tendering, id: TenderId)     -> Result<Tendering, CartError>;
pub fn begin_finalize(t: Tendering)                  -> Result<Finalizing, CartError>;
pub fn complete(f: Finalizing, effects: FinalizeEffects) -> Result<CompletedSale, CartError>;

pub fn void_sale(sale: Sale, reason: VoidReason,
                 auth: &Authorized<{cap::SALE_VOID}>)
                                                     -> Result<(VoidedSale, AuditIntent), CartError>;
```

> **`AuditIntent`.** A pure domain function cannot write an audit row — that is I/O. It returns the *intent*: actor, approver, action, entity, and a canonical payload. The shell writes it inside the same transaction as the effect. This is how "every ✓ that reverses money writes the audit log" (master plan C.10) becomes structural rather than remembered.

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

`DiscountAttribution` is not an optimisation. Campaign cost reporting (master plan C.9) and JoFotara's per-line discount requirement (correction **C-2**) both read it. A basket discount that has not been attributed to lines cannot be turned into a fiscal document at all.

### 6.3 `CartError` — [1.4.2]

```rust
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CartError {
    #[error("no line {0}")]                                    LineNotFound(SaleLineId),
    #[error("cannot modify a completed sale")]                 SaleIsComplete,     // I-4
    #[error("cannot go back to building: {0} tender(s) collected")] TenderCollected(usize),
    #[error("quantity must be positive")]                      NonPositiveQty,
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
| `prop_price_cart_is_deterministic` | Same inputs → byte-identical output |
| `prop_price_cart_under_16ms` | `criterion`, 200 lines (conventions §7) |
| `prop_park_resume_roundtrip_is_identity` | E.3 |
| `prop_zero_total_cart_is_valid` | E.18 — 100% discount is a legal sale |

---

## 7 · `tender.rs` — payment collection — [1.5.x]

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TenderType {
    pub code: String,                 // "cash", "card", "voucher", "cliq", "store_credit"
    pub opens_drawer: bool,
    pub allows_change: bool,
    pub is_cash_counted: bool,        // counts toward expected drawer cash
    pub refundable_to: RefundRouting,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TenderState { Collected, Pending, Reversed }   // Pending: CliQ callback lost (E.65)

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
```

Properties: `prop_split_tender_sums_to_total`, `prop_cash_rounding_only_on_final_cash_tender` (E.14), `prop_rounding_adjustment_keeps_total_exact`, `prop_change_never_negative`, `prop_card_tender_never_exceeds_remaining_due`.

---

## 8 · `permissions.rs` — capabilities with teeth — [1.6.x] *(gap G-6)*

"RBAC enforced in Rust, not in the UI" needs a mechanism, or the twentieth command ships without a check.

```rust
pub mod cap {
    pub const SALE_CREATE: &str        = "sale.create";
    pub const SALE_VOID: &str          = "sale.void";
    pub const LINE_VOID: &str          = "line.void";
    pub const DISCOUNT_MANUAL: &str    = "discount.manual";
    pub const PRICE_OVERRIDE: &str     = "price.override";
    pub const REFUND_RECEIPTED: &str   = "refund.receipted";
    pub const REFUND_ABOVE_THRESHOLD: &str = "refund.above_threshold";
    pub const REFUND_RECEIPTLESS: &str = "refund.receiptless";
    pub const REFUND_CASH_FOR_CARD: &str = "refund.cash_for_card";
    pub const DRAWER_OPEN: &str        = "drawer.open";
    pub const CASH_MOVEMENT: &str      = "cash.movement";
    pub const SHIFT_OPEN: &str         = "shift.open";
    pub const SHIFT_CLOSE: &str        = "shift.close";
    pub const ZREPORT_RUN: &str        = "zreport.run";
    pub const PRODUCT_EDIT: &str       = "product.edit";
    pub const TRAINING_TOGGLE: &str    = "training_mode.toggle";
    pub const SETTINGS_EDIT: &str      = "settings.edit";
    pub const USER_ADMIN: &str         = "user.admin";
    pub const REPORTS_ALL: &str        = "reports.all";

    pub const ALL: &[&str] = &[ /* every constant above */ ];
}

/// A proof-carrying token. Constructing one is the ONLY way to get a
/// `&Authorized<C>`, and domain functions that reverse money REQUIRE one.
/// You cannot forget the check, because you cannot call the function without it.
pub struct Authorized<const C: &'static str> {
    pub actor: UserId,
    pub approver: Option<UserId>,     // distinct on escalation (E.52)
    pub at: Timestamp,
}

pub fn authorize<const C: &'static str>(
    actor: UserId, grants: &GrantSet, approver: Option<(UserId, &GrantSet)>,
    policy: &EscalationPolicy, at: Timestamp,
) -> Result<Authorized<C>, PermissionError>;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PermissionError {
    #[error("{0} lacks {1}")]                              Denied(UserId, &'static str),
    #[error("{0} requires manager approval")]              EscalationRequired(&'static str),
    #[error("self-approval is not permitted for {0}")]     SelfApprovalBanned(&'static str), // E.52
    #[error("user {0} is deactivated")]                    UserInactive(UserId),
    #[error("offline authorization window expired")]       OfflineAuthExpired,               // E.55
}
```

**The exhaustiveness test** (`ipc_commands_all_declare_a_capability`, microstep 1.6.7) walks the IPC command registry and fails if any command has no capability entry. Adding a command without declaring one breaks CI.

---

## 9 · `audit.rs` — hash chain — [1.6.x] *(gap G-7)*

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditIntent {
    pub actor: UserId,
    pub approver: Option<UserId>,
    pub action: &'static str,          // "sale.void", "price.override"
    pub entity: &'static str,
    pub entity_id: Uuid,
    pub reason: Option<String>,
    pub payload: serde_json::Value,    // NEVER PII, NEVER card data (conventions §12)
    pub at: Timestamp,
}

/// Canonical serialization: JSON with keys sorted, no whitespace, UTF-8.
/// The hash is only reproducible if this is byte-stable, so it is pinned
/// by a golden test, not left to serde_json's default ordering.
pub fn canonical_bytes(intent: &AuditIntent) -> Vec<u8>;

/// hash = BLAKE3(prev_hash ‖ canonical_bytes(intent))
pub fn chain_hash(prev: &[u8; 32], intent: &AuditIntent) -> [u8; 32];

pub const GENESIS: [u8; 32] = [0u8; 32];

#[derive(Debug, PartialEq)]
pub enum ChainVerdict { Intact { entries: u64 }, Broken { at_seq: u64 } }

pub fn verify_chain<'a>(entries: impl Iterator<Item = (&'a AuditIntent, &'a [u8;32], &'a [u8;32])>)
    -> ChainVerdict;
```

**On a broken chain the register does not stop selling.** It raises an alarm, records the break, and surfaces it in back-office device health. A tamper-evidence mechanism that halts trade converts a forensic signal into an outage.

Properties: `prop_chain_detects_any_single_entry_mutation`, `prop_chain_detects_deletion`, `prop_chain_detects_reordering`, `golden_canonical_bytes_are_stable`.

---

## 10 · `refund.rs` — refundable balances — [2.3.x]

The anti-abuse core (master plan C.5).

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefundableLine {
    pub original_line_id: SaleLineId,
    pub product_id: ProductId,
    pub name_snapshot: String,
    pub sold_qty: Qty,
    pub already_refunded_qty: Qty,
    pub remaining_qty: Qty,           // sold − already_refunded
    pub unit_price: Money,            // ORIGINAL price incl. its discounts (E.34)
    pub unit_tax: LineTax,
}

pub fn refundable_lines(original: &CompletedSale, prior: &[CompletedSale])
    -> Result<Vec<RefundableLine>, RefundError>;

pub fn build_refund(
    original: &CompletedSale, req: &RefundRequest,
    auth: &Authorized<{cap::REFUND_RECEIPTED}>, ctx: &RefundContext,
) -> Result<RefundDocument, RefundError>;

/// Cards refund to the original card via the PSP against `psp_ref`.
/// Cash-for-card is a separate capability with a threshold —
/// it is a money-laundering vector (master plan C.5).
pub fn route_refund_tenders(original: &[Tender], amount: Money, policy: &RefundPolicy)
    -> Result<Vec<RefundTenderPlan>, RefundError>;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RefundError {
    #[error("refund of {0} exceeds remaining refundable {1}")] ExceedsRefundable(String, String),
    #[error("original sale not found")]                        OriginalNotFound,
    #[error("original sale was voided")]                       OriginalVoided,
    #[error("return window of {0} days has expired")]          WindowExpired(u16),
    #[error("cash refund for a card sale requires {0}")]       CashForCardNotPermitted(&'static str),
    #[error("training-mode sales cannot be refunded")]         TrainingSale,
    #[error(transparent)] Money(#[from] MoneyError),
}
```

**The invariant that must never break:** `prop_cumulative_refunds_never_exceed_sold_qty` — across *any* sequence of partial refunds, in any order, including refunds of exchanges (E.30, E.16).

---

## 11 · `shift.rs` — cash accountability — [2.4.x]

```rust
pub fn expected_cash(s: &ShiftTotals) -> Result<Money, MoneyError>;
// float + cash tenders − cash refunds − cash rounding given away
//       + paid_ins − paid_outs − drops        (master plan C.6)

pub fn over_short(expected: Money, counted: Money) -> Result<Money, MoneyError>;

pub fn build_z_report(shift: &Shift, sales: &[CompletedSale], movements: &[CashMovement],
                      z_number: u64) -> Result<ZReport, ShiftError>;
```

`ZReport` carries totals by tender, by tax rate, by category, **and the fraud tells**: counts of voids, refunds, price overrides, no-sale drawer opens, training transactions, and over/short (master plan C.6, E.35).

Properties: `prop_expected_cash_matches_movement_replay`, `prop_z_totals_equal_sum_of_sales`, `prop_z_number_is_gap_free`.

---

## 12 · `stock.rs` — ledger and WAC — [1.10.x, 4.2.x]

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StockEventKind {
    Sale, RefundRestock, RefundDamage, Receive, Adjust, CountCorrection,
    TransferOut, TransferIn, Waste, Rtv, KitExplode,
}

pub fn on_hand(events: &[StockEvent]) -> Qty;   // = Σ qty_delta

/// new_wac = (on_hand×wac + qty_in×unit_cost) / (on_hand + qty_in)
/// Guards divide-by-zero and negative on-hand (master plan C.7).
pub fn recompute_wac(on_hand: Qty, wac: Money, qty_in: Qty, unit_cost: Money,
                     rule: RoundingRule) -> Result<Money, StockError>;

/// Cost deviation guard — a 10× fat-fingered cost must ask (E.43).
pub fn cost_deviation_exceeds(new: Money, last: Money, tolerance: Percent) -> bool;
```

Properties: `prop_on_hand_equals_ledger_sum`, `prop_wac_never_negative`, `prop_wac_stable_under_zero_qty_receipt`, `prop_cache_rebuild_matches_ledger` (I-6).

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
```

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

Arrows point one way. `money` depends on nothing but `rust_decimal`. Nothing depends on `receipt`. A cycle here is a design error; `cargo-modules` in CI (microstep 1.0.4) fails the build on one.
