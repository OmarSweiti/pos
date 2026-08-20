# IPC contract — every Tauri command

The narrow, typed boundary between the React UI and the Rust core. It is the **only** channel: no `fs`, no `shell`, no `http` plugin is exposed to the webview. The core talks to the world; the webview talks to the core.

**The registry is the security surface.** Every command declares its required capability and whether it audits. A command missing from the registry fails CI (test `ipc_commands_all_declare_a_capability`, microstep 1.6.7). Hiding a button is UX; the check in the handler is security.

---

## 1 · Shape

```rust
// apps/terminal/src-tauri/src/ipc/mod.rs
#[derive(Serialize, ts_rs::TS)]
pub struct IpcError {
    pub code: &'static str,        // what the UI branches on
    pub message_key: String,       // what the UI translates
    pub detail: Option<String>,    // for the log and diagnostics, never shown raw
}

pub type IpcResult<T> = Result<T, IpcError>;
```

Rules:

1. `snake_case`, verb-first, noun-scoped: `cart_add_line`, not `addLineToCart`.
2. Returns `IpcResult<T>` where `T` derives `Serialize` **and `ts_rs::TS`**.
3. **TypeScript types are generated, never hand-written.** `ts-rs` emits into `packages/api-types/`; CI fails when the committed output differs from a fresh generation. Two hand-maintained copies of a money type is how a rounding bug ships.
4. Long operations return immediately with a handle and emit **events** for progress (§4). A cashier watching a stateless spinner presses the button again.
5. Every command that reverses money or opens the drawer takes an `Authorized<C>` constructed inside the handler and writes the `AuditIntent` the domain returned, in the same transaction as the effect.

---

## 2 · The registry

```rust
// apps/terminal/src-tauri/src/ipc/registry.rs
pub struct CommandSpec {
    pub name: &'static str,
    pub capability: Option<&'static str>,   // None = unauthenticated (lock screen only)
    pub audited: bool,
    pub escalatable: bool,                  // may be approved by a manager PIN
}
pub const COMMANDS: &[CommandSpec] = &[ /* every row of §3 */ ];
```

---

## 3 · The catalog

Legend — **Ph**: phase introduced · **Cap**: required capability · **A**: writes an audit entry · **E**: manager-escalatable.

### Session and users

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `session_state` | `()` → `SessionState` | 1 | — | | |
| `auth_login_pin` | `{ user_code, pin }` → `Session` | 1 | — | ✓ | |
| `auth_logout` | `()` → `()` | 1 | — | ✓ | |
| `auth_switch_user` | `{ user_code, pin }` → `Session` | 1 | — | ✓ | |
| `auth_verify_pin` | `{ user_code, pin, for_capability }` → `ApprovalToken` | 1 | — | ✓ | |
| `user_reset_pin` | `{ user_id, new_pin }` → `()` | 1 | `user.admin` | ✓ | |

`auth_verify_pin` is the manager-approval modal's backing call. It returns a short-lived token naming the capability it authorises, so an approval for a refund cannot be replayed as an approval for a price override.

### Catalog and search

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `catalog_search` | `{ query, limit }` → `Vec<ProductHit>` | 1 | `sale.create` | | |
| `catalog_by_barcode` | `{ code }` → `ScanLookup` | 1 | `sale.create` | | |
| `catalog_by_plu` | `{ code }` → `Option<Product>` | 1 | `sale.create` | | |
| `catalog_tiles` | `{ grid_id? }` → `Vec<Tile>` | 1 | `sale.create` | | |
| `product_quick_add` | `{ draft }` → `Product` | 1 | `product.edit` | ✓ | ✓ |
| `price_check` | `{ code }` → `PriceCheckResult` | 4 | — | | |

`catalog_by_barcode` returns `ScanLookup`, not `Product`: a scan may resolve to a plain product, a price-embedded item carrying a weight or price, an ambiguous collision (E.36), a checksum failure (E.40), or nothing at all (E.39). The UI branches on the variant.

