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
}

impl ApprovalPredicate {
    /// The capability whose approvals the condition gates.
    #[must_use]
    pub const fn capability(self) -> &'static str {
        match self {
            ApprovalPredicate::DepartmentAmountAboveEscalation => cap::DepartmentSale::NAME,
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

/// Every command the shell registers, and nothing else. A command's
/// microstep adds its row in the change that registers the command.
pub const COMMANDS: &[CommandSpec] = &[CommandSpec {
    name: "split_tender",
    access: Access::NoCapability {
        reason: "Phase 0's smoke command: arithmetic over its own two arguments, \
                 reading and writing nothing",
    },
    audited: false,
    approval_requirement: ApprovalRequirement::Never,
    args: &[Arg::required("total_minor"), Arg::required("parts")],
}];

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
    #[error("{command} takes approval_id as {found:?}, where its requirement needs {expected:?}")]
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
/// actually takes.
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
