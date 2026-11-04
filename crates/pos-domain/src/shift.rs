//! The minimal shift lifecycle (microstep 1.9.5, `ref/ipc-contract.md`'s
//! shift commands, conventions §11).
//!
//! A sale belongs to a shift, and its business date is its shift's. Phase 1
//! therefore opens a shift with a counted float, keeps one open per register,
//! and closes it. The blind count, over/short, drawer movements and X/Z are
//! Phase 2's, and nothing of them is smuggled in here.
//!
//! * **The business date is decided once, at opening.** With a clock the
//!   register trusts, it is derived from the instant and the store's day
//!   boundary, and a date the caller supplies is refused: an override that works
//!   when it is not needed is an override that gets used. With a `Suspect` or
//!   `Untrusted` clock it cannot be derived honestly, so the operator must
//!   confirm one, and the opening's audit row says so. A bad clock never
//!   silently assigns a filing day, and never closes the store.
//! * **The opening float is counted by denomination.** Only the currency's own
//!   pieces count, so a float is a sum of things a person can hold.
//! * **Only the person who opened a shift closes it on this path,** holding
//!   `shift.close` and no approval: closing your own work is not an escalation.
//!   Another user's stale shift waits for Phase 2's `shift_force_close_stale`.
//! * **A shift's till starts out of training** (E.54). Training is the till's,
//!   not the register's, and a new shift is a new till, so a training session
//!   left on at close does not survive into the next shift's sales.

use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::audit::AuditIntent;
use crate::cart::CartContext;
use crate::ids::{RegisterId, ShiftId, StoreId, UserId};
use crate::money::{Currency, Money, MoneyError};
use crate::permissions::{Authorized, Capability, cap};
use crate::tender::denominations;
use crate::time::{BusinessDate, ClockConfidence, DayBoundary, Timestamp, business_date_of};

/// A shift as its immutable opening fact records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shift {
    pub id: ShiftId,
    pub register_id: RegisterId,
    pub store_id: StoreId,
    pub business_date: BusinessDate,
    pub opened_by: UserId,
    pub opened_at: Timestamp,
    /// The opening float, counted by denomination.
    pub float: Money,
}

/// The business date a shift opens on, and whether a person had to confirm
/// it because the clock could not be trusted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpeningDate {
    pub date: BusinessDate,
    pub confirmed_by_operator: bool,
}

/// What opening a shift needs, every id and instant an argument (I-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShiftOpening {
    pub id: ShiftId,
    pub register_id: RegisterId,
    pub store_id: StoreId,
    pub float: Money,
    pub date: OpeningDate,
    /// The clock's confidence when the date was decided, for the audit row.
    pub confidence: ClockConfidence,
    pub opened_at: Timestamp,
}

/// The fact that closes a shift. It never changes the opening fact; the
/// shift's state is a projection of the two.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShiftClose {
    pub id: Uuid,
    pub shift_id: ShiftId,
    pub closed_by: UserId,
    pub closed_at: Timestamp,
}

/// Why a shift cannot be opened or closed as asked.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ShiftError {
    /// The clock is trusted, so the business date is derived; supplying one is
    /// refused rather than quietly honoured.
    #[error("the clock is trusted, so the business date is derived and none may be supplied")]
    SuppliedDateRefused,
    /// The clock is not trusted, so a person must confirm the business date.
    #[error(
        "the clock cannot be trusted to date this shift; the operator must confirm its business date"
    )]
    ConfirmedDateRequired,
    /// A float is counted in the currency's own pieces.
    #[error("{} {} is not a piece of that currency", .0.minor(), .0.currency().code())]
    NotADenomination(Money),
    /// Only the person who opened a shift closes it on the ordinary path.
    #[error("shift opened by {opened_by} cannot be closed by {actor} on the ordinary path")]
    NotTheOpener { opened_by: UserId, actor: UserId },
    /// Closing one's own shift is not an escalation, so it takes no approval.
    #[error("closing one's own shift takes no approval")]
    ApprovalNotTaken,
    /// A shift closes after it opened.
    #[error("a shift cannot close before it opened")]
    ClosedBeforeOpened,
    #[error(transparent)]
    Money(#[from] MoneyError),
}

