//! Legal metrology for scale-priced labels (microstep 1.2.4, `ref/schema.md`
//! §0007, `ref/hardware-and-receipts.md` §7).
//!
//! **A price-embedded barcode does not remove the scale from the transaction.**
//! The label was produced by an instrument used for trade, so a label may price
//! a line only while the scale that printed it holds current verification
//! evidence. This module is the rule; `0007`'s triggers refuse the same
//! activation in the storage engine, and the repository supplies the scale's
//! current verification event.
//!
//! Two decisions, kept apart because they happen at different moments:
//!
//! * [`rule_may_activate`] — may an embedded-barcode rule bound to this scale
//!   be switched on? Exactly what `embedded_rule_requires_verified_scale_*`
//!   asks: the scale's current evidence says `verified`. Activation is
//!   configuration, and the database has no clock, so it asks nothing else.
//! * [`scale_pricing_allowed`] — may a label from this scale price a line
//!   now? The evidence must still be `verified`, already effective, not
//!   expired, **and the register must trust its own clock**: `0007` says an
//!   untrusted device clock never keeps an expired certificate active, so
//!   insufficient clock confidence fails embedded pricing closed.
//!
//! A refusal here is a commissioning matter, never a damaged label: the
//! operator has to see that it was the scale's evidence that stopped the scan,
//! not a misread, which is why it is its own error type beside `ScanError`.
//!
//! The legal content of the evidence — the JSMO form and the reverification
//! cadence — is the `⚠️ OPEN` item in `ref/schema.md` §0007. Until it is
//! answered, every rule stays inactive and checkout refuses scale-derived
//! pricing; this module is the mechanism that answer will drive.

use serde::{Deserialize, Serialize};

use crate::time::{ClockConfidence, Timestamp};

/// A verification event's state — `trade_scale_verification.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    Verified,
    Revoked,
    /// The scale is out for repair or awaiting reverification. Its labels may
    /// not price anything until a later `verified` event.
    MaintenancePending,
}

/// A scale's **current** verification: its newest event, as
/// `trade_scale_current_verification` selects it. Older events are history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleVerification {
    pub state: VerificationState,
    pub effective_at: Timestamp,
    /// When the evidence lapses, if it does.
    pub valid_until: Option<Timestamp>,
}

/// Why a scale's labels may not price a line.
#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum ScalePricingRefusal {
    #[error("no verification evidence is recorded for this scale")]
    NoEvidence,
    #[error("this scale's verification was revoked")]
    Revoked,
    #[error("this scale is awaiting maintenance or reverification")]
    MaintenancePending,
    #[error("this scale's verification is not effective until {0}")]
    NotYetEffective(Timestamp),
    #[error("this scale's verification expired at {0}")]
    Expired(Timestamp),
    /// The register cannot judge a validity window with a clock it does not
    /// trust, so it refuses rather than assume the certificate is current.
    #[error("the register's clock is not trusted, so the scale's verification cannot be judged")]
    ClockNotTrusted,
}

/// Whether an embedded-barcode rule bound to this scale may be activated:
/// the scale's current evidence must be `verified`.
///
/// # Errors
///
/// [`ScalePricingRefusal`] for a scale with no evidence, or whose current
/// event revoked it or put it into maintenance.
pub fn rule_may_activate(current: Option<&ScaleVerification>) -> Result<(), ScalePricingRefusal> {
    match current.map(|verification| verification.state) {
        None => Err(ScalePricingRefusal::NoEvidence),
        Some(VerificationState::Verified) => Ok(()),
        Some(VerificationState::Revoked) => Err(ScalePricingRefusal::Revoked),
        Some(VerificationState::MaintenancePending) => Err(ScalePricingRefusal::MaintenancePending),
    }
}

