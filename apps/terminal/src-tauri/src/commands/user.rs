//! Staff accounts: `user_reset_pin` (`ref/ipc-contract.md` §3, "Session and
//! users"; microstep 1.6.2, its command, and E.51).
//!
//! **A cashier who forgets their PIN is reset by a manager,** with a second
//! person's approval. Otherwise "forgot PIN" is either an account nobody can
//! use again or a credential taken over without a trace.
//!
//! - **It takes `user.admin` and a bound approval on every call.** The
//!   approval names the account being reset, an amount of exactly zero, and
//!   the approver's reason, which the audit row repeats. Under merchant
//!   decision 9.6's default, which stands until the merchant answers #233,
//!   that approval is the second check: a manager's PIN of six digits or more
//!   and an audited reason. The approver can never be the person resetting
//!   (E.52).
//! - **A store's administrator does not reset the organisation's.** A holder
//!   of `user.admin` limited to their own store may not reset an account that
//!   holds `user.admin` without that limit, such as the owner's, because
//!   whoever sets a PIN can sign in as its account. It reads the grants, never
//!   the role's name, so a merchant's edit of the grid is what decides.
//! - **The new PIN has the digits its account needs:** six or more for an
//!   account holding a manager's capability, four or more otherwise.
//! - **The old hash is retired, and the account's lockout on this register is
//!   released,** in the transaction that writes the audit row and spends the
//!   approval. Nothing else is cleared: no other account's attempts, nor this
//!   account's on another register, nor any audit row. Releasing a lockout
//!   has no other path: no timeout and no restart clears one.
//! - **The PIN and its hash reach no sink.** The audit row names the account,
//!   the attempts the reset cleared and the profile the hash was made under.

use pos_db::DbError;
use pos_db::auth::{
    AuthError, MANAGER_PIN_DIGITS, PIN_PROFILE, check_new_pin, hash_pin, minimum_pin_digits,
};
use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::role::RoleRepository;
use pos_db::repo::user::UserRepository;
use pos_domain::permissions::{GrantSet, Limit};
use pos_domain::{
    ApprovalBinding, ApprovalId, AuditIntent, Capability, EscalationPolicy, PermissionError,
    UserId, cap,
};
use serde_json::json;
use tauri::State;

use super::cart::signed_in_as;
use super::{audit, sqlite, stored};
use crate::ipc::context::{authorize, not_signed_in, permission_refused};
use crate::ipc::{IpcError, IpcResult};
use crate::state::{Register, Till};

/// What a reset is recorded as having changed.
const EFFECT: &str = "pin_reset";

#[tauri::command]
pub fn user_reset_pin(
    till: State<'_, Till>,
    user_id: UserId,
    new_pin: String,
    approval_id: ApprovalId,
) -> IpcResult<()> {
    reset_pin(till.register()?, user_id, &new_pin, approval_id)
}

