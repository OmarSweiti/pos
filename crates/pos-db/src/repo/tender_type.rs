//! The tender kinds this register takes (`ref/schema.md` §0005).
//!
//! `0005` seeds every kind the grid knows, and only cash active: the others
//! are switched on by the steps that build them. The domain's `add_tender`
//! knows a kind by its code and leaves whether this register takes it to the
//! shell (1.4.8), which asks here before a tender is taken (1.8.3).

use rusqlite::{Connection, OptionalExtension};

use crate::DbError;

/// The register's `tender_type` rows.
pub struct TenderTypeRepository<'c> {
    conn: &'c Connection,
}

impl<'c> TenderTypeRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Whether this register takes tenders of kind `code`: its row is active.
    /// A code with no row is not taken.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] if the read fails.
    pub fn is_active(&self, code: &str) -> Result<bool, DbError> {
        let active: Option<bool> = self
            .conn
            .query_row(
                "SELECT is_active = 1 FROM tender_type WHERE code = ?1",
                [code],
                |row| row.get(0),
            )
            .optional()?;
        Ok(active.unwrap_or(false))
    }
}
