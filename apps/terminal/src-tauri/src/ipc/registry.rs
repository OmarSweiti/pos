//! The command registry: every IPC command, the capability its handler
//! proves, whether it audits, and what an approval for it is bound to
//! (`ref/ipc-contract.md` §2, microstep 1.6.7).
//!
//! The registry is the security surface. Hiding a button is UX; the check in
//! the handler is the security, and this table says which check each handler
//! owes. [`check`] holds it to the commands the shell actually registers and
//! to what an approval needs in order to be more than a PIN, and
//! `tests/ipc_contract.rs` runs it on every pull request.
//!
//! The commands are named once, in `lib.rs`, through [`commands!`]. The one
//! list makes both the handler `run` installs and [`crate::REGISTERED`], so
//! the walk cannot read a list the handler does not answer.

use pos_domain::{Capability, cap};

/// The argument every privileged command takes its approval in, by its Rust
/// name.
pub const APPROVAL_ARGUMENT: &str = "approval_id";

/// One IPC command, as its handler must treat it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    /// The command's name, which is its function's name.
    pub name: &'static str,
    /// The capability its handler proves, or why it proves none.
    pub access: Access,
    /// Whether every call writes an audit entry.
    pub audited: bool,
    /// Whether a manager's approval is needed, when, and what it is bound to.
    pub approval_requirement: ApprovalRequirement,
    /// What it takes from the webview, by the Rust names of its parameters.
    /// Tauri sends them in camelCase.
    pub args: &'static [Arg],
}

/// What a command's handler proves before it acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// The handler holds an `Authorized<C>` for this capability, named as
    /// `pos_domain::cap` names it.
    Capability(&'static str),
    /// The handler proves no capability, for this reason: the lock screen,
    /// first-run provisioning, or a restore that runs before the database
    /// holding the grants can open. A blank reason is refused, so "none" is
    /// always a decision someone can read.
    NoCapability { reason: &'static str },
}

/// One argument a command takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arg {
    pub name: &'static str,
    pub presence: Presence,
}

impl Arg {
    /// An argument every call must carry.
    #[must_use]
    pub const fn required(name: &'static str) -> Arg {
        Arg {
            name,
            presence: Presence::Required,
        }
    }

    /// An argument a call may leave out.
    #[must_use]
    pub const fn optional(name: &'static str) -> Arg {
        Arg {
            name,
            presence: Presence::Optional,
        }
    }
}

/// Whether a call must carry an argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Required,
    Optional,
}

/// Whether a command consumes a manager's approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalRequirement {
    /// Never. The command takes no `approval_id`.
    Never,
    /// On every call, so the command takes a required `approval_id`.
    Always { binding: ApprovalBindingSpec },
    /// When `predicate` holds, so the command takes an optional `approval_id`
    /// and refuses the privileged branch without one.
    Conditional {
        predicate: ApprovalPredicate,
        binding: ApprovalBindingSpec,
    },
}

/// When a conditionally privileged command needs an approval.
///
/// A closed set rather than a name or a flag. Each condition is a rule the
/// domain already evaluates, and `tests/ipc_contract.rs` drives it at its
/// boundary. A command that brings a new condition adds its variant here, and
/// the test's exhaustive match does not compile until that boundary has a
/// check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalPredicate {
    /// The amount is above the store's `DepartmentPolicy::escalate_above`
    /// (`ref/domain-api.md` §6.5). `add_department_line` evaluates it.
    DepartmentAmountAboveEscalation,
    /// The merchant's escalation policy names `price.override` (merchant
    /// decision 1.8). `authorize` evaluates it against the policy the command
    /// is given, which today names nothing (`ipc::context::merchant_policy`).
    PriceOverrideEscalatedByPolicy,
    /// The merchant's escalation policy names `user.admin` (the owner's
    /// decision of 10 November 2026 for a PIN reset). `authorize` evaluates
    /// it against the policy the command is given, as for a price override.
    UserAdminEscalatedByPolicy,
}

