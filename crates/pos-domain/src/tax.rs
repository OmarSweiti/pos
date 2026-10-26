//! The tax engine (group 1.3, `ref/tax-jordan.md`).
//!
//! The value types came first (1.3.1): fixed and ad-valorem bases, persisted
//! component order and named dependencies, and the evidence explaining why one
//! particular supply received its treatment. [`resolve_components`] (1.3.2)
//! chooses which rules apply to a line. The arithmetic arrives from 1.3.3 on.
//! No value here is a jurisdiction rate, rounding rule, tolerance or default:
//! rates are data, and a rate compiled into code is a release every time the
//! official schedule changes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::TaxCategoryId;
use crate::money::{Money, MoneyError, Percent};
use crate::time::Timestamp;

/// How a component is classified for charging and reporting.
///
/// `Zero` and `Exempt` are separate values even though both can produce no tax
/// on a line. Collapsing them would lose the reporting distinction that later
/// reconciliation steps need. There is no `Default`: an absent classification
/// must remain absent rather than becoming a plausible treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaxTreatment {
    Standard,
    Reduced,
    Zero,
    Exempt,
}

/// The two dimensions a component may charge.
///
/// A percentage alone cannot represent a fixed amount per unit, while a fixed
/// amount alone cannot represent an ad-valorem charge. The variants keep those
/// alternatives explicit, and `Compound` requires both halves so neither can
/// disappear when a combined rule crosses a wire or is snapshotted on a sale.
/// `Money` carries the fixed amount's currency; `Percent` carries integer ppm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaxBasis {
    Percentage {
        rate: Percent,
    },
    /// A fixed amount multiplied later by the line's typed quantity.
    PerUnit {
        amount: Money,
    },
    /// Both an ad-valorem rate and a fixed amount per line quantity unit.
    Compound {
        rate: Percent,
        per_unit: Money,
    },
}

/// What a component is charged on.
///
/// A tax-on-tax component names the exact prior component codes whose carried
/// amounts enter its base. It never means "all earlier components" implicitly:
/// the dependency list and its persisted order remain readable later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaxBase {
    LineNet,
    NetPlusComponents { codes: Vec<String> },
}

/// One resolved tax component on a line.
///
/// The component's code, basis, application order and named base are data. A
/// later engine can therefore represent more than one component without
/// recovering order or tax-on-tax dependencies from vector position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaxComponent {
    /// Stable component code from the selected tax pack.
    pub code: String,
    pub treatment: TaxTreatment,
    pub basis: TaxBasis,
    /// Application order, ascending. Equal sequences share a base and cannot
    /// depend on one another.
    pub sequence: u8,
    pub base: TaxBase,
    /// Whether an inclusive price can be decomposed for this component.
    pub is_inclusive_capable: bool,
}

/// Whether the supplied line amount already contains tax.
///
/// No `Default` is provided: selecting a price mode changes money facts and is
/// therefore an explicit input to the future calculation step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriceMode {
    Inclusive,
    Exclusive,
}

/// The jurisdiction pack a store resolves rates from.
///
/// Profiles are closed values so a zone-specific pack cannot be represented as
/// an arbitrary string or silently confused with the standard pack. This type
/// does not choose a profile for a merchant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoreTaxProfile {
    Standard,
    Asez,
    DevelopmentArea,
    Unregistered,
}

/// Where one particular supply goes.
///
/// This is supply evidence, not a product attribute: the same catalogue item
/// can be supplied to different destinations without changing the product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupplyDestination {
    Domestic,
    Export,
    FreeZone,
    DevelopmentArea,
    EligibleBody,
}

/// Why a particular supply is represented as zero-rated.
///
/// The vocabulary is deliberately separate from [`SupplyDestination`]. A
/// destination says where the supply goes; this says which reporting reason
/// was recorded for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZeroRatingReason {
    Export,
    FreeZoneSupply,
    EligibleEntity,
    ProductCategory,
}

