//! Test support shared by every crate in the workspace.
//!
//! **This crate is a `[dev-dependencies]` entry and nothing else.** It reads
//! the process environment, which no crate that ships to a register may do —
//! `pos-domain` is pure (I-8) and `pos-db`, `pos-sync` and `pos-hardware` take
//! their configuration as arguments. The boundary that makes the environment
//! read harmless is the dependency table it appears in: a dev-dependency is
//! compiled for test, bench and example targets only and is absent from the
//! binary a merchant installs. Add this crate to a `[dependencies]` table and
//! that boundary is gone.
//!
//! Today it holds one thing, and the reason it exists at all is that the thing
//! must be decided **once**: see [`proptest`] for the case count, the recorded
//! seed and the failure-persistence policy that every property test in the
//! workspace inherits. Microstep 1.1.0 builds it before the first property so
//! that no property can pick a case count that makes itself convenient.

pub mod proptest;

pub use crate::proptest::{
    CASES_ENV, CaseOverrideError, DISABLE_PERSISTENCE_ENV, DOMAIN_CASES, IO_CASES, RECORDED_SEED,
    REGRESSIONS_DIR, domain_proptest_config, io_proptest_config, resolve_cases,
};
