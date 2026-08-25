//! Shared machinery for tests that need the *reference* schema, not just the
//! shipped chain.
//!
//! `pos_db::open` applies migrations 0001 and 0002 — the only ones committed. Most
//! of the schema, and therefore most of its constraints and triggers, exists only
//! in `ref/schema.md` until those migrations are written. A test that opens the
//! shipped chain alone silently skips all of it, which is how the first version of
//! `fact_table_guards.rs` came to validate none of the guards it was written for.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use rusqlite::{Connection, types::Value};

/// The plan of record. Compiled in, so moving or renaming it breaks the build
/// rather than silently skipping the check.
const SCHEMA_REF: &str = include_str!("../../../../docs/implementation/ref/schema.md");

/// Every table the reference calls a fact, read from its own marker.
pub fn declared_fact_tables() -> Vec<String> {
    let line = SCHEMA_REF
        .lines()
        .find(|l| l.trim_start().starts_with("<!-- fact-tables:"))
        .expect("ref/schema.md must carry a <!-- fact-tables: ... --> marker");
    let inner = line
        .trim()
        .trim_start_matches("<!-- fact-tables:")
        .trim_end_matches("-->");
    inner
        .split(',')
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Every `” ```sql ”` block under a `## NNNN — ` heading numbered 0003 or above.
///
/// 0001 and 0002 are applied by `pos_db::open` from the committed migration
/// files, so replaying the reference's own copy of them would double-apply.
pub fn reference_blocks() -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut inside = false;
    let mut migration: Option<u32> = None;

    for line in SCHEMA_REF.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            migration = rest
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<u32>().ok());
        }
        if line.trim() == "```sql" {
            inside = true;
            current.clear();
            continue;
        }
        if inside && line.trim() == "```" {
            inside = false;
            if migration.is_some_and(|n| n >= 3) {
                blocks.push(current.join("\n"));
            }
            continue;
        }
        if inside {
            current.push(line);
        }
    }
    blocks
}

/// The shipped chain, then the whole future schema on top of it.
pub fn full_schema(dir: &tempfile::TempDir, name: &str) -> Connection {
    let conn = pos_db::open(&dir.path().join(name), "test-key").unwrap();
    // The reference declares foreign keys against tables a later block creates,
    // and seeds rows that do not satisfy them. Enforcement is asserted by
    // `durability.rs`; here it only gets in the way of reaching the triggers.
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    for (index, block) in reference_blocks().iter().enumerate() {
        conn.execute_batch(block)
            .unwrap_or_else(|e| panic!("reference block {index} does not execute: {e}"));
    }
    conn
}

/// The first value a `CHECK (col IN (...))` on a TEXT column will accept.
pub fn check_defaults(conn: &Connection, table: &str) -> Vec<(String, String)> {
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = ?1",
            [table],
            |r| r.get(0),
        )
        .unwrap();
    let mut out = Vec::new();
    for piece in sql.split("CHECK").skip(1) {
        // `piece` starts like: " (doc_type IN ('sale','refund')), ..."
        // The column name is INSIDE the parenthesis, not before it.
        let Some(open) = piece.find('(') else {
            continue;
        };
        let rest = &piece[open + 1..];
        let column: String = rest
            .chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if column.is_empty() {
            continue;
        }
        // Only an IN-list tells us a legal value. `CHECK (amount_minor > 0)` or
        // `CHECK (kind <> 'x')` must not be mistaken for one.
        let head = &rest[..rest.find(')').unwrap_or(rest.len())];
        if !head.contains(" IN ") {
            continue;
        }
        if let Some(first) = rest.split('\'').nth(1) {
            out.push((column, first.to_owned()));
        }
    }
    out
}

/// Insert one row, filling every column with something its declared type accepts.
pub fn seed(conn: &Connection, table: &str, overrides: &[(&str, Value)]) {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    let columns: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();

    let checks = check_defaults(conn, table);
    let mut values: Vec<(String, Value)> = Vec::new();
    for (name, declared) in &columns {
        let value = if let Some((_, v)) = checks.iter().find(|(c, _)| c == name) {
            Value::Text(v.clone())
        } else {
            match declared.to_uppercase().as_str() {
                "BLOB" => Value::Blob(vec![0u8; 16]),
                "TEXT" if name.ends_with("_date") => Value::Text("2026-08-25".into()),
                "TEXT" if name.ends_with("_at") => Value::Text("2026-08-25T10:00:00.000Z".into()),
                "TEXT" => Value::Text("x".into()),
                "REAL" => Value::Real(1.0),
                _ => Value::Integer(1),
            }
        };
        values.push((name.clone(), value));
    }
    for (name, value) in overrides {
        if let Some(slot) = values.iter_mut().find(|(n, _)| n == name) {
            slot.1 = value.clone();
        }
    }

    let names: Vec<&str> = values.iter().map(|(n, _)| n.as_str()).collect();
    let holders: Vec<String> = (1..=values.len()).map(|i| format!("?{i}")).collect();
    let params: Vec<&dyn rusqlite::ToSql> = values
        .iter()
        .map(|(_, v)| v as &dyn rusqlite::ToSql)
        .collect();
    conn.execute(
        &format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            names.join(","),
            holders.join(",")
        ),
        params.as_slice(),
    )
    .unwrap_or_else(|e| panic!("could not seed {table}: {e}"));
}

pub fn blob(byte: u8) -> Value {
    Value::Blob(vec![byte; 16])
}
