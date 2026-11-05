//! The shell's database and session (microstep 1.8.2c).
//!
//! Every fixture is a fresh register file built through `pos_db::open`, the
//! registered migration chain, with synthetic staff whose PINs are hashed at
//! the reviewed profile. The register is opened the way `run` opens it, with
//! the key and the device id beside the file, and every command is driven
//! through the plain function its Tauri wrapper calls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use pos_db::fixtures::registered_chain::RegisteredChain;
use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::audit::AuditRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::user::UserRepository;
use pos_domain::{
    ApprovalBinding, ApprovalHandle, ApprovalId, AuditIntent, Capability, EscalationPolicy,
    FixedClock, GrantSet, Role, Timestamp, UserId, cap,
};
use rusqlite::{Transaction, params};
use serde_json::json;
use terminal_lib::commands::auth::{SessionState, login_pin, logout, state_of};
use terminal_lib::ipc::IpcError;
use terminal_lib::ipc::context::authorize;
use terminal_lib::state::{
    CacheCheck, DATABASE_FILE, DEVICE_ID_FILE, Register, StartupError, Till, V7Ids,
};
use uuid::Uuid;

const KEY: &str = "session-suite-key";
const AT: &str = "2026-11-04T09:00:00.000Z";
const DEVICE_ID: &str = "device-REG01";
const REGISTER_ID: [u8; 16] = [0x52; 16];

/// One synthetic member of staff.
struct Staff {
    id: [u8; 16],
    code: &'static str,
    name: &'static str,
    pin: &'static str,
}

const CASHIER: Staff = Staff {
    id: [0xC1; 16],
    code: "C-100",
    name: "Synthetic Cashier",
    pin: "482916",
};
const MANAGER: Staff = Staff {
    id: [0xA1; 16],
    code: "M-200",
    name: "Synthetic Manager",
    pin: "731905",
};
const TWO_ROLES: Staff = Staff {
    id: [0xD2; 16],
    code: "X-300",
    name: "Synthetic Two Roles",
    pin: "264813",
};
const NO_ROLE: Staff = Staff {
    id: [0xD0; 16],
    code: "N-400",
    name: "Synthetic No Role",
    pin: "264813",
};
const ELSEWHERE: Staff = Staff {
    id: [0xE5; 16],
    code: "E-500",
    name: "Synthetic Other Store",
    pin: "264813",
};
const SAME_ROLE_TWICE: Staff = Staff {
    id: [0xD6; 16],
    code: "D-600",
    name: "Synthetic Same Role Twice",
    pin: "264813",
};

fn at() -> Timestamp {
    Timestamp::parse_iso8601(AT).unwrap()
}

fn user(staff: &Staff) -> UserId {
    UserId::from_uuid(Uuid::from_bytes(staff.id))
}

