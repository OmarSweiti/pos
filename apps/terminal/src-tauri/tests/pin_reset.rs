#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A forgotten PIN, reset through the shell (microstep 1.6.2, its command,
//! and E.51): `user_reset_pin` on a register file built through `pos_db::open`,
//! the registered migration chain, with synthetic staff whose PIN is the
//! fixture's.
//!
//! Every approval here is stored as the approval screen will store one, behind
//! its envelope: `auth_verify_pin`, which issues them at the counter, is
//! 1.11.4b's.

mod shop;

use pos_db::auth::verify_pin;
use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::user::UserRepository;
use pos_domain::{
    ApprovalBinding, ApprovalHandle, ApprovalId, EscalationPolicy, GrantSet, RegisterId, Role,
    UserId, cap,
};
use rusqlite::{OptionalExtension, params};
use serde_json::json;
use shop::{
    AT, CASHIER_CODE, CASHIER_ID, CASHIER_PIN, MANAGER_CODE, MANAGER_ID, OWNER_CODE, OWNER_ID,
    REGISTER_ID, SECOND_CODE, SECOND_ID, add_staff, at, audited, field, open, refusal, shop,
};
use terminal_lib::commands::auth::{login_pin, logout};
use terminal_lib::commands::user::reset_pin;
use terminal_lib::state::Register;
use uuid::Uuid;

/// A second manager, who can approve the first's resets and be reset.
const DEPUTY_ID: [u8; 16] = [0xAB; 16];
const DEPUTY_CODE: &str = "M-200";
/// Another register of the same store, whose attempt state a reset here
/// leaves alone.
const OTHER_REGISTER: [u8; 16] = [0x53; 16];
/// A live account that holds no role at this store.
const ROLELESS_ID: [u8; 16] = [0xC3; 16];
const NEW_PIN: &str = "735104";
const REASON: &str = "forgot their PIN";

fn user(id: [u8; 16]) -> UserId {
    UserId::from_uuid(Uuid::from_bytes(id))
}

/// The shop, its cashier signed out, with a manager, a deputy manager and
/// the owner beside them, and the manager signed in.
fn office() -> (tempfile::TempDir, Register) {
    let (dir, register) = shop();
    logout(&register).unwrap();
    add_staff(
        &register,
        MANAGER_ID,
        MANAGER_CODE,
        "Synthetic Manager",
        "manager",
    );
    add_staff(
        &register,
        DEPUTY_ID,
        DEPUTY_CODE,
        "Synthetic Deputy",
        "manager",
    );
    add_staff(&register, OWNER_ID, OWNER_CODE, "Synthetic Owner", "owner");
    login_pin(&register, MANAGER_CODE, CASHIER_PIN).unwrap();
    (dir, register)
}

/// An approval from `approver`, holding `role`, for `actor` to reset `target`,
/// for `amount_minor` and `reason`, stored behind its envelope.
fn approval(
    register: &Register,
    actor: [u8; 16],
    (approver, role): ([u8; 16], Role),
    target: [u8; 16],
    amount_minor: i64,
    reason: &str,
) -> ApprovalId {
    let id = ApprovalId::from_uuid(register.next_id());
    let binding = ApprovalBinding {
        entity_id: Uuid::from_bytes(target),
        amount_minor,
        content_hash: None,
    };
    let proof = pos_domain::authorize::<cap::UserAdmin>(
        user(approver),
        &GrantSet::of_role(role),
        None,
        &binding,
        &EscalationPolicy::empty(),
        at(),
    )
    .unwrap();
    // The nonce is derived from a fresh id, never written as a literal.
    let nonce = *register.next_id().as_bytes();
    let handle = ApprovalHandle::issue::<cap::UserAdmin>(
        id,
        user(actor),
        &proof,
        &binding,
        reason.to_owned(),
        at(),
        60_000,
        nonce,
    )
    .unwrap();
    register.with_db(|conn| {
        let tx = conn.transaction().unwrap();
        let (change, commit) = (register.next_id(), register.next_id());
        OutboxRepository::new(&tx)
            .write_commit(
                &tx,
                &CommitEnvelope {
                    commit_id: commit.as_bytes(),
                    protocol_version: 1,
                    schema_version: pos_db::SCHEMA_VERSION,
                    producer_version: "pin-reset-suite",
                    created_at: AT,
                },
                &[FactMember {
                    change_id: change.as_bytes(),
                    entity: "approval_handle",
                    entity_id: id.as_uuid().as_bytes(),
                    payload: "{}",
                }],
            )
            .unwrap();
        ApprovalRepository::new(&tx).insert(&tx, &handle).unwrap();
        tx.commit().unwrap();
    });
    id
}

