//! A role can be held for one store, or across the whole org.
//!
//! `user_role.store_id` is NULL for an org-wide grant, and that NULL is
//! load-bearing: it is how an owner or an area manager holds a role everywhere.
//! Applying `STRICT` to the table silently forbade it, because SQLite makes every
//! primary-key component of a STRICT table implicitly NOT NULL — composite keys
//! included — and `store_id` was in the key.
//!
//! These tests pin both halves: the NULL must be insertable, and neither kind of
//! grant may be duplicated. A plain unique index over the triple would satisfy the
//! first and not the second, because SQLite treats NULLs as distinct.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{blob, full_schema};

const GRANT: &str = "INSERT INTO user_role (user_id, role_id, store_id) VALUES (?1, ?2, ?3)";

#[test]
fn a_role_can_be_granted_across_the_whole_org() {
    let dir = tempfile::tempdir().unwrap();
    let conn = full_schema(&dir, "orgwide.db");

    conn.execute(
        GRANT,
        rusqlite::params![blob(1), blob(2), rusqlite::types::Null],
    )
    .expect("store_id NULL is an org-wide grant, not an error");

    let scope: Option<Vec<u8>> = conn
        .query_row("SELECT store_id FROM user_role", [], |r| r.get(0))
        .unwrap();
    assert!(scope.is_none(), "the org-wide grant must keep its NULL");
}

#[test]
fn the_same_org_wide_grant_cannot_be_made_twice() {
    let dir = tempfile::tempdir().unwrap();
    let conn = full_schema(&dir, "dup_org.db");

    conn.execute(
        GRANT,
        rusqlite::params![blob(1), blob(2), rusqlite::types::Null],
    )
    .unwrap();
    conn.execute(
        GRANT,
        rusqlite::params![blob(1), blob(2), rusqlite::types::Null],
    )
    .expect_err("two identical org-wide grants must not both exist");
}

#[test]
fn a_store_scoped_grant_coexists_with_an_org_wide_one() {
    let dir = tempfile::tempdir().unwrap();
    let conn = full_schema(&dir, "both.db");

    conn.execute(
        GRANT,
        rusqlite::params![blob(1), blob(2), rusqlite::types::Null],
    )
    .unwrap();
    conn.execute(GRANT, rusqlite::params![blob(1), blob(2), blob(3)])
        .expect("an org-wide grant must not block a store-scoped one");

    let rows: i64 = conn
        .query_row("SELECT count(*) FROM user_role", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 2);
}

#[test]
fn the_same_store_scoped_grant_cannot_be_made_twice() {
    let dir = tempfile::tempdir().unwrap();
    let conn = full_schema(&dir, "dup_store.db");

    conn.execute(GRANT, rusqlite::params![blob(1), blob(2), blob(3)])
        .unwrap();
    conn.execute(GRANT, rusqlite::params![blob(1), blob(2), blob(3)])
        .expect_err("two identical store-scoped grants must not both exist");
}

#[test]
fn one_role_can_be_granted_for_several_stores() {
    let dir = tempfile::tempdir().unwrap();
    let conn = full_schema(&dir, "many.db");

    for store in [3u8, 4, 5] {
        conn.execute(GRANT, rusqlite::params![blob(1), blob(2), blob(store)])
            .expect("the same role in different stores is the ordinary case");
    }
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM user_role", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 3);
}