/// A PIN's hash at the reviewed profile. Hashing is the slow part of a
/// fixture, so each PIN is hashed once per run.
fn hash_of(pin: &str) -> String {
    static HASHES: OnceLock<Vec<(&'static str, String)>> = OnceLock::new();
    HASHES
        .get_or_init(|| {
            [CASHIER.pin, MANAGER.pin, NO_ROLE.pin]
                .into_iter()
                .map(|pin| (pin, pos_db::auth::hash_pin(pin).unwrap()))
                .collect()
        })
        .iter()
        .find(|(known, _)| *known == pin)
        .map(|(_, hash)| hash.clone())
        .unwrap()
}

/// A register file in `dir`: the registered chain's org and store, a second
/// store of the same org, this install's register, the staff, their roles,
/// and the device id beside the file.
fn seed(dir: &Path) {
    let conn = pos_db::open(&dir.join(DATABASE_FILE), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    chain.add_register(&conn, &REGISTER_ID, "REG01");
    let other_store = [0x5B; 16];
    conn.execute(
        "INSERT INTO store
           (id, org_id, code, name_ar, fiscal_obligation, fiscal_obligation_evidence_ref,
            fiscal_profile, tax_rule_pack_id, tax_computation_policy_id)
         SELECT ?1, org_id, 'FIX-S2', 'متجر آخر', fiscal_obligation,
                fiscal_obligation_evidence_ref, fiscal_profile, tax_rule_pack_id,
                tax_computation_policy_id
           FROM store WHERE id = ?2",
        params![other_store.as_slice(), &chain.store],
    )
    .unwrap();
    for staff in [
        &CASHIER,
        &MANAGER,
        &TWO_ROLES,
        &NO_ROLE,
        &ELSEWHERE,
        &SAME_ROLE_TWICE,
    ] {
        conn.execute(
            "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                staff.id.as_slice(),
                &chain.org,
                staff.code,
                staff.name,
                hash_of(staff.pin),
                AT
            ],
        )
        .unwrap();
    }
    let here = Some(chain.store.as_slice());
    let org_wide = None;
    for (staff, role, store) in [
        (&CASHIER, "cashier", org_wide),
        (&MANAGER, "manager", here),
        (&TWO_ROLES, "cashier", org_wide),
        (&TWO_ROLES, "manager", here),
        (&ELSEWHERE, "cashier", Some(other_store.as_slice())),
        (&SAME_ROLE_TWICE, "cashier", org_wide),
        (&SAME_ROLE_TWICE, "cashier", here),
    ] {
        conn.execute(
            "INSERT INTO user_role (user_id, role_id, store_id)
             SELECT ?1, id, ?3 FROM role WHERE code = ?2",
            params![staff.id.as_slice(), role, store],
        )
        .unwrap();
    }
    drop(conn);
    fs::write(dir.join(DEVICE_ID_FILE), format!("{DEVICE_ID}\n")).unwrap();
}

fn open(dir: &Path) -> Result<Register, StartupError> {
    Register::open_with(
        dir,
        KEY,
        Box::new(FixedClock::new(at())),
        Box::new(V7Ids::default()),
    )
}

fn seeded() -> (tempfile::TempDir, Register) {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let register = open(dir.path()).unwrap();
    (dir, register)
}

fn refusal<T>(result: Result<T, IpcError>) -> IpcError {
    match result {
        Ok(_) => panic!("the call was expected to be refused"),
        Err(refusal) => refusal,
    }
}

fn startup_refusal(result: Result<Register, StartupError>) -> StartupError {
    match result {
        Ok(register) => panic!("the till was expected to stay closed: {register:?}"),
        Err(refusal) => refusal,
    }
}

fn sign_in(register: &Register, staff: &Staff) -> SessionState {
    login_pin(register, staff.code, staff.pin).unwrap()
}

fn any_binding() -> ApprovalBinding {
    ApprovalBinding {
        entity_id: Uuid::nil(),
        amount_minor: 0,
        content_hash: None,
    }
}

fn audited(register: &Register, action: &str) -> i64 {
    register.with_db(|conn| {
        conn.query_row(
            "SELECT count(*) FROM audit_log WHERE action = ?1",
            [action],
            |row| row.get(0),
        )
        .unwrap()
    })
}

/// Every `user_session` row, oldest first: who, and how it ended.
fn sessions(register: &Register) -> Vec<(Vec<u8>, Option<String>)> {
    register.with_db(|conn| {
        conn.prepare("SELECT user_id, end_reason FROM user_session ORDER BY started_at, rowid")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    })
}

fn failed_attempts(register: &Register, staff: &Staff) -> u32 {
    register.with_db(|conn| {
        UserRepository::new(conn)
            .attempts(user(staff), register.register())
            .unwrap()
            .failed_attempts
    })
}

/// A ready delivery envelope naming `members`, as a privileged command's
/// transaction writes one.
fn envelope(register: &Register, tx: &Transaction<'_>, members: &[(&str, Uuid)]) {
    let changes: Vec<Uuid> = members.iter().map(|_| register.next_id()).collect();
    let facts: Vec<FactMember<'_>> = members
        .iter()
        .zip(&changes)
        .map(|((entity, id), change)| FactMember {
            change_id: change.as_bytes(),
            entity,
            entity_id: id.as_bytes(),
            payload: "{}",
        })
        .collect();
    let commit = register.next_id();
    OutboxRepository::new(tx)
        .write_commit(
            tx,
            &CommitEnvelope {
                commit_id: commit.as_bytes(),
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "session-suite",
                created_at: AT,
            },
            &facts,
        )
        .unwrap();
}

const REASON: &str = "customer asked at the counter";

/// A manager's approval of the cashier's manual discount on `binding`,
/// stored as the approval modal stores one.
fn approval(register: &Register, binding: &ApprovalBinding) -> ApprovalId {
    let id = ApprovalId::from_uuid(register.next_id());
    let manager = pos_domain::authorize::<cap::DiscountManual>(
        user(&MANAGER),
        &GrantSet::of_role(Role::Manager),
        None,
        binding,
        &EscalationPolicy::empty(),
        at(),
    )
    .unwrap();
    // The nonce is derived from a fresh id, never written as a literal.
    let nonce = *register.next_id().as_bytes();
    let handle = ApprovalHandle::issue::<cap::DiscountManual>(
        id,
        user(&CASHIER),
        &manager,
        binding,
        REASON.to_owned(),
        at(),
        60_000,
        nonce,
    )
    .unwrap();
    register.with_db(|conn| {
        let tx = conn.transaction().unwrap();
        envelope(register, &tx, &[("approval_handle", id.as_uuid())]);
        ApprovalRepository::new(&tx).insert(&tx, &handle).unwrap();
        tx.commit().unwrap();
    });
    id
}

/// Spend `id` on its effect, with the audit row consumption must name.
fn spend(register: &Register, id: ApprovalId, binding: &ApprovalBinding) {
    let audit_id = register.next_id();
    register.with_db(|conn| {
        let tx = conn.transaction().unwrap();
        envelope(
            register,
            &tx,
            &[
                ("audit_log", audit_id),
                ("approval_consumption", id.as_uuid()),
            ],
        );
        AuditRepository::new(&tx)
            .append(
                &tx,
                register.register(),
                audit_id,
                &AuditIntent {
                    actor: user(&CASHIER),
                    approver: Some(user(&MANAGER)),
                    approval: Some(id),
                    action: cap::DiscountManual::NAME,
                    entity: "sale_line",
                    entity_id: binding.entity_id,
                    reason: Some(REASON.to_owned()),
                    payload: json!({ "amount_minor": binding.amount_minor }),
                    at: at(),
                },
            )
            .unwrap();
        ApprovalRepository::new(&tx)
            .consume(&tx, id, binding.entity_id, audit_id, at())
            .unwrap();
        tx.commit().unwrap();
    });
}

/// The discount escalates, as a merchant's policy may make it.
fn discount_escalates() -> EscalationPolicy {
    EscalationPolicy::empty().with_escalation::<cap::DiscountManual>()
}

#[test]
fn the_register_database_is_opened_once_with_its_key() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());

    let register = open(dir.path()).unwrap();
    assert_eq!(register.register().as_uuid(), Uuid::from_bytes(REGISTER_ID));
    let store: Vec<u8> = register.with_db(|conn| {
        conn.query_row(
            "SELECT store_id FROM register WHERE device_id = ?1",
            [DEVICE_ID],
            |row| row.get(0),
        )
        .unwrap()
    });
    assert_eq!(register.store().as_uuid().as_bytes().as_slice(), store);
    assert_eq!(register.recovered().sessions_closed, 0);
    assert_eq!(register.recovered().carts_set_aside, 0);
    assert_eq!(register.recovered().empty_carts_dropped, 0);
    assert_eq!(register.recovered().stock_cache, CacheCheck::True);
    assert_eq!(
        state_of(&Till::Open(Box::new(register))),
        SessionState::SignedOut
    );

    // Another key does not open the same file, and says so.
    let refused = startup_refusal(Register::open_with(
        dir.path(),
        "another-key",
        Box::new(FixedClock::new(at())),
        Box::new(V7Ids::default()),
    ));
    assert!(
        matches!(refused, StartupError::Database(pos_db::DbError::BadKey)),
        "{refused:?}"
    );
    assert_eq!(refused.detail(), "wrong key or corrupt database");
}

