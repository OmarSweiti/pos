# Implementation documentation

The buildable plan for this POS: what to type, in what order, and how you will know it worked.

---

## Start here

1. **[`00-master-plan.md`](00-master-plan.md)** — the spine. Verdict on the master plan, the phase map, the risk register, the long-lead register, and **§4a, the errata and concordance**: every correction to the immutable source plans, every superseded name, and every open item that can still change an architecture. **25 minutes**, and §4a is the part you cannot skip if you have read `docs/plan/` first.
2. **[`01-conventions.md`](01-conventions.md)** — the engineering law. Nine invariants, naming, errors, testing, definition of done. **Read once, then keep it open.**
3. **[`02-development-workflow.md`](02-development-workflow.md)** — how the work gets done: every command, the feature lifecycle, manual testing, the drills, the rituals. **Keep it open next to the law.**
4. **[`03-github-workflow.md`](03-github-workflow.md)** — how the work gets tracked and shipped: the four branches, issues, the board, pull requests, the two release channels. **Read §3 before your first push** — it says which rules a machine enforces and which are only written down.
5. **[`phase-0-closeout.md`](phase-0-closeout.md)** — the dated Phase-0 close-out record. It is
   historical evidence, not a current setup or GitHub runbook; use files 2–4 above for live commands
   and repository posture.

Then work the phase you are in, consulting `ref/` as the microsteps point you there.

