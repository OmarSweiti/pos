# Phase 2 — Money-grade

> **Exit:** the register takes cards that reconcile to the fil, handles returns without being defrauded, closes a shift that balances, and produces fiscal documents that pass every check short of the ISTD network.

**Effort:** 8–10 weeks.
**Scope from the master plan:** C.4 card, C.5 returns/refunds/voids, C.6 shifts & cash, C.11 fiscal pipeline, C.12 X/Z reports, D screens 6–9, migrations 0007–0009, diagnostics.

This phase is where the product stops being a demo. Every feature in it exists because money moves in a direction someone can exploit, or because a device lies about what happened.

---

## Group dependency graph

```
2.1 payment terminal abstraction ──→ 2.2 card tenders ──┐
                                                         ├──→ 2.3 refunds & returns ──┐
2.4 shifts & cash movements ─────────────────────────────┤                             │
                                                         └──→ 2.5 X / Z reports ───────┤
2.6 electronic journal ──────────────────────────────────────────────────────────────  ┤
2.7 fiscal pipeline (independent — start it early) ────────────────────────────────────┤
2.8 diagnostics ──────────────────────────────────────────────────────────────────────┤
                                                                                       ▼
                                                                            2.9 gate & drills
```

**Start 2.7 early and in parallel.** It has no dependency on the card work, it is the largest unknown, and the conformance harness needs iteration time.

---

## Group 2.1 — The payment terminal abstraction

*Blueprint §6. The trait is the insulation; PSP availability in Jordan is not known until you ask.*

### 2.1.1 — Choose the acquirer, before writing driver code
**Files:** `docs/implementation/ref/merchant-decisions.md` (record the answer)
Provider availability and terminal certification vary sharply by country. Evaluate what actually operates in Jordan — bank-provided terminals, regional PSPs, Network International — before a line of integration code.

For each candidate, get in writing: the integration protocol (JSON over local network / cloud API / serial), whether a **last-transaction-status query** exists (non-negotiable — §2.1.3), partial-approval support, refund-by-original-reference support, and **the terminal's PCI P2PE listing number if it has one**.

> That last item is correction §4 in [`ref/plan-validation.md`](ref/plan-validation.md): "semi-integrated" and "P2PE-validated" are different properties. Without a listing number the merchant is on SAQ B-IP or C, not SAQ P2PE. The engineering does not change; the claim does.

**Done when:** one acquirer chosen, protocol documentation in hand, and a physical test terminal on the desk.

### 2.1.2 — The `PaymentTerminal` trait
**Files:** `crates/pos-hardware/src/payment.rs` (new)
```rust
pub trait PaymentTerminal: Send + Sync {
    fn authorize(&self, amount: Money, sale_ref: &str) -> Result<TenderResult, PayError>;
    fn refund(&self, amount: Money, original_psp_ref: &str) -> Result<TenderResult, PayError>;
    fn reverse(&self, psp_ref: &str) -> Result<TenderResult, PayError>;
    /// THE method that prevents double charges. Called after every timeout,
    /// before any retry, always.
    fn last_transaction_status(&self, sale_ref: &str) -> Result<TenderResult, PayError>;
    fn cancel(&self) -> Result<(), PayError>;
    fn ping(&self) -> Result<TerminalInfo, PayError>;
}

pub enum TenderResult {
    Approved { psp_ref: String, masked_pan: String, scheme: String, amount: Money },
    PartialApproval { psp_ref: String, masked_pan: String, scheme: String, amount: Money },
    Declined { code: String, message: String },
    /// Not a failure. An UNKNOWN OUTCOME. Never treated as declined.
    Unknown { sale_ref: String },
}
```
**Tests:** `trait_object_is_send_sync`

