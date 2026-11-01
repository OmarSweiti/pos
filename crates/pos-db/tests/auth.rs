//! Microstep 1.6.2 — PINs and wrong PINs, through the schema the register runs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::fixtures::registered_chain;

/// The module name is load-bearing: this microstep's `Done when` filter is
/// `auth::`, which nextest matches against the test's own name.
mod auth {
    use std::path::Path;

    use pos_db::auth::{ATTEMPT_POLICY, Admission, admission, hash_pin, needs_rehash, verify_pin};
    use pos_db::repo::user::UserRepository;
    use pos_domain::{RegisterId, Timestamp, UserId};
    use rusqlite::Connection;
    use uuid::Uuid;

    use crate::registered_chain::RegisteredChain;

    const KEY: &str = "test-key";
    const REGISTER: [u8; 16] = [0x0c; 16];
    const OTHER_REGISTER: [u8; 16] = [0x0d; 16];

    fn at(ms: i64) -> Timestamp {
        Timestamp::from_epoch_milliseconds(1_790_000_000_000 + ms).unwrap()
    }

    fn open(path: &Path) -> Connection {
        pos_db::open(path, KEY).unwrap()
    }

    /// A register database with the fixture's cashier and two registers.
    fn seeded(path: &Path) -> (Connection, UserId) {
        let conn = open(path);
        let chain = RegisteredChain::seed(&conn);
        chain.add_register(&conn, REGISTER.as_slice(), "REG01");
        chain.add_register(&conn, OTHER_REGISTER.as_slice(), "REG02");
        let cashier = UserId::from_uuid(Uuid::from_slice(&chain.cashier).unwrap());
        (conn, cashier)
    }

    fn register(bytes: [u8; 16]) -> RegisterId {
        RegisterId::from_uuid(Uuid::from_bytes(bytes))
    }

    fn fail(conn: &Connection, user: UserId, on: [u8; 16], ms: i64) {
        let tx = conn.unchecked_transaction().unwrap();
        UserRepository::new(conn)
            .record_failure(&tx, user, register(on), at(ms))
            .unwrap();
        tx.commit().unwrap();
    }

    /// Wrong PINs are counted on disk, so a restart forgives none of them:
    /// the wait survives it, the lockout survives it, and only a correct PIN
    /// or a manager's release clears them. Each register keeps its own count.
    #[test]
    fn failed_attempt_state_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("register.db");
        let (conn, cashier) = seeded(&path);
        for n in 0..4 {
            fail(&conn, cashier, REGISTER, n * 1_000);
        }
        drop(conn);

        let conn = open(&path);
        let users = UserRepository::new(&conn);
        let after_four = users.attempts(cashier, register(REGISTER)).unwrap();
        assert_eq!(after_four.failed_attempts, 4);
        assert_eq!(after_four.delay_until, Some(at(3_000 + 30_000)));
        assert_eq!(after_four.last_attempt_at, Some(at(3_000)));
        assert_eq!(
            admission(&after_four, at(3_000 + 29_999), &ATTEMPT_POLICY),
            Admission::Delayed { until: at(33_000) }
        );
        assert_eq!(
            users
                .attempts(cashier, register(OTHER_REGISTER))
                .unwrap()
                .failed_attempts,
            0
        );

        for n in 4..10 {
            fail(&conn, cashier, REGISTER, 1_000_000 * n);
        }
        drop(conn);

        let conn = open(&path);
        let users = UserRepository::new(&conn);
        let locked = users.attempts(cashier, register(REGISTER)).unwrap();
        assert_eq!(locked.failed_attempts, 10);
        assert_eq!(
            admission(&locked, at(1_000_000_000_000), &ATTEMPT_POLICY),
            Admission::LockedOut
        );

        let tx = conn.unchecked_transaction().unwrap();
        users
            .release(&tx, cashier, register(REGISTER), at(10_000_000_001))
            .unwrap();
        tx.commit().unwrap();
        drop(conn);

        let conn = open(&path);
        let released = UserRepository::new(&conn)
            .attempts(cashier, register(REGISTER))
            .unwrap();
        assert_eq!(released.failed_attempts, 0);
        assert_eq!(released.delay_until, None);
        assert_eq!(
            admission(&released, at(10_000_000_002), &ATTEMPT_POLICY),
            Admission::Allowed
        );
    }

    /// A correct PIN starts the count again; a rolled-back failure was never
    /// counted.
    #[test]
    fn a_correct_pin_clears_the_count_and_a_rolled_back_failure_never_counts() {
        let dir = tempfile::tempdir().unwrap();
        let (conn, cashier) = seeded(&dir.path().join("register.db"));
        let users = UserRepository::new(&conn);
        fail(&conn, cashier, REGISTER, 0);
        fail(&conn, cashier, REGISTER, 1);
        let tx = conn.unchecked_transaction().unwrap();
        users
            .record_failure(&tx, cashier, register(REGISTER), at(2))
            .unwrap();
        tx.rollback().unwrap();
        assert_eq!(
            users
                .attempts(cashier, register(REGISTER))
                .unwrap()
                .failed_attempts,
            2
        );
        let tx = conn.unchecked_transaction().unwrap();
        users
            .record_success(&tx, cashier, register(REGISTER), at(3))
            .unwrap();
        tx.commit().unwrap();
        let cleared = users.attempts(cashier, register(REGISTER)).unwrap();
        assert_eq!(
            (cleared.failed_attempts, cleared.last_attempt_at),
            (0, Some(at(3)))
        );
    }

    /// A new hash replaces the old one, and the old PIN stops working; an
    /// inactive account has no PIN to check.
    #[test]
    fn a_pin_hash_is_replaced_and_the_old_pin_stops_working() {
        let dir = tempfile::tempdir().unwrap();
        let (conn, cashier) = seeded(&dir.path().join("register.db"));
        let users = UserRepository::new(&conn);
        let first = hash_pin("1234").unwrap();
        let second = hash_pin("5678").unwrap();
        for hash in [&first, &second] {
            let tx = conn.unchecked_transaction().unwrap();
            users.set_pin_hash(&tx, cashier, hash, at(0)).unwrap();
            tx.commit().unwrap();
        }
        let stored = users.pin_hash(cashier).unwrap().unwrap();
        assert_eq!(stored, second);
        assert_eq!(verify_pin("5678", &stored), Ok(true));
        assert_eq!(verify_pin("1234", &stored), Ok(false));
        assert_eq!(needs_rehash(&stored), Ok(false));
        let nobody = UserId::from_uuid(Uuid::from_bytes([0xee; 16]));
        let tx = conn.unchecked_transaction().unwrap();
        assert!(users.set_pin_hash(&tx, nobody, &first, at(0)).is_err());
        drop(tx);
        assert_eq!(users.pin_hash(nobody).unwrap(), None);
        conn.execute("UPDATE app_user SET is_active = 0", [])
            .unwrap();
        assert_eq!(users.pin_hash(cashier).unwrap(), None);
    }
}
