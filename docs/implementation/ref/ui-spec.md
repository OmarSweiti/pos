# UI specification — the terminal

**Design law:** the sale screen is where the business makes money; every other screen exists to keep it honest.

Optimise for a cashier's eight-hour day, not for a screenshot.

---

## 1 · Non-negotiables

| Rule | Why |
|---|---|
| **≥ 48 px hit targets** | fingers, not cursors; often gloved, often fast |
| **Zero hover-dependence** | touch screens have no hover; a tooltip is invisible |
| **On-screen numpad wherever a number is entered** | registers frequently have no keyboard |
| **Full keyboard operability** | **scanning *is* typing** — the two are the same input path |
| **< 100 ms scan → line on screen** | the rhythm is a few seconds per item |
| **Optimistic UI on local data** | it is local; there is nothing to wait for |
| **Arabic-first RTL from the first commit** | retrofitting RTL is miserable; scaffolding it is cheap |
| **Western Arabic digits (0–9)** | Jordanian retail practice |
| **Empty and edge states are designed, not defaulted** | the offline banner is a product feature |
| **The UI computes no money** | every figure on screen came out of `CartSnapshot` ([`ipc-contract.md`](ipc-contract.md) §5) |

### RTL mechanics

- `<html dir="rtl" lang="ar">` by default; the English toggle flips `dir` and `lang` only.
- **CSS logical properties everywhere.** `margin-inline-start`, not `margin-left`. Tailwind `ps-*` `pe-*` `ms-*` `me-*` `start-*` `end-*` `text-start` `text-end`.
- Physical utilities (`pl-`, `left-`, `text-left`) **fail lint** (microstep 1.11.2). This is the single cheapest way to keep RTL correct as the UI grows.
- Icons with direction (arrows, chevrons) mirror; icons without (printer, card) do not.
- Numbers, prices and barcodes stay LTR inside RTL text — a bidi isolate, not a manual reverse.

### Money on screen

Two precisions exist and confusing them charges the customer one amount and shows them another.

| Where | Precision | Component |
|---|---|---|
| Shelf price, catalogue list, tile label, price check | the store's `money_decimals` | `ShelfPrice` |
| **Everything else** — line totals, discounts, tax rows, the cash-rounding adjustment, TOTAL, tenders, change, refunds, reports, receipts | the **currency's own exponent**, always | `MoneyDisplay` |

`money_decimals` is a shelf-display setting and nothing more. JOD has three decimals: a two-decimal render of `1.259` shows `1.25` while the card is charged `1.259`, and a three-fil rounding line renders as `0.00` — money the screen hides from the person paying it. `ShelfPrice` therefore **refuses** an amount that is not exactly representable at the configured precision rather than truncating it, which is the same contract `Money::format` carries in Rust ([`domain-api.md`](domain-api.md) §1.2). `MoneyDisplay` has no precision argument at all; there is no code path in which a settings value can reach it.

Neither component calls `toLocaleString` inline. Both take minor units and a currency, never a float, and never a pre-formatted string from a handler.

---

## 2 · Screen map

**Drives** names the commands in [`ipc-contract.md`](ipc-contract.md) §3 that a screen exists to reach. A command in that catalogue with no screen here is unbuildable through the architecture this plan requires; a screen here with no command is a drawing.