/// `user_reset_pin`: give `user_id` the PIN `new_pin`, under the signed-in
/// user's `user.admin` and the approval `approval_id`, which must name this
/// account, an amount of zero, and come from someone else.
///
/// # Errors
///
/// What the context refuses, the approval's own refusals among them
/// (`approval_unknown`, `approval_used`, `approval_mismatch`, `approval_self`,
/// `approval_expired` and `approval_revoked`); `permission_denied` without
/// `user.admin`; `user_unknown` for an account that is not live or holds no
/// role at this store; `permission_outside_limit` when a store's
/// administrator would reset an organisation's; `pin_shape` for a PIN that is
/// not all digits or is too short for the account, with the digits it needs;
/// `auth_unavailable` when the PIN cannot be hashed; and `storage_failed`. A
/// refused reset writes nothing and leaves the approval unspent.
pub fn reset_pin(
    register: &Register,
    user_id: UserId,
    new_pin: &str,
    approval_id: ApprovalId,
) -> IpcResult<()> {
    let at = register.now();
    let binding = ApprovalBinding {
        entity_id: user_id.as_uuid(),
        amount_minor: 0,
        content_hash: None,
    };
    // Every reset escalates, whatever the merchant's policy names.
    let policy = EscalationPolicy::empty().with_escalation::<cap::UserAdmin>();
    let auth = authorize::<cap::UserAdmin>(register, Some(approval_id), &binding, &policy, at)?;
    let (mut db, session) = register.lock();
    signed_in_as(session.as_ref(), auth.actor())?;
    let actor_grants = session
        .as_ref()
        .map(|session| session.grants.clone())
        .ok_or_else(not_signed_in)?;
    drop(session);

    let account = UserRepository::new(&db).by_id(user_id).map_err(stored)?;
    if !account.is_some_and(|account| account.is_live) {
        return Err(user_unknown());
    }
    let roles = RoleRepository::new(&db)
        .roles_at(user_id, register.store())
        .map_err(stored)?;
    if roles.is_empty() {
        return Err(user_unknown());
    }
    let held: Vec<GrantSet> = roles
        .iter()
        .map(|role| RoleRepository::new(&db).grants(*role))
        .collect::<Result<_, DbError>>()
        .map_err(stored)?;
    if beyond_reach(&actor_grants, &held) {
        return Err(permission_refused(&PermissionError::OutsideLimit(
            cap::UserAdmin::NAME,
            Limit::OwnStore,
        )));
    }
    let needs = minimum_pin_digits(
        cap::ALL
            .iter()
            .copied()
            .filter(|capability| held.iter().any(|grants| grants.holds(capability))),
    );
    check_new_pin(new_pin, needs).map_err(|error| pin_refused(&error, needs))?;
    let phc = hash_pin(new_pin).map_err(|error| pin_refused(&error, needs))?;

    let tx = db.transaction().map_err(sqlite)?;
    // The approver's reason, which the audit row must repeat to spend it.
    let reason = ApprovalRepository::new(&tx)
        .load_for_consumption(&tx, approval_id)
        .map_err(stored)?
        .ok_or_else(|| IpcError::new("approval_unknown", "error.approval.unknown"))?
        .reason()
        .to_owned();
    let users = UserRepository::new(&tx);
    let before = users
        .attempts(user_id, register.register())
        .map_err(stored)?;
    users.set_pin_hash(&tx, user_id, &phc, at).map_err(stored)?;
    users
        .release(&tx, user_id, register.register(), at)
        .map_err(stored)?;
    audit(
        register,
        &tx,
        &AuditIntent {
            actor: auth.actor(),
            approver: auth.approver(),
            approval: auth.approval(),
            action: cap::UserAdmin::NAME,
            entity: "app_user",
            entity_id: user_id.as_uuid(),
            reason: Some(reason),
            payload: json!({
                "amount_minor": 0,
                "effect": EFFECT,
                "failed_attempts_cleared": before.failed_attempts,
                "pin_profile_version": PIN_PROFILE.version,
            }),
            at,
        },
    )?;
    tx.commit().map_err(sqlite)?;
    Ok(())
}

/// Whether an account holding `held` is beyond a resetter holding `actor`:
/// the resetter's `user.admin` is limited to their own store, and the
/// account's is held without that limit.
fn beyond_reach(actor: &GrantSet, held: &[GrantSet]) -> bool {
    let own_store = actor.grant(cap::UserAdmin::NAME).limit() == Some(Limit::OwnStore);
    own_store
        && held.iter().any(|grants| {
            let admin = grants.grant(cap::UserAdmin::NAME);
            admin.is_held() && admin.limit() != Some(Limit::OwnStore)
        })
}

fn user_unknown() -> IpcError {
    IpcError::new("user_unknown", "error.user.unknown")
}

/// A PIN the account cannot take, or one that could not be hashed. Neither
/// carries the PIN: [`AuthError`] holds only the digits an account needs.
fn pin_refused(error: &AuthError, needs: usize) -> IpcError {
    match error {
        AuthError::PinShape { .. } => {
            let failure = IpcError::from_source("pin_shape", "error.auth.pin_shape", error);
            if needs >= MANAGER_PIN_DIGITS {
                failure.with_detail("this account needs six digits or more")
            } else {
                failure.with_detail("this account needs four digits or more")
            }
        }
        AuthError::UnreadableHash | AuthError::NotArgon2id | AuthError::Hashing(_) => {
            IpcError::from_source("auth_unavailable", "error.auth.unavailable", error)
        }
    }
}
