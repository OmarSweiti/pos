//! What each fuzz target asserts of an input, kept out of the targets so
//! `pos-domain`'s own tests can replay a committed corpus through the same
//! code (microstep 1.2.8).

pub mod parse_scan;
