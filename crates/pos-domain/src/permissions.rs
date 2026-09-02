//! Capabilities, and the default grant matrix that says who holds which.
//!
//! Three rules shape this file.
//!
//! **A capability is a marker type, not a string a caller can mistype.** The
//! wire form is a `<noun>.<verb>` string (conventions §2) because
//! `capability.code` is a `TEXT` primary key and a `role_capability` row has to
//! name one, but nothing in Rust spells that string by hand: it is declared once
//! beside its type in [`capabilities!`], and every other mention is
//! `cap::SaleVoid::NAME`. `sale.park`, `sale.resume` and the cash-movement
//! capability came to be used by the IPC catalogue while missing from the list
//! of capabilities, which is the failure one declaration site removes.
//!
//! **The default matrix is declared with the capability, not beside it.** A
//! capability with no decision for a role is not a capability nobody thought
//! about — it is an accidental grant or an accidental block, and both are
//! invisible. So [`Capability`] carries [`Capability::DEFAULT_GRANTS`] and the
//! macro takes the four cells in the same breath as the name: adding a
//! capability without deciding all four roles does not compile, and adding a
//! fifth role does not compile until every one of the thirty-two rows answers
//! for it. `ref/domain-api.md` §8.2's grid is the normative statement of those
//! cells; the invocation below is that grid, row for row.
//!
//! **This file records limits; it does not price them.** A cell reads
//! `✓ (role cap)` or `✓ (≤ threshold)` in §8.2, and the number behind either one
//! is a merchant setting that lives in `setting` and `role_capability.limit_json`
//! — not here. [`Limit`] says *which* limit bounds a grant, which is what a seed
//! and a report need to agree on. The two limits whose rule is a shape rather
//! than a number are decided here, because a shape has no configuration:
//! [`GrantSet::journal_scope`] and [`GrantSet::customer_lookup`].
//!
//! **What is deliberately absent.** `Authorized<C>`, `authorize`, `GrantSet`'s
//! union across several `user_role` rows, `ApprovalHandle` and the rest of
//! `ref/domain-api.md` §8's `PermissionError` variants land with microstep 1.6.4,
//! together with the two `trybuild` cases that prove a token for the wrong
//! capability is a type error and that one cannot be forged from outside the
//! module. `authorize` is the only constructor of an `Authorized<C>`, so writing
//! the token here — before the approval machinery it validates exists — would
//! ship a type with no way to make one. The database half of this microstep is
//! deferred for the same kind of reason: `role` and `role_capability` are created
//! by migration `0004` (microstep 1.6.1), which seeds its rows *from* this grid.

use std::collections::BTreeMap;

/// One capability: a marker type, its wire name, and its shipped default grants.
///
/// **Not `Authorized<const C: &'static str>`.** That does not compile and never
/// has — rustc answers "`&'static str` is forbidden as the type of a const
/// generic parameter — the only supported types are integers, `bool`, and
/// `char`", and string const generics remain unstable. The marker type keeps the
/// compile-time property on stable, and microstep 1.6.4 is what spends it. Do
/// not "fix" this into a runtime `&str` field: that discards the property
/// silently.
pub trait Capability {
    /// What `capability.code` stores and a `role_capability` row references.
    const NAME: &'static str;

    /// This capability's row of `ref/domain-api.md` §8.2's grid.
    ///
    /// It is an associated const rather than a lookup table beside the macro
    /// because a missing associated const is a compile error and a missing table
    /// row is a test failure at best. **These are the shipped defaults, not the
    /// live grants**: `role_capability` is editable under `user.admin`, so a
    /// running register reads its rows and only a fresh install reads this.
    const DEFAULT_GRANTS: RoleGrants;
}

/// The four roles migration `0004` seeds, spelled as `role.code` stores them.
///
/// They are the columns of §8.2's grid, and the enum is closed on purpose: a
/// merchant adds *users*, not roles. A fifth role is a schema change and a
/// re-decision of all thirty-two rows, which is exactly what
/// [`RoleGrants::for_role`] and every `RoleGrants` literal make the compiler
/// insist on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// Sells. Holds no capability that reverses money or changes a price.
    Cashier,
    /// Sells, and carries the till: drawer, cash movements, X and Z.
    ShiftLead,
    /// Everything a store is run with, scoped to their store.
    Manager,
    /// The back office. Deliberately holds no till capability — see
    /// [`Role::Owner`]'s blanks in §8.2 and the note on [`Grant::SetsTheLimit`].
    Owner,
}

impl Role {
    /// Every role, once. The order is the grid's column order and the order
    /// [`RoleGrants`] declares its fields in.
    pub const ALL: [Role; 4] = [Role::Cashier, Role::ShiftLead, Role::Manager, Role::Owner];

    /// The token `role.code` stores, and this role's spelling everywhere else.
    /// `0004`'s `CHECK` list is `cashier|shift_lead|manager|owner`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Role::Cashier => "cashier",
            Role::ShiftLead => "shift_lead",
            Role::Manager => "manager",
            Role::Owner => "owner",
        }
    }
}

