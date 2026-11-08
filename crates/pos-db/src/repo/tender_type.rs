//! The tender kinds this register takes (`ref/schema.md` §0005).
//!
//! `0005` seeds every kind the grid knows, and only cash active: the others
//! are switched on by the steps that build them. The domain's `add_tender`
//! knows a kind by its code and leaves whether this register takes it to the
//! shell (1.4.8), which asks here before a tender is taken (1.8.3). The
//! finalize reads here too: the name each kind prints under, and whether a
//! tender of it opens the drawer.

use rusqlite::{Connection, OptionalExtension};

use crate::DbError;

/// A tender kind, as a receipt prints it and the drawer obeys it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenderKind {
    pub code: String,
    pub name_ar: String,
    pub name_en: Option<String>,
    /// Whether a sale taking a tender of this kind opens the drawer.
    pub opens_drawer: bool,
}

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

    /// Every kind the register knows, taken or not, in the grid's order. A
    /// completed sale prints the names of the kinds it was paid in, whether or
    /// not the register still takes them.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] if the read fails.
    pub fn kinds(&self) -> Result<Vec<TenderKind>, DbError> {
        let mut statement = self.conn.prepare(
            "SELECT code, name_ar, name_en, opens_drawer = 1
               FROM tender_type ORDER BY sort_order, code",
        )?;
        let kinds = statement
            .query_map([], |row| {
                Ok(TenderKind {
                    code: row.get(0)?,
                    name_ar: row.get(1)?,
                    name_en: row.get(2)?,
                    opens_drawer: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(kinds)
    }
}
