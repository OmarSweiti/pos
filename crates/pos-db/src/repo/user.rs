//! Staff accounts' PIN hashes, and the wrong PINs each one has tried on each
//! register (microstep 1.6.2).
//!
//! `auth_attempt_state` is register-local operational state, never synced, and
//! kept per user and per register so restarting the register clears nothing:
//! in memory, a count resets on reboot, and rebooting is free. The policy that
//! reads it is [`crate::auth::ATTEMPT_POLICY`]. Who may release a lockout, and
//! the audit row that records it, are the command's (`ref/security-compliance.md`
//! §5): this repository only writes what the command decided, inside the
//! command's transaction.
//!
//! It also keeps the register's sign-in sessions, `user_session` (1.8.2c):
//! register-local like the attempt state, never synced, one row from a
//! sign-in to the moment that session ended and why.

use pos_domain::{RegisterId, Timestamp, UserId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use uuid::Uuid;

use crate::DbError;
use crate::auth::{ATTEMPT_POLICY, AttemptState};

/// Reads and writes of staff PINs and their attempt state.
pub struct UserRepository<'c> {
    conn: &'c Connection,
}

/// A staff account, as sign-in and the command context read it. It carries
/// no PIN hash: [`UserRepository::pin_hash`] is the one read of that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaffAccount {
    pub id: UserId,
    /// The staff number typed at the lock screen.
    pub code: String,
    pub display_name: String,
    /// `is_active = 1` and no `deleted_at`: the only accounts that may sign in
    /// or act (1.6.4's item 5).
    pub is_live: bool,
}

/// Why a session ended, as `user_session.end_reason` spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    /// The user signed out, or was signed out because their account stopped
    /// being live: `0004`'s list has no reason of its own for that.
    Logout,
    IdleLock,
    SwitchUser,
    ShiftClose,
    /// The process died with the session open. Recorded at the next start-up,
    /// which is all that can be known of when.
    Crash,
}

impl SessionEnd {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SessionEnd::Logout => "logout",
            SessionEnd::IdleLock => "idle_lock",
            SessionEnd::SwitchUser => "switch_user",
            SessionEnd::ShiftClose => "shift_close",
            SessionEnd::Crash => "crash",
        }
    }
}

/// One `app_user` row's columns, before its id is checked.
struct AccountRow {
    id: Vec<u8>,
    code: String,
    display_name: String,
    is_live: bool,
}

impl AccountRow {
    fn account(self) -> Result<StaffAccount, DbError> {
        let id = Uuid::from_slice(&self.id).map_err(|_| DbError::AuthStateInvalid {
            reason: "a stored account id is not sixteen bytes",
        })?;
        Ok(StaffAccount {
            id: UserId::from_uuid(id),
            code: self.code,
            display_name: self.display_name,
            is_live: self.is_live,
        })
    }
}

const ACCOUNT: &str = "SELECT id, code, display_name, is_active = 1 AND deleted_at IS NULL
                         FROM app_user";

fn account_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AccountRow> {
    Ok(AccountRow {
        id: row.get(0)?,
        code: row.get(1)?,
        display_name: row.get(2)?,
        is_live: row.get(3)?,
    })
}

fn moment(text: Option<String>) -> Result<Option<Timestamp>, DbError> {
    text.map(|t| {
        Timestamp::parse_iso8601(&t).map_err(|_| DbError::AuthStateInvalid {
            reason: "a stored moment is not a timestamp",
        })
    })
    .transpose()
}