#[test]
fn a_file_this_build_will_not_open_keeps_the_till_closed_by_name() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    // Another build altered the file: a table this build's chain never made.
    let conn = pos_db::open(&dir.path().join(DATABASE_FILE), KEY).unwrap();
    conn.execute_batch("CREATE TABLE altered_elsewhere (x INTEGER) STRICT;")
        .unwrap();
    drop(conn);

    let till = Till::from_result(open(dir.path()));
    let Till::Closed(refusal) = &till else {
        panic!("a file another build altered must not open");
    };
    assert!(
        matches!(
            refusal,
            StartupError::Database(pos_db::DbError::SchemaShapeMismatch { .. })
        ),
        "{refusal:?}"
    );
    assert_eq!(
        state_of(&till),
        SessionState::Closed {
            reason: "the database was altered by another build"
        }
    );
    // Every command answers with the refusal, by name, and runs nothing.
    let closed = refusal_of_till(&till);
    assert_eq!(closed.code, "till_closed");
    assert_eq!(
        closed.detail,
        Some("the database was altered by another build")
    );
}

fn refusal_of_till(till: &Till) -> IpcError {
    match till.register() {
        Ok(_) => panic!("the till was expected to be closed"),
        Err(refusal) => refusal,
    }
}

#[test]
fn an_install_with_no_register_of_its_own_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());

    // No device id: refused before any key is looked up or file opened.
    fs::remove_file(dir.path().join(DEVICE_ID_FILE)).unwrap();
    let missing = startup_refusal(Register::open(dir.path()));
    assert!(
        matches!(missing, StartupError::DeviceIdMissing),
        "{missing:?}"
    );
    assert_eq!(missing.detail(), "this install has no device id");

    fs::write(dir.path().join(DEVICE_ID_FILE), "  \n").unwrap();
    let blank = startup_refusal(open(dir.path()));
    assert!(
        matches!(blank, StartupError::DeviceIdUnreadable),
        "{blank:?}"
    );

    // A device id no register row carries: never a guess at one.
    fs::write(dir.path().join(DEVICE_ID_FILE), "device-NOT-HERE").unwrap();
    let unknown = startup_refusal(open(dir.path()));
    assert!(
        matches!(unknown, StartupError::RegisterUnknown),
        "{unknown:?}"
    );
    assert_eq!(unknown.detail(), "no register here is this install's");
    assert_eq!(unknown.to_ipc().code, "till_closed");

    // A retired register is not this install's to sell on.
    fs::write(dir.path().join(DEVICE_ID_FILE), DEVICE_ID).unwrap();
    let conn = pos_db::open(&dir.path().join(DATABASE_FILE), KEY).unwrap();
    conn.execute("UPDATE register SET is_active = 0", [])
        .unwrap();
    drop(conn);
    let retired = startup_refusal(open(dir.path()));
    assert!(
        matches!(retired, StartupError::RegisterRetired),
        "{retired:?}"
    );
}

