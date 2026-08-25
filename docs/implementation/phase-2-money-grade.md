# Phase 2 — Money-grade

> **Exit:** the register takes cards that reconcile to the fil, handles returns without being defrauded, closes a shift that balances, and produces fiscal documents that pass every check short of the ISTD network.

**Effort:** 10–13 weeks.
**Scope from the master plan:** C.4 card, C.5 returns/refunds/voids, C.6 shifts & cash, C.11 fiscal pipeline, C.12 X/Z reports, D screens 6–9, migrations **0008–0010**, diagnostics.
**Plus:** microstep **2.7.0** — obtain and pin the official ISTD specification, a precondition of everything fiscal; the minimum store-credit instrument Phase 2's own refund policy already depends on; and the packaged-application smoke suite the blueprint asked for and the implementation set dropped.

This phase is where the product stops being a demo. Every feature in it exists because money moves in a direction someone can exploit, or because a device lies about what happened.

---

## Group dependency graph

```
2.7.0 pin the ISTD specification ──→ 2.7 fiscal pipeline ──────────────────────────────┐
   (do this first; it gates every other 2.7.x step)                                    │
                                                                                       │
2.1 payment terminal abstraction ──→ 2.2 card tenders ──┐                              │
                                                         ├──→ 2.3 refunds & returns ──┐│
2.4 shifts & cash movements ─────────────────────────────┤                            ││
                                                         └──→ 2.5 X / Z reports ──────┤│
2.6 electronic journal ────────────────────────────────────────────────────────────── ┤│
2.8 diagnostics ──────────────────────────────────────────────────────────────────────┤│
                                                                                       ▼
                                                                            2.9 gate & drills
```

**Start 2.7.0 on day one.** It is a paperwork step with an external dependency — obtaining a
government package — and everything else in group 2.7 waits behind it. Nothing about `codes.rs`, the
builder, the conformance rules or the goldens may be frozen until it lands. The rest of group 2.7
still has no dependency on the card work, so run it in parallel with 2.1–2.2 once 2.7.0 is done.

### The schema lane

Migrations are forward-only and never edited once committed, so their **file order is a hard
dependency that cuts across the group branches**. Per [`ref/schema.md`](ref/schema.md), which is
authoritative for every migration number, Phase 2 authors three:

| File | Written by | Must be on `development` before |
|---|---|---|
| `0008_shifts_and_cash.sql` | **2.4.1** | 2.3.1 branches |
| `0009_refunds_and_returns.sql` | **2.3.1** | 2.7.4 branches |
| `0010_fiscal.sql` | **2.7.4** | Phase 3 |

So the *behavioural* work of group 2.3 may run whenever, but its **migration** lands after 2.4.1's.
Cut the group-2.4 branch first, merge it, then branch group 2.3. Authoring `0009` before `0008`
exists produces a number collision that a compensating migration cannot fully undo, and
`just verify-schema` rejects the gap on the spot.

---

## Group 2.1 — The payment terminal abstraction

*Blueprint §6. The trait is the insulation; PSP availability in Jordan is not known until you ask.*

### 2.1.1 — Choose the acquirer, before writing driver code
**Files:** `docs/implementation/ref/merchant-decisions.md` (record the answer)
Provider availability and terminal certification vary sharply by country. Evaluate what actually operates in Jordan — bank-provided terminals, regional PSPs, Network International — before a line of integration code.

For each candidate, get in writing: the integration protocol (JSON over local network / cloud API / serial), whether a **last-transaction-status query** exists (non-negotiable — §2.1.3), partial-approval support, refund-by-original-reference support, **the terminal's exact model and firmware version with its PCI PTS and P2PE listing numbers if it has them**, the acquirer's written responsibility matrix, and the store network topology the terminal requires.

> That last group is §4 of [`ref/plan-validation.md`](ref/plan-validation.md): "semi-integrated" and "P2PE-validated" are different properties. Without a listing number the merchant is on SAQ B-IP or C, not SAQ P2PE — and **the engineering does change with the answer**, which the first revision of that section denied. SAQ B-IP carries eligibility and network-isolation requirements; SAQ C pulls the store network, configuration, patching, access control, monitoring, testing and policy evidence into scope. Plan to the more demanding baseline until a QSA says otherwise, and never promise an SAQ before it is determined.

**Wallet and QR acceptance is inside this decision, not beside it.** For v1 a CliQ or wallet QR is accepted **only** through a bank or a CBJ-licensed merchant acquirer — the same terminal, the same driver trait. A vendor-operated direct funds or acceptance path stays blocked until the Central Bank gives a written classification and every required licence is held, because whether that path needs licensing depends on its funds flow and no trait declares that.

**Record, with the answer:** the acquirer's legal entity, its authorisation or licence, and a funds-flow diagram from card tap to merchant settlement. A driver written before that diagram exists is a driver written twice.

**Done when:** one acquirer chosen, protocol documentation in hand, the responsibility matrix and listing numbers recorded in [`ref/merchant-decisions.md`](ref/merchant-decisions.md) section K, and a physical test terminal on the desk.

> ⚠️ **OPEN — blocks 2.1.1.** Which exact PCI SAQ applies to the selected acquirer, terminal model and firmware, PTS/P2PE listing, integration protocol, store network and support model? Default until answered: design and operate to the SAQ C baseline, reject any integration that exposes a full PAN to this process, and make no P2PE-eligibility claim anywhere.
> Owner: `2.1.1` collects the evidence; `5.3.3` determines the SAQ. Source that settles it: the acquirer's written responsibility matrix and a QSA determination against the current PCI SSC eligibility criteria.

> **If no acquirer will supply a terminal to a pre-revenue vendor** — a real possibility, and the reason [`00-master-plan.md`](00-master-plan.md) §6a orders this conversation in Phase 1 — the stated fallback is: Phase 2 ships against the simulator, microstep 2.1.5 moves to Phase 5 alongside the terminal, and the Phase-2 gate says so out loud. That is a decision, not a discovery at the gate.

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

### 2.2.6 — Wallet and QR tenders, through the acquirer terminal
**Files:** `crates/pos-domain/src/tender.rs`, `apps/terminal/src-tauri/src/payment/flow.rs`
A CliQ or wallet QR presented on the acquirer's terminal is the same `PaymentTerminal` trait and the same `Unknown` discipline (2.1.3) — the driver differs, the protocol does not. What differs is **timing**: confirmation can arrive asynchronously, so a tender may sit `Pending` after the customer has walked away believing they paid.

A `Pending` wallet tender is therefore never silently dropped and never silently completed. It polls `last_transaction_status(sale_ref)` by reference on a bounded schedule, surfaces on the status strip and in device health while it is outstanding, and resolves to exactly one of approved, declined, or a named manager flow. The sale stays in `Tendering` until it resolves; it is not finalized against money that may not have moved (E.65).
**Tests:** `pending_tender_polls_by_reference_before_declaring_unpaid` · `pending_tender_is_never_silently_dropped` · `a_pending_wallet_tender_blocks_finalize_until_it_resolves`
**Done when:** no code path completes a sale carrying an unresolved wallet tender, and no code path discards one — asserted by a state-machine test, as in 2.1.3.
> Case E.65 was named at Phase 2 with no microstep building a wallet tender. Money collected and a tender silently dropped is not a case to leave named-but-unowned; if the merchant's acquirer offers no wallet acceptance, mark 11.4 "no" and this microstep is skipped **explicitly**, not forgotten.

---

## Group 2.3 — Refunds, returns, voids, exchanges