| # | Screen | Ph | Drives | Purpose |
|---|---|---|---|---|
| 1 | Lock / PIN | 1 | `auth_login_pin`, `auth_switch_user`, `session_state`, `health_status` | sign in, fast user switch; shows register name, sync state, open-shift owner |
| 2 | **Shift open** | **1** | `shift_open`, `shift_current` | blocking if none; float entry by denomination |
| 3 | **Sale (home)** | 1 | `cart_*`, `catalog_*`, `stock_on_hand` | where the money is made |
| 4 | Tender | 1 | `tender_*`, `sale_finalize` | cash → 1; card → 2 |
| 5 | Post-sale toast | 1 | `sale_reprint` | change due, print/reprint, auto-return |
| 6 | Returns | 2 | `return_*`, `exchange_commit` | find sale → line picker → restock → refund tender |
| 7 | **Manager approval modal** | **1** | `auth_verify_pin` | the shared escalation pattern |
| 8 | Cash management | 2 | `cash_location_list`, `cash_movement`, `drawer_open_no_sale` | paid in/out, drop, bank deposit, count helper |
| 9 | Shift close | 1 / 2 | `shift_close`; then `shift_close_begin`, `shift_close_submit_count`, `shift_force_close_stale`, `report_z` | ordinary own-shift close needs no approval; Phase 2 adds blind count and separately approved stale-shift force-close |
| 10 | Settings / diagnostics | 1 / 2 | `diag_test_print`, `diag_scanner_echo`, `diag_backup_state`, `diag_verify_audit_chain`, `settings_get`, `settings_set`, `backup_restore`; then `diag_terminal_ping`, `diag_fiscal_state`, `fiscal_rebuild_failed`, `drawer_open_no_sale` | device tests, sync detail and about; drawer diagnostics reuse the audited no-sale path |
| 11 | Local product quick-add | 1 | `product_quick_add_prepare`, `product_quick_add` | freeze the proposal before approval; emergency SKU so the queue never stalls |
| 12 | Electronic journal | 2 | `journal_search`, `journal_detail` | searchable log of every document — *added by this plan (J.1)* |
| 13 | Stock count | 4 | `stock_count_*` | scanner-driven counting |
| 14 | Price check | 4 | `price_check` | read-only scan station |
| 15 | Recovery | 1 | `recovery_state`, `recovery_restore_backup` | keychain loss / half-migrated DB / restore by recovery code |
| **16** | **Filing report** | **1** | `report_tax_by_rate` | the sales-side tax reconciliation the whole tax engine exists for |
| **17** | **Department sale** | **1** | `department_list`, `cart_add_department_sale` | the unknown-barcode path that does not stall the queue |
| **18** | **Day so far / X report** | **2** | `report_day_so_far`, `report_x` | takings by hour, by tender, by cashier; the mid-shift read |
| **19** | **Stock adjust** | **1** | `stock_on_hand`, `stock_adjust_prepare`, `stock_adjust` | freeze the proposal before approval; the only Phase-1 path that puts goods *in* |
| **20** | **Buyer invoice details** *(modal)* | **2** | `cart_set_buyer_tin`, `cart_clear_buyer_tin` | the B2B customer who asks for their TIN on the invoice |
| **21** | **Recovery-code provisioning** | **1** | `provision_recovery_code`, `print_recovery_code`, `acknowledge_recovery_code` | display and print the merchant-held code once, then prove it was acknowledged before normal startup |

**The bold rows moved or are new, and each for a stated reason:**

