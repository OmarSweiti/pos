//! Registered-seed correspondence for the default role matrix.
//!
//! A complete domain grid does not prove that migration `0004` wrote the same
//! answers. Every fixture here therefore starts at a fresh path opened directly
//! by `pos_db::open`, so the comparison observes the application's registered
//! migration chain. `tests/common/mod.rs`'s `full_schema` also starts there now,
//! but then disables foreign keys and layers unshipped reference blocks `0005+`
//! on top; that broader, weaker fixture is not evidence about the shipped seed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use pos_domain::permissions::{Capability, Grant, Limit, Role, cap};
use rusqlite::{Connection, params};

const KEY: &str = "test-key";
const PROBE_CAPABILITY: &str = "matrix.probe";

struct TestDb {
    conn: Connection,
    _dir: tempfile::TempDir,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Cell {
    role: String,
    capability: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct StoredGrant {
    decision: String,
    limit_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum MatrixMismatch {
    Missing {
        cell: Cell,
        expected: StoredGrant,
    },
    Unexpected {
        cell: Cell,
        found: StoredGrant,
    },
    Different {
        cell: Cell,
        expected: StoredGrant,
        found: StoredGrant,
    },
}

fn fresh_database(name: &str) -> TestDb {
    let dir = tempfile::tempdir().expect("the test needs a private database directory");
    let conn = pos_db::open(&dir.path().join(name), KEY)
        .expect("the registered migration chain must open a fresh database");
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("the registered schema version must be readable");
    assert_eq!(
        version,
        pos_db::SCHEMA_VERSION,
        "the fixture must contain the whole registered migration chain"
    );
    // The connection drops before its directory, which matters on Windows where
    // an open database file cannot be removed with its temporary directory.
    TestDb { conn, _dir: dir }
}

fn cell(role: Role, capability: &str) -> Cell {
    Cell {
        role: role.as_str().to_owned(),
        capability: capability.to_owned(),
    }
}

fn stored_grant(decision: &str, limit_json: Option<&str>) -> StoredGrant {
    StoredGrant {
        decision: decision.to_owned(),
        limit_json: limit_json.map(str::to_owned),
    }
}

fn encoded_limit(limit: Limit) -> String {
    format!(r#"{{"kind":"{}"}}"#, limit.as_str())
}

fn expected_grant(grant: Grant) -> StoredGrant {
    match grant {
        Grant::Held => stored_grant("granted", None),
        Grant::HeldWithin(limit) => StoredGrant {
            decision: "granted".to_owned(),
            limit_json: Some(encoded_limit(limit)),
        },
        Grant::Withheld => stored_grant("withheld", None),
        Grant::SetsTheLimit => stored_grant("sets_the_limit", None),
    }
}

fn domain_cell(role: Role, capability: &str) -> StoredGrant {
    let grants = cap::DEFAULT_MATRIX
        .iter()
        .find_map(|(name, grants)| (*name == capability).then_some(*grants))
        .unwrap_or_else(|| panic!("`{capability}` is not in cap::DEFAULT_MATRIX"));
    expected_grant(grants.for_role(role))
}

fn mismatches(conn: &Connection) -> rusqlite::Result<Vec<MatrixMismatch>> {
    let mut expected = BTreeMap::new();
    for &(capability, grants) in cap::DEFAULT_MATRIX {
        for role in Role::ALL {
            let cell = cell(role, capability);
            let previous = expected.insert(cell.clone(), expected_grant(grants.for_role(role)));
            assert!(
                previous.is_none(),
                "the domain matrix declares the same cell more than once: {cell:?}"
            );
        }
    }
    assert_eq!(
        expected.len(),
        cap::DEFAULT_MATRIX.len() * Role::ALL.len(),
        "a duplicate capability wire name collapsed cells in the domain grid"
    );

    // Resolving through an inner join would make a corrupt, orphaned role id
    // disappear before the comparison had a chance to reject it.
    let mut statement = conn.prepare(
        "SELECT hex(rc.role_id), r.code, rc.capability, rc.decision, rc.limit_json
           FROM role_capability AS rc
           LEFT JOIN role AS r ON r.id = rc.role_id",
    )?;
    let rows = statement.query_map([], |row| {
        let role_id: String = row.get(0)?;
        let role: Option<String> = row.get(1)?;
        Ok((
            Cell {
                role: role.unwrap_or_else(|| format!("<unresolved role_id={role_id}>")),
                capability: row.get(2)?,
            },
            StoredGrant {
                decision: row.get(3)?,
                limit_json: row.get(4)?,
            },
        ))
    })?;

    let mut actual = BTreeMap::new();
    let mut actual_row_count = 0;
    for row in rows {
        let (cell, grant) = row?;
        actual_row_count += 1;
        assert!(
            actual.insert(cell.clone(), grant).is_none(),
            "multiple database rows resolved to the same cell: {cell:?}"
        );
    }
    assert_eq!(
        actual.len(),
        actual_row_count,
        "no database row may disappear while the actual matrix is indexed"
    );

    let mut found = Vec::new();
    for (cell, expected_grant) in &expected {
        match actual.get(cell) {
            None => found.push(MatrixMismatch::Missing {
                cell: cell.clone(),
                expected: expected_grant.clone(),
            }),
            Some(actual_grant) if actual_grant != expected_grant => {
                found.push(MatrixMismatch::Different {
                    cell: cell.clone(),
                    expected: expected_grant.clone(),
                    found: actual_grant.clone(),
                });
            }
            Some(_) => {}
        }
    }
    for (cell, actual_grant) in &actual {
        if !expected.contains_key(cell) {
            found.push(MatrixMismatch::Unexpected {
                cell: cell.clone(),
                found: actual_grant.clone(),
            });
        }
    }
    found.sort();
    Ok(found)
}

#[test]
fn seeded_role_capability_rows_equal_the_domain_default_matrix() -> rusqlite::Result<()> {
    let baseline = fresh_database("baseline.db");
    let baseline_mismatches = mismatches(&baseline.conn)?;
    assert!(
        baseline_mismatches.is_empty(),
        "migration 0004's seed must match the domain matrix before sensitivity probes run: \
         {baseline_mismatches:#?}"
    );

    let added = fresh_database("added.db");
    let inserted_parent = added.conn.execute(
        "INSERT INTO capability (code, description) VALUES (?1, ?2)",
        params![PROBE_CAPABILITY, "Role-matrix sensitivity probe"],
    )?;
    assert_eq!(
        inserted_parent, 1,
        "the unexpected cell needs a valid parent capability"
    );
    let inserted_cell = added.conn.execute(
        "INSERT INTO role_capability (role_id, capability, decision, limit_json)
         SELECT id, ?1, 'withheld', NULL FROM role WHERE code = ?2",
        params![PROBE_CAPABILITY, Role::Cashier.as_str()],
    )?;
    assert_eq!(
        inserted_cell, 1,
        "the extra role-capability row must exist before testing its detection"
    );
    let probe_cell = cell(Role::Cashier, PROBE_CAPABILITY);
    let probe_grant = stored_grant("withheld", None);
    assert_eq!(
        mismatches(&added.conn)?,
        vec![MatrixMismatch::Unexpected {
            cell: probe_cell.clone(),
            found: probe_grant.clone(),
        }],
        "an extra pair outside the domain grid must be reported by name"
    );

    // Returning the row count to its original value makes the next assertion
    // evidence about key-set equality rather than arithmetic.
    let count_neutral_missing = cell(Role::Owner, cap::BackupRestore::NAME);
    let count_neutral_expected = domain_cell(Role::Owner, cap::BackupRestore::NAME);
    let deleted_for_balance = added.conn.execute(
        "DELETE FROM role_capability
          WHERE role_id = (SELECT id FROM role WHERE code = ?1)
            AND capability = ?2",
        params![Role::Owner.as_str(), cap::BackupRestore::NAME],
    )?;
    assert_eq!(
        deleted_for_balance, 1,
        "the count-neutral probe must remove one genuine cell"
    );
    let row_count: i64 =
        added
            .conn
            .query_row("SELECT count(*) FROM role_capability", [], |row| row.get(0))?;
    let derived_count = i64::try_from(cap::DEFAULT_MATRIX.len() * Role::ALL.len())
        .expect("the domain matrix count fits SQLite's integer type");
    assert_eq!(
        row_count, derived_count,
        "one added and one removed row must leave the count unchanged"
    );
    assert_eq!(
        mismatches(&added.conn)?,
        vec![
            MatrixMismatch::Missing {
                cell: count_neutral_missing,
                expected: count_neutral_expected,
            },
            MatrixMismatch::Unexpected {
                cell: probe_cell,
                found: probe_grant,
            },
        ],
        "equal row counts must not hide an extra pair and a missing pair"
    );

    let removed = fresh_database("removed.db");
    let removed_cell = cell(Role::Owner, cap::BackupRestore::NAME);
    let removed_expected = domain_cell(Role::Owner, cap::BackupRestore::NAME);
    assert_eq!(
        removed_expected,
        stored_grant("granted", None),
        "owner/backup.restore must remain an unbounded grant for this probe"
    );
    let deleted = removed.conn.execute(
        "DELETE FROM role_capability
          WHERE role_id = (SELECT id FROM role WHERE code = ?1)
            AND capability = ?2",
        params![Role::Owner.as_str(), cap::BackupRestore::NAME],
    )?;
    assert_eq!(deleted, 1, "the removal probe must delete its seeded cell");
    assert_eq!(
        mismatches(&removed.conn)?,
        vec![MatrixMismatch::Missing {
            cell: removed_cell,
            expected: removed_expected,
        }],
        "a removed seeded cell must be reported by name"
    );

    let wrong_decision = fresh_database("wrong-decision.db");
    let wrong_decision_cell = cell(Role::Cashier, cap::SaleCreate::NAME);
    let decision_expected = domain_cell(Role::Cashier, cap::SaleCreate::NAME);
    assert_eq!(
        decision_expected,
        stored_grant("granted", None),
        "cashier/sale.create must remain an unbounded grant for this probe"
    );
    let changed_decision = wrong_decision.conn.execute(
        "UPDATE role_capability
            SET decision = 'withheld'
          WHERE role_id = (SELECT id FROM role WHERE code = ?1)
            AND capability = ?2",
        params![Role::Cashier.as_str(), cap::SaleCreate::NAME],
    )?;
    assert_eq!(
        changed_decision, 1,
        "the valid replacement decision must reach the seeded cell"
    );
    assert_eq!(
        mismatches(&wrong_decision.conn)?,
        vec![MatrixMismatch::Different {
            cell: wrong_decision_cell,
            expected: decision_expected,
            found: stored_grant("withheld", None),
        }],
        "a different CHECK-valid decision must be reported with both values"
    );

    let wrong_limit = fresh_database("wrong-limit.db");
    let wrong_limit_cell = cell(Role::Cashier, cap::DiscountManual::NAME);
    let limit_expected = domain_cell(Role::Cashier, cap::DiscountManual::NAME);
    assert_eq!(
        limit_expected,
        StoredGrant {
            decision: "granted".to_owned(),
            limit_json: Some(encoded_limit(Limit::RoleCap)),
        },
        "cashier/discount.manual must remain bounded by role_cap for this probe"
    );
    let replacement_limit = encoded_limit(Limit::OwnShift);
    let changed_limit = wrong_limit.conn.execute(
        "UPDATE role_capability
            SET limit_json = ?1
          WHERE role_id = (SELECT id FROM role WHERE code = ?2)
            AND capability = ?3
            AND decision = 'granted'",
        params![
            replacement_limit.as_str(),
            Role::Cashier.as_str(),
            cap::DiscountManual::NAME
        ],
    )?;
    assert_eq!(
        changed_limit, 1,
        "the valid replacement limit must reach the bounded seeded cell"
    );
    assert_eq!(
        mismatches(&wrong_limit.conn)?,
        vec![MatrixMismatch::Different {
            cell: wrong_limit_cell,
            expected: limit_expected,
            found: StoredGrant {
                decision: "granted".to_owned(),
                limit_json: Some(replacement_limit),
            },
        }],
        "a different valid limit kind must be reported with both values"
    );

    Ok(())
}
