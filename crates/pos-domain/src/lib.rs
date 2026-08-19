//! Pure domain logic. NO I/O, NO SQLite, NO Tauri, NO network — ever.
//! (Blueprint §2: this purity is what makes it shareable between register and server.)

pub mod money;

pub use money::{Money, MoneyError};