/// The supply-specific facts snapshotted onto a sale.
///
/// Category, store profile and effective date cannot explain every zero-rated
/// transaction. Keeping the destination, reason and evidence reference on the
/// sale lets a later report use the evidence captured at sale time instead of
/// today's customer or product record (I-5).
///
/// `reason` and `evidence_ref` remain optional because which reasons the GST
/// return distinguishes, and what evidence each needs, is still an open
/// question (`ref/domain-api.md` §5). Until the merchant's accountant answers
/// it, the owner's decision of 26 October 2026 (`ref/merchant-decisions.md`
/// 6.14) is that only `Domestic` supplies are sold; the calculation that
/// consumes this context refuses every other destination from 1.3.3 on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupplyTaxContext {
    pub destination: SupplyDestination,
    pub reason: Option<ZeroRatingReason>,
    /// Reference to the captured export declaration or eligibility authority.
    pub evidence_ref: Option<String>,
}

/// The carried result for one line.
///
/// `net`, each component, the exact total and `gross` travel together so later
/// summary code never has to reconstruct one from a receipt total. This type
/// stores results only; 1.3.3 onward compute them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineTax {
    pub net: Money,
    pub components: Vec<ComponentTax>,
    pub tax_total: Money,
    pub gross: Money,
    /// Retained so summaries can separate zero-rated supplies by reason without
    /// receiving the cart again.
    pub supply_reason: Option<ZeroRatingReason>,
}

/// The carried result of one component on one line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentTax {
    pub code: String,
    pub treatment: TaxTreatment,
    /// The resolved percentage, fixed amount, or both, preserved on the result.
    pub basis: TaxBasis,
    /// The money base used for this component, snapshotted so later reports do
    /// not have to infer it from today's rules.
    pub base_amount: Money,
    pub amount: Money,
}

/// One receipt-summary row, grouped later by all rate-defining facts.
///
/// A fixed per-unit amount is part of the grouping key: equal percentage rates
/// with different fixed amounts are not the same rate. A supply reason likewise
/// keeps distinct zero-rated reporting reasons from collapsing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaxSummaryRow {
    pub code: String,
    pub treatment: TaxTreatment,
    pub rate: Percent,
    pub per_unit: Option<Money>,
    pub reason: Option<ZeroRatingReason>,
    pub net: Money,
    pub tax: Money,
    pub gross: Money,
}

/// One effective-dated component rule from a tax pack.
///
/// Rates remain data: a future resolver selects rules by category, profile and
/// caller-supplied time. `Timestamp` is a value dependency only; this pure
/// module never reads a clock (I-8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaxRateRule {
    pub tax_category_id: TaxCategoryId,
    pub component_code: String,
    pub treatment: TaxTreatment,
    pub basis: TaxBasis,
    pub sequence: u8,
    pub base: TaxBase,
    /// Inclusive boundary.
    pub valid_from: Timestamp,
    /// Exclusive boundary; `None` leaves the interval open-ended.
    pub valid_to: Option<Timestamp>,
    /// `None` is the standard profile only, never every profile.
    pub profile_scope: Option<StoreTaxProfile>,
}

/// Every typed refusal the tax engine can return as groups 1.3.2–1.3.6 land.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TaxError {
    #[error("no rate rule for category at {0}")]
    NoRuleInEffect(Timestamp),
    #[error("overlapping rate rules for {0}")]
    OverlappingRules(String),
    #[error("inclusive pricing with a component that cannot be inclusive")]
    NotInclusiveCapable,
    #[error("component {0} names a base component {1} that is not on this line")]
    UnknownBaseComponent(String, String),
    #[error("components {0} and {1} depend on each other")]
    CircularComponentBase(String, String),
    #[error("profile {0:?} has no complete rate pack; refusing to fall back")]
    ProfilePackIncomplete(StoreTaxProfile),
    #[error("supply destination {0:?} has no reason code")]
    SupplyReasonMissing(SupplyDestination),
    #[error(transparent)]
    Money(#[from] MoneyError),
}