### 2.1.3 — The `Unknown` protocol
**Files:** `apps/terminal/src-tauri/src/payment/flow.rs` (new)
A timeout means the terminal may have taken the money. The only safe response:
```
Unknown  →  poll last_transaction_status(sale_ref), backoff, up to N times
         →  Approved  ⇒ attach the tender, continue
         →  Declined  ⇒ remain in Tendering, tell the truth on screen
         →  still Unknown after N ⇒ manager flow, sale stays in Tendering,
                                    NEVER a blind retry
```
**Tests:** `unknown_triggers_status_query_before_any_retry` · `unknown_never_produces_two_authorizations` · `status_query_approved_attaches_tender` · `prop_no_input_sequence_yields_two_tenders_for_one_auth`
**Done when:** no code path exists in which a second `authorize` is issued after an `Unknown` without a `last_transaction_status` in between — asserted by a state-machine test, not by reading the code.
> **This is the single most important rule in the phase.** A double charge is a chargeback, a refund, an angry customer, and an accounting hole. Everything else here is recoverable.

### 2.1.4 — Simulated terminal with fault injection
**Files:** `crates/pos-hardware/src/payment/simulator.rs` (new)
Scriptable: approve, decline (each code), partial approval, timeout-then-approved, timeout-then-declined, timeout-then-still-unknown, disconnected, slow, reversal failure.
**Tests:** `simulator_covers_every_tender_result_variant`
**Done when:** CI exercises every card path with no hardware attached.

### 2.1.5 — The real driver
**Files:** `crates/pos-hardware/src/payment/<acquirer>.rs` (new)
Implement against the chosen acquirer. Keep it thin: protocol translation only, no business logic. Every decision that matters already lives in 2.1.3.
**Tests:** integration tests behind `#[ignore]`, run by hand against the physical terminal and recorded in the hardware-lab checklist (2.9.4).

---

## Group 2.2 — Card tenders

### 2.2.1 — Card tender in the domain
**Files:** `crates/pos-domain/src/tender.rs`
Card authorisation is always **≤ remaining due**; only cash may overtender. Partial approval leaves a remaining due that flows naturally because split tender is already the core model (Phase 1, group 1.5).
**Tests:** `prop_card_tender_never_exceeds_remaining_due` · `partial_approval_leaves_remaining_due` (E.15)

### 2.2.2 — What is stored from a card
**Files:** `crates/pos-db/src/repo/sale.rs`
`psp_ref`, `masked_pan` (receipt only), `scheme`. **Nothing else. Ever.** No PAN, no track data, no CVV, in the database, in a log, in a Sentry event, or in a crash dump.
**Tests:** `card_tender_persists_only_the_three_allowed_fields` · `full_pan_never_reaches_the_database` — feed a driver response containing a full PAN and assert it is absent everywhere afterwards.

### 2.2.3 — Partial-approval cancellation
**Files:** `apps/terminal/src-tauri/src/payment/flow.rs`
If the customer abandons after a partial approval, the cashier can void it — a PSP reversal. A reversal *failure* escalates to a manager flow with a full audit trail (E.15).
**Tests:** `partial_then_abandon_reverses` · `reversal_failure_escalates_and_audits`

### 2.2.4 — Terminal unavailable
**Files:** `apps/terminal/src/screens/Tender.tsx`
Card button disabled with a stated reason; the cash path is unaffected (E.21).
**Tests:** `card_disabled_when_terminal_unreachable_cash_still_works`

### 2.2.5 — Tender screen card states
**Files:** `apps/terminal/src/screens/Tender.tsx`
Waiting for card → Processing → result. **The timeout state is visible**: *"Checking last transaction…"* with no cancel button while the query is in flight. A cashier watching an unexplained spinner presses the button again; a cashier reading what is happening waits.

---

## Group 2.3 — Refunds, returns, voids, exchanges

*The fraud-and-money feature. Most rules here are anti-abuse.*

### 2.3.1 — Migration `0008`
**Files:** `crates/pos-db/migrations/0008_refunds_and_returns.sql`
Per [`ref/schema.md`](ref/schema.md) §0008.

### 2.3.2 — Refundable balances
**Files:** `crates/pos-domain/src/refund.rs` (new)
`refundable_lines` computes, per line, `sold − already_refunded`. Amounts derive from the **original** line prices including their discounts, never from today's catalogue (E.34 — automatically correct because of conventions I-5).
**Tests:** `prop_cumulative_refunds_never_exceed_sold_qty` (E.16) · `prop_partial_refunds_in_any_order_converge` · `refund_uses_original_price_after_a_price_change`
**Done when:** across **any** sequence of partial refunds in any order, cumulative refunded quantity per line never exceeds sold quantity. This is a proptest, not an example test.

