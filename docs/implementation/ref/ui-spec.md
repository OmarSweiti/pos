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

### RTL mechanics

- `<html dir="rtl" lang="ar">` by default; the English toggle flips `dir` and `lang` only.
- **CSS logical properties everywhere.** `margin-inline-start`, not `margin-left`. Tailwind `ps-*` `pe-*` `ms-*` `me-*` `start-*` `end-*` `text-start` `text-end`.
- Physical utilities (`pl-`, `left-`, `text-left`) **fail lint** (microstep 1.11.2). This is the single cheapest way to keep RTL correct as the UI grows.
- Icons with direction (arrows, chevrons) mirror; icons without (printer, card) do not.
- Numbers, prices and barcodes stay LTR inside RTL text — a bidi isolate, not a manual reverse.

---

## 2 · Screen map

| # | Screen | Phase | Purpose |
|---|---|---|---|
| 1 | Lock / PIN | 1 | sign in, fast user switch; shows register name, sync state, open-shift owner |
| 2 | Shift open | 2 | blocking if none; float entry by denomination |
| 3 | **Sale (home)** | 1 | where the money is made |
| 4 | Tender | 1 | cash → 1; card → 2 |
| 5 | Post-sale toast | 1 | change due, print/reprint, auto-return |
| 6 | Returns | 2 | find sale → line picker → restock → refund tender |
| 7 | Manager approval modal | 1 | the shared escalation pattern |
| 8 | Cash management | 2 | paid in/out, drop, count helper |
| 9 | Shift close wizard | 2 | blind count → reveal → Z preview → print & close |
| 10 | Settings / diagnostics | 1 | device tests, sync detail, about |
| 11 | Local product quick-add | 1 | emergency SKU so the queue never stalls |
| **12** | **Electronic journal** | **2** | searchable log of every document — *added by this plan (J.1)* |
| 13 | Stock count | 4 | scanner-driven counting |
| 14 | Price check | 4 | read-only scan station |
| 15 | Recovery | 1 | keychain loss / half-migrated DB / restore |

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

**Status strip**, always visible:

| State | Display |
|---|---|
| synced | 🔵 synced |
| offline | 🟡 **"Offline — sales are safe and will sync"** · *n* queued |
| fiscal pending | ⏳ *n* awaiting clearance — tap explains what that means |
| training | full-width banner, unmissable, plus a receipt watermark |
| alarm | 🔴 disk full / audit chain / dead letter — blocks nothing, hides from no one |

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

One pattern, used everywhere escalation happens.

```
┌──────────────────────────────────────────┐
│  Manager approval required               │
│  ────────────────────────────────────    │
│  Action:  Refund 24.500 JOD              │
│  Reason:  [ defective            ▾ ]     │
│  Operator: Layla (cashier)               │
│  ────────────────────────────────────    │
│  Manager PIN:  [ • • • • ]               │
│           ┌───┬───┬───┐                  │
│           │ 1 │ 2 │ 3 │  …               │
│  ────────────────────────────────────    │
│         [ cancel ]   [ approve ]         │
└──────────────────────────────────────────┘
```

- The action is stated in **money and words**, never a code.
- The reason picker uses configured reason codes, not free text — free text is unreportable.
- **The approver is logged distinctly from the operator.** Showing "Operator: Layla" makes it visible that these are two people, which is the whole point.
- Self-approval is refused when the policy bans it (E.52), with a clear message rather than a silent failure.

---

## 6 · Screen 9 — Shift close wizard

Four steps, strictly ordered:

1. **Blind count.** Denomination grid. **The expected figure is not on the wire yet** — that is enforced at the IPC layer ([`ipc-contract.md`](ipc-contract.md) §3), not by hiding a field.
2. **Reveal.** Counted, expected, over/short. Beyond a threshold, a manager acknowledgement is required and recorded.
3. **Z preview.** Totals by tender, by tax rate, by category; counts of voids, refunds, price overrides and no-sale drawer opens — the fraud tells.
4. **Print & close.** Immutable, numbered, synced.

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

Designed, not defaulted. Each one is a component with a test.

| State | Copy and behaviour |
|---|---|
| Offline | *"Offline — sales are safe and will sync."* Calm, not alarming |
| Fiscal pending | badge with a count; tap explains clearance in one sentence |
| Printer out of paper | warns **at Pay**, before money is taken — never after |
| Unknown barcode | quick-add (with permission) or department sale; **the queue must not stall** (E.39) |
| Barcode checksum failure | honest rejection, never a guess (E.40) |
| Empty cart | keyboard hints and the tile grid, not a blank rectangle |
| No parked carts | the Resume button hides its badge rather than showing "0" |
| Card terminal unreachable | card button disabled **with the reason stated**; cash unaffected (E.21) |
| Age-restricted item | confirm / decline; declining removes the line and audits (E.69) |
| Displayed-price mismatch | one-tap `displayed_price` override; queues a label reprint (E.70) |
| Screen too small | min-size guard with an instruction (E.60) |
| Half-migrated database | refuses to run, offers restore (E.58) |
| Keychain lost | recovery screen: restore from backup, or re-provision (E.4) |
| Disk full | alarm; refuses new sales; explains why (E.5) |
| Training mode on | full-width banner; auto-off at shift close (E.54) |

---

## 9 · Component inventory

`packages/ui/` — shared between terminal and back office where it makes sense.

```
Numpad          on-screen, ≥48 px, decimal-aware per currency exponent
MoneyDisplay    formats via the store's decimal setting; never toLocaleString inline
QtyDisplay      weighed vs discrete formatting
DenominationGrid float entry and blind count
PinPad          masked, no clipboard, no autofill, no screenshot on mobile
StatusStrip     sync · fiscal · training · alarms
ApprovalModal   the §5 pattern
LineMenu        long-press actions
ReasonPicker    configured codes, never free text
ConfirmDialog   destructive actions, with the action stated in money
ScanCapture     the hidden input + timing heuristic
Toast           post-sale, alerts
EmptyState      illustration + the action that resolves it
```

---

## 10 · Performance budgets on the UI side

| Budget | Limit | How |
|---|---|---|
| Scan → line on screen | < 100 ms | Playwright trace against the simulator |
| Search keystroke → results | < 50 ms | FTS5 + debounce; measured over 50k SKUs |
| Total recompute | < 16 ms | Rust-side `criterion`; the UI only re-renders |
| Cold start → sellable | < 3 s | packaged-app timer on the slowest supported hardware |

The UI never computes a total. It renders `CartSnapshot`. That is why the 16 ms budget is a Rust benchmark and not a React profiling exercise.
