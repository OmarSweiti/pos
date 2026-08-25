# Sync protocol

Blueprint §4. The component that separates professional POS software from demos, designed around one insight: **different data has different ownership**, so use a different strategy per class instead of one generic conflict resolver.

**The governing constraint:** a cut network cable changes nothing about the ability to sell. Sync is a coordination plane, never a runtime dependency. Every design choice below follows from that.

---

## 1 · Ownership classes

| Class | Examples | Authority | Strategy | Conflicts |
|---|---|---|---|---|
| **Facts** | sales, refunds, tenders, tender settlement events, stock events, cash movements, shift open and close events, Z reports, audit log, loyalty ledger, consent events | the register that created them | append-only `INSERT`, keyed by UUID, grouped by commit | **refused, never merged** |
| **Reference** | products, barcodes, prices, promotions, tax rules, settings, users, roles | server (back office) | pull, last-write-wins by server `version`, tombstones for deletes | server wins |
| **Mutable shared** | customer profile | server arbitrates | field-level LWW + full audit trail; `is_anonymized` is server-authoritative and monotonic | rare, logged |
| **Register-local** | parked carts, in-flight checkout state | the register | **never syncs** | n/a |

90% of writes are append-only facts that cannot conflict. That is why there are no CRDTs here — the machinery would be complexity without benefit.

**"Cannot conflict" is a statement about content, not about UUIDs.** Two registers never mint the same fact, so no merge rule is needed. A UUID that arrives twice is either the same fact replayed — which is the whole point of at-least-once delivery — or a defect. The server distinguishes the two by comparing bytes (§2), and it never merges. Anything that would need merging is not a fact and does not belong in this class.

### Facts that used to be modelled as mutations

Two entities were classified append-only while their design mutated a row after insert: a card tender settles from `pending` to `collected` or `reversed` after the sale closes, and a shift is inserted open and later updated with its count, over/short and Z number. The Phase-3 server revokes `UPDATE` on every fact table, so the two designs could not both be true — the server would have rejected the settlement the register had already performed.

**Both are transition facts.** `tender_status_event` and `shift_close_event` are inserted, never updated, and the current state of a tender or a shift is a **projection** rebuilt from its events — the same relationship `stock_cache` has to `stock_ledger`. The projection may live in a mutable local column for query speed; the *fact* is the event, and the event is what syncs.

This costs one table each and buys three things: the server never has to relax I-4, the settlement of a card that confirmed while the register was offline arrives as an ordinary fact, and a disputed over/short has an event with an actor and a timestamp rather than a column somebody could have edited.

### Per-entity direction

Every table in [`schema.md`](schema.md) is accounted for here, including the ones that never cross the boundary. **An entity with no row is undefined, not defaulted** — an implementer would have to guess, and the two plausible guesses differ by whether a merchant's sales replicate to every till. Microstep 3.1.2 owns the parser that compares the schema's `CREATE TABLE` set with this table; until that implementation lands, completeness is a reviewed plan requirement rather than a current CI claim.