/// The business date a shift opening at `now` takes.
///
/// With a `Trusted` or `Stale` clock it is derived from `now` and the store's
/// day `boundary` (conventions §11), and a `supplied` date is refused. With a
/// `Suspect` or `Untrusted` clock it cannot be derived honestly, so `supplied`
/// is required and is marked as the operator's.
///
/// # Errors
///
/// [`ShiftError::SuppliedDateRefused`] and [`ShiftError::ConfirmedDateRequired`].
pub fn opening_business_date(
    confidence: ClockConfidence,
    now: Timestamp,
    boundary: DayBoundary,
    supplied: Option<BusinessDate>,
) -> Result<OpeningDate, ShiftError> {
    match (confidence.requires_business_date_confirmation(), supplied) {
        (false, None) => Ok(OpeningDate {
            date: business_date_of(now, boundary),
            confirmed_by_operator: false,
        }),
        (false, Some(_)) => Err(ShiftError::SuppliedDateRefused),
        (true, Some(date)) => Ok(OpeningDate {
            date,
            confirmed_by_operator: true,
        }),
        (true, None) => Err(ShiftError::ConfirmedDateRequired),
    }
}

/// The opening float: each piece counted, times its count, in `currency`.
///
/// # Errors
///
/// [`ShiftError::NotADenomination`] for a piece the currency does not have,
/// or any piece of a currency with no denominations; [`ShiftError::Money`]
/// for a piece in another currency or a sum beyond range.
pub fn count_float(currency: Currency, counted: &[(Money, u32)]) -> Result<Money, ShiftError> {
    let pieces = denominations(currency).unwrap_or_default();
    counted
        .iter()
        .try_fold(Money::zero(currency), |total, &(piece, count)| {
            if !pieces.contains(&piece) {
                return Err(ShiftError::NotADenomination(piece));
            }
            let value = piece
                .minor()
                .checked_mul(i64::from(count))
                .ok_or(MoneyError::Overflow)?;
            Ok(total.checked_add(Money::from_minor(value, currency))?)
        })
}

/// Open a shift: the opening fact, and the audit intent that commits with it
/// in its envelope. The opener is the proof's actor.
pub fn open_shift(
    opener: &Authorized<cap::ShiftOpen>,
    opening: &ShiftOpening,
) -> (Shift, AuditIntent) {
    let shift = Shift {
        id: opening.id,
        register_id: opening.register_id,
        store_id: opening.store_id,
        business_date: opening.date.date,
        opened_by: opener.actor(),
        opened_at: opening.opened_at,
        float: opening.float,
    };
    let audit = AuditIntent {
        actor: opener.actor(),
        approver: None,
        approval: None,
        action: cap::ShiftOpen::NAME,
        entity: "shift",
        entity_id: opening.id.as_uuid(),
        reason: opening
            .date
            .confirmed_by_operator
            .then(|| "business date confirmed by the operator".to_owned()),
        payload: json!({
            "business_date": opening.date.date.to_iso(),
            "business_date_source": if opening.date.confirmed_by_operator {
                "operator_confirmed"
            } else {
                "derived"
            },
            "clock_confidence": confidence_token(opening.confidence),
            "float_minor": opening.float.minor(),
        }),
        at: opening.opened_at,
    };
    (shift, audit)
}

fn confidence_token(confidence: ClockConfidence) -> &'static str {
    match confidence {
        ClockConfidence::Trusted => "trusted",
        ClockConfidence::Stale { .. } => "stale",
        ClockConfidence::Suspect { .. } => "suspect",
        ClockConfidence::Untrusted => "untrusted",
    }
}