impl<'c> UserRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// The account whose staff number is `code`, live or not, or `None`.
    ///
    /// # Errors
    ///
    /// [`DbError::AuthStateInvalid`] for a stored id this build cannot read,
    /// and [`DbError::Sqlite`].
    pub fn by_code(&self, code: &str) -> Result<Option<StaffAccount>, DbError> {
        self.conn
            .query_row(&format!("{ACCOUNT} WHERE code = ?1"), [code], account_row)
            .optional()?
            .map(AccountRow::account)
            .transpose()
    }

    /// The account `user`, live or not, or `None`.
    ///
    /// # Errors
    ///
    /// As [`UserRepository::by_code`].
    pub fn by_id(&self, user: UserId) -> Result<Option<StaffAccount>, DbError> {
        self.conn
            .query_row(
                &format!("{ACCOUNT} WHERE id = ?1"),
                [user.as_uuid().as_bytes().as_slice()],
                account_row,
            )
            .optional()?
            .map(AccountRow::account)
            .transpose()
    }

    /// Open session `session`: `user` signed in on `register` at `at`.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`], a second session under the same id among them.
    pub fn start_session(
        &self,
        tx: &Transaction<'_>,
        session: Uuid,
        user: UserId,
        register: RegisterId,
        at: Timestamp,
    ) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO user_session (id, user_id, register_id, started_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                session.as_bytes().as_slice(),
                user.as_uuid().as_bytes().as_slice(),
                register.as_uuid().as_bytes().as_slice(),
                at.to_iso8601(),
            ],
        )?;
        Ok(())
    }

    /// End the open session `session` at `at`, for `reason`.
    ///
    /// # Errors
    ///
    /// [`DbError::AuthStateInvalid`] when no open session has that id: it
    /// never began, or it has already ended. [`DbError::Sqlite`].
    pub fn end_session(
        &self,
        tx: &Transaction<'_>,
        session: Uuid,
        reason: SessionEnd,
        at: Timestamp,
    ) -> Result<(), DbError> {
        let ended = tx.execute(
            "UPDATE user_session SET ended_at = ?2, end_reason = ?3
              WHERE id = ?1 AND ended_at IS NULL",
            params![
                session.as_bytes().as_slice(),
                at.to_iso8601(),
                reason.as_str()
            ],
        )?;
        if ended != 1 {
            return Err(DbError::AuthStateInvalid {
                reason: "no open session has that id",
            });
        }
        Ok(())
    }

    /// End every session still open on `register` at `at`, for `reason`, and
    /// say how many there were. At start-up that is what a crash left.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`].
    pub fn end_open_sessions(
        &self,
        tx: &Transaction<'_>,
        register: RegisterId,
        reason: SessionEnd,
        at: Timestamp,
    ) -> Result<usize, DbError> {
        Ok(tx.execute(
            "UPDATE user_session SET ended_at = ?2, end_reason = ?3
              WHERE register_id = ?1 AND ended_at IS NULL",
            params![
                register.as_uuid().as_bytes().as_slice(),
                at.to_iso8601(),
                reason.as_str()
            ],
        )?)
    }

    /// The PHC string an active account's PIN is checked against, or `None`
    /// for an account that does not exist, is inactive or was deleted.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`].
    pub fn pin_hash(&self, user: UserId) -> Result<Option<String>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT pin_hash FROM app_user
                  WHERE id = ?1 AND is_active = 1 AND deleted_at IS NULL",
                [user.as_uuid().as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Replace an account's PIN hash, as a reset or a rehash does. The old
    /// hash is gone once the caller commits; `at` is when the new one was set.
    ///
    /// # Errors
    ///
    /// [`DbError::AuthStateInvalid`] when no account has that id, and
    /// [`DbError::Sqlite`].
    pub fn set_pin_hash(
        &self,
        tx: &Transaction<'_>,
        user: UserId,
        phc: &str,
        at: Timestamp,
    ) -> Result<(), DbError> {
        let changed = tx.execute(
            "UPDATE app_user SET pin_hash = ?2, pin_set_at = ?3 WHERE id = ?1",
            params![user.as_uuid().as_bytes().as_slice(), phc, at.to_iso8601()],
        )?;
        if changed != 1 {
            return Err(DbError::AuthStateInvalid {
                reason: "no account has that id",
            });
        }
        Ok(())
    }

    /// The wrong PINs `user` has tried on `register` since the last correct
    /// one or release. No row is no attempt.
    ///
    /// # Errors
    ///
    /// [`DbError::AuthStateInvalid`] for a stored row this build cannot read,
    /// and [`DbError::Sqlite`].
    pub fn attempts(&self, user: UserId, register: RegisterId) -> Result<AttemptState, DbError> {
        let row: Option<(i64, Option<String>, String)> = self
            .conn
            .query_row(
                "SELECT failed_attempts, delay_until_at, last_attempt_at
                   FROM auth_attempt_state WHERE user_id = ?1 AND register_id = ?2",
                [
                    user.as_uuid().as_bytes().as_slice(),
                    register.as_uuid().as_bytes().as_slice(),
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((failed, delay_until, last)) = row else {
            return Ok(AttemptState::default());
        };
        Ok(AttemptState {
            failed_attempts: u32::try_from(failed).map_err(|_| DbError::AuthStateInvalid {
                reason: "a stored attempt count is out of range",
            })?,
            delay_until: moment(delay_until)?,
            last_attempt_at: moment(Some(last))?,
        })
    }

    /// One more wrong PIN by `user` on `register` at `at`, and the state it
    /// leaves under [`ATTEMPT_POLICY`]: a wait after the third, doubling, and a
    /// lockout at the tenth in a row.
    ///
    /// # Errors
    ///
    /// As [`UserRepository::attempts`].
    pub fn record_failure(
        &self,
        tx: &Transaction<'_>,
        user: UserId,
        register: RegisterId,
        at: Timestamp,
    ) -> Result<AttemptState, DbError> {
        let failed = self
            .attempts(user, register)?
            .failed_attempts
            .saturating_add(1);
        let delay_until = ATTEMPT_POLICY
            .delay_after(failed)
            .map(|ms| {
                Timestamp::from_epoch_milliseconds(at.epoch_milliseconds().saturating_add(ms))
                    .map_err(|_| DbError::AuthStateInvalid {
                        reason: "the wait runs past the last representable moment",
                    })
            })
            .transpose()?;
        tx.execute(
            "INSERT INTO auth_attempt_state
               (user_id, register_id, failed_attempts, delay_until_at, last_attempt_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(user_id, register_id) DO UPDATE SET
               failed_attempts = excluded.failed_attempts,
               delay_until_at  = excluded.delay_until_at,
               last_attempt_at = excluded.last_attempt_at",
            params![
                user.as_uuid().as_bytes().as_slice(),
                register.as_uuid().as_bytes().as_slice(),
                i64::from(failed),
                delay_until.map(Timestamp::to_iso8601),
                at.to_iso8601(),
            ],
        )?;
        Ok(AttemptState {
            failed_attempts: failed,
            delay_until,
            last_attempt_at: Some(at),
        })
    }

    /// A correct PIN: the count starts again.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`].
    pub fn record_success(
        &self,
        tx: &Transaction<'_>,
        user: UserId,
        register: RegisterId,
        at: Timestamp,
    ) -> Result<(), DbError> {
        Self::clear(tx, user, register, at)
    }

    /// A manager's release of a locked-out user on `register`. Who may release
    /// whom, and the audit row, are the command's; this writes the effect in
    /// the command's transaction.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`].
    pub fn release(
        &self,
        tx: &Transaction<'_>,
        user: UserId,
        register: RegisterId,
        at: Timestamp,
    ) -> Result<(), DbError> {
        Self::clear(tx, user, register, at)
    }

    fn clear(
        tx: &Transaction<'_>,
        user: UserId,
        register: RegisterId,
        at: Timestamp,
    ) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO auth_attempt_state
               (user_id, register_id, failed_attempts, delay_until_at, last_attempt_at)
             VALUES (?1, ?2, 0, NULL, ?3)
             ON CONFLICT(user_id, register_id) DO UPDATE SET
               failed_attempts = 0,
               delay_until_at  = NULL,
               last_attempt_at = excluded.last_attempt_at",
            params![
                user.as_uuid().as_bytes().as_slice(),
                register.as_uuid().as_bytes().as_slice(),
                at.to_iso8601(),
            ],
        )?;
        Ok(())
    }
}
