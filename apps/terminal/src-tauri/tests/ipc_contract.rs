//! Microstep 1.6.7: every IPC command says what its handler proves, and every
//! approval a command takes is bound to one operation (`ref/ipc-contract.md`
//! §2 and §5).
//!
//! The registry holds only commands the shell registers. Its one privileged
//! row is `cart_override_price` (1.4.11), conditional on the merchant's
//! escalation policy. The other privileged rows below are taken from the
//! contract's catalogue as the microsteps that register those commands will
//! write them. The validator has to accept them, so each refusal here shows a
//! rule rather than a validator that refuses every privileged row.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use pos_domain::cart::{AddLine, add_department_line, add_line, open, override_price};
use pos_domain::{
    ApprovalBinding, ApprovalHandle, ApprovalId, AuditIntent, Authorized, Capability, CartContext,
    CartError, CategoryId, Currency, Department, DepartmentPolicy, EscalationPolicy, GrantSet,
    Money, OverrideReason, PermissionError, Product, ProductId, Qty, RegisterId, RegulatedSaleForm,
    Role, SaleId, SaleLineId, ShiftId, TaxCategoryId, Timestamp, UnitOfMeasure, UserId, authorize,
    cap,
};
use terminal_lib::REGISTERED;
use terminal_lib::ipc::registry::{
    APPROVAL_ARGUMENT, Access, AmountBindingSpec, ApprovalBindingSpec, ApprovalPredicate,
    ApprovalRequirement, Arg, COMMANDS, CommandSpec, ContentBindingSpec, EntityBindingSpec,
    PRICE_ARGUMENTS, Presence, ReasonBindingSpec, RegistryError, carries_money, check,
};
use uuid::Uuid;

// ── Rows from the contract's catalogue (`ref/ipc-contract.md` §3) ──────────

/// `stock_adjust`: entity `stock_event_id`, exactly zero, the prepared reason
/// and the prepared request's digest.
const STOCK_ADJUST: CommandSpec = CommandSpec {
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
};

/// `fiscal_rebuild_failed`: entity `queue_id`, exactly zero, no content, the
/// `reason` argument.
const FISCAL_REBUILD_FAILED: CommandSpec = CommandSpec {
    name: "fiscal_rebuild_failed",
    access: Access::Capability(cap::FiscalRemediate::NAME),
    audited: true,
    approval_requirement: ApprovalRequirement::Always {
        binding: ApprovalBindingSpec {
            entity: EntityBindingSpec::Argument("queue_id"),
            amount: AmountBindingSpec::Exact(0),
            content: ContentBindingSpec::None,
            reason: ReasonBindingSpec::Argument("reason"),
        },
    },
    args: &[
        Arg::required("queue_id"),
        Arg::required("reason"),
        Arg::required(APPROVAL_ARGUMENT),
    ],
};

/// `shift_force_close_stale`: the stale `shift_id`, exactly zero and the
/// supplied reason.
const SHIFT_FORCE_CLOSE_STALE: CommandSpec = CommandSpec {
    name: "shift_force_close_stale",
    access: Access::Capability(cap::ShiftClose::NAME),
    audited: true,
    approval_requirement: ApprovalRequirement::Always {
        binding: ApprovalBindingSpec {
            entity: EntityBindingSpec::Argument("shift_id"),
            amount: AmountBindingSpec::Exact(0),
            content: ContentBindingSpec::None,
            reason: ReasonBindingSpec::Argument("reason"),
        },
    },
    args: &[
        Arg::required("shift_id"),
        Arg::required("reason"),
        Arg::required(APPROVAL_ARGUMENT),
    ],
};

/// `cart_add_department_sale`, as 1.4.12's domain half binds it: the open
/// sale, the typed `amount_minor`, and the reason the manager gave.
const CART_ADD_DEPARTMENT_SALE: CommandSpec = CommandSpec {
    name: "cart_add_department_sale",
    access: Access::Capability(cap::DepartmentSale::NAME),
    audited: true,
    approval_requirement: ApprovalRequirement::Conditional {
        predicate: ApprovalPredicate::DepartmentAmountAboveEscalation,
        binding: ApprovalBindingSpec {
            entity: EntityBindingSpec::OpenSale,
            amount: AmountBindingSpec::Argument("amount_minor"),
            content: ContentBindingSpec::None,
            reason: ReasonBindingSpec::Approval,
        },
    },
    args: DEPARTMENT_ARGS,
};

const DEPARTMENT_ARGS: &[Arg] = &[
    Arg::required("department_id"),
    Arg::required("amount_minor"),
    Arg::optional("scanned_code"),
    Arg::optional("note"),
    Arg::optional(APPROVAL_ARGUMENT),
];

const CONTRACT_ROWS: &[CommandSpec] = &[
    STOCK_ADJUST,
    FISCAL_REBUILD_FAILED,
    SHIFT_FORCE_CLOSE_STALE,
    CART_ADD_DEPARTMENT_SALE,
];

/// What `check` says about `rows` registered exactly as they are named.
fn refusals(rows: &[CommandSpec]) -> Vec<RegistryError> {
    let names: Vec<&'static str> = rows.iter().map(|row| row.name).collect();
    check(rows, &names).err().unwrap_or_default()
}