- **Screen 2 moved to Phase 1** because the shift skeleton did. `Cart` carries a non-optional `shift_id` and conventions §11 defines the business date as the business date of its shift, so a Phase 1 with no shift had no defined way to obtain either. Phase 1 gets open, close, an opening float and one-open-per-register; the blind count, over/short and X/Z stay in Phase 2 ([`ipc-contract.md`](ipc-contract.md) §3).
- **Screen 7 moved to Phase 1** because escalation exists in Phase 1. `cart_discount_line`, `cart_override_price`, `cart_void_sale`, `product_quick_add`, `stock_adjust` and `cart_add_department_sale` are all Phase-1 escalatable commands. Without the modal, an implementer either denies the escalation or invents an unbound approval — which is the bearer-token failure the `ApprovalHandle` exists to prevent.
- **Screen 16 is new.** Phase 1's exit gate says *"run the tax report for the day and check it against the receipts by hand"*, and no screen, no command and no microstep produced one. It is the deliverable the tax engine exists for, and until it exists a merchant trading on Phases 1–3 reconstructs a bi-monthly return by hand from paper.
- **Screen 17 is new.** "Department sale" was named as an unknown-barcode policy in four documents and designed in none, so the default policy at 22:00 in a one-person shop was *fetch a manager*.
- **Screen 18 is new.** Two report commands had nowhere to be invoked from, and *"print me today's takings — by hour and by cashier"* is a week-one question over facts that already exist. Its X report sits inside the blind-close guarantee, not beside it (§6).
- **Screen 19 is new.** Every path that increases stock arrived in Phase 4, so from the first sale every product went negative and stayed negative, and the negative-stock flag the plan calls loud was a hundred-per-cent false positive before it ever meant anything.
- **Screen 20 is new.** Buyer TIN capture had a command, two `sale` columns and a fiscal conformance rule, and no way for a cashier to type one — while the printed document had nowhere to put it either ([`hardware-and-receipts.md`](hardware-and-receipts.md) §2.3). The one customer who explicitly asks for something got neither half.
- **Screen 21 is new.** A recovery code that exists only in a backend microstep is not merchant-held custody. Provisioning must display it once, offer the one direct print, and require acknowledgement before the in-memory value is discarded; otherwise the first proof of failure is a restore with no usable code.

---

## 3 · Screen 3 — Sale

The one screen that matters. Three zones, laid out **logically** so RTL and LTR both work without a second layout.

```
┌──────────────────────────────────────────────────────────────────────┐
│ ⬤ synced   shift: Layla   REG01   14:22        [training banner]     │  status strip
├────────────────────────────────┬─────────────────────────────────────┤
│  CART                          │  SEARCH  [ F2 ]                     │
│  ┌──────────────────────────┐  │  ┌───────────────────────────────┐  │
│  │ خبز عربي                 │  │  │ type or scan…                 │  │
│  │   2 × 0.400      0.800   │  │  └───────────────────────────────┘  │
│  │ حليب طازج ١ لتر          │  │  ┌────┬────┬────┬────┐              │
│  │   1 × 0.950      0.950   │  │  │خضار│فواكه│مخبز│مشروبات│  tiles    │
│  │ طماطم (وزن)              │  │  ├────┼────┼────┼────┤              │
│  │   0.347 kg × 1.200 0.416 │  │  │    │    │    │    │              │
│  └──────────────────────────┘  │  └────┴────┴────┴────┘              │
├────────────────────────────────┴─────────────────────────────────────┤
│  subtotal 2.166   discount 0.000   tax 0.299                         │
│                                              TOTAL  2.166            │
├──────────────────────────────────────────────────────────────────────┤
│  [ park F6 ] [ resume F7 ⑵ ] [ customer ] [ returns F9 ] [ PAY F4 ]  │
└──────────────────────────────────────────────────────────────────────┘
```

**Cart line.** Name, quantity stepper, unit, line total. Long-press or right-click opens the line menu: set quantity · discount · price override · void line. Discount attributions render beneath the line they belong to, so a cashier can answer "why is this cheaper?" without leaving the screen.

**Totals block.** TOTAL is the largest element on the screen. A cashier reads it a thousand times a day and a customer reads it upside down.

### The status strip

Always visible, and it is now the register's whole health surface — six other reference documents expect something to appear here. The design constraint is that **a cashier must never be shown a problem they cannot act on**, because an indicator that means "somebody else has work to do" is an indicator that gets ignored, and it takes the ones that matter with it.

