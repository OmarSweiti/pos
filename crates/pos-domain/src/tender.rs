//! How a sale is paid for (microstep 1.5.x).
//!
//! Three things live here. The *vocabulary* (1.5.1) — what kinds of tender
//! exist, what each one does, and what one collected tender records;
//! **cash rounding** (1.5.3) — what the tender that settles a sale is asked
//! for when the smallest coin cannot make up the fils; and the **denomination
//! table** (1.5.4) — the notes and coins a cashier counts. `remaining_due`,
//! `change_due` and `is_settled` are still to come with 1.5.2.
//!
//! Shape is [`ref/domain-api.md`](../../../docs/implementation/ref/domain-api.md) §7 and the
//! table is its §7.1.
//!
//! # The table came from the database, which is backwards on purpose
//!
//! Microstep 1.6.3 built the capability grid *first* and 1.6.1's migration
//! seeded from it. Here it is the other way round: `tender_type`'s complete
//! seed shipped inside migration `0005` at 1.9.1, and **a committed migration
//! is never reopened**. So [`standard_tender_types`] is written to match that
//! insert column for column, `the_standard_grid_matches_the_committed_seed`
//! pins it, and a disagreement is fixed here rather than there.
//!
//! The six behavioural columns are all this type carries. `tender_type` also
//! has `name_ar`, `name_en`, `sort_order` and `is_active` — a display name, an
//! ordering and a row state. The last is the one worth naming: the seed ships
//! `cash` active and the other five inactive, because *"later payment/refund
//! microsteps enable behavior; they never reopen 0005 to append codes"*. That
//! makes activation a property of one store's row rather than of a kind of
//! tender, and putting it here would invite a pure function to answer a
//! question only the database can.

use serde::{Deserialize, Serialize};

use crate::ids::TenderId;
use crate::money::{Currency, Money, MoneyError, RoundingDirection};

/// Where a refund of this tender is allowed to go.
///
/// `Same` routes back to the original instrument; `None` refuses, because an
/// internal or non-refundable tender has no destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefundRouting {
    Same,
    Cash,
    StoreCredit,
    None,
}

impl RefundRouting {
    /// Every variant, once. Written as a full `match` below rather than a
    /// `matches!`, so a fifth destination fails to compile until somebody
    /// decides whether money may go there.
    pub const ALL: [RefundRouting; 4] = [
        RefundRouting::Same,
        RefundRouting::Cash,
        RefundRouting::StoreCredit,
        RefundRouting::None,
    ];

    /// The `refundable_to` value `0005`'s `CHECK` constraint allows.
    ///
    /// The storage spelling lives here rather than in `pos-db`, because the
    /// constraint and this enum are the same closed set and a second copy is a
    /// second chance for them to drift.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            RefundRouting::Same => "same",
            RefundRouting::Cash => "cash",
            RefundRouting::StoreCredit => "store_credit",
            RefundRouting::None => "none",
        }
    }

    /// Whether a refund of this tender can be paid at all.
    #[must_use]
    pub const fn is_refundable(self) -> bool {
        match self {
            RefundRouting::Same | RefundRouting::Cash | RefundRouting::StoreCredit => true,
            RefundRouting::None => false,
        }
    }
}

/// One kind of tender, and what it does.
///
/// The `code` is the key `tender_type` is primary-keyed on and the one a
/// `Tender` names, so the two cannot be told apart by anything but spelling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenderType {
    /// See [`standard_tender_types`].
    pub code: String,
    pub opens_drawer: bool,
    pub allows_change: bool,
    /// Counts toward expected drawer cash at shift close.
    pub is_cash_counted: bool,
    pub refundable_to: RefundRouting,
    /// An internal tender moves value between two documents and never between
    /// the customer and the drawer. It is excluded from every takings figure,
    /// from the PSP reconciliation, and from expected cash.
    ///
    /// **This is not `!is_cash_counted`**, and `0005` says so in as many words:
    /// *"`is_internal` is NOT implied by `is_cash_counted = 0`: `card` and
    /// `cliq` count no drawer cash but do move real value through a PSP."*
    /// `exchange` carries both, or §7.1's failure follows — the offset appears
    /// in expected drawer cash on **both** documents and the shift closes short
    /// by twice the exchanged value.
    pub is_internal: bool,
}

/// The six kinds this product knows, exactly as migration `0005` seeded them.
///
/// `0005_sale_columns_and_sequences.sql:657-662` is the source, and it is a
/// committed migration — so if this function and that insert ever disagree,
/// **this function is wrong**. `the_standard_grid_matches_the_committed_seed`
/// is the test that notices.
///
/// `exchange` is the one that repays reading. *"Return + new sale, settling
/// only the difference"* needs value to pass from the refund document to the
/// sale document without cash or card moving, while each document still
/// balances on its own. With no tender that can carry the offset, the only
/// representable exchange is a full refund followed by a full payment: two PSP
/// round trips, two lines on the customer's statement, two fees.
#[must_use]
pub fn standard_tender_types() -> Vec<TenderType> {
    vec![
        TenderType {
            code: "cash".to_owned(),
            opens_drawer: true,
            allows_change: true,
            is_cash_counted: true,
            refundable_to: RefundRouting::Cash,
            is_internal: false,
        },
        TenderType {
            code: "card".to_owned(),
            opens_drawer: false,
            allows_change: false,
            is_cash_counted: false,
            refundable_to: RefundRouting::Same,
            is_internal: false,
        },
        TenderType {
            code: "cliq".to_owned(),
            opens_drawer: false,
            allows_change: false,
            is_cash_counted: false,
            refundable_to: RefundRouting::Same,
            is_internal: false,
        },
        TenderType {
            code: "voucher".to_owned(),
            opens_drawer: false,
            allows_change: false,
            is_cash_counted: false,
            refundable_to: RefundRouting::None,
            is_internal: false,
        },
        TenderType {
            code: "store_credit".to_owned(),
            opens_drawer: false,
            allows_change: false,
            is_cash_counted: false,
            refundable_to: RefundRouting::StoreCredit,
            is_internal: false,
        },
        TenderType {
            code: "exchange".to_owned(),
            opens_drawer: false,
            allows_change: false,
            is_cash_counted: false,
            refundable_to: RefundRouting::None,
            is_internal: true,
        },
    ]
}

/// The standard kind with this code, if there is one.
///
/// Returns `None` for a code this build does not know rather than a default:
/// a tender whose kind cannot be resolved is one whose drawer, change and
/// refund behaviour are all unknown, and guessing any of the three is worse
/// than refusing.
#[must_use]
pub fn standard_tender_type(code: &str) -> Option<TenderType> {
    standard_tender_types().into_iter().find(|t| t.code == code)
}