| Entity | Direction | Conflict rule |
|---|---|---|
| `sync_commit`, `fact_commit_member` | up only | permanent envelope and complete manifest; never pruned with delivery rows |
| `sale`, `sale_supply_tax_context`, `sale_line`, `sale_tender`, `sale_line_tax`, `sale_line_discount`, `sale_tax_summary`, `receipt_artifact`, `tender_status_event` | up only | same UUID ⇒ compare canonical bytes (§2); never merged |
| `shift`, `shift_close_event`, `shift_count_line`, `approval_handle`, `approval_consumption`, `audit_log` | up only | append-only evidence; a conflict is a defect, not an update |
| `stock_ledger`, `trade_scale_verification`, `cash_movement`, `cash_count`, `z_report`, `drawer_event` | up only | append-only ⇒ conflict-free |
| `credit_note_context`, `refund_line_link`, `defect_resolution_event`, `document_link`, `stored_value_ledger` | up only | immutable refund/value lineage; never merged |
| `fiscal_document`, `fiscal_payload_event`, `fiscal_queue_event`, `fiscal_result`, `fiscal_reconciliation_issue`, `fiscal_resolution_event` | up only | checkout identity, allocation, transitions, result and operator evidence are facts; the mutable `fiscal_queue` projection never travels |
| `loyalty_ledger`, `promotion_attribution`, `supplier_invoice`, `supplier_invoice_line`, `supplier_invoice_line_tax`, `supplier_invoice_post_event` | up only | append-only ⇒ conflict-free |
| `goods_receipt`, `goods_receipt_line`, `goods_receipt_post_event`, `stock_count`, `stock_count_line`, `stock_count_post_event` | up only once posted | posted facts are immutable; draft UI state is not one of these tables |
| `promotion_version`, `promotion_regulated_exclusion`, `promotion_publication`, `regulated_display_approval` | down only, except register-origin attribution above | immutable publication/approval evidence; the server controls activation |
| `consent_acceptance`, `privacy_tombstone`, `authorization_lease`, `tax_filing_event`, `tax_period_adjustment`, `common_input_allocation`, `tax_credit_ledger`, `tax_filing_election`, `credit_note_period_assignment` | down only | server-origin facts; replay is idempotent and a register never mints them |
| `customer`, `consent_event`, `privacy_request_case`, `privacy_request_event`, `audit_checkpoint`, `org_recovery_envelope`, `stored_value_instrument` | bidirectional | the server accepts immutable events or arbitrates the mutable customer/status fields; anonymization is monotonic |
| `transfer`, `transfer_line`, `transfer_ship_event`, `transfer_receipt_line`, `transfer_receive_event`, `transfer_cancel_event` | bidirectional, column/event-owned | origin and destination own disjoint events and quantities; no blind merge |
| `org`, `store`, `register`, `category`, `tax_category`, `tax_rule_pack`, `tax_rate`, `tax_computation_policy`, `product`, `barcode`, `plu_code`, `embedded_barcode_rule`, `trade_scale`, `tender_type`, `capability`, `app_user`, `role`, `role_capability`, `user_role`, `receipt_template`, `setting`, `tile_grid`, `tile`, `cash_location`, `refund_policy`, `stored_value_policy_version`, `stored_value_policy_current`, `fiscal_spec_package`, `consent_notice`, `privacy_lawful_basis`, `loyalty_tax_policy_version`, `loyalty_tax_policy_current`, `price_list`, `price`, `promotion`, `supplier`, `tax_filing_profile`, `tax_filing_period` | down only | server wins; local emergency edits travel as explicit change requests, never as silent reference upserts |
| `sync_outbox`, `sync_cursor`, `user_session`, `auth_attempt_state`, `parked_cart`, `checkout_operation`, `product_quick_add_request`, `stock_adjustment_request`, `print_job`, `print_attempt`, `doc_sequence`, `fiscal_queue`, `label_reprint_queue`, `fiscal_credentials_ref`, `trusted_time_state` | **never** | transport, security, prepared intent, hardware and retry state belong to one register |
| `stock_cache`, `shift_state`, `tender_status_current`, `stored_value_balance_cache`, `consent_current`, `privacy_request_current`, `loyalty_balance_cache`, `tax_filing_current`, `transfer_current`, `refunded_qty_cache`, `product_fts`, `product_fts_map` | **never** | rebuildable projections and search indexes; syncing one lets stale derived state outlive its facts |
| `reprint_bundle` | server only | immutable on-demand projection; never pretends another register's sale is local |
| `sale_line_new`, `stage_product`, `stage_sale`, `stage_sale_line`, `stage_sale_tender`, `stage_sync_outbox`, `stage_sync_cursor`, `assert_stage_sync_outbox_empty` | migration only | absent after the migration transaction; never enters protocol classification |

**Consent is evidence, not shared mutable state.** Field-level last-write-wins on a consent row means a register that has been offline since Tuesday can overwrite Thursday's withdrawal with Tuesday's grant, and the merchant then markets to somebody who said no — with a record that says they said yes. So consent is an append-only event ledger: a grant is an event, a withdrawal is an event, and the effective state is the latest event the **server** accepted, not the latest device timestamp. Events fan out to every register after ingest, because a withdrawal captured at register A has to be honoured at register B. Until it arrives, B is working from stale evidence, and that is an accepted risk with a bound (§7), not a defect that can be designed away offline.