### 2.3.3 — Refund tender routing
**Files:** `crates/pos-domain/src/refund.rs`
Cards refund **to the original card** via `refund(psp_ref, amount)`. Cash sales refund cash. Mixed sales refund proportionally or by manager choice within the originals. Cash-for-card requires an explicit capability with a threshold — it is a money-laundering vector (master plan C.5).
**Tests:** `card_sale_refunds_to_original_card` · `cash_for_card_denied_without_capability` · `mixed_sale_refunds_proportionally` · `refund_api_error_offers_store_credit_with_manager_approval` (E.22)

### 2.3.4 — Restock decision per line
**Files:** `crates/pos-domain/src/refund.rs`
Back to stock → `refund_restock`; damaged → `refund_damage`. Two different stock events because they mean two different things to the buyer.
**Tests:** `restock_choice_writes_the_matching_stock_event`

### 2.3.5 — Receiptless returns
**Files:** `crates/pos-domain/src/refund.rs`
Off by default. When enabled: current lowest price, store-credit-only recommended, hard threshold, manager approval, always audited. ID capture is optional and discouraged — PDPL says collect the minimum (E.32).
**Tests:** `receiptless_denied_when_disabled` · `receiptless_respects_threshold_and_requires_manager`

### 2.3.6 — Void, and the absence of post-void
**Files:** `crates/pos-domain/src/cart.rs`
`Any → Voided` **pre-completion only**, manager permission, reason, audited, parked carts included. **Post-completion "void" does not exist** — it is a same-day full refund document referencing the original, which also means a JoFotara credit note.
**Tests:** `completed_sale_cannot_be_voided_only_refunded` · `void_of_parked_cart_is_audited`

### 2.3.7 — Exchanges
**Files:** `crates/pos-domain/src/refund.rs`
Return + new sale in one flow, settling only the difference. Under the hood: exactly those two documents, linked through `document_link`. Refundable quantity follows the chain (E.30).
**Tests:** `exchange_creates_two_linked_documents` · `refund_of_an_exchanged_item_follows_the_chain`

### 2.3.8 — Escalation thresholds
**Files:** `crates/pos-domain/src/permissions.rs`, `refund_policy` table
Refund above X, receiptless, cash-for-card — settings, enforced in Rust command handlers, never in the UI.
**Tests:** `refund_above_threshold_requires_manager` · `manager_cannot_self_approve_when_banned` (E.52)

### 2.3.9 — Returns UI (D6)
**Files:** `apps/terminal/src/screens/Returns.tsx`
Find sale (scan receipt barcode / number / card last-4 / customer) → line picker showing refundable quantity → restock choice → refund tender → manager PIN modal when escalated.

### 2.3.10 — Manager approval modal (D7)
**Files:** `apps/terminal/src/components/ApprovalModal.tsx`
Shared pattern: action summary, reason picker, PIN pad. **Logs the approver distinctly from the operator** — that separation is the whole point.

---

## Group 2.4 — Shifts and cash management

### 2.4.1 — Migration `0007`
**Files:** `crates/pos-db/migrations/0007_shifts_and_cash.sql`
Note the partial unique index enforcing one open shift per register.

### 2.4.2 — Shift lifecycle
**Files:** `crates/pos-domain/src/shift.rs` (new)
`open(cashier, float)` → sales attach → `close(counted_by_denomination)`. Sales are impossible without an open shift. App relaunch resumes the open shift.
**Tests:** `sale_without_open_shift_is_refused` · `only_one_shift_open_per_register` · `relaunch_resumes_open_shift`

### 2.4.3 — Cash movements
**Files:** `crates/pos-domain/src/shift.rs`, `crates/pos-db/src/repo/cash.rs` (new)
Paid-in, paid-out, drop, bank deposit — amount, reason code, note, actor, timestamp. All feed expected cash.
**Tests:** `every_movement_kind_affects_expected_cash_correctly`

### 2.4.4 — Expected cash
**Files:** `crates/pos-domain/src/shift.rs`
```
expected = float + cash tenders − cash refunds − cash rounding given away
                 + paid_ins − paid_outs − drops
```
**Tests:** `prop_expected_cash_matches_movement_replay` — replay every movement and tender in random order and land on the same figure.

