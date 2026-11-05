//! Signing in and out: `session_state`, `auth_login_pin` and `auth_logout`
//! (`ref/ipc-contract.md` §3, "Session and users"; microstep 1.8.2c).
//!
//! **Sign-in.** The account is found by its staff number. An unknown number,
//! an account that is not live and a wrong PIN all get the same refusal,
//! `auth_refused`: a staff number identifies, the PIN authenticates, and the
//! lock screen does not say which of the two was wrong. 1.6.2's attempt
//! policy is applied before the PIN is checked, so a waiting or locked-out
//! user is refused without one. A wrong PIN is counted and audited. A correct
//! one resets the count, rehashes a PIN stored below the reviewed profile, and
//! then needs exactly one role at this store: none, or more than one, is
//! refused by name, because combining two roles' grants needs a precedence
//! nobody has decided. The session is recorded in `user_session` and audited.
//!
//! **What the webview sees** is [`SessionState`]: who is signed in, by staff
//! number, name and role. Never the grants, a hash, a nonce or a token.
//!
//! **Sign-out sets the basket aside** (the owner's decision of 5 November
//! 2026, 1.4.11). A working cart with lines goes to the parked list, where
//! whoever signs in next can resume it; one with nothing in it is dropped.
//! The same holds when the context signs out an account that stopped being
//! live, which no one can refuse. The sign-out's audit row names the cart it
//! set aside.

use pos_db::auth::{ATTEMPT_POLICY, Admission, admission, hash_pin, needs_rehash, verify_pin};
use pos_db::repo::parked_cart::ParkedCartRepository;
use pos_db::repo::role::RoleRepository;
use pos_db::repo::user::{SessionEnd, UserRepository};
use pos_domain::cart::park;
use pos_domain::{AuditIntent, Cart, Timestamp};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::json;
use tauri::State;

use super::{audit, sqlite, stored};
use crate::ipc::context::not_signed_in;
use crate::ipc::{IpcError, IpcResult};
use crate::state::{Register, Session, Till};

/// Who is signed in, as the webview is told it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SessionState {
    /// The till did not open, and this is why, from a fixed set.
    Closed {
        reason: &'static str,
    },
    SignedOut,
    SignedIn {
        code: String,
        display_name: String,
        role: &'static str,
    },
}

impl SessionState {
    fn of(session: &Session) -> SessionState {
        SessionState::SignedIn {
            code: session.code.clone(),
            display_name: session.display_name.clone(),
            role: session.role.as_str(),
        }
    }
}

/// Why a session was ended, as its audit row says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignOutCause {
    /// The user signed out.
    Requested,
    /// Their account stopped being live while they were signed in.
    UserInactive,
}

impl SignOutCause {
    const fn as_str(self) -> &'static str {
        match self {
            SignOutCause::Requested => "requested",
            SignOutCause::UserInactive => "user_inactive",
        }
    }
}

#[tauri::command]
pub fn session_state(till: State<'_, Till>) -> IpcResult<SessionState> {
    Ok(state_of(&till))
}

#[tauri::command]
pub fn auth_login_pin(
    till: State<'_, Till>,
    user_code: String,
    pin: String,
) -> IpcResult<SessionState> {
    login_pin(till.register()?, &user_code, &pin)
}

#[tauri::command]
pub fn auth_logout(till: State<'_, Till>) -> IpcResult<()> {
    logout(till.register()?)
}

/// What `session_state` answers.
pub fn state_of(till: &Till) -> SessionState {
    match till {
        Till::Closed(refusal) => SessionState::Closed {
            reason: refusal.detail(),
        },
        Till::Open(register) => register
            .session()
            .as_ref()
            .map_or(SessionState::SignedOut, SessionState::of),
    }
}