/// What bounds a grant that is held but not held outright — §8.2's
/// parentheticals, as values.
///
/// "The parenthetical qualifiers are enforced, not decorative." Two of the five
/// are decided in this file, because their rule is a *shape* and a shape needs no
/// configuration: [`Limit::OwnShift`] and [`Limit::ExactMatchOnly`]. The other
/// three name a number a merchant sets, so this type records which limit applies
/// and the microstep that owns the operation applies it — 1.4.5 for the discount
/// cap, 1.4.7 for the override floor and ceiling, 2.3.x for the refund
/// threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Limit {
    /// §8.2 writes this "(own)" on `shift.open`/`shift.close` and "(own shift)"
    /// on `journal.view`; both mean the shift the holder opened. Closing your own
    /// shift is not an escalation and takes no approval; `shift_force_close_stale`
    /// is the separate cross-user path under the same capability.
    OwnShift,
    /// The holder's own store, against an owner's org-wide reach. This is the
    /// `user_role.store_id IS NULL` distinction, seen from the grant side.
    OwnStore,
    /// An exact phone, card or loyalty number, and never a list or a prefix.
    /// PDPL minimisation is a search-shape decision, not a disclaimer.
    ExactMatchOnly,
    /// A ceiling configured per role — the manual-discount cap.
    RoleCap,
    /// The configured receipted-refund threshold. Above it is a different
    /// capability (`refund.above_threshold`), not a bigger number.
    RefundThreshold,
}

impl Limit {
    /// A stable token, so a limit is spelled the same in an error a human reads,
    /// in whatever `role_capability.limit_json` 1.6.1 writes, and in a report.
    /// The inverse direction is deliberately absent for the reason
    /// [`crate::catalog::RegulatedKind::as_str`] gives: whoever parses one has to
    /// decide what a corrupt value becomes, and that belongs to the repository
    /// that reads the column.
    pub const fn as_str(self) -> &'static str {
        match self {
            Limit::OwnShift => "own_shift",
            Limit::OwnStore => "own_store",
            Limit::ExactMatchOnly => "exact_match_only",
            Limit::RoleCap => "role_cap",
            Limit::RefundThreshold => "refund_threshold",
        }
    }
}

/// One cell of §8.2's grid.
///
/// Four states, because the grid has four kinds of cell and flattening the last
/// two into one blank loses the reason for a blank.
///
/// Migration `0004` seeds a `role_capability` row for **every** one of the 128
/// (role, capability) cells, not only the held ones, so an absent row is a
/// capability nobody decided rather than a denial. Each variant maps to exactly
/// one `decision`, and only [`Grant::HeldWithin`] writes a `limit_json`:
///
/// | variant | `decision` | `limit_json` |
/// |---|---|---|
/// | [`Grant::Held`] | `granted` | `NULL` |
/// | [`Grant::HeldWithin`] | `granted` | `{"kind":"own_shift"}` — the token from [`Limit::as_str`] |
/// | [`Grant::Withheld`] | `withheld` | `NULL` |
/// | [`Grant::SetsTheLimit`] | `sets_the_limit` | `NULL` |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Grant {
    /// A plain `✓`: held, unqualified.
    Held,
    /// `✓` with a parenthetical: held, bounded by a [`Limit`].
    HeldWithin(Limit),
    /// A blank cell: not held.
    Withheld,
    /// §8.2's "sets the caps" / "sets floor and ceiling" / "sets the threshold"
    /// cells. **Not held**: the owner runs no till, so they cannot apply a
    /// manual discount or override a price. What they do is configure the
    /// [`Limit`] that bounds the roles which can, through `settings.edit`.
    /// Writing those three cells as an ordinary blank would leave a reader to
    /// guess whether the owner was forgotten, which is why `0004` stores them as
    /// their own `decision = 'sets_the_limit'` rather than as a second kind of
    /// `withheld`.
    SetsTheLimit,
}

impl Grant {
    /// Whether the role may perform the operation at all.
    ///
    /// Not whether `0004` seeds a row: it seeds one for every cell. This is what
    /// separates the two `decision` values that mean yes-with-conditions and yes
    /// from the two that mean no.
    pub const fn is_held(self) -> bool {
        match self {
            Grant::Held | Grant::HeldWithin(_) => true,
            Grant::Withheld | Grant::SetsTheLimit => false,
        }
    }

    /// The limit bounding a held grant, or `None` — for a grant held outright
    /// and for a cell that is not held at all.
    pub const fn limit(self) -> Option<Limit> {
        match self {
            Grant::HeldWithin(limit) => Some(limit),
            Grant::Held | Grant::Withheld | Grant::SetsTheLimit => None,
        }
    }
}

/// One row of §8.2's grid: the shipped default for each of the four roles.
///
/// Named fields rather than `[Grant; 4]`, and the reason is the failure mode of
/// a grid: a positional array lets `refund.cash_for_card` be granted to a cashier
/// by a value landing one column left of where its author meant it, and nothing
/// says so. A field name cannot be off by one.
///
/// It is also what makes a fifth role a decision rather than a default. Adding
/// one is thirty-four compile errors and no green test: `E0004` at
/// [`Role::as_str`] and at [`RoleGrants::for_role`], neither of which has a
/// wildcard arm, and `E0063` at each of the thirty-two rows, which now have a
/// column nobody has answered for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RoleGrants {
    pub cashier: Grant,
    pub shift_lead: Grant,
    pub manager: Grant,
    pub owner: Grant,
}