/// What one collected tender is worth and where it came from.
///
/// The card fields are the whole of what a card may leave behind:
/// `.claude/rules/security.md` — *"never store anything from a card except the
/// PSP reference, the masked PAN the terminal returns for the receipt, and the
/// scheme."* There is no fourth field and there must never be one.
///
/// `Debug` is **derived**, deliberately, and consistently with 1.7.1's
/// `ReceiptTender`: `ref/security-compliance.md` §6's registry names `pan` and
/// `card_number`, a masked value is neither and matches no suffix rule, and
/// redacting it here would put the two types out of step for no protection
/// while removing the one field a tender mismatch is diagnosed by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tender {
    pub id: TenderId,
    /// A [`TenderType::code`].
    pub type_code: String,
    pub amount: Money,
    /// The PSP's own reference, for reconciliation and refunds.
    pub psp_ref: Option<String>,
    /// Receipt only. Nothing else from the card. Ever.
    pub masked_pan: Option<String>,
    pub scheme: Option<String>,
    pub state: TenderState,
}

/// Where one tender has got to.
///
/// **A projection, not a column anyone updates.** A tender is inserted
/// `Pending` or `Collected` and later settles or reverses; `sale_tender` is a
/// fact table on a completed sale, so the transition is an appended
/// `tender_status_event` and this value is the fold over that tender's events.
/// Modelling it as a mutable field is what left the local register updating a
/// row the server had revoked `UPDATE` on
/// ([`00-master-plan.md`](../../../docs/implementation/00-master-plan.md) §4a).
///
/// The fold itself is not here. It reads storage rows, and no reference gives
/// it a signature; this module owns the vocabulary the rows are written in.
///
/// `Pending` exists **from day one** so Phase 2's CliQ callback case (E.65) is
/// a state rather than a schema change — a tender that has been initiated and
/// not yet confirmed is a thing that happens on the first day CliQ is enabled,
/// and adding it later would mean a migration on a fact table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenderState {
    Collected,
    Pending,
    Unknown,
    Failed,
    Reversed,
}

impl TenderState {
    /// Every variant, once, in the order `tender_status_event`'s `CHECK`
    /// constraint lists them.
    pub const ALL: [TenderState; 5] = [
        TenderState::Pending,
        TenderState::Collected,
        TenderState::Reversed,
        TenderState::Unknown,
        TenderState::Failed,
    ];

    /// The `state` value `0005`'s `CHECK` constraint allows.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            TenderState::Collected => "collected",
            TenderState::Pending => "pending",
            TenderState::Unknown => "unknown",
            TenderState::Failed => "failed",
            TenderState::Reversed => "reversed",
        }
    }

    /// Whether a tender in this state has money behind it **right now**.
    ///
    /// `Unknown` answers `false`, and that is the whole point of the variant: a
    /// timed-out authorisation may well have taken the customer's money, and
    /// treating it as collected settles a sale against money nobody has
    /// confirmed. It is resolved by asking the PSP, which is 2.1.3's.
    #[must_use]
    pub const fn is_collected(self) -> bool {
        match self {
            TenderState::Collected => true,
            TenderState::Pending
            | TenderState::Unknown
            | TenderState::Failed
            | TenderState::Reversed => false,
        }
    }
}

/// What a final cash tender is asked for, and the signed difference from what
/// was owed (master plan B.5).
///
/// Cash rounding exists because an amount paid in coin has to be one the coins
/// in use can make, and `0.623` is not. The step is the store's operational
/// choice — one qirsh, ten fils, is the provisional default (merchant decision
/// 2.1) — and `ref/tax-jordan.md` §5 is explicit that it is not a claim about
/// legal tender or tax law. The final cash tender is asked for a payable
/// amount, and the difference is kept as a figure of its own rather than
/// absorbed anywhere, so the books still reconcile to the fil (§5, rule 3):
///
/// ```text
/// Σ tenders − change == total + adjustment
/// ```
///
/// `adjustment` is what `sale.rounding_adj_minor` persists and what the
/// receipt prints as its own line. Under the provisional default it moves no
/// line, no line tax component and no tax summary row: whether a cash-rounding
/// adjustment changes taxable consideration or a JoFotara total is an OPEN
/// item owned by 2.7.0, and until it is answered this is only a signed
/// tender-level amount.
///
/// The fields are public because §7 specifies them so. [`compute_cash_rounding`]
/// is the constructor that keeps `original + adjustment == rounded`, and
/// `prop_rounding_adjustment_keeps_total_exact` is what holds it to that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashRounding {
    /// The exact remainder the earlier tenders left.
    pub original: Money,
    /// What the final cash tender is asked for: `original`, moved to the coin
    /// step.
    pub rounded: Money,
    /// `rounded − original`. Negative when the customer is asked for less than
    /// was owed, positive when for more, and zero when the remainder was
    /// already payable.
    pub adjustment: Money,
}

/// Round what the earlier tenders left to the store's coin step (master plan
/// B.5), and record by how much.
///
/// `step_minor` is in the remainder's own minor units — `10` is one qirsh, the
/// provisional merchant default (merchant decision 2.1) — and `dir` is the
/// store's direction (2.2, default `Nearest`, which breaks an exact half-step
/// tie away from zero, so `1.245` is asked as `1.250`). Both are the store's
/// policy and arrive as arguments, never as a read (I-8).
///
/// **This computes a rounding; it does not decide that one applies.** Cash
/// rounding applies only when the *final* tender is cash (E.14), and a card is
/// charged the exact amount. [`final_tender_rounding`] is that rule, and this
/// is the arithmetic under it.
///
/// # Errors
///
/// - [`MoneyError::Negative`] for a negative remainder. A sale's remainder is
///   never negative: a negative balance is **change**, and change is never
///   rounded — it is what the drawer hands back from an amount that already
///   was. A cash *refund* payout is `compute_refund_rounding`, 2.3.3's, with
///   its own direction default; it is not this function.
/// - [`MoneyError::InvalidStep`] for a step that is not positive, which
///   `tax_computation_policy`'s `CHECK (cash_round_step_minor > 0)` refuses
///   too.
/// - [`MoneyError::Overflow`] when the step multiple lies outside `i64`.
pub fn compute_cash_rounding(
    remaining: Money,
    step_minor: i64,
    dir: RoundingDirection,
) -> Result<CashRounding, MoneyError> {
    refuse_unroundable(remaining, step_minor)?;
    let rounded = remaining.round_to_step(step_minor, dir)?;
    let adjustment = rounded.checked_sub(remaining)?;
    Ok(CashRounding {
        original: remaining,
        rounded,
        adjustment,
    })
}