impl Shift {
    /// Close this shift on the ordinary path: the close fact, and the audit
    /// intent that commits with it.
    ///
    /// # Errors
    ///
    /// [`ShiftError::NotTheOpener`] when the closer did not open it,
    /// [`ShiftError::ApprovalNotTaken`] when the proof carries an approval, and
    /// [`ShiftError::ClosedBeforeOpened`] for a close before the opening.
    pub fn close(
        &self,
        closer: &Authorized<cap::ShiftClose>,
        id: Uuid,
        closed_at: Timestamp,
    ) -> Result<(ShiftClose, AuditIntent), ShiftError> {
        if closer.actor() != self.opened_by {
            return Err(ShiftError::NotTheOpener {
                opened_by: self.opened_by,
                actor: closer.actor(),
            });
        }
        if closer.approval().is_some() {
            return Err(ShiftError::ApprovalNotTaken);
        }
        if closed_at < self.opened_at {
            return Err(ShiftError::ClosedBeforeOpened);
        }
        let close = ShiftClose {
            id,
            shift_id: self.id,
            closed_by: closer.actor(),
            closed_at,
        };
        let audit = AuditIntent {
            actor: closer.actor(),
            approver: None,
            approval: None,
            action: cap::ShiftClose::NAME,
            entity: "shift_close_event",
            entity_id: id,
            reason: None,
            payload: json!({
                "business_date": self.business_date.to_iso(),
                "shift_id": self.id.as_uuid().hyphenated().to_string(),
            }),
            at: closed_at,
        };
        Ok((close, audit))
    }

    /// The till `cashier` works at in this shift. It always starts out of
    /// training: training is a till's, a new shift is a new till, and so a
    /// training session left on at close never reaches the next shift's sales
    /// (E.54). Entering training again is the explicit `training_mode.toggle`.
    pub fn till(&self, cashier: UserId) -> CartContext {
        CartContext {
            register_id: self.register_id,
            shift_id: self.id,
            cashier_id: cashier,
            currency: self.float.currency(),
            is_training: false,
            opened_at: self.opened_at,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;

    use super::*;
    use crate::permissions::{ApprovalBinding, EscalationPolicy, GrantSet, Role, authorize};

    const JOD: Currency = Currency::JOD;

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, JOD)
    }

    fn user(n: u128) -> UserId {
        UserId::from_uuid(Uuid::from_u128(n))
    }

    fn at(iso: &str) -> Timestamp {
        Timestamp::parse_iso8601(iso).unwrap()
    }

    /// Amman, UTC+3, with the default 04:00 cutover.
    fn amman() -> DayBoundary {
        DayBoundary::new(180, 240).unwrap()
    }

    fn unbound() -> ApprovalBinding {
        ApprovalBinding {
            entity_id: Uuid::nil(),
            amount_minor: 0,
            content_hash: None,
        }
    }

    fn opener(who: UserId) -> Authorized<cap::ShiftOpen> {
        authorize::<cap::ShiftOpen>(
            who,
            &GrantSet::of_role(Role::Cashier),
            None,
            &unbound(),
            &EscalationPolicy::empty(),
            at("2026-08-25T06:00:00.000Z"),
        )
        .unwrap()
    }

    fn closer(who: UserId) -> Authorized<cap::ShiftClose> {
        authorize::<cap::ShiftClose>(
            who,
            &GrantSet::of_role(Role::Cashier),
            None,
            &unbound(),
            &EscalationPolicy::empty(),
            at("2026-08-25T18:00:00.000Z"),
        )
        .unwrap()
    }

    fn opened(who: UserId, date: OpeningDate) -> (Shift, AuditIntent) {
        open_shift(
            &opener(who),
            &ShiftOpening {
                id: ShiftId::from_uuid(Uuid::from_u128(0x53)),
                register_id: RegisterId::from_uuid(Uuid::from_u128(0x51)),
                store_id: StoreId::from_uuid(Uuid::from_u128(0x52)),
                float: jod(50_000),
                date,
                confidence: if date.confirmed_by_operator {
                    ClockConfidence::Untrusted
                } else {
                    ClockConfidence::Trusted
                },
                opened_at: at("2026-08-25T06:00:00.000Z"),
            },
        )
    }

    fn derived(iso: &str) -> OpeningDate {
        OpeningDate {
            date: BusinessDate::parse(iso).unwrap(),
            confirmed_by_operator: false,
        }
    }

    #[test]
    fn trusted_clock_refuses_a_supplied_business_date() {
        let early = at("2026-08-24T22:30:00.000Z"); // 01:30 in Amman, before the cutover
        let aug_24 = BusinessDate::parse("2026-08-24").unwrap();
        for confidence in [
            ClockConfidence::Trusted,
            ClockConfidence::Stale { age_ms: 90_000 },
        ] {
            assert_eq!(
                opening_business_date(confidence, early, amman(), None),
                Ok(OpeningDate {
                    date: aug_24,
                    confirmed_by_operator: false
                }),
                "{confidence:?}: an opening at 01:30 belongs to yesterday's trading day"
            );
            assert_eq!(
                opening_business_date(confidence, early, amman(), Some(aug_24)),
                Err(ShiftError::SuppliedDateRefused),
                "{confidence:?}: even the right date is refused when it is not needed"
            );
        }
    }

