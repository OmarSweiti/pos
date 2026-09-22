//! How a sale is paid for (microstep 1.5.x).
//!
//! This module is the *vocabulary* — what kinds of tender exist, what each one
//! does, and what one collected tender records. The arithmetic is next door:
//! `remaining_due`, `change_due` and `is_settled` are 1.5.2's, cash rounding is
//! 1.5.3's, and the denomination table is 1.5.4's. Nothing here computes with
//! a [`Money`]; it only carries one.
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
use crate::money::Money;

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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::money::Currency;
    use uuid::Uuid;

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
}
