//! A store's tax computation policy (microstep 1.3.4's database half,
//! `ref/schema.md` §0003, `ref/tax-jordan.md` §4).
//!
//! The tie rule changes tax facts, not presentation: a 13-fil line at 4%
//! inclusive has an exact net of 12.5 fils, so half-away-from-zero records
//! 13 + 0 and half-even 12 + 1. It therefore belongs to a versioned
//! jurisdiction policy pinned per store, never to a merchant setting, and
//! `RoundingRule` has no `Default`, so an unapproved rule cannot reach a sale
//! through `unwrap_or_default()`. This module is where the rule comes from:
//! [`TaxPolicyRepository::approved_for_store`] reads the policy a store is
//! pinned to, and the [`TaxComputationPolicy`] it returns carries the
//! `RoundingRule` that `pos_domain::compute_line_tax` is handed.
//!
//! ## Refused, by name
//!
//! * **A store this register does not hold** is
//!   [`DbError::TaxPolicyStoreUnknown`]. Today it is the likeliest refusal:
//!   `0003` refuses to write a store without an approved policy, and none is
//!   approved until `ref/schema.md` §0003's `⚠️ OPEN` rounding item is
//!   answered, so a register being set up finds no store at all.
//! * **A store with no policy**, or one naming a policy this register does
//!   not hold, is [`DbError::TaxPolicyMissing`].
//! * **A policy that is not approved** is [`DbError::TaxPolicyUnapproved`].
//!
//! `0003`'s `store_tax_policy_complete_*` triggers refuse the last two when a
//! store row is written, its foreign key refuses a dangling policy, and
//! `tax_computation_policy_no_update` keeps an approved policy approved. The
//! read checks again because the read is what a sale's arithmetic depends on:
//! a later migration that rebuilt `store` and forgot a trigger would otherwise
//! reach the till as a rule nobody approved.
//!
//! ## The schema's spelling, mapped explicitly
//!
//! The domain's serde derive is the IPC spelling (`"HalfAwayFromZero"`,
//! `"Nearest"`); `0003` stores `'half_away_from_zero'` and `'nearest'`. As
//! `ref/domain-api.md` §1.2 requires, each enum gets one visible mapping here
//! instead of a `rename_all` that would make a derive silently responsible for
//! a `CHECK` constraint. A spelling this build does not know is
//! [`DbError::StoredTaxPolicyInvalid`], never read as a fallback.
//!
//! ## What is read, and what is not
//!
//! The coin step and its direction are read from this row because `0003`
//! stores them here. That table's own comment, `ref/tax-jordan.md` §5 and
//! merchant decisions 2.1 and 2.2 call cash rounding a separate setting on
//! `store`, but `store` has no such column, so this row is the only place a
//! register holds them. `source_ref` and `content_hash` are not read: nothing
//! on the register checks a policy's provenance yet. Neither is
//! `cash_round_tax_treatment`: its vocabulary is `ref/tax-jordan.md` §5's
//! `⚠️ OPEN` item, owned by 2.7.0, and until then a cash-rounding adjustment
//! moves no line and no tax row (merchant decision 6.12).

use pos_domain::{RoundingDirection, RoundingRule, StoreId, Timestamp};
use rusqlite::{Connection, OptionalExtension};

use crate::DbError;

/// Ids are a UUIDv7's sixteen bytes, as everywhere in `pos-db`.
pub type Id = [u8; 16];

/// The approved policy a store's lines are computed under.
///
/// Only [`TaxPolicyRepository::approved_for_store`] makes one, and only from a
/// row with `approved_at` set, so holding one is the evidence that the rule it
/// carries was approved. That is why its fields are private.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxComputationPolicy {
    id: Id,
    jurisdiction: String,
    policy_version: String,
    rounding_rule: RoundingRule,
    cash_round_step_minor: i64,
    cash_round_direction: RoundingDirection,
    approved_at: Timestamp,
}

impl TaxComputationPolicy {
    /// The policy's id. A completed sale records it as
    /// `sale.tax_computation_policy_id`, and `0005` refuses one that is not its
    /// store's.
    #[must_use]
    pub const fn id(&self) -> &Id {
        &self.id
    }