impl RoleGrants {
    /// This row's cell for one role.
    pub const fn for_role(self, role: Role) -> Grant {
        match role {
            Role::Cashier => self.cashier,
            Role::ShiftLead => self.shift_lead,
            Role::Manager => self.manager,
            Role::Owner => self.owner,
        }
    }
}

/// One cell of the grid, written the way the document writes it.
///
/// The vocabulary is closed and every token maps to exactly one [`Grant`], so a
/// row of [`capabilities!`] can be read straight against §8.2's table and a
/// spelling that is not in this list is a compile error rather than a silently
/// different grant.
macro_rules! capability_grant {
    (yes) => {
        $crate::permissions::Grant::Held
    };
    (no) => {
        $crate::permissions::Grant::Withheld
    };
    (sets_the_limit) => {
        $crate::permissions::Grant::SetsTheLimit
    };
    (own_shift) => {
        $crate::permissions::Grant::HeldWithin($crate::permissions::Limit::OwnShift)
    };
    (own_store) => {
        $crate::permissions::Grant::HeldWithin($crate::permissions::Limit::OwnStore)
    };
    (exact_match_only) => {
        $crate::permissions::Grant::HeldWithin($crate::permissions::Limit::ExactMatchOnly)
    };
    (role_cap) => {
        $crate::permissions::Grant::HeldWithin($crate::permissions::Limit::RoleCap)
    };
    (refund_threshold) => {
        $crate::permissions::Grant::HeldWithin($crate::permissions::Limit::RefundThreshold)
    };
}

/// Declare each capability exactly once: its marker type, its wire name, its
/// default grant for each of the four roles, and its membership in [`cap::ALL`]
/// and [`cap::DEFAULT_MATRIX`].
///
/// One source, so four things cannot drift apart. `ref/domain-api.md` §8's
/// original macro derived the first two; the grid was a separate table checked by
/// a test, which is one drift later than it needs to be. Carrying the cells here
/// makes "a capability nobody decided the grants for" a missing associated const
/// — `error[E0046]: not all trait items implemented, missing: DEFAULT_GRANTS` —
/// instead of a red test somebody has to be running.
///
/// Both derived constants are `&[…]` over the marker types themselves, never a
/// hand-typed second list and never a count. A hard-coded capability count drifts
/// the moment a capability is added, and it already had.
macro_rules! capabilities {
    ($(
        $ident:ident => $name:literal {
            cashier: $cashier:tt,
            shift_lead: $shift_lead:tt,
            manager: $manager:tt,
            owner: $owner:tt $(,)?
        }
    ),+ $(,)?) => {
        /// The capability marker types. One per row of `ref/domain-api.md` §8.2.
        pub mod cap {
            use super::{Capability, RoleGrants};

            $(
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub struct $ident;

                impl Capability for $ident {
                    const NAME: &'static str = $name;
                    const DEFAULT_GRANTS: RoleGrants = RoleGrants {
                        cashier: capability_grant!($cashier),
                        shift_lead: capability_grant!($shift_lead),
                        manager: capability_grant!($manager),
                        owner: capability_grant!($owner),
                    };
                }
            )+

            /// Every capability's wire name, derived from the types themselves
            /// and never hand-maintained.
            pub const ALL: &[&str] = &[$(<$ident as Capability>::NAME),+];

            /// The whole of §8.2's grid, in [`ALL`]'s order and derived from the
            /// same declarations, so the two can never disagree about which
            /// capabilities exist.
            pub const DEFAULT_MATRIX: &[(&str, RoleGrants)] = &[
                $((<$ident as Capability>::NAME, <$ident as Capability>::DEFAULT_GRANTS)),+
            ];
        }
    };
}

