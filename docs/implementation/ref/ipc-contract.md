# IPC contract — every Tauri command

The narrow, typed boundary between the React UI and the Rust core. It is the **only** channel: no `fs`, no `shell`, no `http`, no `updater` plugin is exposed to the webview. The core talks to the world; the webview talks to the core.

**The registry is the security surface.** Every command declares its required capability, whether it audits, and — when it escalates — what an approval is bound to. Microstep 1.6.7 owns the test `ipc_commands_all_declare_a_capability` and its CI wiring; the current tree does not yet enforce this future registry. Hiding a button is UX; the check in the handler is security.

---

## 1 · Shape

```rust
// apps/terminal/src-tauri/src/ipc/mod.rs
#[derive(Serialize, ts_rs::TS)]
pub struct IpcError {
    pub code: &'static str,             // what the UI branches on
    pub message_key: String,            // what the UI translates
    /// A STATIC diagnostic from a fixed set — never a formatted source error.
    pub detail: Option<&'static str>,
    /// Correlates this failure with the scrubbed log sink. Support asks for it;
    /// it carries no content of its own.
    pub trace_id: Uuid,
}

pub type IpcResult<T> = Result<T, IpcError>;
```

`detail` used to be `Option<String>`, and a free-form string here defeats the scrubber. The `tracing`
layer redacts fields by canonical name inside the log pipeline; it cannot touch a value that was
serialised straight to the webview, and it does not recognise generic keys like `error`, `body` or
`detail` in the first place. A SQL error carrying a bound parameter, a PSP failure body, a fiscal
response, or a panic message reaches the screen, the screenshot the cashier sends, and the JavaScript
crash reporter — with every named PII test still green, because they test the log sink. Typing
`detail` as `&'static str` makes the leak a compile error rather than a review item. The source error
goes to the scrubbed sink under `trace_id`.

Rules:

1. `snake_case`, verb-first, noun-scoped: `cart_add_line`, not `addLineToCart`.
2. Returns `IpcResult<T>` where `T` derives `Serialize` **and `ts_rs::TS`**.
3. **TypeScript types are generated, never hand-written.** `ts-rs` emits into `packages/api-types/`; the owning Phase-1 frontend microsteps must add the CI drift check before the contract ships. The current tree has no generator lane. Two hand-maintained copies of a money type is how a rounding bug ships.
4. Long operations return immediately with a handle and emit **events** for progress (§4). A cashier watching a stateless spinner presses the button again.
5. Every command that reverses money or opens the drawer takes an `Authorized<C>` constructed inside the handler and writes the `AuditIntent` the domain returned, in the same transaction as the effect.
6. **No base sale command accepts an uncontrolled price.** Price-bearing command arguments exist on
   exactly three controlled entries: `cart_override_price` requires `price.override`, a reason,
   audit and escalation; `cart_add_department_sale` is separately capability-gated, capped, audited
   and reported ([`domain-api.md`](domain-api.md) §6.5); and `product_quick_add_prepare` writes only a
   content-hashed proposal that creates neither a product nor a cart line. Catalogue and
   price-embedded-label amounts are resolved inside Rust through `PriceSource`; neither arrives as a
   base `sale.create` argument.
7. **A privileged command takes an `approval_id` and consumes it once.** The handler resolves it to
   an immutable `ApprovalHandle`, checks it against the operation about to happen, and inserts
   `approval_consumption` in the same transaction as the effect and audit row. The handle is never
   deleted or updated because it is audit evidence (`domain-api.md` §8.1). A capability-only approval is
   a reusable bearer proof: one manager PIN, typed once and watched over a shoulder, authorises that
   whole class of operation for the rest of the day, with every one of them attributed to the manager.
8. **The webview reaches no Tauri plugin the core has not wrapped.** A command in this catalog is the
   only path; the capability file grants no plugin permission that would bypass one. `updater:default`
   in particular exposes check, download, install and download-and-install straight to JavaScript,
   which would route around the Rust check that refuses an update while a shift is open.

---

## 2 · The registry