fn binding_of(row: CommandSpec) -> ApprovalBindingSpec {
    match row.approval_requirement {
        ApprovalRequirement::Always { binding }
        | ApprovalRequirement::Conditional { binding, .. } => binding,
        ApprovalRequirement::Never => panic!("{} takes no approval", row.name),
    }
}

/// `row` with its approval bound by `binding` instead.
fn rebound(row: CommandSpec, binding: ApprovalBindingSpec) -> CommandSpec {
    let approval_requirement = match row.approval_requirement {
        ApprovalRequirement::Always { .. } => ApprovalRequirement::Always { binding },
        ApprovalRequirement::Conditional { predicate, .. } => {
            ApprovalRequirement::Conditional { predicate, binding }
        }
        ApprovalRequirement::Never => panic!("{} takes no approval", row.name),
    };
    CommandSpec {
        approval_requirement,
        ..row
    }
}

fn is_privileged(row: &CommandSpec) -> bool {
    !matches!(row.approval_requirement, ApprovalRequirement::Never)
}

fn split_tender() -> CommandSpec {
    *COMMANDS
        .iter()
        .find(|row| row.name == "split_tender")
        .unwrap()
}

// ── The shell's own source ──────────────────────────────────────────────────

/// Every `.rs` file under the shell's `src`, with its comment lines removed.
fn shell_source() -> Vec<(PathBuf, String)> {
    let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let code: String = fs::read_to_string(&path)
                    .unwrap()
                    .lines()
                    .filter(|line| !line.trim_start().starts_with("//"))
                    .map(|line| format!("{line}\n"))
                    .collect();
                files.push((path, code));
            }
        }
    }
    files
}

/// The files whose code contains `needle`, and how many times each does.
fn spelled(source: &[(PathBuf, String)], needle: &str) -> Vec<(String, usize)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    source
        .iter()
        .filter(|(_, code)| code.contains(needle))
        .map(|(path, code)| {
            let relative = path.strip_prefix(root).unwrap().to_string_lossy();
            (relative.replace('\\', "/"), code.matches(needle).count())
        })
        .collect()
}

/// The walk reads the list the handler registers only while the shell has
/// one list. A second `generate_handler!`, or a second `invoke_handler`
/// replacing the first, would register commands the walk never sees.
fn the_shell_names_its_commands_once() {
    let source = shell_source();
    assert_eq!(
        spelled(&source, "generate_handler!"),
        vec![("src/ipc/registry.rs".to_owned(), 1)],
        "the handler's list is spelled once, inside `commands!`"
    );
    assert_eq!(
        spelled(&source, ".invoke_handler("),
        vec![("src/lib.rs".to_owned(), 1)]
    );
    assert_eq!(
        spelled(&source, ".invoke_handler(invoke_handler())"),
        vec![("src/lib.rs".to_owned(), 1)],
        "`run` installs the handler `commands!` made"
    );
    // A plugin's commands are not in `generate_handler!`'s list. The first
    // plugin the shell registers extends this walk to them
    // (`ref/ipc-contract.md` §3).
    assert_eq!(spelled(&source, ".plugin("), Vec::new());
    // The walk names a command by its function. A renamed command would
    // reach the webview under a name the registry does not hold.
    for (path, code) in &source {
        for line in code.lines().filter(|line| line.contains("tauri::command(")) {
            assert!(
                !line.replace("rename_all", "").contains("rename"),
                "{} renames a command: {line}",
                path.display()
            );
        }
    }
}

// ── The five tests ──────────────────────────────────────────────────────────

/// Every registered command has exactly one row, every row a registered
/// command, and every row names the capability its handler proves, or says
/// why it proves none.
#[test]
fn ipc_commands_all_declare_a_capability() {
    the_shell_names_its_commands_once();
    assert_eq!(check(COMMANDS, REGISTERED), Ok(()));
    for row in COMMANDS {
        match row.access {
            Access::Capability(capability) => assert!(cap::ALL.contains(&capability)),
            Access::NoCapability { reason } => assert!(!reason.trim().is_empty()),
        }
    }

    // A capability the domain does not define is refused, so a misspelt one
    // cannot check nothing.
    let misspelt = CommandSpec {
        access: Access::Capability("stock.adjsut"),
        ..STOCK_ADJUST
    };
    assert_eq!(
        refusals(&[misspelt]),
        vec![RegistryError::UnknownCapability {
            command: "stock_adjust",
            capability: "stock.adjsut",
        }]
    );
    // And "none" without its reason is refused.
    let unexplained = CommandSpec {
        access: Access::NoCapability { reason: "  " },
        ..split_tender()
    };
    assert_eq!(
        refusals(&[unexplained]),
        vec![RegistryError::UnexplainedAccess("split_tender")]
    );
}

