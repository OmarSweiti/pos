//! Pure domain logic. NO I/O, NO SQLite, NO Tauri, NO network — ever.
//! (Blueprint §2: this purity is what makes it shareable between register and server.)

pub mod catalog;
pub mod ids;
pub mod money;
pub mod permissions;
pub mod tax;
pub mod time;

// The module graph points one way (ref/domain-api.md §15): `ids`, `money` and
// `time` have no cross-module edges. `catalog` reads `money` and `ids`; `tax`
// reads those two plus the pure `Timestamp` value from `time`; nothing depends
// on either module yet. `permissions` has no cross-module edges either while it
// holds only the capability grid; §15 gives it an edge to `ids` at microstep
// 1.6.4, when `Authorized<C>` starts carrying a `UserId`. `money`'s external
// arithmetic dependency is `rust_decimal`; `time` performs only integer
// calendar arithmetic. `just acyclic` is what establishes the graph, not this
// comment.
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
    Capability, CustomerQueryShape, Grant, GrantSet, JournalScope, Limit, PermissionError, Role,
    RoleGrants, cap, default_grants,
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