**The ICV allocator changes location when the server exists.** `doc_sequence` keyed
`(scope_kind, scope_id, kind)` gives `receipt` and `zreport` per-register counters. In Phase 2, which
has no server and supports one register per store, that register locks its own store-scoped
`fiscal_icv` row in-process at first submission and records its register id in `allocator_ref`. From
Phase 3 the server owns the store row; a register requests a one-value lease bound to its
`fiscal_uuid`. A register without a lease queues with `icv IS NULL` and **completes the sale anyway**:
clearance waits, selling never does. [`fiscal-jofotara.md`](fiscal-jofotara.md) §5 owns the complete
allocation rule and the still-open authoritative scope.

---

## 2 · Push — facts up

**The transactional outbox.** Every local business transaction inserts one immutable `sync_commit`,
the complete permanent `fact_commit_member` manifest, one `sync_outbox` delivery row per member, and
the facts themselves in the same SQLite transaction (conventions I-9). The fact graph and its
delivery envelope commit together or not at all. A sale without a manifest would be partial history;
a delivery row without its member would be a phantom.

```
POST /sync/push
{ "protocol_version": 1,
  "schema_version": 7,
  "producer_version": "0.4.0",
  "batch_id": "<uuid>",
  "commits": [
    { "commit_id": "<uuid>",
      "commit_size": 11,
      "commit_hash": "<blake3 of the commit's canonical members>",
      "changes": [ { change_id, commit_index, entity, entity_id, op, payload, payload_hash } ] }
  ] }

200
{ "protocol_version": 1,
  "batch_id": "...",
  "results": [ { "commit_id": "...",
                 "status": "applied" | "duplicate" | "rejected",
                 "reason": null } ] }
```

Every field comes from a durable three-table join, never request-only memory:

```sql
SELECT c.id, c.commit_size, c.commit_hash, c.protocol_version, c.schema_version,
       c.producer_version,
       m.change_id, m.commit_index, m.entity, m.entity_id, m.op,
       m.payload, m.payload_hash,
       o.seq, o.state
  FROM sync_outbox o
  JOIN fact_commit_member m ON m.change_id = o.change_id
  JOIN sync_commit c ON c.id = m.commit_id
 WHERE o.state IN ('pending','retry')
 ORDER BY o.seq, m.commit_index;
```

`sync_commit` owns versions, size and group hash once. `fact_commit_member` permanently owns the
canonical change bytes and index. `sync_outbox` owns only delivery sequence, lease/retry state and
acknowledgement. Acknowledged delivery rows may be pruned; the manifest and envelope remain evidence.

- **Ordering is `(register_id, seq)`.** The outbox's autoincrement, never a timestamp and never a UUID (§4). Within a commit, `commit_index` orders the members.
- **The row's `state` is the transport's truth**: `pending` → `in_flight` → `acknowledged`, with `retry` on a recoverable failure and `dead` after the attempt ceiling. `acknowledged_at` and `pushed_at` are set only on a confirmed 200 for the commit the row belongs to. A crash before the acknowledgement replays; a replay is harmless because application is idempotent.
- The register's identity is **never** read from the body. See §5 rule 4.

### The commit group

The plan alternated between one outbox row per fact and one row per completed sale, and the outbox had no commit identity at all. Neither is harmless: a per-item protocol lets the server keep a sale header and reject one of its lines, and central tax, stock and revenue reports then describe a transaction that never happened locally.

**One delivery row per permanent member, joined through `change_id` to one commit envelope.** Members
written by one local business transaction share `fact_commit_member.commit_id`; size, hash and
versions live once on `sync_commit`, so repeated metadata cannot disagree. A single unbounded payload
was rejected because it would erase per-entity reconciliation, while repeating commit metadata on
every delivery row was rejected because pruning transport would also prune the only manifest.

The rules that make the group mean something:

1. **A commit applies whole, in one server transaction, with foreign keys enforced.** Parents before children *inside* the group, which is a topological sort of the group's own rows, not a global entity order.
2. **A commit with fewer than `commit_size` members is incomplete**, never partially applied. It is held — not dead-lettered — until the missing rows arrive, because they are in the next batch. `commit_hash` then decides completeness by content rather than by count, so a group that is the right size and the wrong contents is also caught.
3. **Acknowledgement is per commit, not per row.** One bad commit is dead-lettered and alarmed without blocking the batch (E.11); its rows stay unacknowledged on the register so the fact is never the server's to lose alone.
4. **A malformed commit leaves zero business rows applied.** That is the assertion, and it is the one worth fault-injecting.

### `INSERT`, not upsert