    /// The jurisdiction the policy is for, as stored (`"JO"`).
    #[must_use]
    pub fn jurisdiction(&self) -> &str {
        &self.jurisdiction
    }

    /// The version of that jurisdiction's policy. A change of rule is a new
    /// version, never an edit: `0003` refuses every `UPDATE` of this table.
    #[must_use]
    pub fn policy_version(&self) -> &str {
        &self.policy_version
    }

    /// The tie rule `pos_domain::compute_line_tax` is handed for this store's
    /// lines.
    #[must_use]
    pub const fn rounding_rule(&self) -> RoundingRule {
        self.rounding_rule
    }

    /// The coin step a final cash tender is rounded to, in minor units
    /// (`pos_domain::final_tender_rounding`'s `step_minor`). Always positive.
    #[must_use]
    pub const fn cash_round_step_minor(&self) -> i64 {
        self.cash_round_step_minor
    }

    /// Which way a final cash tender moves to reach that step.
    #[must_use]
    pub const fn cash_round_direction(&self) -> RoundingDirection {
        self.cash_round_direction
    }

    /// When the policy was approved.
    #[must_use]
    pub const fn approved_at(&self) -> Timestamp {
        self.approved_at
    }
}

/// Reads the computation policy a store is pinned to.
pub struct TaxPolicyRepository<'c> {
    conn: &'c Connection,
}

impl<'c> TaxPolicyRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// The approved policy `store` is pinned to.
    ///
    /// A finalize reads it through its own transaction (a `Transaction`
    /// dereferences to its `Connection`), so the rule its lines are computed
    /// under and the policy id the sale records come from one snapshot.
    ///
    /// # Errors
    ///
    /// - [`DbError::TaxPolicyStoreUnknown`]: this register holds no such store.
    /// - [`DbError::TaxPolicyMissing`]: the store names no policy, or one this
    ///   register does not hold.
    /// - [`DbError::TaxPolicyUnapproved`]: the store's policy is not approved.
    /// - [`DbError::IdWidthInvalid`] and [`DbError::StoredTaxPolicyInvalid`]: a
    ///   row this build cannot read, refused rather than read as a fallback.
    pub fn approved_for_store(&self, store: StoreId) -> Result<TaxComputationPolicy, DbError> {
        let pinned: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT tax_computation_policy_id FROM store WHERE id = ?1",
                [store.as_uuid().as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(DbError::TaxPolicyStoreUnknown(store))?;
        let pinned = pinned.ok_or(DbError::TaxPolicyMissing(store))?;
        let found = pinned.len();
        let id: Id = pinned.try_into().map_err(|_| DbError::IdWidthInvalid {
            table: "store",
            column: "tax_computation_policy_id",
            found,
        })?;

        let row = self
            .conn
            .query_row(
                "SELECT jurisdiction, policy_version, rounding_rule, cash_round_step_minor,
                        cash_round_direction, approved_at
                   FROM tax_computation_policy
                  WHERE id = ?1",
                [id.as_slice()],
                |row| {
                    Ok(StoredPolicy {
                        jurisdiction: row.get(0)?,
                        policy_version: row.get(1)?,
                        rounding_rule: row.get(2)?,
                        cash_round_step_minor: row.get(3)?,
                        cash_round_direction: row.get(4)?,
                        approved_at: row.get(5)?,
                    })
                },
            )
            .optional()?
            .ok_or(DbError::TaxPolicyMissing(store))?;
        row.into_policy(store, id)
    }
}

/// A `tax_computation_policy` row as stored, before anything is believed.
struct StoredPolicy {
    jurisdiction: String,
    policy_version: String,
    rounding_rule: String,
    cash_round_step_minor: i64,
    cash_round_direction: String,
    approved_at: Option<String>,
}

