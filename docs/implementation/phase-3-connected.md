# Phase 3 — Connected

> **Exit:** two registers and a back office converge — the server holding exactly the union of what the registers produced, every reference table projecting identically on all three nodes — through a week of offline chaos; customers exist under PDPL-grade consent; the server is something a person operates rather than something that happens to be running; and a new register can be provisioned from scratch by someone who is not you.

**Effort:** 11–14 weeks.
**Scope from the master plan:** C.8 customers & loyalty, C.13 back office core, C.14 sync, migration **0011**.
**Plus:** gap G-3 device provisioning, licensing/entitlements (blueprint §7), Sentry (gap G-8's remote half), **back-office human authentication and the tenancy posture** ([`ref/security-compliance.md`](ref/security-compliance.md) §5a and §5b), and **group 3.10 — operating the server**, which nothing in the plan owned and Phase 4's gate depends on entirely.

Until now every register has been an island, and that was deliberate — register autonomy is the product's spine and building it first means sync can never become a runtime dependency. This phase adds coordination without adding dependency: **a cut network cable must still change nothing about the ability to sell.**

---

## Group dependency graph

```
3.10 operate the server (start early — it has a hosting lead time)
  │
3.1 sync protocol & server ─┬─→ 3.2 push (facts up) ──┬─→ 3.5 chaos convergence
                            └─→ 3.3 pull (reference down) ┤
3.4 customers & loyalty ─────────────────────────────────┤
3.6 back office (needs 3.1.6's principals) ──────────────┤
3.7 device provisioning ─────────────────────────────────┤
3.8 licensing ───────────────────────────────────────────┤
3.9 observability ───────────────────────────────────────┘
```

**Start 3.10.1 alongside 3.1.** Choosing a host, a region and a legal basis is a decision with an
external dependency and a legal question attached, and every later microstep in this phase deploys
onto whatever it chose.

---

## Group 3.1 — Sync protocol and the server

*Blueprint §4. Everything here is specified in [`ref/sync-protocol.md`](ref/sync-protocol.md); this is the build order.*

### 3.1.1 — Protocol types
**Files:** `crates/pos-sync/src/lib.rs`, `src/protocol.rs` (new)
Extend the existing `PushBatch` / `Change` / `PullRequest` skeleton with `PushResponse` (per-commit acknowledgement), `PullResponse` (changes + next cursor + a `has_more` flag), and an `Entity` enum so entity names are typed rather than stringly.

**Both envelopes carry `protocol_version` and `schema_version`**, with a stated compatibility rule: the server accepts N and N−1; a register further behind is told to update before syncing **and keeps selling**. Two release channels, a staged rollout and registers that can be offline for months guarantee mixed versions, and adding a version field to a wire format that already has durable queued rows behind it is a fleet migration. The durable envelope's version fields therefore land in Phase 1 with the outbox, not here — this microstep is where the server learns to branch on them.
**Tests:** `protocol_types_roundtrip_json` · `entity_names_are_stable` (a golden — renaming an entity is a wire break) · `an_unsupported_protocol_version_fails_the_batch_and_applies_nothing` (E.88) · `a_version_mismatch_never_dead_letters_a_fact` · `a_too_old_register_keeps_selling_and_says_so_in_device_health`

### 3.1.2 — Server schema mirror, with tenancy in the schema
**Files:** `apps/server/migrations/` (mirroring SQLite 0002–0011), `scripts/verify-pg-migrations.py`, `scripts/tests/verify_pg_migrations_test.py` (new), `crates/pos-db/tests/sync_authority.rs` (new), `apps/server/tests/tenancy.rs` (new)
Per [`ref/schema.md`](ref/schema.md) "Postgres mirror". Four things the SQLite side does not have:

- a `BEFORE INSERT OR UPDATE` trigger on every reference table assigning `version = nextval('change_seq')`, so the pull cursor cannot drift because someone forgot to bump it — **insert as well as update**, or a newly inserted row sits at version 0 and never pulls;
- `REVOKE UPDATE, DELETE` on every fact table for the application role — immutability enforced by the database, not by discipline (conventions I-4);
- **`org_id NOT NULL` on every merchant-owned table**, tenant-scoped unique keys (`(org_id, sku)`, not a global `sku` — two merchants selling the same barcode is the normal case), and **composite foreign keys** so a child references `(org_id, parent_id)` and cannot point across a tenant boundary even if a query forgets to filter;
- `ENABLE` **and** `FORCE ROW LEVEL SECURITY` under an application role that is neither the table owner nor `BYPASSRLS`, with default-deny policies and the tenant set per transaction from the authenticated principal.

The deployment model is **one shared multi-tenant service**, recorded for sign-off in [`ref/security-compliance.md`](ref/security-compliance.md) §5b with its alternative and its trade-off. It is here rather than in application code because a multi-tenant schema can serve a single tenant and a single-tenant schema cannot be made multi-tenant without rewriting every query that has already shipped — and because a missed application filter is then a cross-tenant personal-data breach caused by the vendor, affecting every merchant on the instance at once.
**Tests:** `apps/server/tests/immutability.rs::fact_tables_reject_update_and_delete` · `rls_is_forced_on_every_merchant_owned_table` — a catalogue test over the schema, so a new table without a policy fails · `the_application_role_is_not_the_owner_and_lacks_bypassrls` · `a_composite_foreign_key_refuses_a_cross_org_parent` · `two_orgs_may_use_the_same_sku` · `a_newly_inserted_reference_row_has_a_nonzero_version` · `every_persistent_table_has_one_sync_authority_class` · `a_synced_table_cannot_reference_a_local_only_table` · `register_up_facts_have_one_ready_commit`
**Done when:** `just verify-schema && just verify-pg` exits zero, including the negative classification fixtures and the engine-backed two-org attacks executed as non-owner `pos_app` rather than the migration role.

### 3.1.3 — Push endpoint
**Files:** `apps/server/src/sync/push.rs` (new)
`POST /sync/push { protocol_version, schema_version, device_id, batch_id, commits[] }`.

**`INSERT`, never upsert.** "Upsert by UUID" and `REVOKE UPDATE ON` every fact table are mutually exclusive, and `DO NOTHING` silently treats a *different* financial payload under a known UUID as a harmless duplicate. On conflict the server compares canonical payload bytes: identical is `duplicate` and acknowledged; different is `rejected`, the stored row is **not** touched, and the item is dead-lettered and alarmed (E.89).

Acknowledgement is **per commit group**, not per row. A completed sale is one commit — header, lines, tenders, tax rows, tax summary, stock event, audit row — validated and applied in one PostgreSQL transaction. Per-row acknowledgement let the server keep a sale header while permanently rejecting one of its lines, and central tax, inventory and revenue reports would then describe a transaction that never existed locally. A malformed group leaves **zero** business rows applied (E.11).

Scope comes from the authenticated principal, never from the body: a mismatching `device_id` is refused rather than trusted, and every payload's ownership is validated against the principal's org, store and register.
**Tests:** `duplicate_batch_is_a_no_op` (E.10) · `partial_failure_acks_per_commit` · `poison_commit_goes_to_dead_letter_without_blocking` (E.11) · `an_incomplete_commit_group_is_held_not_partially_applied` · `an_identical_replay_is_reported_as_duplicate` (E.89) · `a_different_payload_under_a_known_uuid_is_rejected_and_alarms` · `the_stored_row_is_never_mutated_by_a_conflict` · `a_body_device_id_that_disagrees_with_the_principal_is_refused` · `prop_apply_is_idempotent_under_any_replay_order`

### 3.1.4 — Pull endpoint
**Files:** `apps/server/src/sync/pull.rs` (new)
`GET /sync/pull?entity=…&after=<cursor>&limit=500`. Deletes arrive as tombstones. First run bootstraps from a snapshot, then tails the changelog. The caller may not select an entity outside its principal's direction table, and pull is subject to revocation exactly as push is.
**Tests:** `pull_returns_monotonic_versions` · `tombstones_are_delivered` · `bootstrap_then_tail_equals_full_history` · `a_register_cannot_pull_an_entity_outside_its_direction` · `a_revoked_token_is_refused_on_pull_as_well_as_push`

### 3.1.5 — Contract fixtures
**Files:** `crates/pos-sync/tests/fixtures/` (new)
Client and server both test against the **same** JSON fixtures — one per edge of the apply graph, and one per supported protocol version. This is what stops the two sides drifting into a shared misunderstanding.
**Tests:** `client_and_server_agree_on_every_fixture` · `every_dependency_edge_has_a_fixture`

### 3.1.6 — Principals, auth and transport
**Files:** `apps/server/src/auth.rs` (new), `apps/server/src/routes/registry.rs` (new)
**Two kinds of principal, and they are not interchangeable** ([`ref/security-compliance.md`](ref/security-compliance.md) §5a). `DevicePrincipal { org_id, store_id, register_id }` is what a register presents. `BackOfficePrincipal { subject, org_id, store_grants, capabilities }` is what a human presents, and nothing in the plan previously said how an owner, a manager or a support operator signs in — while the back office is where customer exports, user administration, reports, device control and fiscal reconciliation live. The Tauri capability registry protects Tauri commands; it has no opinion about an Axum route.

Humans authenticate through managed OIDC Authorization Code with PKCE; this product stores no back-office password. Phishing-resistant MFA is required for any principal holding `user.admin`, `settings.edit`, `customer.lookup` or `reports.all`. Sessions are server-side, short-lived, revocable, revoked on role change, `Secure`/`HttpOnly`/`SameSite`, with CSRF protection on every state-changing route.

Device tokens are bound to a **non-exportable** OS- or hardware-backed keypair enrolled at provisioning, with proof of possession on every request. A device fingerprint is not proof of anything: a cloned disk carries the original `register_id` and bearer token, so a restored image must fail on its **first authenticated request**, not at a registration it never performs (3.7.2).

Authorization is **deny by default**: every route declares its required capability in a registry, exactly as IPC commands do, and the OpenAPI security requirement per operation is emitted from that same registry so the published contract cannot disagree with the check.
**Tests:** `http_routes_all_declare_a_capability` · `an_unauthenticated_request_is_refused_on_every_route` · `a_session_revoked_mid_flight_is_refused_on_the_next_request` · `a_principal_without_a_store_grant_cannot_read_that_stores_sales` · `mfa_is_required_for_every_privileged_capability` · `prop_no_query_crosses_an_org_boundary` (E.90) · `revoked_device_token_is_rejected` · `token_scoped_to_its_register` · `a_cloned_image_fails_its_first_authenticated_request` (E.13)
**Done when:** `http_routes_all_declare_a_capability` passes over the whole router, and a two-org fixture — both orgs fully populated — cannot read or write across the boundary through any route in the API surface. One org proves nothing about isolation.

> ⚠️ **OPEN — blocks 3.1.6.** In which country and legal entity will the shared service and each subprocessor host merchant and customer data, and what cross-border basis applies? Default until answered: no customer PII may sync or enter telemetry outside Jordan; only non-PII fixtures may use a development host.
> Owner: 3.1.6. Source that settles it: the signed hosting/subprocessor contract, Jordan PDPL transfer assessment and counsel's written conclusion.

### 3.1.7 — Server ICV allocator and one-value leases
**Depends on:** 2.7.0, 2.7.4, 3.1.2, 3.1.6
**Files:** `apps/server/src/fiscal/icv.rs` (new), `apps/server/src/routes/registry.rs`, `apps/server/migrations/`, `apps/terminal/src-tauri/src/fiscal/icv_lease.rs` (new), `crates/pos-sync/src/protocol.rs`, `crates/pos-sync/tests/fixtures/icv_lease.json` (new), `apps/server/tests/icv_allocator.rs` (new)

Phase 2's in-process allocator is valid only because one register owns the store database. From this
step onward the server exclusively locks `(org_id, 'store', store_id, 'fiscal_icv')` and
`POST /fiscal/icv/leases` allocates one value bound immutably to `fiscal_uuid`. Device-principal
scope supplies org, store and register; none is trusted from JSON. A replay returns the same
`lease_id` and ICV, and the register persists that lease id as `allocator_ref` before freezing XML.

A disconnected register completes the sale and keeps `fiscal_queue.icv IS NULL`. It never falls back
to a local store counter, because two such fallbacks recreate the collision this step exists to
remove. Reconnect order may change which document receives the next number; it may not create a
duplicate, change a local UUID, or make selling wait.

**Tests:** `two_offline_registers_never_allocate_the_same_icv` (E.87) · `two_registers_offline_then_reconnect_allocate_distinct_icvs`
**Fixture:** `two_registers_offline_then_reconnect_allocate_distinct_icvs` — both registers complete sales with NULL ICV, reconnect in either order, receive distinct one-value leases, then replay every request without changing UUID, ICV, allocator reference or payload bytes.
**Done when:** `cargo nextest run -p pos-sync two_offline_registers_never_allocate_the_same_icv && cargo nextest run -p server --test icv_allocator` exits zero for both reconnect orders and for a response dropped after the server commits the lease.

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

### 3.2.3 — Outbox growth, retention and the long-offline register
**Files:** `apps/terminal/src-tauri/src/health.rs`, `crates/pos-db/src/repo/outbox.rs`
Alarm at a configurable depth — the blueprint suggests ~48 h of accumulation — plus a disk-budget check (E.8). The UI stays calm; the metric does not.

**And the transport rows are pruned.** `sync_outbox` held a duplicate JSON payload beside every immutable source fact, indefinitely, with no retention policy at any offline horizon: a mature or long-offline register grows every encrypted backup and every migration until the disk-space guard blocks new sales. Delivery rows are prunable once durably acknowledged; the permanent record of what belonged to a commit is `fact_commit_member`, which is not transport and is not pruned ([`ref/sync-protocol.md`](ref/sync-protocol.md) §5). Byte budgets are stated for 30, 90 and 365 offline days, along with the checkpoint, freelist and vacuum policy, and measured against the 2.9.6 soak dataset rather than guessed.
**Tests:** `deep_outbox_alarms_without_blocking_sales` · `acknowledged_delivery_rows_are_pruned_and_the_commit_manifest_is_not` · `the_outbox_stays_within_budget_at_ninety_offline_days`

### 3.2.4 — Audit checkpoints, so a deleted tail is visible
**Files:** `apps/terminal/src-tauri/src/sync/pusher.rs`, `apps/server/src/audit/checkpoint.rs` (new)
A local hash chain detects a modified row and **cannot detect a deleted tail**: removing the newest entries leaves a shorter, valid chain, and the newest entries are the drawer opens, refunds and overrides worth deleting. The chain needs an anchor outside the register.

The register pushes a signed `audit_checkpoint` — `(register_id, last_seq, last_hash)` — with every batch. The server keeps the per-register head **immutably**, rejects a fork or a gap, and alarms. Local verification compares against the last server checkpoint, so a tail deleted below it is detected rather than reported `Intact`. The unsynced tail above the last checkpoint remains a stated residual risk, bounded by how often the register syncs.
**Tests:** `tail_deletion_is_detected_against_the_server_checkpoint` (E.91) · `a_forked_checkpoint_is_refused_and_alarms` · `a_checkpoint_below_the_stored_head_is_refused`

---

## Group 3.3 — Pull: reference data down

### 3.3.1 — The pull task and dependency ordering
**Files:** `apps/terminal/src-tauri/src/sync/puller.rs` (new)
**Use the generated order in [`ref/sync-protocol.md`](ref/sync-protocol.md) §3, not a hand-written list.** The prose order this microstep used to carry — tax categories → tax rates → categories → products → barcodes → prices → users → roles → settings — was both wrong and incomplete: `price_list` came before `store` although its schema references a store, and a dozen tables were missing. The order is a DAG derived from `ref/schema.md`, including self-referential category handling, and a contract fixture covers every edge.

**"Facts apply in any order" was also false.** Append-only ownership removes *write* conflicts; it does not remove referential dependencies. `sale_line` needs `sale` and `product`; tax and discount rows need `sale_line`; stock events need product and store. A page or commit group is staged whole, and a group with a missing parent is **held pending, not dead-lettered and not partially applied** — with the cursor left where it was.
**Tests:** `apply_order_respects_dependencies` · `a_commit_group_arriving_out_of_order_is_held_then_applied` · `a_missing_parent_does_not_advance_the_cursor`

### 3.3.2 — Catalog apply and open carts
**Files:** `apps/terminal/src-tauri/src/sync/puller.rs`
On applying a catalogue change, **unfinalized carts keep their captured prices** — the customer saw the shelf (E.37, conventions I-5). New line additions get the new price. A manual *reprice cart* action exists for merchants whose policy differs.
**Tests:** `open_cart_keeps_captured_price_after_catalog_apply` · `finalized_sales_are_never_touched` (E.9) · `reprice_cart_action_applies_new_prices`

### 3.3.3 — Local edits as change-requests
**Files:** `apps/terminal/src-tauri/src/sync/puller.rs`, back office in 3.6
The emergency quick-add from Phase 1 (1.11.10) syncs **up as a change-request**, flagged for back-office approval — **never silently merged** (master plan C.14).
**Tests:** `local_product_edit_syncs_as_change_request_not_upsert`

### 3.3.4 — Parked carts never sync
**Files:** `crates/pos-db/src/repo/parked.rs`
A parked cart is a register-physical concept: the customer is standing at *that* till.
**Tests:** `parked_carts_produce_no_outbox_rows`

### 3.3.5 — Cross-register reprint, on demand
**Files:** `apps/server/src/documents/reprint.rs` (new), `apps/terminal/src-tauri/src/sync/documents.rs` (new)
"Reprint days later from another register" was claimed as working in three documents and had no data path. The clearance result syncs down; the **sale does not** — facts travel up only — so register B held a QR and none of the document it belonged to.

The answer is a fetch, not replication: the server owns a `reprint_bundle` containing the immutable receipt facts, the rendered artifact's content hash and the fiscal result, with **no foreign key to a local sale** ([`ref/sync-protocol.md`](ref/sync-protocol.md) §3). It is capability-gated, it is never written into the fetching register's database, and offline it is refused with a named reason rather than approximated from whatever is to hand.
**Tests:** `another_register_fetches_the_reprint_bundle_when_connected` (E.47) · `document_fetch_is_refused_offline_with_a_named_error` · `a_fetched_bundle_is_never_written_to_the_local_database`

---

## Group 3.4 — Customers and loyalty

*PDPL is the specification for this group. Read [`ref/security-compliance.md`](ref/security-compliance.md) §2 first.*

### 3.4.1 — Migration `0011`
**Files:** `crates/pos-db/migrations/0011_customers_loyalty.sql`
Per [`ref/schema.md`](ref/schema.md) §0011: customers, immutable `consent_notice` records, the append-only `consent_event` ledger, privacy request cases and events, server-issued `privacy_tombstone`, the loyalty ledger, the offline `authorization_lease`, and `org_recovery_envelope`.

> ⚠️ **OPEN — blocks 3.4.1.** For this deployment, which entity is controller, which is processor, who is a recipient, is a DPO required, and is the Personal Data Processing Register entry required and complete? Default until answered: the schema may migrate, but customer capture, consent collection and customer-PII sync remain disabled.
> Owner: 3.4.1. Source that settles it: the current MoDEE Personal Data Processing Register instructions and dated Jordanian counsel advice for the deployed roles.

### 3.4.2 — Consent as an append-only event ledger
**Files:** `crates/pos-domain/src/customer.rs` (new)
Every consent event references an **immutable `consent_notice`** — the full Arabic and English text, the locale, the controller and contact, the purpose-specific options, the data categories, the recipients, any transfer destination and safeguard, the effective dates, and a content digest. A bare `text_version` label could show "v2" and could not reproduce what the customer actually saw, which is the only thing the record exists to prove.

**Consent is a fact, not a field.** Field-level last-write-wins and "a withdrawal is a new record" cannot both be true: a stale or concurrent grant would become effective after a withdrawal, and marketing would proceed without current consent with no defensible record of which state governed. So `consent_event` is append-only with a `supersedes` link and a server acceptance version, the effective state is derived on the server, and a **concurrent withdrawal dominates** until a human resolves it. Device timestamps decide nothing.
**Tests:** `a_consent_event_references_an_immutable_notice` · `a_notice_version_cannot_be_edited_after_use` · `marketing_consent_is_separate_from_loyalty_terms` · `withdrawn_consent_is_a_new_event_not_an_update` · `prop_consent_effective_state_is_the_latest_accepted_event` · `a_concurrent_grant_and_withdrawal_resolve_to_withdrawn` · `the_exact_historical_notice_renders_years_later`

### 3.4.3 — Loyalty ledger
**Files:** `crates/pos-domain/src/loyalty.rs` (new)
Append-only: `earn`, `redeem`, `adjust`, `expire`. Balance = Σ. Conflict-free across offline registers, exactly like stock and cash — and unlike **stored value**, which is a spendable balance and therefore online-authorise-only (Phase 2, 2.3.11). Loyalty earning converging offline is a different property from a store-credit balance not being spent twice, and the two were conflated.
The implementation carries separate discount and tender strategies, but selects neither until the open item below is closed. If the recorded, advisor-approved policy selects discount treatment, redemption reduces the taxable base through the ordinary discount engine; if it selects tender treatment, it settles consideration without changing that base. Encoding the first branch as universal would turn an unresolved tax position into every merchant's default.
**Tests:** `prop_balance_equals_ledger_sum` · `redeem_beyond_balance_is_refused` · `advisor_selected_discount_policy_keeps_tax_math_standard` · `prop_two_offline_registers_earning_converge` · `loyalty_stays_disabled_until_its_tax_policy_is_recorded`

> ⚠️ **OPEN — blocks 3.4.3.** Is a loyalty redemption a discount that reduces the taxable base, or consideration settled by a tender, and is any part of the reward funded by a third party? Default until answered: loyalty ships **disabled**, and enabling it requires a recorded funding source and an advisor-approved tax treatment persisted against every ledger event.
> Owner: 3.4.3. Source that settles it: a written ISTD ruling and the merchant's tax advisor for the exact reward funding flow.

### 3.4.4 — Anonymisation across the whole PII estate
**Files:** `crates/pos-db/src/repo/customer.rs` (new), `apps/server/src/privacy/`
Erasure = **anonymisation**: null the person, keep the immutable financial facts against the anonymised id. Never a hard delete of a sale.

**It applies to the estate, not to the customer row.** Personal data also sits in sale buyer fields, fiscal XML and raw responses, dead letters, outbox payloads, server replicas, support views, telemetry, receipt artifacts and backups; and a successful demonstration that erased the CRM row while the same identity stayed searchable elsewhere is a demonstration that proves nothing. Work from the inventory in [`ref/security-compliance.md`](ref/security-compliance.md) §2: every column, JSON/XML field, cache, copy, recipient and retention action, with **CRM identity split from lawfully retained invoice identity** and each retention exception named rather than assumed.

Erasure propagates as a **server-issued `privacy_tombstone`** that every register applies — and re-applies after a restore, or a restored backup silently reintroduces an erased person.
**Tests:** `anonymize_nulls_pii_and_keeps_ledger_rows` · `anonymized_customer_is_not_findable_by_phone` · `sales_survive_anonymization_with_totals_intact` · `a_canary_identity_is_absent_from_every_store_in_the_inventory` — one unique value seeded everywhere, asserted gone from SQLite, PostgreSQL, backups, telemetry and support retrieval, with lawful retention exceptions listed explicitly · `a_restored_backup_reapplies_every_tombstone`

### 3.4.5 — Data-subject rights, as a timed case workflow
**Files:** `apps/server/src/customer/export.rs` (new), `apps/server/src/privacy/cases.rs`
Export is profile + consent history + purchase history + loyalty ledger, as one file — a PDPL right and a support tool. It is not the only right.

Objection, restriction to a scope, portability and complaint handling each need a path a customer can actually use, and each needs a **request case with a clock**: `privacy_request_case` and its events, authenticated, with an alarm before the response period expires. Official guidance gives fifteen working days; a right the product cannot receive is a right the merchant cannot honour, whatever the policy says.

A versioned **privacy notice** is presented before processing starts, and the controller/processor allocation for each data flow is recorded in the deployment artefacts rather than assumed by both parties to be the other's job.
**Tests:** `export_contains_every_category_of_stored_data` · `every_right_has_a_route_and_a_case` · `a_request_approaching_its_deadline_raises_an_alarm` · `objection_stops_the_processing_it_names_without_deleting_a_financial_fact`

### 3.4.6 — Register-side lookup
**Files:** `apps/terminal/src/screens/Customer.tsx`
By phone or QR card, under `customer.lookup` — the capability that gates PII access and had no row in the master plan's matrix at all. Last *N* purchases visible at the register; full history in the back office. Powers receipted-return lookup by customer.

### 3.4.7 — Receipt delivery by email or SMS
**Files:** `apps/server/src/receipts/deliver.rs` (new)
A named edge case (E.48) with no microstep anywhere. Delivery is **consent-gated** — an emailed receipt is processing a contact detail, and the consent event that authorises it is the one from 3.4.2 — and a bounce is logged once with no retry storm. The printed receipt remains available regardless: delivery is an addition to the handover, never a substitute for it.
**Tests:** `email_bounce_logged_without_retry_storm` (E.48) · `delivery_without_consent_is_refused` · `a_failed_delivery_never_blocks_or_reprints_the_sale`

---

## Group 3.5 — Chaos convergence

*The blueprint's headline test, and this phase's real exit criterion.*

### 3.5.1 — The chaos harness
**Files:** `crates/pos-sync/tests/chaos.rs` (new), `crates/pos-sync/src/canonical.rs` (new), `crates/pos-sync/tests/seeds/`
Two simulated registers plus a server, in process. The harness replays batches, drops responses **after** the server applied them, duplicates pushes, reorders pulls, delivers a commit group split across batches and out of order, partitions one register for simulated days, restarts processes mid-batch, corrupts a payload, and pushes a known UUID with an altered amount.

**The old single property could not hold and could not be built.** Facts travel up only, so register A never receives register B's sales and the two fact sets are disjoint *by design*; the register is SQLite and the server is PostgreSQL, so `BLOB`/`UUID`, `TEXT`/`TIMESTAMPTZ` and JSON text/`JSONB` differ deliberately; and `stock_cache`, `parked_cart` and the outbox's own autoincrement are local by definition. Whoever implemented `prop_both_databases_converge_byte_identical` would have had to weaken it, which is worse than never having claimed it. Three checkable properties replace it, over the canonical dump specified in [`ref/sync-protocol.md`](ref/sync-protocol.md) §6 — which states exactly which tables are included, which columns are excluded, and why.

The generator is **seeded, bounded and unshrunk**: the fault alphabet is the enumerated list above, sequences come from a seeded RNG whose seed is printed on failure and committed to `tests/seeds/` the moment it finds something, and sequence length is bounded with the bound stated in the test. Shrinking a fault sequence across processes produces a different execution; replaying a recorded seed does not.
The chaos harness re-runs `prop_apply_is_idempotent_under_any_replay_order`, owned by 3.1.3, against every generated schedule; it does not create a competing test alias.
**Tests:** `prop_server_facts_equal_the_union_of_register_outboxes` · `prop_reference_tables_converge_across_all_three_nodes`
**Done when:** after any scripted sequence of faults, all three properties hold, and the canonical dump of any node can be regenerated deterministically from `crates/pos-sync/src/canonical.rs` and diffed as text.

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
**Depends on:** 3.1.6 — the principals and the route registry are prerequisites, not a later hardening pass.
**Files:** `apps/server/src/api/` (new)
Catalog, users/roles, customers, settings, reports, device health, fiscal reconciliation. Every route declares its capability in the registry from 3.1.6; the OpenAPI security requirement per operation is emitted from that registry, so the published contract cannot disagree with the check.

`packages/api-types` is generated from the schema — into **`src/http/`**. `ts-rs` owns `src/ipc/` from the Rust IPC types (conventions §13). Two generators writing one directory, each with a CI job that fails when the committed contents differ from *its* fresh generation, means whichever runs second overwrites the first and both gates then fail permanently. Any type crossing both boundaries — `Money`, `Qty`, `CartSnapshot` — is generated by `ts-rs` and **re-exported** by the HTTP side, never generated twice.
**Tests:** `openapi_schema_is_committed_and_current` · `no_type_name_is_emitted_by_both_generators`

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

### 3.6.6 — Multi-store scoping, inside multi-tenant isolation
**Files:** `apps/server/src/scope.rs` (new)
Org → stores → registers. Catalog org-global; prices, promotions, tax profile and settings resolve org → store; stock and shifts per store; users org-level with per-store role grants.

**Intra-org scoping is the easy half.** The hard half is that org boundary itself, and the plan tested only the easy half: the org, the store grants and the register all come from the authenticated principal, never from a path or query parameter, and the tenant is set per transaction so the schema's row-level security (3.1.2) is the backstop when a query forgets. A cross-tenant leak is a personal-data breach caused by the vendor, it hits every merchant on the instance simultaneously, and both statutory clocks start for all of them — which is why it is tested adversarially rather than reviewed.
**Tests:** `store_scoped_setting_overrides_org` · `user_without_store_grant_cannot_read_its_sales`
**Owned by 3.1.6:** `prop_no_query_crosses_an_org_boundary` (E.90) over the complete route surface.

### 3.6.7 — Catalogue import
**Files:** `apps/server/src/import/catalog.rs` (new), `apps/backoffice/src/pages/catalog/import/`
CSV with Arabic names, **multiple barcodes per product including `pack_qty_milli`**, PLU codes, prices, tax categories, `min_age`, `max_price_minor`, and an optional opening-stock column. Per-row validation with an error report, rather than failing the file — a 2 000-row import that dies on row 1 987 with no detail is an import nobody completes.

It lands **here**, in the back office beside the catalogue pages it feeds, and not in Phase 5 as originally planned. The Phase-4 pilot needs three real assortments of 1 000–2 500 SKUs each, with Arabic names and accountant-reviewed tax categories. Typed through a CRUD form that is weeks of unbudgeted work, and the realistic outcome is a truncated assortment — which invalidates the pilot, because the assortment is what generates the surprises the pilot exists to find. Phase 5's onboarding wizard wraps this rather than reimplementing it (5.6.2).
**Tests:** `import_reports_per_row_errors_and_imports_the_rest` · `a_multipack_barcode_row_imports_its_pack_quantity` · `an_unknown_tax_category_fails_the_row_and_not_the_file`

---

## Group 3.7 — Device provisioning

*Gap G-3. Until now, `register_id` came from a fixture.*

### 3.7.1 — Enrollment flow
**Files:** `apps/terminal/src-tauri/src/provisioning.rs` (new)
A fresh install shows an enrollment screen: server URL + a one-time enrollment code from the back office → the server issues a `register_id`, a device token, and the store binding → the terminal bootstraps its catalogue from a snapshot.
**Tests:** `enrollment_issues_token_and_binds_store` · `expired_enrollment_code_is_refused` · `bootstrap_snapshot_is_complete`

### 3.7.2 — Clone detection, at the first request
**Files:** `apps/server/src/sync/push.rs`
A register restored from a disk image carries someone else's `register_id` **and their bearer token**, so it never registers again and a check at registration cannot observe the threat at all. The enrolled keypair from 3.1.6 is what settles it: the private key is non-exportable, so a restored image cannot prove possession and **fails its first authenticated request** (E.13). Silently accepting it corrupts two registers' receipt sequences.
**Tests:** `a_cloned_image_fails_its_first_authenticated_request` · `device_id_collision_refuses_sync_with_a_named_error` · `a_copied_bearer_token_without_the_key_is_refused` · `a_replayed_signed_request_is_refused` · `an_enrollment_code_cannot_be_reused`

### 3.7.3 — Deactivation propagation and the offline authorization lease
**Files:** `apps/terminal/src-tauri/src/sync/puller.rs`, `crates/pos-domain/src/auth.rs`
A terminated employee's PIN deactivates at next contact. Offline terminals honour a **max-offline-auth window** (E.55) — a real limit of offline-first, and one to disclose to the merchant rather than hide.

**A window nobody can enforce is a disclosure, not a control.** "72 hours" had no state and no algorithm behind it, while conventions §11 forbids branching on device time — so a changed or frozen clock plus a reboot defeated the naïve implementation, and a terminated manager kept refund, drawer and settings authority indefinitely by keeping the register isolated. The mechanism is a **server-signed `authorization_lease`** binding register, grant version, issue time and hard expiry, combined with the persisted maximum trusted timestamp and boot-monotonic elapsed time from `ClockState`. Time is marked suspect after a rollback or an unsupported reboot, and a stale privileged grant **fails closed** when expiry cannot be proved.
**Tests:** `deactivation_applies_at_next_contact` · `offline_auth_window_expires_and_says_why` · `a_clock_rollback_marks_time_suspect_and_fails_privileged_grants_closed` · `a_frozen_clock_cannot_extend_a_lease` · `repeated_offline_reboots_do_not_reset_the_window` · `an_expiry_during_an_open_shift_never_blocks_a_sale`

---

## Group 3.8 — Licensing

*Blueprint §7. A store must not die because a licence server did — and for a point of sale, "read-only" is dying.*

### 3.8.1 — Entitlement files, and what expiry actually blocks
**Depends on:** the commercial decision. **The unit of sale determines what an entitlement asserts**, so it is decided before this microstep, not discovered during it — [`00-master-plan.md`](00-master-plan.md) §6 records running out of runway as a risk for the same reason.
**Files:** `crates/pos-sync/src/license.rs` (new)
Ed25519-signed entitlement files, each carrying a `kid`, the org, the licensed registers or stores, the features, and its validity window.

**Expiry blocks new register enrollment and updates. It never blocks a sale on a register that was entitled when it last synced.** "Degrade to read-only, never lock out" was a euphemism: a register that cannot complete a sale is a closed shop. Worse, it composed badly with a solo vendor — entitlements needing periodic online validation, signed by a key only the vendor holds, mean an unavailable vendor eventually stops every register at every merchant at about the same moment. Entitlements are therefore issued dated to the end of the paid term **plus a stated buffer**, so continuing to trade requires no online validation at all, and non-payment is collected by asking, as every other B2B vendor collects it. The decision, its alternative and the overrule path are in [`ref/security-compliance.md`](ref/security-compliance.md) §7.

The grace period is a merchant-facing term, so it is **a number** in [`ref/merchant-decisions.md`](ref/merchant-decisions.md), not the word "generous".
**Tests:** `tampered_entitlement_is_rejected` · `an_entitlement_for_another_org_is_rejected` · `an_unknown_kid_is_rejected` · `licence_expiry_never_prevents_a_sale_on_an_entitled_register` (E.57) · `expiry_blocks_enrollment_and_updates` · `grace_period_survives_a_long_outage`
**Done when:** no code path prevents a sale on an entitled register, in an open shift or out of one. Prove it with a test that opens a shift, expires the licence, closes the shift, and asserts the next sale still completes.

### 3.8.2 — Entitlement issuance
**Files:** `apps/server/src/licensing/issue.rs` (new), `apps/backoffice/src/pages/licensing/`
Every microstep in this group verified an entitlement and none created one. There was no path from *merchant pays* to *register is entitled*: no issuance action, no key custody, no renewal, and no licence step in the onboarding wizard — while the Phase-5 exit gate is "the product can be sold to someone who is not you".

A back-office action issues, renews and revokes, signing with the entitlement key held per [`ref/security-compliance.md`](ref/security-compliance.md) §6a — **not** as a CI secret. Revocation is by `kid` and by serial. Every issuance writes an audit row naming who issued what to whom, and for how long.
**Tests:** `an_issued_entitlement_verifies_on_the_target_register` · `a_revoked_serial_is_refused` · `two_kids_may_be_valid_during_a_rotation` · `issuance_is_audited_with_actor_org_and_term`

---

## Group 3.9 — Observability

### 3.9.1 — Sentry, scrubbed, consented and regional
**Files:** `apps/terminal/src-tauri/src/telemetry.rs`, `apps/terminal/src/lib/sentry.ts`
Rust panics and JS errors. The Phase-1 scrubber (1.6.8) sits in front of the transport. Events buffer offline, are capped, and **never block selling** (E.59).

Scrubbing is a different question from lawfulness. Sending crash events from a merchant's register to a third party is a disclosed-sub-processor question, and if that processor's region is outside Jordan it is also a transfer question — the same one 3.1.6 answers for sync. So: the processor and its region are named in the sub-processor list, telemetry is a merchant setting that can be switched off, and deployment into an unapproved region is refused rather than defaulted.
**Tests:** `sentry_events_pass_through_the_scrubber` · `offline_telemetry_is_buffered_and_capped` · `no_pii_in_a_captured_panic` · `telemetry_disabled_sends_nothing` · `an_unapproved_region_refuses_to_initialise`

### 3.9.2 — The metrics that predict support tickets
**Files:** `apps/terminal/src-tauri/src/health.rs`
Outbox depth · sync lag · print-failure rate · terminal-timeout rate · uncleared fiscal count and oldest age · unallocated ICV count · build-failed count · crash-free sessions · cold-start time · backup age **per destination** · audit-chain status. Surfaced per device in the back office.

### 3.9.3 — Alert delivery, with thresholds, recipients and acknowledgement
**Files:** `apps/server/src/alerts/` (new), `apps/backoffice/src/pages/alerts/`
**Every metric above terminates in a dashboard a human must open, and "watched daily" is not a control for a solo vendor with pilot stores.** The failures that cost money are silent by design — sync failures deliberately never surface to the cashier — so a register that stopped syncing, stopped backing up, or stopped clearing fiscal documents can run for days undetected. That window is exactly when the money is unprotected.

Each alert is a **number**, a recipient, a channel, and an acknowledgement:

| Alert | Default threshold |
|---|---|
| Register unseen | > 6 h during the store's trading hours |
| Backup age, either destination | > 26 h |
| Backup verification failed | any |
| Outbox depth | the configured depth, ~48 h of accumulation |
| Oldest uncleared fiscal document | > 4 h |
| Unallocated ICV count | growing across two drain cycles |
| Build-failed count | any |
| Dead-letter count | any |
| Audit chain | any break, or a checkpoint fork |
| `rejection_rate_24h` | > 2%, or three consecutive rejections with the same pinned ISTD error code |

Delivery is recorded, acknowledgement is recorded, and an **unacknowledged fiscal alarm escalates** — a queue can grow while checkout still looks perfectly healthy. Thresholds and recipients are per deployment, and the merchant's own recipients are configured with them rather than assumed to be the vendor.
**Tests:** `every_threshold_has_a_recipient_and_a_channel` · `a_breach_produces_a_delivered_alert_with_evidence` · `an_unacknowledged_fiscal_alarm_escalates` · `an_alert_recipient_list_is_never_empty_for_an_enabled_alert`
**Done when:** a deliberately stalled fiscal queue on a test register produces an alert that arrives, is acknowledged, and appears in the delivery record — demonstrated, not asserted.

---

## Group 3.10 — Operating the server

*Nothing in the plan owned this, and Phase 4's exit gate — "three stores trade for a full week with no intervention from you" — depends on all of it. The register has a backup design, a restore drill and a durability assertion; the server had none of the three, while holding the customer records, the consent evidence, the loyalty ledger and the licence endpoint.*

### 3.10.1 — Hosting target, region and the deploy procedure
**Files:** `docs/runbooks/server.md` (new), `apps/server/deploy/`
Name the host, the region and the recipient legal entity. Record the transfer basis relied on, or the fact that no personal data leaves Jordan. Then write the deploy procedure down as commands: how a version reaches the server, how it rolls back, and how you know which revision is running.

"Hosting region is a merchant decision" was not an answerable question on a shared service — it is structurally the vendor's, and the merchant's answer belongs in [`ref/merchant-decisions.md`](ref/merchant-decisions.md) as an acceptance, with a dedicated deployment priced separately if they need one.
**Done when:** `docs/runbooks/server.md` exists and a second person can deploy from it.

### 3.10.2 — Backup, and a restore that has actually been performed
**Files:** `docs/runbooks/server.md`, `apps/server/ops/backup/`
A `pg_dump` schedule plus point-in-time recovery, with a stated RPO and RTO. **The restore is performed into a scratch database and verified**, not configured and believed: an unverified backup is a rumour, and a server backup nobody has restored is a rumour about the merchant's customer records.
**Tests:** `a_restored_scratch_database_passes_the_schema_verifier_and_a_row_count_check`
**Done when:** a restore into a scratch database has been done once, timed, and written into `docs/drills/`.

### 3.10.3 — Migrating with live registers attached
**Files:** `docs/runbooks/server.md`
Registers push and pull while the server migrates. Write down which changes are safe online (additive), which need a window, how the window is announced, and what a register does when it meets a server mid-migration — which is **queue and retry**, never lose a fact and never block a sale.
**Tests:** `a_push_during_a_server_migration_is_retried_not_dead_lettered`

### 3.10.4 — Health checks, and who is on call
**Files:** `apps/server/src/health.rs`, `docs/runbooks/server.md`
Liveness and readiness endpoints that mean something — readiness fails when the database is unreachable, not when the process is merely up. Then the honest part: **name the on-call expectation.** For a solo vendor that is a stated response window during trading hours and an explicit "not overnight", written down and told to the merchant, rather than an implied 24/7 that will be broken the first weekend.
**Done when:** the merchant knows, in writing, what happens when the server is down at 19:40 on a Thursday.

---

## Exit gate

```bash
just lint && just test
cargo nextest run -p pos-sync                # chaos convergence + contract fixtures
cargo nextest run --workspace -E 'test(prop_)'
```

By demonstration:

1. **Provision a second register from scratch** using only the back office and an enrollment code. Someone who is not you does it, from written instructions.
2. **Chaos week** runs end to end; all three convergence properties hold afterwards, over the canonical dump.
3. **Both registers lose the server while selling.** Each completes sales and queues NULL ICVs. On reconnect in both possible orders, the server issues unique store-scoped one-value leases, `allocator_ref` names those leases, and a dropped response replays to the same allocation without changing either local fiscal UUID.
4. **Edit a product centrally** while a cart is open on an offline register. The open cart keeps its price; the next scan gets the new one; the finalized sale from yesterday is untouched.
5. **Both registers sell the last unit offline.** Both sales stand. Stock goes negative and is flagged, not blocked.
6. **Restore register A from a disk image onto new hardware.** Its **first** authenticated request is refused and says why, until re-provisioned.
7. **Push the same UUID twice**: identical bytes are acknowledged as a duplicate; altered bytes are rejected, dead-lettered and alarmed, and the stored row is unchanged.
8. **Create a customer with recorded consent**, earn and redeem points, export their data, exercise an objection, then anonymise them. Their sales still total correctly; the canary identity is absent from every store in the PII inventory; the loyalty ledger rows remain against the anonymised id.
9. **Sign in to the back office as a human**, with MFA, and then attempt to read a second merchant's data through every route in the API surface. Nothing crosses.
10. **Expire the licence during an open shift.** Selling continues — through shift close, and through the next shift. Enrollment and updates are what stop.
11. **Fiscal reconciliation is clean** over the chaos week — no cleared document without a local sale.
12. **Trigger a panic with a customer attached.** The Sentry event contains no name, no phone, no PIN, no PAN.
13. **Restore the server's database into a scratch instance** and verify it. Timed, and written into `docs/drills/`.
14. **Stall a fiscal queue deliberately.** An alert arrives at a named recipient, is acknowledged, and the delivery is recorded.
15. **Automated tests exist** for E.8, E.9, E.10, E.11, E.13, E.37, E.47, E.48, E.55, E.57, E.59, E.87, E.88, E.89, E.90, and for the Phase-3 half of E.12, E.31 and E.91.

→ **Next:** [`phase-4-depth.md`](phase-4-depth.md)