impl ApprovalPredicate {
    /// The capability whose approvals the condition gates.
    #[must_use]
    pub const fn capability(self) -> &'static str {
        match self {
            ApprovalPredicate::DepartmentAmountAboveEscalation => cap::DepartmentSale::NAME,
            ApprovalPredicate::PriceOverrideEscalatedByPolicy => cap::PriceOverride::NAME,
            ApprovalPredicate::UserAdminEscalatedByPolicy => cap::UserAdmin::NAME,
        }
    }
}

/// The exact operation an approval names: one entity, one amount, its prepared
/// content when there is any, and the reason.
///
/// Every part is required. An approval that leaves one open is a manager's PIN
/// reusable for a whole class of operations (`ref/ipc-contract.md` §1, rule 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApprovalBindingSpec {
    pub entity: EntityBindingSpec,
    pub amount: AmountBindingSpec,
    pub content: ContentBindingSpec,
    pub reason: ReasonBindingSpec,
}

/// Where the entity an approval names comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityBindingSpec {
    /// An argument naming it, such as `line_id`, `shift_id` or `queue_id`.
    Argument(&'static str),
    /// The sale the register has open, which a `cart_*` command acts on
    /// without naming it.
    OpenSale,
}

/// Where the amount an approval names comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmountBindingSpec {
    /// An argument, such as `amount_minor` or `unit_price_minor`.
    Argument(&'static str),
    /// A field of the prepared request or preview the approver was shown.
    PreparedEffect(&'static str),
    /// A fixed amount. An effect that moves no money binds exactly zero,
    /// never a wildcard.
    Exact(i64),
}

/// Where the reason an approval names comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasonBindingSpec {
    /// An argument, such as `reason`.
    Argument(&'static str),
    /// A field of the prepared request or preview the approver was shown.
    PreparedEffect(&'static str),
    /// The reason the approver gave when the handle was issued, which the
    /// audit row repeats. The command takes no reason of its own, as the
    /// department sale does not (1.4.12).
    Approval,
}

/// The prepared content an approval also names, when there is any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentBindingSpec {
    None,
    /// A prepared row whose digest the handler recomputes from every field,
    /// so the row cannot change under a stable id after it is approved.
    PreparedIntent {
        table: &'static str,
        hash_field: &'static str,
    },
}

/// An argument that carries money into a command, and why it is not an
/// uncontrolled price (`ref/ipc-contract.md` §1, rule 6; 1.4.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceArgument {
    pub command: &'static str,
    pub argument: &'static str,
    /// What the command's row must keep for the argument to be allowed.
    pub control: PriceControl,
}

/// What keeps a money-bearing argument from setting a sale's price
/// unchecked. [`check`] holds each row to its control, so a row that loses it
/// loses the argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceControl {
    /// The command proves a capability other than the base `sale.create` and
    /// writes an audit entry: the price override, and the capped department
    /// sale.
    AuditedCapability,
    /// The command proves `product.edit` and writes only a content-hashed
    /// proposal, which creates neither a product nor a line.
    InertProposal,
    /// Phase 0's smoke command, which proves no capability and reads and
    /// writes nothing: the amount is a total being split, not a price.
    TouchesNoSale,
    /// Money handed over toward the sale's own priced total (1.8.3). The
    /// handler builds a tender, which the domain's `add_tender` holds to what
    /// is owed, and no line or price is touched. The registry cannot see the
    /// handler; what it holds is the row's shape: the command proves the
    /// selling capability, `sale.create`, and takes no approval, because
    /// taking payment is selling.
    Tendered,
}

/// Every argument allowed to carry money, by command. Any other argument
/// whose name says money, with `price` anywhere in it or a minor-unit
/// `_minor` suffix, is refused on every command: a new one is a reviewed
/// line here, never a quiet addition to a signature. The rows for commands
/// not registered yet are the contract's (§3), so the step that registers
/// one meets its control already written.
pub const PRICE_ARGUMENTS: &[PriceArgument] = &[
    PriceArgument {
        command: "cart_override_price",
        argument: "unit_price_minor",
        control: PriceControl::AuditedCapability,
    },
    PriceArgument {
        command: "cart_add_department_sale",
        argument: "amount_minor",
        control: PriceControl::AuditedCapability,
    },
    PriceArgument {
        command: "product_quick_add_prepare",
        argument: "unit_price_minor",
        control: PriceControl::InertProposal,
    },
    PriceArgument {
        command: "split_tender",
        argument: "total_minor",
        control: PriceControl::TouchesNoSale,
    },
    PriceArgument {
        command: "tender_add_cash",
        argument: "amount_minor",
        control: PriceControl::Tendered,
    },
];