### Cart

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `cart_open` | `()` → `CartSnapshot` | 1 | `sale.create` | | |
| `cart_current` | `()` → `Option<CartSnapshot>` | 1 | `sale.create` | | |
| `cart_add_line` | `{ product_id, qty_milli?, unit_price_minor? }` → `CartSnapshot` | 1 | `sale.create` | | |
| `cart_add_scan` | `{ raw_code }` → `CartSnapshot` | 1 | `sale.create` | | |
| `cart_set_qty` | `{ line_id, qty_milli }` → `CartSnapshot` | 1 | `sale.create` | | |
| `cart_void_line` | `{ line_id, reason }` → `CartSnapshot` | 1 | `line.void` | ✓ | |
| `cart_discount_line` | `{ line_id, kind, value, reason }` → `CartSnapshot` | 1 | `discount.manual` | ✓ | ✓ |
| `cart_discount_basket` | `{ kind, value, reason }` → `CartSnapshot` | 1 | `discount.manual` | ✓ | ✓ |
| `cart_override_price` | `{ line_id, unit_price_minor, reason }` → `CartSnapshot` | 1 | `price.override` | ✓ | ✓ |
| `cart_confirm_age` | `{ line_id, confirmed }` → `CartSnapshot` | 2 | `sale.create` | ✓ | |
| `cart_attach_customer` | `{ customer_id }` → `CartSnapshot` | 3 | `sale.create` | | |
| `cart_set_buyer_tin` | `{ tin, name? }` → `CartSnapshot` | 2 | `sale.create` | | |
| `cart_park` | `{ label? }` → `ParkedRef` | 1 | `sale.park` | | |
| `cart_resume` | `{ parked_id }` → `CartSnapshot` | 1 | `sale.resume` | | |
| `cart_list_parked` | `()` → `Vec<ParkedRef>` | 1 | `sale.resume` | | |
| `cart_void_sale` | `{ reason }` → `()` | 1 | `sale.void` | ✓ | ✓ |
| `cart_set_training` | `{ on }` → `CartSnapshot` | 1 | `training_mode.toggle` | ✓ | |

`CartSnapshot` is the **priced** cart — lines, discount attributions, tax summary, totals — so the UI never computes money. It re-renders what Rust decided.

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
| `sale_reprint` | `{ sale_id }` → `()` | 1 | `sale.create` | ✓ | |

`tender_start_card` returns a handle and drives the flow through events (§4). The `Unknown`/timeout protocol — status-query before any retry (Phase 2, 2.1.3) — lives entirely on the Rust side. The UI **cannot** issue a second authorisation because no command exists that would let it.

### Returns

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `return_find_sale` | `{ receipt_no? , barcode?, card_last4?, customer_id? }` → `Vec<SaleRef>` | 2 | `refund.receipted` | | |
| `return_refundable` | `{ sale_id }` → `Vec<RefundableLine>` | 2 | `refund.receipted` | | |
| `return_build` | `{ sale_id, lines[], restock[] }` → `RefundPreview` | 2 | `refund.receipted` | | |
| `return_commit` | `{ preview_id, tenders[] }` → `CompletedSaleRef` | 2 | `refund.receipted` | ✓ | ✓ |
| `return_receiptless` | `{ lines[] }` → `RefundPreview` | 2 | `refund.receiptless` | ✓ | ✓ |

`return_build` produces a **preview** with an id; `return_commit` references it. The refundable-quantity check runs at both points, so a second cashier cannot slip a refund between preview and commit.

### Shift and cash

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `shift_open` | `{ float_by_denomination }` → `Shift` | 2 | `shift.open` | ✓ | |
| `shift_current` | `()` → `Option<Shift>` | 2 | — | | |
| `shift_close_begin` | `()` → `CloseSession` | 2 | `shift.close` | | |
| `shift_close_submit_count` | `{ session_id, count_by_denomination }` → `CloseResult` | 2 | `shift.close` | ✓ | |
| `shift_force_close_stale` | `{ shift_id }` → `CloseResult` | 2 | `shift.close` | ✓ | ✓ |
| `cash_movement` | `{ kind, amount_minor, reason, note? }` → `()` | 2 | `cash.paid_in_out` | ✓ | ✓ |
| `drawer_open_no_sale` | `{ reason }` → `()` | 2 | `drawer.open` | ✓ | ✓ |
| `report_x` | `()` → `XReport` | 2 | `zreport.run` | | |
| `report_z` | `()` → `ZReport` | 2 | `zreport.run` | ✓ | |

