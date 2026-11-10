//! Staff accounts: `user_reset_pin` (`ref/ipc-contract.md` §3, "Session and
//! users"; microstep 1.6.2, its command, and E.51).
//!
//! **A cashier who forgets their PIN is reset by a manager,** audited, with
//! the reason. Otherwise "forgot PIN" is either an account nobody can use
//! again or a credential taken over without a trace.
//!
//! - **One person is enough, unless the merchant's policy names
//!   `user.admin`** (the owner's decision of 10 November 2026, as for a price
//!   override). A holder of `user.admin` resets under their own PIN, six
//!   digits or more, and names the reason, which the audit row records. That
//!   is merchant decision 9.6's default, which stands until the merchant
//!   answers #233: a manager's PIN and an audited reason. Otherwise a shop
//!   with one manager could never reset a forgotten PIN, since nobody
//!   approves their own.
//! - **The reason is a code from a fixed list** (the owner's decision of the
//!   same day): `forgot_pin`, `locked_out` or `pin_disclosed`. Free text is
//!   unreportable, and a name typed there would stay in the audit chain for
//!   good.
//! - **Where the policy names `user.admin`, a second person approves.** The
//!   approval names the account being reset, an amount of exactly zero, and
//!   the same reason, and it is spent with the audit row. The approver can
//!   never be the person resetting (E.52).
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
use crate::ipc::context::{authorize, merchant_policy, not_signed_in, permission_refused};
use crate::ipc::{IpcError, IpcResult};
use crate::state::{Register, Till};

/// What a reset is recorded as having changed.
const EFFECT: &str = "pin_reset";

/// Why a PIN may be reset, by the code the screen sends and the audit row
/// records (the owner's decision of 10 November 2026).
pub const PIN_RESET_REASONS: [&str; 3] = ["forgot_pin", "locked_out", "pin_disclosed"];

#[tauri::command]
pub fn user_reset_pin(
    till: State<'_, Till>,
    user_id: UserId,
    new_pin: String,
    reason: String,
    approval_id: Option<ApprovalId>,
) -> IpcResult<()> {
    reset_pin(
        till.register()?,
        user_id,
        &new_pin,
        &reason,
        approval_id,
        &merchant_policy(),
    )
}

/// `user_reset_pin`: give `user_id` the PIN `new_pin`, under the signed-in
/// user's `user.admin`, for `reason`, one of [`PIN_RESET_REASONS`]. Where
/// `policy` names `user.admin`, the approval `approval_id` must name this
/// account, an amount of zero and the same reason, and come from someone
/// else. Where it does not, no approval is needed, and one offered is not
/// spent.
///
/// # Errors
///
/// What the context refuses: `permission_denied` without `user.admin`, and
/// under an escalating policy `approval_required` and the approval's own
/// refusals (`approval_unknown`, `approval_used`, `approval_mismatch`,
/// `approval_self`, `approval_expired` and `approval_revoked`).
/// `pin_reset_reason_unknown` for a reason that is not on the list;
/// `user_unknown` for an account that is not live or holds no role at this
/// store; `permission_outside_limit` when a store's administrator would reset
/// an organisation's; `pin_shape` for a PIN that is not all digits or is too
/// short for the account, with the digits it needs; `approval_mismatch` for
/// an approval given for another reason; `auth_unavailable` when the PIN
/// cannot be hashed; and `storage_failed`. A refused reset writes nothing and
/// leaves any approval unspent.
pub fn reset_pin(
    register: &Register,
    user_id: UserId,
    new_pin: &str,
    reason: &str,
    approval_id: Option<ApprovalId>,
    policy: &EscalationPolicy,
) -> IpcResult<()> {
    let at = register.now();
    let binding = ApprovalBinding {
        entity_id: user_id.as_uuid(),
        amount_minor: 0,
        content_hash: None,
    };
    let auth = authorize::<cap::UserAdmin>(register, approval_id, &binding, policy, at)?;
    if !PIN_RESET_REASONS.contains(&reason) {
        return Err(IpcError::new(
            "pin_reset_reason_unknown",
            "error.user.pin_reset_reason_unknown",
        ));
    }
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
            // An approval is spent only beside an audit row that repeats its
            // reason, so one given for another reason refuses the reset here
            // and the transaction writes nothing.
            reason: Some(reason.to_owned()),
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
