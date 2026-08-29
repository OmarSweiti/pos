//! Tax value types: the facts later calculation steps consume and produce.
//!
//! This microstep deliberately contains no rate resolution or arithmetic. Its
//! job is to keep the inputs that arithmetic will need representable: fixed and
//! ad-valorem bases, persisted component order and named dependencies, and the
//! evidence explaining why one particular supply received its treatment. No
//! value here is a jurisdiction rate, rounding rule, tolerance or default.

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
/// `reason` and `evidence_ref` remain optional because the required mapping is
/// an explicit open question owned by 1.3.2. This types-only step does not turn
/// a provisional evidence rule into a constructor invariant.
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
    #[error("no rate rule for category at {0:?}")]
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::ids::TaxCategoryId;
    use crate::money::{Currency, Money, Percent};
    use crate::time::Timestamp;
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
}