The blueprint said "the server upserts by UUID, making the call idempotent". A real upsert overwrites an immutable sale, which is I-4 broken by the transport; `ON CONFLICT DO NOTHING` is worse, because it accepts a *different* financial payload under a known UUID and reports success.

```
INSERT … ON CONFLICT (id) DO NOTHING
  0 rows affected  →  compare canonical bytes of the stored row against the incoming payload
                      identical  →  `duplicate`   (the expected replay; nothing changes)
                      different  →  `rejected`    + dead letter + alarm; the stored row is untouched
```

The comparison is over the same canonical serialization the audit chain uses — sorted keys, no whitespace, UTF-8 — so "identical" is a byte fact rather than a field-by-field judgement. `payload_hash` travels with the change so the cheap path is a hash comparison and the expensive path only runs on a mismatch.

A same-UUID-different-payload rejection is not a routine event. It means two different facts were minted under one identifier, and the only causes are a cloned register (§5 rule 5) or a bug. It alarms.

### Envelope versioning and compatibility

Neither envelope carried a version, and the release model's steady state is a staged rollout of registers against one shared server, with registers that may have been offline for months. Two fields, both on **both** envelopes:

| Field | Means | Changes when |
|---|---|---|
| `protocol_version` | the envelope and endpoint shape | the wire shape changes — a new field the other side must understand |
| `schema_version` | the migration `user_version` the payloads were produced by | a synced entity's payload shape changes |

The rules:

1. **The server accepts `protocol_version` N and N−1.** One version of overlap is what a staged rollout needs; two is a maintenance burden nobody pays back.
2. **A version the server cannot serve fails the whole batch with a named reason, and applies nothing.** `protocol_unsupported` is not a rejection of the facts — the rows stay queued on the register, unacknowledged.
3. **A version mismatch never dead-letters a fact.** There is no state in which money is discarded because two binaries disagree about a field name.
4. **A register that is too old keeps selling.** It stops syncing, raises the condition in device health with the version it speaks and the version the server wants, and prompts for an update at a moment that is not mid-sale. It does **not** stop selling — an availability failure caused by a coordination plane is the exact failure this architecture exists to refuse.
5. **A register that is too new for the server** behaves identically. This happens when a register updates before the server does, which a staged rollout makes possible.
6. **On pull, a reference row the register cannot map** stops that entity's cursor rather than advancing past it. Facts already captured are unaffected; the register keeps selling on the catalogue it has.

The envelope shape is frozen in Phase 1, before the first outbox row exists — not in Phase 3 when the pusher is written. A durable queue of unversioned payloads written by an older binary is a migration of queued financial history, and there is no safe time to perform one.

---

## 3 · Pull — reference down

### ICV allocation lease — Phase 3 onward

Phase 2 has no server and uses the in-process allocator specified in
[`fiscal-jofotara.md`](fiscal-jofotara.md) §5. From Phase 3, the server endpoint is:

```text
POST /fiscal/icv/leases
{ protocol_version, schema_version, fiscal_uuid, document_fact_id }

200
{ lease_id, scope_kind: "store", scope_id, icv, issued_at }
```

`org_id`, `store_id` and `register_id` come from the authenticated device principal, never the body.
The server locks `(org_id, 'store', store_id, 'fiscal_icv')`, allocates exactly one ICV, and binds it
immutably to `fiscal_uuid`; replay returns the same lease. The register persists `lease_id` as
`allocator_ref` in `fiscal_payload_event`. A network failure before a durable response leaves
`fiscal_queue.icv IS NULL` and the sale complete; the next request is an idempotent lookup/allocation,
not a second counter advance.

The server keeps a monotonically increasing `version` per row, from one global `change_seq` in Postgres. Every reference table has a `BEFORE INSERT OR UPDATE` trigger assigning it — insert included, or a newly created row sits at version 0 and never pulls — so **the cursor cannot drift because someone forgot to bump it**.

```
GET /sync/pull?entity=product&after=<cursor>&limit=500

200
{ "protocol_version": 1, "schema_version": 7,
  "entity": "product", "changes": [...], "next_cursor": 41822, "has_more": true }
```

- Deletes arrive as **tombstones** (`deleted_at` set), never as absence.
- First run bootstraps from a snapshot, then tails the changelog.
- The register advances `sync_cursor` only after applying a page successfully, in one transaction.
- The entity a caller may pull is checked against the authenticated principal's scope, not taken on trust (§5 rule 4).