| State | Display | Blocks selling? | What the cashier does |
|---|---|---|---|
| synced | 🔵 synced | no | nothing |
| offline | 🟡 **"Offline — sales are safe and will sync"** · *n* queued | no | nothing — that is the point of the copy |
| outbox deep | the same badge, with the depth, once it crosses the alarm threshold | no | mentions it at handover |
| fiscal pending | ⏳ *n* awaiting clearance — tap explains what that means in one sentence | no | nothing |
| fiscal build failed | ⚠ with a count; tap names the remediation path | no | tells the manager |
| clock | 🕐 shown **only** at `Suspect` or `Untrusted`; tap says the business date will be confirmed at shift open | no | confirms the date when asked |
| printer | 🖨 shown when the printer is unreachable, or a job is waiting for an answer (§8) | no | answers the job's one question, or trades on |
| backup | shown only when the newer of the two destinations crosses its age threshold | no | tells the manager |
| licence grace | shown in the final days of the grace period | no | tells the owner |
| training | full-width banner, unmissable, plus a receipt watermark | no | ends training mode when the shift closes |
| audit chain · dead letter | 🔴 alarm; blocks nothing, hides from no one | no | tells the manager |
| **disk full** | 🔴 with the reason stated | **yes** — a new sale cannot be started or finalized (E.5) | frees space or calls support; this is the one that stops trade |

**Exactly one row blocks selling, and it is the one where a sale could not be durably written anyway.** An earlier version of this file said alarms "block nothing" in this table and that disk-full "refuses new sales" in §8; a cashier who kept selling on the first reading would have been taking money the register could not record.

The offline copy is deliberate. *"Offline"* alone makes a cashier stop selling. *"Sales are safe and will sync"* is the entire product promise in five words.

**Global scan capture.** A hidden input listens everywhere on this screen. Scan bursts are distinguished from typing by inter-key timing, **and route correctly even when focus is in the search box** — that is where most implementations break.

---

## 4 · Screen 4 — Tender

```
┌──────────────────────────────────────────────────────────────┐
│                    AMOUNT DUE   2.166                        │
├───────────────────────────┬──────────────────────────────────┤
│  [ CASH ]  [ CARD ]  [ … ]│  collected:                      │
│  ┌───┬───┬───┐            │    cash   1.000                  │
│  │ 7 │ 8 │ 9 │  quick:    │    ─────────────                 │
│  ├───┼───┼───┤  [0.500]   │    remaining  1.166              │
│  │ 4 │ 5 │ 6 │  [1.000]   │                                  │
│  ├───┼───┼───┤  [5.000]   │                                  │
│  │ 1 │ 2 │ 3 │  [10.000]  │                                  │
│  ├───┴───┼───┤  [exact]   │                                  │
│  │   0   │ ⌫ │            │                                  │
│  └───────┴───┘            │                                  │
├───────────────────────────┴──────────────────────────────────┤
│  change: —                              [ COMPLETE ]         │
└──────────────────────────────────────────────────────────────┘
```

Every figure on this screen — due, collected, remaining, change, the rounding line — renders through `MoneyDisplay` at the currency exponent (§1). This is the screen where a hidden fil is a customer dispute.

**Card states, made visible:**

```
Waiting for card …
Processing …
Checking last transaction …        ← named, visible, no cancel button
Approved · **** 4242 · Visa
```

That third state can last many seconds. A cashier reading *"Checking last transaction…"* waits. A cashier watching an unexplained spinner presses the button that causes the double charge. **The visibility of this state is a money-safety feature, not a polish item.**

**Split tender is the default model, not a mode.** The collected list and remaining due are always on screen, even for a single cash payment — so the first split tender a cashier does needs no learning.

**Cash rounding** appears as its own line the moment it applies, with the adjustment stated. Never a silent number change.

---

## 5 · Screen 7 — Manager approval modal

One pattern, used everywhere escalation happens — from Phase 1, because escalation exists from Phase 1.

```
┌──────────────────────────────────────────┐
│  Manager approval required               │
│  ────────────────────────────────────    │
│  Action:  Refund 24.500 JOD              │
│  On:      receipt R-000412               │
│  Reason:  [ defective            ▾ ]     │
│  Operator: Layla (cashier)               │
│  Valid for: 45 s                         │
│  ────────────────────────────────────    │
│  Manager PIN:  [ • • • • ]               │
│           ┌───┬───┬───┐                  │
│           │ 1 │ 2 │ 3 │  …               │
│  ────────────────────────────────────    │
│         [ cancel ]   [ approve ]         │
└──────────────────────────────────────────┘
```