impl StoredPolicy {
    fn into_policy(self, store: StoreId, id: Id) -> Result<TaxComputationPolicy, DbError> {
        let Some(approved_at) = self.approved_at else {
            return Err(DbError::TaxPolicyUnapproved {
                store,
                jurisdiction: self.jurisdiction,
                policy_version: self.policy_version,
            });
        };
        let approved_at = Timestamp::parse_iso8601(&approved_at).map_err(|_| {
            invalid(format!(
                "approved_at {approved_at:?} is not an ISO-8601 instant"
            ))
        })?;
        // `0003`'s CHECK refuses this too; a step that is not positive cannot
        // round a tender, and is refused here rather than at the first sale.
        if self.cash_round_step_minor <= 0 {
            return Err(invalid(format!(
                "cash_round_step_minor {} is not a coin step",
                self.cash_round_step_minor
            )));
        }
        Ok(TaxComputationPolicy {
            id,
            rounding_rule: rounding_rule_from_token(&self.rounding_rule)?,
            cash_round_direction: cash_round_direction_from_token(&self.cash_round_direction)?,
            jurisdiction: self.jurisdiction,
            policy_version: self.policy_version,
            cash_round_step_minor: self.cash_round_step_minor,
            approved_at,
        })
    }
}

/// `0003`'s spelling of each tie rule, from its
/// `CHECK (rounding_rule IN ('half_away_from_zero','half_even','floor','ceil'))`.
/// The match is exhaustive, so a fifth rule does not compile until it is given
/// a spelling here.
const fn rounding_rule_token(rule: RoundingRule) -> &'static str {
    match rule {
        RoundingRule::HalfAwayFromZero => "half_away_from_zero",
        RoundingRule::HalfEven => "half_even",
        RoundingRule::Floor => "floor",
        RoundingRule::Ceil => "ceil",
    }
}

/// The reverse, through the same table, so the two directions cannot disagree.
fn rounding_rule_from_token(token: &str) -> Result<RoundingRule, DbError> {
    [
        RoundingRule::HalfAwayFromZero,
        RoundingRule::HalfEven,
        RoundingRule::Floor,
        RoundingRule::Ceil,
    ]
    .into_iter()
    .find(|rule| rounding_rule_token(*rule) == token)
    .ok_or_else(|| {
        invalid(format!(
            "rounding_rule {token:?} is not a rule this build knows"
        ))
    })
}

/// `0003`'s spelling of each coin-step direction, from its
/// `CHECK (cash_round_direction IN ('nearest','up','down'))`.
const fn cash_round_direction_token(direction: RoundingDirection) -> &'static str {
    match direction {
        RoundingDirection::Nearest => "nearest",
        RoundingDirection::Up => "up",
        RoundingDirection::Down => "down",
    }
}

/// The reverse, through the same table.
fn cash_round_direction_from_token(token: &str) -> Result<RoundingDirection, DbError> {
    [
        RoundingDirection::Nearest,
        RoundingDirection::Up,
        RoundingDirection::Down,
    ]
    .into_iter()
    .find(|direction| cash_round_direction_token(*direction) == token)
    .ok_or_else(|| {
        invalid(format!(
            "cash_round_direction {token:?} is not a direction this build knows"
        ))
    })
}

