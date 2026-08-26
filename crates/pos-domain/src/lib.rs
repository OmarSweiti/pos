//! Pure domain logic. NO I/O, NO SQLite, NO Tauri, NO network — ever.
//! (Blueprint §2: this purity is what makes it shareable between register and server.)

pub mod ids;
pub mod money;

// The module graph points one way (ref/domain-api.md §15): `ids` depends on
// nothing, `money` depends on nothing but `rust_decimal`, and neither depends on
// the other. `just acyclic` is what establishes that, not this comment.
pub use ids::{
    ApprovalId, CategoryId, CustomerId, IdSource, OrgId, ProductId, PromotionId, RegisterId,
    SaleId, SaleLineId, SeqIdSource, ShiftId, StockEventId, StoreId, TaxCategoryId, TenderId,
    UserId,
};
pub use money::{Currency, Money, MoneyError, Percent, Qty, RoundingDirection, RoundingRule};