```rust
// apps/terminal/src-tauri/src/ipc/registry.rs
pub struct CommandSpec {
    pub name: &'static str,
    /// `None` is deliberate: a lock-screen command, a first-run recovery-code
    /// provisioning command, or `recovery_restore_backup`, which runs before
    /// the database that holds the grants can be opened.
    pub capability: Option<&'static str>,
    pub audited: bool,
    pub approval_requirement: ApprovalRequirement,
}

pub enum ApprovalRequirement {
    Never,
    Always { binding: ApprovalBindingSpec },
    Conditional { predicate: &'static str, binding: ApprovalBindingSpec },
}

pub struct ApprovalBindingSpec {
    pub entity_field: &'static str,          // "sale_id", "line_id", "preview_id", "shift_id"
    pub amount: AmountBindingSpec,
    pub content: ContentBindingSpec,
    pub reason: ReasonBindingSpec,
}

pub enum AmountBindingSpec {
    Argument(&'static str),                  // e.g. "unit_price_minor"
    PreparedEffect(&'static str),            // immutable request/preview field
    Exact(i64),                              // non-money effects use Exact(0), never wildcard
}

pub enum ReasonBindingSpec {
    Argument(&'static str),
    PreparedEffect(&'static str),
}

pub enum ContentBindingSpec {
    None,
    PreparedIntent { table: &'static str, hash_field: &'static str },
}

pub const COMMANDS: &[CommandSpec] = &[ /* every row of §3 */ ];
```

Three tests walk this table, all in `apps/terminal/src-tauri/tests/ipc_contract.rs` [1.6.7]:
`ipc_commands_all_declare_a_capability`, `every_privileged_command_binds_its_approval`, and
`conditional_privilege_cannot_cross_threshold_without_approval`. An `Always` or `Conditional`
entry without an exact entity, amount and reason binding is the reusable bearer proof rule 7 exists
to prevent; a boolean cannot express the department-sale threshold at all.

---

## 3 · The catalog

Legend — **Ph**: phase introduced · **Cap**: required capability · **A**: writes an audit entry · **E**: privileged or manager-escalatable. An always-privileged command takes `approval_id`; a conditional command takes `approval_id?` and refuses the privileged branch without it.

### Session and users

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `session_state` | `()` → `SessionState` | 1 | — | | |
| `auth_login_pin` | `{ user_code, pin }` → `Session` | 1 | — | ✓ | |
| `auth_logout` | `()` → `()` | 1 | — | ✓ | |
| `auth_switch_user` | `{ user_code, pin }` → `Session` | 1 | — | ✓ | |
| `auth_verify_pin` | `{ user_code, pin, for_capability, entity_id, amount_minor, reason }` → `ApprovalRef` | 1 | — | ✓ | |
| `user_reset_pin` | `{ user_id, new_pin, approval_id }` → `()` | 1 | `user.admin` | ✓ | ✓ |

`auth_verify_pin` is the manager-approval modal's backing call, and its arguments identify the
binding. For a prepared intent, the Rust handler loads the row and recomputes its canonical
`content_hash`; JavaScript cannot submit or replace that digest.
The approving manager is shown, and approves, **this sale for this amount** — so the handler that
later spends the approval can refuse a different sale, a different amount, a different capability, a
different cashier, or a second use ([`domain-api.md`](domain-api.md) §8.1).

It returns `ApprovalRef { approval_id, capability, expires_at }`, not the handle. The handle's fields
stay in Rust and in the `approval_handle` row; handing the webview a self-contained proof would make it a
bearer token in JavaScript, which is the thing being prevented. The UI passes `approval_id` back.

### Catalog and search

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `catalog_search` | `{ query, limit }` → `Vec<ProductHit>` | 1 | `sale.create` | | |
| `catalog_by_barcode` | `{ code }` → `ScanLookup` | 1 | `sale.create` | | |
| `catalog_by_plu` | `{ code }` → `Option<Product>` | 1 | `sale.create` | | |
| `catalog_tiles` | `{ grid_id? }` → `Vec<Tile>` | 1 | `sale.create` | | |
| `department_list` | `()` → `Vec<Department>` | 1 | `sale.create` | | |
| `product_quick_add_prepare` | `{ product_id, barcode, name_ar, unit_price_minor, tax_category_id }` → `ProductQuickAddRequest` | 1 | `product.edit` | | |
| `product_quick_add` | `{ product_id, approval_id }` → `Product` | 1 | `product.edit` | ✓ | ✓ |
| `price_check` | `{ code }` → `PriceCheckResult` | 4 | — | | |

`catalog_by_barcode` is the shell boundary around one domain flow: parse raw scanner bytes into an
opaque `ParsedScan`, look up `ParsedScan::item_code()` as `Vec<ProductHit>`, then call
`resolve_scan(parsed, hits, currency)`. It returns `ScanLookup`, not `Product`: a scan may resolve to
a plain product, a price-embedded item carrying a weight or price, an ambiguous collision (E.36), a
checksum failure (E.40), or nothing at all (E.39). The UI branches on the variant.

