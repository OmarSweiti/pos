//! Microstep 1.1.9, deferred database half — `ClockState` across a restart.
//!
//! Every test opens through `pos_db::open`, so it runs the exact `MIGRATIONS`
//! array the application compiles with. `ref/schema.md` is not replayed on top:
//! `trusted_time_state` shipped in `0005`, and a green run against the
//! reference document would not be evidence that a register carries it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/registered_chain.rs"]
mod registered_chain;

/// The module name is load-bearing, as it was for `1.8.9`. This microstep's
/// `Done when` command is `cargo nextest run -p pos-db clock::`, and nextest
/// matches that filter against the test's own name — not against the binary id,
/// which is `pos-db::clock`. Without the module the filter selects nothing and
/// the proving command passes by running no tests at all.
mod clock {
    use std::path::Path;

    use pos_db::repo::clock::{ClockRepository, StoredClock};
    use pos_domain::{ClockAnomaly, ClockState, Timestamp};
    use rusqlite::Connection;

    use crate::registered_chain::RegisteredChain;

    const KEY: &str = "test-key";
    /// Time is an argument here as in the repository (I-8). Fixed instants, so
    /// nothing in this file depends on when it ran.
    const TRUSTED_AT: &str = "2026-08-25T10:00:00.000Z";
    const DEVICE_AT: &str = "2026-08-25T10:00:01.500Z";
    const HIGH_WATER: &str = "2026-08-25T11:30:00.000Z";
    const ANOMALY_AT: &str = "2026-08-25T11:31:00.000Z";
    const UPDATED_AT: &str = "2026-08-25T11:31:00.500Z";

    /// The register the clock row hangs off. `trusted_time_state.register_id`
    /// carries `REFERENCES register(id)`, so a register that does not exist has
    /// no clock — which is the right shape and not an obstacle to route around.
    const REGISTER: [u8; 16] = [0x0c; 16];

    fn at(iso: &str) -> Timestamp {
        Timestamp::parse_iso8601(iso).unwrap()
    }

    fn open(path: &Path) -> Connection {
        pos_db::open(path, KEY).expect("the registered migration chain must open")
    }

    /// A register with its reference world, ready to hold a clock row.
    fn register(dir: &tempfile::TempDir, name: &str) -> Connection {
        let conn = open(&dir.path().join(name));
        RegisteredChain::seed(&conn).add_register(&conn, REGISTER.as_slice(), "REG01");
        conn
    }

    fn save(conn: &Connection, stored: &StoredClock) {
        let tx = conn.unchecked_transaction().unwrap();
        ClockRepository::new(conn)
            .save(&tx, &REGISTER, stored)
            .expect("the row must round-trip through the shipped schema");
        tx.commit().unwrap();
    }

    fn load(conn: &Connection) -> Option<StoredClock> {
        ClockRepository::new(conn).load(&REGISTER).unwrap()
    }

    /// A register that has been trusted, has a monotonic anchor, and has seen
    /// its wall clock move backwards since.
    fn anchored() -> StoredClock {
        StoredClock {
            state: ClockState {
                last_trusted_at: Some(at(TRUSTED_AT)),
                device_at_trust: Some(at(DEVICE_AT)),
                monotonic_since_trust_ms: Some(90_000),
                high_water: at(HIGH_WATER),
                anomaly: Some(ClockAnomaly::JumpedBack {
                    by_ms: 4_000,
                    at: at(ANOMALY_AT),
                }),
            },
            boot_token: Some(vec![0xB0, 0x07, 0x7A, 0x11]),
            updated_at: UPDATED_AT.to_owned(),
        }
    }

    #[test]
    fn clock_state_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("restart.db");
        let conn = open(&path);
        RegisteredChain::seed(&conn).add_register(&conn, REGISTER.as_slice(), "REG01");
        let written = anchored();
        save(&conn, &written);
        drop(conn);

        // The restart is the point. A second read on the same connection would
        // prove only that the row is in the page cache; a reboot is exactly
        // when a wrong clock arrives, which is why this value is persisted at
        // all rather than held for the life of the process.
        let reopened = open(&path);
        let restored = ClockRepository::new(&reopened)
            .load(&REGISTER)
            .unwrap()
            .expect("the register wrote a clock state before it restarted");