- The action is stated in **money and words**, never a code, and it names the **thing** as well as the amount. That pair — capability, entity, amount, actor — is exactly what `auth_verify_pin` binds the approval to ([`domain-api.md`](domain-api.md) §8.1). What the manager reads is what the handler will later refuse to deviate from.
- The reason picker uses configured reason codes, not free text — free text is unreportable.
- **The approver is logged distinctly from the operator.** Showing "Operator: Layla" makes it visible that these are two people, which is the whole point.
- `ban_self_approval` decides whether an operation must enter this escalation flow. Once it does,
  self-approval is always refused (E.52), with a clear message rather than a silent failure.
- **The countdown is shown because the approval really does expire**, in seconds rather than minutes, since the manager is standing at the till. An expired approval fails at the command, so hiding the timer would produce a refusal a cashier cannot explain to the queue.
- **The modal receives an `approval_id` and nothing else.** The handle's fields stay in Rust; handing the webview a self-contained proof would make it a bearer token in JavaScript, which is the thing being prevented.
- **One approval, one operation, one use.** Changing the amount, the sale, the reason or the cashier after approval invalidates it — so the UI re-opens this modal rather than reusing the last answer, and a second attempt on the same handle is refused rather than silently repeated.

---

## 6 · Screen 9 — Shift close wizard

Four steps, strictly ordered:

1. **Blind count.** Denomination grid. **The expected figure is not on the wire yet** — that is enforced at the IPC layer ([`ipc-contract.md`](ipc-contract.md) §3), not by hiding a field.
2. **Reveal.** Counted, expected, over/short. Beyond a threshold, a manager acknowledgement is required and recorded.
3. **Z preview.** Totals by tender, by tax rate, by category; counts of voids, refunds, price overrides and no-sale drawer opens — the fraud tells.
4. **Print & close.** Immutable, numbered, synced.

Screen 18's X report sits inside the same guarantee rather than beside it: for a user who holds `shift.close` on the currently open shift, it omits the cash-tender total and the expected figure entirely. Otherwise a shift lead reads their own target two minutes before counting to it, and the blind count is decorative.

---

## 7 · Keyboard map

Memorisable, and printed on a card taped to the register.

| Key | Action |
|---|---|
| `F2` | search |
| `F4` | pay |
| `F6` | park |
| `F7` | resume |
| `F9` | returns |
| `Del` | void line |
| `+` / `−` | quantity |
| `F12` | lock |
| `Esc` | back / cancel |
| `Enter` | confirm / commit scan |

**Barcode scans need no focus.** Every action is reachable without a mouse — test `every_action_reachable_without_a_mouse`.

---

## 8 · Empty and edge states

Designed, not defaulted. The states with **behaviour** carry a named test; the rest are confirmed by eye in the release pass (§8a), and saying which is which is the difference between coverage and a claim.

