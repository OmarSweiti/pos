//! The role grid a register ships and a merchant edits: `role_capability`,
//! seeded by migration `0004` and given its discount caps by `0008`.
//!
//! This module only reads, and one role at a time. A user may hold several
//! roles, and combining their cells into the grants a session holds is the
//! persistence boundary's job, which [`pos_domain::permissions::GrantSet`]
//! leaves to the session that 1.8.x wires into the shell. What lives here is
//! the reading of one cell, and the decision of what an unreadable one means.
//!
//! **A cell this module cannot read is an error, never a default.** A discount
//! cap read wrongly is money a cashier can give away, so a role-capped cell
//! with no number, a number out of range, or a limit of another kind is
//! refused by name rather than taken as no cap or as zero.

use pos_domain::permissions::{Capability, Limit, Role, cap};
use pos_domain::{DiscountCap, Percent};
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

impl<'a> RoleRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        RoleRepository { conn }
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