*The fraud-and-money feature. Most rules here are anti-abuse.*

### 2.3.1 — Migration `0009`
**Depends on:** 2.4.1 merged — see "the schema lane" above.
**Files:** `crates/pos-db/migrations/0009_refunds_and_returns.sql`
Per [`ref/schema.md`](ref/schema.md) §0009: immutable refund/exchange links, restock decisions, the refund policy, the defect-resolution facts, and the **minimum stored-value instrument** (2.3.11).
`refund_line_link` and `document_link` are fact tables, so they get their no-`UPDATE`/no-`DELETE` triggers **in this migration** and their rows in `FACT_TABLES` in the same commit. They are the remaining-refundable ledger and the exchange chain; leaving them writable would put the plan's "refunds cannot exceed what was sold" claim on top of a table anyone with the database key can edit.
`refund_line_link.reason_code` carries a `CHECK` matching `ReturnReason`, so a reason nobody handles cannot be stored.

### 2.3.2 — Refundable balances
**Files:** `crates/pos-domain/src/refund.rs` (new)
`refundable_lines` computes, per line, `sold − already_refunded`. Amounts derive from the **original** line prices including their discounts, never from today's catalogue (E.34 — automatically correct because of conventions I-5).

**Allocate from the line total, not from a per-unit price.** A line of 3 at 0.500 carrying a 0.500 line discount has a true per-unit value of 0.333⅓; refunding all three units at a stored per-unit 0.333 returns 0.999 against a line total of 1.000, and destroys a fil in the one direction that matters. `RefundableLine` therefore carries `line_total` and `remaining_value`, and a partial refund takes a proportional share of the remaining value so the **last** partial absorbs the remainder ([`ref/domain-api.md`](ref/domain-api.md) §10).

`build_refund` takes a `ReturnReason { ChangeOfMind, Defective, Damaged, WrongItem }` and a `RequalifyPolicy { DealBreak, ProportionalShare }`, defaulting to `DealBreak`:

- **`Defective` alone bypasses `window_days` under the interim default** below, requires the audited `refund.outside_window` capability, and writes `is_defective_claim = 1`. `Damaged` records the condition of returned goods but follows the store window; it is not evidence that the merchant supplied a defect. `ChangeOfMind` and `WrongItem` also follow the configured window. Without the explicit defect path a customer returning a faulty kettle on day 20 is refused by the domain with no override available to anyone, the owner included, and the shop's workaround is a receiptless return that leaves the controls entirely.
- **`DealBreak` reprices what the customer keeps.** Returning one unit of a "3 for 1.000" group leaves two units, which no longer qualify; the refund is the group price minus the un-promoted price of the kept quantity. Refunding the discounted per-unit share instead lets the customer keep two units for 0.667 when the shelf price for two is 1.000 — a per-abuse leak that scales with the depth of the offer. The promotion engine is Phase 4; the **shape** lands here, before `refund.rs` exists, because retrofitting it means rewriting shipped refund documents.

**Tests:** `prop_cumulative_refunds_never_exceed_sold_qty` (E.16) · `prop_partial_refunds_in_any_order_converge` · `prop_refunding_every_unit_returns_the_line_total_exactly` (E.75) · `prop_partial_refunds_sum_to_the_line_total` · `refund_uses_original_price_after_a_price_change` · `prop_refund_uses_original_rate` (E.34) · `defective_claim_bypasses_the_window_with_manager_approval` (E.82) · `change_of_mind_outside_the_window_is_still_refused` · `a_defective_refund_records_the_reason_code` · `partial_return_of_a_multibuy_reprices_the_remainder` (E.74) · `prop_refund_never_leaves_the_customer_better_off_than_not_buying`
**Done when:** across **any** sequence of partial refunds in any order, cumulative refunded quantity per line never exceeds sold quantity **and** refunding every unit returns the line total exactly. Both are proptests, not example tests.

> ⚠️ **OPEN — blocks 2.3.2.** For how long, and on what terms, must a defective-goods refund be honoured in Jordan, and may repair or replacement be offered instead of a refund? Default until answered: `ReturnReason::Defective` is not time-barred by `window_days`, refund-to-original-value is offered on request, and repair or replacement is recorded only where the customer chose it.
> Owner: 2.3.2. Source that settles it: Consumer Protection Law No. 7 of 2017 as read by Jordanian counsel, recorded in [`ref/merchant-decisions.md`](ref/merchant-decisions.md).

### 2.3.3 — Refund tender routing
**Files:** `crates/pos-domain/src/refund.rs`
Cards refund **to the original card** via `refund(psp_ref, amount)`. Cash sales refund cash. Mixed sales refund proportionally or by manager choice within the originals. Cash-for-card requires an explicit capability with a threshold — it is a money-laundering vector (master plan C.5).

**A cash payout rounds to the coin step.** Cash rounding was scoped to *collection*, so a refund derived from line values could be 1.247 JOD — an amount nobody can hand over when the smallest circulating coin is 10 fils. The payout rounds to the same step in the customer's favour by default, and the difference is recorded as `rounding_adj_minor` on the refund document so the drawer still reconciles ([`ref/tax-jordan.md`](ref/tax-jordan.md) §5). A few fils each time, never diagnosed, is a permanent low-level shortage that pollutes the over/short signal — which is the control the shortage was supposed to reveal.

**Tests:** `card_sale_refunds_to_original_card` · `cash_for_card_denied_without_capability` · `mixed_sale_refunds_proportionally` · `refund_api_error_offers_store_credit_with_manager_approval` (E.22) · `cash_refund_is_rounded_to_the_coin_step` (E.73) · `prop_cash_refund_is_payable_in_circulating_coin` · `prop_refund_rounding_keeps_expected_cash_exact`

> ⚠️ **OPEN — blocks 2.3.3.** What payout direction, customer disclosure, and tax/fiscal treatment apply when a cash refund is not divisible by the configured coin step? Default until answered: round the cash payout in the customer's favour, persist and print the signed refund adjustment, and never alter the credited line or tax facts.
> Owner: 2.3.3. Source that settles it: current ISTD cash/credit-note rules plus written Jordanian consumer and tax counsel advice for the merchant's refund policy.

### 2.3.4 — Restock decision per line
**Files:** `crates/pos-domain/src/refund.rs`
Back to stock → `refund_restock`; damaged → `refund_damage`. Two different stock events because they mean two different things to the buyer.
**Tests:** `restock_choice_writes_the_matching_stock_event` · `a_refund_preserves_an_unknown_cost_basis`

### 2.3.5 — Receiptless returns
**Depends on:** 2.3.11 — the store-credit instrument, which is the configured **default** outcome here.
**Files:** `crates/pos-domain/src/refund.rs`
Off by default. When enabled: current lowest price, store-credit-only, hard threshold, manager approval, always audited. ID capture is optional and discouraged — PDPL says collect the minimum (E.32).

`refund_policy.receiptless_store_credit_only` defaults to 1 and there was nothing to issue, so an implementer under pressure would have substituted cash — which is precisely the fraud channel store-credit-only exists to close. If the merchant genuinely does not want store credit, `allow_receiptless` stays off; it does not silently become a cash payout.
**Tests:** `receiptless_denied_when_disabled` · `receiptless_respects_threshold_and_requires_manager` · `receiptless_store_credit_only_never_falls_back_to_cash`

### 2.3.6 — Void, and the absence of post-void
**Files:** `crates/pos-domain/src/cart.rs`
`Any → Voided` **pre-completion only**, manager permission, reason, audited, parked carts included. **Post-completion "void" does not exist** — it is a same-day full refund document referencing the original, which also means a JoFotara credit note.
**Tests:** `completed_sale_cannot_be_voided_only_refunded` · `void_of_parked_cart_is_audited`

