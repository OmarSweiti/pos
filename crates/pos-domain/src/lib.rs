//! Pure domain logic. NO I/O, NO SQLite, NO Tauri, NO network — ever.
//! (Blueprint §2: this purity is what makes it shareable between register and server.)

pub mod catalog;
pub mod ids;
pub mod money;
pub mod time;

// The module graph points one way (ref/domain-api.md §15): `ids`, `money` and
// `time` have no cross-module edges, and `catalog` is the first module with any
// — it reads `money` for `Money`/`Qty` and `ids` for the three id types a
// product row refers to, and nothing depends on it yet. `money`'s external
// arithmetic dependency is `rust_decimal`; `time` performs only integer calendar
// arithmetic. `just acyclic` is what establishes the graph, not this comment.
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
pub use time::{
    BusinessDate, Clock, ClockAnomaly, ClockConfidence, ClockPolicy, ClockState, DayBoundary,
    FixedClock, MonotonicClock, TimeError, Timestamp, business_date_of, clock_confidence,
    effective_now,
};
