//! The sale's own commands (`ref/ipc-contract.md` §3, "Tender and finalize").
//!
//! **Receipt numbering (microstep 1.9.3).** A receipt number is
//! `REG01-000123`: the register's prefix, its code and a hyphen, then a
//! counter zero-padded to six digits. A central counter cannot exist offline,
//! so the prefix is what keeps two registers' numbers apart, and each
//! register's counter is its own `doc_sequence` row, advanced in the
//! transaction of the document it numbers (I-7). The counter's first number
//! fixes its prefix, so a register whose code changes keeps numbering where
//! its receipts left off ([`SequenceRepository::next_prefixed`]).
//!
//! 1.8.3's third part, the finalize, allocates each sale's number here.

use pos_db::DbError;
use pos_db::repo::sequence::{SeqKind, SequenceRepository, SequenceScope};
use pos_domain::RegisterId;
use rusqlite::Transaction;

/// The digits a receipt number's counter is zero-padded to.
pub const RECEIPT_COUNTER_DIGITS: usize = 6;

/// The prefix a register's receipts carry: its code, then a hyphen.
#[must_use]
pub fn receipt_prefix(register_code: &str) -> String {
    format!("{register_code}-")
}

/// A receipt number as it is printed and stored: `prefix`, then `value`
/// zero-padded to [`RECEIPT_COUNTER_DIGITS`]. A counter past 999 999 grows a
/// digit rather than wrap, so a number is never issued twice.
#[must_use]
pub fn receipt_number(prefix: &str, value: u64) -> String {
    format!("{prefix}{value:0width$}", width = RECEIPT_COUNTER_DIGITS)
}

/// The next receipt number of `register`, whose code is `code`, taken inside
/// `tx`, the transaction that writes the document it numbers. A rolled-back
/// transaction spends no number.
///
/// # Errors
///
/// What [`SequenceRepository::next_prefixed`] refuses: a code that makes a
/// prefix a cashier could not type back, a receipt counter that already
/// numbered without a prefix, and storage failures.
pub fn next_receipt_number(
    tx: &Transaction<'_>,
    register: RegisterId,
    code: &str,
) -> Result<String, DbError> {
    let issued = SequenceRepository::new(tx).next_prefixed(
        tx,
        SequenceScope::Register(register),
        SeqKind::Receipt,
        &receipt_prefix(code),
    )?;
    Ok(receipt_number(&issued.prefix, issued.value))
}