### 2.3.7 — Exchanges
**Files:** `crates/pos-domain/src/refund.rs`
Return + new sale in one flow, settling only the difference. Under the hood: exactly those two documents, linked through `document_link`. Refundable quantity follows the chain (E.30).

**"Settling only the difference" needs an instrument, and there was none.** Each document must balance on its own — a sale is settled when collected ≥ due, and a refund routes its full amount — so value has to pass from the refund to the sale without cash or a card moving. The `0005` tender catalogue therefore seeds an internal `exchange` type: `opens_drawer = 0`, `allows_change = 0`, `is_cash_counted = 0`, `is_internal = 1`, `refundable_to = 'none'`. The refund document is settled by an `exchange` tender for the offset portion and by ordinary routing for any excess; the new sale is settled by a matching `exchange` tender plus real tenders for the balance. Both documents are written in one transaction: a refund that exists without its replacement sale is a customer at the counter with neither their goods nor their money.

An `exchange` tender is never `is_cash_counted`, or the expected-cash formula in [`ref/domain-api.md`](ref/domain-api.md) §11 breaks again.

**Tests:** `exchange_creates_two_linked_documents` · `refund_of_an_exchanged_item_follows_the_chain` · `prop_exchange_pair_nets_to_the_customer_facing_difference` (E.81) · `exchange_with_a_negative_difference_routes_to_the_original_card` · `an_exchange_tender_is_never_cash_counted` · `prop_internal_tenders_never_reach_expected_cash`

### 2.3.8 — Escalation thresholds
**Files:** `crates/pos-domain/src/permissions.rs`, `refund_policy` table
Refund above X, receiptless, cash-for-card — settings, enforced in Rust command handlers, never in the UI. Each escalation is bound to its operation by an `ApprovalHandle` (Phase 1, group 1.6): one use, one capability, one entity, one amount, consumed in the same transaction as the refund and its audit row. The self-approval ban itself is Phase 1's, and this microstep only supplies the thresholds it fires on.

**The controls here are all per-transaction, and the fraud pattern is not.** Many small refunds, each below the threshold, is what the refunds-by-user report exists to catch — and that report is Phase 4, two phases after refunds ship. So `refund_policy` also carries a **per-cashier, per-shift cumulative refund cap**, defaulting to 50.000 JOD, and the running total prints on the Z beside the refund count that is already there.
**Tests:** `refund_above_threshold_requires_manager` · `a_second_use_of_an_approval_handle_is_refused` · `cumulative_refunds_past_the_shift_cap_require_a_manager` · `the_z_report_carries_the_shifts_refund_total_per_cashier`

### 2.3.9 — Returns UI (D6)
**Files:** `apps/terminal/src/screens/Returns.tsx`
Find sale (scan receipt barcode / number / card last-4 / customer) → line picker showing refundable quantity → restock choice → refund tender → manager PIN modal when escalated.

### 2.3.10 — Manager approval modal — refund and cash flows (D7)
**Files:** `apps/terminal/src/components/ApprovalModal.tsx`
The shared escalation pattern itself lands in Phase 1, because Phase 1 already has escalatable actions and an `ApprovalHandle` to bind them to. This microstep wires the refund, receiptless, cash-for-card, force-close and no-sale flows into it: action summary, reason picker, PIN pad, **the exact amount and entity the handle will be bound to**, shown before the approver types anything.
**Logs the approver distinctly from the operator** — that separation is the whole point, and Phase 1's self-approval case is what tests it.
**Tests:** `the_modal_shows_the_amount_the_handle_binds` · `an_amount_changed_after_approval_invalidates_the_handle`

### 2.3.11 — The minimum store-credit instrument
**Files:** `crates/pos-domain/src/stored_value.rs` (new), `crates/pos-db/src/repo/stored_value.rs` (new)
Phase 2 depends on store credit twice — as the fallback when a PSP refund errors (E.22) and as the *default* outcome of a receiptless return, since `refund_policy.receiptless_store_credit_only` defaults to 1 — and no phase built it. Three documents put it in three different phases and no microstep created it, so an implementer under time pressure substitutes cash, which is precisely the fraud channel store-credit-only exists to close.

The **minimum** is issue, redeem, and balance, against `stored_value_instrument` and the append-only `stored_value_ledger` in [`ref/schema.md`](ref/schema.md) §0009. Balance = Σ of the ledger, exactly like stock and loyalty. Gift cards, top-ups and expiry stay in Phase 4; the instrument does not.

Redemption is **online-authorise-only by default**: an unbacked balance spent twice offline is money owed to a customer with no record of it. When the register is offline, redemption is refused with a named reason rather than approximated.
**Tests:** `issued_store_credit_appears_as_a_balance` · `prop_balance_equals_stored_value_ledger_sum` · `redeem_beyond_balance_is_refused` · `stored_value_is_online_authorize_only_by_default` (E.61) · `an_offline_redemption_is_refused_with_a_named_error` · `two_offline_registers_cannot_both_spend_the_same_balance` (E.62)
**Done when:** `refund_api_error_offers_store_credit_with_manager_approval` and `receiptless_respects_threshold_and_requires_manager` both run against a real instrument rather than a placeholder.
> This microstep is what gives cases 61 and 62 an owner. Case 62's named test used to be `prop_two_offline_registers_earning_converge`, which is a *loyalty* property: it shows an append-only ledger converges, not that a balance cannot be spent twice. Both rows move to Phase 2, and Phase 4 re-proves them for gift cards.

### 2.3.12 — Store credit as a tender
**Files:** `crates/pos-domain/src/tender.rs`, `apps/terminal/src/screens/Tender.tsx`
The `store_credit` tender type — `is_cash_counted = 0`, `is_internal = 1` — redeems against a balance at Pay. A liability report arrives with the rest of the report suite in Phase 4 (4.7.6); the **liability itself** starts accruing here, which is why the ledger is append-only from the first issue.
**Tests:** `store_credit_tender_reduces_the_balance_by_exactly_what_it_settled` · `store_credit_tender_never_reaches_expected_cash` · `an_over_redemption_leaves_a_remaining_due`

---

## Group 2.4 — Shifts and cash management

### 2.4.1 — Migration `0008`
**Files:** `crates/pos-db/migrations/0008_shifts_and_cash.sql`
Per [`ref/schema.md`](ref/schema.md) §0008. **This is the first migration of Phase 2** — see "the schema lane" above.

The `shift` skeleton itself — open, the opening float, one-open-per-register — is already in `0005`, because `Cart.shift_id` is not optional and conventions §11 defines the business date as the business date of its shift. What `0008` adds is the rest of cash accountability: `shift_count_line` and `cash_count` for the blind count, `cash_location` and the two-ended `cash_movement`, `drawer_event`, and `z_report`.

`cash_location(store_id, kind IN ('drawer','safe','bank_in_transit'))` is the change that makes the arithmetic in 2.4.4 expressible at all, and `cash_movement` carries `from_location_id` / `to_location_id` with a nullable `shift_id`, so a safe-to-bank deposit needs no drawer to be recorded against.

`shift` closes through the append-only `shift_close_event`, not through an `UPDATE`: the server revokes `UPDATE` on fact tables, so a shift that closed by mutation would be rejected centrally or force fact immutability to be relaxed ad hoc. `shift_state` is the rebuildable projection that carries the one-open index.