**Apply order matters, and it is derived from the foreign keys rather than from memory.** The previous order in this file put `price_list` before `store` and omitted `org` entirely, which fails against `price_list.store_id` and `app_user.org_id` on the first page that contains either. The order below is a topological sort of [`schema.md`](schema.md)'s reference-table references, and it is a **generated** artifact: the test derives it from the DDL and fails when the two disagree, because a hand-maintained order drifts the moment a table gains a column.

```
org
 → tax_computation_policy → tax_rule_pack → store → register → cash_location
 → tax_filing_profile → tax_filing_period
 → tax_category → tax_rate
 → category → product → barcode → plu_code
 → price_list → price
 → promotion → promotion_version
 → role → role_capability → app_user → user_role
 → tender_type → receipt_template → consent_notice → embedded_barcode_rule
 → tile_grid → tile
 → setting → refund_policy → supplier
 → customer
```

Three edges are easy to miss and each fails on the first page that contains it: `store.tax_rule_pack_id` and `store.tax_computation_policy_id` put the jurisdiction pack and the arithmetic policy **before** the store that references them, and `tile` depends on `product` *and* `category` as well as its grid.

The down-flowing entities that are not reference data obey the same rule and sit after their parents: `authorization_lease` after `app_user` and `store`, `stored_value_instrument` after `org`, `consent_event` after `consent_notice` and `customer`, `privacy_tombstone` and `privacy_request_case` after `customer`, `fiscal_result` after the `sale` it clears.

`category.parent_id` is self-referential, so a single page can carry a child before its parent. Within an entity, rows apply parents-first by depth, and a row whose parent is not yet present is **held pending until the end of the page**, then reported if it is still unresolved. Nothing is dead-lettered for arriving early.

**Facts do not apply in any order either.** Append-only ownership removes the need for a *merge* rule; it does not remove referential dependencies — `sale_line` needs its `sale`, a tax row needs its line, a cash movement needs its shift. Facts apply as complete commit groups (§2), parents before children inside the group, and a group whose external parent is missing is held rather than rejected.

**Tests:** `apply_order_respects_dependencies` · `a_commit_group_arriving_out_of_order_is_held_then_applied` · `a_missing_parent_does_not_advance_the_cursor`. The first derives the graph from the schema and also proves each reference table appears exactly once; a hand-maintained count cannot catch a newly added dependency.

**Catalog apply and open carts (E.37).** Applying a catalogue change re-prices **only unfinalized carts**, and even then only for *new* line additions: existing lines keep the price the customer saw on the shelf (conventions I-5). Finalized sales are never touched, under any circumstance (E.9). A *reprice cart* manual action exists for merchants whose policy differs.

**Anonymisation arrives as a `privacy_tombstone`, and it is one-way.** The tombstone is server-issued and monotonic: a register applies it by nulling the PII columns and setting `customer.is_anonymized`, and no register may ever clear it. Applying it twice is a no-op, so a replay is harmless. A restore from an older backup re-applies every outstanding tombstone on the next pull, so a restored register cannot resurrect an erased identity — which is the failure that would turn a completed PDPL erasure into an incomplete one months later. The estate the tombstone has to cover is inventoried in [`security-compliance.md`](security-compliance.md) §2.

### Documents on demand — cross-register reprint

Three documents claimed any register can reprint a sale from facts plus the stored QR, and this file's direction table sends sales up only. Both cannot be true: register B has the fiscal result and none of the sale.

**Reprint from another register is an on-demand fetch, not replication.**

```
GET /sync/document/{sale_id}   →  reprint_bundle
```

- The `reprint_bundle` is the persisted `receipt_artifact` — the exact rendered bytes and their hash — plus the fiscal result. The requesting register renders nothing and recomputes nothing, so a template change cannot alter a historical document, and a QR alone is not a bundle: [`fiscal-jofotara.md`](fiscal-jofotara.md) §10 records why syncing the clearance result down was never sufficient on its own.
- **Permission-gated**, using the same capability the local reprint requires. Reading another till's takings is not a side effect of holding `sale.create`.
- **Never written to the local database and never cached.** A customer-attached sale replicated to every register in the estate is a privacy exposure created for the convenience of an occasional reprint, and it makes the PDPL erasure inventory unbounded.
- **Offline it is unavailable**, with a named error that says so. The register that made the sale can always reprint it; the other one asks the customer to come back to the till they bought from, or the merchant looks it up in the back office. That is a smaller cost than replicating every sale everywhere.