/// The owner's approval of the manager's reset of `target`.
fn owners_approval(register: &Register, target: [u8; 16]) -> ApprovalId {
    approval(
        register,
        MANAGER_ID,
        (OWNER_ID, Role::Owner),
        target,
        0,
        REASON,
    )
}

/// `failures` wrong PINs by `who` on `register`, recorded as sign-in records
/// them.
fn fail(register: &Register, who: [u8; 16], on: [u8; 16], failures: u32) {
    register.with_db(|conn| {
        let tx = conn.transaction().unwrap();
        for _ in 0..failures {
            UserRepository::new(&tx)
                .record_failure(
                    &tx,
                    user(who),
                    RegisterId::from_uuid(Uuid::from_bytes(on)),
                    at(),
                )
                .unwrap();
        }
        tx.commit().unwrap();
    });
}

fn failed_attempts(register: &Register, who: [u8; 16], on: [u8; 16]) -> u32 {
    register.with_db(|conn| {
        UserRepository::new(conn)
            .attempts(user(who), RegisterId::from_uuid(Uuid::from_bytes(on)))
            .unwrap()
            .failed_attempts
    })
}

/// Whether `pin` is the account's PIN now. The hash is read and checked here,
/// and never printed.
fn pin_is(register: &Register, who: [u8; 16], pin: &str) -> bool {
    register.with_db(|conn| {
        let stored = UserRepository::new(conn)
            .pin_hash(user(who))
            .unwrap()
            .unwrap();
        verify_pin(pin, &stored).unwrap()
    })
}

/// The audit row a reset wrote, as it names the people, the approval, the
/// account and the reason.
#[derive(Debug, PartialEq, Eq)]
struct ResetRow {
    actor: Vec<u8>,
    approver: Vec<u8>,
    approval: Vec<u8>,
    entity: String,
    entity_id: Vec<u8>,
    reason: String,
}

fn reset_row(register: &Register) -> Option<ResetRow> {
    register.with_db(|conn| {
        conn.query_row(
            "SELECT actor_id, approver_id, approval_handle_id, entity, entity_id, reason
               FROM audit_log WHERE action = 'user.admin'",
            [],
            |row| {
                Ok(ResetRow {
                    actor: row.get(0)?,
                    approver: row.get(1)?,
                    approval: row.get(2)?,
                    entity: row.get(3)?,
                    entity_id: row.get(4)?,
                    reason: row.get(5)?,
                })
            },
        )
        .optional()
        .unwrap()
    })
}

fn spent(register: &Register, approval: ApprovalId) -> bool {
    register.with_db(|conn| {
        conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM approval_consumption WHERE handle_id = ?1)",
            [approval.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap()
    })
}