#[test]
fn no_command_runs_without_a_signed_in_user() {
    let (_dir, register) = seeded();
    for denied in [
        refusal(authorize::<cap::SaleCreate>(
            &register,
            None,
            &any_binding(),
            &EscalationPolicy::empty(),
            at(),
        )),
        refusal(authorize::<cap::DiscountManual>(
            &register,
            None,
            &any_binding(),
            &EscalationPolicy::empty(),
            at(),
        )),
        refusal(logout(&register)),
    ] {
        assert_eq!(denied.code, "not_signed_in");
    }
    assert_eq!(audited(&register, "auth.logout"), 0);
}

#[test]
fn a_pin_sign_in_holds_the_role_grants() {
    let (_dir, register) = seeded();
    assert_eq!(
        sign_in(&register, &CASHIER),
        SessionState::SignedIn {
            code: CASHIER.code.to_owned(),
            display_name: CASHIER.name.to_owned(),
            role: "cashier",
        }
    );
    let session = register.session().unwrap();
    assert_eq!(session.user, user(&CASHIER));
    assert_eq!(session.role, Role::Cashier);
    // The shipped grid, read from the file, is the domain's column.
    assert_eq!(session.grants, GrantSet::of_role(Role::Cashier));
    assert_eq!(session.signed_in_at, at());

    let selling = authorize::<cap::SaleCreate>(
        &register,
        None,
        &any_binding(),
        &EscalationPolicy::empty(),
        at(),
    )
    .unwrap();
    assert_eq!(selling.actor(), user(&CASHIER));
    assert_eq!(selling.approver(), None);
    let admin = refusal(authorize::<cap::UserAdmin>(
        &register,
        None,
        &any_binding(),
        &EscalationPolicy::empty(),
        at(),
    ));
    assert_eq!(admin.code, "permission_denied");

    // One session at a time: switching is `auth_switch_user`'s.
    assert_eq!(
        refusal(login_pin(&register, MANAGER.code, MANAGER.pin)).code,
        "auth_already_signed_in"
    );
    assert_eq!(sessions(&register), vec![(CASHIER.id.to_vec(), None)]);
    assert_eq!(audited(&register, "auth.login"), 1);

    // A role held at this store alone signs in as that role.
    logout(&register).unwrap();
    sign_in(&register, &MANAGER);
    assert_eq!(
        register.session().unwrap().grants,
        GrantSet::of_role(Role::Manager)
    );
}

