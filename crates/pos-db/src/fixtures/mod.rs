//! Test fixtures other crates' tests need, behind the `fixtures` feature.
//!
//! **Never product.** A fixture here writes placeholder PIN hashes, invented
//! chain hashes and fixed instants, and it panics on any refusal — exactly
//! right for a test and exactly wrong for a register. Nothing a register ships
//! enables the feature: `pos-db`'s own tests turn it on through a
//! dev-dependency on the crate itself, the terminal's tests through theirs, and
//! `the_register_never_ships_the_sale_fixture` holds the terminal's shipped
//! graph to that. A dev-dependency's features never reach a build that ships
//! (resolver 3).
//!
//! It is here rather than in `tests/common/` because a crate's `tests/`
//! directory is visible to that crate alone. The terminal's print worker
//! (1.7.7) is the first code outside `pos-db` that has to complete a sale
//! before it can test anything, and a second copy of a fixture that satisfies
//! `0005`'s seven completion gates would drift from this one the first time a
//! gate changed. The search catalogue (1.2.7) is here for the same reason
//! inside this crate: `benches/search.rs` and the test that checks what it
//! measures are separate targets, and neither can see the other's files.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod registered_chain;
pub mod search_catalogue;