/// Whether a label printed by this scale may price a line at `now`, a
/// business-time reading whose confidence is `clock`.
///
/// # Errors
///
/// Everything [`rule_may_activate`] refuses, then
/// [`ScalePricingRefusal::ClockNotTrusted`] for any clock short of
/// [`ClockConfidence::Trusted`], [`ScalePricingRefusal::NotYetEffective`]
/// before the evidence takes effect, and [`ScalePricingRefusal::Expired`] at
/// or after `valid_until`.
pub fn scale_pricing_allowed(
    current: Option<&ScaleVerification>,
    now: Timestamp,
    clock: ClockConfidence,
) -> Result<(), ScalePricingRefusal> {
    rule_may_activate(current)?;
    let Some(verification) = current else {
        return Err(ScalePricingRefusal::NoEvidence);
    };
    if clock != ClockConfidence::Trusted {
        return Err(ScalePricingRefusal::ClockNotTrusted);
    }
    if now < verification.effective_at {
        return Err(ScalePricingRefusal::NotYetEffective(
            verification.effective_at,
        ));
    }
    if let Some(until) = verification.valid_until
        && now >= until
    {
        return Err(ScalePricingRefusal::Expired(until));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn at(iso: &str) -> Timestamp {
        Timestamp::parse_iso8601(iso).unwrap()
    }

    fn verified() -> ScaleVerification {
        ScaleVerification {
            state: VerificationState::Verified,
            effective_at: at("2026-01-01T00:00:00.000Z"),
            valid_until: Some(at("2027-01-01T00:00:00.000Z")),
        }
    }

    fn with_state(state: VerificationState) -> ScaleVerification {
        ScaleVerification {
            state,
            ..verified()
        }
    }

    const NOON: &str = "2026-08-25T12:00:00.000Z";

    /// `0007`'s activation triggers, in the domain: a rule bound to a scale
    /// may be switched on only while the scale's current evidence is
    /// `verified`. No evidence, a revocation and a pending maintenance are
    /// each refused, by name.
    #[test]
    fn unverified_trade_scale_cannot_activate_an_embedded_rule() {
        assert_eq!(
            rule_may_activate(None),
            Err(ScalePricingRefusal::NoEvidence)
        );
        assert_eq!(
            rule_may_activate(Some(&with_state(VerificationState::Revoked))),
            Err(ScalePricingRefusal::Revoked)
        );
        assert_eq!(
            rule_may_activate(Some(&with_state(VerificationState::MaintenancePending))),
            Err(ScalePricingRefusal::MaintenancePending)
        );
        assert_eq!(rule_may_activate(Some(&verified())), Ok(()));
    }

    /// A scale whose newest event puts it into maintenance stops pricing at
    /// once, though its rule was activated while it was verified and is still
    /// switched on.
    #[test]
    fn maintenance_pending_disables_scale_pricing() {
        assert_eq!(
            scale_pricing_allowed(Some(&verified()), at(NOON), ClockConfidence::Trusted),
            Ok(())
        );
        let pending = ScaleVerification {
            state: VerificationState::MaintenancePending,
            effective_at: at("2026-08-25T09:00:00.000Z"),
            valid_until: None,
        };
        assert_eq!(
            scale_pricing_allowed(Some(&pending), at(NOON), ClockConfidence::Trusted),
            Err(ScalePricingRefusal::MaintenancePending)
        );
    }

    /// Evidence has a window, and the window is judged with a trusted clock
    /// or not at all.
    #[test]
    fn scale_evidence_prices_only_inside_its_window_on_a_trusted_clock() {
        let evidence = verified();
        assert_eq!(
            scale_pricing_allowed(
                Some(&evidence),
                at("2025-12-31T23:59:59.999Z"),
                ClockConfidence::Trusted
            ),
            Err(ScalePricingRefusal::NotYetEffective(evidence.effective_at))
        );
        assert_eq!(
            scale_pricing_allowed(
                Some(&evidence),
                at("2027-01-01T00:00:00.000Z"),
                ClockConfidence::Trusted
            ),
            Err(ScalePricingRefusal::Expired(at("2027-01-01T00:00:00.000Z")))
        );
        for clock in [
            ClockConfidence::Stale { age_ms: 1 },
            ClockConfidence::Suspect { skew_ms: 1 },
            ClockConfidence::Untrusted,
        ] {
            assert_eq!(
                scale_pricing_allowed(Some(&evidence), at(NOON), clock),
                Err(ScalePricingRefusal::ClockNotTrusted),
                "{clock:?}"
            );
        }
        let open_ended = ScaleVerification {
            valid_until: None,
            ..evidence
        };
        assert_eq!(
            scale_pricing_allowed(
                Some(&open_ended),
                at("2030-01-01T00:00:00.000Z"),
                ClockConfidence::Trusted
            ),
            Ok(())
        );
    }
}
