//! Pure domain logic. NO I/O, NO SQLite, NO Tauri, NO network — ever.
//! (Blueprint §2: this purity is what makes it shareable between register and server.)

pub mod audit;
pub mod catalog;
pub mod ids;
pub mod money;
pub mod permissions;
pub mod tax;
pub mod time;

// The module graph points one way (ref/domain-api.md §15): `ids`, `money` and
// `time` have no cross-module edges. `catalog` reads `money` and `ids`; `tax`
// reads those two plus the pure `Timestamp` value from `time`; `audit` reads
// `ids` and `time`. Nothing depends on any of them yet. `permissions` gained its
// edges at microstep 1.6.4, when `Authorized<C>` and `ApprovalHandle` started
// carrying a `UserId` and a `Timestamp`: the checker reports `ids` and `time`,
// not the `ids` and `audit` §15 predicted — the handle names an approver and an
// expiry, and reaches for no audit type to do it. `money`'s external arithmetic dependency is
// `rust_decimal`; `time` performs only integer calendar arithmetic. `just
// acyclic` is what establishes the graph, not this comment.
pub use audit::{
    AuditError, AuditIntent, CanonicalAuditEntry, ChainAnchor, ChainVerdict, canonical_bytes,
    chain_hash, check_payload, verify_chain,
};
pub use catalog::{
    Barcode, BarcodeKind, CatalogError, Product, RegulatedKind, RegulatedSaleForm, SaleForm,
    UnitOfMeasure,
};
pub use ids::{
    ApprovalId, CategoryId, CustomerId, IdSource, OrgId, ProductId, PromotionId, RegisterId,
    SaleId, SaleLineId, SeqIdSource, ShiftId, StockEventId, StoreId, TaxCategoryId, TenderId,
    UserId,
};
pub use money::{Currency, Money, MoneyError, Percent, Qty, RoundingDirection, RoundingRule};
pub use permissions::{
    ApprovalBinding, ApprovalHandle, Authorized, Capability, CustomerQueryShape, EscalationPolicy,
    Grant, GrantSet, JournalScope, Limit, PermissionError, PreparedIntentHash, Role, RoleGrants,
    StoredApprovalHandle, authorize, cap, default_grants,
};
pub use tax::{
    ComponentTax, LineTax, PriceMode, StoreTaxProfile, SupplyDestination, SupplyTaxContext,
    TaxBase, TaxBasis, TaxComponent, TaxError, TaxRateRule, TaxSummaryRow, TaxTreatment,
    ZeroRatingReason,
};
pub use time::{
    BusinessDate, Clock, ClockAnomaly, ClockConfidence, ClockPolicy, ClockState, DayBoundary,
    FixedClock, MonotonicClock, TimeError, Timestamp, business_date_of, clock_confidence,
    effective_now,
};