| State | Copy and behaviour | What stops it regressing |
|---|---|---|
| Offline | *"Offline — sales are safe and will sync."* Calm, not alarming | `offline_banner_states_sales_are_safe` |
| Fiscal pending | badge with a count; tap explains clearance in one sentence | eye |
| Printer out of paper | warns **at Pay**, before money is taken — never after | `paper_out_warns_before_tender_not_after` |
| **No printer at all** | the shift opens, the sale completes, the artifact queues; an alarm, never a modal ([`hardware-and-receipts.md`](hardware-and-receipts.md) §3a) | `the_missing_printer_is_an_alarm_not_a_modal` |
| **Print outcome unknown** | one question — *did a readable receipt come out?* — and nothing retries until it is answered | `an_unknown_job_resolves_only_by_operator_answer` |
| Unknown barcode | quick-add (with permission) **or department sale**; **the queue must not stall** (E.39) | `queue_never_stalls_on_unknown_code` |
| Barcode checksum failure | honest rejection, never a guess (E.40) | `prop_corrupt_digit_never_parses_clean` |
| Empty cart | keyboard hints and the tile grid, not a blank rectangle | eye |
| No parked carts | the Resume button hides its badge rather than showing "0" | eye |
| Card terminal unreachable | card button disabled **with the reason stated**; cash unaffected (E.21) | `card_disabled_when_terminal_unreachable_cash_still_works` |
| **An in-flight sale was found at startup** | the cashier is told what was recovered before anything else happens, and a card operation that was outstanding is status-queried, not assumed (E.1, E.2) | `an_interrupted_tendering_is_recovered_and_status_queried` |
| Age-restricted item | confirm / decline; declining removes the line and audits (E.69) | `age_decline_removes_line_and_audits` |
| Displayed-price mismatch | one-tap `displayed_price` override; queues a label reprint (E.70) | `displayed_price_override_queues_a_label_reprint` |
| **Clock suspect or untrusted** | shift open asks the operator to confirm the business date, and says why; selling is never refused for a clock | `no_clock_confidence_refuses_a_sale` |
| Screen too small | min-size guard with an instruction (E.60) | `sale_screen_min_size_guard` |
| Half-migrated database | refuses to run, offers restore (E.58) | `half_migrated_db_refuses_to_open_with_a_named_error` |
| **Keychain lost** | screen 15: choose a backup, enter the **printed recovery code**, restore. No sign-in, because the grants live in the database that will not open (E.4, E.4d) | `a_backup_opens_with_the_recovery_code_alone` |
| Disk full | alarm; **refuses to start or finalize a sale**; explains why (E.5) | `low_disk_blocks_new_sales_and_alarms` |
| Training mode on | full-width banner; auto-off at shift close (E.54) | `training_auto_off_at_shift_close` |

**A named test in that column defends the behaviour, not the screen.** Most of them live in Rust or in an integration harness: they prove that a low-disk register refuses the sale, that an age decline removes the line, that a queued artifact survives a restart. Whether the screen then *says the right thing* about it is a different question with a different answer (§8a), and the rows marked *eye* have only that second half.

**The recovery screen is a flow, not a state.** It runs before any session exists, so it shows: what is wrong (`recovery_state`), which backups it can see and how old each is, a field for the recovery code, and — after a successful restore — what was recovered, counted in sales. It never displays the recovery code, and it never offers "continue without restoring", because minting a fresh key over an existing database is how an openable register becomes an unopenable one.

---

## 8a · What actually defends this interface

The previous version of §8 said each edge state "is a component with a test", and that was not true of a single one of them. The whole automated defence of an Arabic-first RTL interface was a grep for physical CSS utilities. Stating coverage that does not exist is worse than stating none, because it is what stops anybody building the coverage.

Here is the real ladder, weakest first. Each rung catches a class the rung below cannot.

| Layer | Catches | Cannot catch |
|---|---|---|
| **Logical-CSS lint** (1.11.2) | `pl-4`, `left-0`, `text-left` — a physical utility anywhere in the tree | order, mirroring, truncation side, bidi |
| **DOM component tests** — `@testing-library/react` + `user-event` + `jsdom`, with fake timers | keyboard reachability, scan routing and burst timing, bidi isolation of a Latin SKU inside an Arabic name, the min-size guard, and that each §8 state renders the copy and the disabled controls its row describes | anything that is only visible as pixels |
| **Screenshot baselines** of the sale, tender and approval screens in `ar` **and** `en`, over the seeded fixture, at the reference register's resolution | a flex row that reverses, an icon that should mirror and does not, an ellipsis on the wrong side, a total that overflows its box in one direction only | whether the Arabic *reads* well |
| **Accessibility baseline** — automated rules on the same three screens: contrast, name/role/value, focus order, no keyboard trap | a regression in the things a cashier with tired eyes at hour seven depends on | everything an automated accessibility check cannot see, which is most of it |
| **A native reader, on the register's own display** | shaping quality, tone, whether a mirrored icon reads as *back* or as *forward* | nothing — but it happens once per release, so it must not be the only rung |