#[test]
fn a_user_with_no_role_here_or_two_cannot_sign_in() {
    let (_dir, register) = seeded();
    for (staff, detail) in [
        (&NO_ROLE, "no role at this store"),
        (&ELSEWHERE, "no role at this store"),
        (&TWO_ROLES, "more than one role at this store"),
    ] {
        let refused = refusal(login_pin(&register, staff.code, staff.pin));
        assert_eq!(refused.code, "auth_role_unresolved", "{}", staff.code);
        assert_eq!(refused.detail, Some(detail), "{}", staff.code);
        assert_eq!(register.session(), None);
        // The PIN was right, so it is not counted against them.
        assert_eq!(failed_attempts(&register, staff), 0);
    }
    assert_eq!(audited(&register, "auth.login_refused"), 3);
    assert!(sessions(&register).is_empty());

    // The same role held org-wide and here is one role.
    sign_in(&register, &SAME_ROLE_TWICE);
    assert_eq!(register.session().unwrap().role, Role::Cashier);
}

#[test]
fn a_session_is_recorded_and_a_crash_closes_it() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let register = open(dir.path()).unwrap();
    sign_in(&register, &CASHIER);
    assert_eq!(sessions(&register), vec![(CASHIER.id.to_vec(), None)]);
    let first_nonce = register.process_nonce();

    // The process dies with the session open.
    drop(register);

    let restarted = open(dir.path()).unwrap();
    assert_eq!(restarted.recovered().sessions_closed, 1);
    assert_eq!(
        sessions(&restarted),
        vec![(CASHIER.id.to_vec(), Some("crash".to_owned()))]
    );
    assert_eq!(restarted.session(), None);
    assert_ne!(restarted.process_nonce(), first_nonce);
}

#[test]
fn a_wrong_pin_signs_nobody_in_and_counts_toward_the_lockout() {
    let (_dir, register) = seeded();
    let wrong = refusal(login_pin(&register, CASHIER.code, "000000"));
    assert_eq!(wrong.code, "auth_refused");
    assert_eq!(register.session(), None);
    assert_eq!(failed_attempts(&register, &CASHIER), 1);
    assert_eq!(audited(&register, "auth.login_refused"), 1);

    // An unknown staff number is refused the same way, and counts nothing.
    let unknown = refusal(login_pin(&register, "Z-999", CASHIER.pin));
    assert_eq!(unknown.code, "auth_refused");
    assert_eq!(audited(&register, "auth.login_refused"), 1);
    assert!(sessions(&register).is_empty());

    // A correct PIN starts the count again.
    sign_in(&register, &CASHIER);
    assert_eq!(failed_attempts(&register, &CASHIER), 0);
}

#[test]
fn a_locked_out_user_cannot_sign_in_until_released() {
    let (_dir, register) = seeded();
    register.with_db(|conn| {
        let tx = conn.transaction().unwrap();
        for _ in 0..10 {
            UserRepository::new(&tx)
                .record_failure(&tx, user(&CASHIER), register.register(), at())
                .unwrap();
        }
        tx.commit().unwrap();
    });

    // Even the right PIN is not checked.
    let locked = refusal(login_pin(&register, CASHIER.code, CASHIER.pin));
    assert_eq!(locked.code, "auth_locked_out");
    assert_eq!(register.session(), None);
    assert_eq!(audited(&register, "auth.login"), 0);

    // A manager's release, which 1.6.2's command audits, lets them in.
    register.with_db(|conn| {
        let tx = conn.transaction().unwrap();
        UserRepository::new(&tx)
            .release(&tx, user(&CASHIER), register.register(), at())
            .unwrap();
        tx.commit().unwrap();
    });
    sign_in(&register, &CASHIER);
    assert_eq!(register.session().unwrap().user, user(&CASHIER));
}