/// E.51: a cashier who forgets their PIN, and has locked themselves out
/// trying, is reset by a manager with the owner's approval. The old PIN stops
/// working, the new one signs in, and the lockout is released: by this path
/// only, since a restart does not. The audit row names the manager, the owner,
/// the approval and its reason, and carries neither PIN nor hash; the
/// approval is spent with it. A manager's own account needs six digits, so a
/// four-digit PIN for one is refused. Nothing unrelated is cleared: another
/// account's attempts here, and the cashier's on another register, stand.
#[test]
fn manager_reset_retires_old_hash_and_audits() {
    let (dir, register) = office();
    register.with_db(|conn| {
        conn.execute(
            "INSERT INTO register
               (id, store_id, code, name, device_id, credential_key_id,
                credential_algorithm, credential_public_key, credential_issued_at)
             SELECT ?1, store_id, 'REG02', 'REG02', 'device-REG02', 'key-REG02',
                    'ed25519', ?2, ?3
               FROM register WHERE id = ?4",
            params![
                OTHER_REGISTER.as_slice(),
                vec![0x98u8; 32],
                AT,
                REGISTER_ID.as_slice()
            ],
        )
        .unwrap();
    });
    add_staff(
        &register,
        SECOND_ID,
        SECOND_CODE,
        "Synthetic Second",
        "cashier",
    );
    fail(&register, CASHIER_ID, REGISTER_ID, 10);
    fail(&register, CASHIER_ID, OTHER_REGISTER, 2);
    fail(&register, SECOND_ID, REGISTER_ID, 2);
    logout(&register).unwrap();
    assert_eq!(
        refusal(login_pin(&register, CASHIER_CODE, CASHIER_PIN)).code,
        "auth_locked_out"
    );

    // A restart releases nothing.
    drop(register);
    let register = open(dir.path());
    assert_eq!(
        refusal(login_pin(&register, CASHIER_CODE, CASHIER_PIN)).code,
        "auth_locked_out"
    );
    login_pin(&register, MANAGER_CODE, CASHIER_PIN).unwrap();

    // A manager's account needs six digits.
    let deputy = owners_approval(&register, DEPUTY_ID);
    let short = refusal(reset_pin(&register, user(DEPUTY_ID), "4826", deputy));
    assert_eq!(
        (short.code, short.detail),
        ("pin_shape", Some("this account needs six digits or more"))
    );
    assert!(!spent(&register, deputy), "a refused reset spends nothing");

    let granted = owners_approval(&register, CASHIER_ID);
    reset_pin(&register, user(CASHIER_ID), NEW_PIN, granted).unwrap();
    assert!(
        !pin_is(&register, CASHIER_ID, CASHIER_PIN),
        "the old hash is retired"
    );
    assert!(pin_is(&register, CASHIER_ID, NEW_PIN));
    assert_eq!(failed_attempts(&register, CASHIER_ID, REGISTER_ID), 0);
    assert_eq!(failed_attempts(&register, CASHIER_ID, OTHER_REGISTER), 2);
    assert_eq!(failed_attempts(&register, SECOND_ID, REGISTER_ID), 2);

    assert_eq!(
        reset_row(&register),
        Some(ResetRow {
            actor: MANAGER_ID.to_vec(),
            approver: OWNER_ID.to_vec(),
            approval: granted.as_uuid().as_bytes().to_vec(),
            entity: "app_user".to_owned(),
            entity_id: CASHIER_ID.to_vec(),
            reason: REASON.to_owned(),
        })
    );
    let rows = audited(&register, "user.admin");
    let row = rows.first().unwrap();
    assert_eq!(field(row, "amount_minor"), &json!(0));
    assert_eq!(field(row, "effect"), &json!("pin_reset"));
    assert_eq!(field(row, "failed_attempts_cleared"), &json!(10));
    assert_eq!(field(row, "pin_profile_version"), &json!(1));
    let text = row.to_string();
    assert!(!text.contains(NEW_PIN), "the audit row carries no PIN");
    assert!(!text.contains("argon2"), "the audit row carries no hash");
    assert!(spent(&register, granted));
    assert_eq!(
        refusal(reset_pin(&register, user(CASHIER_ID), "246813", granted)).code,
        "approval_used"
    );

    logout(&register).unwrap();
    assert_eq!(
        refusal(login_pin(&register, CASHIER_CODE, CASHIER_PIN)).code,
        "auth_refused"
    );
    login_pin(&register, CASHIER_CODE, NEW_PIN).unwrap();
}

/// Every reset needs `user.admin` and an approval bound to it: this account,
/// exactly zero, issued to the person resetting. A cashier holds no
/// `user.admin`; an approval for another account, another amount or another
/// resetter is refused, and so is a missing one. Each refusal leaves the PIN
/// and the approval as they were.
#[test]
fn a_reset_needs_its_own_bound_approval() {
    let (_dir, register) = office();
    let for_the_deputy = owners_approval(&register, DEPUTY_ID);
    let for_ten_fils = approval(
        &register,
        MANAGER_ID,
        (OWNER_ID, Role::Owner),
        CASHIER_ID,
        10,
        REASON,
    );
    let for_the_deputy_to_use = approval(
        &register,
        DEPUTY_ID,
        (OWNER_ID, Role::Owner),
        CASHIER_ID,
        0,
        REASON,
    );
    for (wrong, detail) in [
        (for_the_deputy, "the approval names something else"),
        (for_ten_fils, "the approval is for another amount"),
        (
            for_the_deputy_to_use,
            "the approval was given to someone else",
        ),
    ] {
        let refused = refusal(reset_pin(&register, user(CASHIER_ID), NEW_PIN, wrong));
        assert_eq!(
            (refused.code, refused.detail),
            ("approval_mismatch", Some(detail))
        );
        assert!(!spent(&register, wrong));
    }
    let missing = ApprovalId::from_uuid(Uuid::from_u128(0xAB5E));
    assert_eq!(
        refusal(reset_pin(&register, user(CASHIER_ID), NEW_PIN, missing)).code,
        "approval_unknown"
    );

    // A cashier holds no `user.admin`, whatever approval they carry.
    let granted = owners_approval(&register, SECOND_ID);
    logout(&register).unwrap();
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(
        refusal(reset_pin(&register, user(SECOND_ID), NEW_PIN, granted)).code,
        "permission_denied"
    );
    logout(&register).unwrap();
    assert_eq!(
        refusal(reset_pin(&register, user(SECOND_ID), NEW_PIN, granted)).code,
        "not_signed_in"
    );
    assert!(pin_is(&register, CASHIER_ID, CASHIER_PIN));
    assert!(!spent(&register, granted));
    assert_eq!(reset_row(&register), None);
}