/// Whether an argument's name says it carries money.
#[must_use]
pub fn carries_money(argument: &str) -> bool {
    argument.contains("price") || argument.ends_with("_minor")
}

/// Every command the shell registers, and nothing else. A command's
/// microstep adds its row in the change that registers the command.
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "split_tender",
        access: Access::NoCapability {
            reason: "Phase 0's smoke command: arithmetic over its own two arguments, \
                     reading and writing nothing",
        },
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::required("total_minor"), Arg::required("parts")],
    },
    // 1.8.2c: the lock screen's three commands.
    CommandSpec {
        name: "session_state",
        access: Access::NoCapability {
            reason: "the lock screen asks who is signed in before anyone is, and \
                     the answer carries no grant, hash or token",
        },
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[],
    },
    CommandSpec {
        name: "auth_login_pin",
        access: Access::NoCapability {
            reason: "signing in is how a capability is first held: the PIN is the proof, \
                     under 1.6.2's attempt policy",
        },
        audited: true,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::required("user_code"), Arg::required("pin")],
    },
    CommandSpec {
        name: "auth_logout",
        access: Access::NoCapability {
            reason: "anyone signed in may end their own session, whatever they hold",
        },
        audited: true,
        approval_requirement: ApprovalRequirement::Never,
        args: &[],
    },
    // 1.4.11, its first part: building a basket. None takes a price; see
    // `PRICE_ARGUMENTS`.
    CommandSpec {
        name: "cart_open",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[],
    },
    CommandSpec {
        name: "cart_current",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[],
    },
    CommandSpec {
        name: "cart_add_line",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::required("product_id"), Arg::optional("qty_milli")],
    },
    CommandSpec {
        name: "cart_add_scan",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::required("raw_code")],
    },
    // Conditional on the merchant's policy, which escalates nothing yet. When
    // it does, the approval names this line, this price and this reason, and
    // is spent with the audit row every override writes.
    CommandSpec {
        name: "cart_override_price",
        access: Access::Capability(cap::PriceOverride::NAME),
        audited: true,
        approval_requirement: ApprovalRequirement::Conditional {
            predicate: ApprovalPredicate::PriceOverrideEscalatedByPolicy,
            binding: ApprovalBindingSpec {
                entity: EntityBindingSpec::Argument("line_id"),
                amount: AmountBindingSpec::Argument("unit_price_minor"),
                content: ContentBindingSpec::None,
                reason: ReasonBindingSpec::Argument("reason"),
            },
        },
        args: &[
            Arg::required("line_id"),
            Arg::required("unit_price_minor"),
            Arg::required("reason"),
            Arg::optional(APPROVAL_ARGUMENT),
        ],
    },
    CommandSpec {
        name: "cart_confirm_age",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: true,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::required("line_id"), Arg::required("confirmed")],
    },
    // 1.4.11, its second part: setting a basket aside and bringing it back.
    CommandSpec {
        name: "cart_park",
        access: Access::Capability(cap::SalePark::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::optional("label")],
    },
    CommandSpec {
        name: "cart_list_parked",
        access: Access::Capability(cap::SaleResume::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[],
    },
    // Not every call is audited: taking over another cashier's basket writes
    // `sale.resume`, and a cashier resuming their own writes nothing (#363).
    CommandSpec {
        name: "cart_resume",
        access: Access::Capability(cap::SaleResume::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::required("parked_id")],
    },
    // 1.8.3, its first part: paying for the basket.
    CommandSpec {
        name: "tender_begin",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[],
    },
    CommandSpec {
        name: "tender_add_cash",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::required("amount_minor")],
    },
    // Taking a tender back reverses money, so every call is audited.
    CommandSpec {
        name: "tender_remove",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: true,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::required("tender_id")],
    },
    CommandSpec {
        name: "tender_back_to_cart",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[],
    },
    CommandSpec {
        name: "sale_in_flight_state",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[],
    },
    // 1.8.4, its recovery half: a sale journalled mid-payment, taken up on the
    // till. Like `cart_resume`, it writes a row only when it takes over another
    // cashier's sale or abandons a checkout with nothing taken, so not on
    // every call.
    CommandSpec {
        name: "sale_resume_in_flight",
        access: Access::Capability(cap::SaleResume::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::required("sale_id")],
    },
    // 1.8.3, its third part: the sale completes. Audited: the row records what
    // the sale does not, the drawer's opening asked for before it happens.
    CommandSpec {
        name: "sale_finalize",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: true,
        approval_requirement: ApprovalRequirement::Never,
        args: &[],
    },
    // 1.9.5, its terminal half: a shift opened, read and closed. Opening and
    // closing each write their fact and its audit row in one envelope. The
    // float is cash counted into the drawer, not a price, and its argument's
    // name says so: see `PRICE_ARGUMENTS`.
    CommandSpec {
        name: "shift_open",
        access: Access::Capability(cap::ShiftOpen::NAME),
        audited: true,
        approval_requirement: ApprovalRequirement::Never,
        args: &[
            Arg::required("float_by_denomination"),
            Arg::optional("business_date"),
        ],
    },
    CommandSpec {
        name: "shift_current",
        access: Access::NoCapability {
            reason: "the lock screen names whose shift is open before anyone signs in, and \
                     the answer is the opening fact, with no grant, PIN or key",
        },
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[],
    },
    // The opener's own close takes no approval: closing your own work is not
    // an escalation. Another user's stale shift is Phase 2's
    // `shift_force_close_stale`.
    CommandSpec {
        name: "shift_close",
        access: Access::Capability(cap::ShiftClose::NAME),
        audited: true,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::required("shift_id")],
    },
    // 1.6.2, its command: a forgotten PIN reset, audited with its reason.
    // Conditional on the merchant's policy, which escalates nothing yet (the
    // owner's decision of 10 November 2026). When it does, the approval names
    // the account being reset, exactly zero, and this reason, and is spent
    // with the audit row.
    CommandSpec {
        name: "user_reset_pin",
        access: Access::Capability(cap::UserAdmin::NAME),
        audited: true,
        approval_requirement: ApprovalRequirement::Conditional {
            predicate: ApprovalPredicate::UserAdminEscalatedByPolicy,
            binding: ApprovalBindingSpec {
                entity: EntityBindingSpec::Argument("user_id"),
                amount: AmountBindingSpec::Exact(0),
                content: ContentBindingSpec::None,
                reason: ReasonBindingSpec::Argument("reason"),
            },
        },
        args: &[
            Arg::required("user_id"),
            Arg::required("new_pin"),
            Arg::required("reason"),
            Arg::optional(APPROVAL_ARGUMENT),
        ],
    },
    // 1.10.5, its terminal half: stock read, and moved by hand. Preparing
    // stores a register-local request and moves nothing, so it is not
    // audited; posting it is privileged on every call.
    CommandSpec {
        name: "stock_on_hand",
        access: Access::Capability(cap::SaleCreate::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[Arg::required("product_id")],
    },
    // One person moves stock at once, audited, under the shipped policy (the
    // owner's decision of 10 November 2026). It takes no approval: where a
    // merchant's policy names `stock.adjust` it is refused, and the request
    // below is prepared, approved and posted instead.
    CommandSpec {
        name: "stock_adjust_now",
        access: Access::Capability(cap::StockAdjust::NAME),
        audited: true,
        approval_requirement: ApprovalRequirement::Never,
        args: &[
            Arg::required("product_id"),
            Arg::required("qty_delta_milli"),
            Arg::required("reason_code"),
            Arg::optional("note"),
        ],
    },
    CommandSpec {
        name: "stock_adjust_prepare",
        access: Access::Capability(cap::StockAdjust::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[
            Arg::required("product_id"),
            Arg::required("qty_delta_milli"),
            Arg::required("reason_code"),
            Arg::optional("note"),
        ],
    },
    // The approval names the event the request will post as, exactly zero,
    // and the request's digest, which binds its reason code with every other
    // column. The handler recomputes the digest from the stored row before
    // the effect, and `0006` refuses any change to the row once an approval
    // names it.
    CommandSpec {
        name: "stock_adjust",
        access: Access::Capability(cap::StockAdjust::NAME),
        audited: true,
        approval_requirement: ApprovalRequirement::Always {
            binding: ApprovalBindingSpec {
                entity: EntityBindingSpec::Argument("stock_event_id"),
                amount: AmountBindingSpec::Exact(0),
                content: ContentBindingSpec::PreparedIntent {
                    table: "stock_adjustment_request",
                    hash_field: "content_hash",
                },
                reason: ReasonBindingSpec::PreparedEffect("reason_code"),
            },
        },
        args: &[
            Arg::required("stock_event_id"),
            Arg::required(APPROVAL_ARGUMENT),
        ],
    },
    // 1.7.6b: the register's device health, which the lock screen shows.
    CommandSpec {
        name: "health_status",
        access: Access::NoCapability {
            reason: "the lock screen shows the register's health before anyone signs in: the \
                     printer as read now and how many receipts wait, with no grant, PIN, key \
                     or receipt content",
        },
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: &[],
    },
];

/// Names the commands the shell registers, once.
///
/// It expands to [`crate::REGISTERED`], the list [`check`] walks, and to the
/// `invoke_handler` that `run` installs, both from the same tokens. So the
/// walk reads exactly the list `tauri::generate_handler!` registers. It is
/// invoked once, in `lib.rs`, where the commands' paths resolve as the
/// handler needs them to.
macro_rules! commands {
    ($($($segment:ident)::+),+ $(,)?) => {
        /// Every command the webview can invoke, as the handler registers it.
        /// `ipc::registry::check` holds it to the registry.
        pub const REGISTERED: &[&str] = &[$(stringify!($($segment)::+)),+];

        /// The one invoke handler the shell installs.
        fn invoke_handler<R: tauri::Runtime>()
        -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
            tauri::generate_handler![$($($segment)::+),+]
        }
    };
}
pub(crate) use commands;