`ScanLookup::PriceEmbedded` carries a `PriceSource` that neither the webview nor dependent Rust code
can construct from an integer
([`domain-api.md`](domain-api.md) §4.2–4.3). That is what lets a deli label's price reach a line
while no command argument anywhere accepts a price for an ordinary one.

`product_quick_add` is `Always` with entity `product_id`, amount from the prepared
`unit_price_minor`, reason from the prepared approval prompt and content from
`PreparedIntent { table: "product_quick_add_request", hash_field: "content_hash" }`. The prepare
command is inert: it writes a request and its digest, not a product or line. Issue and commit each
recompute the hash from all request fields, and the database refuses an update after any matching
approval exists, because approving a product id does not approve later substitutions under that id.

**`catalog_search`'s `query` never reaches FTS5 as an expression.** `MATCH` parses its right-hand
side as a query expression whatever it was bound as, so `"`, `(`, `)`, `:`, `*` and a bareword `OR`
or `NEAR` are operators, not letters. A cashier typing one gets a syntax error surfaced as a
`DbError`, or silently a different query — on the only path they have when the scanner fails. The
handler tokenises the input on the same boundaries the index uses, quotes each token with internal
quotes doubled, joins with `AND`, and appends `*` to the last token for prefix search. Tests:
`search_survives_every_fts5_metacharacter` · `prop_no_query_string_produces_a_database_error`.

### Cart

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `cart_open` | `()` → `CartSnapshot` | 1 | `sale.create` | | |
| `cart_current` | `()` → `Option<CartSnapshot>` | 1 | `sale.create` | | |
| `cart_add_line` | `{ product_id, qty_milli? }` → `CartSnapshot` | 1 | `sale.create` | | |
| `cart_add_scan` | `{ raw_code }` → `CartSnapshot` | 1 | `sale.create` | | |
| `cart_add_department_sale` | `{ department_id, amount_minor, scanned_code?, note?, approval_id? }` → `CartSnapshot` | 1 | `sale.department` | ✓ | ✓ |
| `cart_set_qty` | `{ line_id, qty_milli }` → `CartSnapshot` | 1 | `sale.create` | | |
| `cart_void_line` | `{ line_id, reason }` → `CartSnapshot` | 1 | `line.void` | ✓ | |
| `cart_discount_line` | `{ line_id, kind, value, reason, approval_id? }` → `CartSnapshot` | 1 | `discount.manual` | ✓ | ✓ |
| `cart_discount_basket` | `{ kind, value, reason, approval_id? }` → `CartSnapshot` | 1 | `discount.manual` | ✓ | ✓ |
| `cart_override_price` | `{ line_id, unit_price_minor, reason, approval_id? }` → `CartSnapshot` | 1 | `price.override` | ✓ | ✓ |
| `cart_confirm_age` | `{ line_id, confirmed }` → `CartSnapshot` | 1 | `sale.create` | ✓ | |
| `cart_attach_customer` | `{ customer_id }` → `CartSnapshot` | 3 | `sale.create` | | |
| `cart_set_buyer_tin` | `{ tin, name? }` → `CartSnapshot` | 2 | `sale.create` | | |
| `cart_clear_buyer_tin` | `()` → `CartSnapshot` | 2 | `sale.create` | | |
| `cart_park` | `{ label? }` → `ParkedRef` | 1 | `sale.park` | | |
| `cart_resume` | `{ parked_id }` → `CartSnapshot` | 1 | `sale.resume` | | |
| `cart_list_parked` | `()` → `Vec<ParkedRef>` | 1 | `sale.resume` | | |
| `cart_void_sale` | `{ reason, approval_id? }` → `()` | 1 | `sale.void` | ✓ | ✓ |
| `cart_set_training` | `{ on }` → `CartSnapshot` | 1 | `training_mode.toggle` | ✓ | |

`CartSnapshot` is the **priced** cart — lines, discount attributions, tax summary, totals — so the UI never computes money. It re-renders what Rust decided.