### 2.4.5 — Blind close
**Files:** `apps/terminal/src/screens/ShiftClose.tsx`
The UI collects the count **before revealing expected**. A cashier who can see the target counts to the target.
**Tests:** `expected_is_not_sent_to_the_ui_before_the_count_is_submitted` — assert on the IPC payload, not on the component.

### 2.4.6 — Over/short and acknowledgement
**Files:** `crates/pos-domain/src/shift.rs`
Computed and stored; beyond a threshold, a manager acknowledgement is required and recorded.
**Tests:** `over_short_computed_and_stored` · `large_variance_requires_manager_ack`

### 2.4.7 — Stale shift
**Files:** `apps/terminal/src-tauri/src/commands/shift.rs`
A shift left open overnight is detected at next open and force-closed by a manager, flagged in reports (E.53).
**Tests:** `stale_shift_detected_and_force_closed_with_flag`

### 2.4.8 — Drawer events
**Files:** `crates/pos-db/src/repo/drawer.rs` (new)
Every open logged with actor and cause, **including no-sale opens** — the classic theft tell (E.35). Drawer jammed or open at close does not block the close; the state is logged and an alert raised (E.50).
**Tests:** `no_sale_open_is_logged_and_counted` · `jammed_drawer_does_not_block_shift_close`

### 2.4.9 — Cash-management screen (D8)
**Files:** `apps/terminal/src/screens/CashManagement.tsx`
Paid in/out, drop, and the denomination count helper.

---

## Group 2.5 — X and Z reports

### 2.5.1 — X report
**Files:** `crates/pos-domain/src/shift.rs`
Read-only, any time, closes nothing.
**Tests:** `x_report_does_not_mutate_anything`

### 2.5.2 — Z report
**Files:** `crates/pos-domain/src/shift.rs`
Immutable, sequentially numbered per register, closes the shift. Totals by tender, by tax rate, by category; **counts of voids, refunds, price overrides and no-sale drawer opens** — the fraud tells; over/short. Stored as a document, reprintable, synced.
**Tests:** `prop_z_totals_equal_sum_of_sales` · `prop_z_number_is_gap_free` · `z_is_immutable_after_generation` · `z_belongs_to_the_shifts_business_date_not_the_wall_clock` (E.7)

### 2.5.3 — Shift-close wizard (D9)
**Files:** `apps/terminal/src/screens/ShiftClose.tsx`
Blind count → over/short reveal → Z preview → print & close.

### 2.5.4 — Today-so-far and health counters
**Files:** `apps/terminal/src/components/StatusStrip.tsx`
Sales so far, uncleared fiscal count, outbox depth. The numbers a manager glances at.

---

## Group 2.6 — Electronic journal

*Master plan J.1 marks this planned for Phase 2; it is a thin UI over facts already stored, and support teams live in it.*

### 2.6.1 — Journal query
**Files:** `crates/pos-db/src/repo/journal.rs` (new)
Searchable log of every document and privileged event: by receipt number, amount, time window, cashier, tender, or card last-4.
**Tests:** `journal_finds_by_every_supported_key`

### 2.6.2 — Journal screen (D12 — new)
**Files:** `apps/terminal/src/screens/Journal.tsx`
Results list → document detail → reprint (DUPLICATE) → start a return from it.
**Done when:** "a customer is at the counter with a receipt from Tuesday" takes under ten seconds.

---

## Group 2.7 — The fiscal pipeline

*Start this first, in parallel with 2.1. Everything here is specified in [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md); this group is the build order.*

> **Corrections C-1, C-2 and C-3 all live in this group.** Read [`ref/plan-validation.md`](ref/plan-validation.md) §1 before writing any of it. In particular: **there is no sandbox**, so the master plan's Phase-2 exit gate is replaced by 2.7.6–2.7.8 and the real ISTD hop moves to Phase 5.

### 2.7.1 — The crate and its code tables
**Files:** `crates/pos-fiscal/` (new), `src/lib.rs`, `src/codes.rs`, `Cargo.toml`
`FiscalProfile { Disabled, JordanJoFotara }`, and `codes.rs` as plain mapping tables — invoice types, categories, payment methods, tax categories, units. Isolated deliberately: when Phase 5 diffs the official ISTD spec against this reconstruction, the corrections land here and nowhere else.