**Current implementation frontier (24 September 2026):** Phase 0 is closed by transfer: `0.3.2`
remains open in [`phase-0-closeout.md`](phase-0-closeout.md), with updater signing owned by
microstep `5.5.0`.
<!-- frontier:begin phase=1 -->
Phase 1 has **82 of 124 executable microsteps fully complete (~66%)**: `1.1.0`
(the shared property harness), `1.1.1` (`Currency`), `1.1.2a` (`Money` carries `Currency`), `1.1.6`
(`RoundingRule` and the one rounding point), `1.1.3` (`Qty` in milli-units), `1.1.4` (`Percent` in
parts-per-million), `1.1.2b` (`Money` arithmetic and formatting), `1.1.7` (migration `0002`,
shipped earlier), `1.1.5` (the complete money property suite), `1.1.8` (the fifteen typed ids,
the `IdSource` port and `SeqIdSource`), `1.2.1` (migration `0003`, the STRICT rebuild and the
org / store / register / taxonomy tables), `1.2.2` (`Product`, `UnitOfMeasure` and the regulated
pair), `1.3.1` (the tax engine's value and evidence types), `1.8.9` (the outbox writer, so every
fact graph commits with its delivery envelope), `1.8.5` (the release build's key policy, proven
in a release build), `1.11.2` (the RTL lint, whose escape hatch now requires its reason), `1.6.5`
(the audit hash chain, and the external anchor that closes what a chain alone cannot detect),
`1.6.1` (migration `0004`, the people and audit tables, and a role/capability matrix in which a
denial is a row rather than an absence), `1.6.3` (the capability grid, and the test that holds
all 128 seeded cells to it), `1.11.0` (the register's DOM component-test harness — a jsdom
environment whose document fixture is `index.html` itself, `renderWithProviders`, explicit
cleanup, and the bridge that makes a fake clock usable), `1.11.3` (the formatting helpers —
a transaction amount at the currency's own exponent, a catalogue form that shortens only where
shortening is exact, and Western Arabic digits in an Arabic locale), and `1.9.1` (migration
`0005` — immutable shift opens with one open shift per register, the seven gates a sale must pass
to become a fact, append-only tender settlement, immutable receipt artifacts and their print
queue, `trusted_time_state`, and the scoped `doc_sequence` both counters share), and `1.1.9`
(complete at last — its deferred database half persists `ClockState` across the restart that is
exactly when a wrong clock arrives), `1.2.6` (a register with no FTS5 is refused at `open`,
because the alternative is a catalogue search that answers "we do not stock it" for everything),
`1.11.6` (global scan capture — a keystroke stream split into scans and typing by inter-key
timing alone, routed whatever holds focus, and the one character that physics forces into the
search box before a burst can be recognised taken back out of it), `1.11.11` (the keyboard map —
every action in `ref/ui-spec.md` §7 reachable without a mouse, with the keys that mean something
inside a text box yielding to one that has focus), `1.9.2` (the owned document counters — a
number spent only by a transaction that commits, and G-2's real requirement met: a gap that does
happen is *detectable* rather than pretended impossible), `1.7.2` (the embedded typeface — IBM
Plex Sans Arabic in two faces and its OFL licence, committed rather than fetched, so the receipt is
drawn with the file the screen uses), `1.6.6` (the audit repository — a row whose `seq` is
fixed before the hash is taken over it, a chain that cannot fork because the envelope holds the
write lock before the head is read, and a read path that walks a row written by a build it has
never heard of rather than refusing it), `1.6.6b` (`verify-audit` — the forensic command that
turns the chain into evidence somebody can run, reporting the first break, refusing to answer
"verified" about a database it would have had to create, and detecting against an external anchor
the one tamper a chain cannot see in itself), and `1.11.1` (the i18n infrastructure — a typed
catalogue whose two languages cannot drift apart, Arabic made the rendered default by the entry
point rather than by the document it was served, and the screen wired to the very font file the
receipt rasteriser embeds, proven by comparing what each language's source names), and `1.7.1`
(the receipt render model — one pure description of the document three renderers read, with the
store's shelf-display precision deliberately absent from it so no renderer can hide a fil, and
five whole-document rules refused rather than hoped for, including the one that stops a
non-fiscal acknowledgement printing a fiscal QR), and `1.7.3` (the raster pipeline — Arabic that
joins, a Latin SKU and a Western-digit price that keep their own direction inside a right-to-left
line, both paper widths, and every measurement an integer so the same receipt is the same dots on
any machine), and `1.7.4` (the ESC/POS emitter — a document that is an initialise, one raster and a
cut, carrying the bitmap's bytes unchanged and the drawer pulse nowhere, because a stream is
persisted and retried and a pulse is cash moving), and `1.5.1` (the tender vocabulary — six kinds
written to match a seed that shipped before them in a migration nobody may reopen, and the one
distinction a shift closes short without: an internal tender is not merely a tender that counts no
drawer cash), and `1.5.3` (cash rounding — a final cash tender asked for the remainder moved to the
store's coin step while a card is charged the exact amount, the signed difference kept as a figure
of its own so the books reconcile to the fil, and change refused rather than rounded), and `1.5.4`
(the dinar's denominations — the plan's eleven pieces for the quick-keys and the float and close
grids, no table borrowed across currencies, and a coin set its own first test showed to be
inconsistent with a ten-fil step, recorded and put to the merchant rather than guessed), `1.7.9`
(raster strips — the printer profile made §6a's record as a type, a page taller than its device
takes sent as consecutive raster commands whose payloads join to the bitmap unchanged, `GS ( L`
held to its own 16-bit count as well as the device's limit, and a page rendered for one paper
refused by a device of another), `1.7.10` (the network printer — ESC/POS over TCP 9100 behind a
`Transport` trait, every socket event classified as one of the four outcomes, `DLE EOT` status
parsed and *unsupported* when a device never answers, and the drawer pulse taken from the profile
on a connection of its own), and `1.7.11` (device profiles as data — seventeen versioned
files seeded from the hardware lab, every value with its source and `unknown` exactly where the
seed has one, a printer bound to a register only when it knows every parameter its driver needs,
and a committed hash per version so a value changes only as a new version), and `1.7.7` (the print
retry queue — a sale's original and its one job written in the sale's own transaction, a worker
that sends nothing it cannot vouch for and retries only a refusal, a lapsed lease resolved as
unknown rather than sent twice, and a person's answer that turns an ambiguous print into a
watermarked duplicate of the original, audited, with no drawer ever pulsed by a retry), and
`1.10.1` (migration `0006`, the stock ledger — an event that is never edited, carries the cost
basis it was captured at and projects into its cache in the same statement; a sale that moves
stock completes only with each line's exact stock fact in its commit; a price label's stock moves
by the weight it implies, flagged as derived; and the three guards #197 tracked since #179), and
`1.2.5` (migration `0007` — Arabic catalogue search over the two projections `0003` generates, so a
name stored with tashkeel is found by the plain spelling a cashier types and that spelling ranks
first, with the query-side fold held to the SQL by a property; and scale pricing that cannot be
switched on without current verified evidence and switches off the moment it is lost), and `1.2.3`
(the product repository — every way a cashier reaches a product, by id, barcode, PLU or typed
search, and one save gated by the domain's own check, so a price above the ministry ceiling never
reaches the catalogue; a search no keystroke can turn into FTS5 syntax, proven over the whole
metacharacter corpus and by a property that sends arbitrary text through it; a barcode with one
live owner, a second claim refused by name; and `UnitOfMeasure` cut to one unit per stored
dimension, so a product survives the database exactly), `1.2.7` (the search benchmark —
50 000 Arabic products saved through the repository, typed into at 1, 2, 3 and 5 characters with
the first keystroke matching every one of them, and the two tests that hold the benchmark to that
catalogue and those queries; it claims no budget, which is 5.8.2's, and its first indicative run
fired a risk-register trigger, recorded at the microstep), `1.2.8` (the scan parser's fuzz
target — arbitrary bytes through `parse_scan` against every layout a store could save, failing on
a panic, a loop or a clean parse of a damaged label, from a committed corpus that a `pos-domain`
test replays on every pull request while the search itself stays off the merge path), and `1.10.2` (the
stock repository — an event prepared from the position it leaves and appended only under an envelope
that carries exactly its payload, the cost basis in force captured on every outgoing event, a
different cost into positive stock refused rather than blended until 4.2.2, the cache rebuilt from the
ledger, and every companion row rolled back by a fault in front of each of the transaction's seven
writes), `1.3.2` (rate resolution — the components a line's category carries in a store's profile
at an instant, from-inclusive and to-exclusive, a scoped rule overriding an unscoped one, and an
overlap or an absence refused rather than a rate guessed), `1.3.3` (exclusive line tax — the
net times the rate, rounded once by the caller's rule and added exactly, every supply but an
ordinary domestic one refused, and every line a later step computes refused by name rather than
approximated), `1.3.5` (several components on a line — applied in order, each charged on the
net or on the net plus the components it names, a fixed amount per unit beside or instead of a
rate, every component rounded once on a base it can be re-derived from, a dependency cycle or an
incomplete zone pack refused, and a tax-inclusive price on several components refused until a
source says how it divides), `1.3.6` (the receipt tax summary — the exact sum of the carried
components, one row per rate, so an exempt line and a zero-rated one are two rows although neither
carries tax, a zero-rated or exempt component that charges anything refused, and a line that no
longer adds up refused rather than summarized), `1.3.8` (a store not registered for GST
charging none: its lines carry no component and its receipt no tax rows, while whether it owes an
electronic invoice stays a separate setting), `1.4.1` and `1.4.2` (the cart's first types — a
sale being built or parked, its lines with the name and price they were captured at, a cart that
opens domestic, on its shift and knowing whether it is training, and every refusal a cart can return),
`1.4.3` (adding, changing and voiding a line — by product or by scan, never at a price the caller
chose, an age-restricted line held blocked until confirmed, and a void that says who and why),
`1.2.4` (the scan path, completed now that a price label becomes one package in the cart),
`1.4.4` (parking a cart and resuming it exactly as it was — only at its own till and in its own
training mode, and rung up in the shift that takes the money), `1.4.5` (a manual discount on a
line, never more than the line is worth and never above the actor's cap without a manager's bound
approval, audited either way, with the caps the owner adopted — 5% for a cashier, 15% for a shift
lead — in the shipped role grid), `1.4.6` (a discount on the whole basket, divided among its
lines to the fil by largest remainder, the tie decided by what a line is and never where it sits),
`1.4.7` (a price override, never below the product's cost unless the shelf showed that price,
never above a ministry ceiling whoever approved it, audited either way, with the shelf's label sent
to be printed again), `1.4.9` (`price_cart`, the one function that turns a cart into money: each
line at the price it charges, less its discounts, taxed under the store's price mode, the same in any
scan order, with its 200-line benchmark), and `1.4.8` with `1.5.2` (paying for a sale — priced by the
transition that begins payment, so it is never paid against another cart's total; split across any
tenders, with only the cash tender that settles it rounded, and change from cash alone; completed
only when settled, voided only before any money is taken, and never changed once complete), and
`1.4.10` (every cart transition in one list with its audit duty, checked against its signature by the
compiler; every one that reverses money returns its audit intent, a collected tender taken back
included), and `1.8.0` (the storage-engine gate: every open reads SQLite's and SQLCipher's versions
from the engine and refuses an unreviewed one before WAL; the build without the WAL-reset fix opens
in single-connection mode; and the minimums are pinned to the digest of the passage that sourced them),
and `1.8.1` (transaction discipline held by a test: every shipped write takes the caller's
transaction, none begins or ends one of its own but the migration runner, and a fact write handed
only a connection does not compile), and `1.6.8` (the never-log registry, one constant read by the
log scrubber and by the audit-payload guard alike; every log line scrubbed by field name at any depth;
and a command's error that hands the webview a code, never its source), and `1.6.7` (the IPC command
registry — every command the shell registers named once, for the handler and for the walk; each one
proving a capability or saying why it proves none; and every approval a command takes bound to one
entity, one amount and its reason, with the department sale's threshold held at its boundary), and `1.8.1b`
(a file whose schema is not the one its recorded version creates refused by name and left as it was
found — on either side of an interrupted step, part-way through one, or written by a newer build —
and the forensic verifier still reading such a copy, naming every object that differs), and `1.8.2`
(the completed sale written as one prepared fact graph inside its envelope and read back field for
field, with no method that updates or deletes one, and migration `0009` giving every fact the sale
captured a column of its own), and `1.8.2b` (a parked basket kept on the register's own disk through
any restart, claimed by one session when it is resumed, carried through a process death with every
change made to it, and removed only by the sale it becomes; resumed by whoever holds `sale.resume`,
with another cashier's basket audited, and expired at the end of its business day), and `1.10.3`
(the stock cache held to its ledger — after any sequence of events the cache projected one event at a
time is the full replay and the domain's sum, a rebuild leaves every row byte for byte as it was, and a
cache that has drifted in any column is found at verification, named product by product, and rebuilt
in the same transaction, or left whole when the rebuild fails), and `1.10.4` (negative stock allowed and
flagged by default — a basket screened before tender, every product it would take below zero named, the
sale completed and its event recording the position it left — and refused before tender, never at
finalize, under a store's hard block; with two offline registers both selling the last unit, because
each can screen only its own ledger), and `1.8.2c` (the register's database opened once at start-up
with its key, for the register this install is, or the till kept closed and the reason named; a
crash-left session closed and parked carts reclaimed before the first command; a PIN sign-in under
the attempt policy, recorded and audited, holding the grants of the user's one role at this store;
and one command context that refuses a command with no one signed in, signs out a deactivated user,
and resolves a stored approval by id and holds it to its binding), and `1.6.4` (`Authorized<C>` and
the one-use `ApprovalHandle` bound to one actor, operation and amount, stored and consumed in the
effect's transaction and resolved by the command context, and, as the owner decided on 5 November,
refused once its approver is deactivated), and `1.4.3b` (an age-restricted line kept on screen and
blocked until an operator answers, yes opening its gate and no removing it, each answer audited, now
through the register's `cart_confirm_age`), and `1.9.3` (receipt numbering — `REG01-000123`, the
register's code and a hyphen before its own counter, so two registers counting offline to the same
value never issue the same number, and a prefix fixed by the counter's first number, so a register
whose code changes keeps numbering where its receipts left off), and `1.9.4` (a sale dated by its
shift's business day, never by the clock at finalize — a sale at 01:00 belongs to the trading day that
began the evening before, and neither a zone that moves nor a clock that is wrong moves it), and
`1.8.4` (crash recovery for a sale mid-payment — journalled before its first irreversible effect,
each step under the version it read, and after a restart taken up by a cashier from its journal
alone: a checkout with nothing taken goes back to its basket, one with tenders taken comes back with
every one, and a finalize begun replays under its own key, never posting a stock event or a delivery
row twice), and `1.6.2` (PINs under a reviewed Argon2id profile, with wrong PINs waiting longer and
then locking a user out until a manager releases them, and a forgotten PIN reset by one manager for a
reason from a short list, a second person's approval only where the merchant's policy asks for it, the
old hash retired and the reset audited), and `1.7.6` (the printer read at
Pay, before any money is taken, so paper out or no printer is told before the customer pays and never
refuses the sale), and `1.7.6b` (no printer, and selling never stops: every receipt kept in the
sale's own transaction and printed unchanged once a printer can take it, and the missing printer an
alarm on the register's health, never a modal).
<!-- frontier:end -->
Group 1.1 is **complete**. `1.1.9` was the last of it: its pure-domain time values, clock policy
and terminal IANA-zone resolution landed first, and its database persistence half followed on
14 September once `1.9.1` shipped `trusted_time_state`. `ClockRepository` now maps `ClockState`
field for field across a restart, and **one of the three `Full-step status:` markers is gone** —
the first to be cleared rather than added. `1.6.3` is **fully delivered**: `crates/pos-db/tests/role_matrix.rs` reads back all
128 seeded `role_capability` rows and holds each one to `cap::DEFAULT_MATRIX`, so the grid the
domain declares and the grid the register ships are the same grid or the gate is red.
Group 1.2 begins with the **partially delivered** `1.2.0` benchmark gate: `just
bench-gate`, `scripts/bench-gate.py`, its refusal-path suite, and
`benchmarks/reference-register.toml` are committed with **every identity value deliberately blank**
beside §6a.1's empty register-hardware table. `python3 scripts/bench-gate.py --check-profile`
therefore exits **non-zero**, which is conventions §7.1 working rather than failing; filling both
records is `1.2.0`'s deferred half and waits on milestone 5.8, where the register is bought — no
device is bought before it ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b). **`1.8.9` has landed, so the gate it held is open**: groups 1.6, 1.9 and 1.10 may now write
append-only facts, because every fact graph commits with its delivery envelope and the writer refuses
to return success on an incomplete one. **`1.9.1`, `1.10.1` and `1.2.5` have landed too**, so the
migration chain is contiguous to `0007`, and `1.2.3`, the product repository and its search, is
built on it; the `1.2.4` database half and `1.10.2`–`1.10.4` have landed, and `1.10.5`'s domain and database half with them. `1.2.4` is **complete**: its pure half
— the scan parser, `PriceSource`, `ScanLookup`, `resolve_scan` and the trade-scale gate — is in
`pos-domain`, with the compile-fail proof that only the scan handler can make a price-embedded
lookup. Its database half has landed too: `TradeScaleRepository` reads a scale's current evidence and
a store's active label layouts, and records evidence only with the delivery envelope it travels in.
Its shell half has landed as well: the scan handler reads a code with every layout the store has,
makes a label answer for the scale that printed it before anything is looked up, and hands the hits to
`resolve_scan`. Its last named test landed with 1.4.3's `AddLine::from_scan`, where a label becomes
one package in the cart. `1.2.7`'s search benchmark
has landed, and its first indicative run on a development laptop put the first two keystrokes over
the 50 ms limit, which fires the risk-register trigger in [`00-master-plan.md`](00-master-plan.md)
§6; what to do about it is the owner's decision, and the microstep's note records the measurement.
`1.2.8`, the scan-parser fuzz target, has landed with `just fuzz`, so every microstep of group 1.2
has begun; `1.2.0` is its one partial step, waiting on milestone 5.8. Group 1.4's `1.4.11` is
**partially delivered**: a basket is built through the register's first cart commands, every answer
the cart priced under the store's own settings, no command taking a price, stock screened as each
line is added, and every working basket on disk, so a crash or a sign-out sets it aside in the parked
list. A cashier can now park a basket, list what is parked and resume one, and taking over another
cashier's basket is audited. A shift lead or a manager can override a line's price, for a reason
from the store's list, held to the cost floor and the ministry ceiling and audited every time. The
department sale follows, as the owner decided, and waits on #330. `1.4.12` is
**partially delivered**: its domain half sells goods with no product through a department, capped,
escalated above a threshold and always audited; its terminal half needs a department that can be
stored, which #330 asks about. Group 1.6's `1.6.2` is **complete**: its reset command landed on
9 November 2026, under merchant decision 9.6's default while #233 waits for the merchant's answer
on a second factor. `1.6.3` is closed: its seed comparison landed against `0004`'s 128 rows. `1.6.4` is **complete**: `Authorized<C>`,
`authorize`, `ApprovalHandle` and `EscalationPolicy` are complete in `pos-domain`, with the two
compile-fail proofs that a token cannot be forged or substituted across capabilities. Its
persistence half now stores, validates, reloads and one-use consumes a handle through a
caller-owned transaction, with restart and rollback coverage; the additional compile-fail proof
prevents deserialization from bypassing that validated seam. Its terminal half landed with 1.8.2c:
the command context resolves a stored handle by id and holds it to its binding. Its last `⚠️ OPEN`
question was answered by the owner on 5 November: deactivating an approver revokes every handle they
issued that has not been spent, and the context refuses one as `approval_revoked`. Group 1.6 is not
**blocked on the merchant legal name or TIN**, which instead gate store provisioning and the issuing
of a valid tax receipt. `1.3.2`, the first code step on the phase's
longest critical path, has landed. It was waiting on which zero-rating reasons the GST return
distinguishes; the owner decided on 26 October that, while it is unknown whether the merchant makes
such supplies at all, every supply other than domestic is refused
([`ref/merchant-decisions.md`](ref/merchant-decisions.md) 6.14), and the `⚠️ OPEN` item in
[`ref/domain-api.md`](ref/domain-api.md) §5 now belongs to 2.7.0. `1.3.3`, the next step on that
path, has landed and enforces the refusal. `1.3.4` is **partially delivered**: its pure half,
extracting the tax from a tax-inclusive price, and its database half, which reads a store's approved
computation policy into the rounding rule the engine is handed, have landed. Its terminal half
waits for the files 1.8.3 and 1.8.5b create, and the policy itself waits on the `⚠️ OPEN` item in
[`ref/schema.md`](ref/schema.md) §0003. `1.3.5` has landed on the default arrangement its
`⚠️ OPEN` item in [`ref/domain-api.md`](ref/domain-api.md) §5 names; no Special Sales Tax rule is
seeded, and the question now waits with the rate pack at 1.3.7. `1.3.7` is **partially
delivered**: the path a jurisdiction pack takes has landed. A pack is pinned to the hash of what
was reviewed, imported only with the accountant's dated approval, and read back by business date
as what a store charges, with an unconfigured band or an incomplete zone pack refused. It is proved
on a synthetic fixture; the merchant's real pack waits on [`ref/schema.md`](ref/schema.md) §0003's
`⚠️ OPEN` item.
Group 1.7's `1.7.8` is **partially delivered**: `ReceiptPrinter::print` returns one of the four
outcomes, the simulator scripts paper-out and silence at byte *N*, cover-open, offline and a slow
device, and a sweep injects a fault at every byte of a real receipt. The sweep moves onto a golden
when `1.7.5` commits them, and its last clause — no second original artifact — can now run against
`1.7.7`'s queue. `1.7.12` is **partially delivered** too: the emulated network printer behaves as each seeded
printer and drawer profile says — refusing what a device would refuse, reporting unknown behaviour
where a profile does not know — and `just printer-emulator <profile>` prints to it before any
printer exists; only its golden comparison waits for `1.7.5`'s goldens.
`1.7.13` is **partially delivered** as well: one conformance suite runs against the emulated printer for every receipt-printer profile, refuses to reach a device that is not named, replays a synthetic capture, and is replayed through a pinned independent emulator in CI; only its golden replay waits for `1.7.5`.
Group 1.8's `1.8.3` is **partially delivered**: a basket can now be paid for in cash. Payment begins
journalled, after the basket's stock is screened once more; cash settles at the store's coin step,
and only cash gives change; taking a tender back is audited; and a sale with money taken is never
set aside. A cashier's sign-out waits for it, and a crash or a forced sign-out leaves it journalled
for recovery, as the owner decided. Its receipt model has landed too: a completed sale becomes the
one document the printer, the PDF and the email render, every amount read from the sale, adding up
on its face, and headed as an invoice only where the store owes none or JoFotara has cleared it. And
the finalize has landed: a paid sale completes in one transaction, its envelope written before any
fact it names and its completion last, numbered, its stock moved, its original receipt rendered and
queued, its journal removed and its basket consumed, with a fault injected at every write it makes
and nothing but the journal's own step left behind. The printer and the drawer run after the commit,
and their failure never un-completes the sale. Two of its ten named tests wait, by the owner's
decision of 8 November 2026: the approval one for the first command that brings an approval bound to
the sale to payment, and the department one for #330. Until `1.7.14`, added that day, binds a
register to its printer, every receipt is rendered for the 80 mm simulator profile and queued.
Group 1.9's `1.9.5` is **partially delivered**: a shift opens with a float counted by denomination,
on a business date the clock derives when it is trusted and the operator confirms, audited, when it
is not; one stays open per register; and only its opener closes it, with a fact of its own and no
approval. A shift is never dated before the register's last. A sale completes only in the open shift
and on its business date. Its three commands landed
on 9 November 2026: `shift_open`, `shift_current` and `shift_close` open, read and close a shift
from the till, a close refused while a sale in the shift is mid-payment, and the till's basket set
aside as a sign-out sets it aside. Its two screens wait for `1.11.3b`'s design direction.
Group 1.10's `1.10.5` is **partially delivered**: stock moved by hand, with a reason from five, a
note, and an approval bound to the exact request by its digest, posts its event, audit row and
consumption together; opening stock is read from its three-column file and posted as ledger events,
never cache rows. Its three commands landed on 10 November 2026: `stock_on_hand` reads the store's
stock, its cost shown only to a holder of `stock.adjust`; `stock_adjust_prepare` stores what a manager
asks for, and `stock_adjust` posts it with a second person's approval, dated by the open shift, and
only for the person who asked. Since the owner's decisions of 10 November 2026, `stock_adjust_now`
posts one manager's adjustment at once, audited, and the two steps are the way only where the
merchant's policy names `stock.adjust`. Its screen waits for `1.11.3b`'s design direction.
Repository-hardening and documentation work may land between numbered product steps, but it does
not advance that frontier.

**This pointer is maintained by station 13 of the feature lifecycle** — the documentation loop in
[`02-development-workflow.md`](02-development-workflow.md) §4.13 — and by nothing else. It is a
dated convenience, not a source of truth: the delivery board and the merged history are. If the two
disagree, the history is right and this line is a bug; fix it in the same commit that noticed.

---

## Layout

```
docs/implementation/
├── README.md                      ← you are here
├── 00-master-plan.md              the spine: verdict, corrections, phases, risks
├── 01-conventions.md              the engineering law
├── 02-development-workflow.md     how the work gets done: commands, loops, manual tests
├── 03-github-workflow.md          branches, issues, the board, PRs, release channels
│
├── phase-0-closeout.md            historical close-out record
├── phase-1-sellable-mvp.md        cash · tax · Arabic receipts 15–21 weeks
├── phase-2-money-grade.md         cards · refunds · fiscal     10–13 weeks
├── phase-3-connected.md           sync · back office · CRM     11–14 weeks
├── phase-4-depth.md               promos · supply · reports     9–12 weeks
├── phase-5-harden-and-launch.md   certification · launch       11–16 weeks
│
└── ref/
    ├── plan-validation.md         the audit of record + every source
    ├── domain-api.md              every pos-domain type & signature
    ├── schema.md                  migrations 0002→0014 — authoritative for every migration number
    ├── tax-jordan.md              GST as an engine
    ├── fiscal-jofotara.md         the highest-risk component
    ├── ipc-contract.md            every Tauri command
    ├── sync-protocol.md           ownership, chaos, accepted risks
    ├── hardware-and-receipts.md   traits, Arabic raster, emulators and the gate, lab checklist
    ├── ui-design.md               the look: principles, references, skills, 3D budget
    ├── ui-spec.md                 screens, RTL, keyboard map
    ├── test-catalog.md            95 edge cases → named tests
    ├── security-compliance.md     PDPL · PCI · tenancy · keys · audit chain
    └── merchant-decisions.md      the questionnaire
```

Source documents live in [`../plan/`](../plan/): the business & functional master plan, the engineering blueprint, and the original Phase-0 setup guide.

`status-page.html` is a locally shareable summary of the spine — verdict, corrections, phase map, risk register, and live Phase-0 status — for people who need the picture without the microsteps. No publication workflow is configured in this repository; it is a view of this set, never the source of truth.

---

## How to read a phase file

Each opens with its exit statement, a group dependency graph, and a note on which groups are independent — useful when you are stuck. Each closes with an exit gate written as runnable commands plus demonstrations.

Microsteps are numbered `<phase>.<group>.<step>` and **the numbers are stable** — they are commit-message references and checklist IDs.

```
### 1.3.4 — Tax engine: inclusive extraction
Depends on:  1.3.1, 1.1.2
Files:       crates/pos-domain/src/tax.rs  (new)

<the actual signatures / SQL / types to write>

Tests to add:  inclusive_16pct_extracts_exactly
               prop_line_tax_sum_equals_receipt_tax
Verify:        cargo nextest run -p pos-domain tax::
Done when:     Σ line tax == receipt tax, exactly, ∀ inputs
```

**A microstep is done when its `Done when` line is objectively true, checked by running its command** — not when the code looks finished. The full definition of done is [`01-conventions.md`](01-conventions.md) §6.

---

## The four corrections, and what became of them

Research found four claims in the business master plan that are wrong. A later independent audit
found that **two of the corrections were themselves wrong**, in the direction that matters: each
would have rejected valid fiscal documents. The current disposition of all four, the concordance
that maps every superseded name to its current authority, and the open items none of them closed
live in one place — **[`00-master-plan.md`](00-master-plan.md) §4a, "Errata and concordance"**.
[`ref/plan-validation.md`](ref/plan-validation.md) is the audit of record and carries a dated
revision note against each correction in place.

| | Original correction | Now |
|---|---|---|
| **C-1** | **JoFotara has no sandbox.** The master plan's Phase-2 exit gate is unbuildable as written | **Retained.** But the specification *is* obtainable, so pinning it moved from Phase 5 into microstep **2.7.0**, ahead of every fiscal build step |
| **C-2** | ISTD rejects global discounts — per-line, as a percentage, with an exact round-trip | **Superseded.** Exact line allowance **amounts** plus a document recap equal to their sum. A percentage round-trip is not a representation of the money, and gating on it dead-letters correct baskets |
| **C-3** | ISTD recomputes totals at 9 decimals, tolerating < 0.001 JOD drift | **Superseded.** The tolerance is not sourced. The local check is a half-fil per-line comparison plus exact identities over the document's own carried values |
| **C-4** | GST thresholds are 75k goods / 30k services / 10k special-tax goods | **Numbers retained, categories corrected.** The 10k class is the *producer* of SST goods. A minimarket that resells tobacco does not enter it |

Each still appears inline at the exact microstep it changes, so reading only a phase file cannot
lead you to implement the wrong thing.

---

## Conventions in one screen

If you read nothing else:

- **Money is `i64` minor units, always.** No float touches money, anywhere.
- **The minor-unit exponent is per-currency data.** JOD = 3.
- **Quantities are `i64` milli-units.** Weighed and discrete share one representation.
- **Completed sales are immutable.** Corrections are new documents. What looks like a mutation —
  tender settlement, shift close — is an append-only transition fact plus a rebuildable projection.
- **Price and name are copied onto the sale line** at capture time.
- **Stock is a ledger.** On-hand is a rebuildable cache.
- **Ordering comes from server versions and the outbox sequence**, never a device clock. UUIDv7 is
  identity and index locality; it embeds a device timestamp and cannot be the ordering authority.
- **`pos-domain` is pure.** No I/O, no clock, no randomness — they are arguments.
- **Every fact write and its outbox row commit in one transaction**, and a business transaction's
  facts travel as one commit group that the server applies whole or not at all.
- **No base sale command accepts an uncontrolled price.** Price-bearing IPC arguments exist only on
  audited `cart_override_price`, capped audited `cart_add_department_sale`, and inert,
  content-hashed `product_quick_add_prepare`; every privileged effect consumes its bound
  `ApprovalHandle` in the effect-and-audit transaction.

---

## Questions this set answers

| Question | Where |
|---|---|
| Is the business plan any good? | [`ref/plan-validation.md`](ref/plan-validation.md) |
| The source plan says X and this set says Y — which wins? | [`00-master-plan.md`](00-master-plan.md) §4a |
| What is still unanswered, and what do we do meanwhile? | [`00-master-plan.md`](00-master-plan.md) §4a.3, then the `⚠️ OPEN` blocks in `ref/` |
| What do I do first? | [`02-development-workflow.md`](02-development-workflow.md) §1 |
| What happened during Phase 0? | [`phase-0-closeout.md`](phase-0-closeout.md) — historical record |
| Which command do I type, and how do I test it by hand? | [`02-development-workflow.md`](02-development-workflow.md) |
| Which branch do I work from, and how does a change reach a merchant? | [`03-github-workflow.md`](03-github-workflow.md) |
| Is this rule actually enforced, or only written down? | [`03-github-workflow.md`](03-github-workflow.md) §3 |
| Should we use Jira, and is it free? | [`03-github-workflow.md`](03-github-workflow.md) §9 |
| What does this function look like? | [`ref/domain-api.md`](ref/domain-api.md) |
| What columns does this table have? | [`ref/schema.md`](ref/schema.md) |
| How does Jordanian GST actually work? | [`ref/tax-jordan.md`](ref/tax-jordan.md) |
| How do I do e-invoicing without a sandbox? | [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §6 |
| How do I print Arabic on a thermal printer? | [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §2.1 |
| Which hardware do I need, and when is it bought? | [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b — none before milestone 5.8; emulators until then |
| Which test covers edge case N? | [`ref/test-catalog.md`](ref/test-catalog.md) |
| What do I need to ask the merchant? | [`ref/merchant-decisions.md`](ref/merchant-decisions.md) |
| Am I allowed to say "PCI compliant"? | [`ref/security-compliance.md`](ref/security-compliance.md) §3 — probably not yet |

---

## Keeping this set alive

- **Re-run the validation audit quarterly**, and re-diff the pinned ISTD manifest in the same pass. Jordanian rates move by Cabinet decree, JoFotara is still adding waves and changing validation, and the PDPL register is now live. A compliance claim has a shelf life.
- **When a merchant or a pilot surfaces something new, it becomes E.96** in the test catalog — with a test, an accepted risk, an open question with a stated default, or an explicit out-of-scope. A surprise that becomes none of those will happen again.
- **When a correction turns out to be wrong, correct it in [`00-master-plan.md`](00-master-plan.md) §4a and mark it in place in `ref/plan-validation.md`.** Two of the original four were wrong; leaving an audit reading as still-true is how a fix becomes a defect.
- **When a microstep turns out to be wrong at the keyboard, fix the microstep.** These files are the plan of record, not a historical artefact.

*Verified against the repository and current sources on 20 August 2026; remediated against an independent seven-lens audit on 25 August 2026.*