`receipt_artifact` is owned by [`schema.md`](schema.md) and [`hardware-and-receipts.md`](hardware-and-receipts.md); this file owns only the direction it travels and who may ask for it.

---

## 4 · Clocks

**Never order by device wall-clock.** Registers drift and cashiers change the system time.

- Pull ordering comes from server-assigned `version`.
- **Push ordering comes from `(register_id, sync_outbox.seq)`** — the outbox's autoincrement. Not from UUIDv7: a v7 embeds a Unix timestamp taken from the device clock, so ordering by it inherits exactly the error this section exists to exclude. UUIDv7 is used for identity and index locality, and for nothing else. An earlier revision of this section said push order came from UUIDv7, two lines after §2 said it came from `seq`; the outbox is right.
- Acceptance order for anything that fans out — `consent_event`, `fiscal_result` — is assigned by the **server**, because two registers' local orderings are not comparable.
- Device time never supplies causal order. The register may branch on persisted `ClockState` for
  safety decisions such as operator-confirmed business date and deferring a fiscal `issue_date`; it
  still orders sync only by owned sequences and server versions.
- A backward clock jump is an audit entry, not a silent reordering (E.6).

---

## 5 · Operational rules

1. **Sync failures are silent to the cashier and loud to you.** No modal, no error toast. The status strip shows *"Offline — sales are safe and will sync"*; the back office shows last-seen, outbox depth, and sync lag.
2. **Payments are never blocked on sync.** There is no code path in which a card authorisation waits for the server.
3. **Outbox growth alarms** at a configurable depth (~48 h of accumulation) plus a disk-budget check (E.8).
4. **Scope comes from the authenticated principal, never from the request body.** The token resolves to an `org_id`, a `store_id` and a `register_id`; a body field that disagrees with any of them is a rejection, not a hint. Every change's ownership is validated against that scope before it is applied, the entity-direction table above is enforced server-side as an allowlist, and pull is restricted to entities the principal may read. Revocation applies to push, pull, bootstrap and health alike — a revoked token is refused at the next request, not at some later reconciliation.
5. **Clone detection happens at the first authenticated request, not at registration.** A register restored from a disk image already holds the original `register_id` and its bearer token, so it never registers again — checking at enrollment cannot see the threat it was written for. The device token is bound to proof of possession of a non-exportable device key, so a copied image fails its first request and says why (E.13). Silently accepting it corrupts two registers' receipt sequences and produces same-UUID-different-payload rejections downstream. The key material and its custody are specified in [`security-compliance.md`](security-compliance.md) §6a.

### Outbox retention and the long-offline register

The outbox held payloads for ever and recorded only `pushed_at`; the alarm depth was specified and nothing said what happens on day 30. A register that trades offline for a month carries a second copy of every fact as JSON, inside a file that every backup copies and every migration walks.

| Rule | Why |
|---|---|
| A row is **prunable** only in `state = 'acknowledged'` with `acknowledged_at` durably committed | Deleting on a hopeful 200 turns a lost acknowledgement into a lost fact |
| Acknowledged rows are pruned in bounded batches on a background task, oldest first, never during checkout | An unbounded `DELETE` on a busy register is a checkout stall |
| `receipt_artifact` payloads are the largest thing the outbox carries, so they are pruned first and measured separately | A month of receipt bitmaps is a different order of magnitude from a month of sale headers |
| **Pruning deletes the copy, never the fact.** The source rows are immutable and stay | The outbox is a transport, not an archive |
| The register keeps a byte budget for the unacknowledged queue, sized from the merchant's stated worst-case offline window (30 / 90 / 365 days) | "It filled up" must be a threshold somebody chose, with an alarm before it, not a surprise at the disk-space guard |
| Crossing the alarm threshold raises device health; crossing the hard budget **still does not block a sale** — the disk-space guard (E.5) is the only thing that may, and it does so for the database, not for the queue | A queue that blocks selling has inverted the architecture |
| WAL growth, checkpointing and `VACUUM` behaviour are measured against the soak dataset, not assumed | Pruning that leaves the file the same size on disk has solved nothing the backup notices |