/// An `Always` or `Conditional` row binds one entity, one amount, its content
/// and its reason; proves a capability for the handle to be issued under;
/// writes the audit entry the handle is consumed beside; and takes
/// `approval_id` as its requirement says.
#[test]
fn every_privileged_command_binds_its_approval() {
    let privileged: Vec<CommandSpec> = COMMANDS
        .iter()
        .chain(CONTRACT_ROWS)
        .copied()
        .filter(is_privileged)
        .collect();
    // The registry's privileged rows, the conditional override and the
    // always-privileged reset, then the contract's.
    let names: Vec<&str> = privileged.iter().map(|row| row.name).collect();
    let expected: Vec<&str> = ["cart_override_price", "user_reset_pin"]
        .into_iter()
        .chain(CONTRACT_ROWS.iter().map(|row| row.name))
        .collect();
    assert_eq!(names, expected);
    for row in &privileged {
        assert_eq!(refusals(&[*row]), Vec::new(), "{}", row.name);
    }

    // An approval nothing can be issued under, or consumed beside.
    let open = CommandSpec {
        access: Access::NoCapability {
            reason: "lock screen",
        },
        ..STOCK_ADJUST
    };
    assert_eq!(
        refusals(&[open]),
        vec![RegistryError::PrivilegedWithoutCapability("stock_adjust")]
    );
    let silent = CommandSpec {
        audited: false,
        ..STOCK_ADJUST
    };
    assert_eq!(
        refusals(&[silent]),
        vec![RegistryError::PrivilegedUnaudited("stock_adjust")]
    );

    // An always-privileged command that may be called without its approval
    // has a branch that runs without one.
    const OPTIONAL: &[Arg] = &[
        Arg::required("stock_event_id"),
        Arg::optional(APPROVAL_ARGUMENT),
    ];
    let optional = CommandSpec {
        args: OPTIONAL,
        ..STOCK_ADJUST
    };
    assert_eq!(
        refusals(&[optional]),
        vec![RegistryError::ApprovalArgument {
            command: "stock_adjust",
            expected: Some(Presence::Required),
            found: Some(Presence::Optional),
        }]
    );
    // And a command that consumes no approval takes none.
    const STRAY: &[Arg] = &[
        Arg::required("total_minor"),
        Arg::required("parts"),
        Arg::optional(APPROVAL_ARGUMENT),
    ];
    let stray = CommandSpec {
        args: STRAY,
        ..split_tender()
    };
    assert_eq!(
        refusals(&[stray]),
        vec![RegistryError::ApprovalArgument {
            command: "split_tender",
            expected: None,
            found: Some(Presence::Optional),
        }]
    );
}

/// A conditional command's condition is a rule the domain evaluates, held at
/// its boundary: at it no approval is needed, one minor unit past it the
/// branch is refused without a handle bound to this operation. And the
/// command takes `approval_id?`, so the branch can be crossed at all.
#[test]
fn conditional_privilege_cannot_cross_threshold_without_approval() {
    let predicates: Vec<ApprovalPredicate> = COMMANDS
        .iter()
        .chain(CONTRACT_ROWS)
        .filter_map(|row| match row.approval_requirement {
            ApprovalRequirement::Conditional { predicate, .. } => Some(predicate),
            ApprovalRequirement::Always { .. } | ApprovalRequirement::Never => None,
        })
        .collect();
    assert_eq!(
        predicates,
        vec![
            ApprovalPredicate::PriceOverrideEscalatedByPolicy,
            ApprovalPredicate::DepartmentAmountAboveEscalation,
        ]
    );
    for predicate in predicates {
        holds_at_its_boundary(predicate);
    }

    // A conditional command whose schema omits `approval_id?` has no way to
    // cross its threshold; one that requires it is not conditional.
    const WITHOUT: &[Arg] = &[
        Arg::required("department_id"),
        Arg::required("amount_minor"),
        Arg::optional("scanned_code"),
        Arg::optional("note"),
    ];
    const REQUIRED: &[Arg] = &[
        Arg::required("department_id"),
        Arg::required("amount_minor"),
        Arg::optional("scanned_code"),
        Arg::optional("note"),
        Arg::required(APPROVAL_ARGUMENT),
    ];
    for (args, found) in [(WITHOUT, None), (REQUIRED, Some(Presence::Required))] {
        let row = CommandSpec {
            args,
            ..CART_ADD_DEPARTMENT_SALE
        };
        assert_eq!(
            refusals(&[row]),
            vec![RegistryError::ApprovalArgument {
                command: "cart_add_department_sale",
                expected: Some(Presence::Optional),
                found,
            }]
        );
    }
    // The condition gates its own capability's approvals, not another's.
    let borrowed = CommandSpec {
        access: Access::Capability(cap::DiscountManual::NAME),
        ..CART_ADD_DEPARTMENT_SALE
    };
    assert_eq!(
        refusals(&[borrowed]),
        vec![RegistryError::PredicateOfAnotherCapability {
            command: "cart_add_department_sale",
            capability: "discount.manual",
            gates: "sale.department",
        }]
    );
}

