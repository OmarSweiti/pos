//! The role grid a register ships and a merchant edits: `role_capability`,
//! seeded by migration `0004` and given its discount caps by `0008`.
//!
//! This module only reads, and one role at a time. `user_role` lets a user hold
//! several roles, and combining two roles' cells needs a precedence nobody has
//! decided, so a session holds exactly one: [`RoleRepository::roles_at`] lists
//! a user's roles at a store and the shell refuses a sign-in with none or with
//! more than one (1.8.2c). What lives here is the reading of the cells, and the
//! decision of what an unreadable one means.
//!
//! **A cell this module cannot read is an error, never a default.** A discount
//! cap read wrongly is money a cashier can give away, so a role-capped cell
//! with no number, a number out of range, or a limit of another kind is
//! refused by name rather than taken as no cap or as zero. The same holds for
//! a whole role's grants: a cell with a decision or a limit this build does not
//! know is refused rather than read as withheld or as held.

use pos_domain::permissions::{Capability, Grant, GrantSet, Limit, Role, cap};
use pos_domain::{DiscountCap, Percent, StoreId, UserId};
use rusqlite::{Connection, OptionalExtension, params};

use crate::DbError;

/// One hundred percent, in ppm: the largest cap a cell can hold.
const WHOLE_PPM: i64 = 1_000_000;

/// The cells of the role grid, read.
pub struct RoleRepository<'a> {
    conn: &'a Connection,
}

/// One `role_capability` row, with its limit taken apart by SQLite's JSON
/// functions so a malformed column is reported rather than raised.
struct StoredCell {
    decision: String,
    limit_json: Option<String>,
    is_json: Option<bool>,
    kind: Option<String>,
    number_type: Option<String>,
    number: Option<i64>,
}

/// Every [`Limit`], so a stored token is matched against the domain's own
/// spelling of it. `tests::every_limit_is_listed` fails to compile when the
/// domain adds one this list does not name.
const LIMITS: [Limit; 5] = [
    Limit::OwnShift,
    Limit::OwnStore,
    Limit::ExactMatchOnly,
    Limit::RoleCap,
    Limit::RefundThreshold,
];

/// One `role_capability` row as [`RoleRepository::grants`] reads it.
struct StoredGrant {
    capability: String,
    decision: String,
    limit_json: Option<String>,
    is_json: Option<bool>,
    kind: Option<String>,
}