/// What the tender that settles a sale is asked for (master plan B.5, E.14).
///
/// Cash rounding applies **only when the final tender is cash, and only to
/// what the earlier tenders left**. So a cash `kind` gets the
/// [`CashRounding`] of `remaining`, and every other kind gets `None` and is
/// asked for `remaining` exactly — a card is charged the exact, unrounded
/// amount. The earlier tenders never pass through here, which is what keeps
/// them exact and makes "cash rounds once" structural rather than remembered.
///
/// **"Cash" means [`TenderType::is_cash_counted`], not the code `"cash"`.**
/// Rounding exists because an amount paid in coin has to be one the coins can
/// make, and `is_cash_counted` is the flag that makes a tender's amount coin:
/// `ref/domain-api.md` §11 counts `amount − change` into
/// expected drawer cash over exactly those tenders — which is also why that
/// formula needs no rounding term of its own, since the rounded amount arrives
/// as the tender's amount. Every other kind moves its amount electronically or
/// on paper and can carry any fil, so rounding it would charge the customer
/// fils nothing required. `ref/tax-jordan.md` §5 rule 5 pairs the two in one
/// sentence: the `exchange` tender *"is never cash-counted and never receives
/// cash rounding"*.
///
/// **This answers "if this tender settles the sale"; it does not decide that
/// it does.** A cash tender that covers `rounded` is final. One that does not
/// is a partial cash tender, applied exactly, and the next tender is asked
/// again. Making that call is `add_tender`'s (1.4.8), and the `Some` returned
/// here is exactly what §7's `Tendering.cash_rounding` stores.
///
/// A zero adjustment is still `Some`: the tender was cash and nothing moved.
/// The receipt model refuses a zero rounding *line* (1.7.1), so whatever maps
/// a sale into it keeps the line only when the adjustment is non-zero.
///
/// # Errors
///
/// The same as [`compute_cash_rounding`], **for every kind**. A non-positive
/// step or a negative remainder is refused even where no rounding would
/// happen, so a policy that cannot round is found by the first sale rather
/// than by the first cash sale.
pub fn final_tender_rounding(
    kind: &TenderType,
    remaining: Money,
    step_minor: i64,
    dir: RoundingDirection,
) -> Result<Option<CashRounding>, MoneyError> {
    if kind.is_cash_counted {
        compute_cash_rounding(remaining, step_minor, dir).map(Some)
    } else {
        refuse_unroundable(remaining, step_minor)?;
        Ok(None)
    }
}

/// The two requests no coin step can serve, refused in one place so the
/// rounding and the rule that decides whether to round cannot disagree about
/// them.
fn refuse_unroundable(remaining: Money, step_minor: i64) -> Result<(), MoneyError> {
    if step_minor <= 0 {
        return Err(MoneyError::InvalidStep(step_minor));
    }
    if remaining.is_negative() {
        return Err(MoneyError::Negative);
    }
    Ok(())
}

/// The dinar's notes and coins, largest first, as the plan lists them.
///
/// Fifty, twenty, ten, five and one dinar; 500, 250, 100, 50, 25 and 10 fils.
/// Written as minor units against [`Currency::JOD`], whose exponent is three,
/// so one dinar is `1_000` — the value `100` would be the classic mistake I-2
/// exists to prevent.
const JOD_DENOMINATIONS: [Money; 11] = [
    Money::from_minor(50_000, Currency::JOD),
    Money::from_minor(20_000, Currency::JOD),
    Money::from_minor(10_000, Currency::JOD),
    Money::from_minor(5_000, Currency::JOD),
    Money::from_minor(1_000, Currency::JOD),
    Money::from_minor(500, Currency::JOD),
    Money::from_minor(250, Currency::JOD),
    Money::from_minor(100, Currency::JOD),
    Money::from_minor(50, Currency::JOD),
    Money::from_minor(25, Currency::JOD),
    Money::from_minor(10, Currency::JOD),
];