fn invalid(reason: String) -> DbError {
    DbError::StoredTaxPolicyInvalid { reason }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use pos_domain::{
        Currency, Money, Percent, PriceMode, Qty, StoreTaxProfile, SupplyDestination,
        SupplyTaxContext, TaxBase, TaxBasis, TaxComponent, TaxTreatment, compute_line_tax,
    };
    use rusqlite::params;
    use uuid::Uuid;

    use super::*;
    use crate::fixtures::registered_chain::{AT, RegisteredChain};

    const KEY: &str = "tax-policy-test-key";

    /// The schema's spellings, written out here rather than read from the
    /// code under test, so a mapping that drifted would disagree with them.
    const RULE_SPELLINGS: [(RoundingRule, &str); 4] = [
        (RoundingRule::HalfAwayFromZero, "half_away_from_zero"),
        (RoundingRule::HalfEven, "half_even"),
        (RoundingRule::Floor, "floor"),
        (RoundingRule::Ceil, "ceil"),
    ];
    const DIRECTION_SPELLINGS: [(RoundingDirection, &str); 3] = [
        (RoundingDirection::Nearest, "nearest"),
        (RoundingDirection::Up, "up"),
        (RoundingDirection::Down, "down"),
    ];

    struct Till {
        _dir: tempfile::TempDir,
        conn: Connection,
        chain: RegisteredChain,
    }

    /// A registered store pinned to the fixture's approved policy: half-even,
    /// a 5-fil coin step, nearest.
    fn till() -> Till {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open(&dir.path().join("register.db"), KEY).unwrap();
        let chain = RegisteredChain::seed(&conn);
        Till {
            _dir: dir,
            conn,
            chain,
        }
    }

    fn store(t: &Till) -> StoreId {
        StoreId::from_uuid(Uuid::from_slice(&t.chain.store).unwrap())
    }

    fn read(t: &Till) -> Result<TaxComputationPolicy, DbError> {
        TaxPolicyRepository::new(&t.conn).approved_for_store(store(t))
    }

    fn policy_id(n: u128) -> Vec<u8> {
        Uuid::from_u128(0x7a00 + n).as_bytes().to_vec()
    }

    struct NewPolicy<'a> {
        id: Vec<u8>,
        version: &'a str,
        rule: &'a str,
        step: i64,
        direction: &'a str,
        approved_at: Option<&'a str>,
    }

    impl<'a> NewPolicy<'a> {
        fn approved(n: u128, version: &'a str) -> Self {
            Self {
                id: policy_id(n),
                version,
                rule: "half_away_from_zero",
                step: 10,
                direction: "nearest",
                approved_at: Some(AT),
            }
        }
    }

    fn insert_policy(conn: &Connection, p: &NewPolicy<'_>) -> rusqlite::Result<usize> {
        conn.execute(
            "INSERT INTO tax_computation_policy
               (id, jurisdiction, policy_version, rounding_rule, cash_round_step_minor,
                cash_round_direction, cash_round_tax_treatment, source_ref, content_hash,
                approved_at)
             VALUES (?1, 'JO', ?2, ?3, ?4, ?5, 'none', 'synthetic', ?6, ?7)",
            params![
                p.id,
                p.version,
                p.rule,
                p.step,
                p.direction,
                vec![0x0bu8; 32],
                p.approved_at
            ],
        )
    }

    fn pin(t: &Till, policy: Option<&[u8]>) -> rusqlite::Result<usize> {
        t.conn.execute(
            "UPDATE store SET tax_computation_policy_id = ?1 WHERE id = ?2",
            params![policy, &t.chain.store],
        )
    }

    /// A 13-fil line at 4%, tax inclusive: the exact net is 12.5 fils, the
    /// tie `ref/domain-api.md` §1.2 uses to part the rules.
    fn tie_line(rule: RoundingRule) -> (i64, i64) {
        let component = TaxComponent {
            code: "SYNTH_A".to_owned(),
            treatment: TaxTreatment::Standard,
            basis: TaxBasis::Percentage {
                rate: Percent::from_ppm(40_000),
            },
            sequence: 0,
            base: TaxBase::LineNet,
            is_inclusive_capable: true,
        };
        let supply = SupplyTaxContext {
            destination: SupplyDestination::Domestic,
            reason: None,
            evidence_ref: None,
        };
        let line = compute_line_tax(
            Money::from_minor(13, Currency::JOD),
            Qty::ONE,
            PriceMode::Inclusive,
            &[component],
            StoreTaxProfile::Standard,
            &supply,
            rule,
        )
        .unwrap();
        (line.net.minor(), line.tax_total.minor())
    }

    /// The approved policy is read whole, and its rule is the one the engine
    /// computes with. The fixture's policy is half-even because that is not
    /// the provisional default: a read that ignored the row and fell back to
    /// half-away-from-zero would record the tie as 13 + 0, not 12 + 1.
    #[test]
    fn an_approved_policy_is_read_into_the_rule_the_engine_is_handed() {
        let t = till();
        let policy = read(&t).expect("the fixture's store is pinned to an approved policy");

        assert_eq!(policy.id().as_slice(), t.chain.tax_policy.as_slice());
        assert_eq!(policy.jurisdiction(), "JO");
        assert_eq!(policy.policy_version(), "fixture-v1");
        assert_eq!(policy.rounding_rule(), RoundingRule::HalfEven);
        assert_eq!(policy.cash_round_step_minor(), 5);
        assert_eq!(policy.cash_round_direction(), RoundingDirection::Nearest);
        assert_eq!(policy.approved_at(), Timestamp::parse_iso8601(AT).unwrap());

        assert_eq!(
            tie_line(policy.rounding_rule()),
            (12, 1),
            "half-even's tax fact"
        );
        assert_eq!(
            tie_line(RoundingRule::HalfAwayFromZero),
            (13, 0),
            "the default the read must not have fallen back to"
        );
    }

    /// Every rule and every direction reads back from the spelling `0003`'s
    /// CHECK accepts, through a real row pinned to the store.
    #[test]
    fn each_rule_and_direction_reads_back_from_the_schemas_spelling() {
        let t = till();
        for (n, (rule, spelling)) in (1..).zip(RULE_SPELLINGS) {
            assert_eq!(rounding_rule_token(rule), spelling);
            let version = format!("rule-{spelling}");
            let mut policy = NewPolicy::approved(n, &version);
            policy.rule = spelling;
            insert_policy(&t.conn, &policy).expect("0003's CHECK accepts the schema spelling");
            pin(&t, Some(&policy.id)).unwrap();
            assert_eq!(read(&t).unwrap().rounding_rule(), rule, "{spelling}");
        }
        for (n, (direction, spelling)) in (11..).zip(DIRECTION_SPELLINGS) {
            assert_eq!(cash_round_direction_token(direction), spelling);
            let version = format!("direction-{spelling}");
            let mut policy = NewPolicy::approved(n, &version);
            policy.direction = spelling;
            insert_policy(&t.conn, &policy).expect("0003's CHECK accepts the schema spelling");
            pin(&t, Some(&policy.id)).unwrap();
            assert_eq!(
                read(&t).unwrap().cash_round_direction(),
                direction,
                "{spelling}"
            );
        }
    }

    /// The serde derive is the IPC spelling. The schema refuses it, and so does
    /// the mapping: storing an enum through its derive is the coupling
    /// `ref/domain-api.md` §1.2 forbids, and this is what would catch it.
    #[test]
    fn the_wire_spelling_is_not_the_stored_spelling() {
        let t = till();
        for (n, (rule, spelling)) in (21..).zip(RULE_SPELLINGS) {
            let wire = serde_json::to_value(rule).unwrap();
            let wire = wire.as_str().unwrap();
            assert_ne!(wire, spelling);
            let version = format!("wire-{wire}");
            let mut policy = NewPolicy::approved(n, &version);
            policy.rule = wire;
            assert!(
                insert_policy(&t.conn, &policy).is_err(),
                "0003's CHECK refuses {wire:?}"
            );
            assert!(matches!(
                rounding_rule_from_token(wire),
                Err(DbError::StoredTaxPolicyInvalid { .. })
            ));
        }
        for (n, (direction, spelling)) in (31..).zip(DIRECTION_SPELLINGS) {
            let wire = serde_json::to_value(direction).unwrap();
            let wire = wire.as_str().unwrap();
            assert_ne!(wire, spelling);
            let version = format!("wire-{wire}");
            let mut policy = NewPolicy::approved(n, &version);
            policy.direction = wire;
            assert!(
                insert_policy(&t.conn, &policy).is_err(),
                "0003's CHECK refuses {wire:?}"
            );
            assert!(matches!(
                cash_round_direction_from_token(wire),
                Err(DbError::StoredTaxPolicyInvalid { .. })
            ));
        }
    }

    /// A store this register does not hold has no policy to compute under.
    #[test]
    fn a_store_this_register_does_not_hold_is_refused_by_name() {
        let t = till();
        let elsewhere = StoreId::from_uuid(Uuid::from_u128(0x5707e));
        let refused = TaxPolicyRepository::new(&t.conn)
            .approved_for_store(elsewhere)
            .unwrap_err();
        assert!(
            matches!(refused, DbError::TaxPolicyStoreUnknown(s) if s == elsewhere),
            "{refused:?}"
        );
    }

    /// `0003` refuses to unpin a store or pin it to a policy that does not
    /// exist. Its trigger and foreign key are removed on this scratch
    /// database to prove the read refuses without them.
    #[test]
    fn a_store_without_a_policy_is_refused_by_name() {
        let t = till();
        assert!(
            pin(&t, None).is_err(),
            "0003's trigger refuses an unpinned store"
        );

        t.conn
            .execute_batch("DROP TRIGGER store_tax_policy_complete_update;")
            .unwrap();
        pin(&t, None).unwrap();
        let refused = read(&t).unwrap_err();
        assert!(
            matches!(refused, DbError::TaxPolicyMissing(s) if s == store(&t)),
            "{refused:?}"
        );
        assert!(
            refused.to_string().contains("no sale finished"),
            "{refused}"
        );

        t.conn.pragma_update(None, "foreign_keys", false).unwrap();
        pin(&t, Some(&policy_id(99))).unwrap();
        let refused = read(&t).unwrap_err();
        assert!(
            matches!(refused, DbError::TaxPolicyMissing(s) if s == store(&t)),
            "a policy this register does not hold: {refused:?}"
        );
    }

    /// `0003` refuses to pin a store to a policy nobody approved. Its trigger
    /// is removed on this scratch database to prove the read refuses without
    /// it.
    #[test]
    fn an_unapproved_policy_is_refused_by_name() {
        let t = till();
        let mut draft = NewPolicy::approved(41, "draft-v2");
        draft.approved_at = None;
        insert_policy(&t.conn, &draft).unwrap();
        assert!(
            pin(&t, Some(&draft.id)).is_err(),
            "0003's trigger refuses an unapproved policy"
        );

        t.conn
            .execute_batch("DROP TRIGGER store_tax_policy_complete_update;")
            .unwrap();
        pin(&t, Some(&draft.id)).unwrap();
        let refused = read(&t).unwrap_err();
        match &refused {
            DbError::TaxPolicyUnapproved {
                store: s,
                jurisdiction,
                policy_version,
            } => {
                assert_eq!(*s, store(&t));
                assert_eq!(jurisdiction, "JO");
                assert_eq!(policy_version, "draft-v2");
            }
            other => panic!("expected TaxPolicyUnapproved, found {other:?}"),
        }
        assert!(refused.to_string().contains("not approved"), "{refused}");
    }

    /// A row this build cannot read is refused, never read as a fallback.
    /// The CHECKs that refuse the first three are switched off on this scratch
    /// database; the last two have no CHECK to switch off.
    #[test]
    fn a_stored_policy_this_build_cannot_read_is_refused() {
        let t = till();
        t.conn
            .pragma_update(None, "ignore_check_constraints", true)
            .unwrap();
        type Spoil = fn(&mut NewPolicy<'_>);
        let cases: [(&str, Spoil, &str); 4] = [
            ("bad-rule", |p| p.rule = "half_up", "\"half_up\""),
            (
                "bad-direction",
                |p| p.direction = "sideways",
                "\"sideways\"",
            ),
            ("bad-step", |p| p.step = 0, "cash_round_step_minor 0"),
            (
                "bad-instant",
                |p| p.approved_at = Some("yesterday"),
                "\"yesterday\"",
            ),
        ];
        for (n, (version, spoil, named)) in (51..).zip(cases) {
            let mut policy = NewPolicy::approved(n, version);
            spoil(&mut policy);
            insert_policy(&t.conn, &policy).unwrap();
            pin(&t, Some(&policy.id)).unwrap();
            match read(&t).unwrap_err() {
                DbError::StoredTaxPolicyInvalid { reason } => {
                    assert!(reason.contains(named), "{version}: {reason}");
                }
                other => panic!("{version}: expected StoredTaxPolicyInvalid, found {other:?}"),
            }
        }

        let mut short = NewPolicy::approved(61, "short-id");
        short.id.truncate(15);
        insert_policy(&t.conn, &short).unwrap();
        pin(&t, Some(&short.id)).unwrap();
        assert!(matches!(
            read(&t).unwrap_err(),
            DbError::IdWidthInvalid {
                table: "store",
                column: "tax_computation_policy_id",
                found: 15
            }
        ));
    }
}