**`cart_add_line` no longer takes `unit_price_minor?`.** It took one, under the base `sale.create`
capability, with the audit and escalation columns blank — six rows above `cart_override_price`, which
requires `price.override`, writes an audit row, escalates to a manager, and respects a margin floor
and the ministry ceiling. Two commands could set a line's price and only one of them was controlled,
so a cashier could add the line at 0.100 instead of 1.000 with no audit row, no reason code, no entry
in the override report, and no label-reprint worklist row. That is the threat the plan's own model
ranks first, executable without a trace, and it also made
`override_below_floor_is_denied` and `override_above_max_price_is_hard_blocked` (E.71 — where "the
fine is real") optional. Price-embedded barcodes, which is what the optional field was for, arrive
through `cart_add_scan` as a typed `ScanLookup::PriceEmbedded`.

**`cart_confirm_age` moves to Phase 1.** Phase 1 builds the refusal (`add_line` rejects a `min_age`
product without confirmation, E.69) and Phase 2 built the confirmation, so every age-restricted
product was unsellable for a whole phase — including the tobacco line in Phase 1's own seed fixture,
against a phase whose stated capability is that a real Jordanian minimarket could sell all day. The
refusal is the valuable half and it is useless without a way to say yes.

**There is no command that sets a non-domestic supply context.** `SupplyTaxContext`
([`domain-api.md`](domain-api.md) §5) exists so the tax engine can represent an export or a
free-zone supply, and in v1 it is always `Domestic`: exports and free-zone supplies are hard-blocked
at the register rather than approximated. Adding the command without the reason codes, the evidence
capture and the return-box mapping would let a cashier zero-rate a sale that is not zero-rated.

### Tender and finalize

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `tender_begin` | `()` → `TenderState` | 1 | `sale.create` | | |
| `tender_back_to_cart` | `()` → `CartSnapshot` | 1 | `sale.create` | | |
| `tender_add_cash` | `{ amount_minor }` → `TenderState` | 1 | `sale.create` | | |
| `tender_start_card` | `{ amount_minor }` → `CardOpHandle` | 2 | `sale.create` | | |
| `tender_cancel_card` | `{ handle }` → `TenderState` | 2 | `sale.create` | ✓ | |
| `tender_remove` | `{ tender_id }` → `TenderState` | 1 | `sale.create` | ✓ | |
| `sale_finalize` | `()` → `CompletedSaleRef` | 1 | `sale.create` | ✓ | |
| `sale_reprint` | `{ sale_id }` → `()` | 1 | `sale.reprint` | ✓ | |
| `sale_in_flight_state` | `()` → `Option<InFlightSummary>` | 1 | `sale.create` | | |

`tender_start_card` returns a handle and drives the flow through events (§4). The `Unknown`/timeout protocol — status-query before any retry (Phase 2, 2.1.3) — lives entirely on the Rust side. The UI **cannot** issue a second authorisation because no command exists that would let it.

`sale_reprint` moved off `sale.create` onto `sale.reprint`. On `sale.create` any cashier could reprint
any document they could name, which is a customer-data and evidence question rather than a selling
one; the default grant is still the cashier ([`domain-api.md`](domain-api.md) §8.2), so nothing a shop
does today stops working, but the capability now exists to withhold.

`sale_in_flight_state` is what the recovery path reads after a restart. It reports the durable
`InFlightSale` row (`domain-api.md` §6.6) — including whether a card operation was outstanding, which
is the input E.2's status-query needs and which nothing previously persisted.

### Returns

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `return_find_sale` | `{ receipt_no? , barcode?, card_last4?, customer_id? }` → `Vec<SaleRef>` | 2 | `refund.receipted` | | |
| `return_refundable` | `{ sale_id }` → `Vec<RefundableLine>` | 2 | `refund.receipted` | | |
| `return_build` | `{ sale_id, lines[], restock[], reason, requalify }` → `RefundPreview` | 2 | `refund.receipted` | | |
| `return_commit` | `{ preview_id, tenders[], approval_id? }` → `CompletedSaleRef` | 2 | `refund.receipted` | ✓ | ✓ |
| `return_receiptless` | `{ lines[], approval_id? }` → `RefundPreview` | 2 | `refund.receiptless` | ✓ | ✓ |
| `exchange_commit` | `{ preview_id, cart_id, approval_id? }` → `ExchangePair` | 2 | `refund.receipted` | ✓ | ✓ |

`return_build` produces a **preview** with an id; `return_commit` references it. The refundable-quantity check runs at both points, so a second cashier cannot slip a refund between preview and commit.

`reason` and `requalify` are arguments to the **preview**, not the commit, because both change the
amount the approving manager is shown: a `Defective` claim may bypass the window, and `DealBreak`
requalification reprices what the customer keeps ([`domain-api.md`](domain-api.md) §10). The
approval binds to the preview's total, so changing either after approval invalidates the handle —
which is the point.

`return_build` also refuses a `ChangeOfMind` claim outside `window_days` and returns
`refund.outside_window` as the capability to escalate to. Without that path, a customer returning a
faulty item on day 20 was refused by the domain with no override available to anyone, owner included.

`exchange_commit` writes both documents and their `document_link` in one transaction, settling the
offset through the internal `exchange` tender (`domain-api.md` §7.1). The pair is atomic: a refund
document that exists without its replacement sale is a customer standing at the counter with neither
their goods nor their money.

### Shift and cash

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `shift_open` | `{ float_by_denomination, business_date? }` → `Shift` | 1 | `shift.open` | ✓ | |
| `shift_current` | `()` → `Option<Shift>` | 1 | — | | |
| `shift_close` | `{ shift_id }` → `ShiftCloseEvent` | 1 | `shift.close` | ✓ | |
| `shift_close_begin` | `()` → `CloseSession` | 2 | `shift.close` | | |
| `shift_close_submit_count` | `{ session_id, count_by_denomination }` → `CloseResult` | 2 | `shift.close` | ✓ | |
| `shift_force_close_stale` | `{ shift_id, reason, approval_id }` → `CloseResult` | 2 | `shift.close` | ✓ | ✓ |
| `cash_location_list` | `()` → `Vec<CashLocation>` | 2 | `cash.movement` | | |
| `cash_movement` | `{ kind, amount_minor, from_location_id?, to_location_id?, reason, note?, approval_id? }` → `()` | 2 | `cash.movement` | ✓ | ✓ |
| `drawer_open_no_sale` | `{ reason, approval_id? }` → `()` | 2 | `drawer.open` | ✓ | ✓ |
| `report_x` | `()` → `XReport` | 2 | `xreport.run` | | |
| `report_z` | `()` → `ZReport` | 2 | `zreport.run` | ✓ | |

`shift_close` is ordinary only when the authenticated actor opened `shift_id`; the repository
refuses any other shift. Closing your own work is not an escalation and therefore takes no
`ApprovalHandle`. `shift_force_close_stale` is the only cross-user close path. It is `Always`, binds
the stale `shift_id`, exact zero and the supplied reason, and preserves the unconditional
actor-versus-approver separation that applies to every handle path.

**`shift_open` and `shift_current` move to Phase 1.** `Cart` carries a non-optional `shift_id`,
`sale.business_date` is `NOT NULL`, and conventions §11 *defines* the business date as the business
date of its shift — so with no shift in Phase 1 the phase's core write path had no defined way to
obtain either value, and whoever implemented it would have invented one. The two plausible inventions
disagree at the 04:00 cutover, which mis-buckets every report and every Z boundary afterwards, on
rows that are immutable. Phase 1 gets open, close, an opening float, and one-open-per-register; the
blind count, over/short, drawer movements and X/Z stay in Phase 2 as planned.

`business_date?` is the operator confirmation a `Suspect` or `Untrusted` clock requires
([`domain-api.md`](domain-api.md) §3.2). Supplied under any other confidence, it is refused rather
than accepted — an override that works when it is not needed is an override that gets used.

`cash_movement` carries `from_location_id` / `to_location_id` because a movement is a transfer
between two places, not a signed number against one drawer. A drop reduces the drawer and increases
the safe; a bank deposit leaves the safe and touches no drawer at all, and expressing it as a
drawer-scoped movement created a phantom shortage in the shift it was recorded against.

> **Blind close is enforced here, not in the UI.** `shift_close_begin` returns a `CloseSession` **without** the expected figure. `shift_close_submit_count` returns the comparison. The expected amount is never on the wire before the count is submitted — asserted by test `expected_is_not_sent_to_the_ui_before_the_count_is_submitted`.
>
> **And `report_x` is part of that guarantee, not an exception to it.** Totals by tender plus the
> opening float *is* the expected figure, and `report_x` ran on `zreport.run` — held by shift lead and
> manager, the same two roles that close shifts. In a small store the shift lead is the person
> counting their own drawer, so the wire-level guarantee was airtight against a cashier and open to
> exactly the people it was written about. `report_x` now requires its own `xreport.run`, **and omits
> the cash-tender total and the expected figure entirely** for a caller who holds `shift.close` on the
> currently open shift. Asserted by `x_report_does_not_reveal_expected_cash_to_the_closing_user` (E.84).

### Reports

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `report_tax_by_rate` | `{ from_date, to_date }` → `TaxByRateReport` | 1 | `reports.all` | | |
| `report_day_so_far` | `()` → `DaySoFar` | 2 | `reports.own` | | |

**`report_tax_by_rate` is the deliverable the whole tax engine exists for**, and it had no command,
no screen, and no microstep in any phase — while Phase 1's exit gate says "run the tax report for the
day and check it against the receipts by hand". The nearest thing that existed was
`tax_report_matches_hand_check_fixture`, a Rust test over a committed fixture, which an operator
cannot run for an arbitrary day. Without this, a store trading on Phases 1–3 reconstructs its
bi-monthly return by hand from receipts.

It returns rows by `(component, treatment, rate, per_unit, reason)`, bucketed by `business_date`,
with refunds as **negatives in the same rows** — an accountant files a net figure — and training
sales excluded **with a visible count**, so the exclusion is stated rather than assumed
([`tax-jordan.md`](tax-jordan.md) §6).

> ⚠️ **OPEN — blocks 4.7.2.** Is a sales-side report by rate sufficient as the merchant's filing
> input, or does the return also require purchase, import and input-tax figures this product does not
> hold? Default until answered: `report_tax_by_rate` is named and described as a **sales-side tax
> reconciliation**, not as a completed return, in the UI, the export header and the owner guide.
> Owner: 4.7.2. Source that settles it: the official ISTD declaration form and its filing manual, as
> read by the merchant's accountant.

### Journal, health, diagnostics

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `journal_search` | `{ filters }` → `Vec<JournalEntry>` | 2 | `journal.view` | | |
| `journal_detail` | `{ sale_id }` → `JournalDetail` | 2 | `journal.view` | | |
| `health_status` | `()` → `HealthSnapshot` | 1 | — | | |
| `diag_test_print` | `()` → `()` | 1 | `settings.edit` | ✓ | |
| `diag_scanner_echo` | `{ on }` → `()` | 1 | `settings.edit` | | |
| `diag_terminal_ping` | `()` → `TerminalInfo` | 2 | `settings.edit` | | |
| `diag_fiscal_state` | `()` → `FiscalHealth` | 2 | `settings.edit` | | |
| `fiscal_rebuild_failed` | `{ queue_id, reason, approval_id }` → `FiscalHealth` | 2 | `fiscal.remediate` | ✓ | ✓ |
| `diag_backup_state` | `()` → `BackupInfo` | 1 | `settings.edit` | | |
| `diag_verify_audit_chain` | `()` → `ChainVerdict` | 1 | `settings.edit` | ✓ | |

`health_status` is unauthenticated because the lock screen shows it — sync state, offline queue depth, uncleared fiscal count, backup age, clock confidence. A cashier who cannot sign in still needs to know whether the register is healthy.

Diagnostics has no drawer-kick bypass. From Phase 2 its drawer action invokes the same
`drawer_open_no_sale { reason, approval_id? }` command as the shift screen, producing the same
`drawer_event`, audit row and conditional approval. `fiscal_rebuild_failed` is `Always` with entity
`queue_id`, amount `Exact(0)`, content `None` and reason `Argument("reason")`; it rebuilds only after
the builder or pinned configuration is corrected and preserves the fiscal identity.

`journal_search` and `journal_detail` moved off `reports.all` onto `journal.view`, which the cashier
holds. The journal's own acceptance criterion is that *"a customer is at the counter with a receipt
from Tuesday"* takes under ten seconds; behind `reports.all` — manager and owner only — it took
however long finding a manager takes. `journal.view` is scoped to the holder's **own shift** unless
they also hold `reports.all`, which is the answer to "who may see another cashier's sales"
([`domain-api.md`](domain-api.md) §8.2).

### Stock

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `stock_on_hand` | `{ product_id }` → `StockPosition` | 1 | `sale.create` | | |
| `stock_adjust_prepare` | `{ product_id, qty_delta_milli, reason_code, note? }` → `StockAdjustmentRequest` | 1 | `stock.adjust` | | |
| `stock_adjust` | `{ stock_event_id, approval_id }` → `StockPosition` | 1 | `stock.adjust` | ✓ | ✓ |
| `stock_count_begin` | `{ scope }` → `CountSession` | 4 | `stock.adjust` | ✓ | |
| `stock_count_record` | `{ session_id, product_id, counted_milli }` → `()` | 4 | `stock.adjust` | | |
| `stock_count_preview` | `{ session_id }` → `CountVariance` | 4 | `stock.adjust` | | |
| `stock_count_post` | `{ session_id, approval_id? }` → `()` | 4 | `stock.adjust` | ✓ | ✓ |

Phase 4 defines a stock-count screen and the exhaustive command catalog had no way to reach it, so
the demonstration was unbuildable through the architecture the plan requires. `stock_adjust` lands in
Phase 1 alongside the ledger for a different reason: every path that *increases* stock arrives in
Phase 4, so from the first sale every product goes negative and stays negative, and the negative-stock
flag the plan calls "loud" is a hundred-per-cent false positive that the merchant learns to ignore
before it ever means anything.

`stock_adjust` is `Always` with entity `stock_event_id`, amount `Exact(0)`, the prepared reason and
content from `PreparedIntent { table: "stock_adjustment_request", hash_field: "content_hash" }`.
Issue and commit each recompute the hash from every request field, and the database refuses an
update after approval, so quantity cannot be smuggled through a money binding or changed under a
stable event id.

### Provisioning, sync, settings

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `provision_status` | `()` → `ProvisionState` | 3 | — | | |
| `provision_enroll` | `{ server_url, enrollment_code }` → `ProvisionState` | 3 | — | ✓ | |
| `provision_recovery_code` | `()` → `RecoveryCodeDisplay` | 1 | — | | |
| `print_recovery_code` | `{ provisioning_id }` → `()` | 1 | — | | |
| `acknowledge_recovery_code` | `{ provisioning_id }` → `()` | 1 | — | | |
| `sync_status` | `()` → `SyncStatus` | 3 | — | | |
| `sync_force_push` | `()` → `()` | 3 | `settings.edit` | ✓ | |
| `settings_get` | `{ keys[] }` → `Map<String, Json>` | 1 | — | | |
| `settings_set` | `{ key, value }` → `()` | 1 | `settings.edit` | ✓ | |
| `tax_rate_upsert` | `{ rule }` → `()` | 3 | `tax.rate.edit` | ✓ | |
| `backup_restore` | `{ path }` → `()` | 1 | `backup.restore` | ✓ | |
| `recovery_state` | `()` → `RecoveryState` | 1 | — | | |
| `recovery_restore_backup` | `{ path, recovery_code }` → `()` | 1 | — | ✓ | |
| `update_check` | `()` → `UpdateStatus` | 5 | `settings.edit` | | |
| `update_download` | `()` → `UpdateHandle` | 5 | `settings.edit` | ✓ | |
| `update_apply` | `()` → `()` | 5 | `settings.edit` | ✓ | |

`settings_get` is unauthenticated for **display-only** keys (locale, decimals, store name) so the lock screen renders correctly. Reading a policy threshold requires a capability — the allowlist lives in the handler and is tested.

**`settings_set` refuses every key in the tax namespace.** A rate is a legal fact with an effective
date, not a preference: it needs `valid_from`/`valid_to`, an overlap check, and a profile scope, and
none of that survives a key/value write. `tax_rate_upsert` under `tax.rate.edit` is the only path,
and it is the answer to "who changes a tax rate on the register" — nobody below the owner.

**`backup_restore` and `recovery_restore_backup` are two different commands on purpose.**
`backup_restore` restores a register whose database opens, and takes `backup.restore`, which only the
owner holds. `recovery_restore_backup` runs when the database **cannot** be opened — the credential
store was wiped, the machine was replaced — and it declares no capability because it cannot check
one: the roles, grants and sessions that would answer "may this user restore?" live inside the
database that will not open. Authorising it with `settings.edit`, as it was, is a deadlock dressed as
a permission check. It is authorised instead by the merchant recovery code issued and displayed once
at provisioning (microstep 1.8.5b), which is also what unwraps the backup's key envelope. It audits
into the restored database as the first entry after the restore, so the trail is not lost with the
old one.

**The updater is wrapped, never exposed.** Tauri's `updater:default` permission puts check, download,
install and download-and-install directly in reach of frontend JavaScript, and the product rule is
that an update is never applied while a shift is open. A rule enforced in Rust that the webview can
route around is not enforced. These three commands are the only path; the capability file grants no
updater permission, and a machine-audited check rejects `updater:default`,
`updater:allow-install` and `updater:allow-download-and-install` if one is ever added. `update_apply`
refuses while a shift is open, and the exhaustiveness test walks plugin commands as well as
`tauri::generate_handler!`, because walking only the latter is why this hole was invisible.

---

## 4 · Events

Emitted from Rust, subscribed in the UI. Long operations are events, not blocking calls.

| Event | Payload | Emitted when |
|---|---|---|
| `cart://changed` | `CartSnapshot` | any cart mutation — the UI never patches locally |
| `card://progress` | `{ handle, stage, message_key }` | `WaitingForCard` → `Processing` → **`CheckingLastTransaction`** → result |
| `card://result` | `{ handle, result }` | authorisation resolves |
| `print://failed` | `{ sale_id, reason }` | print failure after finalize (E.46) |
| `printer://status` | `PrinterStatus` | status changes; drives the pay-time paper warning |
| `fiscal://changed` | `FiscalHealth` | queue depth or state changes |
| `sync://changed` | `SyncStatus` | connectivity or outbox depth changes |
| `shift://changed` | `Option<Shift>` | open, close, force-close |
| `session://locked` | `{ reason }` | idle auto-lock |
| `sale://recovered` | `InFlightSummary` | startup found an in-flight sale to resume (E.1, E.2) |
| `clock://changed` | `ClockConfidence` | the register's trust in its own clock moved |
| `alarm://raised` | `{ kind, detail }` | disk full, audit-chain break, dead-letter, licence grace, clock suspect |

`card://progress` exposing `CheckingLastTransaction` as a **named, visible stage** is deliberate. That state can last many seconds, and a cashier who can read *"Checking last transaction…"* waits, while a cashier watching an unexplained spinner reaches for the button that causes the double charge.

---

## 5 · Rules the catalog encodes

1. **The UI computes no money.** Every total on screen came from `CartSnapshot`. If a number is not in the snapshot, add it to the snapshot — never compute it in TypeScript.
2. **The UI cannot skip a permission check.** Every privileged command constructs an `Authorized<C>` in the handler. Removing the button changes nothing about what the command does.
3. **The UI cannot double-charge.** There is no command that issues a card authorisation without going through the Rust flow that owns the `Unknown` protocol.
4. **The UI cannot see blind-close data early.** The split between `shift_close_begin` and `shift_close_submit_count` is a wire-level guarantee, not a component-level one — and `report_x` is inside that guarantee, not beside it.
5. **The UI cannot mutate a completed sale.** No command exists. Not a disabled one — none.
6. **The UI cannot set an uncontrolled sale price.** Price-bearing arguments exist only on audited
   `cart_override_price`, capped and audited `cart_add_department_sale`, and inert, content-hashed
   `product_quick_add_prepare`; base `sale.create` commands carry no price. Catalogue and label
   prices are resolved inside Rust through `PriceSource`.
7. **The UI cannot reuse or re-aim an approval.** An `approval_id` names one capability, actor,
   approver, entity, exact amount, optional prepared-content hash and reason. Spending it inserts an
   immutable consumption fact beside the effect and audit row; the retained handle remains evidence.
8. **The UI cannot reach an unwrapped plugin.** The updater in particular is three Rust commands, not a granted permission.

| Test | Rule |
|---|---|
| `ipc_commands_all_declare_a_capability` | a command without a capability breaks CI |
| `every_privileged_command_binds_its_approval` | `Always` and `Conditional` mean exactly bound, not merely PIN-gated |
| `conditional_privilege_cannot_cross_threshold_without_approval` | the department-sale predicate cannot be represented as an unchecked boolean |
| `sale_screen_renders_cart_total_and_status_strip` | every displayed total traces to the exact `CartSnapshot` fixture field |
| `prop_no_input_sequence_yields_two_tenders_for_one_auth` | the UI cannot double-charge |
| `expected_is_not_sent_to_the_ui_before_the_count_is_submitted` | blind close is a wire guarantee |
| `x_report_does_not_reveal_expected_cash_to_the_closing_user` | …and an X report does not leak around it (E.84) |
| `prop_no_operation_mutates_a_complete_sale` | not a disabled mutation — no operation can reopen a completed sale |
| `no_command_argument_carries_a_price` | the registry permits price fields only on audited `cart_override_price`, capped audited `cart_add_department_sale`, and inert content-hashed `product_quick_add_prepare`; no base sale command carries one |
| `the_effect_and_the_consumption_commit_together_or_not_at_all` | one approval, one operation, one use |
| `altering_a_stock_request_after_approval_is_refused` | every prepared field is refused by both recomputed-hash validation and the post-approval update trigger |
| `altering_a_quick_add_request_after_approval_is_refused` | a stable product id cannot carry substituted barcode, name, price, tax category or request metadata |
| `fiscal_rebuild_failed_requires_bound_approval_and_preserves_identity` | build remediation is catalogued, manager-approved and cannot mint a new fiscal identity |
| `webview_cannot_invoke_the_updater_plugin` | the shift-open gate cannot be routed around |
| `ipc_errors_carry_no_source_detail_in_release` | `IpcError.detail` is `&'static str`, so the release assertion receives a code rather than formatted source detail |

[`test-catalog.md`](test-catalog.md)'s *IPC contract* table is the coverage matrix over these same
rows. This file is the contract and that one is the ledger, so a name added here belongs in both — a
rule stated in the direction that matters, because a test named only in the coverage matrix has no
contract to defend.
