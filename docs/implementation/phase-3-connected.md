# Phase 3 — Connected

> **Exit:** two registers and a back office converge to byte-identical state through a week of offline chaos, customers exist under PDPL-grade consent, and a new register can be provisioned from scratch by someone who is not you.

**Effort:** 8–10 weeks.
**Scope from the master plan:** C.8 customers & loyalty, C.13 back office core, C.14 sync, migration 0010.
**Plus:** gap G-3 device provisioning, licensing/entitlements (blueprint §7), Sentry (gap G-8's remote half).

Until now every register has been an island, and that was deliberate — register autonomy is the product's spine and building it first means sync can never become a runtime dependency. This phase adds coordination without adding dependency: **a cut network cable must still change nothing about the ability to sell.**

---

## Group dependency graph

```
3.1 sync protocol & server ─┬─→ 3.2 push (facts up) ──┬─→ 3.5 chaos convergence
                            └─→ 3.3 pull (reference down) ┤
3.4 customers & loyalty ─────────────────────────────────┤
3.6 back office ─────────────────────────────────────────┤
3.7 device provisioning ─────────────────────────────────┤
3.8 licensing ───────────────────────────────────────────┤
3.9 observability ───────────────────────────────────────┘
```

---

## Group 3.1 — Sync protocol and the server

*Blueprint §4. Everything here is specified in [`ref/sync-protocol.md`](ref/sync-protocol.md); this is the build order.*

### 3.1.1 — Protocol types
**Files:** `crates/pos-sync/src/lib.rs`, `src/protocol.rs` (new)
Extend the existing `PushBatch` / `Change` / `PullRequest` skeleton with `PushResponse` (per-item acknowledgement), `PullResponse` (changes + next cursor + a `has_more` flag), and an `Entity` enum so entity names are typed rather than stringly.
**Tests:** `protocol_types_roundtrip_json` · `entity_names_are_stable` (a golden — renaming an entity is a wire break)

### 3.1.2 — Server schema mirror
**Files:** `apps/server/migrations/` (mirroring SQLite 0002–0010)
Per [`ref/schema.md`](ref/schema.md) "Postgres mirror". Two things the SQLite side does not have:
- a `BEFORE UPDATE` trigger on every reference table assigning `version = nextval('change_seq')`, so the pull cursor cannot drift because someone forgot to bump it;
- `REVOKE UPDATE, DELETE` on every fact table for the application role — immutability enforced by the database, not by discipline (conventions I-4).
**Tests:** `apps/server/tests/immutability.rs::fact_tables_reject_update_and_delete`

### 3.1.3 — Push endpoint
**Files:** `apps/server/src/sync/push.rs` (new)
`POST /sync/push { device_id, batch_id, changes[] }`. Upsert by UUID → **idempotent**, so a retry after a timeout is safe. Per-item acknowledgement so one poison row cannot block a batch (E.11).
**Tests:** `duplicate_batch_is_a_no_op` (E.10) · `partial_failure_acks_per_item` · `poison_item_goes_to_dead_letter_without_blocking` (E.11) · `prop_apply_is_idempotent_under_any_replay_order`

### 3.1.4 — Pull endpoint
**Files:** `apps/server/src/sync/pull.rs` (new)
`GET /sync/pull?entity=…&after=<cursor>&limit=500`. Deletes arrive as tombstones. First run bootstraps from a snapshot, then tails the changelog.
**Tests:** `pull_returns_monotonic_versions` · `tombstones_are_delivered` · `bootstrap_then_tail_equals_full_history`

### 3.1.5 — Contract fixtures
**Files:** `crates/pos-sync/tests/fixtures/` (new)
Client and server both test against the **same** JSON fixtures. This is what stops the two sides drifting into a shared misunderstanding.
**Tests:** `client_and_server_agree_on_every_fixture`

### 3.1.6 — Auth and transport
**Files:** `apps/server/src/auth.rs` (new)
Device-scoped tokens; TLS with certificate validation on; consider pinning the sync API. A device token is bound to a `register_id` and revocable from the back office.
**Tests:** `revoked_device_token_is_rejected` · `token_scoped_to_its_register`

---

## Group 3.2 — Push: facts up

### 3.2.1 — The drain task
**Files:** `apps/terminal/src-tauri/src/sync/pusher.rs` (new)
A background task batching from `sync_outbox` in `seq` order. Marks `pushed_at` **only after a confirmed 200**.
**Tests:** `pushed_at_set_only_after_confirmation` · `crash_before_ack_replays_safely`

### 3.2.2 — Silent to the cashier, loud to you
**Files:** `apps/terminal/src-tauri/src/sync/pusher.rs`
Sync failures never surface as an error dialog. They surface as the status strip's offline indicator and as back-office device health: last-seen, outbox depth, sync lag.
**Tests:** `sync_failure_never_blocks_a_sale` · `sync_failure_never_raises_a_modal`

### 3.2.3 — Outbox growth alarms
**Files:** `apps/terminal/src-tauri/src/health.rs`
Alarm at a configurable depth — the blueprint suggests ~48 h of accumulation — plus a disk-budget check (E.8). The UI stays calm; the metric does not.
**Tests:** `deep_outbox_alarms_without_blocking_sales`

---

## Group 3.3 — Pull: reference data down

### 3.3.1 — The pull task and dependency ordering
**Files:** `apps/terminal/src-tauri/src/sync/puller.rs` (new)
Reference data applies in dependency order: tax categories → tax rates → categories → products → barcodes → prices → users → roles → settings. Facts apply in any order because they are append-only.
**Tests:** `apply_order_respects_dependencies` · `out_of_order_facts_still_converge`

### 3.3.2 — Catalog apply and open carts
**Files:** `apps/terminal/src-tauri/src/sync/puller.rs`
On applying a catalogue change, **unfinalized carts keep their captured prices** — the customer saw the shelf (E.37, conventions I-5). New line additions get the new price. A manual *reprice cart* action exists for merchants whose policy differs.
**Tests:** `open_cart_keeps_captured_price_after_catalog_apply` · `finalized_sales_are_never_touched` (E.9)

### 3.3.3 — Local edits as change-requests
**Files:** `apps/terminal/src-tauri/src/sync/puller.rs`, back office in 3.6
The emergency quick-add from Phase 1 (1.11.10) syncs **up as a change-request**, flagged for back-office approval — **never silently merged** (master plan C.14).
**Tests:** `local_product_edit_syncs_as_change_request_not_upsert`

### 3.3.4 — Parked carts never sync
**Files:** `crates/pos-db/src/repo/parked.rs`
A parked cart is a register-physical concept: the customer is standing at *that* till.
**Tests:** `parked_carts_produce_no_outbox_rows`

---

## Group 3.4 — Customers and loyalty

*PDPL is the specification for this group. Read [`ref/security-compliance.md`](ref/security-compliance.md) §2 first.*

### 3.4.1 — Migration `0010`
**Files:** `crates/pos-db/migrations/0011_customers_loyalty.sql`

### 3.4.2 — Consent as a record, not a boolean
**Files:** `crates/pos-domain/src/customer.rs` (new)
Every consent stores **the wording version and the timestamp**. "We had consent" must be provable years later, and a boolean cannot prove which words the customer agreed to.
**Tests:** `consent_records_text_version_and_timestamp` · `marketing_consent_is_separate_from_loyalty_terms` · `withdrawn_consent_is_a_new_record_not_an_update`

### 3.4.3 — Loyalty ledger
**Files:** `crates/pos-domain/src/loyalty.rs` (new)
Append-only: `earn`, `redeem`, `adjust`, `expire`. Balance = Σ. Conflict-free across offline registers, exactly like stock and cash.
**Redemption is a discount, not a tender** — that keeps the tax maths standard (master plan C.8).
**Tests:** `prop_balance_equals_ledger_sum` · `redeem_beyond_balance_is_refused` · `redemption_as_discount_keeps_tax_math_standard` · `prop_two_offline_registers_earning_converge` (E.62)

### 3.4.4 — Anonymisation
**Files:** `crates/pos-db/src/repo/customer.rs` (new)
Erasure = **anonymisation**: null the person, keep the immutable financial facts against the anonymised id. Never a hard delete of a sale.
**Tests:** `anonymize_nulls_pii_and_keeps_ledger_rows` · `anonymized_customer_is_not_findable_by_phone` · `sales_survive_anonymization_with_totals_intact`

### 3.4.5 — Export my data
**Files:** `apps/server/src/customer/export.rs` (new)
Profile + consent history + purchase history + loyalty ledger, as one file. A PDPL data-subject right, and a support tool.
**Tests:** `export_contains_every_category_of_stored_data`

### 3.4.6 — Register-side lookup
**Files:** `apps/terminal/src/screens/Customer.tsx`
By phone or QR card. Last *N* purchases visible at the register; full history in the back office. Powers receipted-return lookup by customer.

---

## Group 3.5 — Chaos convergence

*The blueprint's headline test, and this phase's real exit criterion.*

### 3.5.1 — The chaos harness
**Files:** `crates/pos-sync/tests/chaos.rs` (new)
Two simulated registers plus a server. The harness replays batches, drops responses, duplicates pushes, reorders, partitions one register for simulated days, and restarts processes mid-batch.
**Tests:** `prop_both_databases_converge_byte_identical`
**Done when:** after any scripted sequence of faults, a canonical dump of both register databases and the server is **byte-identical** for every fact table and every reference table.

### 3.5.2 — The offline week
**Files:** `crates/pos-sync/tests/chaos.rs`
A scripted week: register A offline for three days while B trades, catalogue edited centrally throughout, both sell the last unit of a product, a refund is attempted at both, and a price changes mid-week.
**Tests:** `offline_week_converges` · `both_sales_of_the_last_unit_stand_and_stock_goes_negative_flagged` (E.12) · `serial_refund_attempt_is_caught_when_connected_and_surfaced_when_not` (E.31)
> E.31 has an accepted residual risk: two stores can each refund the same receipt inside the offline window. The mitigation is a server-side remaining-refundable check whenever connected, plus surfacing the case in the refunds-by-user report. **Say this out loud to the merchant** rather than implying it is impossible.

### 3.5.3 — Fiscal reconciliation over the chaos week
**Files:** `crates/pos-fiscal/tests/reconciliation.rs`
The reconciliation report from [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §8 must be clean at the end of the chaos week: nothing uncleared without a reason, no dead letters, and **no cleared document without a matching local sale**.
**Tests:** `reconciliation_clean_after_chaos_week`

---

## Group 3.6 — Back office

### 3.6.1 — Server API surface
**Files:** `apps/server/src/api/` (new)
Catalog, users/roles, customers, settings, reports, device health, fiscal reconciliation. OpenAPI schema emitted; `packages/api-types` generated from it, with CI failing when the committed types differ from a fresh generation.
**Tests:** `openapi_schema_is_committed_and_current`

### 3.6.2 — Catalog management
**Files:** `apps/backoffice/src/pages/catalog/`
Products, barcodes, categories, tax categories and rates. **Overlapping tax rate rules are rejected at save time**, not discovered at the register (see [`ref/tax-jordan.md`](ref/tax-jordan.md) §3).
**Tests:** `overlapping_rate_rules_rejected_on_save`

### 3.6.3 — Users, roles, device health
**Files:** `apps/backoffice/src/pages/`
Role editing against the capability list; per-store role grants. Device health: last-seen, outbox depth, sync lag, print-failure rate, terminal-timeout rate, uncleared fiscal count, backup age.

### 3.6.4 — Fiscal reconciliation view
**Files:** `apps/backoffice/src/pages/fiscal/`
The five row classes from §8 of the fiscal reference. The dead-letter list shows the ISTD error **verbatim** with a requeue action.

### 3.6.5 — Change-request approval queue
**Files:** `apps/backoffice/src/pages/changes/`
Where local emergency edits (3.3.3) land for a human decision.

### 3.6.6 — Multi-store scoping
**Files:** `apps/server/src/scope.rs` (new)
Org → stores → registers. Catalog org-global; prices, promotions, tax profile and settings resolve org → store; stock and shifts per store; users org-level with per-store role grants.
**Tests:** `store_scoped_setting_overrides_org` · `user_without_store_grant_cannot_read_its_sales`

---

## Group 3.7 — Device provisioning

*Gap G-3. Until now, `register_id` came from a fixture.*

### 3.7.1 — Enrollment flow
**Files:** `apps/terminal/src-tauri/src/provisioning.rs` (new)
A fresh install shows an enrollment screen: server URL + a one-time enrollment code from the back office → the server issues a `register_id`, a device token, and the store binding → the terminal bootstraps its catalogue from a snapshot.
**Tests:** `enrollment_issues_token_and_binds_store` · `expired_enrollment_code_is_refused` · `bootstrap_snapshot_is_complete`

### 3.7.2 — Clone detection
**Files:** `apps/server/src/sync/push.rs`
A register restored from a disk image carries someone else's `register_id`. Detect the device-fingerprint collision at registration and **refuse to sync until re-provisioned** (E.13) — silently accepting it corrupts two registers' receipt sequences.
**Tests:** `device_id_collision_refuses_sync_with_a_named_error`

### 3.7.3 — Deactivation propagation
**Files:** `apps/terminal/src-tauri/src/sync/puller.rs`
A terminated employee's PIN deactivates at next contact. Offline terminals honour a **max-offline-auth window** (E.55) — a real limit of offline-first, and one to disclose to the merchant rather than hide.
**Tests:** `deactivation_applies_at_next_contact` · `offline_auth_window_expires_and_says_why`

---

## Group 3.8 — Licensing

*Blueprint §7. A store must not die because a licence server did.*

### 3.8.1 — Entitlement files
**Files:** `crates/pos-sync/src/license.rs` (new)
Ed25519-signed entitlement files; periodic online validation; a **generous offline grace period**; on expiry, degrade to **read-only, never lock-out** (E.57).
**Tests:** `tampered_entitlement_is_rejected` · `expired_licence_degrades_read_only_never_mid_day` · `grace_period_survives_a_long_outage`
**Done when:** no code path locks a register during an open shift. Prove it with a test that opens a shift, expires the licence, and asserts selling continues to shift close.

---

## Group 3.9 — Observability

### 3.9.1 — Sentry, scrubbed
**Files:** `apps/terminal/src-tauri/src/telemetry.rs`, `apps/terminal/src/lib/sentry.ts`
Rust panics and JS errors. The Phase-1 scrubber (1.6.8) sits in front of the transport. Events buffer offline, are capped, and **never block selling** (E.59).
**Tests:** `sentry_events_pass_through_the_scrubber` · `offline_telemetry_is_buffered_and_capped` · `no_pii_in_a_captured_panic`

### 3.9.2 — The metrics that predict support tickets
**Files:** `apps/terminal/src-tauri/src/health.rs`
Outbox depth · sync lag · print-failure rate · terminal-timeout rate · uncleared fiscal count and oldest age · crash-free sessions · cold-start time · backup age · audit-chain status. Surfaced per device in the back office.

---

## Exit gate

```bash
just lint && just test
cargo nextest run -p pos-sync                # chaos convergence + contract fixtures
cargo nextest run --workspace -E 'test(prop_)'
```

By demonstration:

1. **Provision a second register from scratch** using only the back office and an enrollment code. Someone who is not you does it, from written instructions.
2. **Chaos week** runs end to end; both register databases and the server are byte-identical afterwards.
3. **Register A offline for three days** keeps selling; on reconnect it drains in order with no duplicates and no gaps in its receipt sequence.
4. **Edit a product centrally** while a cart is open on an offline register. The open cart keeps its price; the next scan gets the new one; the finalized sale from yesterday is untouched.
5. **Both registers sell the last unit offline.** Both sales stand. Stock goes negative and is flagged, not blocked.
6. **Restore register A from a disk image onto new hardware.** It refuses to sync and says why, until re-provisioned.
7. **Create a customer with recorded consent**, earn and redeem points, export their data, then anonymise them. Their sales still total correctly; their name is gone; the loyalty ledger rows remain against the anonymised id.
8. **Expire the licence during an open shift.** Selling continues to shift close, then degrades to read-only.
9. **Fiscal reconciliation is clean** over the chaos week — no cleared document without a local sale.
10. **Trigger a panic with a customer attached.** The Sentry event contains no name, no phone, no PIN, no PAN.
11. **Automated tests exist** for E.8, E.9, E.10, E.11, E.12, E.13, E.31, E.55, E.57, E.59, E.62.

→ **Next:** [`phase-4-depth.md`](phase-4-depth.md)