        assert_eq!(restored, written, "the same ClockState, field for field");
        assert_eq!(
            restored.state.high_water,
            at(HIGH_WATER),
            "E.6: the high-water mark is the one guarantee a restart must not lower"
        );
    }

    #[test]
    fn a_register_with_no_row_reads_as_absent_not_as_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let conn = register(&dir, "absent.db");

        assert_eq!(
            load(&conn),
            None,
            "a register provisioned offline this morning has no anchor, and that \
             is `Untrusted` rather than a failure to read"
        );
    }

    #[test]
    fn a_never_trusted_register_round_trips_its_three_nones() {
        let dir = tempfile::tempdir().unwrap();
        let conn = register(&dir, "never-trusted.db");

        // A round-trip that only ever stores `Some` proves half the mapping.
        // Three independently nullable columns is where an accidental
        // `unwrap_or_default()` would hide, and a zeroed monotonic anchor is
        // materially different from an absent one: it claims boot continuity
        // this register has never established.
        let written = StoredClock {
            state: ClockState {
                last_trusted_at: None,
                device_at_trust: None,
                monotonic_since_trust_ms: None,
                high_water: at(HIGH_WATER),
                anomaly: None,
            },
            boot_token: None,
            updated_at: UPDATED_AT.to_owned(),
        };
        save(&conn, &written);

        assert_eq!(load(&conn), Some(written));
    }

    #[test]
    fn every_anomaly_variant_round_trips_through_its_three_columns() {
        let dir = tempfile::tempdir().unwrap();
        let conn = register(&dir, "anomalies.db");

        for anomaly in [
            ClockAnomaly::JumpedBack {
                by_ms: 4_000,
                at: at(ANOMALY_AT),
            },
            ClockAnomaly::JumpedForward {
                by_ms: 31_536_000_000,
                at: at(ANOMALY_AT),
            },
            // The variant with no magnitude. The schema's second CHECK refuses
            // a `by_ms` beside it, so a mapping that wrote 0 would be refused
            // rather than quietly storing a lie.
            ClockAnomaly::MonotonicReset { at: at(ANOMALY_AT) },
        ] {
            let mut written = anchored();
            written.state.anomaly = Some(anomaly);
            save(&conn, &written);
            assert_eq!(
                load(&conn),
                Some(written),
                "{anomaly:?} did not survive the flattening"
            );
        }
    }

    #[test]
    fn saving_twice_replaces_the_row_rather_than_appending() {
        let dir = tempfile::tempdir().unwrap();
        let conn = register(&dir, "upsert.db");

        save(&conn, &anchored());
        let mut later = anchored();
        later.state.high_water = at("2026-08-25T12:00:00.000Z");
        later.state.anomaly = None;
        later.boot_token = Some(vec![0xFE, 0xED]);
        save(&conn, &later);

        let rows: i64 = conn
            .query_row("SELECT count(*) FROM trusted_time_state", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            rows, 1,
            "this is operational state rebuilt from the world, not a fact anyone \
             can later be asked to prove — one row per register, replaced in place"
        );
        assert_eq!(load(&conn), Some(later), "and it is the later reading");
    }

    #[test]
    fn a_cleared_anomaly_clears_all_three_columns() {
        let dir = tempfile::tempdir().unwrap();
        let conn = register(&dir, "cleared.db");

        save(&conn, &anchored());
        let mut resolved = anchored();
        resolved.state.anomaly = None;
        save(&conn, &resolved);

        // The upsert must null the columns rather than leave the previous
        // anomaly behind. A stale `anomaly_by_ms` with no `anomaly_kind` would
        // fail the schema's coupled CHECK, so this is the guard proving the
        // write clears the whole group and not just the discriminant.
        let (kind, by_ms, when): (Option<String>, Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT anomaly_kind, anomaly_by_ms, anomaly_at FROM trusted_time_state",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((kind, by_ms, when), (None, None, None));
        assert_eq!(load(&conn).unwrap().state.anomaly, None);
    }

    #[test]
    fn a_row_written_by_a_newer_build_is_refused_not_downgraded() {
        let dir = tempfile::tempdir().unwrap();
        let conn = register(&dir, "newer-build.db");
        save(&conn, &anchored());

        // A future migration may widen the CHECK with a fourth anomaly kind.
        // Reading such a row as `anomaly: None` would downgrade a register from
        // "something is wrong with the clock" to "nothing is wrong with it",
        // which is the failure mode worth a hard error. The CHECK is bypassed
        // deliberately here — the point is what `load` does with a value it
        // does not know, not whether today's schema can hold one.
        conn.execute_batch(
            "PRAGMA writable_schema = ON;
             UPDATE sqlite_schema
                SET sql = replace(sql, '''monotonic_reset''', '''monotonic_reset'',''leaped''')
              WHERE type = 'table' AND name = 'trusted_time_state';
             PRAGMA writable_schema = OFF;",
        )
        .unwrap();
        drop(conn);

        let reopened = open(&dir.path().join("newer-build.db"));
        reopened
            .execute(
                "UPDATE trusted_time_state SET anomaly_kind = 'leaped', anomaly_by_ms = NULL",
                [],
            )
            .unwrap();

        let error = ClockRepository::new(&reopened)
            .load(&REGISTER)
            .expect_err("an unreadable anomaly must not read as the absence of one");
        assert!(
            matches!(error, pos_db::DbError::ClockStateInvalid { ref reason }
                     if reason.contains("leaped")),
            "expected a named ClockStateInvalid, found {error:?}"
        );
    }
}
