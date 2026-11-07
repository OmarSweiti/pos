//! The command context: every command takes its proof here (1.8.2c, and
//! 1.6.4's deferred half).
//!
//! [`authorize`] is the one path from a webview call to an `Authorized<C>`.
//! It refuses a command when no one is signed in, signs out a user whose
//! account has stopped being live, resolves a privileged command's
//! `approval_id` from storage, and hands both to the domain's `authorize`,
//! which holds the handle to the command's binding before any effect. An
//! approval that authorizes is then refused if its approver's account has
//! stopped being live: deactivating an approver revokes every handle they
//! issued that has not been spent (1.6.4, the owner's decision of
//! 5 November 2026).
//!
//! It resolves an approval; it does not spend one. The command writes the
//! `approval_consumption` row in its effect's transaction, beside the audit
//! row (`ref/ipc-contract.md` §1, rule 7).

use pos_db::DbError;
use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::user::UserRepository;
use pos_domain::{
    ApprovalBinding, ApprovalHandle, ApprovalId, Authorized, Capability, EscalationPolicy,
    PermissionError, Timestamp, UserId,
};
use rusqlite::Connection;

use crate::commands::auth::{SignOutCause, sign_out};
use crate::ipc::{IpcError, IpcResult};
use crate::state::Register;

/// Proof that the signed-in user may run `C` at `at`, with the approval
/// `approval` names when the policy escalates `C`.
///
/// # Errors
///
/// - `not_signed_in` when no one is.
/// - `user_inactive` when the signed-in account is no longer live. The
///   session is ended first, audited, and what the till was working on is
///   set aside, except a sale with money taken, which stays journalled for
///   recovery.
/// - `approval_unknown`, `approval_used` or `approval_unreadable` for an
///   `approval_id` that names no handle, a consumed one, or a row this build
///   cannot read.
/// - `approval_revoked` when the approval would authorize, and its approver's
///   account is no longer live. Nothing is written: the handle stays unspent,
///   and the signed-in user stays signed in.
/// - Each [`PermissionError`] as its own code (see [`permission_refused`]).
pub fn authorize<C: Capability>(
    register: &Register,
    approval: Option<ApprovalId>,
    binding: &ApprovalBinding,
    policy: &EscalationPolicy,
    at: Timestamp,
) -> IpcResult<Authorized<C>> {
    let (mut db, mut session) = register.lock();
    let Some(current) = session.clone() else {
        return Err(not_signed_in());
    };

    let account = UserRepository::new(&db)
        .by_id(current.user)
        .map_err(|error| {
            IpcError::from_source("session_unreadable", "error.session.unreadable", &error)
        })?;
    if !account.is_some_and(|account| account.is_live) {
        let mut working = register.working_after(&db, &session);
        sign_out(
            register,
            &mut db,
            &current,
            &mut working,
            SignOutCause::UserInactive,
            at,
        )?;
        drop(working);
        *session = None;
        return Err(IpcError::new("user_inactive", "error.auth.user_inactive"));
    }

    let handle = approval.map(|id| resolve(&mut db, id)).transpose()?;
    let authorized = pos_domain::authorize::<C>(
        current.user,
        &current.grants,
        handle.as_ref(),
        binding,
        policy,
        at,
    )
    .map_err(|refusal| permission_refused(&refusal))?;
    // Only a handle the domain accepted is used, so only its approver is
    // read: an id supplied to a command that does not escalate is ignored.
    if let Some(approver) = authorized.approver() {
        ensure_approver_live(&db, approver)?;
    }
    Ok(authorized)
}

/// The merchant's escalation policy: which capabilities a user who holds
/// them may still use only with a second person's approval (merchant
/// decision 1.8).
///
/// No merchant's policy is stored yet, so it names none. On 6 November 2026
/// the owner decided that a price override needs no second person until one
/// is: a shift lead or a manager overrides under their own `price.override`,
/// audited. A command whose approval this policy decides takes the policy as
/// an argument of its plain function, so its tests drive the branch that
/// escalates, and the command's Tauri wrapper passes this one.
#[must_use]
pub const fn merchant_policy() -> EscalationPolicy {
    EscalationPolicy::empty()
}

/// What a command answers when no one is signed in.
pub fn not_signed_in() -> IpcError {
    IpcError::new("not_signed_in", "error.session.not_signed_in")
}