#[test]
fn signing_out_ends_the_session() {
    let (_dir, register) = seeded();
    sign_in(&register, &CASHIER);
    logout(&register).unwrap();

    assert_eq!(register.session(), None);
    assert_eq!(
        sessions(&register),
        vec![(CASHIER.id.to_vec(), Some("logout".to_owned()))]
    );
    assert_eq!(audited(&register, "auth.logout"), 1);
    assert_eq!(
        refusal(authorize::<cap::SaleCreate>(
            &register,
            None,
            &any_binding(),
            &EscalationPolicy::empty(),
            at(),
        ))
        .code,
        "not_signed_in"
    );
}

#[test]
fn a_deactivated_user_is_signed_out_before_acting() {
    let (_dir, register) = seeded();
    sign_in(&register, &CASHIER);
    register.with_db(|conn| {
        conn.execute(
            "UPDATE app_user SET is_active = 0 WHERE id = ?1",
            [CASHIER.id.as_slice()],
        )
        .unwrap();
    });

    let stopped = refusal(authorize::<cap::SaleCreate>(
        &register,
        None,
        &any_binding(),
        &EscalationPolicy::empty(),
        at(),
    ));
    assert_eq!(stopped.code, "user_inactive");
    assert_eq!(register.session(), None);
    assert_eq!(
        sessions(&register),
        vec![(CASHIER.id.to_vec(), Some("logout".to_owned()))]
    );
    let cause: String = register.with_db(|conn| {
        conn.query_row(
            "SELECT json_extract(payload, '$.cause') FROM audit_log WHERE action = 'auth.logout'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    });
    assert_eq!(cause, "user_inactive");

    // And the account no longer signs in, with the same refusal as a wrong PIN.
    assert_eq!(
        refusal(login_pin(&register, CASHIER.code, CASHIER.pin)).code,
        "auth_refused"
    );
    // Even locked out: the screen learns nothing about an account it will
    // not sign in, not even that its attempts ran out.
    register.with_db(|conn| {
        let tx = conn.transaction().unwrap();
        for _ in 0..10 {
            UserRepository::new(&tx)
                .record_failure(&tx, user(&CASHIER), register.register(), at())
                .unwrap();
        }
        tx.commit().unwrap();
    });
    assert_eq!(
        refusal(login_pin(&register, CASHIER.code, CASHIER.pin)).code,
        "auth_refused"
    );
}

#[test]
fn a_stored_approval_is_resolved_by_id_and_held_to_its_binding() {
    let (_dir, register) = seeded();
    sign_in(&register, &CASHIER);
    let binding = ApprovalBinding {
        entity_id: register.next_id(),
        amount_minor: 1_250,
        content_hash: None,
    };
    let id = approval(&register, &binding);

    let approved = authorize::<cap::DiscountManual>(
        &register,
        Some(id),
        &binding,
        &discount_escalates(),
        at(),
    )
    .unwrap();
    assert_eq!(approved.actor(), user(&CASHIER));
    assert_eq!(approved.approver(), Some(user(&MANAGER)));
    assert_eq!(approved.approval(), Some(id));

    // Without it, the escalated discount is refused.
    assert_eq!(
        refusal(authorize::<cap::DiscountManual>(
            &register,
            None,
            &binding,
            &discount_escalates(),
            at(),
        ))
        .code,
        "approval_required"
    );
    // For another amount, it is refused by name.
    let more = ApprovalBinding {
        amount_minor: 1_251,
        ..binding
    };
    let mismatch = refusal(authorize::<cap::DiscountManual>(
        &register,
        Some(id),
        &more,
        &discount_escalates(),
        at(),
    ));
    assert_eq!(mismatch.code, "approval_mismatch");
    assert_eq!(mismatch.detail, Some("the approval is for another amount"));
    // An id that names no stored handle resolves to nothing.
    let unknown = ApprovalId::from_uuid(register.next_id());
    assert_eq!(
        refusal(authorize::<cap::DiscountManual>(
            &register,
            Some(unknown),
            &binding,
            &discount_escalates(),
            at(),
        ))
        .code,
        "approval_unknown"
    );
}

#[test]
fn a_consumed_approval_is_refused_by_the_context() {
    let (_dir, register) = seeded();
    sign_in(&register, &CASHIER);
    let binding = ApprovalBinding {
        entity_id: register.next_id(),
        amount_minor: 900,
        content_hash: None,
    };
    let id = approval(&register, &binding);
    spend(&register, id, &binding);

    let replayed = refusal(authorize::<cap::DiscountManual>(
        &register,
        Some(id),
        &binding,
        &discount_escalates(),
        at(),
    ));
    assert_eq!(replayed.code, "approval_used");
}

/// 1.6.4, the owner's decision of 5 November 2026: deactivating an approver
/// revokes every handle they issued that has not been spent.
#[test]
fn an_approval_is_revoked_when_its_approver_is_deactivated() {
    let (_dir, register) = seeded();
    sign_in(&register, &CASHIER);
    let binding = ApprovalBinding {
        entity_id: register.next_id(),
        amount_minor: 1_250,
        content_hash: None,
    };
    let id = approval(&register, &binding);
    let use_it = || {
        authorize::<cap::DiscountManual>(&register, Some(id), &binding, &discount_escalates(), at())
    };
    let manager = |change: &str| {
        register.with_db(|conn| {
            conn.execute(
                &format!("UPDATE app_user SET {change} WHERE id = ?1"),
                [MANAGER.id.as_slice()],
            )
            .unwrap();
        });
    };
    // While the manager's account is live, their approval authorizes.
    assert_eq!(use_it().unwrap().approver(), Some(user(&MANAGER)));

    // Deactivated after approving, and before the handle expires.
    manager("is_active = 0");
    let revoked = refusal(use_it());
    assert_eq!(revoked.code, "approval_revoked");
    assert_eq!(
        revoked.detail,
        Some("the approver's account is no longer active")
    );
    // Deleted rather than deactivated, alike.
    manager("is_active = 1, deleted_at = '2026-11-04T09:00:00.000Z'");
    assert_eq!(refusal(use_it()).code, "approval_revoked");

    // It is the approval that is refused, not the cashier: they stay signed
    // in, a command that needs no approval still runs, and nothing was spent.
    assert_eq!(
        register.session().map(|session| session.user),
        Some(user(&CASHIER))
    );
    authorize::<cap::SaleCreate>(
        &register,
        None,
        &any_binding(),
        &EscalationPolicy::empty(),
        at(),
    )
    .unwrap();
    // Only an approval that would authorize is refused. Where the policy does
    // not escalate the discount, the id is ignored, as the domain ignores it.
    let unescalated = authorize::<cap::DiscountManual>(
        &register,
        Some(id),
        &binding,
        &EscalationPolicy::empty(),
        at(),
    )
    .unwrap();
    assert_eq!(unescalated.approver(), None);
    assert!(!register.with_db(|conn| ApprovalRepository::new(conn).is_consumed(id).unwrap()));
    assert_eq!(audited(&register, "auth.logout"), 0);
}

#[test]
fn the_webview_sees_who_is_signed_in_and_nothing_more() {
    let (_dir, register) = seeded();
    let till = Till::Open(Box::new(register));
    assert_eq!(
        serde_json::to_value(state_of(&till)).unwrap(),
        json!({ "state": "signed_out" })
    );

    let Till::Open(register) = &till else {
        unreachable!()
    };
    let signed_in = serde_json::to_value(sign_in(register, &CASHIER)).unwrap();
    let expected = json!({
        "state": "signed_in",
        "code": CASHIER.code,
        "display_name": CASHIER.name,
        "role": "cashier",
    });
    assert_eq!(signed_in, expected);
    assert_eq!(serde_json::to_value(state_of(&till)).unwrap(), expected);

    // A refused sign-in carries no PIN, no hash and no count.
    logout(register).unwrap();
    let refused =
        serde_json::to_string(&refusal(login_pin(register, CASHIER.code, "000000"))).unwrap();
    // Each is named by what it is, so a failure never prints the value.
    for (what, secret) in [
        ("the PIN typed", "000000"),
        ("the account's PIN", CASHIER.pin),
        ("a PIN hash", "argon2"),
        ("the attempt count", "failed_attempts"),
    ] {
        assert!(!refused.contains(secret), "{what} reached the webview");
    }
}