/// A command the handler registers without a row is refused: that is what
/// adding a command without declaring what it requires looks like.
#[test]
fn a_registry_fixture_without_a_command_spec_is_refused() {
    // The shell's own list, and one command more.
    let with = |extra: &'static str| -> Vec<&'static str> {
        REGISTERED.iter().copied().chain([extra]).collect()
    };
    assert_eq!(
        check(COMMANDS, &with("cart_void_sale")),
        Err(vec![RegistryError::Unspecified("cart_void_sale")])
    );
    // The handler registers a path under its last segment.
    assert_eq!(
        check(COMMANDS, &with("commands :: cart :: cart_void_sale")),
        Err(vec![RegistryError::Unspecified("cart_void_sale")])
    );
    assert_eq!(
        check(&[], &["split_tender"]),
        Err(vec![RegistryError::Unspecified("split_tender")])
    );
    // The other direction: a row for a command nothing registers.
    let mut rows = COMMANDS.to_vec();
    rows.push(STOCK_ADJUST);
    assert_eq!(
        check(&rows, REGISTERED),
        Err(vec![RegistryError::Unregistered("stock_adjust")])
    );
    // And neither list may name a command twice.
    assert_eq!(
        check(&[split_tender(), split_tender()], &["split_tender"]),
        Err(vec![RegistryError::DuplicateSpec("split_tender")])
    );
    assert_eq!(
        check(&[split_tender()], &["split_tender", "split_tender"]),
        Err(vec![RegistryError::DuplicateRegistration("split_tender")])
    );
}

/// A privileged row cannot be written without its binding at all, because
/// `Always` and `Conditional` carry one: that refusal is the compiler's. What
/// is left to refuse is a binding with a part left open, or bound to an
/// argument the command does not take or may be called without.
#[test]
fn a_privileged_fixture_without_a_binding_is_refused() {
    let adjust = binding_of(STOCK_ADJUST);
    let blank = |command: &'static str, part: &'static str| RegistryError::BindingIncomplete {
        command,
        part,
    };
    let cases = [
        (
            ApprovalBindingSpec {
                entity: EntityBindingSpec::Argument(""),
                ..adjust
            },
            blank("stock_adjust", "entity"),
        ),
        (
            ApprovalBindingSpec {
                amount: AmountBindingSpec::PreparedEffect(" "),
                ..adjust
            },
            blank("stock_adjust", "amount"),
        ),
        (
            ApprovalBindingSpec {
                reason: ReasonBindingSpec::PreparedEffect(""),
                ..adjust
            },
            blank("stock_adjust", "reason"),
        ),
        (
            ApprovalBindingSpec {
                content: ContentBindingSpec::PreparedIntent {
                    table: "",
                    hash_field: "content_hash",
                },
                ..adjust
            },
            blank("stock_adjust", "content table"),
        ),
        (
            ApprovalBindingSpec {
                content: ContentBindingSpec::PreparedIntent {
                    table: "stock_adjustment_request",
                    hash_field: "",
                },
                ..adjust
            },
            blank("stock_adjust", "content hash field"),
        ),
        (
            ApprovalBindingSpec {
                entity: EntityBindingSpec::Argument("sale_id"),
                ..adjust
            },
            RegistryError::BindingArgumentUndeclared {
                command: "stock_adjust",
                part: "entity",
                argument: "sale_id",
            },
        ),
    ];
    for (binding, refusal) in cases {
        assert_eq!(refusals(&[rebound(STOCK_ADJUST, binding)]), vec![refusal]);
    }

    let department = binding_of(CART_ADD_DEPARTMENT_SALE);
    for (binding, refusal) in [
        (
            ApprovalBindingSpec {
                amount: AmountBindingSpec::Argument(""),
                ..department
            },
            blank("cart_add_department_sale", "amount"),
        ),
        (
            ApprovalBindingSpec {
                reason: ReasonBindingSpec::Argument("reason"),
                ..department
            },
            RegistryError::BindingArgumentUndeclared {
                command: "cart_add_department_sale",
                part: "reason",
                argument: "reason",
            },
        ),
        // A binding to an argument a call may leave out binds to nothing.
        (
            ApprovalBindingSpec {
                reason: ReasonBindingSpec::Argument("note"),
                ..department
            },
            RegistryError::BindingArgumentOptional {
                command: "cart_add_department_sale",
                part: "reason",
                argument: "note",
            },
        ),
    ] {
        assert_eq!(
            refusals(&[rebound(CART_ADD_DEPARTMENT_SALE, binding)]),
            vec![refusal]
        );
    }
}

// ── The handlers' own signatures (1.4.11) ───────────────────────────────────

/// The arguments the handler `command` takes from the webview, read from its
/// signature in the shell's source: every parameter but those Tauri injects,
/// and whether a call may leave it out. Sorted by name, because Tauri finds
/// each argument by its name. `None` when no `#[tauri::command]` function has
/// that name.
fn handler_schema(source: &[(PathBuf, String)], command: &str) -> Option<Vec<(String, Presence)>> {
    let mut found = Vec::new();
    for (_, code) in source {
        for after in code.split("#[tauri::command").skip(1) {
            let Some((_, after_fn)) = after.split_once("fn ") else {
                continue;
            };
            let Some((name, params)) = after_fn.split_once('(') else {
                continue;
            };
            if name.trim() == command {
                found.push(webview_arguments(params));
            }
        }
    }
    assert!(found.len() <= 1, "{command} is defined as a command twice");
    found.pop()
}