Sizing the budget needs a payload size the plan does not yet have; the measurement belongs to the volume soak, and the number it produces belongs in a merchant-facing setting.

---

## 6 · Contract tests and chaos

### Contract fixtures — `crates/pos-sync/tests/fixtures/`

Client and server test against the **same** JSON fixtures. This is what stops the two sides drifting into a shared misunderstanding that only production reveals.

```
fixtures/
├── push_commit_sale.json
├── push_batch_mixed_commits.json
├── push_response_partial_failure.json
├── push_commit_incomplete_group.json
├── push_conflict_same_uuid_different_payload.json
├── push_protocol_version_unsupported.json
├── pull_products_page1.json
├── pull_products_with_tombstone.json
├── pull_reference_dependency_order.json
└── pull_category_child_before_parent.json
```

**Tests:** `client_and_server_agree_on_every_fixture` · one fixture per supported `protocol_version`, so N and N−1 are both exercised rather than assumed.

### The chaos harness — `crates/pos-sync/tests/chaos.rs`

Two simulated registers plus a server. The harness:

- replays batches;
- drops responses **after** the server applied them (the nastiest case — the client does not know it succeeded);
- duplicates pushes;
- reorders pulls;
- delivers a commit group split across batches, and out of order;
- partitions one register for simulated days;
- restarts processes mid-batch;
- corrupts one payload to exercise the dead letter;
- pushes a known UUID with an altered amount, to exercise the conflict rejection.

**The properties.** The previous single property — `prop_both_databases_converge_byte_identical` — could not hold and could not be built. Facts travel up only, so register A never receives register B's sales and the two fact sets are disjoint *by design*; the register is SQLite and the server is Postgres, so `BLOB`/`UUID`, `TEXT`/`TIMESTAMPTZ` and JSON text/`JSONB` differ deliberately; and `stock_cache`, `parked_cart` and the outbox's own autoincrement are local by definition. Anyone implementing it would have had to weaken it, which is worse than never having claimed it.

Three checkable properties replace it:

| Property | Asserts |
|---|---|
| `prop_server_facts_equal_the_union_of_register_outboxes` | for every fact each register produced, the server holds exactly one row with identical canonical bytes — and holds nothing else |
| `prop_reference_tables_converge_across_all_three_nodes` | every caught-up register's reference state projects identically to the server's, entity by entity |
| `prop_apply_is_idempotent_under_any_replay_order` | any permutation and duplication of a delivered batch sequence produces the same final state |

### The canonical dump

"Canonical" was doing a lot of unexamined work, so it is specified here rather than at the keyboard.

`crates/pos-sync/src/canonical.rs` produces, for one node, a deterministic text projection:

| Included | Excluded, and why |
|---|---|
| every fact table in the direction table above | everything in that table's two **never** rows — register-physical state, local security state, and the transport itself |
| every reference table in the apply order above | every `*_cache` and `*_current` projection: rebuildable, and compared against its own source by its own property. Comparing a cache across nodes tests the cache, not convergence |
| `consent_event`, `privacy_tombstone` | `audit_log.seq` — a local autoincrement; the row's `id` and chain hash carry its identity |
| `fiscal_result`, `receipt_artifact` (content hash, not bytes) | `doc_sequence` — allocation mechanics differ by phase and are proven through immutable payload events, not counter-row equality |
| `audit_checkpoint` | `fiscal_queue.attempts`, `next_attempt_at`, `last_error`, `claimed_at` — local retry bookkeeping, not the document |
| | `print_attempt.started_at`/`finished_at` — device timings, compared by outcome rather than by clock |

Normalisation, per column type, so two engines can be compared at all: UUIDs as lower-case hyphenated text · timestamps as ISO-8601 UTC with milliseconds · booleans as `0`/`1` · JSON re-serialized with sorted keys and no whitespace · money and quantity as decimal integers, never floats. Rows sort by primary key; tables sort by name.

The projection is **semantic, not byte-level**. A difference in it is a real disagreement about a fact or a reference row, which is the only kind of difference worth failing a phase gate over.

### The chaos generator

A property over fault sequences on a stateful three-node system is not a proptest default, and 256 random cases from an unspecified strategy is a green tick rather than evidence.