Three rules make this a mechanism rather than a list:

1. **A screenshot baseline is a binary artifact, so it is reviewed as an image diff** — the same discipline the receipt goldens carry ([`hardware-and-receipts.md`](hardware-and-receipts.md) §2.2). Here the artifact *is* the image, so the diff is the review, and a baseline updated to make a check pass is a check deleted.
2. **The accessibility baseline is a floor, not a conformance claim.** It says a set of automated rules passes on three screens. It does not say this product meets any accessibility standard, and no document, UI string or sales conversation may say that it does.
3. **The native-reader pass covers the screen, not only the paper.** It is check 13 of the hardware-lab checklist, in the same session, on the register's own display — because the receipt has seven goldens defending it and the screen, until these layers exist, has a grep.

The harnesses the middle three rungs need — a DOM environment for `apps/terminal`, and a packaged-app driver — are inventoried in [`test-catalog.md`](test-catalog.md). `apps/terminal` today has `vitest` and no DOM environment at all, while `apps/backoffice` already has the pattern; so every component test this section depends on, including the ones the catalogue already lists against that harness, cannot be written yet. **That is a missing harness, not a missing intention**, and it is why it is written down here rather than discovered at the moment the first of those tests is due.

---

## 9 · Component inventory

`packages/ui/` — shared between terminal and back office where it makes sense.

```
Numpad          on-screen, ≥48 px, decimal-aware per currency exponent
MoneyDisplay    ALWAYS the currency exponent; no precision argument exists
ShelfPrice      the ONLY component that may use money_decimals; refuses an
                amount not exactly representable at it
QtyDisplay      weighed vs discrete formatting
DenominationGrid float entry and blind count
PinPad          masked, no clipboard, no autofill, no screenshot on mobile
StatusStrip     sync · fiscal · clock · printer · backup · training · alarms (§3)
ApprovalModal   the §5 pattern: binding, countdown, one use
LineMenu        long-press actions
ReasonPicker    configured codes, never free text
ConfirmDialog   destructive actions, with the action stated in money
ScanCapture     the hidden input + timing heuristic
Toast           post-sale, alerts
EmptyState      illustration + the action that resolves it
```

---

## 10 · Performance budgets on the UI side

| Budget | Limit | Measured by |
|---|---|---|
| Scan → line on screen | < 100 ms | packaged-app WebDriver trace against the hardware simulator |
| Search keystroke → results | < 50 ms | FTS5 + debounce; measured over 50k SKUs |
| Total recompute | < 16 ms | Rust-side `criterion`; the UI only re-renders |
| Cold start → sellable | < 3 s | packaged-app timer |

Two corrections are load-bearing here.

**The measurement runs against the packaged application, not a browser.** Playwright automates the browser engines it bundles; it cannot attach to a Tauri window's WebView2, WKWebView or WebKitGTK. The scan and cold-start budgets are the two that describe what a merchant actually runs, so they are measured through `tauri-driver`, on the harness [`test-catalog.md`](test-catalog.md) inventories — otherwise they are measured against something nobody ships.

**"The slowest supported hardware" now has a referent.** It is the lowest row of the register class in the device matrix ([`hardware-and-receipts.md`](hardware-and-receipts.md) §6a). A budget measured on a machine nobody named is a budget that fails randomly and gets disabled within a month.

The UI never computes a total. It renders `CartSnapshot`. That is why the 16 ms budget is a Rust benchmark and not a React profiling exercise.