### 2.4.2 — Shift lifecycle
**Files:** `crates/pos-domain/src/shift.rs` (new)
Phase 1 has `open(cashier, float)`, ordinary own-shift close and `shift_current`. This microstep adds `close(counted_by_denomination)` as a `shift_close_event`, and the rule that sales are impossible without an open shift. App relaunch resumes the open shift from `shift_state`. `shift_force_close_stale { shift_id, reason, approval_id }` is the only path that closes another user's shift; it always consumes an approval bound to that shift, exact zero and the reason, so force-close cannot become an undocumented self-approval exception.
**Tests:** `sale_without_open_shift_is_refused` · `only_one_shift_open_per_register` · `relaunch_resumes_open_shift` · `user_switch_inside_an_open_shift_is_refused_when_the_policy_forbids_it` (E.76) · `a_closed_shifts_count_cannot_be_edited` — the close event and its counts are facts; a variance edited after the Z is a variance nobody can trust · `stale_shift_force_close_requires_a_different_approver`

### 2.4.3 — Cash movements between locations
**Files:** `crates/pos-domain/src/shift.rs`, `crates/pos-db/src/repo/cash.rs` (new)
**A movement is a transfer between two places, not a signed number against one drawer.** Paid-in, paid-out, drop, bank deposit and float top-up each declare a `from_location_id` and a `to_location_id` — plus amount, reason code, note, actor and timestamp. A drop reduces the drawer and increases the safe; a bank deposit leaves the safe and touches no drawer at all.

That is what the old model could not say. Every movement was `shift_id NOT NULL`, so a `bank_deposit` had to be recorded against a drawer the money never left, creating a phantom shortage in that shift — and the largest single-event cash risk in a small shop, drop → safe → bank, was broken at its first link. Where the money then *is*, and whether the bank got Thursday's deposit, is the safe and bank-in-transit balances, which 2.4.10 counts and 4.7.8 reconciles.
**Tests:** `every_movement_kind_declares_its_location_pair` — an exhaustive match over the movement-kind list, so a sixth kind cannot ship without saying where the money came from and where it went · `a_safe_to_bank_movement_does_not_change_expected_drawer_cash` (E.77) · `a_drop_moves_the_same_amount_out_of_the_drawer_and_into_the_safe`

### 2.4.4 — Expected cash
**Files:** `crates/pos-domain/src/shift.rs`
**The formula is normative in [`ref/domain-api.md`](ref/domain-api.md) §11.** Implement it from there, not from a copy: master-plan C.6 and the earlier text of this microstep both carried a version that was wrong in three independent ways — change given out was never subtracted, two of the five movement kinds had no term, and the cash-rounding term double-counted under the storage convention the rest of the plan uses. [`00-master-plan.md`](00-master-plan.md) §4a records the erratum.

A false variance is worse than no variance: over/short becomes noise, the cashier learns to ignore it, and the second-ranked control in the whole threat model is dead from the first close.
**Tests:** `prop_expected_cash_matches_movement_replay` — replay every movement and tender in random order and land on the same figure · `prop_expected_cash_equals_physical_drawer_replay` — simulate the coins in and out and assert the formula reproduces the count, which the replay property alone cannot, because a replay of a wrong formula is order-independently wrong · `every_movement_kind_has_a_term_in_expected_cash` (E.77) · `paid_in_from_safe_adjusts_expected_cash` (E.17) · `change_given_leaves_the_drawer` · `a_bank_deposit_from_the_safe_does_not_move_the_drawer` · `a_float_add_and_a_paid_in_are_not_the_same_movement` · `a_cash_rounded_sale_reconciles_without_a_rounding_term`
**Done when:** a scripted day containing a drop, a bank deposit taken from the safe, a paid-in, a cash refund and a rounded cash tender closes at exactly zero variance.

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
Every **software-commanded** open logged with actor and cause, **including no-sale opens** — the classic theft tell (E.35). Past the configured count in a shift, a no-sale open requires a manager reason. Drawer jammed or open at close does not block the close; the state is logged and an alert raised (E.50).

**"Every drawer open is logged" is not true and must not be claimed.** The default interface is a one-way ESC/POS kick, so the register can record what it *commanded* and cannot observe the physical drawer at all: a manual key, a wedged latch, or an exact-cash sale rung on no system leaves the no-sale count clean. Where the hardware offers a drawer-state sensor, observed transitions are recorded too and an unexplained one appears in the report. Where it does not, the residual risk is accepted, disclosed, and controlled by procedure — key custody, tamper seals, and cash counts — rather than by a sentence in a threat model.
**Tests:** `no_sale_open_is_logged_and_counted` · `no_sale_past_the_threshold_requires_a_manager_reason` · `jammed_drawer_does_not_block_shift_close` · `an_observed_drawer_transition_without_a_command_is_reported` (where a sensor exists)

### 2.4.9 — Cash-management screen (D8)
**Files:** `apps/terminal/src/screens/CashManagement.tsx`
Paid in/out, drop, bank deposit, and the denomination count helper. Every movement picks its **from** and **to** location, because that is what the domain takes.

### 2.4.10 — The safe count
**Files:** `apps/terminal/src/screens/CashManagement.tsx`, `crates/pos-domain/src/shift.rs`
A blind count of the safe, the same shape as `shift_count_line`, so the safe has an expected figure and a counted one. Without it a manager who takes a 300 JOD drop and never banks it produces no variance anywhere: the shift balances because the drop was recorded, and the safe has nothing to reconcile against. That is worse than not recording drops, because the drop record creates the appearance of control.
**Tests:** `a_safe_count_produces_an_over_short_against_the_safe_balance` · `a_carried_float_is_declared_once_and_reconciles_across_both_shifts` (E.79)

---

## Group 2.5 — X and Z reports

### 2.5.1 — X report
**Files:** `crates/pos-domain/src/shift.rs`
Read-only, any time, closes nothing — and it runs on its own `xreport.run` capability, **not** on `zreport.run`.

**The X report is inside the blind-close guarantee, not beside it.** Totals by tender plus the opening float *is* the expected figure. On `zreport.run` — held by shift lead and manager, the same two roles that close shifts, and in a small store the shift lead is the person counting their own drawer — the wire-level guarantee was airtight against a cashier and wide open to exactly the people it was written about. A skimming shift lead runs an X two minutes before their count, removes cash, counts to the figure, and shows a clean over/short trend forever.

So `report_x` **omits the cash-tender total and the expected figure entirely** for a caller who holds `shift.close` on the currently open shift ([`ref/ipc-contract.md`](ref/ipc-contract.md) §3).
**Tests:** `x_report_does_not_mutate_anything` · `x_report_does_not_reveal_expected_cash_to_the_closing_user` (E.84)

### 2.5.2 — Z report
**Files:** `crates/pos-domain/src/shift.rs`
Immutable, sequentially numbered per register, closes the shift through the `shift_close_event`. Totals by tender, by tax rate, by category; **counts of voids, refunds, price overrides and no-sale drawer opens** — the fraud tells — plus the **refund total per cashier** (2.3.8), the training-transaction count, and over/short. Stored as a document, reprintable, synced.

The Z also **anchors the audit chain head** for the shift, so the chain has a fixed point that a later tail deletion cannot move (1.6.6b, 3.2.4).
**Tests:** `prop_z_totals_equal_sum_of_sales` · `prop_z_number_is_gap_free` · `z_is_immutable_after_generation` · `z_belongs_to_the_shifts_business_date_not_the_wall_clock` (E.7) · `z_report_counts_no_sale_opens` (E.35) · `a_z_close_anchors_the_head` (E.91)