    #[test]
    fn an_untrusted_clock_requires_the_operator_to_confirm_the_date() {
        let now = at("2026-08-25T06:00:00.000Z");
        let confirmed = BusinessDate::parse("2026-08-25").unwrap();
        for confidence in [
            ClockConfidence::Suspect { skew_ms: 7_200_000 },
            ClockConfidence::Untrusted,
        ] {
            assert_eq!(
                opening_business_date(confidence, now, amman(), None),
                Err(ShiftError::ConfirmedDateRequired)
            );
            assert_eq!(
                opening_business_date(confidence, now, amman(), Some(confirmed)),
                Ok(OpeningDate {
                    date: confirmed,
                    confirmed_by_operator: true
                })
            );
        }
        let (_, audit) = opened(
            user(1),
            OpeningDate {
                date: confirmed,
                confirmed_by_operator: true,
            },
        );
        assert_eq!(
            audit.reason.as_deref(),
            Some("business date confirmed by the operator")
        );
        assert_eq!(
            audit.payload.get("business_date_source"),
            Some(&json!("operator_confirmed"))
        );
        assert_eq!(
            audit.payload.get("clock_confidence"),
            Some(&json!("untrusted"))
        );
    }

    #[test]
    fn the_opening_float_is_counted_in_the_currencys_pieces() {
        // Two twenty-dinar notes, a ten, and four quarter-dinar coins: 51.000.
        let counted = [(jod(20_000), 2), (jod(10_000), 1), (jod(250), 4)];
        assert_eq!(count_float(JOD, &counted), Ok(jod(51_000)));
        assert_eq!(count_float(JOD, &[]), Ok(jod(0)));
        assert_eq!(
            count_float(JOD, &[(jod(20_000), 1), (jod(30_000), 1)]),
            Err(ShiftError::NotADenomination(jod(30_000))),
            "there is no thirty-dinar note"
        );
        let usd = Money::from_minor(100, Currency::USD);
        assert_eq!(
            count_float(Currency::USD, &[(usd, 1)]),
            Err(ShiftError::NotADenomination(usd)),
            "a currency with no denominations counts nothing"
        );
    }

    #[test]
    fn only_the_opener_closes_a_shift_and_without_approval() {
        let (shift, opening_audit) = opened(user(1), derived("2026-08-25"));
        assert_eq!(opening_audit.action, "shift.open");
        assert_eq!(opening_audit.reason, None);
        assert_eq!(
            opening_audit.payload.get("business_date_source"),
            Some(&json!("derived"))
        );

        let close_id = Uuid::from_u128(0xC0);
        let later = at("2026-08-25T18:00:00.000Z");
        let (close, audit) = shift.close(&closer(user(1)), close_id, later).unwrap();
        assert_eq!(
            close,
            ShiftClose {
                id: close_id,
                shift_id: shift.id,
                closed_by: user(1),
                closed_at: later,
            }
        );
        assert_eq!(
            (audit.action, audit.entity, audit.entity_id, audit.approver),
            ("shift.close", "shift_close_event", close_id, None)
        );

        assert_eq!(
            shift.close(&closer(user(2)), close_id, later),
            Err(ShiftError::NotTheOpener {
                opened_by: user(1),
                actor: user(2)
            })
        );
        assert_eq!(
            shift.close(&closer(user(1)), close_id, at("2026-08-25T05:59:59.999Z")),
            Err(ShiftError::ClosedBeforeOpened)
        );
    }

