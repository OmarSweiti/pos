//! Microstep 1.9.3: receipt numbering.
//!
//! A receipt number is `REG01-000123`: the register's code and a hyphen, then
//! its own counter zero-padded to six digits. A central counter cannot exist
//! offline, so two registers count independently and the prefix keeps their
//! numbers apart. Each register is a fresh file built through `pos_db::open`,
//! the registered migration chain.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod shop;

use std::collections::BTreeSet;

use pos_db::fixtures::registered_chain::RegisteredChain;
use pos_domain::RegisterId;
use rusqlite::{Connection, params};
use terminal_lib::commands::sale::{next_receipt_number, receipt_number, receipt_prefix};
use terminal_lib::state::DATABASE_FILE;
use uuid::Uuid;

/// `n` numbers for `register`, whose code is `code`, each in a transaction of
/// its own that commits, as each sale's would.
fn issue(conn: &Connection, register: RegisterId, code: &str, n: usize) -> Vec<String> {
    (0..n)
        .map(|_| {
            let tx = conn.unchecked_transaction().unwrap();
            let number = next_receipt_number(&tx, register, code).unwrap();
            tx.commit().unwrap();
            number
        })
        .collect()
}

/// The shape is the register's code, a hyphen and six digits, and it does not
/// change: past 999 999 the counter grows a digit rather than wrap. A register
/// opened as `run` opens it numbers under its code, not its display name, one
/// number per committed transaction; a rolled-back one spends nothing; and a
/// code that changes later does not change the prefix its receipts carry.
#[test]
fn receipt_number_format_is_stable() {
    assert_eq!(receipt_prefix("REG01"), "REG01-");
    for (value, printed) in [
        (1, "REG01-000001"),
        (123, "REG01-000123"),
        (999_999, "REG01-999999"),
        (1_000_000, "REG01-1000000"),
    ] {
        assert_eq!(receipt_number("REG01-", value), printed);
    }

    let dir = tempfile::tempdir().unwrap();
    shop::seed(dir.path(), true);
    let conn = pos_db::open(&dir.path().join(DATABASE_FILE), shop::KEY).unwrap();
    conn.execute("UPDATE register SET name = 'Front till'", [])
        .unwrap();
    drop(conn);
    let register = shop::open(dir.path());
    assert_eq!(
        register.code(),
        "REG01",
        "the code is read with the register's row, not its display name"
    );
    register.with_db(|conn| {
        assert_eq!(
            issue(conn, register.register(), register.code(), 2),
            ["REG01-000001", "REG01-000002"]
        );

        let tx = conn.unchecked_transaction().unwrap();
        assert_eq!(
            next_receipt_number(&tx, register.register(), register.code()).unwrap(),
            "REG01-000003"
        );
        tx.rollback().unwrap();
        assert_eq!(
            issue(conn, register.register(), register.code(), 1),
            ["REG01-000003"],
            "a transaction that rolled back spent no number"
        );
        assert_eq!(
            issue(conn, register.register(), "REG07", 1),
            ["REG01-000004"],
            "a register whose code changed keeps the prefix its receipts carry"
        );
    });
}

/// Registers count on their own, so four of them reach the same counter
/// values, and their prefixes keep every number distinct. The hyphen is what
/// keeps one code's numbers out of another's: `REG1`, a million receipts in,
/// would otherwise issue `REG11000001`, which is `REG11`'s first.
#[test]
fn two_registers_never_collide() {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("two-registers.db"), shop::KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    let registers: Vec<(RegisterId, &str)> = [
        (0x01, "REG01"),
        (0x02, "REG02"),
        (0x03, "REG1"),
        (0x04, "REG11"),
    ]
    .into_iter()
    .map(|(n, code)| {
        let id = Uuid::from_u128(n);
        chain.add_register(&conn, id.as_bytes(), code);
        (RegisterId::from_uuid(id), code)
    })
    .collect();
    // `REG1` has issued a million receipts: its counter row, as its first
    // number would have left it, a million numbers ago.
    conn.execute(
        "INSERT INTO doc_sequence (scope_kind, scope_id, kind, next_value, prefix)
         VALUES ('register', ?1, 'receipt', 1000001, ?2)",
        params![
            Uuid::from_u128(0x03).as_bytes().as_slice(),
            receipt_prefix("REG1")
        ],
    )
    .unwrap();

    let mut every = BTreeSet::new();
    let mut issued = 0;
    // Interleaved, as registers sharing a store would sell.
    for _ in 0..3 {
        for (register, code) in &registers {
            for number in issue(&conn, *register, code, 4) {
                assert!(every.insert(number.clone()), "{number} was issued twice");
                issued += 1;
            }
        }
    }
    assert_eq!(every.len(), issued);
    for code in ["REG01", "REG02", "REG11"] {
        assert!(
            every.contains(&format!("{code}-000001")) && every.contains(&format!("{code}-000012")),
            "{code} counted from one to twelve, as the others did"
        );
    }
    assert!(
        every.contains("REG1-1000001") && every.contains("REG11-000001"),
        "REG1's millionth-and-first number and REG11's first are both issued, and differ"
    );
}