// `ref/domain-api.md` §8.2, row for row. Read it against that table; the columns
// are in the table's order and the tokens are its cells. Two of the grid's shapes
// are worth knowing before you read a blank:
//
//   * The owner runs no till. No `sale.create`, no `xreport.run`, no
//     `zreport.run`, no drawer and no shift — master plan C.10's deliberate split
//     of till roles from back-office roles. An owner reads the day through
//     `reports.all`, which is a report over facts; running a Z *closes a shift*
//     on a register they are not standing at.
//   * `sets_the_limit` is a blank with a reason: the owner configures the cap,
//     the floor and ceiling, or the threshold that bounds the roles who hold it.
capabilities! {
    SaleCreate           => "sale.create"            { cashier: yes, shift_lead: yes, manager: yes, owner: no },
    SalePark             => "sale.park"              { cashier: yes, shift_lead: yes, manager: yes, owner: no },
    SaleResume           => "sale.resume"            { cashier: yes, shift_lead: yes, manager: yes, owner: no },
    SaleVoid             => "sale.void"              { cashier: no, shift_lead: yes, manager: yes, owner: no },
    // Any document, not only your own (microstep 1.9.3).
    SaleReprint          => "sale.reprint"           { cashier: yes, shift_lead: yes, manager: yes, owner: yes },
    // The open-price line (ref/domain-api.md §6.5), capped and audited.
    DepartmentSale       => "sale.department"        { cashier: yes, shift_lead: yes, manager: yes, owner: no },
    LineVoid             => "line.void"              { cashier: yes, shift_lead: yes, manager: yes, owner: no },
    DiscountManual       => "discount.manual"        { cashier: role_cap, shift_lead: yes, manager: yes, owner: sets_the_limit },
    PriceOverride        => "price.override"         { cashier: no, shift_lead: yes, manager: yes, owner: sets_the_limit },
    RefundReceipted      => "refund.receipted"       { cashier: refund_threshold, shift_lead: yes, manager: yes, owner: sets_the_limit },
    RefundAboveThreshold => "refund.above_threshold" { cashier: no, shift_lead: no, manager: yes, owner: no },
    RefundReceiptless    => "refund.receiptless"     { cashier: no, shift_lead: no, manager: yes, owner: no },
    RefundCashForCard    => "refund.cash_for_card"   { cashier: no, shift_lead: no, manager: yes, owner: no },
    // A defect claim on day 20 (ref/domain-api.md §10).
    RefundOutsideWindow  => "refund.outside_window"  { cashier: no, shift_lead: no, manager: yes, owner: no },
    DrawerOpen           => "drawer.open"            { cashier: no, shift_lead: yes, manager: yes, owner: no },
    // Every kind (ref/schema.md `cash_movement`).
    CashMovement         => "cash.movement"          { cashier: no, shift_lead: yes, manager: yes, owner: no },
    ShiftOpen            => "shift.open"             { cashier: own_shift, shift_lead: yes, manager: yes, owner: no },
    ShiftClose           => "shift.close"            { cashier: own_shift, shift_lead: yes, manager: yes, owner: no },
    // Split from zreport.run: totals by tender plus the opening float *is* the
    // expected figure, so an X report defeats the blind close (§8.3).
    XReportRun           => "xreport.run"            { cashier: no, shift_lead: yes, manager: yes, owner: no },
    ZReportRun           => "zreport.run"            { cashier: no, shift_lead: yes, manager: yes, owner: no },
    // Find Tuesday's receipt in ten seconds.
    JournalView          => "journal.view"           { cashier: own_shift, shift_lead: yes, manager: yes, owner: yes },
    StockAdjust          => "stock.adjust"           { cashier: no, shift_lead: no, manager: yes, owner: yes },
    ProductEdit          => "product.edit"           { cashier: no, shift_lead: no, manager: yes, owner: yes },
    // A rate is a legal fact, not a setting.
    TaxRateEdit          => "tax.rate.edit"          { cashier: no, shift_lead: no, manager: no, owner: yes },
    // Rebuild a failed fiscal payload after the builder is corrected (§8.2).
    FiscalRemediate      => "fiscal.remediate"       { cashier: no, shift_lead: no, manager: yes, owner: yes },
    // PII: name, phone — PDPL (microstep 3.x).
    CustomerLookup       => "customer.lookup"        { cashier: exact_match_only, shift_lead: yes, manager: yes, owner: yes },
    TrainingToggle       => "training_mode.toggle"   { cashier: no, shift_lead: yes, manager: yes, owner: no },
    SettingsEdit         => "settings.edit"          { cashier: no, shift_lead: no, manager: own_store, owner: yes },
    UserAdmin            => "user.admin"             { cashier: no, shift_lead: no, manager: own_store, owner: yes },
    // The back-office restore of a register whose database opens. Recovery after
    // credential-store loss is authorised by the merchant recovery code instead
    // (microstep 1.8.5b) — the capability tables live inside the database that
    // cannot be opened.
    BackupRestore        => "backup.restore"         { cashier: no, shift_lead: no, manager: no, owner: yes },
    ReportsOwn           => "reports.own"            { cashier: yes, shift_lead: yes, manager: yes, owner: yes },
    // Anyone's shift, any day, any cashier.
    ReportsAll           => "reports.all"            { cashier: no, shift_lead: no, manager: own_store, owner: yes },
}

/// §8.2's row for one capability, or `None` when nothing by that name exists.
///
/// `None` is not "denied to everyone": it means the string is not a capability,
/// which is what `role_capability.capability REFERENCES capability(code)` refuses
/// in the storage engine and what this answers in memory.
pub fn default_grants(capability: &str) -> Option<RoleGrants> {
    cap::DEFAULT_MATRIX
        .iter()
        .find(|(name, _)| *name == capability)
        .map(|(_, grants)| *grants)
}

/// What one holder may do: the capabilities they hold, and the limit on each.
///
/// A set only ever contains what its holder *holds* — a [`Grant::Withheld`] or
/// [`Grant::SetsTheLimit`] cell contributes nothing. That is a property of this
/// in-memory view, not of storage: `role_capability` carries a row for those
/// cells too, recording which of the two answers was given.
/// [`GrantSet::grant`] answers `Withheld` for everything else, so a caller never
/// has to distinguish "absent" from "denied".
///
/// **Unioning several roles is not here.** `user_role` lets one user hold more
/// than one role, and widening two different [`Limit`]s has no obvious answer —
/// `HeldWithin(OwnStore)` beside `HeldWithin(RoleCap)` is not a lattice. That
/// decision belongs with `authorize` at microstep 1.6.4, where there is an actor,
/// a store and an escalation policy to decide it against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrantSet {
    held: BTreeMap<&'static str, Grant>,
}