> **Blind close is enforced here, not in the UI.** `shift_close_begin` returns a `CloseSession` **without** the expected figure. `shift_close_submit_count` returns the comparison. The expected amount is never on the wire before the count is submitted — asserted by test `expected_is_not_sent_to_the_ui_before_the_count_is_submitted`.

### Journal, health, diagnostics

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `journal_search` | `{ filters }` → `Vec<JournalEntry>` | 2 | `reports.all` | | |
| `journal_detail` | `{ sale_id }` → `JournalDetail` | 2 | `reports.all` | | |
| `health_status` | `()` → `HealthSnapshot` | 1 | — | | |
| `diag_test_print` | `()` → `()` | 1 | `settings.edit` | ✓ | |
| `diag_drawer_kick` | `()` → `()` | 1 | `drawer.open` | ✓ | |
| `diag_scanner_echo` | `{ on }` → `()` | 1 | `settings.edit` | | |
| `diag_terminal_ping` | `()` → `TerminalInfo` | 2 | `settings.edit` | | |
| `diag_fiscal_state` | `()` → `FiscalHealth` | 2 | `settings.edit` | | |
| `diag_backup_state` | `()` → `BackupInfo` | 1 | `settings.edit` | | |
| `diag_verify_audit_chain` | `()` → `ChainVerdict` | 1 | `settings.edit` | ✓ | |

`health_status` is unauthenticated because the lock screen shows it — sync state, offline queue depth, uncleared fiscal count. A cashier who cannot sign in still needs to know whether the register is healthy.

### Provisioning, sync, settings

| Command | Args → Returns | Ph | Cap | A | E |
|---|---|---|---|---|---|
| `provision_status` | `()` → `ProvisionState` | 3 | — | | |
| `provision_enroll` | `{ server_url, enrollment_code }` → `ProvisionState` | 3 | — | ✓ | |
| `sync_status` | `()` → `SyncStatus` | 3 | — | | |
| `sync_force_push` | `()` → `()` | 3 | `settings.edit` | ✓ | |
| `settings_get` | `{ keys[] }` → `Map<String, Json>` | 1 | — | | |
| `settings_set` | `{ key, value }` → `()` | 1 | `settings.edit` | ✓ | |
| `recovery_state` | `()` → `RecoveryState` | 1 | — | | |
| `recovery_restore_backup` | `{ path }` → `()` | 1 | `settings.edit` | ✓ | |

`settings_get` is unauthenticated for **display-only** keys (locale, decimals, store name) so the lock screen renders correctly. Reading a policy threshold requires a capability — the allowlist lives in the handler and is tested.

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
| `alarm://raised` | `{ kind, detail }` | disk full, audit-chain break, dead-letter, licence grace |

`card://progress` exposing `CheckingLastTransaction` as a **named, visible stage** is deliberate. That state can last many seconds, and a cashier who can read *"Checking last transaction…"* waits, while a cashier watching an unexplained spinner reaches for the button that causes the double charge.

---

## 5 · Rules the catalog encodes

1. **The UI computes no money.** Every total on screen came from `CartSnapshot`. If a number is not in the snapshot, add it to the snapshot — never compute it in TypeScript.
2. **The UI cannot skip a permission check.** Every privileged command constructs an `Authorized<C>` in the handler. Removing the button changes nothing about what the command does.
3. **The UI cannot double-charge.** There is no command that issues a card authorisation without going through the Rust flow that owns the `Unknown` protocol.
4. **The UI cannot see blind-close data early.** The split between `shift_close_begin` and `shift_close_submit_count` is a wire-level guarantee, not a component-level one.
5. **The UI cannot mutate a completed sale.** No command exists. Not a disabled one — none.

Each of these is a test, listed in [`test-catalog.md`](test-catalog.md) under *IPC contract*.