    #[test]
    fn a_close_proof_that_carries_an_approval_is_refused() {
        let (shift, _) = opened(user(1), derived("2026-08-25"));
        let binding = unbound();
        let manager = authorize::<cap::ShiftClose>(
            user(3),
            &GrantSet::of_role(Role::Manager),
            None,
            &binding,
            &EscalationPolicy::empty(),
            at("2026-08-25T17:00:00.000Z"),
        )
        .unwrap();
        let handle = crate::permissions::ApprovalHandle::issue::<cap::ShiftClose>(
            crate::ids::ApprovalId::from_uuid(Uuid::from_u128(0xA1)),
            user(1),
            &manager,
            &binding,
            "synthetic".to_owned(),
            at("2026-08-25T17:00:00.000Z"),
            600_000,
            // Derived from the shift's id rather than written as a literal.
            shift.id.as_uuid().as_bytes().map(|b| b ^ 0xA1),
        )
        .unwrap();
        let escalated = authorize::<cap::ShiftClose>(
            user(1),
            &GrantSet::of_role(Role::Cashier),
            Some(&handle),
            &binding,
            &EscalationPolicy::empty().with_escalation::<cap::ShiftClose>(),
            at("2026-08-25T17:01:00.000Z"),
        )
        .unwrap();
        assert_eq!(
            shift.close(
                &escalated,
                Uuid::from_u128(0xC0),
                at("2026-08-25T18:00:00.000Z")
            ),
            Err(ShiftError::ApprovalNotTaken)
        );
    }

    /// E.54: a till left in training at close does not carry training into the
    /// next shift. The next shift's till starts out of it.
    #[test]
    fn training_auto_off_at_shift_close() {
        let (first, _) = opened(user(1), derived("2026-08-25"));
        assert!(
            !first.till(user(1)).is_training,
            "a shift's till starts out of training"
        );
        // The cashier enters training and leaves it on at close.
        let left_on = CartContext {
            is_training: true,
            ..first.till(user(1))
        };
        first
            .close(
                &closer(user(1)),
                Uuid::from_u128(0xC0),
                at("2026-08-25T18:00:00.000Z"),
            )
            .unwrap();

        let next = Shift {
            id: ShiftId::from_uuid(Uuid::from_u128(0x54)),
            opened_at: at("2026-08-26T06:00:00.000Z"),
            business_date: BusinessDate::parse("2026-08-26").unwrap(),
            ..first.clone()
        };
        let next_till = next.till(user(1));
        assert!(
            left_on.is_training && !next_till.is_training,
            "training does not survive a shift close"
        );
        assert_eq!(next_till.shift_id, next.id);
        assert_eq!(next_till.currency, JOD);
    }

    fn any_confidence() -> impl Strategy<Value = ClockConfidence> {
        prop_oneof![
            Just(ClockConfidence::Trusted),
            (0i64..10_000_000).prop_map(|age_ms| ClockConfidence::Stale { age_ms }),
            (-10_000_000i64..10_000_000).prop_map(|skew_ms| ClockConfidence::Suspect { skew_ms }),
            Just(ClockConfidence::Untrusted),
        ]
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// A shift's business date has exactly one author. A trusted clock
        /// decides it and refuses to be overruled; a clock that cannot be
        /// trusted never decides it and waits for a person. No opening is
        /// both derived and confirmed, and none is neither.
        #[test]
        fn prop_a_business_date_is_derived_or_confirmed_never_both(
            confidence in any_confidence(),
            now_ms in 1_700_000_000_000i64..1_900_000_000_000,
            offset in -720i16..=840,
            cutover in 0u16..1_440,
            supplied in prop::option::of((2020i16..2030, 1u8..=12, 1u8..=28)),
        ) {
            let boundary = DayBoundary::new(offset, cutover).unwrap();
            let now = Timestamp::from_epoch_milliseconds(now_ms).unwrap();
            let supplied = supplied.map(|(y, m, d)| BusinessDate::new(y, m, d).unwrap());
            let result = opening_business_date(confidence, now, boundary, supplied);
            let needs_a_person = confidence.requires_business_date_confirmation();
            match (needs_a_person, supplied) {
                (false, None) => prop_assert_eq!(result, Ok(OpeningDate {
                    date: business_date_of(now, boundary),
                    confirmed_by_operator: false,
                })),
                (false, Some(_)) => prop_assert_eq!(result, Err(ShiftError::SuppliedDateRefused)),
                (true, Some(date)) => prop_assert_eq!(result, Ok(OpeningDate {
                    date,
                    confirmed_by_operator: true,
                })),
                (true, None) => prop_assert_eq!(result, Err(ShiftError::ConfirmedDateRequired)),
            }
        }
    }
}