impl GrantSet {
    /// Nobody's grants: a user with no role, and the base every fixture builds
    /// from.
    pub const fn empty() -> GrantSet {
        GrantSet {
            held: BTreeMap::new(),
        }
    }

    /// §8.2's column for one role, as the grants a fresh install gives it.
    ///
    /// This is the *default*. A running register reads `role_capability`, which
    /// `user.admin` can edit, so a rule that branches on [`Role`] instead of on
    /// what the holder actually holds would silently ignore every merchant edit.
    /// Nothing in this file takes a `Role` except this constructor.
    pub fn of_role(role: Role) -> GrantSet {
        cap::DEFAULT_MATRIX
            .iter()
            .fold(GrantSet::empty(), |set, (name, grants)| {
                set.with_grant(name, grants.for_role(role))
            })
    }

    /// Add one grant, or remove the capability when the grant is not held.
    pub fn with_grant(mut self, capability: &'static str, grant: Grant) -> GrantSet {
        if grant.is_held() {
            self.held.insert(capability, grant);
        } else {
            self.held.remove(capability);
        }
        self
    }

    /// What the holder has for one capability, [`Grant::Withheld`] if nothing.
    pub fn grant(&self, capability: &str) -> Grant {
        self.held
            .get(capability)
            .copied()
            .unwrap_or(Grant::Withheld)
    }

    /// Whether the holder may perform the operation at all. Spell the argument
    /// `cap::SaleVoid::NAME`, never a string literal.
    pub fn holds(&self, capability: &str) -> bool {
        self.grant(capability).is_held()
    }

    /// Whose sales the holder may read back, or `None` when they may read none.
    ///
    /// **`journal.view` is scoped to the holder's own shift unless they also hold
    /// `reports.all`** (`ref/ipc-contract.md` §"Journal, health, diagnostics",
    /// §8.2). The journal is behind `journal.view` rather than `reports.all`
    /// because *"a customer is at the counter with a receipt from Tuesday"* has to
    /// take ten seconds, and behind `reports.all` it took however long finding a
    /// manager takes. Another cashier's sales are the thing that stayed behind
    /// `reports.all`, and that is the answer to "who may see whose takings".
    ///
    /// The rule reads `reports.all` and not the cashier's recorded
    /// [`Limit::OwnShift`], which is deliberate and is the one place the grid's
    /// parenthetical and the rule can be made to disagree: grant a cashier
    /// `reports.all` and they see every shift, because that grant is what the
    /// sentence above is written about. The parenthetical records why §8.2 wrote
    /// the qualifier in the cashier's cell — with the default matrix the two
    /// always agree, which `journal_view_is_scoped_to_the_holders_own_shift_without_reports_all`
    /// asserts for all four roles.
    ///
    /// How far `EveryShift` reaches — one store or the whole organisation — is
    /// the limit on the `reports.all` grant itself, not a second scope here.
    pub fn journal_scope(&self) -> Option<JournalScope> {
        if !self.holds(cap::JournalView::NAME) {
            return None;
        }
        if self.holds(cap::ReportsAll::NAME) {
            Some(JournalScope::EveryShift)
        } else {
            Some(JournalScope::OwnShift)
        }
    }

    /// Whether the holder may run a customer lookup of this shape.
    ///
    /// **`customer.lookup` returns a customer for an exact phone, card or loyalty
    /// number and never lists or prefix-searches** (§8.2). PDPL data minimisation
    /// is a search-shape decision, not a disclaimer on a screen: a prefix query
    /// over customers enumerates the merchant's customer base one keystroke at a
    /// time, and the capability exists to bound the disclosure rather than to log
    /// it. The refusal is therefore the same for every role — §8.2 annotates
    /// `(exact match only)` in the cashier's cell because that is where the grid
    /// had room, and the sentence it abbreviates is about the capability.
    pub fn customer_lookup(&self, shape: CustomerQueryShape) -> Result<(), PermissionError> {
        let capability = cap::CustomerLookup::NAME;
        if !self.holds(capability) {
            return Err(PermissionError::NotHeld(capability));
        }
        match shape {
            CustomerQueryShape::ExactIdentifier => Ok(()),
            CustomerQueryShape::Prefix | CustomerQueryShape::ListAll => Err(
                PermissionError::OutsideLimit(capability, Limit::ExactMatchOnly),
            ),
        }
    }
}

/// Whose sales a holder of `journal.view` may read.
///
/// Two values, because the journal answers one question — *is this receipt one of
/// mine, or anybody's?* Which store or organisation "anybody" spans is carried by
/// the `reports.all` grant's own [`Limit`], so it is not repeated here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalScope {
    /// The shift the holder opened, and no other.
    OwnShift,
    /// Any shift, any day, any cashier.
    EveryShift,
}

/// The shape of a customer lookup — not its contents.
///
/// **It carries no phone number, card number, loyalty number or name.** A value
/// here would put customer PII into a type that `Debug`-prints into every error,
/// assertion and log line that touches it, and `phone`, `email` and
/// `customer_name` are on the never-log list
/// (`ref/security-compliance.md` §6). The shell classifies what the cashier
/// typed; the domain decides whether that *shape* is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerQueryShape {
    /// One complete phone, card or loyalty number: at most one customer.
    ExactIdentifier,
    /// A leading fragment of one of those. Refused — it enumerates.
    Prefix,
    /// No identifier at all: browse or export the customer base. Refused.
    ListAll,
}