### 2.5.3 — Shift-close wizard (D9)
**Files:** `apps/terminal/src/screens/ShiftClose.tsx`
Blind count → over/short reveal → Z preview → print & close.

### 2.5.4 — Day so far, and the health counters
**Files:** `apps/terminal/src/components/StatusStrip.tsx`, `apps/terminal/src/screens/DaySoFar.tsx` (screen 18)
The status strip carries sales so far, uncleared fiscal count, unallocated ICV count and outbox depth — the numbers a manager glances at.

Behind it, **takings by hour, by tender and by cashier**, on `reports.own`. *"Print me today's takings — by hour, and by cashier"* is asked by every owner in week one; it is a query over facts that already exist, and without it a merchant trading on Phases 1–3 has no daily summary at all. It is not the X report and does not reveal expected cash: 2.5.1 governs that.

**"By cashier" means by shift and its opener** unless the merchant forbids a shared drawer (decision 8.6). Over/short is a shift-level fact and cannot be apportioned between two people who shared the till, so a column that says otherwise will discipline the wrong person.
**Tests:** `day_so_far_matches_the_z_for_a_closed_shift` · `day_so_far_never_reveals_expected_cash` · `over_short_is_attributed_to_the_shift_and_its_opener_not_invented_per_cashier` (E.76)

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

**It runs on `journal.view`, which the cashier holds** — scoped to their own shift unless they also hold `reports.all`. On `reports.all` it was manager-and-owner only, so the acceptance criterion below took however long finding a manager takes, at the counter, with a customer waiting. Reprint is separately `sale.reprint`, because reprinting any document you can name is a customer-data question rather than a selling one.
**Done when:** "a customer is at the counter with a receipt from Tuesday" takes under ten seconds, performed by a cashier.

---

## Group 2.7 — The fiscal pipeline

*Start 2.7.0 on day one. Everything here is specified in [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md); this group is the build order.*

> **Corrections C-1, C-2 and C-3 all live in this group, and two of them were wrong.** Read
> [`00-master-plan.md`](00-master-plan.md) §4a before writing any of it. In short: there is no
> sandbox, so the master plan's Phase-2 exit gate is replaced by 2.7.6–2.7.8 and the credentialed
> hop stays in Phase 5 — but the *specification* is obtainable now, and 2.7.0 gets it first.

### 2.7.0 — Obtain and pin the official ISTD specification
**Files:** `crates/pos-fiscal/spec/manifest.toml` (new), `crates/pos-fiscal/src/spec.rs` (new)
**This is a precondition of every other step in group 2.7.** The plan's premise — that the authoritative material cannot be had until a merchant exists in Phase 5 — is stale: ISTD publicly lists its Technical Integration Guide. Building `codes.rs`, the builder, twenty-two conformance rules and five goldens from a reconstruction and *then* diffing them against the real package is the expensive way round, and it puts the diff in the phase where every mistake is a live invoice on a merchant's tax record.