/// The stored handle `id` names, refused if it was already spent.
fn resolve(db: &mut Connection, id: ApprovalId) -> IpcResult<ApprovalHandle> {
    let unreadable = |error: &dyn std::error::Error| {
        IpcError::from_source("approval_unreadable", "error.approval.unreadable", error)
    };
    let tx = db.transaction().map_err(|error| unreadable(&error))?;
    let approvals = ApprovalRepository::new(&tx);
    let handle = approvals
        .load_for_consumption(&tx, id)
        .map_err(|error| unreadable(&error))?
        .ok_or_else(|| IpcError::new("approval_unknown", "error.approval.unknown"))?;
    approvals
        .ensure_unconsumed(&tx, id)
        .map_err(|error| match error {
            DbError::ApprovalAlreadyConsumed => {
                IpcError::new("approval_used", "error.approval.used")
            }
            other => unreadable(&other),
        })?;
    // Nothing was written: dropping the transaction ends it.
    Ok(handle)
}

/// Refuses an approval whose approver's account is not live: deactivated, or
/// with a `deleted_at`, or gone. It reads the account as it is when the
/// approval is used, so the refusal holds while the account is not live, and
/// every handle expires on its own.
fn ensure_approver_live(db: &Connection, approver: UserId) -> IpcResult<()> {
    let account = UserRepository::new(db).by_id(approver).map_err(|error| {
        IpcError::from_source("approval_unreadable", "error.approval.unreadable", &error)
    })?;
    if account.is_some_and(|account| account.is_live) {
        Ok(())
    } else {
        Err(permission_refused(&PermissionError::ApproverInactive(
            approver,
        )))
    }
}

/// A [`PermissionError`] as the webview receives it: a code to branch on, a
/// key to translate, a static detail where one helps, and never the error's
/// text, which names users and amounts (1.6.8). The source goes to the
/// scrubbed log under the trace id.
///
/// No wildcard arm: a refusal the domain adds is a decision here.
pub fn permission_refused(refusal: &PermissionError) -> IpcError {
    let (code, key, detail) = match refusal {
        PermissionError::NotHeld(_) | PermissionError::Denied(..) => {
            ("permission_denied", "error.permission.denied", None)
        }
        PermissionError::OutsideLimit(..) => (
            "permission_outside_limit",
            "error.permission.outside_limit",
            None,
        ),
        PermissionError::EscalationRequired(_) => {
            ("approval_required", "error.approval.required", None)
        }
        PermissionError::SelfApprovalBanned(_) => ("approval_self", "error.approval.self", None),
        PermissionError::UserInactive(_) => ("user_inactive", "error.auth.user_inactive", None),
        PermissionError::ApproverInactive(_) => (
            "approval_revoked",
            "error.approval.revoked",
            Some("the approver's account is no longer active"),
        ),
        PermissionError::OfflineAuthExpired => (
            "offline_auth_expired",
            "error.permission.offline_auth_expired",
            None,
        ),
        PermissionError::ApprovalCapabilityMismatch(..) => (
            "approval_mismatch",
            "error.approval.mismatch",
            Some("the approval is for another operation"),
        ),
        PermissionError::ApprovalActorMismatch(..) => (
            "approval_mismatch",
            "error.approval.mismatch",
            Some("the approval was given to someone else"),
        ),
        PermissionError::ApprovalEntityMismatch(..) => (
            "approval_mismatch",
            "error.approval.mismatch",
            Some("the approval names something else"),
        ),
        PermissionError::ApprovalAmountMismatch(..) => (
            "approval_mismatch",
            "error.approval.mismatch",
            Some("the approval is for another amount"),
        ),
        PermissionError::ApprovalContentHashMismatch => (
            "approval_mismatch",
            "error.approval.mismatch",
            Some("what was approved has changed"),
        ),
        PermissionError::ApprovalValidityIntervalInvalid(..) => {
            ("approval_unreadable", "error.approval.unreadable", None)
        }
        PermissionError::ApprovalNotYetValid(_) => (
            "approval_not_yet_valid",
            "error.approval.not_yet_valid",
            None,
        ),
        PermissionError::ApprovalExpired(_) => ("approval_expired", "error.approval.expired", None),
        PermissionError::ApprovalAlreadyUsed(_) => ("approval_used", "error.approval.used", None),
    };
    let failure = IpcError::from_source(code, key, refusal);
    match detail {
        Some(detail) => failure.with_detail(detail),
        None => failure,
    }
}
