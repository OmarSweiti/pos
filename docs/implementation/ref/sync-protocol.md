# Sync protocol

Blueprint §4. The component that separates professional POS software from demos, designed around one insight: **different data has different ownership**, so use a different strategy per class instead of one generic conflict resolver.

**The governing constraint:** a cut network cable changes nothing about the ability to sell. Sync is a coordination plane, never a runtime dependency. Every design choice below follows from that.

---

## 1 · Ownership classes

| Class | Examples | Authority | Strategy | Conflicts |
|---|---|---|---|---|
| **Facts** | sales, refunds, tenders, stock events, cash movements, Z reports, audit log, loyalty ledger | the register that created them | append-only push, keyed by UUID | **impossible** |
| **Reference** | products, barcodes, prices, promotions, tax rules, settings, users, roles | server (back office) | pull, last-write-wins by server `version`, tombstones for deletes | server wins |
| **Mutable shared** | customer profile | server arbitrates | field-level LWW + full audit trail | rare, logged |
| **Register-local** | parked carts | the register | **never syncs** | n/a |

90% of writes are append-only facts that cannot conflict. That is why there are no CRDTs here — the machinery would be complexity without benefit.

### Per-entity direction

| Entity | Direction | Conflict rule |
|---|---|---|
| `sale`, `sale_line`, `sale_line_tax`, `sale_line_discount`, `sale_tax_summary`, `sale_tender` | up only | none possible; dedupe by UUID |
| `stock_ledger`, `cash_movement`, `drawer_event`, `shift`, `z_report`, `audit_log` | up only | none possible |
| `loyalty_ledger`, `stored_value_ledger` | up only | append-only ⇒ conflict-free |
| `fiscal_queue` | up (state reporting only) | server observes, never commands |
| `fiscal_result` | up (submission) / **down** (QR to other registers for reprint) | server-authoritative |
| `product`, `barcode`, `category`, `tax_category`, `tax_rate`, `price_list`, `price`, `promotion`, `setting`, `app_user`, `role`, `role_capability`, `store`, `register` | down only | server wins; **local emergency edits sync up as change-requests**, flagged for approval, never silently merged |
| `customer`, `consent` | bidirectional | field-level LWW + audit trail |
| `parked_cart` | **never** | register-physical concept |

---

## 2 · Push — facts up

**The transactional outbox.** Every local fact write appends to `sync_outbox` *in the same SQLite transaction* as the write itself (conventions I-9). Sale and outbox row commit together or not at all. A sale that exists without its outbox row would never sync; an outbox row without its sale is a phantom.

```
POST /sync/push
{ "device_id": "...", "batch_id": "<uuid>", "changes": [ { entity, entity_id, op, payload } ] }

200
{ "batch_id": "...",
  "results": [ { "entity_id": "...", "status": "applied" | "duplicate" | "rejected", "error": null } ] }
```

- **Upsert by UUID ⇒ idempotent.** A retry after a timeout is safe, which is the whole reason timeouts are survivable.
- **Per-item acknowledgement.** One poison row cannot block a batch (E.11). Rejected items go to a server-side dead letter with an alert; the queue keeps moving.
- **`pushed_at` is set only after a confirmed 200.** A crash before the acknowledgement replays; a replay is harmless.
- Batches drain in `seq` order — the outbox's autoincrement, not a timestamp.

---

## 3 · Pull — reference down

The server keeps a monotonically increasing `version` per row, from one global `change_seq` in Postgres. Every reference table has a `BEFORE UPDATE` trigger assigning it, so **the cursor cannot drift because someone forgot to bump it**.

```
GET /sync/pull?entity=product&after=<cursor>&limit=500

200
{ "entity": "product", "changes": [...], "next_cursor": 41822, "has_more": true }
```

- Deletes arrive as **tombstones** (`deleted_at` set), never as absence.
- First run bootstraps from a snapshot, then tails the changelog.
- The register advances `sync_cursor` only after applying a page successfully.

**Apply order matters.** Reference data applies in dependency order:

```
tax_category → tax_rate → category → product → barcode
             → price_list → price → promotion
             → role → role_capability → app_user → user_role
             → store → register → setting
```

Facts apply in any order — that is what append-only buys.

**Catalog apply and open carts (E.37).** Applying a catalogue change re-prices **only unfinalized carts**, and even then only for *new* line additions: existing lines keep the price the customer saw on the shelf (conventions I-5). Finalized sales are never touched, under any circumstance (E.9). A *reprice cart* manual action exists for merchants whose policy differs.

---

## 4 · Clocks

**Never order by device wall-clock.** Registers drift and cashiers change the system time.

- Pull ordering comes from server-assigned `version`.
- Push ordering comes from UUIDv7 plus append-only semantics.
- Device time is recorded **for humans to read**, never branched on.
- A backward clock jump is an audit entry, not a silent reordering (E.6).

---

## 5 · Operational rules

1. **Sync failures are silent to the cashier and loud to you.** No modal, no error toast. The status strip shows *"Offline — sales are safe and will sync"*; the back office shows last-seen, outbox depth, and sync lag.
2. **Payments are never blocked on sync.** There is no code path in which a card authorisation waits for the server.
3. **Outbox growth alarms** at a configurable depth (~48 h of accumulation) plus a disk-budget check (E.8).
4. **Device tokens are register-scoped and revocable.** A revoked token is refused at push, not at some later reconciliation.
5. **Clone detection** at registration: a device-fingerprint collision refuses to sync until re-provisioned (E.13). Silently accepting it corrupts two registers' receipt sequences.

---

## 6 · Contract tests and chaos

### Contract fixtures — `crates/pos-sync/tests/fixtures/`

Client and server test against the **same** JSON fixtures. This is what stops the two sides drifting into a shared misunderstanding that only production reveals.

```
fixtures/
├── push_batch_sale.json
├── push_batch_mixed_facts.json
├── push_response_partial_failure.json
├── pull_products_page1.json
├── pull_products_with_tombstone.json
└── pull_tax_rules_dependency_order.json
```

**Test:** `client_and_server_agree_on_every_fixture`.

### The chaos harness — `crates/pos-sync/tests/chaos.rs`

Two simulated registers plus a server. The harness:

- replays batches;
- drops responses **after** the server applied them (the nastiest case — the client does not know it succeeded);
- duplicates pushes;
- reorders pulls;
- partitions one register for simulated days;
- restarts processes mid-batch;
- corrupts one payload to exercise the dead letter.

**The property:** `prop_both_databases_converge_byte_identical`. After any scripted fault sequence, a canonical dump of both register databases and the server is byte-identical for every fact and reference table.

### The offline week

A scripted seven-day scenario: register A offline for three days while B trades; the catalogue edited centrally throughout; both registers selling the last unit of a product; a refund attempted at both; a price change mid-week.

**Asserted:**
- both convergent afterwards;
- both sales of the last unit stand; stock goes negative and is flagged, not blocked (E.12) — **inventory is a ledger, not a lock**;
- the serial-refund attempt is caught when connected (E.31);
- receipt sequences on A have no gaps.

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

A vendor who claims none of these exist is either not offline-first or not being straight with you.

---

## 8 · Build vs. buy

The protocol above is a few thousand lines of Rust and is the classic pattern real POS vendors run.

The credible alternative is **PowerSync** — an open-source, production-grade Postgres↔SQLite bidirectional sync engine with first-class offline write queues, explicitly aimed at retail POS. The trade-off is decisive here: its client SDKs are JS/Flutter/Kotlin/Swift-centric, so in a Tauri app the synced SQLite would live **webview-side** — on the wrong side of the boundary this whole architecture exists to draw. The Rust core would no longer own the database.

**Decision: build the custom outbox** (Phase 3). It is small, it teaches the invariants, and it keeps the database where it belongs. Revisit only if multi-entity partial sync across many stores starts eating real engineering time — and if it does, the cost of switching is a `pos-sync` rewrite, not a product rewrite, because nothing above `pos-sync` knows how bytes reach the server.

ElectricSQL is worth watching but is read-path-first today.