/// The components that apply to a line of `category` in a store with
/// `profile`, at `at` (`ref/tax-jordan.md` §3, rules 1–4).
///
/// * A rule is in effect from `valid_from`, inclusive, to `valid_to`,
///   exclusive, so a rate that changes at midnight has no ambiguous instant.
/// * A rule with no `profile_scope` belongs to `Standard` only. Every other
///   profile resolves only the rules scoped to it, so an ASEZ store never
///   inherits the mainland pack.
/// * For one component, a rule scoped to the store's profile overrides an
///   unscoped one. Two rules left in effect at the same tier are refused,
///   never chosen between.
/// * A line with no component in effect is refused too. The engine never
///   guesses a rate.
///
/// Components come back in application order — `sequence`, then code —
/// whatever order `rules` arrived in.
///
/// `is_inclusive_capable` is derived, because a rule carries no such field:
/// only a percentage of the line net can be taken back out of an inclusive
/// price by 1.3.4's extraction. A fixed or tax-on-tax component is `false`
/// until 1.3.5 extends the arithmetic, so an inclusive line that carries one
/// fails with [`TaxError::NotInclusiveCapable`] instead of being decomposed by
/// guesswork.
///
/// An `Unregistered` store resolves only rules scoped to it. Whether it charges
/// any GST at all is 1.3.8's short-circuit, which comes before resolution.
///
/// # Errors
///
/// [`TaxError::OverlappingRules`] naming the component with more than one rule
/// in effect, and [`TaxError::NoRuleInEffect`] when no component is.
pub fn resolve_components(
    category: TaxCategoryId,
    rules: &[TaxRateRule],
    profile: StoreTaxProfile,
    at: Timestamp,
) -> Result<Vec<TaxComponent>, TaxError> {
    // For each component, the rules in effect that apply to this profile:
    // those scoped to it, and the unscoped ones a `Standard` store reads.
    let mut tiers: BTreeMap<&str, (Vec<&TaxRateRule>, Vec<&TaxRateRule>)> = BTreeMap::new();
    for rule in rules {
        if rule.tax_category_id != category || !in_effect(rule, at) {
            continue;
        }
        let (scoped, unscoped) = tiers.entry(rule.component_code.as_str()).or_default();
        match rule.profile_scope {
            Some(scope) if scope == profile => scoped.push(rule),
            None if profile == StoreTaxProfile::Standard => unscoped.push(rule),
            Some(_) | None => {}
        }
    }

    let mut components = Vec::with_capacity(tiers.len());
    for (code, (scoped, unscoped)) in tiers {
        let tier = if scoped.is_empty() { unscoped } else { scoped };
        let rule = match tier.as_slice() {
            // Every rule for this component belongs to another profile.
            [] => continue,
            [rule] => *rule,
            _ => return Err(TaxError::OverlappingRules(code.to_owned())),
        };
        components.push(TaxComponent {
            code: rule.component_code.clone(),
            treatment: rule.treatment,
            basis: rule.basis.clone(),
            sequence: rule.sequence,
            base: rule.base.clone(),
            is_inclusive_capable: matches!(
                (&rule.basis, &rule.base),
                (TaxBasis::Percentage { .. }, TaxBase::LineNet)
            ),
        });
    }
    if components.is_empty() {
        return Err(TaxError::NoRuleInEffect(at));
    }
    components.sort_by(|a, b| {
        a.sequence
            .cmp(&b.sequence)
            .then_with(|| a.code.cmp(&b.code))
    });
    Ok(components)
}

