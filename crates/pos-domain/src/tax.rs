//! The tax engine (group 1.3, `ref/tax-jordan.md`).
//!
//! The value types came first (1.3.1): fixed and ad-valorem bases, persisted
//! component order and named dependencies, and the evidence explaining why one
//! particular supply received its treatment. [`resolve_components`] (1.3.2)
//! chooses which rules apply to a line, [`compute_line_tax`] (1.3.3 on)
//! charges them, and [`summarize_tax`] (1.3.6) adds a receipt's lines up by
//! rate.
//! No value here is a jurisdiction rate, rounding rule, tolerance or default:
//! rates are data, and a rate compiled into code is a release every time the
//! official schedule changes.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::ids::TaxCategoryId;
use crate::money::{Money, MoneyError, Percent, Qty, RoundingRule};
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
/// 6.14) is that only `Domestic` supplies are sold: [`compute_line_tax`]
/// refuses every other destination, and any zero-rating reason.
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

/// One receipt-summary row, grouped by [`summarize_tax`] on all rate-defining
/// facts.
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
    /// What the row's rate was charged on: the sum of its components' bases.
    pub net: Money,
    pub tax: Money,
    /// `net + tax`.
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
    #[error(
        "supply destination {0:?} is refused: only domestic supplies are sold until the \
         accountant names the zero-rated lines"
    )]
    SupplyDestinationRefused(SupplyDestination),
    #[error(
        "zero-rating reason {0:?} is refused: no supply is zero-rated until the accountant \
         names the zero-rated lines"
    )]
    ZeroRatingReasonRefused(ZeroRatingReason),
    #[error("component {0} has a negative rate or amount; a tax is never charged below zero")]
    NegativeRate(String),
    #[error(
        "a line with no tax component is refused: an untaxed line is zero-rated or exempt, \
         never componentless"
    )]
    NoComponents,
    #[error("component {0} is charged on {1}, which is not applied before it")]
    BaseComponentNotEarlier(String, String),
    #[error(
        "a tax-inclusive price on a line with {0} tax components is refused: how such a price \
         divides between them is not settled"
    )]
    InclusiveSplitUndefined(usize),
    #[error("component {0} charges a fixed amount larger than the tax-inclusive price")]
    FixedTaxExceedsPrice(String),
    #[error(
        "component {0} is zero-rated or exempt but charges a rate or an amount; an untaxed \
         treatment never carries tax"
    )]
    UntaxedTreatmentCharges(String),
    #[error(
        "the line at position {0} does not add up, its components to its tax or its net and tax \
         to its gross; a summary of it would disagree with the receipt"
    )]
    LineTaxUnbalanced(usize),
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
/// * A zone store, `Asez` or `DevelopmentArea`, whose own pack has no rule for
///   a component the standard pack charges on this category is refused as an
///   incomplete pack: selling it under the components its pack does have
///   would leave one out.
/// * A line with no component in effect is refused too. The engine never
///   guesses a rate.
///
/// Components come back in application order — `sequence`, then code —
/// whatever order `rules` arrived in.
///
/// `is_inclusive_capable` is derived, because a rule carries no such field:
/// a component on the line net can be taken back out of an inclusive price,
/// whether it is a percentage, a fixed amount per unit or both, and a tax on
/// another tax cannot, because it never stands on a line alone.
///
/// An `Unregistered` store resolves only rules scoped to it. Whether it charges
/// any GST at all is 1.3.8's short-circuit, which comes before resolution.
///
/// # Errors
///
/// [`TaxError::OverlappingRules`] naming the component with more than one rule
/// in effect, [`TaxError::ProfilePackIncomplete`] for a zone pack missing a
/// component, and [`TaxError::NoRuleInEffect`] when no component is in effect.
pub fn resolve_components(
    category: TaxCategoryId,
    rules: &[TaxRateRule],
    profile: StoreTaxProfile,
    at: Timestamp,
) -> Result<Vec<TaxComponent>, TaxError> {
    let zone = matches!(
        profile,
        StoreTaxProfile::Asez | StoreTaxProfile::DevelopmentArea
    );
    // For each component, the rules in effect that apply to this profile:
    // those scoped to it, and the unscoped ones a `Standard` store reads. A
    // zone store also notes which components the standard pack charges.
    let mut tiers: BTreeMap<&str, (Vec<&TaxRateRule>, Vec<&TaxRateRule>)> = BTreeMap::new();
    let mut standard_charges: Vec<&str> = Vec::new();
    for rule in rules {
        if rule.tax_category_id != category || !in_effect(rule, at) {
            continue;
        }
        let code = rule.component_code.as_str();
        let (scoped, unscoped) = tiers.entry(code).or_default();
        match rule.profile_scope {
            Some(scope) if scope == profile => scoped.push(rule),
            None if profile == StoreTaxProfile::Standard => unscoped.push(rule),
            Some(_) | None => {}
        }
        if zone && matches!(rule.profile_scope, None | Some(StoreTaxProfile::Standard)) {
            standard_charges.push(code);
        }
    }

    let mut components = Vec::with_capacity(tiers.len());
    for (code, (scoped, unscoped)) in tiers {
        let tier = if scoped.is_empty() { unscoped } else { scoped };
        let rule = match tier.as_slice() {
            [] if standard_charges.contains(&code) => {
                return Err(TaxError::ProfilePackIncomplete(profile));
            }
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
            is_inclusive_capable: rule.base == TaxBase::LineNet,
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

/// The tax on one line (`ref/tax-jordan.md` §4).
///
/// `taxable` is the line's amount after discount: its net when `mode` is
/// exclusive, its gross when inclusive. `qty` is what a fixed amount per unit
/// is multiplied by. The arithmetic is exact in `rust_decimal`, and every
/// amount the line carries is rounded once, by `rule`:
///
/// * **Exclusive** (1.3.3, 1.3.5): the components are applied in application
///   order — `sequence`, then code — whatever order they arrive in. Each is
///   charged on the base its [`TaxBase`] names: the net, or the net plus the
///   components it names, which must be applied before it. Its exact charge,
///   the rate times that base plus the fixed amount times the quantity, is
///   rounded once. A base is built from amounts already rounded, because that
///   is what the line carries, so every component re-derives from its own
///   persisted base. The tax is the sum of the components and the gross is net
///   plus tax.
/// * **Inclusive** (1.3.4, 1.3.5): one component, on the line net. The net is
///   the price less the fixed amount, divided by one plus the rate, rounded
///   once; the tax is `gross − net`, a residual that is never rounded on its
///   own, so net and tax always add back to the price the customer saw.
///   Several components are refused: how a price that includes them all
///   divides between them, a fil at a time, is the `⚠️ OPEN` item in
///   `ref/domain-api.md` §5, and no source settles it yet.
///
/// Each component carries the base it was charged on, so a report never
/// re-derives it.
///
/// This is the first code to read a [`SupplyTaxContext`], and it enforces the
/// owner's decision of 26 October 2026 (`ref/merchant-decisions.md` 6.14):
/// until the accountant names the return's zero-rated lines, only an ordinary
/// domestic supply is sold. Any other destination is refused, whatever reason
/// or evidence it carries. A domestic supply that claims a zero-rating reason
/// is refused too, so no reason reaches a line yet.
///
/// **A store not registered for GST charges none** (1.3.8, `ref/tax-jordan.md`
/// §2). Once the supply is read, its line is returned untaxed whatever it is
/// handed: no component, no tax, and a net and gross that are both the price,
/// in either price mode. The components are not read at all, because no rate
/// applies to a seller who may not charge one. That decides the GST arithmetic
/// only: whether the store must issue an electronic invoice is a separate,
/// evidenced setting, and an unregistered store may still owe one.
///
/// # Errors
///
/// In the order they are checked:
/// [`TaxError::SupplyDestinationRefused`] and
/// [`TaxError::ZeroRatingReasonRefused`] for a supply that is not ordinary
/// domestic retail, for every store; then, for a registered store, the
/// component list — [`TaxError::NoComponents`],
/// [`TaxError::OverlappingRules`] for a code that appears twice,
/// [`TaxError::NegativeRate`] for a rate or fixed amount below zero,
/// [`TaxError::Money`] for a fixed amount in another currency,
/// [`TaxError::UntaxedTreatmentCharges`] for a zero-rated or exempt component
/// that charges a rate or an amount,
/// [`TaxError::UnknownBaseComponent`], [`TaxError::CircularComponentBase`] and
/// [`TaxError::BaseComponentNotEarlier`]; then, for an inclusive price,
/// [`TaxError::InclusiveSplitUndefined`], [`TaxError::NotInclusiveCapable`] and
/// [`TaxError::FixedTaxExceedsPrice`]; and [`TaxError::Money`] if an amount
/// leaves the `i64` range.
pub fn compute_line_tax(
    taxable: Money,
    qty: Qty,
    mode: PriceMode,
    components: &[TaxComponent],
    profile: StoreTaxProfile,
    supply: &SupplyTaxContext,
    rule: RoundingRule,
) -> Result<LineTax, TaxError> {
    if supply.destination != SupplyDestination::Domestic {
        return Err(TaxError::SupplyDestinationRefused(supply.destination));
    }
    if let Some(reason) = supply.reason {
        return Err(TaxError::ZeroRatingReasonRefused(reason));
    }
    if profile == StoreTaxProfile::Unregistered {
        return Ok(LineTax {
            net: taxable,
            components: Vec::new(),
            tax_total: Money::zero(taxable.currency()),
            gross: taxable,
            supply_reason: None,
        });
    }
    let ordered = application_order(components, taxable)?;
    match mode {
        PriceMode::Exclusive => exclusive_line(taxable, qty, &ordered, rule),
        PriceMode::Inclusive => inclusive_line(taxable, qty, &ordered, rule),
    }
}

/// The components in application order, once the list is known to be one the
/// engine can charge: not empty, no code twice, no rate or fixed amount below
/// zero, every fixed amount in the line's currency, no zero-rated or exempt
/// component charging anything, and every named base a component applied
/// earlier.
///
/// A dependency on a component that is not applied earlier is a cycle when
/// that component depends back, directly or not, and is refused as one;
/// otherwise it contradicts the order, and is refused as that. Every cycle has
/// such an edge, because a component may only be charged on one with a lower
/// `sequence`.
fn application_order(
    components: &[TaxComponent],
    taxable: Money,
) -> Result<Vec<&TaxComponent>, TaxError> {
    if components.is_empty() {
        return Err(TaxError::NoComponents);
    }
    let mut by_code: BTreeMap<&str, &TaxComponent> = BTreeMap::new();
    for component in components {
        if by_code.insert(&component.code, component).is_some() {
            return Err(TaxError::OverlappingRules(component.code.clone()));
        }
        let (rate, fixed) = charge_parts(&component.basis);
        if rate < Percent::ZERO || fixed.is_some_and(Money::is_negative) {
            return Err(TaxError::NegativeRate(component.code.clone()));
        }
        if let Some(fixed) = fixed
            && fixed.currency() != taxable.currency()
        {
            return Err(MoneyError::CurrencyMismatch(
                taxable.currency().code(),
                fixed.currency().code(),
            )
            .into());
        }
        // A zero-rated or exempt component that charges would report a tax in
        // a box that carries none.
        let untaxed = matches!(
            component.treatment,
            TaxTreatment::Zero | TaxTreatment::Exempt
        );
        if untaxed && (rate != Percent::ZERO || fixed.is_some_and(|f| !f.is_zero())) {
            return Err(TaxError::UntaxedTreatmentCharges(component.code.clone()));
        }
    }
    let mut ordered: Vec<&TaxComponent> = by_code.into_values().collect();
    ordered.sort_by_key(|c| c.sequence);

    for component in &ordered {
        for named in named_bases(component) {
            if !ordered.iter().any(|c| c.code == *named) {
                return Err(TaxError::UnknownBaseComponent(
                    component.code.clone(),
                    named.clone(),
                ));
            }
        }
    }
    for component in &ordered {
        for named in named_bases(component) {
            let Some(base) = ordered.iter().find(|c| c.code == *named) else {
                continue;
            };
            if base.sequence < component.sequence {
                continue;
            }
            return Err(if depends_on(&ordered, base, &component.code) {
                TaxError::CircularComponentBase(component.code.clone(), named.clone())
            } else {
                TaxError::BaseComponentNotEarlier(component.code.clone(), named.clone())
            });
        }
    }
    Ok(ordered)
}

/// The codes a component's base names, if it names any.
fn named_bases(component: &TaxComponent) -> &[String] {
    match &component.base {
        TaxBase::LineNet => &[],
        TaxBase::NetPlusComponents { codes } => codes,
    }
}

/// Whether `from` is charged on `target`, directly or through the components
/// it is charged on. A component reaches itself.
fn depends_on(ordered: &[&TaxComponent], from: &TaxComponent, target: &str) -> bool {
    let mut seen: Vec<&str> = Vec::new();
    let mut pending = vec![from];
    while let Some(component) = pending.pop() {
        if component.code == target {
            return true;
        }
        if seen.contains(&component.code.as_str()) {
            continue;
        }
        seen.push(&component.code);
        pending.extend(
            named_bases(component)
                .iter()
                .filter_map(|named| ordered.iter().find(|c| c.code == *named).copied()),
        );
    }
    false
}

/// A basis as the rate it charges on a base and the fixed amount it charges
/// per unit: a percentage has no fixed amount, and a fixed amount alone
/// charges a rate of zero.
fn charge_parts(basis: &TaxBasis) -> (Percent, Option<Money>) {
    match *basis {
        TaxBasis::Percentage { rate } => (rate, None),
        TaxBasis::PerUnit { amount } => (Percent::ZERO, Some(amount)),
        TaxBasis::Compound { rate, per_unit } => (rate, Some(per_unit)),
    }
}

/// `amount × qty`, exact, in fils: what a fixed amount per unit charges on a
/// line, before any rounding.
fn fixed_charge(amount: Option<Money>, qty: Qty) -> Result<Decimal, TaxError> {
    let Some(amount) = amount else {
        return Ok(Decimal::ZERO);
    };
    within_i64(Decimal::from(amount.minor()).checked_mul(qty.to_decimal()))
}

/// An exact intermediate that no `i64` amount could hold is refused, never
/// carried on into a sum where its lost digits would move a fil.
fn within_i64(value: Option<Decimal>) -> Result<Decimal, TaxError> {
    match value {
        Some(value) if value.abs() <= Decimal::from(i64::MAX) => Ok(value),
        _ => Err(MoneyError::Overflow.into()),
    }
}

fn exclusive_line(
    net: Money,
    qty: Qty,
    ordered: &[&TaxComponent],
    rule: RoundingRule,
) -> Result<LineTax, TaxError> {
    let mut charged: Vec<ComponentTax> = Vec::with_capacity(ordered.len());
    for component in ordered {
        // Named bases were applied earlier, so each is already in `charged`,
        // once, however many times the base lists it.
        let mut base = net;
        for earlier in &charged {
            if named_bases(component).contains(&earlier.code) {
                base = base.checked_add(earlier.amount)?;
            }
        }
        let (rate, fixed) = charge_parts(&component.basis);
        let on_base = within_i64(Decimal::from(base.minor()).checked_mul(rate.to_decimal()))?;
        let exact = on_base + fixed_charge(fixed, qty)?;
        charged.push(ComponentTax {
            code: component.code.clone(),
            treatment: component.treatment,
            basis: component.basis.clone(),
            base_amount: base,
            amount: Money::from_minor(rule.round_to_i64(exact)?, net.currency()),
        });
    }
    let tax_total = Money::sum(charged.iter().map(|c| c.amount), net.currency())?;
    Ok(LineTax {
        net,
        components: charged,
        tax_total,
        gross: net.checked_add(tax_total)?,
        // A reason was refused before anything was charged.
        supply_reason: None,
    })
}

fn inclusive_line(
    gross: Money,
    qty: Qty,
    ordered: &[&TaxComponent],
    rule: RoundingRule,
) -> Result<LineTax, TaxError> {
    let [component] = ordered else {
        return Err(TaxError::InclusiveSplitUndefined(ordered.len()));
    };
    if !component.is_inclusive_capable {
        return Err(TaxError::NotInclusiveCapable);
    }
    let (rate, fixed) = charge_parts(&component.basis);
    let net = extract_net(gross, fixed_charge(fixed, qty)?, rate, rule)?;
    // A net of the opposite sign, or any net in a price of zero, would carry
    // more tax than the customer pays.
    if net.minor() != 0 && net.minor().signum() != gross.minor().signum() {
        return Err(TaxError::FixedTaxExceedsPrice(component.code.clone()));
    }
    let tax = gross.checked_sub(net)?;
    Ok(LineTax {
        net,
        components: vec![ComponentTax {
            code: component.code.clone(),
            treatment: component.treatment,
            basis: component.basis.clone(),
            base_amount: net,
            amount: tax,
        }],
        tax_total: tax,
        gross,
        // A reason was refused before anything was charged.
        supply_reason: None,
    })
}

/// `(gross − fixed) / (1 + rate)`, rounded once by `rule`: the net inside an
/// inclusive price whose one component charges `rate` on the net and `fixed`,
/// exact in fils, on the quantity.
///
/// `fixed` is a whole number of thousandths of a fil, so the exact quotient is
/// an integer `M` over `10⁶ + ppm`, and the price less the fixed amount is
/// held to the `i64` range like any amount. When the quotient lies on a
/// rounding boundary it terminates, and `rust_decimal` computes it exactly.
/// When it does not, its distance from a boundary is a whole number of
/// `1 / (2 × (10⁶ + ppm))` fils, and the 28 significant digits leave an error
/// of the order of `10⁻⁹ / (1 + rate)` fils, hundreds of times smaller. So the
/// one rounding lands where exact rational arithmetic puts it, which
/// `inclusive_extraction_is_exact_over_the_whole_fils_range` and
/// `prop_inclusive_net_plus_tax_equals_gross` check against integer division.
fn extract_net(
    gross: Money,
    fixed: Decimal,
    rate: Percent,
    rule: RoundingRule,
) -> Result<Money, TaxError> {
    let priced = within_i64(Decimal::from(gross.minor()).checked_sub(fixed))?;
    let divisor = Decimal::ONE
        .checked_add(rate.to_decimal())
        .ok_or(MoneyError::Overflow)?;
    let exact_net = priced.checked_div(divisor).ok_or(MoneyError::Overflow)?;
    Ok(Money::from_minor(
        rule.round_to_i64(exact_net)?,
        gross.currency(),
    ))
}

/// A receipt's tax summary: one row per rate its lines were charged at, each
/// the exact sum of the carried amounts it groups (`ref/tax-jordan.md` §4).
///
/// A row is keyed by everything that makes a rate a distinct rate on the
/// filing report: the component's code, its treatment, its percentage, its
/// fixed amount per unit, and the line's zero-rating reason. So an exempt line
/// and a zero-rated one never share a row although neither carries tax, and
/// one percentage beside two fixed amounts is two rows.
///
/// A row's `tax` is the sum of its components' carried amounts, its `net` the
/// sum of the bases they were charged on, and its `gross` the two together.
/// Nothing is re-derived from a rate or from the receipt total. On a line with
/// one component the base is the line's net, so the rows add up to the
/// receipt: their nets to its net, their taxes to its tax and their grosses to
/// its gross. A line with several components counts its net under each of
/// them, and a tax charged on another tax has that tax in its base, because a
/// row states what its own rate was charged on; across such a receipt only the
/// taxes add up.
///
/// Rows come back ordered by code, then treatment, percentage, fixed amount
/// and reason, so the order the lines were scanned in never moves a row.
///
/// A store not registered for GST carries lines with no component (1.3.8).
/// Its receipt has no tax rows at all: there is no rate to report.
///
/// # Errors
///
/// [`TaxError::NoComponents`] for a line with no component on a receipt whose
/// other lines carry one: a store is registered for GST or it is not, and on a
/// registered store's receipt an untaxed line is zero-rated or exempt, never
/// componentless. [`TaxError::LineTaxUnbalanced`] for a
/// line whose components do not add up to its tax, or whose net and tax do not
/// add up to its gross: its summary would disagree with the receipt's total.
/// [`TaxError::Money`] for lines or amounts in more than one currency, and for
/// a sum no amount can hold.
pub fn summarize_tax(lines: &[LineTax]) -> Result<Vec<TaxSummaryRow>, TaxError> {
    let Some(first) = lines.first() else {
        return Ok(Vec::new());
    };
    let currency = first.net.currency();
    let registered = lines.iter().any(|line| !line.components.is_empty());
    let mut rows: BTreeMap<SummaryKey, TaxSummaryRow> = BTreeMap::new();
    for (position, line) in lines.iter().enumerate() {
        // Beside taxed lines its net would be on no row at all. On a receipt
        // of such lines, none is, and the balance below holds each one to
        // carrying no tax.
        if registered && line.components.is_empty() {
            return Err(TaxError::NoComponents);
        }
        // Summed in the first line's currency, and then held to the line's own
        // net and gross, so a line in another currency stops here.
        let charged = Money::sum(line.components.iter().map(|c| c.amount), currency)?;
        if charged != line.tax_total || line.net.checked_add(line.tax_total)? != line.gross {
            return Err(TaxError::LineTaxUnbalanced(position));
        }
        for component in &line.components {
            let (rate, per_unit) = charge_parts(&component.basis);
            if let Some(fixed) = per_unit
                && fixed.currency() != currency
            {
                return Err(
                    MoneyError::CurrencyMismatch(currency.code(), fixed.currency().code()).into(),
                );
            }
            let gross = component.base_amount.checked_add(component.amount)?;
            let key = SummaryKey {
                code: component.code.clone(),
                treatment: treatment_rank(component.treatment),
                rate,
                per_unit: per_unit.map(Money::minor),
                reason: line.supply_reason.map(reason_rank),
            };
            match rows.entry(key) {
                Entry::Vacant(row) => {
                    row.insert(TaxSummaryRow {
                        code: component.code.clone(),
                        treatment: component.treatment,
                        rate,
                        per_unit,
                        reason: line.supply_reason,
                        net: component.base_amount,
                        tax: component.amount,
                        gross,
                    });
                }
                Entry::Occupied(mut row) => {
                    let row = row.get_mut();
                    row.net = row.net.checked_add(component.base_amount)?;
                    row.tax = row.tax.checked_add(component.amount)?;
                    row.gross = row.gross.checked_add(gross)?;
                }
            }
        }
    }
    Ok(rows.into_values().collect())
}

/// What makes two charged components one summary row, in the order rows are
/// printed. The treatment and reason are ranked by their declaration order.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct SummaryKey {
    code: String,
    treatment: u8,
    rate: Percent,
    per_unit: Option<i64>,
    reason: Option<u8>,
}

fn treatment_rank(treatment: TaxTreatment) -> u8 {
    match treatment {
        TaxTreatment::Standard => 0,
        TaxTreatment::Reduced => 1,
        TaxTreatment::Zero => 2,
        TaxTreatment::Exempt => 3,
    }
}

fn reason_rank(reason: ZeroRatingReason) -> u8 {
    match reason {
        ZeroRatingReason::Export => 0,
        ZeroRatingReason::FreeZoneSupply => 1,
        ZeroRatingReason::EligibleEntity => 2,
        ZeroRatingReason::ProductCategory => 3,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::ids::TaxCategoryId;
    use crate::money::{Currency, Money, MoneyError, Percent, Qty, RoundingRule};
    use crate::time::Timestamp;
    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;
    use rust_decimal::Decimal;
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
        // inherits it: a zone store's pack is incomplete without its own
        // (`an_incomplete_profile_pack_fails_closed`), and an unregistered
        // store has no rule at all.
        for zone in [StoreTaxProfile::Asez, StoreTaxProfile::DevelopmentArea] {
            assert_eq!(
                resolve_components(category(), &rules, zone, at(1_500)),
                Err(TaxError::ProfilePackIncomplete(zone)),
                "{zone:?}"
            );
        }
        assert_eq!(
            resolve_components(category(), &rules, StoreTaxProfile::Unregistered, at(1_500)),
            Err(TaxError::NoRuleInEffect(at(1_500)))
        );

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
            Err(TaxError::ProfilePackIncomplete(
                StoreTaxProfile::DevelopmentArea
            ))
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

        // Within one sequence, by code. A component on the line net can come
        // back out of an inclusive price, fixed or not; a tax on a tax cannot.
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
                ("SYNTH_FIXED", 0, true),
                ("SYNTH_Z_PLAIN", 0, true),
                ("SYNTH_A_ON_TOP", 1, false)
            ]
        );
    }

    /// A run of two to seven consecutive rules for one component: distinct
    /// start instants within about eleven days either side of the epoch,
    /// whether the last rule is open-ended, and how far to rotate the order the
    /// pack lists them in. It excludes a gap between rules, which
    /// `no_rule_in_effect_errors` covers, and an overlap, which
    /// `overlapping_rules_error` does.
    fn consecutive_rate_changes() -> impl Strategy<Value = (Vec<i64>, bool, usize)> {
        (
            prop::collection::btree_set(-1_000_000_000i64..1_000_000_000, 2..8),
            any::<bool>(),
            0usize..8,
        )
            .prop_map(|(starts, open_ended, rotation)| {
                (starts.into_iter().collect(), open_ended, rotation)
            })
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// A rate changes at an instant, and at that instant exactly one rule
        /// answers: the old rule up to the millisecond before, the new one from
        /// the millisecond itself, and nothing before the first or after the
        /// last — whatever order the pack happens to list its rules in.
        #[test]
        fn prop_rate_resolution_is_deterministic_at_boundaries(
            (starts, open_ended, rotation) in consecutive_rate_changes(),
        ) {
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

    // ── compute_line_tax (1.3.3) ────────────────────────────────────────
    // 16% below is an arithmetic vector, the rate of `ref/tax-jordan.md` §4's
    // worked example, and not a seeded rate: the real pack is 1.3.7's, and
    // the component codes stay synthetic.

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, Currency::JOD)
    }

    fn percentage(code: &str, ppm: i64) -> TaxComponent {
        TaxComponent {
            code: code.to_owned(),
            treatment: TaxTreatment::Standard,
            basis: TaxBasis::Percentage {
                rate: Percent::from_ppm(ppm),
            },
            sequence: 0,
            base: TaxBase::LineNet,
            is_inclusive_capable: true,
        }
    }

    fn domestic() -> SupplyTaxContext {
        SupplyTaxContext {
            destination: SupplyDestination::Domestic,
            reason: None,
            evidence_ref: None,
        }
    }

    /// An exclusive line of one unit, in a standard store, sold domestically.
    fn exclusive(
        net: i64,
        components: &[TaxComponent],
        rule: RoundingRule,
    ) -> Result<LineTax, TaxError> {
        compute_line_tax(
            jod(net),
            Qty::ONE,
            PriceMode::Exclusive,
            components,
            StoreTaxProfile::Standard,
            &domestic(),
            rule,
        )
    }

    #[test]
    fn exclusive_16pct_adds_exactly() {
        let sixteen = [percentage("SYNTH_A", 160_000)];
        // (net, tax, gross) in fils. 1078 is §4's worked example run forwards:
        // 1078 × 0.16 = 172.48 rounds to 172, and 1078 + 172 is the 1250 the
        // inclusive extraction starts from.
        let cases = [
            (1_000, 160, 1_160),
            (1_078, 172, 1_250),
            (3, 0, 3), // 0.48 rounds down
            (4, 1, 5), // 0.64 rounds up
            (0, 0, 0),
            (-1_078, -172, -1_250), // a negative net mirrors a positive one
        ];
        for (net, tax, gross) in cases {
            let line = exclusive(net, &sixteen, RoundingRule::HalfAwayFromZero).unwrap();
            assert_eq!(
                line,
                LineTax {
                    net: jod(net),
                    components: vec![ComponentTax {
                        code: "SYNTH_A".to_owned(),
                        treatment: TaxTreatment::Standard,
                        basis: TaxBasis::Percentage {
                            rate: Percent::from_ppm(160_000),
                        },
                        base_amount: jod(net),
                        amount: jod(tax),
                    }],
                    tax_total: jod(tax),
                    gross: jod(gross),
                    supply_reason: None,
                },
                "net {net}"
            );
        }

        // The caller's rule is the one rounding: 172.48 floors and ceils apart.
        let rounded = |rule| exclusive(1_078, &sixteen, rule).map(|line| line.tax_total);
        assert_eq!(rounded(RoundingRule::Floor), Ok(jod(172)));
        assert_eq!(rounded(RoundingRule::Ceil), Ok(jod(173)));
    }

    #[test]
    fn a_non_domestic_supply_is_refused() {
        let sixteen = [percentage("SYNTH_A", 160_000)];
        let reasons = [
            None,
            Some(ZeroRatingReason::Export),
            Some(ZeroRatingReason::FreeZoneSupply),
            Some(ZeroRatingReason::EligibleEntity),
            Some(ZeroRatingReason::ProductCategory),
        ];
        let profiles = [
            StoreTaxProfile::Standard,
            StoreTaxProfile::Asez,
            StoreTaxProfile::DevelopmentArea,
            StoreTaxProfile::Unregistered,
        ];
        // Whatever reason or evidence it carries, in any store and either mode:
        // the refusal comes before anything else is read.
        for destination in [
            SupplyDestination::Export,
            SupplyDestination::FreeZone,
            SupplyDestination::DevelopmentArea,
            SupplyDestination::EligibleBody,
        ] {
            for reason in reasons {
                for evidence_ref in [None, Some("SYNTH-DECLARATION-1".to_owned())] {
                    let supply = SupplyTaxContext {
                        destination,
                        reason,
                        evidence_ref,
                    };
                    for profile in profiles {
                        for mode in [PriceMode::Exclusive, PriceMode::Inclusive] {
                            assert_eq!(
                                compute_line_tax(
                                    jod(1_000),
                                    Qty::ONE,
                                    mode,
                                    &sixteen,
                                    profile,
                                    &supply,
                                    RoundingRule::HalfAwayFromZero,
                                ),
                                Err(TaxError::SupplyDestinationRefused(destination)),
                                "{supply:?} in a {profile:?} store, {mode:?}"
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(
            TaxError::SupplyDestinationRefused(SupplyDestination::Export).to_string(),
            "supply destination Export is refused: only domestic supplies are sold until the \
             accountant names the zero-rated lines"
        );
    }

    #[test]
    fn a_domestic_supply_claiming_zero_rating_is_refused() {
        let sixteen = [percentage("SYNTH_A", 160_000)];
        for reason in [
            ZeroRatingReason::Export,
            ZeroRatingReason::FreeZoneSupply,
            ZeroRatingReason::EligibleEntity,
            ZeroRatingReason::ProductCategory,
        ] {
            let supply = SupplyTaxContext {
                destination: SupplyDestination::Domestic,
                reason: Some(reason),
                evidence_ref: None,
            };
            assert_eq!(
                compute_line_tax(
                    jod(1_000),
                    Qty::ONE,
                    PriceMode::Exclusive,
                    &sixteen,
                    StoreTaxProfile::Standard,
                    &supply,
                    RoundingRule::HalfAwayFromZero,
                ),
                Err(TaxError::ZeroRatingReasonRefused(reason))
            );
        }
        assert_eq!(
            TaxError::ZeroRatingReasonRefused(ZeroRatingReason::ProductCategory).to_string(),
            "zero-rating reason ProductCategory is refused: no supply is zero-rated until the \
             accountant names the zero-rated lines"
        );
    }

    #[test]
    fn an_unregistered_store_charges_no_gst() {
        let sixteen = [percentage("SYNTH_A", 160_000)];
        let compute = |mode, profile| {
            compute_line_tax(
                jod(1_000),
                Qty::ONE,
                mode,
                &sixteen,
                profile,
                &domestic(),
                RoundingRule::HalfAwayFromZero,
            )
        };

        // Handed a 16% component, a store not registered for GST still
        // charges nothing, whichever way the price is written: the price is
        // the net and the gross, and no component is carried.
        for mode in [PriceMode::Exclusive, PriceMode::Inclusive] {
            assert_eq!(
                compute(mode, StoreTaxProfile::Unregistered),
                Ok(LineTax {
                    net: jod(1_000),
                    components: Vec::new(),
                    tax_total: jod(0),
                    gross: jod(1_000),
                    supply_reason: None,
                })
            );
        }

        // Every registered profile charges its rule with the same arithmetic.
        for zone in [
            StoreTaxProfile::Standard,
            StoreTaxProfile::Asez,
            StoreTaxProfile::DevelopmentArea,
        ] {
            let tax = |mode| compute(mode, zone).map(|line| line.tax_total);
            assert_eq!(tax(PriceMode::Exclusive), Ok(jod(160)));
            assert_eq!(tax(PriceMode::Inclusive), Ok(jod(138)));
        }
    }

    #[test]
    fn a_negative_rate_is_refused() {
        let rule = RoundingRule::HalfAwayFromZero;
        assert_eq!(
            exclusive(1_000, &[percentage("SYNTH_A", -1)], rule),
            Err(TaxError::NegativeRate("SYNTH_A".to_owned()))
        );
        // Zero is a rate, and charges nothing.
        assert_eq!(
            exclusive(1_000, &[percentage("SYNTH_A", 0)], rule).map(|line| line.gross),
            Ok(jod(1_000))
        );
        assert_eq!(
            TaxError::NegativeRate("SYNTH_A".to_owned()).to_string(),
            "component SYNTH_A has a negative rate or amount; a tax is never charged below zero"
        );
    }

    #[test]
    fn an_overflowing_line_is_refused_not_wrapped() {
        let rule = RoundingRule::HalfAwayFromZero;
        // The tax fits; net plus tax does not.
        assert_eq!(
            exclusive(i64::MAX, &[percentage("SYNTH_A", 160_000)], rule),
            Err(TaxError::Money(MoneyError::Overflow))
        );
        // The tax itself does not fit.
        assert_eq!(
            exclusive(i64::MAX, &[percentage("SYNTH_A", 2_000_000)], rule),
            Err(TaxError::Money(MoneyError::Overflow))
        );
    }

    /// Every rounding rule a computation policy can name. It excludes none.
    fn any_rounding_rule() -> impl Strategy<Value = RoundingRule> {
        prop_oneof![
            Just(RoundingRule::HalfAwayFromZero),
            Just(RoundingRule::HalfEven),
            Just(RoundingRule::Floor),
            Just(RoundingRule::Ceil),
        ]
    }

    /// A line amount in fils, up to a trillion dinars either way, zero and
    /// negatives included. It excludes the far ends of `i64`, where an
    /// exclusive line's gross overflows; that refusal is
    /// `an_overflowing_line_is_refused_not_wrapped`.
    fn line_amounts() -> impl Strategy<Value = i64> {
        -1_000_000_000_000_000i64..=1_000_000_000_000_000
    }

    /// Every amount `i64` can hold. Extraction never overflows, because the net
    /// inside a price is never larger than the price.
    fn any_gross() -> impl Strategy<Value = i64> {
        any::<i64>()
    }

    /// A rate from 0% to 100% in ppm, the range in which an exclusive tax is
    /// never larger than its net. It excludes negative rates, which are refused
    /// (`a_negative_rate_is_refused`).
    fn rates_to_a_hundred_percent() -> impl Strategy<Value = i64> {
        0i64..=1_000_000
    }

    /// A rate from 0% to 1000% in ppm, past any general sales tax and into an
    /// excise's range. It excludes negative rates, which are refused.
    fn rates_to_a_thousand_percent() -> impl Strategy<Value = i64> {
        0i64..=10_000_000
    }

    /// Either way a price can be written.
    fn either_price_mode() -> impl Strategy<Value = PriceMode> {
        prop_oneof![Just(PriceMode::Exclusive), Just(PriceMode::Inclusive)]
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// An exclusive line's tax is its net times the rate, rounded once by
        /// the caller's rule: never a whole fil from the exact product, and no
        /// more than half a fil under a half-way rule. Its gross is exactly net
        /// plus tax. Between 0% and 100% the tax never has the opposite sign to
        /// the net and is never larger than it.
        #[test]
        fn prop_exclusive_tax_rounds_once_and_adds_exactly(
            net in line_amounts(),
            ppm in rates_to_a_hundred_percent(),
            rule in any_rounding_rule(),
        ) {
            let line = exclusive(net, &[percentage("SYNTH_A", ppm)], rule).unwrap();
            let tax = line.tax_total.minor();
            let exact = Decimal::from(net) * Percent::from_ppm(ppm).to_decimal();
            let error = Decimal::from(tax) - exact;
            let within = match rule {
                RoundingRule::Floor => error <= Decimal::ZERO && error > -Decimal::ONE,
                RoundingRule::Ceil => error >= Decimal::ZERO && error < Decimal::ONE,
                RoundingRule::HalfAwayFromZero | RoundingRule::HalfEven => {
                    error.abs() <= Decimal::new(5, 1)
                }
            };
            prop_assert!(within, "{rule:?}: tax {tax} against exact {exact}");
            prop_assert_eq!(line.net, jod(net));
            prop_assert_eq!(line.gross, jod(net + tax));
            let component = line.components.first().unwrap();
            prop_assert_eq!(line.components.len(), 1);
            prop_assert_eq!(component.amount, line.tax_total);
            prop_assert_eq!(component.base_amount, line.net);
            prop_assert!(tax.signum() * net.signum() >= 0);
            prop_assert!(tax.abs() <= net.abs());
        }
    }

    // ── compute_line_tax, inclusive (1.3.4) ─────────────────────────────

    /// An inclusive line of one unit, in a standard store, sold domestically.
    fn inclusive(
        gross: i64,
        components: &[TaxComponent],
        rule: RoundingRule,
    ) -> Result<LineTax, TaxError> {
        compute_line_tax(
            jod(gross),
            Qty::ONE,
            PriceMode::Inclusive,
            components,
            StoreTaxProfile::Standard,
            &domestic(),
            rule,
        )
    }

    /// The net inside `gross` at `ppm`, by integer division alone: the exact
    /// quotient `gross × 10⁶ / (10⁶ + ppm)`, rounded by `rule`. No decimal type
    /// is involved, so it checks the engine rather than restating it.
    fn exact_net(gross: i64, ppm: i64, rule: RoundingRule) -> i64 {
        rounded_ratio(
            i128::from(gross) * 1_000_000,
            1_000_000 + i128::from(ppm),
            rule,
        )
    }

    /// `numerator / denominator`, rounded by `rule` with integer division
    /// alone. The denominator is positive.
    fn rounded_ratio(numerator: i128, denominator: i128, rule: RoundingRule) -> i64 {
        // The quotient is whole + fraction / denominator, the fraction in [0, 1).
        let whole = numerator.div_euclid(denominator);
        let fraction = numerator.rem_euclid(denominator);
        let tie = 2 * fraction == denominator;
        let up = match rule {
            RoundingRule::Floor => false,
            RoundingRule::Ceil => fraction > 0,
            RoundingRule::HalfAwayFromZero if tie => whole >= 0,
            RoundingRule::HalfEven if tie => whole.rem_euclid(2) == 1,
            RoundingRule::HalfAwayFromZero | RoundingRule::HalfEven => 2 * fraction > denominator,
        };
        i64::try_from(whole + i128::from(up)).unwrap()
    }

    #[test]
    fn inclusive_16pct_extracts_exactly() {
        let sixteen = [percentage("SYNTH_A", 160_000)];
        // (gross, net, tax) in fils. 1250 is `ref/tax-jordan.md` §4's worked
        // example: 1250 / 1.16 = 1077.586… rounds to 1078, the tax is the
        // residual 172, and 1078 + 172 is the 1250 on the shelf.
        let cases = [
            (1_250, 1_078, 172),
            (1_160, 1_000, 160),
            // 3.448 rounds to 3, so the tax is 1. Rounded on its own it would be
            // 0.48 → 0, and the line would sum to 3 fils, not the 4 charged.
            (4, 3, 1),
            (1, 1, 0), // 0.862 rounds up, and leaves no tax
            (0, 0, 0),
            (-1_250, -1_078, -172), // a negative gross mirrors a positive one
        ];
        for (gross, net, tax) in cases {
            let line = inclusive(gross, &sixteen, RoundingRule::HalfAwayFromZero).unwrap();
            assert_eq!(
                line,
                LineTax {
                    net: jod(net),
                    components: vec![ComponentTax {
                        code: "SYNTH_A".to_owned(),
                        treatment: TaxTreatment::Standard,
                        basis: TaxBasis::Percentage {
                            rate: Percent::from_ppm(160_000),
                        },
                        base_amount: jod(net),
                        amount: jod(tax),
                    }],
                    tax_total: jod(tax),
                    gross: jod(gross),
                    supply_reason: None,
                },
                "gross {gross}"
            );
        }
    }

    #[test]
    fn inclusive_half_fil_tie_follows_the_rule() {
        // `ref/domain-api.md` §1.2's boundary: 13 fils at 4% inclusive has an
        // exact net of 12.5 fils, and the tie rule decides the tax fact.
        let four = [percentage("SYNTH_A", 40_000)];
        let split = |rule| {
            inclusive(13, &four, rule).map(|line| (line.net.minor(), line.tax_total.minor()))
        };
        assert_eq!(split(RoundingRule::HalfAwayFromZero), Ok((13, 0)));
        assert_eq!(split(RoundingRule::HalfEven), Ok((12, 1)));
        assert_eq!(split(RoundingRule::Floor), Ok((12, 1)));
        assert_eq!(split(RoundingRule::Ceil), Ok((13, 0)));
    }

    #[test]
    fn inclusive_extraction_is_exact_over_the_whole_fils_range() {
        // Arithmetic vectors, not a claim that any band is enabled for a
        // merchant: every gross from one fil to a thousand dinars, at each
        // rate, against integer division. The quotient is the same whatever the
        // rule; what a rule changes is which boundary it must land the right
        // side of. Half-away-from-zero meets every half-fil boundary and floor
        // every whole one (half-even and ceil share them), and the property
        // below runs all four.
        for ppm in [0, 10_000, 20_000, 40_000, 50_000, 100_000, 160_000] {
            let component = [percentage("SYNTH_A", ppm)];
            for rule in [RoundingRule::HalfAwayFromZero, RoundingRule::Floor] {
                for gross in 1..=1_000_000 {
                    let line = inclusive(gross, &component, rule).unwrap();
                    let (net, tax) = (line.net.minor(), line.tax_total.minor());
                    assert!(
                        net == exact_net(gross, ppm, rule) && net + tax == gross,
                        "{ppm} ppm, {rule:?}, gross {gross}: net {net}, tax {tax}"
                    );
                }
            }
        }
    }

    #[test]
    fn an_inclusive_price_needs_an_inclusive_capable_component() {
        let held_back = [TaxComponent {
            is_inclusive_capable: false,
            ..percentage("SYNTH_A", 160_000)
        }];
        let rule = RoundingRule::HalfAwayFromZero;
        assert_eq!(
            inclusive(1_250, &held_back, rule),
            Err(TaxError::NotInclusiveCapable)
        );
        // The same component charges an exclusive price.
        assert_eq!(
            exclusive(1_000, &held_back, rule).map(|line| line.tax_total),
            Ok(jod(160))
        );
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// An inclusive line's net is the price divided by one plus the rate,
        /// rounded once by the caller's rule, and its tax is what is left: net
        /// plus tax is the price, to the fil, for any price and any rate.
        #[test]
        fn prop_inclusive_net_plus_tax_equals_gross(
            gross in any_gross(),
            ppm in rates_to_a_thousand_percent(),
            rule in any_rounding_rule(),
        ) {
            let line = inclusive(gross, &[percentage("SYNTH_A", ppm)], rule).unwrap();
            prop_assert_eq!(line.net.minor(), exact_net(gross, ppm, rule));
            prop_assert_eq!(
                i128::from(line.net.minor()) + i128::from(line.tax_total.minor()),
                i128::from(gross)
            );
            prop_assert_eq!(line.gross, jod(gross));
            let component = line.components.first().unwrap();
            prop_assert_eq!(component.base_amount, line.net);
            prop_assert_eq!(component.amount, line.tax_total);
        }

        /// Whichever way a price is written, the tax on it is never larger than
        /// the gross and never of the opposite sign, and neither is the net:
        /// a line cannot carry more tax than the customer pays.
        #[test]
        fn prop_tax_never_exceeds_gross(
            taxable in line_amounts(),
            ppm in rates_to_a_thousand_percent(),
            rule in any_rounding_rule(),
            mode in either_price_mode(),
        ) {
            let line = compute_line_tax(
                jod(taxable),
                Qty::ONE,
                mode,
                &[percentage("SYNTH_A", ppm)],
                StoreTaxProfile::Standard,
                &domestic(),
                rule,
            )
            .unwrap();
            let (net, tax, gross) =
                (line.net.minor(), line.tax_total.minor(), line.gross.minor());
            prop_assert!(tax.abs() <= gross.abs(), "tax {tax} against gross {gross}");
            prop_assert!(net.abs() <= gross.abs(), "net {net} against gross {gross}");
            prop_assert!(tax.signum() * gross.signum() >= 0);
            prop_assert!(net.signum() * gross.signum() >= 0);
        }
    }

    // ── several components, fixed amounts, tax on tax (1.3.5) ───────────
    // Every code is synthetic and every rate and amount an arithmetic vector.
    // `SYNTH_SST` and `SYNTH_GST` follow the default arrangement of the
    // `⚠️ OPEN` item in `ref/domain-api.md` §5 — the special tax first on the
    // net, the general tax on the net plus it — which no seeded rule uses yet.

    fn component(code: &str, sequence: u8, basis: TaxBasis, on: &[&str]) -> TaxComponent {
        TaxComponent {
            code: code.to_owned(),
            treatment: TaxTreatment::Standard,
            basis,
            sequence,
            base: if on.is_empty() {
                TaxBase::LineNet
            } else {
                TaxBase::NetPlusComponents {
                    codes: on.iter().map(|&code| code.to_owned()).collect(),
                }
            },
            is_inclusive_capable: on.is_empty(),
        }
    }

    fn rate(ppm: i64) -> TaxBasis {
        TaxBasis::Percentage {
            rate: Percent::from_ppm(ppm),
        }
    }

    fn per_unit(fils: i64) -> TaxBasis {
        TaxBasis::PerUnit { amount: jod(fils) }
    }

    fn line(
        mode: PriceMode,
        taxable: i64,
        qty_milli: i64,
        components: &[TaxComponent],
        rule: RoundingRule,
    ) -> Result<LineTax, TaxError> {
        compute_line_tax(
            jod(taxable),
            Qty::from_milli(qty_milli),
            mode,
            components,
            StoreTaxProfile::Standard,
            &domestic(),
            rule,
        )
    }

    /// Each component as (code, base, amount), in the order the line carries
    /// them.
    fn charged(line: &LineTax) -> Vec<(&str, i64, i64)> {
        line.components
            .iter()
            .map(|c| (c.code.as_str(), c.base_amount.minor(), c.amount.minor()))
            .collect()
    }

    #[test]
    fn sst_fixed_and_ad_valorem_components_compound_in_order() {
        let half_away = RoundingRule::HalfAwayFromZero;
        // A fixed special tax, an ad-valorem one on top of it, and a general
        // tax on top of both, listed in reverse so only the order the line
        // applies them in can pass. Two units at a net of 10.000 JOD.
        let stacked = [
            component(
                "SYNTH_GST",
                2,
                rate(160_000),
                &["SYNTH_SST_FIXED", "SYNTH_SST_RATE"],
            ),
            component("SYNTH_SST_RATE", 1, rate(100_000), &["SYNTH_SST_FIXED"]),
            component("SYNTH_SST_FIXED", 0, per_unit(250), &[]),
        ];
        let carried = line(PriceMode::Exclusive, 10_000, 2_000, &stacked, half_away).unwrap();
        assert_eq!(
            charged(&carried),
            [
                ("SYNTH_SST_FIXED", 10_000, 500),  // 250 × 2 units
                ("SYNTH_SST_RATE", 10_500, 1_050), // 10% of the net plus 500
                ("SYNTH_GST", 11_550, 1_848),      // 16% of the net plus both
            ]
        );
        assert_eq!(
            (carried.tax_total, carried.gross),
            (jod(3_398), jod(13_398))
        );
        let mut reversed = stacked.clone();
        reversed.reverse();
        assert_eq!(
            line(PriceMode::Exclusive, 10_000, 2_000, &reversed, half_away),
            Ok(carried),
            "the listing order is not the application order"
        );

        // Both shapes in one component: 10% of the net and 250 fils a unit.
        let compound = [
            component("SYNTH_GST", 1, rate(160_000), &["SYNTH_SST"]),
            component(
                "SYNTH_SST",
                0,
                TaxBasis::Compound {
                    rate: Percent::from_ppm(100_000),
                    per_unit: jod(250),
                },
                &[],
            ),
        ];
        let carried = line(PriceMode::Exclusive, 10_000, 2_000, &compound, half_away).unwrap();
        assert_eq!(
            charged(&carried),
            [("SYNTH_SST", 10_000, 1_500), ("SYNTH_GST", 11_500, 1_840)]
        );
        assert_eq!(carried.gross, jod(13_340));

        // Its two parts are summed before its one rounding: 10% of 1005 is
        // 100.5 and 333 fils on 1.5 units is 499.5, so it charges exactly 600,
        // where rounding each part first would charge 601.
        let halves = [component(
            "SYNTH_SST",
            0,
            TaxBasis::Compound {
                rate: Percent::from_ppm(100_000),
                per_unit: jod(333),
            },
            &[],
        )];
        assert_eq!(
            line(PriceMode::Exclusive, 1_005, 1_500, &halves, half_away).map(|l| l.tax_total),
            Ok(jod(600))
        );

        // A weighed 1.5 units at 333 fils a unit is 499.5 fils, which each rule
        // rounds once; the components above it are charged on what it rounded
        // to, which is what the line carries.
        let weighed = [
            component(
                "SYNTH_GST",
                2,
                rate(160_000),
                &["SYNTH_SST_FIXED", "SYNTH_SST_RATE"],
            ),
            component("SYNTH_SST_RATE", 1, rate(100_000), &["SYNTH_SST_FIXED"]),
            component("SYNTH_SST_FIXED", 0, per_unit(333), &[]),
        ];
        let at = |rule| line(PriceMode::Exclusive, 10_000, 1_500, &weighed, rule).unwrap();
        assert_eq!(
            charged(&at(RoundingRule::Floor)),
            [
                ("SYNTH_SST_FIXED", 10_000, 499),
                ("SYNTH_SST_RATE", 10_499, 1_049), // 1049.9
                ("SYNTH_GST", 11_548, 1_847),      // 1847.68
            ]
        );
        assert_eq!(
            charged(&at(half_away)),
            [
                ("SYNTH_SST_FIXED", 10_000, 500),
                ("SYNTH_SST_RATE", 10_500, 1_050),
                ("SYNTH_GST", 11_550, 1_848),
            ]
        );
    }

    #[test]
    fn gst_base_includes_sst() {
        let rule = RoundingRule::HalfAwayFromZero;
        let sst = component("SYNTH_SST", 0, per_unit(100), &[]);
        let on_both = [
            sst.clone(),
            component("SYNTH_GST", 1, rate(160_000), &["SYNTH_SST"]),
        ];
        let carried = line(PriceMode::Exclusive, 1_000, 1_000, &on_both, rule).unwrap();
        // The general tax is charged on the net plus the special tax: 16% of
        // 1100, not of 1000.
        assert_eq!(
            charged(&carried),
            [("SYNTH_SST", 1_000, 100), ("SYNTH_GST", 1_100, 176)]
        );
        assert_eq!(carried.gross, jod(1_276));

        // Left on the net alone, the same general tax would be 160: the
        // dependency is what puts the special tax in its base.
        let on_net = [sst, component("SYNTH_GST", 1, rate(160_000), &[])];
        assert_eq!(
            charged(&line(PriceMode::Exclusive, 1_000, 1_000, &on_net, rule).unwrap()),
            [("SYNTH_SST", 1_000, 100), ("SYNTH_GST", 1_000, 160)]
        );
    }

    #[test]
    fn tax_component_dependency_cycle_is_refused() {
        let refused = |components: &[TaxComponent]| {
            let mut refusals: Vec<_> = [PriceMode::Exclusive, PriceMode::Inclusive]
                .into_iter()
                .map(|mode| {
                    line(
                        mode,
                        1_000,
                        1_000,
                        components,
                        RoundingRule::HalfAwayFromZero,
                    )
                    .unwrap_err()
                })
                .collect();
            let inclusive = refusals.pop().unwrap();
            let exclusive = refusals.pop().unwrap();
            assert_eq!(
                exclusive, inclusive,
                "the list is refused before the mode is read"
            );
            exclusive
        };
        let named = |a: &str, b: &str| (a.to_owned(), b.to_owned());
        let circular = |(a, b)| TaxError::CircularComponentBase(a, b);

        // Two components, each charged on the other.
        let mutual = [
            component("SYNTH_A", 0, rate(10_000), &["SYNTH_B"]),
            component("SYNTH_B", 1, rate(10_000), &["SYNTH_A"]),
        ];
        assert_eq!(refused(&mutual), circular(named("SYNTH_A", "SYNTH_B")));
        // One charged on itself.
        let itself = [component("SYNTH_A", 0, rate(10_000), &["SYNTH_A"])];
        assert_eq!(refused(&itself), circular(named("SYNTH_A", "SYNTH_A")));
        // Three in a ring.
        let ring = [
            component("SYNTH_A", 0, rate(10_000), &["SYNTH_C"]),
            component("SYNTH_B", 1, rate(10_000), &["SYNTH_A"]),
            component("SYNTH_C", 2, rate(10_000), &["SYNTH_B"]),
        ];
        assert_eq!(refused(&ring), circular(named("SYNTH_A", "SYNTH_C")));
        // Two sharing a sequence, each on the other.
        let level = [
            component("SYNTH_A", 0, rate(10_000), &["SYNTH_B"]),
            component("SYNTH_B", 0, rate(10_000), &["SYNTH_A"]),
        ];
        assert_eq!(refused(&level), circular(named("SYNTH_A", "SYNTH_B")));

        // No cycle, but a base that is not applied first contradicts the order.
        let forward = [
            component("SYNTH_A", 0, rate(10_000), &["SYNTH_B"]),
            component("SYNTH_B", 1, rate(10_000), &[]),
        ];
        assert_eq!(
            refused(&forward),
            TaxError::BaseComponentNotEarlier("SYNTH_A".to_owned(), "SYNTH_B".to_owned())
        );
        let same_sequence = [
            component("SYNTH_A", 0, rate(10_000), &["SYNTH_B"]),
            component("SYNTH_B", 0, rate(10_000), &[]),
        ];
        assert_eq!(
            refused(&same_sequence),
            TaxError::BaseComponentNotEarlier("SYNTH_A".to_owned(), "SYNTH_B".to_owned())
        );
        // And a base that is not on the line at all.
        let dangling = [component("SYNTH_A", 0, rate(10_000), &["SYNTH_Z"])];
        assert_eq!(
            refused(&dangling),
            TaxError::UnknownBaseComponent("SYNTH_A".to_owned(), "SYNTH_Z".to_owned())
        );

        assert_eq!(
            circular(named("SYNTH_A", "SYNTH_B")).to_string(),
            "components SYNTH_A and SYNTH_B depend on each other"
        );
        assert_eq!(
            TaxError::BaseComponentNotEarlier("SYNTH_A".to_owned(), "SYNTH_B".to_owned())
                .to_string(),
            "component SYNTH_A is charged on SYNTH_B, which is not applied before it"
        );
    }

    #[test]
    fn an_incomplete_profile_pack_fails_closed() {
        let resolve = |rules: &[TaxRateRule], profile| {
            rates(resolve_components(category(), rules, profile, at(10)))
        };
        let zones = [StoreTaxProfile::Asez, StoreTaxProfile::DevelopmentArea];

        // The standard pack charges this category, and no zone pack does: a
        // zone store refuses rather than inheriting the standard 16%.
        let standard_only = [rule("SYNTH_A", 160_000, 0, None, None)];
        for zone in zones {
            assert_eq!(
                resolve(&standard_only, zone),
                Err(TaxError::ProfilePackIncomplete(zone))
            );
        }
        assert_eq!(
            resolve(&standard_only, StoreTaxProfile::Standard),
            charges("SYNTH_A", 160_000)
        );
        // A rule scoped to the standard profile is the standard pack's too.
        let scoped_standard = [rule(
            "SYNTH_A",
            160_000,
            0,
            None,
            Some(StoreTaxProfile::Standard),
        )];
        assert_eq!(
            resolve(&scoped_standard, StoreTaxProfile::Asez),
            Err(TaxError::ProfilePackIncomplete(StoreTaxProfile::Asez))
        );

        // A complete zone pack resolves its own rule, never the standard one.
        let complete = [
            rule("SYNTH_A", 160_000, 0, None, None),
            rule("SYNTH_A", 0, 0, None, Some(StoreTaxProfile::Asez)),
        ];
        assert_eq!(
            resolve(&complete, StoreTaxProfile::Asez),
            charges("SYNTH_A", 0)
        );
        // A zone pack missing one of the standard pack's components is
        // incomplete, even though it has the other.
        let partial = [
            rule("SYNTH_A", 160_000, 0, None, None),
            rule("SYNTH_B", 10_000, 0, None, None),
            rule("SYNTH_A", 0, 0, None, Some(StoreTaxProfile::Asez)),
        ];
        assert_eq!(
            resolve(&partial, StoreTaxProfile::Asez),
            Err(TaxError::ProfilePackIncomplete(StoreTaxProfile::Asez))
        );
        // A standard rule that has ended asks nothing of a zone pack, and a
        // zone pack may charge a component the standard pack does not.
        let ended_and_own = [
            rule("SYNTH_A", 160_000, 0, Some(5), None),
            rule("SYNTH_Z", 20_000, 0, None, Some(StoreTaxProfile::Asez)),
        ];
        assert_eq!(
            resolve(&ended_and_own, StoreTaxProfile::Asez),
            charges("SYNTH_Z", 20_000)
        );
        // With no rule anywhere, there is nothing incomplete: no rule applies.
        for zone in zones {
            assert_eq!(resolve(&[], zone), Err(TaxError::NoRuleInEffect(at(10))));
        }

        assert_eq!(
            TaxError::ProfilePackIncomplete(StoreTaxProfile::Asez).to_string(),
            "profile Asez has no complete rate pack; refusing to fall back"
        );
    }

    #[test]
    fn a_malformed_component_list_is_refused() {
        let rule = RoundingRule::HalfAwayFromZero;
        for mode in [PriceMode::Exclusive, PriceMode::Inclusive] {
            let refused = |components: &[TaxComponent]| line(mode, 1_000, 1_000, components, rule);
            // No component: an untaxed line is zero-rated or exempt, never empty.
            assert_eq!(refused(&[]), Err(TaxError::NoComponents));
            // One code twice is two rules for one component.
            let twice = [
                component("SYNTH_A", 0, rate(10_000), &[]),
                component("SYNTH_A", 1, rate(20_000), &[]),
            ];
            assert_eq!(
                refused(&twice),
                Err(TaxError::OverlappingRules("SYNTH_A".to_owned()))
            );
            // A fixed amount below zero, alone or beside a rate.
            for basis in [
                per_unit(-1),
                TaxBasis::Compound {
                    rate: Percent::from_ppm(10_000),
                    per_unit: jod(-1),
                },
            ] {
                assert_eq!(
                    refused(&[component("SYNTH_A", 0, basis, &[])]),
                    Err(TaxError::NegativeRate("SYNTH_A".to_owned()))
                );
            }
            // A fixed amount in another currency than the line's.
            let euros = [component(
                "SYNTH_A",
                0,
                TaxBasis::PerUnit {
                    amount: Money::from_minor(50, Currency::EUR),
                },
                &[],
            )];
            assert_eq!(
                refused(&euros),
                Err(TaxError::Money(MoneyError::CurrencyMismatch("JOD", "EUR")))
            );
        }
        // Zero is an amount, and charges nothing.
        assert_eq!(
            line(
                PriceMode::Exclusive,
                1_000,
                3_000,
                &[component("SYNTH_A", 0, per_unit(0), &[])],
                rule
            )
            .map(|line| line.tax_total),
            Ok(jod(0))
        );
    }

    #[test]
    fn an_inclusive_price_on_several_components_is_refused() {
        let rule = RoundingRule::HalfAwayFromZero;
        let side_by_side = [
            component("SYNTH_A", 0, rate(160_000), &[]),
            component("SYNTH_B", 0, rate(10_000), &[]),
        ];
        let stacked = [
            component("SYNTH_SST", 0, per_unit(100), &[]),
            component("SYNTH_GST", 1, rate(160_000), &["SYNTH_SST"]),
        ];
        for components in [&side_by_side[..], &stacked[..]] {
            assert_eq!(
                line(PriceMode::Inclusive, 1_276, 1_000, components, rule),
                Err(TaxError::InclusiveSplitUndefined(2))
            );
            // The same components charge an exclusive price.
            assert!(line(PriceMode::Exclusive, 1_000, 1_000, components, rule).is_ok());
        }
        assert_eq!(
            TaxError::InclusiveSplitUndefined(2).to_string(),
            "a tax-inclusive price on a line with 2 tax components is refused: how such a price \
             divides between them is not settled"
        );
    }

    #[test]
    fn inclusive_fixed_amounts_extract_exactly() {
        let half_away = RoundingRule::HalfAwayFromZero;
        let split = |gross, qty_milli, basis: TaxBasis, rule| {
            line(
                PriceMode::Inclusive,
                gross,
                qty_milli,
                &[component("SYNTH_A", 0, basis, &[])],
                rule,
            )
            .map(|line| (line.net.minor(), line.tax_total.minor()))
        };
        // A fixed amount alone: two units at 250 fils in a 1.000 JOD price.
        assert_eq!(
            split(1_000, 2_000, per_unit(250), half_away),
            Ok((500, 500))
        );
        // Both shapes: (1260 − 100) / 1.16 is exactly 1000.
        let compound = || TaxBasis::Compound {
            rate: Percent::from_ppm(160_000),
            per_unit: jod(100),
        };
        assert_eq!(split(1_260, 1_000, compound(), half_away), Ok((1_000, 260)));
        // A weighed 1.5 units at 333 fils is 499.5 fils, so the net is 500.5:
        // the rule rounds the net once and the tax is what is left.
        assert_eq!(
            split(1_000, 1_500, per_unit(333), half_away),
            Ok((501, 499))
        );
        assert_eq!(
            split(1_000, 1_500, per_unit(333), RoundingRule::HalfEven),
            Ok((500, 500))
        );
        assert_eq!(
            split(1_000, 1_500, per_unit(333), RoundingRule::Floor),
            Ok((500, 500))
        );
        assert_eq!(
            split(1_000, 1_500, per_unit(333), RoundingRule::Ceil),
            Ok((501, 499))
        );

        // A price that is all fixed tax leaves a net of zero; a smaller one
        // cannot contain the tax, and is refused rather than given a negative
        // net.
        assert_eq!(split(500, 2_000, per_unit(250), half_away), Ok((0, 500)));
        for gross in [499, 0] {
            assert_eq!(
                split(gross, 2_000, per_unit(250), half_away),
                Err(TaxError::FixedTaxExceedsPrice("SYNTH_A".to_owned()))
            );
        }
        assert_eq!(
            TaxError::FixedTaxExceedsPrice("SYNTH_A".to_owned()).to_string(),
            "component SYNTH_A charges a fixed amount larger than the tax-inclusive price"
        );
    }

    /// One to four components with sequences from 0 to 3, each a percentage
    /// to 100%, a fixed amount to 100 JOD a unit, or both, and each charged on
    /// the net or on any choice of the components applied before it; with the
    /// listing rotated so its order is not the application order. It excludes
    /// a base that is not applied earlier, which
    /// `tax_component_dependency_cycle_is_refused` covers, and negative rates
    /// and amounts, which are refused.
    fn component_stacks() -> impl Strategy<Value = Vec<TaxComponent>> {
        let one = (
            0u8..4,
            0u8..3,
            0i64..=1_000_000,
            0i64..=100_000,
            any::<u8>(),
        );
        (prop::collection::vec(one, 1..=4), 0usize..4).prop_map(|(drawn, rotation)| {
            let mut stack: Vec<TaxComponent> = Vec::new();
            let mut drawn = drawn;
            drawn.sort_by_key(|&(sequence, ..)| sequence);
            for (i, (sequence, kind, ppm, fixed, choice)) in drawn.into_iter().enumerate() {
                let basis = match kind {
                    0 => rate(ppm),
                    1 => per_unit(fixed),
                    _ => TaxBasis::Compound {
                        rate: Percent::from_ppm(ppm),
                        per_unit: jod(fixed),
                    },
                };
                let earlier: Vec<String> = stack
                    .iter()
                    .filter(|c| c.sequence < sequence)
                    .enumerate()
                    .filter(|(bit, _)| choice & (1 << bit) != 0)
                    .map(|(_, c)| c.code.clone())
                    .collect();
                let on: Vec<&str> = earlier.iter().map(String::as_str).collect();
                stack.push(component(&format!("SYNTH_{i}"), sequence, basis, &on));
            }
            let len = stack.len();
            stack.rotate_left(rotation % len);
            stack
        })
    }

    /// A tax-inclusive price: any amount `i64` holds, or one from nothing to a
    /// million dinars, where a fixed amount of up to a thousand dinars on up to
    /// a thousand units is as likely to exceed the price as not. A draw from
    /// the whole of `i64` alone almost never lands that low.
    fn inclusive_prices() -> impl Strategy<Value = i64> {
        prop_oneof![any::<i64>(), 0i64..=1_000_000_000]
    }

    /// A quantity from none to a thousand units, fractions of a unit included.
    /// It excludes negative quantities, which no sale line carries.
    fn quantities() -> impl Strategy<Value = i64> {
        0i64..=1_000_000
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// However many components a line carries, in whatever order and on
        /// whatever bases: each is applied after the components it is charged
        /// on, on a base that is the net plus exactly their carried amounts,
        /// and is that base times its rate plus its fixed amount times the
        /// quantity, rounded once. The tax is exactly the sum of the
        /// components and the gross is exactly net plus tax. An inclusive
        /// price on more than one is refused, never divided by guesswork.
        #[test]
        fn prop_multi_component_line_sums_correctly(
            stack in component_stacks(),
            net in -1_000_000_000_000i64..=1_000_000_000_000,
            qty in quantities(),
            rule in any_rounding_rule(),
        ) {
            let carried = line(PriceMode::Exclusive, net, qty, &stack, rule).unwrap();
            prop_assert_eq!(carried.components.len(), stack.len());
            let mut sum = 0i64;
            for (i, charged) in carried.components.iter().enumerate() {
                let source = stack.iter().find(|c| c.code == charged.code).unwrap();
                let named = match &source.base {
                    TaxBase::LineNet => Vec::new(),
                    TaxBase::NetPlusComponents { codes } => codes.clone(),
                };
                let before = carried.components.get(..i).unwrap();
                let mut base = net;
                for code in &named {
                    let earlier = before.iter().find(|c| &c.code == code);
                    prop_assert!(earlier.is_some(), "{} is applied before {code}", charged.code);
                    base += earlier.unwrap().amount.minor();
                }
                let in_order = before.iter().all(|c| {
                    let other = stack.iter().find(|s| s.code == c.code).unwrap();
                    (other.sequence, &other.code) < (source.sequence, &source.code)
                });
                prop_assert!(in_order, "{} comes after every component before it", charged.code);
                prop_assert_eq!(charged.base_amount, jod(base));
                let (ppm, fixed) = match source.basis {
                    TaxBasis::Percentage { rate } => (rate.ppm(), 0),
                    TaxBasis::PerUnit { amount } => (0, amount.minor()),
                    TaxBasis::Compound { rate, per_unit } => (rate.ppm(), per_unit.minor()),
                };
                // base × ppm / 10⁶ + fixed × qty / 10³, over one denominator.
                let exact = i128::from(base) * i128::from(ppm)
                    + i128::from(fixed) * i128::from(qty) * 1_000;
                prop_assert_eq!(charged.amount, jod(rounded_ratio(exact, 1_000_000, rule)));
                sum += charged.amount.minor();
            }
            prop_assert_eq!(carried.tax_total, jod(sum));
            prop_assert_eq!(carried.gross, jod(net + sum));
            if stack.len() > 1 {
                prop_assert_eq!(
                    line(PriceMode::Inclusive, net, qty, &stack, rule),
                    Err(TaxError::InclusiveSplitUndefined(stack.len()))
                );
            }
        }

        /// A fixed amount on three units is exactly three times the amount on
        /// one, whichever way the price is written; on a fraction of a unit it
        /// is the amount times the quantity, rounded once by the caller's rule.
        #[test]
        fn prop_per_unit_component_scales_with_quantity(
            fils in 0i64..=1_000_000,
            units in 0i64..=1_000,
            milli in quantities(),
            price in 0i64..=1_000_000_000,
            rule in any_rounding_rule(),
        ) {
            let fixed = [component("SYNTH_A", 0, per_unit(fils), &[])];
            let tax = |mode, taxable, qty_milli| {
                line(mode, taxable, qty_milli, &fixed, rule).map(|line| line.tax_total.minor())
            };
            // An inclusive price large enough to contain the tax on every unit.
            let gross = price + fils * units;
            for (mode, taxable) in [(PriceMode::Exclusive, price), (PriceMode::Inclusive, gross)] {
                let one = tax(mode, taxable, 1_000).unwrap();
                prop_assert_eq!(one, fils);
                prop_assert_eq!(tax(mode, taxable, units * 1_000).unwrap(), units * one);
            }
            prop_assert_eq!(
                tax(PriceMode::Exclusive, price, milli).map(jod),
                jod(fils).mul_qty(Qty::from_milli(milli), rule).map_err(TaxError::from)
            );
        }

        /// An inclusive price whose one component charges a fixed amount, alone
        /// or beside a rate: the net is the price less the fixed amount,
        /// divided by one plus the rate and rounded once, exactly as integer
        /// division puts it, and net plus tax is the price. A price too small
        /// to contain the fixed amount is refused, and so is one whose
        /// remainder no amount can hold; no other price is.
        #[test]
        fn prop_inclusive_fixed_amount_extracts_exactly(
            gross in inclusive_prices(),
            fils in 0i64..=1_000_000,
            qty in quantities(),
            ppm in rates_to_a_thousand_percent(),
            with_rate in any::<bool>(),
            rule in any_rounding_rule(),
        ) {
            let (ppm, basis) = if with_rate {
                (ppm, TaxBasis::Compound { rate: Percent::from_ppm(ppm), per_unit: jod(fils) })
            } else {
                (0, per_unit(fils))
            };
            let result = line(PriceMode::Inclusive, gross, qty, &[component("SYNTH_A", 0, basis, &[])], rule);
            // In thousandths of a fil: the price less the fixed amount.
            let priced = i128::from(gross) * 1_000 - i128::from(fils) * i128::from(qty);
            if priced.abs() > i128::from(i64::MAX) * 1_000 {
                prop_assert_eq!(result, Err(TaxError::Money(MoneyError::Overflow)));
                return Ok(());
            }
            let net = rounded_ratio(priced * 1_000, 1_000_000 + i128::from(ppm), rule);
            if net != 0 && net.signum() != gross.signum() {
                prop_assert_eq!(result, Err(TaxError::FixedTaxExceedsPrice("SYNTH_A".to_owned())));
                return Ok(());
            }
            let carried = result.unwrap();
            prop_assert_eq!(carried.net, jod(net));
            prop_assert_eq!(
                i128::from(carried.net.minor()) + i128::from(carried.tax_total.minor()),
                i128::from(gross)
            );
            prop_assert_eq!(carried.gross, jod(gross));
            let component = carried.components.first().unwrap();
            prop_assert_eq!(carried.components.len(), 1);
            prop_assert_eq!(component.base_amount, carried.net);
            prop_assert_eq!(component.amount, carried.tax_total);
        }
    }

    // ── summarize_tax (1.3.6) ───────────────────────────────────────────
    // Codes stay synthetic and rates are arithmetic vectors. A zero-rated or
    // exempt `SYNTH_GST` is a treatment the engine is handed, not a claim
    // about which Jordanian supplies are either: the pack is 1.3.7's.

    fn treated(code: &str, treatment: TaxTreatment, basis: TaxBasis) -> TaxComponent {
        TaxComponent {
            treatment,
            ..component(code, 0, basis, &[])
        }
    }

    /// An exclusive line of one unit, rounded half away from zero.
    fn sold(net: i64, components: &[TaxComponent]) -> LineTax {
        exclusive(net, components, RoundingRule::HalfAwayFromZero).unwrap()
    }

    fn summary_row(
        code: &str,
        treatment: TaxTreatment,
        ppm: i64,
        per_unit: Option<i64>,
        net: i64,
        tax: i64,
        gross: i64,
    ) -> TaxSummaryRow {
        TaxSummaryRow {
            code: code.to_owned(),
            treatment,
            rate: Percent::from_ppm(ppm),
            per_unit: per_unit.map(jod),
            reason: None,
            net: jod(net),
            tax: jod(tax),
            gross: jod(gross),
        }
    }

    #[test]
    fn exempt_and_zero_rated_lines_make_two_summary_rows() {
        let standard = treated("SYNTH_GST", TaxTreatment::Standard, rate(160_000));
        let zero = treated("SYNTH_GST", TaxTreatment::Zero, rate(0));
        let exempt = treated("SYNTH_GST", TaxTreatment::Exempt, rate(0));
        let lines = [
            sold(2_500, std::slice::from_ref(&zero)),
            sold(10_000, &[standard]),
            sold(3_000, &[exempt]),
            sold(1_500, &[zero]),
        ];
        // The two untaxed rows share a code and a rate and carry no tax; only
        // the treatment parts them. A summary that dropped it would print one
        // untaxed row of 7.000 JOD, and the return would put an exempt supply
        // in the zero-rated box.
        assert_eq!(
            summarize_tax(&lines).unwrap(),
            vec![
                summary_row(
                    "SYNTH_GST",
                    TaxTreatment::Standard,
                    160_000,
                    None,
                    10_000,
                    1_600,
                    11_600
                ),
                summary_row("SYNTH_GST", TaxTreatment::Zero, 0, None, 4_000, 0, 4_000),
                summary_row("SYNTH_GST", TaxTreatment::Exempt, 0, None, 3_000, 0, 3_000),
            ]
        );
    }

    #[test]
    fn summary_rows_are_keyed_by_every_rate_defining_fact() {
        let compound = |ppm, fils| TaxBasis::Compound {
            rate: Percent::from_ppm(ppm),
            per_unit: jod(fils),
        };
        let line_of = |code, treatment, basis| sold(10_000, &[treated(code, treatment, basis)]);
        let standard = TaxTreatment::Standard;
        // 16% of 10.000 JOD and 100 fils on one unit.
        let base = line_of("SYNTH_A", standard, compound(160_000, 100));
        // No sale carries a zero-rating reason until the accountant names the
        // zero-rated lines (`ref/merchant-decisions.md` 6.14), so the engine
        // refuses one and this line is built by hand.
        let mut exported = base.clone();
        exported.supply_reason = Some(ZeroRatingReason::Export);
        let variants = [
            (
                "its code",
                line_of("SYNTH_B", standard, compound(160_000, 100)),
            ),
            (
                "its treatment",
                line_of("SYNTH_A", TaxTreatment::Reduced, compound(160_000, 100)),
            ),
            (
                "its percentage",
                line_of("SYNTH_A", standard, compound(100_000, 100)),
            ),
            (
                "its fixed amount",
                line_of("SYNTH_A", standard, compound(160_000, 200)),
            ),
            (
                "having a fixed amount at all",
                line_of("SYNTH_A", standard, rate(160_000)),
            ),
            ("its zero-rating reason", exported),
        ];
        for (fact, variant) in variants {
            let summary = summarize_tax(&[base.clone(), variant]).unwrap();
            assert_eq!(
                summary.len(),
                2,
                "a line differing in {fact} is a row of its own"
            );
        }
        // The same rate twice is one row: exactly both lines.
        assert_eq!(
            summarize_tax(&[base.clone(), base]).unwrap(),
            vec![summary_row(
                "SYNTH_A",
                standard,
                160_000,
                Some(100),
                20_000,
                3_400,
                23_400
            )]
        );
    }

    #[test]
    fn a_stacked_line_summarizes_each_component_on_its_own_base() {
        // A fixed special tax, and a general tax charged on the net plus it.
        let stacked = sold(
            10_000,
            &[
                component("SYNTH_SST", 0, per_unit(500), &[]),
                component("SYNTH_GST", 1, rate(160_000), &["SYNTH_SST"]),
            ],
        );
        assert_eq!(
            charged(&stacked),
            [("SYNTH_SST", 10_000, 500), ("SYNTH_GST", 10_500, 1_680)]
        );
        // The same general tax on a line with nothing under it.
        let plain = sold(2_000, &[percentage("SYNTH_GST", 160_000)]);
        let summary = summarize_tax(&[stacked, plain]).unwrap();
        // One general-tax row for both lines, on 10.500 + 2.000 JOD: what its
        // rate was charged on, so 16% of its net is its tax.
        assert_eq!(
            summary,
            vec![
                summary_row(
                    "SYNTH_GST",
                    TaxTreatment::Standard,
                    160_000,
                    None,
                    12_500,
                    2_000,
                    14_500
                ),
                summary_row(
                    "SYNTH_SST",
                    TaxTreatment::Standard,
                    0,
                    Some(500),
                    10_000,
                    500,
                    10_500
                ),
            ]
        );
        // The taxes add up to the receipt's: 2.180 on the stacked line and
        // 0.320 on the plain one. The nets do not, because the stacked line's
        // net is the base of both its rows.
        let rows = |pick: fn(&TaxSummaryRow) -> i64| summary.iter().map(pick).sum::<i64>();
        assert_eq!(rows(|r| r.tax.minor()), 2_180 + 320);
        assert_eq!(rows(|r| r.net.minor()), 12_500 + 10_000);
    }

    /// A line that has been changed after the engine computed it.
    type Spoil = fn(&mut LineTax);

    #[test]
    fn a_summary_refuses_a_line_that_disagrees_with_itself() {
        let good = sold(10_000, &[percentage("SYNTH_A", 160_000)]);
        assert_eq!(summarize_tax(std::slice::from_ref(&good)).unwrap().len(), 1);
        let first = |line: &mut LineTax| line.components.first_mut().unwrap().amount = jod(1_601);
        let cases: [(&str, Spoil, TaxError); 5] = [
            ("a component", first, TaxError::LineTaxUnbalanced(1)),
            (
                "the tax, with the gross to match",
                |line| {
                    line.tax_total = jod(1_601);
                    line.gross = jod(11_601);
                },
                TaxError::LineTaxUnbalanced(1),
            ),
            (
                "the gross",
                |line| line.gross = jod(11_601),
                TaxError::LineTaxUnbalanced(1),
            ),
            (
                "the net",
                |line| line.net = jod(10_001),
                TaxError::LineTaxUnbalanced(1),
            ),
            (
                "every component, with no tax left",
                |line| {
                    line.components.clear();
                    line.tax_total = jod(0);
                    line.gross = line.net;
                },
                TaxError::NoComponents,
            ),
        ];
        for (changed, spoil, refusal) in cases {
            let mut line = good.clone();
            spoil(&mut line);
            assert_eq!(
                summarize_tax(&[good.clone(), line]),
                Err(refusal),
                "a line whose {changed} changed is refused, by its position"
            );
        }
    }

    #[test]
    fn a_summary_refuses_mixed_currencies_and_overflow() {
        assert_eq!(summarize_tax(&[]), Ok(Vec::new()));
        let in_dinars = sold(10_000, &[percentage("SYNTH_A", 160_000)]);
        // Under another code, so no row would ever add the two together: only
        // the line itself can stop it.
        let in_dollars = compute_line_tax(
            Money::from_minor(10_000, Currency::USD),
            Qty::ONE,
            PriceMode::Exclusive,
            &[percentage("SYNTH_B", 160_000)],
            StoreTaxProfile::Standard,
            &domestic(),
            RoundingRule::HalfAwayFromZero,
        )
        .unwrap();
        assert_eq!(
            summarize_tax(&[in_dinars, in_dollars]),
            Err(TaxError::Money(MoneyError::CurrencyMismatch("JOD", "USD")))
        );
        // A fixed amount in another currency than its line, which the engine
        // never returns.
        let mut foreign = sold(10_000, &[component("SYNTH_A", 0, per_unit(100), &[])]);
        for charged in &mut foreign.components {
            charged.basis = TaxBasis::PerUnit {
                amount: Money::from_minor(100, Currency::EUR),
            };
        }
        assert_eq!(
            summarize_tax(&[foreign]),
            Err(TaxError::Money(MoneyError::CurrencyMismatch("JOD", "EUR")))
        );
        // Lines on one row whose nets, taxes or grosses no amount can hold
        // together, each with the other two sums in range. The engine never
        // carries a tax of the opposite sign to its net, so these are built by
        // hand; each one adds up.
        let carried = |net: i64, tax: i64| {
            let mut line = sold(net, &[percentage("SYNTH_A", 0)]);
            for charged in &mut line.components {
                charged.amount = jod(tax);
            }
            line.tax_total = jod(tax);
            line.gross = jod(net + tax);
            line
        };
        let quarter = i64::MAX / 4;
        for (sum, net, tax) in [
            ("the nets", 2 * quarter + 1, -quarter),
            ("the taxes", -quarter, 2 * quarter + 1),
            ("the grosses", quarter, quarter),
        ] {
            let line = carried(net, tax);
            assert_eq!(
                summarize_tax(&[line.clone(), line.clone(), line]),
                Err(TaxError::Money(MoneyError::Overflow)),
                "{sum} overflow"
            );
        }
    }

    /// One to `most` components on a line, coded by position so that lines
    /// share codes: each of any treatment, charging 0%, 4% or 16%, a fixed 100
    /// or 250 fils a unit, or both, on the net or on the net plus the component
    /// before it; a zero-rated or exempt one charges 0%, since anything else is
    /// refused. The rates are few so rows often merge across lines. It
    /// excludes only the shapes `component_stacks` already covers for the
    /// engine: rotated listings and a base naming several components.
    fn summary_stacks(most: usize) -> impl Strategy<Value = Vec<TaxComponent>> {
        let treatment = prop_oneof![
            Just(TaxTreatment::Standard),
            Just(TaxTreatment::Reduced),
            Just(TaxTreatment::Zero),
            Just(TaxTreatment::Exempt),
        ];
        let rates = || prop::sample::select(vec![0i64, 40_000, 160_000]);
        let fixed = || prop::sample::select(vec![100i64, 250]);
        let basis = prop_oneof![
            rates().prop_map(rate),
            fixed().prop_map(per_unit),
            (rates(), fixed()).prop_map(|(ppm, fils)| TaxBasis::Compound {
                rate: Percent::from_ppm(ppm),
                per_unit: jod(fils),
            }),
        ];
        prop::collection::vec((treatment, basis, any::<bool>()), 1..=most).prop_map(|drawn| {
            let mut stack: Vec<TaxComponent> = Vec::new();
            for ((treatment, basis, on_previous), sequence) in drawn.into_iter().zip(0u8..) {
                let previous: Vec<String> = stack
                    .last()
                    .filter(|_| on_previous)
                    .map(|c| c.code.clone())
                    .into_iter()
                    .collect();
                let on: Vec<&str> = previous.iter().map(String::as_str).collect();
                let basis = match treatment {
                    TaxTreatment::Zero | TaxTreatment::Exempt => rate(0),
                    TaxTreatment::Standard | TaxTreatment::Reduced => basis,
                };
                stack.push(TaxComponent {
                    treatment,
                    ..component(&format!("SYNTH_{sequence}"), sequence, basis, &on)
                });
            }
            stack
        })
    }

    /// A receipt of up to a dozen lines under one rounding rule, each priced
    /// either way at up to a million dinars either side of zero, on up to a
    /// thousand units; half the receipts carry one component a line, the case
    /// in which the rows add up to the whole receipt. A line the engine
    /// refuses, such as a tax-inclusive price on several components, is
    /// dropped: a summary is only ever handed lines the engine computed.
    fn receipts() -> impl Strategy<Value = Vec<LineTax>> {
        let of = |most| {
            let one = (
                summary_stacks(most),
                either_price_mode(),
                -1_000_000_000i64..=1_000_000_000,
                quantities(),
            );
            (prop::collection::vec(one, 0..=12), any_rounding_rule()).prop_map(|(specs, rule)| {
                specs
                    .into_iter()
                    .filter_map(|(stack, mode, taxable, qty)| {
                        line(mode, taxable, qty, &stack, rule).ok()
                    })
                    .collect::<Vec<LineTax>>()
            })
        };
        prop_oneof![of(1), of(3)]
    }

    /// A price either way, up to a million dinars either side of zero.
    fn summary_prices() -> impl Strategy<Value = Vec<i64>> {
        prop::collection::vec(-1_000_000_000i64..=1_000_000_000, 1..=5)
    }

    /// A basis that charges something: a rate above 0% to 1000%, a fixed
    /// amount from a fil to a thousand dinars a unit, or both with at least one
    /// above zero. It excludes negative rates and amounts, which are refused
    /// before the treatment is read (`a_negative_rate_is_refused`).
    fn charging_bases() -> impl Strategy<Value = TaxBasis> {
        prop_oneof![
            (1i64..=10_000_000).prop_map(rate),
            (1i64..=1_000_000).prop_map(per_unit),
            (0i64..=10_000_000, 0i64..=1_000_000)
                .prop_filter("charges something", |&(ppm, fils)| ppm + fils > 0)
                .prop_map(|(ppm, fils)| TaxBasis::Compound {
                    rate: Percent::from_ppm(ppm),
                    per_unit: jod(fils),
                }),
        ]
    }

    #[test]
    fn an_untaxed_treatment_that_charges_is_refused() {
        let rule = RoundingRule::HalfAwayFromZero;
        let refused = |code: &str| Err(TaxError::UntaxedTreatmentCharges(code.to_owned()));
        for mode in [PriceMode::Exclusive, PriceMode::Inclusive] {
            let charge = |components: &[TaxComponent]| line(mode, 10_000, 2_000, components, rule);
            // A zero rate or a fixed amount of nothing is what an untaxed
            // treatment charges, and it carries no tax.
            for treatment in [TaxTreatment::Zero, TaxTreatment::Exempt] {
                for nothing in [rate(0), per_unit(0)] {
                    let untaxed = charge(&[treated("SYNTH_GST", treatment, nothing)]).unwrap();
                    assert_eq!(untaxed.tax_total, jod(0), "{mode:?} {treatment:?}");
                }
            }
            assert_eq!(
                charge(&[treated("SYNTH_GST", TaxTreatment::Zero, rate(160_000))]),
                refused("SYNTH_GST")
            );
            assert_eq!(
                charge(&[treated("SYNTH_GST", TaxTreatment::Exempt, per_unit(100))]),
                refused("SYNTH_GST")
            );
            // A reduced rate is a rate, and is charged.
            let reduced = charge(&[treated("SYNTH_GST", TaxTreatment::Reduced, rate(40_000))]);
            assert!(!reduced.unwrap().tax_total.is_zero());
        }
        // Beside a taxed component, the untaxed one is named.
        let exempt = TaxComponent {
            treatment: TaxTreatment::Exempt,
            ..component("SYNTH_EXEMPT", 1, rate(10_000), &[])
        };
        assert_eq!(
            exclusive(10_000, &[percentage("SYNTH_GST", 160_000), exempt], rule),
            refused("SYNTH_EXEMPT")
        );
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// The receipt's tax summary is the sum of what its lines carry, never
        /// a second calculation: its taxes add up to the lines' taxes to the
        /// fil. Each row is exactly the components it groups, every component
        /// is on exactly one row, and each row's gross is its net plus its
        /// tax. When every line carries one component the rows add up to the
        /// whole receipt, net and gross too. The order the lines were scanned
        /// in never changes the summary.
        #[test]
        fn prop_line_tax_sum_equals_receipt_tax(lines in receipts()) {
            let summary = summarize_tax(&lines).unwrap();
            let receipt = |pick: fn(&LineTax) -> i64| lines.iter().map(pick).sum::<i64>();
            let rows = |pick: fn(&TaxSummaryRow) -> i64| summary.iter().map(pick).sum::<i64>();
            prop_assert_eq!(rows(|r| r.tax.minor()), receipt(|l| l.tax_total.minor()));

            let mut grouped = 0usize;
            for row in &summary {
                let (mut net, mut tax) = (0i64, 0i64);
                for line in &lines {
                    for charged in &line.components {
                        let (ppm, fixed) = match charged.basis {
                            TaxBasis::Percentage { rate } => (rate, None),
                            TaxBasis::PerUnit { amount } => (Percent::ZERO, Some(amount)),
                            TaxBasis::Compound { rate, per_unit } => (rate, Some(per_unit)),
                        };
                        if charged.code == row.code
                            && charged.treatment == row.treatment
                            && ppm == row.rate
                            && fixed == row.per_unit
                            && line.supply_reason == row.reason
                        {
                            net += charged.base_amount.minor();
                            tax += charged.amount.minor();
                            grouped += 1;
                        }
                    }
                }
                prop_assert_eq!(row.net, jod(net));
                prop_assert_eq!(row.tax, jod(tax));
                prop_assert_eq!(row.gross, jod(net + tax));
            }
            // A component counted twice would mean two rows for one rate.
            prop_assert_eq!(grouped, lines.iter().map(|l| l.components.len()).sum::<usize>());

            if lines.iter().all(|l| l.components.len() == 1) {
                prop_assert_eq!(rows(|r| r.net.minor()), receipt(|l| l.net.minor()));
                prop_assert_eq!(rows(|r| r.gross.minor()), receipt(|l| l.gross.minor()));
            }
            let mut rescanned = lines.clone();
            rescanned.reverse();
            prop_assert_eq!(summarize_tax(&rescanned).unwrap(), summary);
        }

        /// A zero-rated line and an exempt one both carry no tax, whichever way
        /// the price is written and under any rule: a zero-rated or exempt
        /// component that charges any rate or amount is refused. And the
        /// summary still keeps them apart: one zero-rated row of exactly the
        /// zero-rated lines, one exempt row of exactly the exempt lines, beside
        /// the taxed row. Pooling them would report an exempt supply in the
        /// zero-rated box.
        #[test]
        fn prop_exempt_and_zero_produce_zero_tax_but_differ_in_reporting(
            zero in summary_prices(),
            exempt in summary_prices(),
            taxed in prop::collection::vec(-1_000_000_000i64..=1_000_000_000, 0..=5),
            charging in charging_bases(),
            mode in either_price_mode(),
            rule in any_rounding_rule(),
        ) {
            for treatment in [TaxTreatment::Zero, TaxTreatment::Exempt] {
                let charges = [treated("SYNTH_GST", treatment, charging.clone())];
                prop_assert_eq!(
                    line(mode, 10_000, 1_000, &charges, rule),
                    Err(TaxError::UntaxedTreatmentCharges("SYNTH_GST".to_owned()))
                );
            }
            let priced = |prices: &[i64], treatment, ppm| {
                let gst = [treated("SYNTH_GST", treatment, rate(ppm))];
                prices
                    .iter()
                    .map(|&price| line(mode, price, 1_000, &gst, rule).unwrap())
                    .collect::<Vec<LineTax>>()
            };
            let mut lines = priced(&zero, TaxTreatment::Zero, 0);
            lines.extend(priced(&exempt, TaxTreatment::Exempt, 0));
            for untaxed in &lines {
                prop_assert_eq!(untaxed.tax_total, jod(0));
                prop_assert_eq!(untaxed.net, untaxed.gross);
            }
            lines.extend(priced(&taxed, TaxTreatment::Standard, 160_000));

            let summary = summarize_tax(&lines).unwrap();
            for (treatment, prices) in [(TaxTreatment::Zero, &zero), (TaxTreatment::Exempt, &exempt)] {
                let total = jod(prices.iter().sum());
                let row: Vec<&TaxSummaryRow> =
                    summary.iter().filter(|r| r.treatment == treatment).collect();
                prop_assert_eq!(row.len(), 1, "one {:?} row", treatment);
                let row = row.first().unwrap();
                prop_assert_eq!(row.tax, jod(0));
                prop_assert_eq!(row.net, total);
                prop_assert_eq!(row.gross, total);
            }
            prop_assert_eq!(summary.len(), 2 + usize::from(!taxed.is_empty()));
        }
    }

    // ── a store not registered for GST (1.3.8) ──────────────────────────

    /// Any destination but the one a store may sell to today.
    fn other_destinations() -> impl Strategy<Value = SupplyDestination> {
        prop_oneof![
            Just(SupplyDestination::Export),
            Just(SupplyDestination::FreeZone),
            Just(SupplyDestination::DevelopmentArea),
            Just(SupplyDestination::EligibleBody),
        ]
    }

    /// Whatever components a caller might hand the engine: none, or one to
    /// four of any shape on any base (`component_stacks`), which it never reads
    /// for an unregistered store.
    fn any_components() -> impl Strategy<Value = Vec<TaxComponent>> {
        prop_oneof![Just(Vec::new()), component_stacks()]
    }

    #[test]
    fn unregistered_lines_summarize_to_no_tax_rows() {
        let untaxed = |price| {
            compute_line_tax(
                jod(price),
                Qty::ONE,
                PriceMode::Inclusive,
                &[percentage("SYNTH_GST", 160_000)],
                StoreTaxProfile::Unregistered,
                &domestic(),
                RoundingRule::HalfAwayFromZero,
            )
            .unwrap()
        };
        // A receipt of them has no tax rows: there is no rate to report.
        let receipt = [untaxed(2_500), untaxed(1_000)];
        assert_eq!(summarize_tax(&receipt), Ok(Vec::new()));
        // But one beside a registered store's line is not an untaxed line on
        // that receipt; it is a line whose components are missing.
        let taxed = sold(10_000, &[percentage("SYNTH_GST", 160_000)]);
        assert_eq!(
            summarize_tax(&[taxed, untaxed(2_500)]),
            Err(TaxError::NoComponents)
        );
        // And with no component, it carries no tax: one that claims some does
        // not add up.
        let mut claims_tax = untaxed(2_500);
        claims_tax.tax_total = jod(1);
        claims_tax.gross = jod(2_501);
        assert_eq!(
            summarize_tax(&[claims_tax]),
            Err(TaxError::LineTaxUnbalanced(0))
        );
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// A store not registered for GST charges no tax on any line: whatever
        /// components it is handed, at any price either way, any quantity and
        /// any rule, the line carries no component and its net and gross are
        /// both the price, and its receipt has no tax rows. The supply is still
        /// read first: a non-domestic supply is refused for every store.
        #[test]
        fn prop_unregistered_profile_yields_no_tax(
            components in any_components(),
            price in any_gross(),
            qty in quantities(),
            mode in either_price_mode(),
            rule in any_rounding_rule(),
            elsewhere in other_destinations(),
        ) {
            let compute = |supply: &SupplyTaxContext| {
                compute_line_tax(
                    jod(price),
                    Qty::from_milli(qty),
                    mode,
                    &components,
                    StoreTaxProfile::Unregistered,
                    supply,
                    rule,
                )
            };
            let line = compute(&domestic()).unwrap();
            prop_assert_eq!(&line, &LineTax {
                net: jod(price),
                components: Vec::new(),
                tax_total: jod(0),
                gross: jod(price),
                supply_reason: None,
            });
            prop_assert_eq!(summarize_tax(&[line]), Ok(Vec::new()));

            let abroad = SupplyTaxContext { destination: elsewhere, ..domestic() };
            prop_assert_eq!(
                compute(&abroad),
                Err(TaxError::SupplyDestinationRefused(elsewhere))
            );
        }
    }
}