/// The notes and coins a cashier counts and hands over in `currency`,
/// largest first — the numpad's quick-keys, and the denomination grid a shift
/// opens with (float entry) and closes with (the blind count).
///
/// **`None` means "no helper", never an error.** Master plan E.17 is the
/// reason: *"system doesn't care about denominations for correctness, but
/// count helper does"*, so the numpad works without quick-keys and the count
/// grids without rows. What `None` must never become is the dinar's table
/// borrowed: a USD amount has two minor digits, and ten fils read as ten cents
/// is off by a factor of ten (I-2).
///
/// **The smallest piece is ten fils, one qirsh**, merchant decision 2.1's
/// default cash step, so every amount on that step can be counted. A store
/// whose answer to 2.1 is a five-fil step needs a five-fil row, and that change
/// is made here together with `the_smallest_denomination_is_one_qirsh`, not by
/// one silent edit.
///
/// **The plan's set is not consistent with a ten-fil step, and that is recorded
/// rather than repaired.** Its 25-fil piece is not a whole number of qirsh: a
/// customer who hands one over against a remainder rounded to `0.020` is owed
/// five fils of change, and no piece here pays it. Either the 25-fil piece is
/// not in everyday use or the 5-fil piece is and belongs here. That is master
/// plan B.5's standing *"Verify the store's actual coin practice with the
/// merchant"*, and issue #237 carries it. The values stay the plan's until
/// then.
///
/// **This is not a change-maker, and nothing may use it as one.** With 25 fils
/// in the table and no 5, counting out largest-first strands a remainder no
/// coin can cover: 30 fils takes 25 and leaves 5, where the answer is
/// 10 + 10 + 10. Measured, 80 of the 200 qirsh-multiples up to 2.000 JOD fail
/// that way. Quick-keys add amounts and the count grid counts them, so neither
/// needs change made. Anything that ever suggests change in coins needs an
/// exact algorithm, not a greedy pass over this list.
#[must_use]
pub fn denominations(currency: Currency) -> Option<&'static [Money]> {
    if currency == Currency::JOD {
        Some(&JOD_DENOMINATIONS)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::money::Currency;
    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;
    use uuid::Uuid;

    use RoundingDirection::{Down, Nearest, Up};

    /// One qirsh — ten fils — the provisional merchant default step
    /// (`ref/merchant-decisions.md` 2.1).
    const QIRSH: i64 = 10;

    /// The three directions merchant decision 2.2 offers, each once.
    const DIRECTIONS: [RoundingDirection; 3] = [Nearest, Up, Down];

    /// Migration `0005_sale_columns_and_sequences.sql:657-662`, transcribed.
    ///
    /// The six behavioural columns of every seeded row, in the migration's own
    /// order, as literals. Deliberately **not** built from
    /// [`standard_tender_types`] — a fixture derived from the thing it checks
    /// asserts nothing. These are the numbers a reader can diff against the
    /// `.sql` by eye.
    const COMMITTED_SEED: [(&str, bool, bool, bool, &str, bool); 6] = [
        // code            drawer change counted refundable_to    internal
        ("cash", true, true, true, "cash", false),
        ("card", false, false, false, "same", false),
        ("cliq", false, false, false, "same", false),
        ("voucher", false, false, false, "none", false),
        ("store_credit", false, false, false, "store_credit", false),
        ("exchange", false, false, false, "none", true),
    ];

    /// The grid is the migration's, and the migration cannot be reopened.
    ///
    /// 1.6.3 built its grid first and seeded from it. This one is the reverse:
    /// `0005` shipped at 1.9.1 and a committed migration is never reopened, so
    /// if these two disagree the **domain type** is what changes.
    #[test]
    fn the_standard_grid_matches_the_committed_seed() {
        let grid = standard_tender_types();
        assert_eq!(grid.len(), COMMITTED_SEED.len(), "a row was added or lost");

        for (kind, (code, drawer, change, counted, routing, internal)) in
            grid.iter().zip(COMMITTED_SEED)
        {
            assert_eq!(kind.code, code, "order differs from the seed");
            assert_eq!(kind.opens_drawer, drawer, "{code}: opens_drawer");
            assert_eq!(kind.allows_change, change, "{code}: allows_change");
            assert_eq!(kind.is_cash_counted, counted, "{code}: is_cash_counted");
            assert_eq!(
                kind.refundable_to.as_str(),
                routing,
                "{code}: refundable_to"
            );
            assert_eq!(kind.is_internal, internal, "{code}: is_internal");
        }
    }

    #[test]
    fn only_cash_opens_the_drawer_and_gives_change() {
        for kind in standard_tender_types() {
            let is_cash = kind.code == "cash";
            assert_eq!(kind.opens_drawer, is_cash, "{}: opens_drawer", kind.code);
            assert_eq!(kind.allows_change, is_cash, "{}: allows_change", kind.code);
        }

        // Change is only ever given from a drawer that opened. Stated as an
        // implication rather than as a list, so a seventh kind that gives
        // change without opening a drawer fails here rather than in a shift
        // that closes short.
        for kind in standard_tender_types() {
            assert!(
                !kind.allows_change || kind.opens_drawer,
                "{} gives change without opening a drawer",
                kind.code
            );
        }
    }

    /// The distinction `0005`'s own comment spells out, held so a later
    /// refactor cannot collapse the two flags into one.
    #[test]
    fn an_internal_tender_is_not_merely_uncounted_cash() {
        let grid = standard_tender_types();

        let uncounted: Vec<&str> = grid
            .iter()
            .filter(|t| !t.is_cash_counted)
            .map(|t| t.code.as_str())
            .collect();
        let internal: Vec<&str> = grid
            .iter()
            .filter(|t| t.is_internal)
            .map(|t| t.code.as_str())
            .collect();

        assert_eq!(internal, vec!["exchange"], "exactly one kind is internal");
        assert_eq!(
            uncounted,
            vec!["card", "cliq", "voucher", "store_credit", "exchange"],
            "five kinds count no drawer cash"
        );

        // `card` and `cliq` are the witnesses: no drawer cash, and real value
        // through a PSP. If `is_internal` ever became `!is_cash_counted`, these
        // two would be excluded from the PSP reconciliation.
        for code in ["card", "cliq"] {
            let kind = standard_tender_type(code).expect("a seeded kind");
            assert!(!kind.is_cash_counted, "{code} counts no drawer cash");
            assert!(!kind.is_internal, "{code} moves real value");
        }

        // And an internal tender never counts, which is the half of §7.1 that
        // keeps a shift from closing short by twice the exchanged value.
        for kind in grid.iter().filter(|t| t.is_internal) {
            assert!(
                !kind.is_cash_counted,
                "{} is internal and counted — the offset would appear on both documents",
                kind.code
            );
        }
    }

    #[test]
    fn an_internal_tender_can_be_refunded_nowhere() {
        for kind in standard_tender_types().iter().filter(|t| t.is_internal) {
            assert_eq!(kind.refundable_to, RefundRouting::None, "{}", kind.code);
            assert!(!kind.refundable_to.is_refundable());
        }

        // The routings that do pay, so the predicate is not vacuously false.
        assert!(RefundRouting::Same.is_refundable());
        assert!(RefundRouting::Cash.is_refundable());
        assert!(RefundRouting::StoreCredit.is_refundable());
        assert!(!RefundRouting::None.is_refundable());
    }

    #[test]
    fn every_standard_code_is_unique_and_resolvable() {
        let grid = standard_tender_types();

        let mut codes: Vec<&str> = grid.iter().map(|t| t.code.as_str()).collect();
        let total = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), total, "two kinds share a code");

        for kind in &grid {
            assert_eq!(
                standard_tender_type(&kind.code).as_ref(),
                Some(kind),
                "{} does not resolve to itself",
                kind.code
            );
        }

        // A code this build does not know resolves to nothing rather than to a
        // default whose drawer, change and refund behaviour would all be
        // guesses.
        assert_eq!(standard_tender_type("bitcoin"), None);
        assert_eq!(standard_tender_type(""), None);
        assert_eq!(standard_tender_type("CASH"), None, "codes are exact");
    }

    /// E.65: Phase 2's CliQ callback is a state, not a schema change.
    #[test]
    fn tender_state_carries_pending_from_day_one() {
        assert!(TenderState::ALL.contains(&TenderState::Pending));
        assert_eq!(TenderState::ALL.len(), 5);

        // The storage spellings are `tender_status_event`'s CHECK constraint.
        let spellings: Vec<&str> = TenderState::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            spellings,
            vec!["pending", "collected", "reversed", "unknown", "failed"]
        );

        // Only one state means money is behind the tender now. `Unknown` in
        // particular is not collected: a timed-out authorisation may well have
        // taken the customer's money, and settling a sale against it settles
        // against money nobody has confirmed.
        for state in TenderState::ALL {
            assert_eq!(
                state.is_collected(),
                state == TenderState::Collected,
                "{state:?}"
            );
        }
    }

    /// `.claude/rules/security.md`: never store anything from a card except the
    /// PSP reference, the masked PAN the terminal returns for the receipt, and
    /// the scheme.
    ///
    /// The field list is held through the wire form, because that is what a
    /// fourth field would have to pass through to be stored — and a `Debug`
    /// rendering cannot show a field that does not exist.
    #[test]
    fn a_tender_carries_only_what_a_card_may_leave_behind() {
        let tender = card_tender();
        let json = serde_json::to_value(&tender).expect("serialises");
        let object = json.as_object().expect("a tender is an object");

        let mut fields: Vec<&str> = object.keys().map(String::as_str).collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            vec![
                "amount",
                "id",
                "masked_pan",
                "psp_ref",
                "scheme",
                "state",
                "type_code"
            ],
            "a field arrived or left"
        );

        // The three card fields, and nothing that could be a PAN.
        assert_eq!(
            object.get("masked_pan").and_then(|v| v.as_str()),
            Some("**** **** **** 4242")
        );
        assert!(!json.to_string().contains("4242424242424242"));

        // `Debug` is derived and the masked form stays visible, consistently
        // with 1.7.1's `ReceiptTender`: the §6 registry names `pan` and
        // `card_number`, and a masked value is neither.
        assert!(format!("{tender:?}").contains("4242"));
    }

    #[test]
    fn a_tender_round_trips_through_canonical_json() {
        for tender in [card_tender(), cash_tender()] {
            let json = serde_json::to_string(&tender).expect("serialises");
            let back: Tender = serde_json::from_str(&json).expect("deserialises");
            assert_eq!(back, tender);
        }

        // The wire spellings are the storage ones, so a state written by this
        // build reads back through `tender_status_event`'s CHECK constraint.
        let json = serde_json::to_string(&cash_tender()).expect("serialises");
        assert!(json.contains(r#""state":"collected""#), "{json}");
    }

    /// Every routing the grid uses is one the enum knows, and every variant is
    /// reachable from a `match` rather than from a wildcard.
    #[test]
    fn the_grid_is_exhaustive_over_every_routing_it_uses() {
        for kind in standard_tender_types() {
            assert!(
                RefundRouting::ALL.contains(&kind.refundable_to),
                "{} routes somewhere the enum does not list",
                kind.code
            );
        }

        // The storage spellings are `tender_type`'s CHECK constraint, in its
        // order, and all four are distinct.
        let spellings: Vec<&str> = RefundRouting::ALL.iter().map(|r| r.as_str()).collect();
        assert_eq!(spellings, vec!["same", "cash", "store_credit", "none"]);
        assert_eq!(RefundRouting::ALL.len(), 4);

        // Three of the four are used by a seeded kind; `Cash` and `Same` and
        // `StoreCredit` and `None` all appear, so no variant is decoration.
        let used: Vec<RefundRouting> = standard_tender_types()
            .iter()
            .map(|t| t.refundable_to)
            .collect();
        for routing in RefundRouting::ALL {
            assert!(used.contains(&routing), "{routing:?} is unused by any kind");
        }
    }

    /// The storage spelling and the wire spelling are the same mapping written
    /// twice, and nothing made them agree.
    ///
    /// `as_str` exists for a SQL bind and `#[serde(rename_all = "snake_case")]`
    /// produces the JSON form; they happen to coincide, which is exactly the
    /// condition under which a change to one goes unnoticed. Both are checked
    /// against `0005`'s `CHECK` constraints in the tests above, so this asserts
    /// the third edge of the triangle: the two definitions inside this module
    /// agree with each other.
    ///
    /// The same pair exists in `catalog.rs` and `permissions.rs` and is
    /// unchecked there. Noted, not fixed — those are other microsteps' files.
    #[test]
    fn the_wire_form_and_the_storage_form_are_the_same_word() {
        for routing in RefundRouting::ALL {
            let wire = serde_json::to_value(routing).expect("serialises");
            assert_eq!(wire.as_str(), Some(routing.as_str()), "{routing:?}");
        }
        for state in TenderState::ALL {
            let wire = serde_json::to_value(state).expect("serialises");
            assert_eq!(wire.as_str(), Some(state.as_str()), "{state:?}");
        }
    }

    fn card_tender() -> Tender {
        Tender {
            id: TenderId::from_uuid(Uuid::from_u128(1)),
            type_code: "card".to_owned(),
            amount: Money::from_minor(2_163, Currency::JOD),
            psp_ref: Some("PSP-000123".to_owned()),
            masked_pan: Some("**** **** **** 4242".to_owned()),
            scheme: Some("visa".to_owned()),
            state: TenderState::Collected,
        }
    }

    fn cash_tender() -> Tender {
        Tender {
            id: TenderId::from_uuid(Uuid::from_u128(2)),
            type_code: "cash".to_owned(),
            amount: Money::from_minor(2_163, Currency::JOD),
            psp_ref: None,
            masked_pan: None,
            scheme: None,
            state: TenderState::Collected,
        }
    }

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, Currency::JOD)
    }

    /// The phase entry's eleven, transcribed as literals rather than built
    /// from the table they check: 50, 20, 10, 5 and 1 dinar; 500, 250, 100,
    /// 50, 25 and 10 fils.
    const PLANNED_DINAR_TABLE: [i64; 11] = [
        50_000, 20_000, 10_000, 5_000, 1_000, 500, 250, 100, 50, 25, 10,
    ];

    #[test]
    fn denominations_are_descending_and_complete() {
        let table = denominations(Currency::JOD).expect("the dinar has a table");

        // Complete, and in order: the plan's eleven, value for value.
        let minor: Vec<i64> = table.iter().map(|d| d.minor()).collect();
        assert_eq!(minor, PLANNED_DINAR_TABLE);

        // Largest first, strictly. A repeated value would be a count-grid row
        // counted twice, and a quick-key the cashier cannot tell from its
        // neighbour.
        for (larger, smaller) in minor.iter().zip(minor.iter().skip(1)) {
            assert!(larger > smaller, "{larger} is not above {smaller}");
        }

        // Every entry is a positive dinar amount — `shift_count_line` refuses a
        // denomination that is zero or negative, and a fils value tagged with
        // another currency would carry the wrong exponent.
        for d in table {
            assert_eq!(d.currency(), Currency::JOD, "{}", d.format_exact());
            assert!(d.minor() > 0, "{}", d.format_exact());
        }
    }

    /// Merchant decision 2.1's default step is one qirsh, ten fils, and the
    /// table reaches it: the smallest piece is exactly one qirsh, so every
    /// amount on that step can be counted into the grid.
    ///
    /// This test was first written as "every denomination is a whole number of
    /// qirsh", and the plan's own table refused it — 25 fils is not. The
    /// inconsistency is recorded on `denominations` and in #237 rather than
    /// asserted here, because a test of a hole has to be deleted the day the
    /// hole closes. A store whose answer to 2.1 is a five-fil step changes this
    /// test and the table together.
    #[test]
    fn the_smallest_denomination_is_one_qirsh() {
        let table = denominations(Currency::JOD).expect("the dinar has a table");
        assert_eq!(table.iter().map(|d| d.minor()).min(), Some(QIRSH));
    }

    /// I-2: the exponent is per-currency data, and so is the table. A currency
    /// with no table gets no helper; it never gets the dinar's.
    #[test]
    fn no_other_currency_borrows_the_dinar_denominations() {
        assert_eq!(denominations(Currency::USD), None);
        assert_eq!(denominations(Currency::EUR), None);

        // And the table that does exist is written in its own currency's
        // exponent: the one-dinar piece is exactly one major unit, 1000 fils,
        // not the 100 a two-decimal habit would write.
        let one_dinar = Money::from_minor(Currency::JOD.minor_per_major(), Currency::JOD);
        assert!(
            denominations(Currency::JOD)
                .expect("the dinar has a table")
                .contains(&one_dinar),
            "one dinar is {} fils",
            Currency::JOD.minor_per_major()
        );
    }

    fn seeded(code: &str) -> TenderType {
        standard_tender_type(code).expect("a seeded kind")
    }

    /// The Done-when, figure for figure: a `1.247` sale, `0.624` of it on a
    /// card, and cash for the rest.
    #[test]
    fn mixed_tender_1247_card_624_cash_620_adjustment_minus_3() {
        let total = jod(1_247);
        let card = jod(624);

        // The card is not the final tender, so it is charged exactly what
        // the cashier asked of it and leaves the rest exact.
        let remaining = total.checked_sub(card).unwrap();
        assert_eq!(remaining, jod(623));

        let rounding = final_tender_rounding(&seeded("cash"), remaining, QIRSH, Nearest)
            .unwrap()
            .expect("a final cash tender is rounded");
        assert_eq!(
            rounding,
            CashRounding {
                original: jod(623),
                rounded: jod(620),
                adjustment: jod(-3),
            }
        );

        // Settled exactly: what was collected is the total plus the one
        // recorded adjustment, and not a fil appears or disappears between
        // them (`ref/tax-jordan.md` §5, rule 3).
        let settled = card.checked_add(rounding.rounded).unwrap();
        assert_eq!(settled, jod(1_244));
        assert_eq!(settled, total.checked_add(rounding.adjustment).unwrap());

        // The same figures as a receipt prints them, at the currency's own
        // exponent (I-2) — a three-fil line at two decimals would read 0.00.
        assert_eq!(remaining.format_exact(), "0.623");
        assert_eq!(rounding.rounded.format_exact(), "0.620");
        assert_eq!(rounding.adjustment.format_exact(), "-0.003");
        assert_eq!(settled.format_exact(), "1.244");
    }

    /// E.14, the half a card owns: charged the exact amount, never a rounded
    /// one, whatever step or direction the store chose.
    #[test]
    fn card_charged_exact_unrounded_total() {
        let card = seeded("card");

        // The whole sale on one card: 1.247 is charged as 1.247, not 1.250.
        for step in [1, 5, QIRSH, 50, 100, 1_000] {
            for dir in DIRECTIONS {
                assert_eq!(
                    final_tender_rounding(&card, jod(1_247), step, dir),
                    Ok(None),
                    "step {step}, {dir:?}"
                );
            }
        }

        // And as the final tender after cash went first: exact again. The
        // cash was not final, so it was applied exactly too.
        let remaining = jod(1_247).checked_sub(jod(1_000)).unwrap();
        assert_eq!(
            final_tender_rounding(&card, remaining, QIRSH, Nearest),
            Ok(None)
        );
    }

    /// An exact half-step tie goes away from zero.
    ///
    /// `1.245` is the case that separates the rules: banker's rounding asks
    /// for `1.240` because 124 qirsh is even, and B.5's default asks for
    /// `1.250`. `1.235` separates it from rounding half *down*, which would
    /// ask for `1.230`. Between them the two rule out both of the other tie
    /// rules a plausible implementation reaches for.
    #[test]
    fn half_away_tie_1245_rounds_to_1250() {
        let r = compute_cash_rounding(jod(1_245), QIRSH, Nearest).unwrap();
        assert_eq!(
            r,
            CashRounding {
                original: jod(1_245),
                rounded: jod(1_250),
                adjustment: jod(5),
            }
        );
        assert_eq!(r.rounded.format_exact(), "1.250");

        let r = compute_cash_rounding(jod(1_235), QIRSH, Nearest).unwrap();
        assert_eq!(r.rounded, jod(1_240));

        // And through the rule a cash tender actually meets.
        let r = final_tender_rounding(&seeded("cash"), jod(1_245), QIRSH, Nearest).unwrap();
        assert_eq!(r.map(|r| r.rounded), Some(jod(1_250)));
    }

    /// The customer hands over more than is asked. The rounding is decided by
    /// the remainder alone, before any cash is counted; the change is what the
    /// drawer hands back from the *rounded* amount. Two figures, and neither
    /// absorbs the other.
    ///
    /// Rounding the change instead would invert the store's direction: a
    /// store that chose `Up` would be rounding in the customer's favour, and
    /// every tie would go the other way. The remainder here is a tie so that
    /// the two readings disagree — `2.000 − 1.245` is a change of `0.755`,
    /// which rounds away from zero to `0.760`, where the right answer is
    /// `2.000 − 1.250 = 0.750`.
    #[test]
    fn cash_overtender_and_change_are_separate_from_rounding() {
        let cash = seeded("cash");
        let remaining = jod(1_245);
        let rounding = final_tender_rounding(&cash, remaining, QIRSH, Nearest)
            .unwrap()
            .expect("a final cash tender is rounded");
        assert_eq!(rounding.adjustment, jod(5));

        for (handed, change) in [(1_250, 0), (2_000, 750), (5_000, 3_750)] {
            // The subtraction is `change_due`'s (1.5.2); what this pins is the
            // amount it subtracts from — the rounded one.
            let given = jod(handed).checked_sub(rounding.rounded).unwrap();
            assert_eq!(given, jod(change), "handed {handed}");
            assert_eq!(given.minor() % QIRSH, 0, "change a drawer can pay");

            // What stays in the drawer is the remainder plus the adjustment,
            // however large the note.
            assert_eq!(
                jod(handed).checked_sub(given).unwrap(),
                remaining.checked_add(rounding.adjustment).unwrap()
            );
        }

        // The balance after an over-tender is change, and change is refused
        // rather than rounded — there is no way to hand it to either function.
        let balance = remaining.checked_sub(jod(2_000)).unwrap();
        assert_eq!(balance, jod(-755));
        assert_eq!(
            compute_cash_rounding(balance, QIRSH, Nearest),
            Err(MoneyError::Negative)
        );
        assert_eq!(
            final_tender_rounding(&cash, balance, QIRSH, Nearest),
            Err(MoneyError::Negative)
        );
    }

    /// The rule keys on `is_cash_counted`. On today's grid that cannot be told
    /// apart from `code == "cash"`, `opens_drawer` or `allows_change` — all
    /// four select the same row — so two constructed kinds pull them apart,
    /// and the choice is held by a test rather than by a coincidence of the
    /// seed.
    #[test]
    fn rounding_follows_the_drawer_count_not_the_code() {
        for kind in standard_tender_types() {
            let rounding = final_tender_rounding(&kind, jod(1_247), QIRSH, Nearest).unwrap();
            assert_eq!(rounding.is_some(), kind.is_cash_counted, "{}", kind.code);
        }

        // `ref/tax-jordan.md` §5 rule 5, by name: the internal tender is
        // never cash-counted and never receives cash rounding.
        assert_eq!(
            final_tender_rounding(&seeded("exchange"), jod(1_247), QIRSH, Nearest),
            Ok(None)
        );

        // Counted into the drawer under another name, opening nothing and
        // giving no change: rounded.
        let counted = TenderType {
            code: "coins".to_owned(),
            opens_drawer: false,
            allows_change: false,
            is_cash_counted: true,
            refundable_to: RefundRouting::Cash,
            is_internal: false,
        };
        assert!(
            final_tender_rounding(&counted, jod(1_247), QIRSH, Nearest)
                .unwrap()
                .is_some()
        );

        // Called "cash", opening the drawer and giving change, but counted
        // nowhere: not rounded, because an amount that never reaches the
        // drawer is never paid in coin and can carry any fil.
        let uncounted = TenderType {
            code: "cash".to_owned(),
            opens_drawer: true,
            allows_change: true,
            is_cash_counted: false,
            refundable_to: RefundRouting::Cash,
            is_internal: false,
        };
        assert_eq!(
            final_tender_rounding(&uncounted, jod(1_247), QIRSH, Nearest),
            Ok(None)
        );
    }

    /// Merchant decision 2.2 offers three directions, and each moves the
    /// remainder the way its name says — `Up` asks for more, `Down` for less,
    /// `Nearest` for whichever payable amount is closer.
    ///
    /// The last row is the one a cashier will meet: a three-fil remainder is
    /// asked as nothing at all under `Nearest` and `Down`, and the adjustment
    /// alone settles it.
    #[test]
    fn each_direction_moves_the_remainder_its_own_way() {
        let rows = [
            // remaining, Nearest, Up, Down
            (1_241, 1_240, 1_250, 1_240),
            (1_249, 1_250, 1_250, 1_240),
            (1_245, 1_250, 1_250, 1_240),
            (3, 0, 10, 0),
        ];
        for (remaining, nearest, up, down) in rows {
            for (dir, expected) in [(Nearest, nearest), (Up, up), (Down, down)] {
                let r = compute_cash_rounding(jod(remaining), QIRSH, dir).unwrap();
                assert_eq!(r.rounded, jod(expected), "{remaining} {dir:?}");
                assert_eq!(
                    r.adjustment,
                    jod(expected - remaining),
                    "{remaining} {dir:?}"
                );
            }
        }
    }

    /// A remainder already on the step is asked for exactly, by every
    /// direction — and a cash tender still reports `Some`.
    #[test]
    fn an_already_payable_remainder_is_rounded_by_nothing() {
        for remaining in [0, 620, 1_000_000] {
            for dir in DIRECTIONS {
                assert_eq!(
                    compute_cash_rounding(jod(remaining), QIRSH, dir),
                    Ok(CashRounding {
                        original: jod(remaining),
                        rounded: jod(remaining),
                        adjustment: jod(0),
                    }),
                    "{remaining} {dir:?}"
                );
            }
        }

        // `Some` with a zero adjustment, not `None`: the tender was cash and
        // nothing moved. 1.7.1's receipt model refuses a zero rounding line
        // (`a_zero_rounding_adjustment_is_absent_rather_than_zero`), so the
        // mapping into it is where the zero is dropped — not here.
        let r = final_tender_rounding(&seeded("cash"), jod(620), QIRSH, Nearest).unwrap();
        assert_eq!(r.map(|r| r.adjustment), Some(jod(0)));
    }

    /// Requests no coin step can serve are refused, and refused by every kind
    /// — so a policy that cannot round fails the first sale, not the first
    /// cash one.
    #[test]
    fn an_unroundable_request_is_refused_whatever_the_tender() {
        for kind in standard_tender_types() {
            for step in [0, -10] {
                assert_eq!(
                    final_tender_rounding(&kind, jod(1_247), step, Nearest),
                    Err(MoneyError::InvalidStep(step)),
                    "{} with step {step}",
                    kind.code
                );
            }
            assert_eq!(
                final_tender_rounding(&kind, jod(-3), QIRSH, Nearest),
                Err(MoneyError::Negative),
                "{} with a negative remainder",
                kind.code
            );
        }

        assert_eq!(
            compute_cash_rounding(jod(1_247), 0, Nearest),
            Err(MoneyError::InvalidStep(0))
        );
        assert_eq!(
            compute_cash_rounding(jod(-1), QIRSH, Nearest),
            Err(MoneyError::Negative)
        );

        // There is no multiple of ten above the largest amount, so asking to
        // round it up is an error rather than a wrapped or saturated figure.
        assert_eq!(
            compute_cash_rounding(jod(i64::MAX), QIRSH, Up),
            Err(MoneyError::Overflow)
        );
    }

    /// The wire form is `Tendering.cash_rounding`'s, so its field names are
    /// pinned as literally as `Money`'s golden pins its own.
    #[test]
    fn a_cash_rounding_round_trips_through_canonical_json() {
        let r = compute_cash_rounding(jod(623), QIRSH, Nearest).unwrap();
        let json = serde_json::to_string(&r).expect("serialises");
        assert_eq!(
            json,
            concat!(
                r#"{"original":{"minor":623,"currency":"JOD"},"#,
                r#""rounded":{"minor":620,"currency":"JOD"},"#,
                r#""adjustment":{"minor":-3,"currency":"JOD"}}"#
            )
        );

        let back: CashRounding = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back, r);
    }

    // Covers every currency the build knows. The step is in each currency's
    // own minor units, so nothing about rounding may assume three decimals.
    fn known_currency() -> impl Strategy<Value = Currency> {
        prop_oneof![
            Just(Currency::JOD),
            Just(Currency::USD),
            Just(Currency::EUR),
        ]
    }

    // Covers the steps a store would actually choose — one fil, five, a
    // qirsh, and up to a whole dinar — beside any positive step up to ten
    // thousand minor units, so no claim leans on a tidy one. Non-positive
    // steps are excluded: they are a refusal, tested by example.
    fn coin_steps() -> impl Strategy<Value = i64> {
        prop_oneof![
            proptest::sample::select(vec![1_i64, 5, 10, 25, 50, 100, 250, 500, 1_000]),
            1_i64..=10_000,
        ]
    }

    // Covers each cash direction once. Kept apart from any tax rounding rule
    // because the two axes must never be conflated.
    fn every_direction() -> impl Strategy<Value = RoundingDirection> {
        proptest::sample::select(DIRECTIONS.to_vec())
    }

    // Covers split sales: a total from nothing to 10^15 minor units in every
    // known currency, the part of it the earlier tenders already paid —
    // anywhere from none of it to all of it — a coin step and a direction.
    //
    // The earlier tenders are drawn as their exact sum, not as a list of
    // kinds, and that is deliberate: a tender that does not settle the sale
    // is applied exactly whatever its kind, so its kind is not an input to
    // the rule under test. Negative totals are excluded because a sale's
    // total never is one; the refusal of a negative remainder is an example
    // test. Totals near `i64::MAX` are excluded too — the overflow refusal
    // has its own example.
    fn split_sales() -> impl Strategy<Value = (Money, Money, i64, RoundingDirection)> {
        (0_i64..=1_000_000_000_000_000, known_currency()).prop_flat_map(|(total, currency)| {
            (
                Just(Money::from_minor(total, currency)),
                (0..=total).prop_map(move |paid| Money::from_minor(paid, currency)),
                coin_steps(),
                every_direction(),
            )
        })
    }

    // Covers every split sale above, settled by a final tender of each of
    // the six seeded kinds. Constructed kinds are left to
    // `rounding_follows_the_drawer_count_not_the_code`: this property is
    // about the grid the register actually ships.
    fn split_sales_with_a_final_kind()
    -> impl Strategy<Value = (Money, Money, TenderType, i64, RoundingDirection)> {
        (
            split_sales(),
            proptest::sample::select(standard_tender_types()),
        )
            .prop_map(|((total, paid, step, dir), kind)| (total, paid, kind, step, dir))
    }

    proptest! {
        // The crate's one shared configuration: 4,096 cases, the recorded
        // seed, and a minimized failure persisted under proptest-regressions/.
        // Conventions §5.1 is the rule and microstep 1.1.0 owns it.
        #![proptest_config(domain_proptest_config())]

        /// E.14. A split sale is rounded only when its final tender is cash,
        /// and then only on what the earlier tenders left: any other final
        /// kind — a card first of all — is asked for that remainder exactly.
        /// "Once" is structural: one tender reaches the rule.
        #[test]
        fn prop_cash_rounding_only_on_final_cash_tender(
            (total, paid, kind, step, dir) in split_sales_with_a_final_kind()
        ) {
            let remaining = total.checked_sub(paid).unwrap();
            let rounding = final_tender_rounding(&kind, remaining, step, dir).unwrap();

            if let Some(r) = rounding {
                prop_assert_eq!(kind.code.as_str(), "cash", "only cash is rounded");
                // On the final remainder — not on the total, and not on
                // anything an earlier tender paid.
                prop_assert_eq!(r.original, remaining);
                // In the store's own direction, exactly as the arithmetic
                // rounds: the rule decides whether, never how.
                prop_assert_eq!(
                    Ok(r),
                    compute_cash_rounding(remaining, step, dir),
                    "the rule must delegate to the arithmetic"
                );
                prop_assert_eq!(r.rounded.minor() % step, 0, "asked for a payable amount");
                prop_assert!(r.adjustment.minor().abs() < step, "moved by less than a step");
                prop_assert_eq!(
                    paid.checked_add(r.rounded).unwrap(),
                    total.checked_add(r.adjustment).unwrap(),
                    "settled at the total plus the one adjustment"
                );
            } else {
                prop_assert!(!kind.is_cash_counted, "{} went unrounded", kind.code);
            }
        }

        /// The books still reconcile. Whatever the step and direction, a sale
        /// settled by cash is collected at exactly its total plus the one
        /// recorded adjustment — no fil appears or disappears between what was
        /// owed, what was asked for and what was written down — and the
        /// adjustment moves the way the store's direction says, by less than a
        /// coin.
        #[test]
        fn prop_rounding_adjustment_keeps_total_exact(
            (total, paid, step, dir) in split_sales()
        ) {
            let remaining = total.checked_sub(paid).unwrap();
            let r = compute_cash_rounding(remaining, step, dir).unwrap();

            prop_assert_eq!(r.original, remaining);
            prop_assert_eq!(r.original.checked_add(r.adjustment), Ok(r.rounded));
            prop_assert_eq!(
                paid.checked_add(r.rounded).unwrap(),
                total.checked_add(r.adjustment).unwrap()
            );
            prop_assert!(
                r.rounded.currency() == total.currency()
                    && r.adjustment.currency() == total.currency(),
                "rounding never changes currency"
            );
            prop_assert_eq!(r.rounded.minor() % step, 0, "asked for a payable amount");

            let adjustment = r.adjustment.minor();
            match dir {
                Nearest => prop_assert!(adjustment.abs() * 2 <= step, "{adjustment} for step {step}"),
                Up => prop_assert!((0..step).contains(&adjustment), "{adjustment} for step {step}"),
                Down => prop_assert!((1 - step..=0).contains(&adjustment), "{adjustment} for step {step}"),
            }
        }
    }
}