/// `valid_from` inclusive, `valid_to` exclusive or absent.
fn in_effect(rule: &TaxRateRule, at: Timestamp) -> bool {
    rule.valid_from <= at && rule.valid_to.is_none_or(|to| at < to)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::ids::TaxCategoryId;
    use crate::money::{Currency, Money, Percent};
    use crate::time::Timestamp;
    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;
    use uuid::Uuid;

    // These are wire-format review tripwires, not tax arithmetic vectors. Every
    // code, rate, amount, instant and id is visibly synthetic and carries no
    // claim about a real jurisdiction or merchant configuration.
    const GOLDEN_COMPOUND_BASIS_JSON: &str =
        r#"{"Compound":{"rate":123456,"per_unit":{"minor":789,"currency":"EUR"}}}"#;
    const GOLDEN_BASE_JSON: &str =
        r#"{"NetPlusComponents":{"codes":["SYNTH_FIXED_B","SYNTH_FIXED_A"]}}"#;
    const GOLDEN_COMPONENT_JSON: &str = r#"{"code":"SYNTH_COMBINED","treatment":"Reduced","basis":{"Compound":{"rate":123456,"per_unit":{"minor":789,"currency":"EUR"}}},"sequence":7,"base":{"NetPlusComponents":{"codes":["SYNTH_FIXED_B","SYNTH_FIXED_A"]}},"is_inclusive_capable":false}"#;
    const GOLDEN_LINE_JSON: &str = r#"{"net":{"minor":10000,"currency":"EUR"},"components":[{"code":"SYNTH_COMBINED","treatment":"Reduced","basis":{"Compound":{"rate":123456,"per_unit":{"minor":789,"currency":"EUR"}}},"base_amount":{"minor":10000,"currency":"EUR"},"amount":{"minor":321,"currency":"EUR"}}],"tax_total":{"minor":321,"currency":"EUR"},"gross":{"minor":10321,"currency":"EUR"},"supply_reason":"EligibleEntity"}"#;
    const GOLDEN_SUMMARY_JSON: &str = r#"{"code":"SYNTH_COMBINED","treatment":"Reduced","rate":123456,"per_unit":{"minor":789,"currency":"EUR"},"reason":"EligibleEntity","net":{"minor":10000,"currency":"EUR"},"tax":{"minor":321,"currency":"EUR"},"gross":{"minor":10321,"currency":"EUR"}}"#;
    const GOLDEN_RULE_JSON: &str = r#"{"tax_category_id":"00000000-0000-0000-0000-000000000001","component_code":"SYNTH_COMBINED","treatment":"Reduced","basis":{"Compound":{"rate":123456,"per_unit":{"minor":789,"currency":"EUR"}}},"sequence":7,"base":{"NetPlusComponents":{"codes":["SYNTH_FIXED_B","SYNTH_FIXED_A"]}},"valid_from":1234567,"valid_to":2345678,"profile_scope":"DevelopmentArea"}"#;

    fn synthetic_basis() -> TaxBasis {
        TaxBasis::Compound {
            rate: Percent::from_ppm(123_456),
            per_unit: Money::from_minor(789, Currency::EUR),
        }
    }

    fn synthetic_base() -> TaxBase {
        TaxBase::NetPlusComponents {
            codes: vec!["SYNTH_FIXED_B".to_owned(), "SYNTH_FIXED_A".to_owned()],
        }
    }

    fn synthetic_component() -> TaxComponent {
        TaxComponent {
            code: "SYNTH_COMBINED".to_owned(),
            treatment: TaxTreatment::Reduced,
            basis: synthetic_basis(),
            sequence: 7,
            base: synthetic_base(),
            is_inclusive_capable: false,
        }
    }

    fn synthetic_component_tax() -> ComponentTax {
        ComponentTax {
            code: "SYNTH_COMBINED".to_owned(),
            treatment: TaxTreatment::Reduced,
            basis: synthetic_basis(),
            base_amount: Money::from_minor(10_000, Currency::EUR),
            amount: Money::from_minor(321, Currency::EUR),
        }
    }

    fn synthetic_line_tax() -> LineTax {
        LineTax {
            net: Money::from_minor(10_000, Currency::EUR),
            components: vec![synthetic_component_tax()],
            tax_total: Money::from_minor(321, Currency::EUR),
            gross: Money::from_minor(10_321, Currency::EUR),
            supply_reason: Some(ZeroRatingReason::EligibleEntity),
        }
    }

    fn synthetic_summary() -> TaxSummaryRow {
        TaxSummaryRow {
            code: "SYNTH_COMBINED".to_owned(),
            treatment: TaxTreatment::Reduced,
            rate: Percent::from_ppm(123_456),
            per_unit: Some(Money::from_minor(789, Currency::EUR)),
            reason: Some(ZeroRatingReason::EligibleEntity),
            net: Money::from_minor(10_000, Currency::EUR),
            tax: Money::from_minor(321, Currency::EUR),
            gross: Money::from_minor(10_321, Currency::EUR),
        }
    }

    fn synthetic_rule() -> TaxRateRule {
        TaxRateRule {
            tax_category_id: TaxCategoryId::from_uuid(Uuid::from_u128(1)),
            component_code: "SYNTH_COMBINED".to_owned(),
            treatment: TaxTreatment::Reduced,
            basis: synthetic_basis(),
            sequence: 7,
            base: synthetic_base(),
            valid_from: Timestamp::from_epoch_milliseconds(1_234_567).unwrap(),
            valid_to: Some(Timestamp::from_epoch_milliseconds(2_345_678).unwrap()),
            profile_scope: Some(StoreTaxProfile::DevelopmentArea),
        }
    }

    #[test]
    fn compound_tax_basis_round_trips_without_loss() {
        let basis = synthetic_basis();
        let encoded = serde_json::to_string(&basis).unwrap();
        assert_eq!(encoded, GOLDEN_COMPOUND_BASIS_JSON);

        let decoded = serde_json::from_str::<TaxBasis>(&encoded).unwrap();
        assert_eq!(decoded, basis);
        assert_eq!(
            decoded,
            TaxBasis::Compound {
                rate: Percent::from_ppm(123_456),
                per_unit: Money::from_minor(789, Currency::EUR),
            }
        );
    }

    #[test]
    fn tax_base_preserves_named_component_dependency_order() {
        let base = synthetic_base();
        let encoded = serde_json::to_string(&base).unwrap();
        assert_eq!(encoded, GOLDEN_BASE_JSON);
        assert_eq!(serde_json::from_str::<TaxBase>(&encoded).unwrap(), base);
        assert_eq!(
            base,
            TaxBase::NetPlusComponents {
                codes: vec!["SYNTH_FIXED_B".to_owned(), "SYNTH_FIXED_A".to_owned()],
            }
        );
    }

    #[test]
    fn supply_tax_context_carries_destination_reason_and_evidence() {
        let context = SupplyTaxContext {
            destination: SupplyDestination::EligibleBody,
            reason: Some(ZeroRatingReason::EligibleEntity),
            evidence_ref: Some("SYNTH-EVIDENCE-0001".to_owned()),
        };
        let encoded = serde_json::to_string(&context).unwrap();
        assert_eq!(
            encoded,
            r#"{"destination":"EligibleBody","reason":"EligibleEntity","evidence_ref":"SYNTH-EVIDENCE-0001"}"#
        );
        assert_eq!(
            serde_json::from_str::<SupplyTaxContext>(&encoded).unwrap(),
            context
        );
    }

    #[test]
    fn tax_error_names_the_component_and_unknown_base_code() {
        let error =
            TaxError::UnknownBaseComponent("SYNTH_COMBINED".to_owned(), "SYNTH_MISSING".to_owned());
        assert_eq!(
            error,
            TaxError::UnknownBaseComponent("SYNTH_COMBINED".to_owned(), "SYNTH_MISSING".to_owned(),)
        );
        assert_eq!(
            error.to_string(),
            "component SYNTH_COMBINED names a base component SYNTH_MISSING that is not on this line"
        );
    }

    #[test]
    fn golden_tax_json_is_stable() {
        assert_eq!(
            serde_json::to_string(&synthetic_component()).unwrap(),
            GOLDEN_COMPONENT_JSON
        );
        assert_eq!(
            serde_json::from_str::<TaxComponent>(GOLDEN_COMPONENT_JSON).unwrap(),
            synthetic_component()
        );

        assert_eq!(
            serde_json::to_string(&synthetic_line_tax()).unwrap(),
            GOLDEN_LINE_JSON
        );
        assert_eq!(
            serde_json::from_str::<LineTax>(GOLDEN_LINE_JSON).unwrap(),
            synthetic_line_tax()
        );

        assert_eq!(
            serde_json::to_string(&synthetic_summary()).unwrap(),
            GOLDEN_SUMMARY_JSON
        );
        assert_eq!(
            serde_json::from_str::<TaxSummaryRow>(GOLDEN_SUMMARY_JSON).unwrap(),
            synthetic_summary()
        );

        assert_eq!(
            serde_json::to_string(&synthetic_rule()).unwrap(),
            GOLDEN_RULE_JSON
        );
        assert_eq!(
            serde_json::from_str::<TaxRateRule>(GOLDEN_RULE_JSON).unwrap(),
            synthetic_rule()
        );

        assert_eq!(
            serde_json::to_string(&PriceMode::Exclusive).unwrap(),
            r#""Exclusive""#
        );
    }

    // ── resolve_components (1.3.2) ──────────────────────────────────────
    // Every code and rate below is synthetic, as above: these test the
    // resolution rules, not any jurisdiction's schedule.

    fn at(ms: i64) -> Timestamp {
        Timestamp::from_epoch_milliseconds(ms).unwrap()
    }

    fn category() -> TaxCategoryId {
        TaxCategoryId::from_uuid(Uuid::from_u128(0x0C47))
    }

    /// A synthetic percentage rule for `code`, in effect over `[from, to)`.
    fn rule(
        code: &str,
        ppm: i64,
        from: i64,
        to: Option<i64>,
        scope: Option<StoreTaxProfile>,
    ) -> TaxRateRule {
        TaxRateRule {
            tax_category_id: category(),
            component_code: code.to_owned(),
            treatment: TaxTreatment::Standard,
            basis: TaxBasis::Percentage {
                rate: Percent::from_ppm(ppm),
            },
            sequence: 0,
            base: TaxBase::LineNet,
            valid_from: at(from),
            valid_to: to.map(at),
            profile_scope: scope,
        }
    }

    /// Each resolved component's code and the rate it charges, which is how
    /// these tests tell one synthetic rule from another.
    fn rates(
        resolved: Result<Vec<TaxComponent>, TaxError>,
    ) -> Result<Vec<(String, i64)>, TaxError> {
        resolved.map(|components| {
            components
                .into_iter()
                .map(|c| {
                    let ppm = match c.basis {
                        TaxBasis::Percentage { rate } => rate.ppm(),
                        TaxBasis::PerUnit { .. } | TaxBasis::Compound { .. } => -1,
                    };
                    (c.code, ppm)
                })
                .collect()
        })
    }

    fn charges(code: &str, ppm: i64) -> Result<Vec<(String, i64)>, TaxError> {
        Ok(vec![(code.to_owned(), ppm)])
    }

    #[test]
    fn overlapping_rules_error() {
        let standard = |rules: &[TaxRateRule], ms| {
            resolve_components(category(), rules, StoreTaxProfile::Standard, at(ms))
        };

        // Two unscoped rules for one component, both in effect from 1500 to 2000.
        let overlapping = [
            rule("SYNTH_A", 111_000, 1_000, Some(2_000), None),
            rule("SYNTH_A", 222_000, 1_500, None, None),
        ];
        assert_eq!(
            standard(&overlapping, 1_500),
            Err(TaxError::OverlappingRules("SYNTH_A".to_owned()))
        );
        // Either one alone, outside the overlap.
        assert_eq!(
            rates(standard(&overlapping, 1_499)),
            charges("SYNTH_A", 111_000)
        );
        assert_eq!(
            rates(standard(&overlapping, 2_000)),
            charges("SYNTH_A", 222_000)
        );

        // Two rules scoped to the same profile overlap just the same.
        let scoped = [
            rule("SYNTH_A", 111_000, 0, None, Some(StoreTaxProfile::Asez)),
            rule("SYNTH_A", 222_000, 0, None, Some(StoreTaxProfile::Asez)),
        ];
        assert_eq!(
            resolve_components(category(), &scoped, StoreTaxProfile::Asez, at(10)),
            Err(TaxError::OverlappingRules("SYNTH_A".to_owned()))
        );

        // Rules that only touch, one ending where the next begins, do not.
        let touching = [
            rule("SYNTH_A", 111_000, 0, Some(1_000), None),
            rule("SYNTH_A", 222_000, 1_000, None, None),
        ];
        assert_eq!(
            rates(standard(&touching, 1_000)),
            charges("SYNTH_A", 222_000)
        );

        // Nor do two different components in effect at once.
        let two = [
            rule("SYNTH_A", 111_000, 0, None, None),
            rule("SYNTH_B", 222_000, 0, None, None),
        ];
        assert_eq!(
            rates(standard(&two, 10)),
            Ok(vec![
                ("SYNTH_A".to_owned(), 111_000),
                ("SYNTH_B".to_owned(), 222_000)
            ])
        );
    }

    #[test]
    fn no_rule_in_effect_errors() {
        let rules = [rule("SYNTH_A", 111_000, 1_000, Some(2_000), None)];
        let standard =
            |ms| resolve_components(category(), &rules, StoreTaxProfile::Standard, at(ms));

        // Before the rule begins, and at the instant it ends — named as the
        // instant an operator can read, not a count of milliseconds.
        assert_eq!(standard(999), Err(TaxError::NoRuleInEffect(at(999))));
        assert_eq!(
            standard(999).unwrap_err().to_string(),
            "no rate rule for category at 1970-01-01T00:00:00.999Z"
        );
        assert_eq!(standard(2_000), Err(TaxError::NoRuleInEffect(at(2_000))));

        // Another category's rule is not this category's.
        let other = TaxCategoryId::from_uuid(Uuid::from_u128(0x0BAD));
        assert_eq!(
            resolve_components(other, &rules, StoreTaxProfile::Standard, at(1_500)),
            Err(TaxError::NoRuleInEffect(at(1_500)))
        );

        // An unscoped rule is the standard profile's, and no other profile
        // inherits it.
        for profile in [
            StoreTaxProfile::Asez,
            StoreTaxProfile::DevelopmentArea,
            StoreTaxProfile::Unregistered,
        ] {
            assert_eq!(
                resolve_components(category(), &rules, profile, at(1_500)),
                Err(TaxError::NoRuleInEffect(at(1_500))),
                "{profile:?}"
            );
        }

        // And no rules at all.
        assert_eq!(
            resolve_components(category(), &[], StoreTaxProfile::Standard, at(1_500)),
            Err(TaxError::NoRuleInEffect(at(1_500)))
        );
    }

    #[test]
    fn scoped_rule_overrides_unscoped() {
        let rules = [
            rule("SYNTH_A", 111_000, 0, None, None),
            rule("SYNTH_A", 222_000, 0, None, Some(StoreTaxProfile::Standard)),
            rule("SYNTH_A", 333_000, 0, None, Some(StoreTaxProfile::Asez)),
        ];
        let resolve = |profile| rates(resolve_components(category(), &rules, profile, at(10)));
        assert_eq!(
            resolve(StoreTaxProfile::Standard),
            charges("SYNTH_A", 222_000)
        );
        assert_eq!(resolve(StoreTaxProfile::Asez), charges("SYNTH_A", 333_000));
        assert_eq!(
            resolve(StoreTaxProfile::DevelopmentArea),
            Err(TaxError::NoRuleInEffect(at(10)))
        );

        // The override is per component: an unscoped component with no scoped
        // rival still applies beside it.
        let mixed = [
            rule("SYNTH_A", 111_000, 0, None, None),
            rule("SYNTH_A", 222_000, 0, None, Some(StoreTaxProfile::Standard)),
            rule("SYNTH_B", 444_000, 0, None, None),
        ];
        assert_eq!(
            rates(resolve_components(
                category(),
                &mixed,
                StoreTaxProfile::Standard,
                at(10)
            )),
            Ok(vec![
                ("SYNTH_A".to_owned(), 222_000),
                ("SYNTH_B".to_owned(), 444_000)
            ])
        );

        // A scoped rule that has ended overrides nothing.
        let ended = [
            rule("SYNTH_A", 111_000, 0, None, None),
            rule(
                "SYNTH_A",
                222_000,
                0,
                Some(5),
                Some(StoreTaxProfile::Standard),
            ),
        ];
        assert_eq!(
            rates(resolve_components(
                category(),
                &ended,
                StoreTaxProfile::Standard,
                at(10)
            )),
            charges("SYNTH_A", 111_000)
        );
    }

    #[test]
    fn resolved_components_come_in_application_order() {
        // The code that sorts first alphabetically is applied last, so only an
        // order by `sequence` passes, not one by code or by the pack's listing.
        let fixed = TaxRateRule {
            basis: TaxBasis::PerUnit {
                amount: Money::from_minor(50, Currency::EUR),
            },
            ..rule("SYNTH_FIXED", 0, 0, None, None)
        };
        let on_top = TaxRateRule {
            sequence: 1,
            base: TaxBase::NetPlusComponents {
                codes: vec!["SYNTH_FIXED".to_owned()],
            },
            ..rule("SYNTH_A_ON_TOP", 123_456, 0, None, None)
        };
        let plain = rule("SYNTH_Z_PLAIN", 7_000, 0, None, None);

        // Within one sequence, by code. Only the plain percentage of the line
        // net can come back out of an inclusive price.
        let resolved = resolve_components(
            category(),
            &[on_top, plain, fixed],
            StoreTaxProfile::Standard,
            at(10),
        )
        .unwrap();
        assert_eq!(
            resolved
                .iter()
                .map(|c| (c.code.as_str(), c.sequence, c.is_inclusive_capable))
                .collect::<Vec<_>>(),
            [
                ("SYNTH_FIXED", 0, false),
                ("SYNTH_Z_PLAIN", 0, true),
                ("SYNTH_A_ON_TOP", 1, false)
            ]
        );
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// A rate changes at an instant, and at that instant exactly one rule
        /// answers: the old rule up to the millisecond before, the new one from
        /// the millisecond itself, and nothing before the first or after the
        /// last — whatever order the pack happens to list its rules in.
        #[test]
        fn prop_rate_resolution_is_deterministic_at_boundaries(
            starts in prop::collection::btree_set(-1_000_000_000i64..1_000_000_000, 2..8),
            open_ended in any::<bool>(),
            rotation in 0usize..8,
        ) {
            let starts: Vec<i64> = starts.into_iter().collect();
            let first = *starts.first().unwrap();
            let end = (!open_ended).then(|| starts.last().unwrap() + 1_000);
            // Rule i runs from starts[i] to starts[i + 1], the last one to
            // `end`, and charges a rate that names it.
            let ends = starts.iter().skip(1).map(|&s| Some(s)).chain(std::iter::once(end));
            let rules: Vec<TaxRateRule> = starts
                .iter()
                .zip(ends)
                .enumerate()
                .map(|(i, (&from, to))| rule("SYNTH_A", 1_000 + i as i64, from, to, None))
                .collect();
            let mut reordered = rules.clone();
            reordered.rotate_left(rotation % rules.len());
            let resolve = |rules: &[TaxRateRule], ms: i64| {
                rates(resolve_components(category(), rules, StoreTaxProfile::Standard, at(ms)))
            };

            prop_assert_eq!(resolve(&rules, first - 1), Err(TaxError::NoRuleInEffect(at(first - 1))));
            for (i, &from) in starts.iter().enumerate() {
                let rate = 1_000 + i as i64;
                prop_assert_eq!(resolve(&rules, from), charges("SYNTH_A", rate));
                prop_assert_eq!(resolve(&reordered, from), charges("SYNTH_A", rate));
                if i > 0 {
                    prop_assert_eq!(resolve(&rules, from - 1), charges("SYNTH_A", rate - 1));
                    prop_assert_eq!(resolve(&reordered, from - 1), charges("SYNTH_A", rate - 1));
                }
            }
            if let Some(end) = end {
                let last = 999 + rules.len() as i64;
                prop_assert_eq!(resolve(&rules, end - 1), charges("SYNTH_A", last));
                prop_assert_eq!(resolve(&rules, end), Err(TaxError::NoRuleInEffect(at(end))));
            }
        }
    }
}