Obtain the current guide, XSD, business rules and code lists. Record, in a committed manifest: the retrieval date, the package or version identifier, a SHA-256 digest per source artifact, and whether each artifact may be vendored. Then walk every row of [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §3 marked `PROVISIONAL` and either confirm it against the package or replace it — and close, or explicitly carry forward with a restated default, every `⚠️ OPEN` block that names 2.7.0 as its owner.

**Do not assert what the specification says. Assert that it must be read first.** Where the package is silent — ICV scope, the outage procedure, the validator's tolerance, discount percentage precision, credential scope — the answer is a written question to the ISTD E-Invoicing Directorate, recorded with its date, not a guess propagated into `codes.rs`.

**Tests:** `every_provisional_row_is_resolved_or_explicitly_carried_forward` · `the_manifest_hash_matches_the_vendored_artifacts` · `no_golden_is_frozen_before_the_manifest_exists`
**Verify:** `cargo nextest run -p pos-fiscal spec::`
**Done when:** the manifest exists with a digest per artifact, `cargo nextest run -p pos-fiscal spec::` passes, and every `2.7.0`-owned `⚠️ OPEN` block in the fiscal and tax references is either closed with a dated source or restated with its default and the question that remains.

> If the package cannot be obtained — the portal requires a registered taxpayer, or the guide is withdrawn — **that is a blocking finding, not a reason to proceed**. Record it, escalate it to the long-lead register in [`00-master-plan.md`](00-master-plan.md) §6a, and keep every provisional row flagged. A frozen golden built on a reconstruction is a golden that will be regenerated in Phase 5 with a merchant watching.

### 2.7.1 — The crate and its code tables
**Depends on:** 2.7.0
**Files:** `crates/pos-fiscal/` (new), `src/lib.rs`, `src/codes.rs`, `Cargo.toml`
`FiscalProfile { Disabled, JordanJoFotara }`, and `codes.rs` as plain mapping tables driven by the pinned manifest — invoice types, taxpayer categories, tax categories, units. Isolated deliberately: a specification correction lands here and nowhere else.

**`InvoiceTypeCode@name` is composed, not looked up.** It is a three-digit composite of document scope, settlement method and fiscal taxpayer type — not a "payment method code", which is what an earlier revision of the contract table called `012` and `022`. `codes.rs` carries the three component tables and `compose_invoice_type_name(scope, settlement, taxpayer_type)`, which **refuses** a combination the pinned code lists do not support rather than emitting a plausible-looking triple. Hard-coding two values misclassifies every merchant profile this plan already supports — income, special-tax, export, development-area, free-zone — into the wrong fiscal category.

`DISCOUNT_PERCENT_DECIMALS` is a single named constant here, so that if the pinned profile requires a percentage element at all, its precision is one line rather than a number scattered through the builder.

**Tests:** `compose_invoice_type_name_covers_every_supported_store_profile` — exhaustive over the profiles the plan supports · `an_unsupported_combination_is_refused_not_approximated` · `code_tables_match_the_pinned_manifest`

### 2.7.2 — The UBL 2.1 builder
**Depends on:** 2.7.0, 2.7.1
**Files:** `crates/pos-fiscal/src/builder.rs`, `src/model.rs`
Built from the **persisted** sale rows, never recomputed. Build order in [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §4.2, including the mandatory headers an XSD-valid document can still omit: the profile identifier, the immutable register-prefixed invoice number in `cbc:ID` — which is distinct from both the fiscal UUID and the ICV — and the buyer block with its scheme token.

`cbc:IssueDate` is `YYYY-MM-DD`. It is `xs:date`, whose lexical form is normative, so the earlier `dd-mm-yyyy` rule guaranteed that every golden failed either the schema check or the format check.

**Tests:** `training_sale_produces_no_document` · `zero_due_tender_completes_and_issues_a_fiscal_doc` (E.18) · `disabled_profile_produces_no_queue_row` · `builder_reads_only_persisted_rows` (assert no catalogue access) · `issue_date_is_iso_and_validates_against_the_xsd` · `the_invoice_number_is_never_the_uuid_or_the_icv` · `buyer_rules_follow_the_pinned_matrix_for_cash_and_receivable`

### 2.7.2b — Buyer invoice details (D20)
**Files:** `apps/terminal/src/components/BuyerDetailsModal.tsx` (new)
Buyer TIN capture had a command, two `sale` columns and a fiscal conformance rule, and no way for a cashier to type one — so the one customer who explicitly asks for something got neither the capture nor the printed line. The modal drives `cart_set_buyer_tin` and `cart_clear_buyer_tin`, validates the TIN shape against the pinned scheme, and shows what will appear on the document.

The **printed** half is the receipt's buyer block, which the receipt model carries from Phase 1 — adding it later means regenerating every receipt golden, which is exactly the cost the golden discipline exists to make deliberate.
**Tests:** `buyer_details_reach_the_persisted_sale_and_the_receipt` · `clearing_the_buyer_removes_it_from_both`

### 2.7.3 — Discounts and the totals pre-submit check
**Depends on:** 2.7.0
**Files:** `crates/pos-fiscal/src/builder.rs`, `src/totals.rs`
This is where corrections **C-2** and **C-3** were, and both were wrong in the same direction: each would have refused arithmetically correct documents. [`00-master-plan.md`](00-master-plan.md) §4a records both errata; what follows is what to build.

**Discounts.** Keep largest-remainder proration to exact line **amounts** in fils (`Money::split_proportional`). Emit the line allowance and a document-level allowance recap equal to the exact sum of the line allowances. An entered percentage is stored as **provenance only** and never gates eligibility: at any fixed precision some absolute discounts are unrepresentable, so a round-trip gate dead-letters a completed, immutable sale that is perfectly correct. If the pinned profile requires a percentage element, emit it at `DISCOUNT_PERCENT_DECIMALS` and derive nothing from it.

**Totals.** The document's tax total is the exact sum of the per-line rounded values (conventions I-1, retained). So an *unrounded* recomputation of the invoice total differs from the carried total by the accumulated per-line error — up to half a fil per line — and comparing the two at a one-fil tolerance rejects an ordinary eight-line basket. The check that cannot do that:

```
per line:      compare the fixed-scale projection with the carried value,
               at a half-fil tolerance
per document:  exact identities over the document's own carried values —
               tax_exclusive_total == Σ carried line nets
               tax_total           == Σ carried line taxes
               tax_inclusive_total == tax_exclusive_total + tax_total
               payable_total       == the exact identity over those values
```

A failure is `QueueState::BuildFailed`, not `Rejected` and not `Dead`: ISTD did not reject it and no retry was exhausted. It gets a `Local, build failed` reconciliation row, is excluded from `dead_letter_count`, and is remediated through the audited operator command `fiscal_rebuild_failed` after the builder or the pinned configuration is corrected — preserving the immutable sale and its `fiscal_uuid`.

**Tests:** `prop_document_allowance_recap_equals_sum_of_line_allowances` · `prop_line_level_drift_never_exceeds_half_a_fil` · `a_twenty_line_basket_is_submitted_not_dead_lettered` · `a_build_failure_becomes_build_failed_and_never_rejected` (E.92) · `build_failed_is_excluded_from_dead_letter_count` · `a_rebuild_preserves_the_uuid_and_any_allocated_icv` · `fiscal_rebuild_failed_requires_bound_approval_and_preserves_identity`
**Done when:** a synthetic twenty-line inclusive-price basket with mixed rates and a basket discount passes the pre-submit check, and a document with a genuinely wrong carried total fails it with the offending line named.

> ⚠️ **OPEN — blocks 2.7.0.** What tolerance, if any, does the current ISTD validator apply to transmitted line and document equations? Default until answered: enforce the half-fil per-line projection check and exact identities over the document's own carried values; do not implement an invoice-level tolerance or claim an ISTD tolerance.
> Owner: 2.7.0. Source that settles it: the pinned official ISTD business rules and Schematron/XSD package, plus credentialed accepted boundary vectors.

### 2.7.4 — Migration `0010` and the queue
**Depends on:** 2.3.1 merged, 2.7.0
**Files:** `crates/pos-db/migrations/0010_fiscal.sql`, `crates/pos-fiscal/src/queue.rs`
Durable queue, backoff with jitter, dead letters, `depends_on` for credit-note ordering. The queue row is written **in the same transaction as the sale** — the drain loop is a background task and never sits in the checkout path.

**`fiscal_queue.icv` is nullable, and ICV is not allocated at checkout.** The requirement is a counter monotonic in the authority-confirmed scope; allocating it from independent register counters would let two registers each allocate `1`. `doc_sequence` is keyed `(scope_kind, scope_id, kind)` with `scope_kind IN ('register','store')`: receipts and Z reports stay register-scoped, while fiscal ICV is store-scoped by default. Phase 2 has no server and supports one register per store, so the submission worker locks that register database's store row in process, allocates **once after preflight at first submission**, and records the allocating register in `allocator_ref`. Idempotency rests on `fiscal_uuid`, generated locally in the sale transaction; the checkout queue row starts with `icv IS NULL`, and selling never waits for the worker. A `Suspect` or `Untrusted` clock also leaves `issue_date IS NULL`; merely reaching ISTD is not a time-authentication event, so allocation and payload freeze wait for the authenticated source required by the open item in API reference §3.2.

**`Sending` is a lease, not a state.** A crash between marking a row `sending` and recording a response left the oldest ICV unreachable forever, and because later ICVs deliberately wait behind it, the whole scope stopped clearing. A claim carries `claimed_at`, `lease_owner` and `lease_expires_at`; startup and every drain cycle reclaim expired claims and resubmit the identical bytes.

**Tests:** `queue_row_written_in_sale_transaction` · `a_sale_completes_with_a_null_icv_and_allocates_on_reconnect` (E.24; checkout commits before the first local drain) · `a_store_scoped_counter_allocates_in_order_on_reconnect` · `prop_icv_is_gap_free_and_strictly_increasing_within_its_scope` · `build_failure_does_not_consume_icv` · `crash_after_claim_reclaims_expired_lease` · `prop_credit_note_never_precedes_its_invoice` (E.26) · `backoff_has_jitter` · `dead_after_max_attempts_alerts` · `single_register_local_allocator_assigns_store_scoped_icv_at_first_submission` · `reaching_the_clearance_endpoint_does_not_make_device_time_trusted` · `a_never_synchronised_register_keeps_issue_date_null_and_sale_complete`
**Fixture:** `single_register_local_allocator_assigns_store_scoped_icv_at_first_submission` — commit the sale and NULL-ICV queue row, start the local drain, and prove the register-scoped allocator reference, monotonic store counter, identical replay identity and no lost sale.

> ⚠️ **OPEN — blocks 2.7.0.** Is the authoritative ICV namespace per register, store/income source, or one TIN across stores? Default until answered: allocate from one store-scoped counter keyed as `('store', store_id, 'fiscal_icv')`; Phase 2 uses the single register's in-process allocator, Phase 3 uses a server-issued one-value lease, and no register advances an independent register-scoped ICV counter.
> Owner: 2.7.0. Source that settles it: the official ISTD business rules or a written ISTD E-Invoicing Directorate ruling.

### 2.7.5 — The clearance client
**Depends on:** 2.7.0
**Files:** `crates/pos-fiscal/src/client.rs`
The `ClearanceClient` trait and an HTTP implementation against **the pinned transport** — headers, envelope and response shape as the manifest records them, not as an earlier reconstruction guessed. Credentials are read through a versioned reference in the OS keyring — never a file, never the database, never a fixture.

**Do not invent a recovery operation.** The accessible official material documents submission and the returned QR; it does not document a fetch-existing endpoint, an HTTP `409` contract, or duplicate-resubmit semantics. An ambiguous timeout preserves the exact request bytes and identity and keeps the row recoverable; the client asserts nothing about what the service will say.
**Tests:** `credentials_never_logged` · `envelope_matches_the_pinned_contract` · `an_ambiguous_timeout_preserves_the_bytes_and_the_identity` · `no_client_path_invents_a_duplicate_lookup`

### 2.7.6 — The conformance harness
**Depends on:** 2.7.0
**Files:** `crates/pos-fiscal/src/conformance.rs`
All 22 rules from [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §6.1, evaluated against the pinned manifest and an explicit conformance case rather than a bare document. Rule `F-001` is a real XSD validation against the vendored schema, not a string-pattern check — a pattern check moves the rejection from CI to ISTD. Any rule the pinned package leaves unresolved is reported `provisional` **by name**, so a green harness is never mistaken for certification and the gate's headline count is not quietly inflated by rules that could not run.
**Tests:** `all_rules_run_on_every_conformance_case` · `provisional_rules_are_reported_as_provisional` · `an_unrunnable_rule_is_reported_not_counted_as_passed`

### 2.7.7 — The mock ISTD server
**Depends on:** 2.7.0
**Files:** `crates/pos-fiscal/tests/mock_istd.rs`
Every fault from §6.2, header-driven — and **only** behaviour the pinned contract documents. The mock does not invent a `409`, a fetch-existing endpoint or duplicate semantics; inventing one turns an assumption into an apparently verified behaviour, and the tests then prove the mock rather than the client.
**Tests:** `rejection_dead_letters_verbatim_and_never_mutates_the_sale` (E.25) · `an_ambiguous_timeout_resends_identical_bytes_under_the_same_uuid` · `duplicate_recovery_follows_the_pinned_procedure` (E.27) · one per remaining fault row · `prop_no_fault_sequence_produces_two_fiscal_results` · `the_mock_implements_no_operation_absent_from_the_manifest`

### 2.7.8 — The five golden documents
**Depends on:** 2.7.0, 2.7.6
**Files:** `crates/pos-fiscal/tests/golden/`
Plain · discounted · multi-rate · weighed · credit note, plus the training-absence case that proves a training sale produces nothing. Byte-stable, reviewed on every change. **These replace the master plan's four-sandbox-document gate** — note *five*, not four; the weighed document is the only fractional-quantity fiscal fixture and the earlier count omitted it.

**No golden may be frozen before 2.7.0 pins the specification.** A golden regenerated against a real XSD after the fact is a golden that never proved anything.

The credit note carries the **original's** immutable buyer block and line identity, price and tax facts, plus the remaining refundable quantity — not today's customer record and today's catalogue. A partial or later refund against a changed catalogue or a changed customer is exactly when a credit note gets rejected, and it is exactly the case a same-day golden does not cover.
**Tests:** `golden_documents_are_byte_stable` · `a_credit_note_carries_the_originals_buyer_and_line_facts` · `a_partial_credit_note_never_exceeds_the_remaining_quantity` · `partial_credit_note_copies_original_facts` · `repeated_credit_note_respects_remaining_qty_milli` · `credit_note_survives_catalog_change` · `credit_note_survives_customer_change`

### 2.7.9 — QR persistence and rendering
**Files:** `crates/pos-fiscal/src/qr.rs`, `crates/pos-hardware/src/render/`
The ISTD QR payload persists as a `fiscal_result` fact and rasterises into the receipt. **A reprint days later produces the identical QR** (E.46). A reprint from *another* register uses the server-owned `reprint_bundle` ([`ref/sync-protocol.md`](ref/sync-protocol.md) §3) and lands in Phase 3 with sync — a QR alone cannot reproduce another register's receipt, because that register's sale never travelled down. That is case 47, and it belongs to Phase 3.
**Tests:** `reprint_is_byte_identical_including_qr` (E.46) · `receipt_prints_without_qr_when_disabled` (E.29)

### 2.7.10 — Queue chaos
**Files:** `crates/pos-fiscal/tests/queue_chaos.rs`
Crash mid-submit, duplicate submission, reordered responses, restart with a full queue, and a crash at each of the five identity boundaries: before identity freeze, before claim, after claim, after the mock commits, and before the local result transaction.
**Tests:** `prop_queue_converges_under_crash_and_duplication` · `restart_with_full_queue_drains_in_icv_order` · `crash_before_claim_leaves_row_queued` · `crash_after_remote_commit_preserves_submission_identity` · `crash_before_result_persist_reconciles_without_new_uuid_or_icv`

### 2.7.11 — Health metrics and the pending badge
**Files:** `apps/terminal/src/components/StatusStrip.tsx`, `apps/terminal/src-tauri/src/health.rs`
`uncleared_count`, `unallocated_icv_count`, `oldest_uncleared_age`, `build_failed_count`, `expired_sending_lease_count`, `dead_letter_count`, `rejection_rate_24h` — as defined in [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §8. `dead_letter_count` never includes `BuildFailed`; a local construction failure is not a regulator rejection and conflating them hides both.

The badge explains itself on tap. **It must never silently grow** — and a badge is not a control, because nobody is looking at it on a Saturday. Delivery to a person is microstep `3.9.3`.

### 2.7.12 — Environment guard
**Files:** `crates/pos-fiscal/src/lib.rs`
Hard config check at startup: mock credentials in a production build refuse to start, and vice versa. A mismatched TIN in a response is an alarm (E.28).
**Tests:** `production_build_refuses_mock_credentials` · `tin_mismatch_in_response_alarms`

---

## Group 2.8 — Diagnostics

### 2.8.1 — Diagnostics screen (D10, extended)
**Files:** `apps/terminal/src/screens/Diagnostics.tsx`
Test print · scanner echo · **terminal ping** · printer status · fiscal queue state and last error · database health and backup age · clock skew · disk space. The drawer action invokes `drawer_open_no_sale { reason, approval_id? }`; there is no diagnostics-only kick command that could omit the conditional approval, drawer event or no-sale count. A `BuildFailed` row exposes `fiscal_rebuild_failed { queue_id, reason, approval_id }` only after the builder or pinned configuration is corrected; the command requires `fiscal.remediate`, a distinct approver and a handle bound to that row.
**Tests:** `diagnostics_drawer_action_uses_drawer_open_no_sale` · `diagnostics_exposes_fiscal_build_failure_remediation`
**Done when:** `pnpm --filter terminal exec vitest run src/screens/Diagnostics.test.tsx` exits zero with the drawer spy observing only `drawer_open_no_sale` and the fiscal remediation action passing the selected `queue_id` to the catalogued command.

### 2.8.2 — Structured tracing fields
**Files:** `apps/terminal/src-tauri/src/telemetry.rs`
`register_id`, `store_id`, `sale_id`, `shift_id` on every span. Never a customer id, never a PIN, never a PAN. The Phase-1 scrubber (1.6.8) already enforces this; this step is about having fields worth scrubbing.

---

## Group 2.9 — Gate and drills

### 2.9.1 — Card reconciliation drill
Run a scripted trading day against the simulator: approvals, declines, one partial, two timeouts (one resolving approved, one declined), one reversal, and one wallet tender whose callback is lost. Export the tender summary and reconcile against the simulator's own ledger by `psp_ref`.
**Tests:** `settlement_report_lists_unmatched_separately_by_direction` (E.23)
**Done when:** it matches to the fil, every unmatched entry on either side is listed separately (E.23), and the run is recorded in `docs/drills/` with its date, operator and elapsed time.

### 2.9.2 — Blind-Z drill
A scripted day including a drop to the safe, a bank deposit taken from the safe while the shift is open, a paid-out, a paid-in, a cash refund, and a rounded cash tender. Count blind, close, compare.
**Done when:** over/short is zero — including the bank deposit, which must move the drawer's expected cash by nothing — and each deliberately introduced error produces exactly the expected variance. Recorded in `docs/drills/`.

### 2.9.3 — Cold-start budget
**Depends on:** 2.9.5
**Files:** `apps/terminal/tests/e2e/coldstart.e2e.ts`
Measured through the packaged-app harness from 2.9.5, on the reference register named in [`01-conventions.md`](01-conventions.md) §7 — not on a hosted runner, whose variance on a three-second workload exceeds the budget.
**Done when:** packaged app, cold start to sellable, median of at least five runs under 3 seconds, and `just bench-gate` exits non-zero on a regression beyond the stated tolerance. `cargo bench` alone exits 0 whatever it measures, which is why the gate is a recipe and not a bench invocation.

### 2.9.4 — Hardware-lab checklist
**Files:** `docs/implementation/ref/hardware-and-receipts.md` (the checklist section), `docs/drills/` (the record)
One real thermal printer at **each** supported width, one scanner, one payment terminal. Run diagnostics; print every receipt golden on paper; confirm the Arabic **by eye**, by a native reader who is not you.
**Done when:** a dated record exists in `docs/drills/` naming the drill, the commit or tag, the hardware, the operator, the elapsed time and every surprise with the case number it became. "Signed off and dated" needs somewhere to be signed, and a normative reference document is not a log. A golden file proves bytes; only paper proves a receipt.

### 2.9.5 — The packaged-application smoke suite
**Files:** `apps/terminal/tests/e2e/` (new), `.github/workflows/ci.yml`
**Nothing automated has ever executed the artefact a merchant actually runs.** CI builds the Tauri bundle on three platforms and never launches the result; `ipc_contract.rs` walks the handler registry and invokes no command; and the existing `.spec.ts` naming implies Playwright, which drives the browser engines it bundles and cannot attach to a WebView2, WKWebView or WebKitGTK window. So the desktop shell — IPC dispatch, the capability file, the CSP, credential-store access on three different stores, window setup, the recovery screen — is defended by a human clicking, at release time, on whichever OS that human owns.

WebdriverIO plus `tauri-driver`, three scenarios: launch → the lock screen renders in Arabic; PIN unlock → the sale screen; one cash sale to a printed simulator receipt. This microstep adds the suite to CI's `cross-platform` job on the platforms `tauri-driver` supports; the current workflow has no WebDriver lane. **Any platform the implemented driver does not support is named in [`02-development-workflow.md`](02-development-workflow.md) §17** rather than left implicit.

This is the blueprint's own WebDriver smoke suite, dropped from the implementation set without a reason ([`00-master-plan.md`](00-master-plan.md) §4a).
**Tests:** `packaged_app_launches_to_the_lock_screen_in_arabic` · `packaged_app_completes_a_cash_sale` · `packaged_app_retrieves_its_key_from_the_credential_store`
**Done when:** CI fails when the packaged application does not start, on every platform the driver supports.

### 2.9.6 — The soak dataset generator
**Files:** `justfile`, `.config/nextest.toml`, `crates/pos-db/tests/common/soak_dataset.rs` (new)
A year of a busy minimarket, generated deterministically: ~250 000 sales, ~800 000 lines, ~1 200 000 stock events, ~300 000 audit entries. It is built **here**, not in Phase 5, because the answer it produces — an index, an archival strategy, a migration that takes four minutes — is a schema and reporting change, and discovering it after the schema and the reports have shipped is discovering it too late.

Phase 5 then *uses* it (5.1.1, 5.5.3) rather than inventing it.
**Tests:** `the_soak_dataset_is_deterministic_for_a_given_seed`
**Done when:** every Phase-1 and Phase-2 budget is measured against it once, and the numbers are recorded.

---

## Exit gate

```bash
just lint && just test
cargo nextest run -p pos-fiscal              # 22 rules × 5 goldens + training case, all mock faults
cargo nextest run --workspace -E 'test(prop_)'
just bench-gate                              # budgets, on the reference register, exits non-zero on regression
```

By demonstration:

1. **Card sale, split with cash.** Card charged the exact unrounded amount; cash rounded; totals exact.
2. **Timeout injected mid-authorisation.** The UI shows *"Checking last transaction…"*; the status query resolves; exactly one tender exists. Then the same with the terminal returning approved-after-timeout, and again with declined-after-timeout.
3. **Partial approval**, remaining paid in cash. Then repeat and abandon: the partial reverses.
4. **Receipted return** of two of three units. Attempt to return two more — refused by invariant, not by a UI check. Then refund all three units of a discounted line and confirm the refund equals the line total **to the fil**.
5. **Refund to the original card** via `psp_ref`. Then attempt cash-for-card without the capability — refused. Then a defective claim on day 30 against a 14-day window: refused as change-of-mind, permitted as a defect with manager approval and an audit row.
6. **Exchange**: return one item, buy another, settle the difference through the internal `exchange` tender. Two linked documents, written in one transaction, and neither is left unsettled.
7. **Shift**: open with a float, sell, drop to the safe, bank from the safe, pay out, pay in, close blind. Z prints and balances at zero, and the bank deposit moved the drawer by nothing.
8. **Fiscal**: the pinned manifest exists with a hash per artifact; five golden documents plus the training case pass every runnable rule, with any unrunnable rule named; every mock fault handled; restart with a full queue drains in ICV order with no gaps and no duplicates.
9. **Fiscal reject**: the mock returns a validation error; the dead letter carries the verbatim message; the local sale is untouched; the receipt is still reprintable. Then a **build failure**: it becomes `BuildFailed`, does not appear in `dead_letter_count`, and is remediated through `fiscal_rebuild_failed` with the same `fiscal_uuid`.
10. **Single-register local ICV allocation.** Complete a sale with the checkout queue row at `icv IS NULL`; start the Phase-2 drain; the in-process store-scoped allocator records this register in `allocator_ref`, assigns the next monotonic ICV once, and every retry keeps the same UUID, ICV and bytes.
11. **Credit note for an uncleared invoice** waits until the invoice clears, then submits.
12. **Journal**: find Tuesday's receipt by card last-4 in under ten seconds; reprint it marked DUPLICATE with the identical QR.
13. **The packaged application** launches on every supported OS and completes a cash sale, driven by the automated suite rather than by you.
14. **Automated tests exist** for E.2, E.2b, E.15, E.16, E.17, E.20, E.21, E.22, E.23, E.24, E.25, E.26, E.27, E.28, E.29, E.30, E.32, E.34, E.35, E.50, E.53, E.65, E.73, E.74, E.75, E.76, E.77, E.79, E.81, E.82, E.84, E.92, and for the Phase-2 half of E.7, E.18, E.19, E.46, E.54 and E.69.

> **Three multi-node cases live later because there is no server, sync or second register here.** Case 31 is serial refund abuse across two stores, enforced later by a server-side check. Case 47 is cross-register reprint through the server-owned `reprint_bundle`. Case 87 is the two-register ICV collision fixture, now owned by Phase 3's server allocator and lease microstep. A gate item nobody can satisfy honestly teaches the reader that the whole list is decorative.

**Not claimed at this gate:** that ISTD accepts anything. That claim requires Phase 5 milestone 5.2 and nothing else can produce it. Say so out loud to anyone who asks — including the merchant. What *is* claimed is narrower and true: the documents pass our conformance harness **against the pinned official specification**, which is a materially stronger statement than it was before 2.7.0 existed.

→ **Next:** [`phase-3-connected.md`](phase-3-connected.md)