/// `auth_login_pin`: sign `user_code` in with `pin`.
///
/// # Errors
///
/// - `auth_already_signed_in` while someone is. Switching users is
///   `auth_switch_user`'s (2.4.2).
/// - `auth_refused` for an unknown staff number, an account that is not live,
///   or a wrong PIN.
/// - `auth_delayed` inside the wait after a run of wrong PINs, and
///   `auth_locked_out` once a manager must release the user.
/// - `auth_role_unresolved` for a correct PIN with no role at this store, or
///   more than one, named in the detail.
/// - `auth_unavailable` when the stored hash cannot be checked, and
///   `storage_failed`.
pub fn login_pin(register: &Register, user_code: &str, pin: &str) -> IpcResult<SessionState> {
    let at = register.now();
    let (mut db, mut session) = register.lock();
    if session.is_some() {
        return Err(IpcError::new(
            "auth_already_signed_in",
            "error.auth.already_signed_in",
        ));
    }

    let users = UserRepository::new(&db);
    let Some(account) = users
        .by_code(user_code)
        .map_err(stored)?
        .filter(|account| account.is_live)
    else {
        return Err(refused());
    };
    let attempts = users
        .attempts(account.id, register.register())
        .map_err(stored)?;
    match admission(&attempts, at, &ATTEMPT_POLICY) {
        Admission::Allowed => {}
        Admission::Delayed { .. } => {
            return Err(IpcError::new("auth_delayed", "error.auth.delayed"));
        }
        Admission::LockedOut => return Err(locked_out()),
    }
    let Some(hash) = users.pin_hash(account.id).map_err(stored)? else {
        return Err(refused());
    };
    let correct = verify_pin(pin, &hash).map_err(unavailable)?;

    let tx = db.transaction().map_err(sqlite)?;
    let users = UserRepository::new(&tx);
    if !correct {
        let after = users
            .record_failure(&tx, account.id, register.register(), at)
            .map_err(stored)?;
        let locked = ATTEMPT_POLICY.locks_out(after.failed_attempts);
        audit(
            register,
            &tx,
            &AuditIntent {
                actor: account.id,
                approver: None,
                approval: None,
                action: "auth.login_refused",
                entity: "app_user",
                entity_id: account.id.as_uuid(),
                reason: None,
                payload: json!({
                    "failed_attempts": after.failed_attempts,
                    "locked_out": locked,
                }),
                at,
            },
        )?;
        tx.commit().map_err(sqlite)?;
        return Err(if locked { locked_out() } else { refused() });
    }

    users
        .record_success(&tx, account.id, register.register(), at)
        .map_err(stored)?;
    if needs_rehash(&hash).map_err(unavailable)? {
        let rehashed = hash_pin(pin).map_err(unavailable)?;
        users
            .set_pin_hash(&tx, account.id, &rehashed, at)
            .map_err(stored)?;
    }
    let roles = RoleRepository::new(&tx)
        .roles_at(account.id, register.store())
        .map_err(stored)?;
    let role = match roles.as_slice() {
        [role] => *role,
        several_or_none => {
            audit(
                register,
                &tx,
                &AuditIntent {
                    actor: account.id,
                    approver: None,
                    approval: None,
                    action: "auth.login_refused",
                    entity: "app_user",
                    entity_id: account.id.as_uuid(),
                    reason: None,
                    payload: json!({ "roles": several_or_none.len() }),
                    at,
                },
            )?;
            tx.commit().map_err(sqlite)?;
            let detail = if several_or_none.is_empty() {
                "no role at this store"
            } else {
                "more than one role at this store"
            };
            return Err(
                IpcError::new("auth_role_unresolved", "error.auth.role_unresolved")
                    .with_detail(detail),
            );
        }
    };
    let grants = RoleRepository::new(&tx).grants(role).map_err(stored)?;

    let session_id = register.next_id();
    users
        .start_session(&tx, session_id, account.id, register.register(), at)
        .map_err(stored)?;
    audit(
        register,
        &tx,
        &AuditIntent {
            actor: account.id,
            approver: None,
            approval: None,
            action: "auth.login",
            entity: "user_session",
            entity_id: session_id,
            reason: None,
            payload: json!({ "role": role.as_str() }),
            at,
        },
    )?;
    tx.commit().map_err(sqlite)?;

    let signed_in = Session {
        id: session_id,
        user: account.id,
        code: account.code,
        display_name: account.display_name,
        role,
        grants,
        signed_in_at: at,
    };
    let view = SessionState::of(&signed_in);
    *session = Some(signed_in);
    Ok(view)
}

/// `auth_logout`: end the signed-in session, setting aside its basket.
///
/// # Errors
///
/// `not_signed_in` when no one is, and `storage_failed`.
pub fn logout(register: &Register) -> IpcResult<()> {
    let at = register.now();
    let mut till = register.lock_till();
    let Some(current) = till.session.clone() else {
        return Err(not_signed_in());
    };
    sign_out(
        register,
        &mut till.db,
        &current,
        &mut till.cart,
        SignOutCause::Requested,
        at,
    )?;
    *till.session = None;
    Ok(())
}

/// End `current`'s `user_session` row and audit it, in one transaction, with
/// the till's working cart `cart` set aside in the parked list when it has
/// lines and dropped when it has none. The cart is cleared once that commits;
/// the caller clears the session.
pub(crate) fn sign_out(
    register: &Register,
    db: &mut Connection,
    current: &Session,
    cart: &mut Option<Cart>,
    cause: SignOutCause,
    at: Timestamp,
) -> IpcResult<()> {
    let tx = db.transaction().map_err(sqlite)?;
    let set_aside = match cart.as_ref() {
        None => None,
        Some(working) => {
            let carts = ParkedCartRepository::new(&tx);
            match park(working.clone()) {
                Ok(parked) => {
                    carts
                        .repark(&tx, register.process_nonce(), &parked, None, at)
                        .map_err(stored)?;
                    Some(working.id)
                }
                Err(_) => {
                    carts
                        .discard(&tx, register.process_nonce(), working.id)
                        .map_err(stored)?;
                    None
                }
            }
        }
    };
    UserRepository::new(&tx)
        .end_session(&tx, current.id, SessionEnd::Logout, at)
        .map_err(stored)?;
    audit(
        register,
        &tx,
        &AuditIntent {
            actor: current.user,
            approver: None,
            approval: None,
            action: "auth.logout",
            entity: "user_session",
            entity_id: current.id,
            reason: None,
            payload: json!({
                "cause": cause.as_str(),
                "parked_cart": set_aside.map(|sale| sale.as_uuid().to_string()),
            }),
            at,
        },
    )?;
    tx.commit().map_err(sqlite)?;
    *cart = None;
    Ok(())
}

fn refused() -> IpcError {
    IpcError::new("auth_refused", "error.auth.refused")
}

fn locked_out() -> IpcError {
    IpcError::new("auth_locked_out", "error.auth.locked_out")
}

fn unavailable(error: pos_db::auth::AuthError) -> IpcError {
    IpcError::from_source("auth_unavailable", "error.auth.unavailable", &error)
}