- The fault alphabet is **bounded and enumerated** — the list above, nothing implicit.
- Sequences come from a **seeded** RNG. The seed is printed on every failure and committed to `crates/pos-sync/tests/seeds/` the moment it finds something, so the case becomes a permanent regression rather than a story about a Tuesday.
- **No shrinking.** Shrinking a fault sequence across processes produces a different execution; replaying a recorded seed does not.
- Sequence length is bounded, and the bound is stated in the test so a reviewer can see what was not explored.

### The offline week

A scripted seven-day scenario: register A offline for three days while B trades; the catalogue edited centrally throughout; both registers selling the last unit of a product; a refund attempted at both; a price change mid-week; a consent withdrawal captured on A while B still holds the grant.

**Asserted:**
- the three convergence properties hold afterwards;
- both sales of the last unit stand; stock goes negative and is flagged, not blocked (E.12) — **inventory is a ledger, not a lock**;
- the serial-refund attempt is caught when connected (E.31);
- receipt sequences on A have no gaps;
- A's fiscal documents queue with `icv IS NULL` and are allocated, in order, on reconnect;
- the withdrawal wins on both registers once it has synced, whatever the device timestamps say.

---

## 7 · The accepted risks, stated plainly

Offline-first buys availability and pays for it in specific, bounded ways. **Say these to the merchant** rather than implying they are impossible:

| Risk | Bound | Mitigation | Residual |
|---|---|---|---|
| **E.31** — the same receipt refunded at two stores inside the offline window | one offline window | server-side remaining-refundable check whenever connected; refunds-by-user report | real, small, visible after the fact |
| **E.12** — two registers sell the last unit offline | one offline window | negative stock allowed and flagged; negative-stock report | intended behaviour, not a bug |
| **E.55** — a terminated employee's PIN works until next contact | max-offline-auth window setting | window configurable; deactivation applies at next contact | real; the window is the merchant's choice |
| **E.61** — gift card redeemed offline at two stores | explicit opt-in only | stored value is **online-authorize-only by default**; an offline cap exists only as quantified, accepted risk | zero unless deliberately enabled |
| **E.63** — a photocopied single-use coupon redeemed twice offline | one offline window | codes marked used on redemption sync; promo report surfaces it | real, small |
| A consent withdrawal captured on one register is not honoured on another until it syncs | one offline window | consent is an event ledger, server-ordered; marketing runs from the server's effective state, never a register's | real; no offline design removes it |
| From Phase 3, a register cannot obtain a server ICV lease offline, so clearance waits | one offline window | the sale completes with `icv IS NULL`; the queue requests its one-value lease on reconnect; `oldest_uncleared_age` is on the status strip | real, and the reason selling is never gated on it; Phase 2's single register allocates locally |
| A register too old for the server stops syncing | until it is updated | it keeps selling; device health names both versions; no fact is ever discarded for a version reason | real; the alternative is refusing to trade |
| Cross-register reprint is unavailable offline | while offline | the originating register can always reprint; the back office holds the journal | deliberate — the alternative replicates every sale everywhere |
| A pruned outbox row means the register is no longer a second copy of that fact | after durable server acknowledgement | pruning requires a durable acknowledgement; the source fact rows are immutable and stay | the server's backups are the second copy from that point |

A vendor who claims none of these exist is either not offline-first or not being straight with you.

---

## 8 · Build vs. buy

The protocol above is the classic pattern real POS vendors run.

The credible alternative is **PowerSync** — an open-source, production-grade Postgres↔SQLite bidirectional sync engine with first-class offline write queues, explicitly aimed at retail POS. The trade-off is decisive here: its client SDKs are JS/Flutter/Kotlin/Swift-centric, so in a Tauri app the synced SQLite would live **webview-side** — on the wrong side of the boundary this whole architecture exists to draw. The Rust core would no longer own the database.

**Decision: build the custom outbox** (Phase 3), and budget it as a first-class subsystem rather than a late add-on. Calling it "a few thousand lines of `pos-sync`" understated it: the durable envelope is written in `pos-db` from Phase 1, the commit group shapes the finalize transaction, version compatibility shapes release engineering, and the convergence definition shapes both schemas. Switching engines later would not be a product rewrite, but it would be considerably more than `pos-sync`.

Revisit only if multi-entity partial sync across many stores starts eating real engineering time.

ElectricSQL is worth watching but is read-path-first today.