/// A store's administrator does not reset the organisation's: a manager,
/// whose `user.admin` is their own store's, is refused the owner's account,
/// approval or not, while the owner may reset a manager's. A reset reaches
/// only a live account with a role here, and a new PIN is all digits and as
/// long as its account's grants need.
#[test]
fn a_reset_reaches_only_the_accounts_it_may() {
    let (_dir, register) = office();
    let deputys = approval(
        &register,
        MANAGER_ID,
        (DEPUTY_ID, Role::Manager),
        OWNER_ID,
        0,
        REASON,
    );
    let refused = refusal(reset_pin(&register, user(OWNER_ID), NEW_PIN, deputys));
    assert_eq!(refused.code, "permission_outside_limit");
    assert!(!spent(&register, deputys));

    // An account that is not live, one live with no role at this store, and
    // no account at all.
    add_staff(
        &register,
        SECOND_ID,
        SECOND_CODE,
        "Synthetic Second",
        "cashier",
    );
    register.with_db(|conn| {
        conn.execute(
            "UPDATE app_user SET is_active = 0 WHERE id = ?1",
            [SECOND_ID.as_slice()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
             SELECT ?1, org_id, 'C-300', 'Synthetic Roleless', pin_hash, pin_set_at
               FROM app_user WHERE id = ?2",
            params![ROLELESS_ID.as_slice(), CASHIER_ID.as_slice()],
        )
        .unwrap();
    });
    for gone in [SECOND_ID, ROLELESS_ID, [0x5A; 16]] {
        let granted = owners_approval(&register, gone);
        assert_eq!(
            refusal(reset_pin(&register, user(gone), NEW_PIN, granted)).code,
            "user_unknown"
        );
    }

    // A PIN is all digits, as long as its account needs. Under the shipped
    // grid a cashier holds `refund.receipted` within its threshold, a
    // manager's capability, so a cashier needs six digits too.
    for bad in ["73510", "73a510", "٧٣٥١٠٤"] {
        let granted = owners_approval(&register, CASHIER_ID);
        let refused = refusal(reset_pin(&register, user(CASHIER_ID), bad, granted));
        assert_eq!(
            (refused.code, refused.detail),
            ("pin_shape", Some("this account needs six digits or more")),
            "{bad:?} is refused"
        );
    }
    // The grants decide, not the role's name: where the merchant withholds
    // that cell, a cashier's four digits are enough.
    register.with_db(|conn| {
        conn.execute(
            "UPDATE role_capability SET decision = 'withheld', limit_json = NULL
              WHERE capability = 'refund.receipted'
                AND role_id = (SELECT id FROM role WHERE code = 'cashier')",
            [],
        )
        .unwrap();
    });
    let granted = owners_approval(&register, CASHIER_ID);
    let refused = refusal(reset_pin(&register, user(CASHIER_ID), "735", granted));
    assert_eq!(
        (refused.code, refused.detail),
        ("pin_shape", Some("this account needs four digits or more"))
    );
    reset_pin(&register, user(CASHIER_ID), "7351", granted).unwrap();
    assert!(pin_is(&register, CASHIER_ID, "7351"));

    // The owner, whose reach is the whole organisation, resets a manager.
    logout(&register).unwrap();
    login_pin(&register, OWNER_CODE, CASHIER_PIN).unwrap();
    let managers = approval(
        &register,
        OWNER_ID,
        (DEPUTY_ID, Role::Manager),
        MANAGER_ID,
        0,
        REASON,
    );
    reset_pin(&register, user(MANAGER_ID), NEW_PIN, managers).unwrap();
    assert!(pin_is(&register, MANAGER_ID, NEW_PIN));
}