/// What [`check`] refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// The handler registers a command the registry does not describe.
    #[error("{0} is registered with the handler but has no CommandSpec")]
    Unspecified(&'static str),
    /// The registry describes a command the handler does not register.
    #[error("{0} has a CommandSpec but the handler does not register it")]
    Unregistered(&'static str),
    #[error("{0} is registered with the handler more than once")]
    DuplicateRegistration(&'static str),
    #[error("{0} has more than one CommandSpec")]
    DuplicateSpec(&'static str),
    #[error("{command} requires {capability}, which is not a capability")]
    UnknownCapability {
        command: &'static str,
        capability: &'static str,
    },
    #[error("{0} proves no capability and does not say why")]
    UnexplainedAccess(&'static str),
    /// `approval_id` is not taken the way the requirement needs: required
    /// when `Always`, optional when `Conditional`, absent when `Never`.
    #[error(
        "{command} declares approval_id {}; its requirement needs it {}",
        declared(*.found),
        declared(*.expected)
    )]
    ApprovalArgument {
        command: &'static str,
        expected: Option<Presence>,
        found: Option<Presence>,
    },
    /// A handle is issued for one capability, so a command proving none has
    /// nothing to issue it under.
    #[error("{0} takes an approval but proves no capability")]
    PrivilegedWithoutCapability(&'static str),
    /// The database consumes a handle only beside the audit row of its effect.
    #[error("{0} takes an approval but writes no audit entry")]
    PrivilegedUnaudited(&'static str),
    #[error("{command}'s approval condition gates {gates}, not {capability}")]
    PredicateOfAnotherCapability {
        command: &'static str,
        capability: &'static str,
        gates: &'static str,
    },
    #[error("{command}'s approval binding leaves its {part} unnamed")]
    BindingIncomplete {
        command: &'static str,
        part: &'static str,
    },
    #[error("{command}'s approval binds its {part} to {argument}, which it does not take")]
    BindingArgumentUndeclared {
        command: &'static str,
        part: &'static str,
        argument: &'static str,
    },
    /// A binding to an argument a call may leave out binds to nothing.
    #[error("{command}'s approval binds its {part} to {argument}, which is optional")]
    BindingArgumentOptional {
        command: &'static str,
        part: &'static str,
        argument: &'static str,
    },
    /// An argument that carries money where no reviewed line allows it: a
    /// price a base command would take unchecked.
    #[error("{command} takes {argument}, which carries money and is not a reviewed price argument")]
    PriceArgument {
        command: &'static str,
        argument: &'static str,
    },
    /// A reviewed price argument on a row that no longer keeps its control.
    #[error("{command} takes {argument} without the control that allows it")]
    PriceArgumentUncontrolled {
        command: &'static str,
        argument: &'static str,
    },
}

/// How an argument is declared, in a refusal's words.
const fn declared(presence: Option<Presence>) -> &'static str {
    match presence {
        Some(Presence::Required) => "required",
        Some(Presence::Optional) => "optional",
        None => "absent",
    }
}

/// Hold `commands` to the commands the handler registers, and each of them to
/// the contract.
///
/// `registered` is [`crate::REGISTERED`]: each entry the path the handler was
/// given, whose last segment is the command's name. Every registered command
/// has exactly one spec and every spec a registered command. Every spec names
/// a real capability or says why it needs none. Every privileged spec proves a
/// capability, audits, takes `approval_id` as its requirement says, and binds
/// its entity, amount, reason and any prepared content to something it
/// actually takes. And no spec takes money but through a [`PRICE_ARGUMENTS`]
/// line whose control it keeps.
///
/// # Errors
///
/// Every [`RegistryError`] found, in the order of `registered` and then of
/// `commands`.
pub fn check(
    commands: &[CommandSpec],
    registered: &[&'static str],
) -> Result<(), Vec<RegistryError>> {
    let mut errors = Vec::new();
    let names: Vec<&'static str> = registered.iter().map(|path| command_name(path)).collect();
    for (position, &name) in names.iter().enumerate() {
        if names.iter().take(position).any(|&earlier| earlier == name) {
            errors.push(RegistryError::DuplicateRegistration(name));
        }
        if !commands.iter().any(|spec| spec.name == name) {
            errors.push(RegistryError::Unspecified(name));
        }
    }
    for (position, spec) in commands.iter().enumerate() {
        if commands
            .iter()
            .take(position)
            .any(|earlier| earlier.name == spec.name)
        {
            errors.push(RegistryError::DuplicateSpec(spec.name));
        }
        if !names.contains(&spec.name) {
            errors.push(RegistryError::Unregistered(spec.name));
        }
        check_spec(spec, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// The command a handler path registers: its last segment.
fn command_name(path: &'static str) -> &'static str {
    path.rsplit("::").next().unwrap_or(path).trim()
}

fn check_spec(spec: &CommandSpec, errors: &mut Vec<RegistryError>) {
    let command = spec.name;
    let capability = match spec.access {
        Access::Capability(capability) => {
            if !cap::ALL.contains(&capability) {
                errors.push(RegistryError::UnknownCapability {
                    command,
                    capability,
                });
            }
            Some(capability)
        }
        Access::NoCapability { reason } => {
            if reason.trim().is_empty() {
                errors.push(RegistryError::UnexplainedAccess(command));
            }
            None
        }
    };

    let (expected, privileged) = match spec.approval_requirement {
        ApprovalRequirement::Never => (None, None),
        ApprovalRequirement::Always { binding } => {
            (Some(Presence::Required), Some((binding, None)))
        }
        ApprovalRequirement::Conditional { predicate, binding } => {
            (Some(Presence::Optional), Some((binding, Some(predicate))))
        }
    };
    check_price_arguments(spec, errors);

    let found = presence(spec, APPROVAL_ARGUMENT);
    if found != expected {
        errors.push(RegistryError::ApprovalArgument {
            command,
            expected,
            found,
        });
    }

    let Some((binding, predicate)) = privileged else {
        return;
    };
    match capability {
        None => errors.push(RegistryError::PrivilegedWithoutCapability(command)),
        Some(capability) => {
            if let Some(gates) = predicate.map(ApprovalPredicate::capability)
                && gates != capability
            {
                errors.push(RegistryError::PredicateOfAnotherCapability {
                    command,
                    capability,
                    gates,
                });
            }
        }
    }
    if !spec.audited {
        errors.push(RegistryError::PrivilegedUnaudited(command));
    }
    check_binding(spec, &binding, errors);
}

/// Refuse an argument that carries money unless a [`PRICE_ARGUMENTS`] line
/// allows it on this command, and the row keeps that line's control.
fn check_price_arguments(spec: &CommandSpec, errors: &mut Vec<RegistryError>) {
    for arg in spec.args.iter().filter(|arg| carries_money(arg.name)) {
        let allowed = PRICE_ARGUMENTS
            .iter()
            .find(|allowed| allowed.command == spec.name && allowed.argument == arg.name);
        let Some(allowed) = allowed else {
            errors.push(RegistryError::PriceArgument {
                command: spec.name,
                argument: arg.name,
            });
            continue;
        };
        let controlled = match allowed.control {
            PriceControl::AuditedCapability => {
                spec.audited
                    && matches!(spec.access, Access::Capability(held) if held != cap::SaleCreate::NAME)
            }
            PriceControl::InertProposal => {
                spec.access == Access::Capability(cap::ProductEdit::NAME)
            }
            PriceControl::TouchesNoSale => {
                matches!(spec.access, Access::NoCapability { .. })
                    && !spec.audited
                    && spec.approval_requirement == ApprovalRequirement::Never
            }
            PriceControl::Tendered => {
                spec.access == Access::Capability(cap::SaleCreate::NAME)
                    && spec.approval_requirement == ApprovalRequirement::Never
            }
        };
        if !controlled {
            errors.push(RegistryError::PriceArgumentUncontrolled {
                command: spec.name,
                argument: arg.name,
            });
        }
    }
}

fn check_binding(
    spec: &CommandSpec,
    binding: &ApprovalBindingSpec,
    errors: &mut Vec<RegistryError>,
) {
    match binding.entity {
        EntityBindingSpec::Argument(argument) => bound_argument(spec, "entity", argument, errors),
        EntityBindingSpec::OpenSale => {}
    }
    match binding.amount {
        AmountBindingSpec::Argument(argument) => bound_argument(spec, "amount", argument, errors),
        AmountBindingSpec::PreparedEffect(field) => named(spec, "amount", field, errors),
        AmountBindingSpec::Exact(_) => {}
    }
    match binding.content {
        ContentBindingSpec::None => {}
        ContentBindingSpec::PreparedIntent { table, hash_field } => {
            named(spec, "content table", table, errors);
            named(spec, "content hash field", hash_field, errors);
        }
    }
    match binding.reason {
        ReasonBindingSpec::Argument(argument) => bound_argument(spec, "reason", argument, errors),
        ReasonBindingSpec::PreparedEffect(field) => named(spec, "reason", field, errors),
        ReasonBindingSpec::Approval => {}
    }
}

/// Refuse a binding `part` whose name is blank.
fn named(spec: &CommandSpec, part: &'static str, name: &str, errors: &mut Vec<RegistryError>) {
    if name.trim().is_empty() {
        errors.push(RegistryError::BindingIncomplete {
            command: spec.name,
            part,
        });
    }
}

/// Refuse a binding `part` to an argument the command does not take, or may
/// be called without.
fn bound_argument(
    spec: &CommandSpec,
    part: &'static str,
    argument: &'static str,
    errors: &mut Vec<RegistryError>,
) {
    if argument.trim().is_empty() {
        errors.push(RegistryError::BindingIncomplete {
            command: spec.name,
            part,
        });
        return;
    }
    match presence(spec, argument) {
        Some(Presence::Required) => {}
        Some(Presence::Optional) => errors.push(RegistryError::BindingArgumentOptional {
            command: spec.name,
            part,
            argument,
        }),
        None => errors.push(RegistryError::BindingArgumentUndeclared {
            command: spec.name,
            part,
            argument,
        }),
    }
}

fn presence(spec: &CommandSpec, argument: &str) -> Option<Presence> {
    spec.args
        .iter()
        .find(|arg| arg.name == argument)
        .map(|arg| arg.presence)
}