### 2.7.2 — The UBL 2.1 builder
**Files:** `crates/pos-fiscal/src/builder.rs`, `src/model.rs`
Built from the **persisted** sale rows, never recomputed. Build order in [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §4.2.
**Tests:** `training_sale_produces_no_document` · `disabled_profile_produces_no_queue_row` · `buyer_block_omitted_below_10000_jod` · `builder_reads_only_persisted_rows` (assert no catalogue access)

### 2.7.3 — Discount conversion and the totals self-check
**Files:** `crates/pos-fiscal/src/builder.rs`, `src/totals.rs`
Corrections **C-2** and **C-3**: absolute → percentage → re-derive → assert; then the high-precision totals recomputation with a < 0.001 JOD tolerance.
**Tests:** `prop_discount_percentage_roundtrip_is_exact` · `drift_beyond_tolerance_is_dead_lettered_not_submitted` · `long_invoice_with_many_discounts_stays_within_tolerance`
**Done when:** a document that would drift is caught **locally, before submission**, with the offending line named.

### 2.7.4 — Migration `0009` and the queue
**Files:** `crates/pos-db/migrations/0009_fiscal.sql`, `crates/pos-fiscal/src/queue.rs`
Durable queue, backoff with jitter, dead letters, `depends_on` for credit-note ordering. The queue row is written **in the same transaction as the sale** — the drain loop is a background task and never sits in the checkout path.
**Tests:** `queue_row_written_in_sale_transaction` · `prop_credit_note_never_precedes_its_invoice` (E.26) · `backoff_has_jitter` · `dead_after_max_attempts_alerts`

### 2.7.5 — The clearance client
**Files:** `crates/pos-fiscal/src/client.rs`
The `ClearanceClient` trait, an HTTP implementation with `Client-Id` / `Secret-Key` headers and base64-in-JSON envelope, credentials read from the OS keyring — never a file, never the database.
**Tests:** `credentials_never_logged` · `envelope_is_base64_of_the_xml`

### 2.7.6 — The conformance harness
**Files:** `crates/pos-fiscal/src/conformance.rs`
All 22 rules from [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §6.1. Rules depending on official code lists are marked `provisional` in the report, so a green harness is never mistaken for certification.
**Tests:** `all_rules_run_on_every_golden` · `provisional_rules_are_reported_as_provisional`

### 2.7.7 — The mock ISTD server
**Files:** `crates/pos-fiscal/tests/mock_istd.rs`
Every fault from §6.2, header-driven.
**Tests:** one per fault row; plus `prop_no_fault_sequence_produces_two_fiscal_results`

### 2.7.8 — The five golden documents
**Files:** `crates/pos-fiscal/tests/golden/`
Plain · discounted · multi-rate · weighed · credit note. Byte-stable, reviewed on every change. **These replace the master plan's four-sandbox-document gate.**
**Tests:** `golden_documents_are_byte_stable`

### 2.7.9 — QR persistence and rendering
**Files:** `crates/pos-fiscal/src/qr.rs`, `crates/pos-hardware/src/render/`
The ISTD QR payload persists onto the sale and rasterises into the receipt. **A reprint days later produces the identical QR** (E.46, E.47).
**Tests:** `reprint_renders_identical_qr_bytes`

### 2.7.10 — Queue chaos
**Files:** `crates/pos-fiscal/tests/queue_chaos.rs`
Crash mid-submit, duplicate submission, reordered responses, restart with a full queue.
**Tests:** `prop_queue_converges_under_crash_and_duplication` · `restart_with_full_queue_drains_in_icv_order`

### 2.7.11 — Health metrics and the pending badge
**Files:** `apps/terminal/src/components/StatusStrip.tsx`, `apps/terminal/src-tauri/src/health.rs`
`uncleared_count`, `oldest_uncleared_age`, `dead_letter_count`, `rejection_rate_24h`. The badge explains itself on tap. **It must never silently grow.**

### 2.7.12 — Environment guard
**Files:** `crates/pos-fiscal/src/lib.rs`
Hard config check at startup: mock credentials in a production build refuse to start, and vice versa. A mismatched TIN in a response is an alarm (E.28).
**Tests:** `production_build_refuses_mock_credentials` · `tin_mismatch_in_response_alarms`

---

## Group 2.8 — Diagnostics

### 2.8.1 — Diagnostics screen (D10, extended)
**Files:** `apps/terminal/src/screens/Diagnostics.tsx`
Test print · drawer kick · scanner echo · **terminal ping** · printer status · fiscal queue state and last error · database health and backup age · clock skew · disk space.
**Done when:** the blueprint's pre-release hardware-lab checklist is a screen a support person can be walked through by phone.

### 2.8.2 — Structured tracing fields
**Files:** `apps/terminal/src-tauri/src/telemetry.rs`
`register_id`, `store_id`, `sale_id`, `shift_id` on every span. Never a customer id, never a PIN, never a PAN. The Phase-1 scrubber (1.6.8) already enforces this; this step is about having fields worth scrubbing.

---

## Group 2.9 — Gate and drills

### 2.9.1 — Card reconciliation drill
Run a scripted trading day against the simulator: approvals, declines, one partial, two timeouts (one resolving approved, one declined), one reversal. Export the tender summary and reconcile against the simulator's own ledger by `psp_ref`.
**Done when:** it matches to the fil, and every unmatched entry on either side is listed separately (E.23).

### 2.9.2 — Blind-Z drill
A scripted day including a drop, a paid-out, a paid-in, a cash refund, and a rounded cash tender. Count blind, close, compare.
**Done when:** over/short is zero, and each deliberately introduced error produces exactly the expected variance.

### 2.9.3 — Cold-start budget
**Files:** `apps/terminal/tests/e2e/coldstart.spec.ts`
**Done when:** packaged app, cold start to sellable, under 3 seconds, measured and failing CI on regression.

### 2.9.4 — Hardware-lab checklist
**Files:** `docs/implementation/ref/hardware-and-receipts.md` (checklist section)
One real thermal printer, one scanner, one payment terminal. Run diagnostics; print all six goldens on paper; confirm the Arabic **by eye**.
**Done when:** signed off and dated. A golden file proves bytes; only paper proves a receipt.

---

## Exit gate

```bash
just lint && just test
cargo nextest run -p pos-fiscal              # 22 rules × 5 goldens, all mock faults
cargo nextest run --workspace -E 'test(prop_)'
```

By demonstration:

1. **Card sale, split with cash.** Card charged the exact unrounded amount; cash rounded; totals exact.
2. **Timeout injected mid-authorisation.** The UI shows *"Checking last transaction…"*; the status query resolves; exactly one tender exists. Then the same with the terminal returning approved-after-timeout, and again with declined-after-timeout.
3. **Partial approval**, remaining paid in cash. Then repeat and abandon: the partial reverses.
4. **Receipted return** of two of three units. Attempt to return two more — refused by invariant, not by a UI check.
5. **Refund to the original card** via `psp_ref`. Then attempt cash-for-card without the capability — refused.
6. **Exchange**: return one item, buy another, settle the difference. Two linked documents.
7. **Shift**: open with a float, sell, drop, pay out, pay in, close blind. Z prints and balances.
8. **Fiscal**: five golden documents pass 22 rules; every mock fault handled; restart with a full queue drains in ICV order with no gaps and no duplicates.
9. **Fiscal reject**: the mock returns a validation error; the dead letter carries the verbatim message; the local sale is untouched; the receipt is still reprintable.
10. **Credit note for an uncleared invoice** waits until the invoice clears, then submits.
11. **Journal**: find Tuesday's receipt by card last-4 in under ten seconds; reprint it marked DUPLICATE with the identical QR.
12. **Automated tests exist** for E.1, E.2, E.14, E.15, E.20, E.21, E.22, E.23, E.24, E.25, E.26, E.27, E.28, E.30, E.31, E.32, E.34, E.35, E.46, E.47, E.50, E.52, E.53.

**Not claimed at this gate:** that ISTD accepts anything. That claim requires Phase 5 milestone 5.2 and nothing else can produce it. Say so out loud to anyone who asks — including the merchant.

→ **Next:** [`phase-3-connected.md`](phase-3-connected.md)