/// The parameters of a signature whose text starts after its `(`, as
/// `(name, presence)`, without the ones Tauri injects.
fn webview_arguments(params: &str) -> Vec<(String, Presence)> {
    let mut depth = 0_i32;
    let mut current = String::new();
    let mut each = Vec::new();
    for c in params.chars() {
        match c {
            ')' if depth == 0 => break,
            '(' | '<' | '[' => depth += 1,
            ')' | '>' | ']' => depth -= 1,
            ',' if depth == 0 => {
                each.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    each.push(current);
    let mut schema: Vec<(String, Presence)> = each
        .iter()
        .filter_map(|param| param.split_once(':'))
        .map(|(name, ty)| (name.trim().trim_start_matches("mut ").to_owned(), ty.trim()))
        .filter(|(_, ty)| !injected(ty))
        .map(|(name, ty)| {
            let presence = if ty.starts_with("Option<") {
                Presence::Optional
            } else {
                Presence::Required
            };
            (name, presence)
        })
        .collect();
    schema.sort_by(|a, b| a.0.cmp(&b.0));
    schema
}

/// A parameter Tauri supplies itself, which the webview never sends.
fn injected(ty: &str) -> bool {
    let ty = ty.trim_start_matches("tauri::");
    ["State<", "AppHandle", "Window", "Webview"]
        .iter()
        .any(|injected| ty.starts_with(injected))
}

/// What a row declares, in the same form.
fn declared_schema(row: &CommandSpec) -> Vec<(String, Presence)> {
    let mut schema: Vec<(String, Presence)> = row
        .args
        .iter()
        .map(|arg| (arg.name.to_owned(), arg.presence))
        .collect();
    schema.sort_by(|a, b| a.0.cmp(&b.0));
    schema
}

fn registered_names() -> Vec<&'static str> {
    REGISTERED
        .iter()
        .map(|path| path.rsplit("::").next().unwrap().trim())
        .collect()
}

/// Every registered command's row declares exactly the arguments its handler
/// takes, as its signature reads, with the presence its type gives. A row is
/// what the approval and price checks read, so a row that differs from its
/// handler would check arguments the webview cannot send and miss ones it
/// can (1.6.7's note, closed here).
#[test]
fn every_command_declares_the_arguments_its_handler_takes() {
    let source = shell_source();
    for name in registered_names() {
        let row = COMMANDS.iter().find(|row| row.name == name).unwrap();
        assert_eq!(
            handler_schema(&source, name),
            Some(declared_schema(row)),
            "{name}'s row and its handler disagree"
        );
    }

    // The reader leaves out what Tauri injects, reads `Option<_>` as
    // optional, and is not misled by a comma inside a type.
    let read = webview_arguments(
        "till: State<'_, Till>, app: tauri::AppHandle, mut line_id: SaleLineId, \n         \
         labels: Vec<(String, u8)>, qty_milli: Option<i64>) -> IpcResult<()> {",
    );
    assert_eq!(
        read,
        vec![
            ("labels".to_owned(), Presence::Required),
            ("line_id".to_owned(), Presence::Required),
            ("qty_milli".to_owned(), Presence::Optional),
        ]
    );
    // A row that leaves an argument out, or says it is required when a call
    // may leave it out, is not its handler's.
    const MISSING: &[Arg] = &[Arg::required("product_id")];
    const REQUIRED: &[Arg] = &[Arg::required("product_id"), Arg::required("qty_milli")];
    let add_line = *COMMANDS
        .iter()
        .find(|row| row.name == "cart_add_line")
        .unwrap();
    for args in [MISSING, REQUIRED] {
        let row = CommandSpec { args, ..add_line };
        assert_ne!(
            handler_schema(&source, "cart_add_line"),
            Some(declared_schema(&row))
        );
    }
    assert_eq!(handler_schema(&source, "cart_void_sale"), None);
}

/// No command takes a price but through a reviewed line whose control its row
/// keeps (`ref/ipc-contract.md` §1, rule 6, and §5, rule 6). What each
/// handler takes is read from its own signature, so an argument a row forgot
/// to declare is walked too. `cart_add_line`'s is a product and a quantity.
#[test]
fn no_command_argument_carries_a_price() {
    let source = shell_source();
    for name in registered_names() {
        for (argument, _) in handler_schema(&source, name).unwrap() {
            if carries_money(&argument) {
                assert!(
                    PRICE_ARGUMENTS
                        .iter()
                        .any(|allowed| allowed.command == name && allowed.argument == argument),
                    "{name} takes {argument}, which carries money"
                );
            }
        }
    }
    assert_eq!(
        handler_schema(&source, "cart_add_line"),
        Some(vec![
            ("product_id".to_owned(), Presence::Required),
            ("qty_milli".to_owned(), Presence::Optional),
        ]),
        "a line is a product and a quantity; its price is the catalogue's"
    );
    assert_eq!(check(COMMANDS, REGISTERED), Ok(()));

    // A price on a base sale command is refused, however it is spelt, and a
    // reviewed argument belongs to its own command only.
    let add_line = *COMMANDS
        .iter()
        .find(|row| row.name == "cart_add_line")
        .unwrap();
    const UNIT_PRICE: &[Arg] = &[
        Arg::required("product_id"),
        Arg::optional("unit_price_minor"),
    ];
    const PRICE: &[Arg] = &[Arg::required("product_id"), Arg::optional("price")];
    const AMOUNT: &[Arg] = &[Arg::required("product_id"), Arg::required("amount_minor")];
    for (args, argument) in [
        (UNIT_PRICE, "unit_price_minor"),
        (PRICE, "price"),
        (AMOUNT, "amount_minor"),
    ] {
        assert_eq!(
            refusals(&[CommandSpec { args, ..add_line }]),
            vec![RegistryError::PriceArgument {
                command: "cart_add_line",
                argument,
            }]
        );
    }

    // Each reviewed argument holds only while its row keeps its control.
    let uncontrolled = |command: &'static str, argument: &'static str| {
        RegistryError::PriceArgumentUncontrolled { command, argument }
    };
    let override_row = *COMMANDS
        .iter()
        .find(|row| row.name == "cart_override_price")
        .unwrap();
    assert_eq!(refusals(&[override_row]), Vec::new());
    // The price rule alone, on the row with its approval left out: it holds
    // while the row keeps its capability and its audit, and not without
    // either.
    const BASE_ARGS: &[Arg] = &[
        Arg::required("line_id"),
        Arg::required("unit_price_minor"),
        Arg::required("reason"),
    ];
    let unapproved = CommandSpec {
        approval_requirement: ApprovalRequirement::Never,
        args: BASE_ARGS,
        ..override_row
    };
    assert_eq!(refusals(&[unapproved]), Vec::new());
    for row in [
        CommandSpec {
            access: Access::Capability(cap::SaleCreate::NAME),
            ..unapproved
        },
        CommandSpec {
            audited: false,
            ..unapproved
        },
    ] {
        assert_eq!(
            refusals(&[row]),
            vec![uncontrolled("cart_override_price", "unit_price_minor")]
        );
    }
    assert!(
        refusals(&[CommandSpec {
            audited: false,
            ..CART_ADD_DEPARTMENT_SALE
        }])
        .contains(&uncontrolled("cart_add_department_sale", "amount_minor"))
    );
    const PREPARE_ARGS: &[Arg] = &[
        Arg::required("product_id"),
        Arg::required("barcode"),
        Arg::required("name_ar"),
        Arg::required("unit_price_minor"),
        Arg::required("tax_category_id"),
    ];
    let prepare = CommandSpec {
        name: "product_quick_add_prepare",
        access: Access::Capability(cap::ProductEdit::NAME),
        audited: false,
        approval_requirement: ApprovalRequirement::Never,
        args: PREPARE_ARGS,
    };
    assert_eq!(refusals(&[prepare]), Vec::new());
    assert_eq!(
        refusals(&[CommandSpec {
            access: Access::Capability(cap::SaleCreate::NAME),
            ..prepare
        }]),
        vec![uncontrolled(
            "product_quick_add_prepare",
            "unit_price_minor"
        )]
    );
    // Phase 0's smoke command keeps its total only while it touches no sale.
    assert_eq!(
        refusals(&[CommandSpec {
            access: Access::Capability(cap::SaleCreate::NAME),
            ..split_tender()
        }]),
        vec![uncontrolled("split_tender", "total_minor")]
    );
    // Cash handed over keeps its amount only while its row stays the selling
    // command's (1.8.3): another capability, or none, loses it.
    let cash = *COMMANDS
        .iter()
        .find(|row| row.name == "tender_add_cash")
        .unwrap();
    assert_eq!(refusals(&[cash]), Vec::new());
    for row in [
        CommandSpec {
            access: Access::Capability(cap::PriceOverride::NAME),
            ..cash
        },
        CommandSpec {
            access: Access::NoCapability {
                reason: "a tender needs no proof",
            },
            ..cash
        },
    ] {
        assert_eq!(
            refusals(&[row]),
            vec![uncontrolled("tender_add_cash", "amount_minor")]
        );
    }
    // And the line is that command's alone: no other tender command takes an
    // amount.
    let remove = *COMMANDS
        .iter()
        .find(|row| row.name == "tender_remove")
        .unwrap();
    const REMOVE_WITH_AMOUNT: &[Arg] = &[Arg::required("tender_id"), Arg::required("amount_minor")];
    assert_eq!(
        refusals(&[CommandSpec {
            args: REMOVE_WITH_AMOUNT,
            ..remove
        }]),
        vec![RegistryError::PriceArgument {
            command: "tender_remove",
            argument: "amount_minor",
        }]
    );
}

// ── Each condition at its boundary ──────────────────────────────────────────

/// The boundary check for each condition. A new `ApprovalPredicate` does not
/// compile here until it has one.
fn holds_at_its_boundary(predicate: ApprovalPredicate) {
    match predicate {
        ApprovalPredicate::DepartmentAmountAboveEscalation => department_threshold(),
        ApprovalPredicate::PriceOverrideEscalatedByPolicy => override_escalation(),
    }
}

const CASHIER: UserId = UserId::from_uuid(Uuid::from_u128(0xc0));
const MANAGER: UserId = UserId::from_uuid(Uuid::from_u128(0xa0));
const SALE: SaleId = SaleId::from_uuid(Uuid::from_u128(0x5a1e));
const OTHER_SALE: SaleId = SaleId::from_uuid(Uuid::from_u128(0x5a1f));

fn at() -> Timestamp {
    Timestamp::from_epoch_milliseconds(1_790_000_000_000).unwrap()
}

fn jod(minor: i64) -> Money {
    Money::from_minor(minor, Currency::JOD)
}

/// `who`'s proof of `C` under the shipped grid for `role`, where the
/// merchant's policy escalates nothing.
fn proof<C: Capability>(who: UserId, role: Role) -> Authorized<C> {
    let unbound = ApprovalBinding {
        entity_id: Uuid::nil(),
        amount_minor: 0,
        content_hash: None,
    };
    authorize(
        who,
        &GrantSet::of_role(role),
        None,
        &unbound,
        &EscalationPolicy::empty(),
        at(),
    )
    .unwrap()
}

/// A manager's approval of the cashier's department sale on `sale` for
/// `amount_minor`, read back through `ApprovalHandle::restore`, the seam
/// `ApprovalRepository::load_for_consumption` reads every stored row through.
fn approval(sale: SaleId, amount_minor: i64) -> ApprovalHandle {
    let id = ApprovalId::from_uuid(Uuid::from_u128(0xa991));
    let issued = ApprovalHandle::issue(
        id,
        CASHIER,
        &proof::<cap::DepartmentSale>(MANAGER, Role::Manager),
        &ApprovalBinding {
            entity_id: sale.as_uuid(),
            amount_minor,
            content_hash: None,
        },
        "unknown_barcode".to_owned(),
        at(),
        120_000,
        *id.as_uuid().as_bytes(),
    )
    .unwrap();
    ApprovalHandle::restore(issued.to_stored()).unwrap()
}

/// `DepartmentPolicy::escalate_above`, through the transition
/// `cart_add_department_sale` will call (1.4.12).
fn department_threshold() {
    let policy = DepartmentPolicy {
        max_line_amount: jod(10_000),
        escalate_above: Some(jod(5_000)),
    };
    let threshold = 5_000;
    let till = CartContext {
        register_id: RegisterId::from_uuid(Uuid::from_u128(0x7e9)),
        shift_id: ShiftId::from_uuid(Uuid::from_u128(0x5f7)),
        cashier_id: CASHIER,
        currency: Currency::JOD,
        is_training: false,
        opened_at: at(),
    };
    let department = Department {
        id: CategoryId::from_uuid(Uuid::from_u128(0xde9)),
        name_ar: "قسم اصطناعي".to_owned(),
        name_en: Some("SYNTH department".to_owned()),
        tax_category_id: TaxCategoryId::from_uuid(Uuid::from_u128(0x7a8)),
        is_active: true,
    };
    let sell = |minor: i64, approval: Option<&ApprovalHandle>| {
        let line = AddLine::department(
            SaleLineId::from_uuid(Uuid::from_u128(0x11e)),
            &department,
            jod(minor),
            None,
        )
        .unwrap();
        add_department_line(
            open(SALE, &till),
            line,
            &policy,
            &proof::<cap::DepartmentSale>(CASHIER, Role::Cashier),
            approval,
        )
    };

    // At the threshold the cashier proceeds alone, and an approval offered
    // anyway is not consumed.
    let (_, alone) = sell(threshold, None).unwrap();
    assert_eq!((alone.approver, alone.approval), (None, None));
    let (_, offered) = sell(threshold, Some(&approval(SALE, threshold))).unwrap();
    assert_eq!((offered.approver, offered.approval), (None, None));

    // One fil past it, the line is refused without a handle...
    assert_eq!(
        sell(threshold + 1, None).unwrap_err(),
        CartError::DepartmentNeedsApproval("5.001".to_owned(), "5.000".to_owned())
    );
    // ...and passes with one bound to this sale and this amount, which the
    // audit intent then names.
    let bound = approval(SALE, threshold + 1);
    let (_, crossed) = sell(threshold + 1, Some(&bound)).unwrap();
    assert_eq!(
        (
            crossed.approver,
            crossed.approval,
            crossed.reason.as_deref()
        ),
        (Some(MANAGER), Some(bound.id()), Some("unknown_barcode"))
    );
    // A handle for another amount, or for another sale, does not cross it.
    assert!(matches!(
        sell(threshold + 1, Some(&approval(SALE, threshold))),
        Err(CartError::Permission(
            PermissionError::ApprovalAmountMismatch(..)
        ))
    ));
    assert!(matches!(
        sell(threshold + 1, Some(&approval(OTHER_SALE, threshold + 1))),
        Err(CartError::Permission(
            PermissionError::ApprovalEntityMismatch(..)
        ))
    ));
    // And no approval carries a line past the cap.
    assert!(matches!(
        sell(10_001, Some(&approval(SALE, 10_001))),
        Err(CartError::DepartmentAmountAboveCap(..))
    ));
}

const SHIFT_LEAD: UserId = UserId::from_uuid(Uuid::from_u128(0x5e));
const LINE: SaleLineId = SaleLineId::from_uuid(Uuid::from_u128(0x11f));
const OTHER_LINE: SaleLineId = SaleLineId::from_uuid(Uuid::from_u128(0x120));

/// A manager's approval of `actor`'s override of `line` to `amount_minor`,
/// read back through `ApprovalHandle::restore`.
fn override_approval(actor: UserId, line: SaleLineId, amount_minor: i64) -> ApprovalHandle {
    let id = ApprovalId::from_uuid(Uuid::from_u128(0xa992));
    let issued = ApprovalHandle::issue(
        id,
        actor,
        &proof::<cap::PriceOverride>(MANAGER, Role::Manager),
        &ApprovalBinding {
            entity_id: line.as_uuid(),
            amount_minor,
            content_hash: None,
        },
        "damaged".to_owned(),
        at(),
        120_000,
        *id.as_uuid().as_bytes(),
    )
    .unwrap();
    ApprovalHandle::restore(issued.to_stored()).unwrap()
}

/// `price.override` under the merchant's escalation policy, through the
/// proof `cart_override_price` takes and the transition it calls. Its
/// binding is the command's: this line and this price.
fn override_escalation() {
    let till = CartContext {
        register_id: RegisterId::from_uuid(Uuid::from_u128(0x7e9)),
        shift_id: ShiftId::from_uuid(Uuid::from_u128(0x5f7)),
        cashier_id: SHIFT_LEAD,
        currency: Currency::JOD,
        is_training: false,
        opened_at: at(),
    };
    let oil = Product {
        id: ProductId::from_uuid(Uuid::from_u128(0x011)),
        sku: "SYN-OIL-1L".to_owned(),
        name_ar: "زيت اصطناعي".to_owned(),
        name_en: None,
        category_id: None,
        tax_category_id: TaxCategoryId::from_uuid(Uuid::from_u128(0x7a8)),
        unit: UnitOfMeasure::Each,
        unit_price: jod(1_250),
        is_weighed: false,
        is_service: false,
        is_active: true,
        min_age: None,
        max_price: None,
        reorder_point: None,
        regulated_sale_form: RegulatedSaleForm::UNREGULATED_SEALED_PACK,
    };
    let price = 1_000;
    let over = |actor: UserId,
                role: Role,
                policy: &EscalationPolicy,
                approval: Option<&ApprovalHandle>|
     -> Result<AuditIntent, PermissionError> {
        let auth = authorize::<cap::PriceOverride>(
            actor,
            &GrantSet::of_role(role),
            approval,
            &ApprovalBinding {
                entity_id: LINE.as_uuid(),
                amount_minor: price,
                content_hash: None,
            },
            policy,
            at(),
        )?;
        let line = AddLine::from_product(LINE, &oil, Qty::ONE).unwrap();
        let cart = add_line(open(SALE, &till), line).unwrap();
        let reason = OverrideReason::from("damaged".to_owned());
        let (_, intent, _) =
            override_price(cart, LINE, jod(price), reason, Some(jod(100)), &auth).unwrap();
        Ok(intent)
    };
    let alone = EscalationPolicy::empty();
    let escalating = EscalationPolicy::empty().with_escalation::<cap::PriceOverride>();

    // Where the policy names nothing, a shift lead overrides alone, and an
    // approval offered anyway is not consumed.
    let intent = over(SHIFT_LEAD, Role::ShiftLead, &alone, None).unwrap();
    assert_eq!((intent.approver, intent.approval), (None, None));
    let bound = override_approval(SHIFT_LEAD, LINE, price);
    let offered = over(SHIFT_LEAD, Role::ShiftLead, &alone, Some(&bound)).unwrap();
    assert_eq!((offered.approver, offered.approval), (None, None));

    // Where it names `price.override`, the override is refused without a
    // handle...
    assert_eq!(
        over(SHIFT_LEAD, Role::ShiftLead, &escalating, None).unwrap_err(),
        PermissionError::EscalationRequired(cap::PriceOverride::NAME)
    );
    // ...and passes with one bound to this line and this price, which the
    // audit intent then names, with the amount the schema compares.
    let crossed = over(SHIFT_LEAD, Role::ShiftLead, &escalating, Some(&bound)).unwrap();
    assert_eq!(
        (crossed.approver, crossed.approval, crossed.entity_id),
        (Some(MANAGER), Some(bound.id()), LINE.as_uuid())
    );
    assert_eq!(crossed.payload.get("amount_minor"), Some(&price.into()));
    // A handle for another price, or another line, does not.
    assert!(matches!(
        over(
            SHIFT_LEAD,
            Role::ShiftLead,
            &escalating,
            Some(&override_approval(SHIFT_LEAD, LINE, price - 1))
        ),
        Err(PermissionError::ApprovalAmountMismatch(..))
    ));
    assert!(matches!(
        over(
            SHIFT_LEAD,
            Role::ShiftLead,
            &escalating,
            Some(&override_approval(SHIFT_LEAD, OTHER_LINE, price))
        ),
        Err(PermissionError::ApprovalEntityMismatch(..))
    ));
    // An approval adds a second person, never the capability: a cashier is
    // refused under either policy, with a handle made out to them.
    let cashiers = override_approval(CASHIER, LINE, price);
    for policy in [&alone, &escalating] {
        assert!(matches!(
            over(CASHIER, Role::Cashier, policy, Some(&cashiers)),
            Err(PermissionError::Denied(..))
        ));
    }
}