/// Everything this module refuses.
///
/// Microstep 1.6.4 adds the variants `ref/domain-api.md` §8 lists for `authorize`
/// and the approval handle — `EscalationRequired`, `SelfApprovalBanned`,
/// `UserInactive`, `OfflineAuthExpired` and the six `Approval*` mismatches. They
/// are absent rather than stubbed because each one names a value (`UserId`,
/// `ApprovalId`, `Timestamp`) that the machinery producing it does not exist to
/// produce yet.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PermissionError {
    /// The holder does not have this capability at all.
    #[error("{0} is not held")]
    NotHeld(&'static str),
    /// The holder has the capability, and what they asked for falls outside the
    /// limit that bounds it.
    #[error("{} is limited to {}", .0, .1.as_str())]
    OutsideLimit(&'static str, Limit),
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    // No `prop_` test here, and that is conventions §5.1 rather than an omission:
    // "a bounded universal claim uses an exhaustive `#[test]` loop when that loop
    // is feasible". The whole claim is thirty-two capabilities times four roles,
    // and a generator sampling from 128 cases would be a weaker proof with more
    // machinery. Every sweep below iterates `cap::ALL` or `Role::ALL` in full.

    #[test]
    fn default_matrix_covers_every_capability_in_cap_all() {
        // The exhaustive iteration, replacing a counted assertion: a hard-coded
        // capability count drifts the moment a capability is added, and it
        // already had. Nothing here names a number the source can disagree with.
        assert!(!cap::ALL.is_empty());

        for name in cap::ALL {
            let rows = cap::DEFAULT_MATRIX
                .iter()
                .filter(|(candidate, _)| candidate == name)
                .count();
            assert_eq!(rows, 1, "{name} must have exactly one row, found {rows}");

            let grants = default_grants(name).expect("a cap::ALL entry has a row");

            // Every role has a decided cell — by construction, since `RoleGrants`
            // has four named fields and no default — and at least one role holds
            // it. A capability granted to nobody is a command that cannot be run
            // and an escalation that can never be approved, which is the shape a
            // newly added capability takes when its grants were not thought
            // about.
            assert!(
                Role::ALL
                    .into_iter()
                    .any(|role| grants.for_role(role).is_held()),
                "{name} is granted to no role"
            );

            // And the cell a role gets is the cell the row declares. `for_role`
            // is the only accessor, so this is what stops a fifth role or a
            // reordered field from quietly answering for the wrong column.
            assert_eq!(grants.for_role(Role::Cashier), grants.cashier);
            assert_eq!(grants.for_role(Role::ShiftLead), grants.shift_lead);
            assert_eq!(grants.for_role(Role::Manager), grants.manager);
            assert_eq!(grants.for_role(Role::Owner), grants.owner);
        }

        // The other direction: the grid has no row for something that is not a
        // capability, and both derived lists cover exactly the same set.
        assert_eq!(cap::DEFAULT_MATRIX.len(), cap::ALL.len());
        for (name, _) in cap::DEFAULT_MATRIX {
            assert!(cap::ALL.contains(name), "{name} is not in cap::ALL");
        }

        // A string nobody declared has no default at all, which is `None` and
        // never "denied to everyone" — the two read the same at a call site that
        // only asks `is_held`, and only one of them is a typo.
        assert_eq!(default_grants("sale.friend_price"), None);
        assert_eq!(default_grants(""), None);
        assert_eq!(default_grants("SALE.CREATE"), None);
    }

    #[test]
    fn journal_view_is_scoped_to_the_holders_own_shift_without_reports_all() {
        // The cashier: the grid's `✓ (own shift)`, and no `reports.all`.
        let cashier = GrantSet::of_role(Role::Cashier);
        assert_eq!(
            cashier.grant(cap::JournalView::NAME),
            Grant::HeldWithin(Limit::OwnShift)
        );
        assert!(!cashier.holds(cap::ReportsAll::NAME));
        assert_eq!(cashier.journal_scope(), Some(JournalScope::OwnShift));

        // The shift lead: a plain `✓` on `journal.view` and still no
        // `reports.all`, so still their own shift. The rule is stated over
        // `reports.all`, not over the parenthetical, and this is the row that
        // shows the difference — a reader who thought the qualifier was the
        // whole rule would expect a shift lead to see everyone's takings.
        let shift_lead = GrantSet::of_role(Role::ShiftLead);
        assert_eq!(shift_lead.grant(cap::JournalView::NAME), Grant::Held);
        assert!(!shift_lead.holds(cap::ReportsAll::NAME));
        assert_eq!(shift_lead.journal_scope(), Some(JournalScope::OwnShift));

        // Manager and owner hold `reports.all`, which is the answer to "who may
        // see another cashier's sales".
        for role in [Role::Manager, Role::Owner] {
            let grants = GrantSet::of_role(role);
            assert!(grants.holds(cap::ReportsAll::NAME), "{}", role.as_str());
            assert_eq!(grants.journal_scope(), Some(JournalScope::EveryShift));
        }

        // On the shipped defaults the grid's parenthetical and the rule always
        // agree: a recorded `OwnShift` limit on `journal.view` occurs exactly
        // where the scope resolves to `OwnShift`.
        for role in Role::ALL {
            let grants = GrantSet::of_role(role);
            if grants.grant(cap::JournalView::NAME).limit() == Some(Limit::OwnShift) {
                assert_eq!(
                    grants.journal_scope(),
                    Some(JournalScope::OwnShift),
                    "{}",
                    role.as_str()
                );
            }
        }

        // `reports.all` is what widens it, and only `reports.all`: a cashier
        // deliberately granted it reads every shift, and `reports.own` — which
        // every role holds — never widens anything.
        let cashier_reporting =
            GrantSet::of_role(Role::Cashier).with_grant(cap::ReportsAll::NAME, Grant::Held);
        assert_eq!(
            cashier_reporting.journal_scope(),
            Some(JournalScope::EveryShift)
        );
        assert!(cashier.holds(cap::ReportsOwn::NAME));

        // And `reports.all` on its own opens no journal: the two capabilities
        // are not substitutes.
        assert_eq!(GrantSet::empty().journal_scope(), None);
        assert_eq!(
            GrantSet::empty()
                .with_grant(cap::ReportsAll::NAME, Grant::Held)
                .journal_scope(),
            None
        );
    }

    #[test]
    fn customer_lookup_refuses_a_prefix_query() {
        // A prefix query over customers enumerates the merchant's customer base
        // one keystroke at a time, so it is refused for every role that holds
        // the capability — including the owner. PDPL minimisation is a
        // search-shape decision, and all four roles hold `customer.lookup`.
        for role in Role::ALL {
            let grants = GrantSet::of_role(role);
            assert!(grants.holds(cap::CustomerLookup::NAME), "{}", role.as_str());

            assert_eq!(
                grants.customer_lookup(CustomerQueryShape::ExactIdentifier),
                Ok(())
            );
            for refused in [CustomerQueryShape::Prefix, CustomerQueryShape::ListAll] {
                assert_eq!(
                    grants.customer_lookup(refused),
                    Err(PermissionError::OutsideLimit(
                        cap::CustomerLookup::NAME,
                        Limit::ExactMatchOnly
                    )),
                    "{} may not run {refused:?}",
                    role.as_str()
                );
            }
        }

        // The refusal names the capability and the limit, because "denied" with
        // neither is unactionable for whoever reads it.
        assert_eq!(
            GrantSet::of_role(Role::Manager)
                .customer_lookup(CustomerQueryShape::Prefix)
                .unwrap_err()
                .to_string(),
            "customer.lookup is limited to exact_match_only"
        );

        // Not holding the capability is a different refusal from asking for the
        // wrong shape, and the exact shape is refused too.
        let none = GrantSet::empty();
        for shape in [
            CustomerQueryShape::ExactIdentifier,
            CustomerQueryShape::Prefix,
            CustomerQueryShape::ListAll,
        ] {
            assert_eq!(
                none.customer_lookup(shape),
                Err(PermissionError::NotHeld(cap::CustomerLookup::NAME))
            );
        }
    }

    #[test]
    fn an_owner_holds_no_till_capability() {
        // Master plan C.10's split of till roles from back-office roles, kept so
        // that a reader does not take the owner's blanks for omissions. An owner
        // reads the day's takings through `reports.all`, which is a report over
        // facts; running a Z *closes a shift* on a register they are not standing
        // at.
        let owner = GrantSet::of_role(Role::Owner);
        for withheld in [
            cap::SaleCreate::NAME,
            cap::SalePark::NAME,
            cap::SaleResume::NAME,
            cap::SaleVoid::NAME,
            cap::DepartmentSale::NAME,
            cap::LineVoid::NAME,
            cap::DrawerOpen::NAME,
            cap::CashMovement::NAME,
            cap::ShiftOpen::NAME,
            cap::ShiftClose::NAME,
            cap::XReportRun::NAME,
            cap::ZReportRun::NAME,
        ] {
            assert!(!owner.holds(withheld), "the owner runs no till: {withheld}");
        }
        assert!(owner.holds(cap::ReportsAll::NAME));
        assert!(owner.holds(cap::JournalView::NAME));
        assert!(owner.holds(cap::SaleReprint::NAME));

        // The two capabilities only an owner has, and the one only a manager has.
        for owner_only in [cap::TaxRateEdit::NAME, cap::BackupRestore::NAME] {
            for role in [Role::Cashier, Role::ShiftLead, Role::Manager] {
                assert!(
                    !GrantSet::of_role(role).holds(owner_only),
                    "{owner_only} belongs to the owner alone"
                );
            }
            assert!(owner.holds(owner_only));
        }
        for escalation in [
            cap::RefundAboveThreshold::NAME,
            cap::RefundReceiptless::NAME,
            cap::RefundCashForCard::NAME,
            cap::RefundOutsideWindow::NAME,
        ] {
            for role in [Role::Cashier, Role::ShiftLead, Role::Owner] {
                assert!(
                    !GrantSet::of_role(role).holds(escalation),
                    "{escalation} is the manager's"
                );
            }
            assert!(GrantSet::of_role(Role::Manager).holds(escalation));
        }
    }

    #[test]
    fn a_cell_that_only_sets_a_limit_grants_nothing() {
        // "Sets the caps", "sets floor and ceiling" and "sets the threshold" are
        // blanks with a reason, not grants. Migration `0004` stores them as
        // `decision = 'sets_the_limit'`, a third answer beside granted and
        // withheld, so reading one of these three as a grant would hand the owner
        // a till operation they have no `sale.create` to perform it inside.
        let owner_matrix = [
            cap::DiscountManual::NAME,
            cap::PriceOverride::NAME,
            cap::RefundReceipted::NAME,
        ];
        let owner = GrantSet::of_role(Role::Owner);
        for capability in owner_matrix {
            let grants = default_grants(capability).unwrap();
            assert_eq!(grants.owner, Grant::SetsTheLimit);
            assert!(!grants.owner.is_held());
            assert_eq!(grants.owner.limit(), None);
            assert!(!owner.holds(capability));
            assert_eq!(owner.grant(capability), Grant::Withheld);
        }
        // The owner does hold `settings.edit`, which is how those limits are set.
        assert!(owner.holds(cap::SettingsEdit::NAME));

        // Every other state answers `is_held` the way the grid reads it.
        assert!(Grant::Held.is_held());
        assert!(Grant::HeldWithin(Limit::OwnStore).is_held());
        assert!(!Grant::Withheld.is_held());
        assert_eq!(Grant::Held.limit(), None);
        assert_eq!(
            Grant::HeldWithin(Limit::RoleCap).limit(),
            Some(Limit::RoleCap)
        );
    }

    #[test]
    fn a_grant_set_contains_only_held_capabilities() {
        // A set is what its holder holds. Absent and denied read the same, so no
        // caller has to remember which of the two a missing row was.
        let cashier = GrantSet::of_role(Role::Cashier);
        assert_eq!(cashier.grant(cap::SaleVoid::NAME), Grant::Withheld);
        assert!(!cashier.holds(cap::SaleVoid::NAME));
        assert_eq!(cashier.grant("not.a.capability"), Grant::Withheld);

        // Every capability in the set is held, and every held cell of the role's
        // column is in the set. `0004` seeds all 128 cells, held or not; what the
        // two assertions below pin is which of them a holder actually holds.
        for role in Role::ALL {
            let grants = GrantSet::of_role(role);
            for name in cap::ALL {
                let cell = default_grants(name).unwrap().for_role(role);
                assert_eq!(
                    grants.holds(name),
                    cell.is_held(),
                    "{name} for {}",
                    role.as_str()
                );
                if cell.is_held() {
                    assert_eq!(grants.grant(name), cell);
                }
            }
        }

        // Granting a withheld cell removes the capability rather than storing a
        // denial, so a set can never answer `holds` from a row that means "no".
        let widened = GrantSet::empty().with_grant(cap::SaleVoid::NAME, Grant::Held);
        assert!(widened.holds(cap::SaleVoid::NAME));
        let narrowed = widened
            .clone()
            .with_grant(cap::SaleVoid::NAME, Grant::Withheld);
        assert!(!narrowed.holds(cap::SaleVoid::NAME));
        assert_eq!(narrowed, GrantSet::empty());
        assert_eq!(
            widened.with_grant(cap::SaleVoid::NAME, Grant::SetsTheLimit),
            GrantSet::empty()
        );
    }

    #[test]
    fn capability_names_are_unique_and_follow_noun_dot_verb() {
        // Conventions §2: a capability string is `<noun>.<verb>`, lower-case, and
        // it is a `capability.code` primary key — so a duplicate is a migration
        // that fails to apply and a rename is a `role_capability` row pointing at
        // nothing. `NAME` is the only spelling of any of them.
        for (index, name) in cap::ALL.iter().enumerate() {
            assert!(
                !cap::ALL.iter().take(index).any(|earlier| earlier == name),
                "{name} is declared twice"
            );
            let segments: Vec<&str> = name.split('.').collect();
            assert!(segments.len() >= 2, "{name} is not <noun>.<verb>");
            for segment in segments {
                assert!(!segment.is_empty(), "{name} has an empty segment");
                assert!(
                    segment.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                    "{name} is not lower-case ascii"
                );
            }
        }

        // The derived list is the marker types' own names, spot-checked against
        // the three the IPC catalogue used before they were declared.
        assert_eq!(cap::SaleVoid::NAME, "sale.void");
        assert_eq!(cap::SalePark::NAME, "sale.park");
        assert_eq!(cap::SaleResume::NAME, "sale.resume");
        assert_eq!(cap::CashMovement::NAME, "cash.movement");
        assert!(cap::ALL.contains(&cap::SaleVoid::NAME));

        // Role codes are `0004`'s CHECK list, and every one is distinct.
        assert_eq!(
            Role::ALL.map(Role::as_str),
            ["cashier", "shift_lead", "manager", "owner"]
        );
        // Limit tokens likewise: one spelling for an error message, a seed and a
        // report.
        assert_eq!(Limit::OwnShift.as_str(), "own_shift");
        assert_eq!(Limit::OwnStore.as_str(), "own_store");
        assert_eq!(Limit::ExactMatchOnly.as_str(), "exact_match_only");
        assert_eq!(Limit::RoleCap.as_str(), "role_cap");
        assert_eq!(Limit::RefundThreshold.as_str(), "refund_threshold");
    }
}