impl<'a> RoleRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        RoleRepository { conn }
    }

    /// The roles `user` holds at `store`, each once: those scoped to the store
    /// and the org-wide ones (`store_id IS NULL`), counting only a user and a
    /// store of the same org. A role held both ways is one role.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredRoleGrantInvalid`] for a role code this build does not
    /// know, and [`DbError::Sqlite`].
    pub fn roles_at(&self, user: UserId, store: StoreId) -> Result<Vec<Role>, DbError> {
        let mut statement = self.conn.prepare(
            "SELECT DISTINCT r.code
               FROM user_role AS ur
               JOIN role      AS r ON r.id = ur.role_id
               JOIN app_user  AS u ON u.id = ur.user_id
               JOIN store     AS s ON s.id = ?2
              WHERE ur.user_id = ?1
                AND u.org_id = s.org_id
                AND (ur.store_id = s.id OR ur.store_id IS NULL)
              ORDER BY r.code",
        )?;
        let codes = statement
            .query_map(
                params![
                    user.as_uuid().as_bytes().as_slice(),
                    store.as_uuid().as_bytes().as_slice()
                ],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        codes
            .iter()
            .map(|code| {
                Role::ALL
                    .into_iter()
                    .find(|role| role.as_str() == code)
                    .ok_or_else(|| DbError::StoredRoleGrantInvalid {
                        reason: format!("the role code {code:?} is not one this build knows"),
                    })
            })
            .collect()
    }

    /// The grants `role` holds, as its `role_capability` cells say: the
    /// shipped grid, or a merchant's edit of it.
    ///
    /// Every capability this build knows must have its cell. A cell for a
    /// capability it does not know is left out, because no command here can
    /// prove one.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredRoleGrantInvalid`] for a known capability with no
    /// cell, a decision this build does not know, a limit on a cell that is
    /// not granted, or a limit that is not JSON or names no known kind.
    /// [`DbError::Sqlite`] when the query fails.
    pub fn grants(&self, role: Role) -> Result<GrantSet, DbError> {
        let mut statement = self.conn.prepare(
            "SELECT rc.capability, rc.decision, rc.limit_json, json_valid(rc.limit_json),
                    CASE WHEN json_valid(rc.limit_json)
                         THEN json_extract(rc.limit_json, '$.kind') END
               FROM role_capability AS rc JOIN role AS r ON r.id = rc.role_id
              WHERE r.code = ?1",
        )?;
        let cells = statement
            .query_map([role.as_str()], |row| {
                Ok(StoredGrant {
                    capability: row.get(0)?,
                    decision: row.get(1)?,
                    limit_json: row.get(2)?,
                    is_json: row.get(3)?,
                    kind: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut grants = GrantSet::empty();
        for &capability in cap::ALL {
            let cell = cells
                .iter()
                .find(|cell| cell.capability == capability)
                .ok_or_else(|| DbError::StoredRoleGrantInvalid {
                    reason: format!("{}'s {capability} cell is missing", role.as_str()),
                })?;
            grants = grants.with_grant(capability, grant_of(role, cell)?);
        }
        Ok(grants)
    }

    /// The `discount.manual` cap `role` holds (merchant decisions 3.1–3.3).
    ///
    /// `None` when the role does not hold the capability at all, whether its
    /// cell is withheld or, like the owner's, only sets the limits;
    /// [`DiscountCap::Unlimited`] when it is granted with no limit; and
    /// [`DiscountCap::AtMost`] the configured share when its cell is
    /// role-capped, `{"kind":"role_cap","max_percent_ppm":N}`.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredRoleLimitInvalid`] for a cell that cannot be read as a
    /// cap: none at all, a role cap with no whole number, a number outside
    /// 0–1,000,000 ppm, a limit of another kind, or a column that is not
    /// JSON. [`DbError::Sqlite`] when the query itself fails.
    pub fn discount_cap(&self, role: Role) -> Result<Option<DiscountCap>, DbError> {
        let capability = cap::DiscountManual::NAME;
        let invalid = |reason: &str| DbError::StoredRoleLimitInvalid {
            reason: format!("{}'s {capability} cell {reason}", role.as_str()),
        };
        let stored = self
            .conn
            .query_row(
                "SELECT rc.decision, rc.limit_json, json_valid(rc.limit_json),
                        CASE WHEN json_valid(rc.limit_json)
                             THEN json_extract(rc.limit_json, '$.kind') END,
                        CASE WHEN json_valid(rc.limit_json)
                             THEN json_type(rc.limit_json, '$.max_percent_ppm') END,
                        CASE WHEN json_valid(rc.limit_json)
                              AND json_type(rc.limit_json, '$.max_percent_ppm') = 'integer'
                             THEN json_extract(rc.limit_json, '$.max_percent_ppm') END
                   FROM role_capability AS rc JOIN role AS r ON r.id = rc.role_id
                  WHERE r.code = ?1 AND rc.capability = ?2",
                params![role.as_str(), capability],
                |row| {
                    Ok(StoredCell {
                        decision: row.get(0)?,
                        limit_json: row.get(1)?,
                        is_json: row.get(2)?,
                        kind: row.get(3)?,
                        number_type: row.get(4)?,
                        number: row.get(5)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| invalid("is missing: no row decides it"))?;
        match stored.decision.as_str() {
            "withheld" | "sets_the_limit" => return Ok(None),
            "granted" => {}
            other => return Err(invalid(&format!("holds the decision {other:?}"))),
        }
        if stored.limit_json.is_none() {
            return Ok(Some(DiscountCap::Unlimited));
        }
        if stored.is_json != Some(true) {
            return Err(invalid("holds a limit that is not JSON"));
        }
        if stored.kind.as_deref() != Some(Limit::RoleCap.as_str()) {
            return Err(invalid(&format!(
                "holds a limit of kind {:?}, not {:?}",
                stored.kind,
                Limit::RoleCap.as_str()
            )));
        }
        match (stored.number_type.as_deref(), stored.number) {
            (Some("integer"), Some(ppm)) if (0..=WHOLE_PPM).contains(&ppm) => {
                Ok(Some(DiscountCap::AtMost(Percent::from_ppm(ppm))))
            }
            (Some("integer"), Some(ppm)) => Err(invalid(&format!(
                "caps at {ppm} ppm, outside 0 to {WHOLE_PPM}"
            ))),
            (found, _) => Err(invalid(&format!(
                "is role-capped with no whole max_percent_ppm (found {found:?})"
            ))),
        }
    }
}

/// One cell as a [`Grant`], per the table on [`Grant`]: only a granted cell
/// carries a limit, and its kind names a [`Limit`].
fn grant_of(role: Role, cell: &StoredGrant) -> Result<Grant, DbError> {
    let invalid = |reason: String| DbError::StoredRoleGrantInvalid {
        reason: format!("{}'s {} cell {reason}", role.as_str(), cell.capability),
    };
    match (cell.decision.as_str(), cell.limit_json.is_some()) {
        ("granted", false) => Ok(Grant::Held),
        ("granted", true) => {
            if cell.is_json != Some(true) {
                return Err(invalid("holds a limit that is not JSON".to_owned()));
            }
            let kind = cell.kind.as_deref();
            LIMITS
                .into_iter()
                .find(|limit| Some(limit.as_str()) == kind)
                .map(Grant::HeldWithin)
                .ok_or_else(|| invalid(format!("holds a limit of unknown kind {kind:?}")))
        }
        ("withheld", false) => Ok(Grant::Withheld),
        ("sets_the_limit", false) => Ok(Grant::SetsTheLimit),
        ("withheld" | "sets_the_limit", true) => Err(invalid(
            "is not granted and still carries a limit".to_owned(),
        )),
        (other, _) => Err(invalid(format!("holds the decision {other:?}"))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn grid() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open(&dir.path().join("grid.db"), "grid-key").unwrap();
        (dir, conn)
    }

    /// Rewrite the cashier's `sale.create` cell, as a corrupt file or a bad
    /// edit would leave it.
    fn set_cell(conn: &Connection, decision: &str, limit_json: Option<&str>) {
        let changed = conn
            .execute(
                "UPDATE role_capability SET decision = ?1, limit_json = ?2
                  WHERE capability = ?3
                    AND role_id = (SELECT id FROM role WHERE code = 'cashier')",
                params![decision, limit_json, cap::SaleCreate::NAME],
            )
            .unwrap();
        assert_eq!(changed, 1);
    }

    /// Read back from the shipped grid, each role's grants are its column of
    /// the domain's matrix.
    #[test]
    fn a_roles_grants_are_its_column_of_the_grid() {
        let (_dir, conn) = grid();
        for role in Role::ALL {
            assert_eq!(
                RoleRepository::new(&conn).grants(role).unwrap(),
                GrantSet::of_role(role),
                "{}",
                role.as_str()
            );
        }
    }

    /// A cell this build cannot read refuses the whole role, by name, rather
    /// than reading as withheld or as held.
    #[test]
    fn a_grant_this_build_cannot_read_is_refused() {
        // A limit on a cell that is not granted is the schema's to refuse
        // (`0004`'s CHECK), so it cannot be written here to be read.
        let cases: [(&str, Option<&str>); 2] = [
            ("granted", Some(r#"{"kind":"nonsense"}"#)),
            ("granted", Some("not json")),
        ];
        for (decision, limit_json) in cases {
            let (_dir, conn) = grid();
            set_cell(&conn, decision, limit_json);
            let refused = RoleRepository::new(&conn).grants(Role::Cashier);
            assert!(
                matches!(refused, Err(DbError::StoredRoleGrantInvalid { .. })),
                "{decision} {limit_json:?}: {refused:?}"
            );
        }

        let (_dir, conn) = grid();
        conn.execute(
            "DELETE FROM role_capability WHERE capability = ?1
               AND role_id = (SELECT id FROM role WHERE code = 'cashier')",
            [cap::SaleCreate::NAME],
        )
        .unwrap();
        let missing = RoleRepository::new(&conn).grants(Role::Cashier);
        assert!(
            matches!(&missing, Err(DbError::StoredRoleGrantInvalid { reason }) if reason.contains("missing")),
            "{missing:?}"
        );
        // Another role's column is untouched by it.
        assert_eq!(
            RoleRepository::new(&conn).grants(Role::Manager).unwrap(),
            GrantSet::of_role(Role::Manager)
        );
    }

    /// Where each limit sits in [`LIMITS`]. No wildcard arm, so a limit the
    /// domain adds is a compile error here until the list names it.
    const fn listed(limit: Limit) -> usize {
        match limit {
            Limit::OwnShift => 0,
            Limit::OwnStore => 1,
            Limit::ExactMatchOnly => 2,
            Limit::RoleCap => 3,
            Limit::RefundThreshold => 4,
        }
    }

    #[test]
    fn every_limit_is_listed() {
        for (position, limit) in LIMITS.into_iter().enumerate() {
            assert_eq!(listed(limit), position, "{}", limit.as_str());
        }
    }
}
