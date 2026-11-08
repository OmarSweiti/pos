//! The sale's own commands (`ref/ipc-contract.md` §3, "Tender and finalize").
//!
//! **Receipt numbering (microstep 1.9.3).** A receipt number is
//! `REG01-000123`: the register's prefix, its code and a hyphen, then a
//! counter zero-padded to six digits. A central counter cannot exist offline,
//! so the prefix is what keeps two registers' numbers apart, and each
//! register's counter is its own `doc_sequence` row, advanced in the
//! transaction of the document it numbers (I-7). The counter's first number
//! fixes its prefix, so a register whose code changes keeps numbering where
//! its receipts left off ([`SequenceRepository::next_prefixed`]).
//!
//! **The atomic finalize (microstep 1.8.3, its third part).** `sale_finalize`
//! completes the sale the till is paying for, once its tenders settle it.
//!
//! *Before its transaction* it refuses, by name and with nothing written,
//! what could never complete: a sale not yet settled, a store not registered
//! for GST (the owner's decision of 8 November 2026), a live sale at a store
//! with no evidenced fiscal decision, a shift no longer open. Then it journals
//! the finalize as begun, in a transaction of its own that commits first, so a
//! power cut leaves a row a restart replays under the same idempotency key
//! (1.8.4). Then it mints every identity the transaction will write.
//!
//! *In one transaction* it reads the acting user again and refuses one whose
//! account stopped being live; takes the receipt number; completes the sale;
//! builds its receipt and renders the bytes a printer receives, for the
//! register's printer profile, touching no device; writes one commit, its
//! whole manifest and its delivery rows **before** any guarded fact, as the
//! schema's `BEFORE INSERT` guards require; then the sale's rows, each line's
//! stock event, the original receipt artifact and its print job, the
//! finalize's audit row, the removal of the checkout journal and the
//! consumption of the working basket; and last, the completion. Each of those
//! writes is a [`FinalizeWritePoint`], and [`FinalizeWritePoint::ALL`] is
//! their catalog. A failure anywhere rolls all of it back.
//!
//! *After the commit only*, the printer and the drawer. Their failure never
//! un-completes the sale (`ref/hardware-and-receipts.md` §3 rule 2): a refused
//! receipt waits in the queue, an unconfirmed one waits for a person, and the
//! drawer, whose opening the audit row recorded before it was asked, is never
//! asked twice.
//!
//! **What it does not do yet.** It spends no approval: no command brings an
//! approval bound to the sale to payment yet, so
//! `approval_effect_audit_and_consumption_commit_together` waits for the first
//! that does, by the owner's decision of 8 November 2026. A department line is
//! refused by `SaleRepository::prepare` until #330 is decided.

use std::cell::Cell;
use std::collections::BTreeMap;

use pos_db::DbError;
use pos_db::repo::audit::AuditRepository;
use pos_db::repo::checkout_operation::CheckoutOperationRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::parked_cart::ParkedCartRepository;
use pos_db::repo::receipt::{ArtifactKind, AttemptOutcome, Id, ReceiptArtifact, ReceiptRepository};
use pos_db::repo::sale::SaleRepository;
use pos_db::repo::sequence::{SeqKind, SequenceRepository, SequenceScope};
use pos_db::repo::shift::ShiftRepository;
use pos_db::repo::stock::StockRepository;
use pos_db::repo::store::{FiscalDecision, StoreProfile, StoreRepository};
use pos_db::repo::tender_type::{TenderKind, TenderTypeRepository};
use pos_db::repo::user::UserRepository;
use pos_db::tax_policy::TaxPolicyRepository;
use pos_domain::cart::{begin_finalize, complete};
use pos_domain::{
    AuditIntent, CompletedSale, FinalizeEffects, FiscalStanding, FooterBlock, IdSource, InFlight,
    InFlightSale, Language, MerchantBlock, Qty, ReceiptContext, RegisterId, SaleId, StockEventId,
    StockEventKind, StockSource, Timestamp, build_receipt_model, cap, is_settled,
};
use pos_hardware::escpos;
use pos_hardware::render::render_receipt;
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde::Serialize;
use serde_json::json;
use tauri::State;
use uuid::Uuid;

use super::cart::{pricing_refused, proof, signed_in_as};
use super::tender::paying;
use super::{sqlite, stored};
use crate::ipc::{IpcError, IpcResult};
use crate::payload::{artifact_payload, audit_payload};
use crate::print_queue::{PrintQueueError, PrintWorker, paper_of};
use crate::state::{PRODUCER_VERSION, Paying, Register, SYNC_PROTOCOL_VERSION, Till, Working};

/// The digits a receipt number's counter is zero-padded to.
pub const RECEIPT_COUNTER_DIGITS: usize = 6;

/// The audit action a finalize is recorded under.
pub const FINALIZE_ACTION: &str = "sale.finalize";

/// The layout every original receipt is rendered with: 1.7.3's renderer, as
/// its version is recorded beside the bytes.
pub const TEMPLATE_VERSION: &str = "receipt-v1";

/// The byte format a receipt printer receives.
const FORMAT: &str = "escpos";

/// The name the finalize's print worker leases a job under.
const WORKER: &str = "sale-finalize";

/// The prefix a register's receipts carry: its code, then a hyphen.
#[must_use]
pub fn receipt_prefix(register_code: &str) -> String {
    format!("{register_code}-")
}

/// A receipt number as it is printed and stored: `prefix`, then `value`
/// zero-padded to [`RECEIPT_COUNTER_DIGITS`]. A counter past 999 999 grows a
/// digit rather than wrap, so a number is never issued twice.
#[must_use]
pub fn receipt_number(prefix: &str, value: u64) -> String {
    format!("{prefix}{value:0width$}", width = RECEIPT_COUNTER_DIGITS)
}

/// The next receipt number of `register`, whose code is `code`, taken inside
/// `tx`, the transaction that writes the document it numbers. A rolled-back
/// transaction spends no number.
///
/// # Errors
///
/// What [`SequenceRepository::next_prefixed`] refuses: a code that makes a
/// prefix a cashier could not type back, a receipt counter that already
/// numbered without a prefix, and storage failures.
pub fn next_receipt_number(
    tx: &Transaction<'_>,
    register: RegisterId,
    code: &str,
) -> Result<String, DbError> {
    let issued = SequenceRepository::new(tx).next_prefixed(
        tx,
        SequenceScope::Register(register),
        SeqKind::Receipt,
        &receipt_prefix(code),
    )?;
    Ok(receipt_number(&issued.prefix, issued.value))
}

// ── The finalize ───────────────────────────────────────────────────────────

/// Every write the finalize makes, in the order it makes them: one variant
/// per repository boundary, then the commit. A count of facts would be false
/// the moment a basket held another line or tax component; this catalog is
/// what the fault seam fires before, every one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FinalizeWritePoint {
    /// The checkout journal records the finalize begun, in a transaction of
    /// its own that commits before the finalize's.
    Journal,
    /// The register's receipt counter advances.
    ReceiptNumber,
    /// One commit, its whole manifest and its delivery rows.
    Envelope,
    /// The sale's rows, written parked.
    SaleRows,
    /// Each line's stock event.
    StockEvents,
    /// The original receipt's bytes.
    ReceiptArtifact,
    /// The original's initial print job.
    PrintJob,
    /// The finalize's audit row.
    AuditRow,
    /// The checkout journal's row, removed.
    CheckoutRemoval,
    /// The working basket's row, consumed by the sale it became.
    WorkingCart,
    /// The sale, from parked to completed: the last write.
    Completion,
    /// The transaction commits.
    Commit,
}

impl FinalizeWritePoint {
    /// Every write point, in the order the finalize reaches them.
    pub const ALL: &'static [Self] = &[
        Self::Journal,
        Self::ReceiptNumber,
        Self::Envelope,
        Self::SaleRows,
        Self::StockEvents,
        Self::ReceiptArtifact,
        Self::PrintJob,
        Self::AuditRow,
        Self::CheckoutRemoval,
        Self::WorkingCart,
        Self::Completion,
        Self::Commit,
    ];
}

/// One envelope member before it is written: its entity, its id and the
/// payload it travels as.
pub(crate) type Member = (&'static str, [u8; 16], String);

/// A failure the fault seam injected.
#[derive(Debug, thiserror::Error)]
#[error("a failure was injected before {0:?}")]
pub(crate) struct Injected(pub(crate) FinalizeWritePoint);

/// What the finalize's tests inject. The register's own finalize injects
/// nothing.
pub(crate) trait FinalizeFaults {
    /// Called before `point`, with the register's connection: the
    /// finalize's transaction from [`FinalizeWritePoint::ReceiptNumber`] on.
    /// An error stops the finalize there.
    fn before(&self, point: FinalizeWritePoint, conn: &Connection) -> Result<(), Injected>;

    /// Called with the envelope's members before they are written.
    fn members(&self, _members: &mut Vec<Member>) {}
}

/// The finalize every command runs: no fault injected.
struct NoFaults;

impl FinalizeFaults for NoFaults {
    fn before(&self, _point: FinalizeWritePoint, _conn: &Connection) -> Result<(), Injected> {
        Ok(())
    }
}

/// A completed sale, as the screen is told it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompletedSaleRef {
    pub sale_id: SaleId,
    pub receipt_number: String,
    /// The currency every `*_minor` is in, by its ISO code.
    pub currency: &'static str,
    /// What the customer was charged: the priced total, with the cash
    /// rounding the final tender settled it under.
    pub total_minor: i64,
    /// What the drawer hands back.
    pub change_minor: i64,
    /// What became of the receipt once the sale committed.
    pub receipt: ReceiptOutcome,
    /// What became of the drawer once the sale committed.
    pub drawer: DrawerOutcome,
}

/// What became of a completed sale's receipt. It exists whatever the answer:
/// the original was written with the sale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptOutcome {
    /// No printer is attached. The receipt waits in the queue, unchanged,
    /// for one (`ref/hardware-and-receipts.md` §3a).
    Queued,
    /// The printer printed it.
    Printed,
    /// The printer took no byte of it. The queue tries again.
    Refused,
    /// Paper may exist and nothing can say whether it is readable: a person
    /// answers, and only a duplicate may follow (§3 rule 5).
    Unconfirmed,
    /// Never sent: the stored bytes did not match their digest, or were
    /// rendered for another printer.
    Withheld,
}

/// What became of the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawerOutcome {
    /// No tender of the sale opens the drawer.
    NotOpened,
    /// One does, and no printer is attached to kick it: open it by hand.
    NoDevice,
    /// The drawer was kicked.
    Opened,
    /// The kick failed. It is never sent again (§4 rule 3): open it by hand.
    Failed,
}

#[tauri::command]
pub fn sale_finalize(till: State<'_, Till>) -> IpcResult<CompletedSaleRef> {
    finalize(till.register()?)
}

/// `sale_finalize`: complete the sale the till is paying for. See the
/// module's documentation for what it writes, and in what order.
///
/// # Errors
///
/// What the context refuses; `cart_not_open` and `payment_not_begun`;
/// `sale_not_settled` while the tenders leave something owed;
/// `store_not_gst_registered`; `fiscal_obligation_undecided` for a live
/// sale at a store with no evidenced fiscal decision; `shift_not_open`; the
/// pricing refusals; `user_inactive` for an acting user whose account stopped
/// being live since the command began; `receipt_unprintable` for a receipt
/// the store cannot print, such as a tender kind with no name; and
/// `storage_failed`. Before the journal is written a refusal changes nothing;
/// after it, nothing but the journal's own step, which a later finalize
/// replays under the same idempotency key.
pub fn finalize(register: &Register) -> IpcResult<CompletedSaleRef> {
    finalize_with(register, &NoFaults)
}

/// [`finalize`], with `faults` asked before every write point.
pub(crate) fn finalize_with(
    register: &Register,
    faults: &dyn FinalizeFaults,
) -> IpcResult<CompletedSaleRef> {
    let at = register.now();
    let auth = proof::<cap::SaleCreate>(register, at)?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let from = paying(till.working.as_ref())?.clone();
    let cart = from.tendering.cart().clone();

    // ── Refused before anything is written ─────────────────────────────
    if !is_settled(&from.tendering) {
        return Err(IpcError::new("sale_not_settled", "error.sale.not_settled"));
    }
    let store = StoreRepository::new(&till.db)
        .profile(register.store())
        .map_err(stored)?;
    if !store.gst_registered {
        return Err(IpcError::new(
            "store_not_gst_registered",
            "error.sale.store_not_gst_registered",
        ));
    }
    let standing = match (store.fiscal, cart.is_training) {
        (FiscalDecision::Exempt, _) => FiscalStanding::Exempt,
        (FiscalDecision::Required, _) | (FiscalDecision::Undecided, true) => {
            FiscalStanding::Uncleared
        }
        (FiscalDecision::Undecided, false) => {
            return Err(IpcError::new(
                "fiscal_obligation_undecided",
                "error.sale.fiscal_obligation_undecided",
            ));
        }
    };
    let shifts = ShiftRepository::new(&till.db);
    let business_date = match shifts.by_id(cart.shift_id).map_err(stored)? {
        Some(shift) if shifts.is_open(cart.shift_id).map_err(stored)? => shift.business_date,
        _ => return Err(IpcError::new("shift_not_open", "error.shift.not_open")),
    };
    let price_mode = TaxPolicyRepository::new(&till.db)
        .pricing_settings(register.store())
        .map_err(|e| pricing_refused(&e))?
        .price_mode;
    let kinds = TenderTypeRepository::new(&till.db)
        .kinds()
        .map_err(stored)?;
    let finalizing = begin_finalize(from.tendering.clone())
        .map_err(|_| IpcError::new("sale_not_settled", "error.sale.not_settled"))?;

    // ── The journal, first and on its own ──────────────────────────────
    faults
        .before(FinalizeWritePoint::Journal, &till.db)
        .map_err(|e| injected(&e))?;
    let journal = CheckoutOperationRepository::new(&till.db)
        .get(cart.id)
        .map_err(stored)?;
    let resumed = journal.is_some_and(|operation| {
        matches!(operation.in_flight.state, InFlight::Finalizing(_))
            && operation.in_flight.idempotency_key == from.idempotency_key
    });
    if !resumed {
        let step = InFlightSale {
            state: InFlight::Finalizing(finalizing.clone()),
            card_op: None,
            idempotency_key: from.idempotency_key,
            updated_at: at,
        };
        let tx = till.db.transaction().map_err(sqlite)?;
        let version = CheckoutOperationRepository::new(&tx)
            .advance(&tx, &step, from.version)
            .map_err(stored)?;
        tx.commit().map_err(sqlite)?;
        *till.working = Some(Working::Paying(Box::new(Paying {
            version,
            ..from.clone()
        })));
    }

    // ── Every identity, before the transaction ─────────────────────────
    let minted = Minted::mint(register, &finalizing);

    // ── One transaction ────────────────────────────────────────────────
    let tx = till
        .db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sqlite)?;
    let users = UserRepository::new(&tx);
    if !users
        .by_id(auth.actor())
        .map_err(stored)?
        .is_some_and(|account| account.is_live)
    {
        return Err(IpcError::new("user_inactive", "error.auth.user_inactive"));
    }
    let cashier_name = users
        .by_id(cart.cashier_id)
        .map_err(stored)?
        .map(|account| account.display_name)
        .ok_or_else(|| IpcError::new("user_inactive", "error.auth.user_inactive"))?;

    faults
        .before(FinalizeWritePoint::ReceiptNumber, &tx)
        .map_err(|e| injected(&e))?;
    let number = next_receipt_number(&tx, register.register(), register.code()).map_err(stored)?;
    let sale = complete(
        finalizing,
        FinalizeEffects {
            receipt_number: number,
            business_date,
            completed_at: at,
        },
    )
    .map_err(|_| IpcError::new("sale_not_settled", "error.sale.not_settled"))?;

    let device = register.receipt_device();
    let context = receipt_context(&store, &kinds, register.code(), cashier_name, standing);
    let model = build_receipt_model(&sale, &context).map_err(|error| {
        IpcError::from_source(
            "receipt_unprintable",
            "error.sale.receipt_unprintable",
            &error,
        )
    })?;
    let paper = paper_of(&device.profile).map_err(|e| print_refused(&e))?;
    let bytes = render_receipt(&model, paper)
        .map_err(PrintQueueError::from)
        .and_then(|document| {
            escpos::encode(&document, &device.profile).map_err(PrintQueueError::from)
        })
        .map_err(|e| print_refused(&e))?;
    let artifact = ReceiptArtifact {
        id: minted.artifact,
        sale_id: *cart.id.as_uuid().as_bytes(),
        kind: ArtifactKind::Original,
        source_artifact_id: None,
        format: FORMAT.to_owned(),
        template_version: TEMPLATE_VERSION.to_owned(),
        printer_profile: device.profile.profile_id.clone(),
        fiscal_version: None,
        content_bytes: bytes,
        generated_at: at,
    };
    let content_hash = *blake3::hash(&artifact.content_bytes).as_bytes();
    let opens_drawer = sale.tenders().iter().any(|tender| {
        kinds
            .iter()
            .any(|kind| kind.code == tender.type_code && kind.opens_drawer)
    });
    let charged = model.totals.total;
    let intent = AuditIntent {
        actor: auth.actor(),
        approver: None,
        approval: None,
        action: FINALIZE_ACTION,
        entity: "sale",
        entity_id: cart.id.as_uuid(),
        reason: None,
        payload: json!({
            "currency": cart.currency.code(),
            "drawer": if opens_drawer { "requested" } else { "none" },
            "is_training": cart.is_training,
            "receipt_number": sale.receipt_number(),
            "tender_count": sale.tenders().len(),
            "total_minor": charged.minor(),
        }),
        at,
    };

    let movements = stock_movements(&sale, register, &minted, business_date, at)?;
    let stock = StockRepository::new(&tx)
        .prepare(&tx, register.register(), &movements)
        .map_err(stored)?;
    let prepared = SaleRepository::new(&tx)
        .prepare(&tx, &sale, price_mode, &minted.commit, &minted.prepare)
        .map_err(stored)?;
    minted.prepare.check("the sale's rows");

    let mut members: Vec<Member> = prepared
        .members()
        .iter()
        .map(|m| (m.entity(), *m.entity_id(), m.payload().to_owned()))
        .collect();
    members.extend(stock.iter().map(|e| {
        (
            "stock_ledger",
            *e.event().id.as_uuid().as_bytes(),
            e.payload(),
        )
    }));
    members.push((
        "receipt_artifact",
        artifact.id,
        artifact_payload(&artifact, &content_hash).map_err(|e| unencodable(&e))?,
    ));
    members.push((
        "audit_log",
        *minted.audit.as_bytes(),
        audit_payload(minted.audit, register.register(), &intent).map_err(|e| unencodable(&e))?,
    ));
    faults.members(&mut members);

    faults
        .before(FinalizeWritePoint::Envelope, &tx)
        .map_err(|e| injected(&e))?;
    let change_ids: Vec<[u8; 16]> = members
        .iter()
        .map(|_| *minted.changes.next().as_bytes())
        .collect();
    minted.changes.check("the envelope's change ids");
    let facts: Vec<FactMember<'_>> = members
        .iter()
        .zip(&change_ids)
        .map(|((entity, entity_id, payload), change_id)| FactMember {
            change_id,
            entity,
            entity_id,
            payload,
        })
        .collect();
    let created_at = at.to_iso8601();
    OutboxRepository::new(&tx)
        .write_commit(
            &tx,
            &CommitEnvelope {
                commit_id: &minted.commit,
                protocol_version: SYNC_PROTOCOL_VERSION,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: PRODUCER_VERSION,
                created_at: &created_at,
            },
            &facts,
        )
        .map_err(stored)?;

    faults
        .before(FinalizeWritePoint::SaleRows, &tx)
        .map_err(|e| injected(&e))?;
    let nonce = register.process_nonce();
    SaleRepository::new(&tx)
        .insert_complete(&tx, &prepared, |tx| -> Result<(), Halted> {
            faults.before(FinalizeWritePoint::StockEvents, tx)?;
            StockRepository::new(tx).append(tx, &stock)?;
            faults.before(FinalizeWritePoint::ReceiptArtifact, tx)?;
            let receipts = ReceiptRepository::new(tx);
            if receipts.append_artifact(tx, &artifact)? != content_hash {
                return Err(Halted::Db(DbError::StoredPrintRecordInvalid {
                    reason: "the stored original's digest is not the one its envelope names"
                        .to_owned(),
                }));
            }
            faults.before(FinalizeWritePoint::PrintJob, tx)?;
            receipts.queue_initial(tx, &minted.job, &artifact.id, at)?;
            faults.before(FinalizeWritePoint::AuditRow, tx)?;
            AuditRepository::new(tx).append(tx, register.register(), minted.audit, &intent)?;
            faults.before(FinalizeWritePoint::CheckoutRemoval, tx)?;
            CheckoutOperationRepository::new(tx).remove_on_finalize(
                tx,
                cart.id,
                from.idempotency_key,
            )?;
            faults.before(FinalizeWritePoint::WorkingCart, tx)?;
            ParkedCartRepository::new(tx).consume_on_finalize(tx, cart.id, nonce)?;
            faults.before(FinalizeWritePoint::Completion, tx)?;
            Ok(())
        })
        .map_err(|halted| match halted {
            Halted::Db(error) => stored(error),
            Halted::Injected(error) => injected(&error),
        })?;
    faults
        .before(FinalizeWritePoint::Commit, &tx)
        .map_err(|e| injected(&e))?;
    tx.commit().map_err(sqlite)?;
    *till.working = None;
    drop(till);

    // ── After the commit only ──────────────────────────────────────────
    let (receipt, drawer) = effects(register, &minted.job, opens_drawer);
    let answer = CompletedSaleRef {
        sale_id: cart.id,
        receipt_number: sale.receipt_number().to_owned(),
        currency: cart.currency.code(),
        total_minor: charged.minor(),
        change_minor: sale.change().minor(),
        receipt,
        drawer,
    };
    trace_finalized(&answer);
    Ok(answer)
}

/// Why the finalize transaction stopped inside `insert_complete`.
#[derive(Debug, thiserror::Error)]
enum Halted {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Injected(#[from] Injected),
}

/// What a receipt needs that the sale does not say, read from the store, its
/// tender kinds and the register (1.8.3's second part, as its note left it).
fn receipt_context(
    store: &StoreProfile,
    kinds: &[TenderKind],
    register_code: &str,
    cashier_name: String,
    fiscal: FiscalStanding,
) -> ReceiptContext {
    let language = store.receipt_language;
    // A kind with no English name prints its Arabic one in an English
    // receipt rather than none: it is the kind's own name, and a refusal would
    // stop the sale for a missing translation.
    let tender_labels: BTreeMap<String, String> = kinds
        .iter()
        .map(|kind| {
            let name = match language {
                Language::Arabic => kind.name_ar.clone(),
                Language::English => kind.name_en.clone().unwrap_or_else(|| kind.name_ar.clone()),
            };
            (kind.code.clone(), name)
        })
        .collect();
    ReceiptContext {
        merchant: MerchantBlock {
            legal_name: store.legal_name.clone(),
            store_name: store.name_in(language).to_owned(),
            address: store.address.clone(),
            phone: store.phone.clone(),
            tin: store.tin.clone(),
        },
        register_code: register_code.to_owned(),
        cashier_name,
        language,
        fiscal,
        tender_labels,
        // No discount reason has a stored printed name yet, and no shell
        // command lets a basket carry a discount: the step that first does
        // gives each code its name (1.8.3's second part, its note).
        discount_labels: BTreeMap::new(),
        loyalty: None,
        footer: FooterBlock {
            return_policy: None,
            thank_you: None,
        },
    }
}

/// Each line's stock event: what leaves the shelf, the weight a label implies
/// where one priced the line. A line with no product moves no stock.
fn stock_movements(
    sale: &CompletedSale,
    register: &Register,
    minted: &Minted<'_>,
    business_date: pos_domain::BusinessDate,
    at: Timestamp,
) -> IpcResult<Vec<pos_domain::NewStockEvent>> {
    let cart = sale.cart();
    let mut events = Vec::new();
    for (line, id) in cart.lines.iter().zip(&minted.stock) {
        let Some(product_id) = line.product_id else {
            continue;
        };
        let moved = line.label.as_ref().map_or(line.qty, |label| label.qty);
        let qty_delta = moved
            .milli()
            .checked_neg()
            .map(Qty::from_milli)
            .ok_or_else(|| {
                IpcError::new("storage_failed", "error.storage.failed")
                    .with_detail("a line's quantity cannot leave the shelf")
            })?;
        events.push(pos_domain::NewStockEvent {
            id: *id,
            product_id,
            store_id: register.store(),
            kind: StockEventKind::Sale,
            qty_delta,
            incoming_cost: None,
            is_weight_derived: line.label.is_some(),
            source: Some(StockSource::SaleLine {
                sale_id: cart.id,
                line_id: line.id,
            }),
            actor_id: Some(cart.cashier_id),
            occurred_at: at,
            business_date,
        });
    }
    Ok(events)
}

/// The printer and the drawer, once the sale has committed. Device time
/// holds the register's database while the print worker records its
/// attempt; the worker `run` will start moves it off the command.
fn effects(register: &Register, job: &Id, opens_drawer: bool) -> (ReceiptOutcome, DrawerOutcome) {
    let device = register.receipt_device();
    let Some(printer) = device.printer.as_deref() else {
        let drawer = if opens_drawer {
            DrawerOutcome::NoDevice
        } else {
            DrawerOutcome::NotOpened
        };
        return (ReceiptOutcome::Queued, drawer);
    };
    let ids = RegisterIds(register);
    let clock = RegisterClock(register);
    let worker = PrintWorker::new(printer, &device.profile.profile_id, &clock, &ids, WORKER);
    let receipt = match register.with_db(|conn| worker.drain(conn)) {
        Ok(drained) => drained
            .attempted
            .iter()
            .find(|attempt| attempt.job_id == *job)
            .map_or(ReceiptOutcome::Queued, |attempt| match attempt.outcome {
                AttemptOutcome::Printed => ReceiptOutcome::Printed,
                AttemptOutcome::Failed if attempt.error_code.is_some_and(withheld) => {
                    ReceiptOutcome::Withheld
                }
                AttemptOutcome::Failed => ReceiptOutcome::Refused,
                AttemptOutcome::Partial | AttemptOutcome::Unknown | AttemptOutcome::Cancelled => {
                    ReceiptOutcome::Unconfirmed
                }
            }),
        Err(error) => {
            // The job may have been leased and sent; recovery resolves it as
            // unknown, never back into the queue.
            tracing::error!(error = %error, "the receipt's print attempt was not recorded");
            ReceiptOutcome::Unconfirmed
        }
    };
    let drawer = if !opens_drawer {
        DrawerOutcome::NotOpened
    } else if printer.open_drawer().is_ok() {
        DrawerOutcome::Opened
    } else {
        tracing::warn!("the drawer did not open; it is never kicked twice");
        DrawerOutcome::Failed
    };
    (receipt, drawer)
}

/// Whether a refusal's code means the bytes were never sent.
fn withheld(code: &str) -> bool {
    code == crate::print_queue::code::CONTENT_HASH_MISMATCH
        || code == crate::print_queue::code::PROFILE_MISMATCH
}

/// What the finalize logs: identifiers and amounts, never a name, a buyer or
/// a payload (1.6.8). `telemetry`'s `no_pii_in_a_full_sale_trace` drives it.
pub(crate) fn trace_finalized(answer: &CompletedSaleRef) {
    tracing::info!(
        sale_id = %answer.sale_id.as_uuid(),
        receipt_number = %answer.receipt_number,
        total_minor = answer.total_minor,
        receipt = ?answer.receipt,
        drawer = ?answer.drawer,
        "sale finalized"
    );
}

/// An injected failure, as the screen would be told a storage failure.
fn injected(error: &Injected) -> IpcError {
    IpcError::from_source("storage_failed", "error.storage.failed", error)
}

/// A receipt the renderer or the encoder refused, which no printer could
/// receive.
fn print_refused(error: &PrintQueueError) -> IpcError {
    IpcError::from_source(
        "receipt_unprintable",
        "error.sale.receipt_unprintable",
        error,
    )
}

/// A payload that could not be written down.
fn unencodable(error: &serde_json::Error) -> IpcError {
    IpcError::from_source("storage_failed", "error.storage.failed", error)
}

// ── Identities, minted before the transaction ──────────────────────────────

/// Every identity the finalize transaction writes, minted before it opens.
struct Minted<'r> {
    commit: [u8; 16],
    audit: Uuid,
    artifact: Id,
    job: Id,
    /// One per line, whether or not the line moves stock.
    stock: Vec<StockEventId>,
    /// Each tax component, discount attribution, summary row and first
    /// tender event `SaleRepository::prepare` writes, in its order.
    prepare: Pool<'r>,
    /// Each envelope member's change id.
    changes: Pool<'r>,
}

impl<'r> Minted<'r> {
    fn mint(register: &'r Register, finalizing: &pos_domain::Finalizing) -> Self {
        let t = finalizing.tendering();
        let priced = t.priced();
        let components: usize = priced
            .lines
            .iter()
            .map(|line| line.tax.components.len())
            .sum();
        let attributions: usize = priced
            .lines
            .iter()
            .map(|line| line.discount_attributions.len())
            .sum();
        let rows = components + attributions + priced.tax_summary.len() + t.tenders().len();
        let lines = t.cart().lines.len();
        // The sale, its supply context, each line, and its rows above; each
        // tender; each line's stock event; the receipt and the audit row.
        let members = 2 + lines + rows + t.tenders().len() + lines + 2;
        Self {
            commit: *register.next_id().as_bytes(),
            audit: register.next_id(),
            artifact: *register.next_id().as_bytes(),
            job: *register.next_id().as_bytes(),
            stock: (0..lines)
                .map(|_| StockEventId::from_uuid(register.next_id()))
                .collect(),
            prepare: Pool::mint(register, rows),
            changes: Pool::mint(register, members),
        }
    }
}

/// Ids minted ahead, handed out in order. One asked for past the last is
/// minted then, so a sale is never refused for a miscount, and the miscount
/// is logged.
struct Pool<'r> {
    ids: Vec<Uuid>,
    next: Cell<usize>,
    overdrawn: Cell<bool>,
    register: &'r Register,
}

impl<'r> Pool<'r> {
    fn mint(register: &'r Register, count: usize) -> Self {
        Self {
            ids: (0..count).map(|_| register.next_id()).collect(),
            next: Cell::new(0),
            overdrawn: Cell::new(false),
            register,
        }
    }

    /// Log a pool that was asked for more ids than were minted ahead.
    fn check(&self, what: &str) {
        if self.overdrawn.get() {
            tracing::error!(
                what,
                "the finalize minted too few ids before its transaction"
            );
        }
    }
}

impl IdSource for Pool<'_> {
    fn next(&self) -> Uuid {
        let n = self.next.get();
        self.next.set(n.saturating_add(1));
        if let Some(id) = self.ids.get(n) {
            *id
        } else {
            self.overdrawn.set(true);
            self.register.next_id()
        }
    }
}

/// The register's ids, for the print worker.
struct RegisterIds<'r>(&'r Register);

impl IdSource for RegisterIds<'_> {
    fn next(&self) -> Uuid {
        self.0.next_id()
    }
}

/// The register's clock, for the print worker.
struct RegisterClock<'r>(&'r Register);

impl pos_domain::Clock for RegisterClock<'_> {
    fn now(&self) -> Timestamp {
        self.0.now()
    }
}

#[cfg(test)]
mod tests {
    //! 1.8.3's ten named tests, eight of which land with this part:
    //! `department_line_writes_no_stock_event` waits on #330, and
    //! `approval_effect_audit_and_consumption_commit_together` on the first
    //! command that brings a sale-bound approval to payment (the owner's
    //! decision of 8 November 2026).
    //!
    //! The shop is the integration suites' (`tests/shop/mod.rs`): the
    //! reference chain's store, exempt from e-invoicing with its evidence and
    //! registered for GST, whose approved policy rounds cash to the nearest
    //! 5 fils. Every figure is worked out by hand, never read back from the
    //! code under test.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use pos_db::repo::receipt::JobState;
    use pos_hardware::profile::PrinterProfile;
    use pos_hardware::render::PaperWidth;
    use pos_hardware::{
        Fault, HwError, PrintOutcome, PrinterStatus, ReceiptPrinter, RenderedReceipt,
        SimulatedPrinter,
    };
    use rusqlite::{OptionalExtension, params};

    use super::*;
    use crate::commands::cart::{add_product, current, open_cart};
    use crate::commands::tender::{add_cash, begin_payment};
    use crate::shop::{
        CASHIER_CODE, CASHIER_ID, CASHIER_PIN, CHEESE, OIL, field, open, product_id, refusal, seed,
        shop,
    };
    use crate::state::ReceiptDevice;

    /// What the fault seam saw, and what it was told to do.
    #[derive(Default)]
    struct Seam {
        seen: RefCell<Vec<FinalizeWritePoint>>,
        fail_at: Option<FinalizeWritePoint>,
        /// Run against the register's connection at this point: a change
        /// that lands between the command's context and its commit.
        meddle_at: Option<(FinalizeWritePoint, &'static str)>,
        drop_entity: Option<&'static str>,
    }

    impl FinalizeFaults for Seam {
        fn before(&self, point: FinalizeWritePoint, conn: &Connection) -> Result<(), Injected> {
            self.seen.borrow_mut().push(point);
            if let Some((at, sql)) = self.meddle_at
                && at == point
            {
                conn.execute(sql, [CASHIER_ID.as_slice()]).unwrap();
            }
            if self.fail_at == Some(point) {
                return Err(Injected(point));
            }
            Ok(())
        }

        fn members(&self, members: &mut Vec<Member>) {
            if let Some(entity) = self.drop_entity {
                let at = members
                    .iter()
                    .position(|(name, _, _)| *name == entity)
                    .unwrap_or_else(|| panic!("the envelope holds no {entity}"));
                members.remove(at);
            }
        }
    }

    /// Oil at 1.250 and 101 g of cheese at 8.500 a kilogram, 0.8585, which the
    /// fixture policy's half-even rule prices at 0.858: 2.108, which the coin
    /// step asks of cash as 2.110. Paid with 1.000 and then 5.000, so the
    /// second settles it with 3.890 change.
    fn settled(register: &Register) -> SaleId {
        open_cart(register).unwrap();
        add_product(register, product_id(OIL), None).unwrap();
        add_product(register, product_id(CHEESE), Some(101)).unwrap();
        let paying = begin_payment(register).unwrap();
        assert_eq!(paying.total_minor, 2_108);
        add_cash(register, 1_000).unwrap();
        let paid = add_cash(register, 5_000).unwrap();
        assert!(paid.settled);
        paying.sale_id
    }

    /// A shop whose register renders for the 80 mm simulator profile and has
    /// `printer` attached, its cashier signed in.
    fn shop_with(printer: Arc<dyn ReceiptPrinter>) -> (tempfile::TempDir, Register) {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), true);
        let register = open(dir.path()).with_receipt_device(ReceiptDevice {
            profile: PrinterProfile::simulator(PaperWidth::Mm80),
            printer: Some(printer),
        });
        crate::commands::auth::login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
        (dir, register)
    }

    /// Every table a finalize writes to, with how many rows each holds, and
    /// the register's receipt counter: what an atomic failure leaves exactly
    /// as it found.
    fn written(register: &Register) -> BTreeMap<&'static str, i64> {
        const TABLES: [&str; 17] = [
            "sale",
            "sale_supply_tax_context",
            "sale_line",
            "sale_line_tax",
            "sale_line_discount",
            "sale_tax_summary",
            "sale_tender",
            "tender_status_event",
            "stock_ledger",
            "receipt_artifact",
            "print_job",
            "approval_consumption",
            "audit_log",
            "sync_commit",
            "fact_commit_member",
            "sync_outbox",
            "doc_sequence",
        ];
        register.with_db(|conn| {
            let mut counts: BTreeMap<&'static str, i64> = TABLES
                .iter()
                .map(|table| {
                    let n = conn
                        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                            row.get(0)
                        })
                        .unwrap();
                    (*table, n)
                })
                .collect();
            let next: Option<i64> = conn
                .query_row(
                    "SELECT next_value FROM doc_sequence WHERE kind = 'receipt'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .unwrap();
            counts.insert("receipt counter", next.unwrap_or(1));
            counts
        })
    }

    /// The journal's state for `sale`, `None` when it holds no row.
    fn journal(register: &Register, sale: SaleId) -> Option<&'static str> {
        register.with_db(|conn| {
            CheckoutOperationRepository::new(conn)
                .get(sale)
                .unwrap()
                .map(|operation| match operation.in_flight.state {
                    InFlight::Tendering(_) => "tendering",
                    InFlight::Finalizing(_) => "finalizing",
                })
        })
    }

    /// The working basket's row for `sale`, by its state.
    fn basket(register: &Register, sale: SaleId) -> Option<String> {
        register.with_db(|conn| {
            conn.query_row(
                "SELECT state FROM parked_cart WHERE id = ?1",
                [sale.as_uuid().as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .unwrap()
        })
    }

    fn completed(register: &Register, sale: SaleId) -> bool {
        register.with_db(|conn| SaleRepository::new(conn).by_id(sale).unwrap().is_some())
    }

    fn still_paying(register: &Register) -> bool {
        matches!(
            register.lock_till().working.as_ref(),
            Some(Working::Paying(_))
        )
    }

    /// The finalize asks the seam before every write point, in the catalog's
    /// order, and before nothing else: `ALL` is every write it makes, once.
    #[test]
    fn finalize_write_point_catalog_is_exhaustive() {
        let (_dir, register) = shop();
        settled(&register);
        let seam = Seam::default();
        finalize_with(&register, &seam).unwrap();
        assert_eq!(seam.seen.borrow().as_slice(), FinalizeWritePoint::ALL);
        let distinct: BTreeSet<_> = FinalizeWritePoint::ALL.iter().collect();
        assert_eq!(
            distinct.len(),
            FinalizeWritePoint::ALL.len(),
            "no point twice"
        );
    }

    /// A failure before any write point leaves no financial fact, artifact,
    /// job, consumption, delivery row or receipt number: only the journal's
    /// own step, once the journal has been reached. The till still holds the
    /// sale, and the next finalize completes it, replaying the journal under
    /// the same key.
    #[test]
    fn finalize_is_atomic_under_injected_failure() {
        for &point in FinalizeWritePoint::ALL {
            let (_dir, register) = shop();
            let sale = settled(&register);
            let before = written(&register);
            let seam = Seam {
                fail_at: Some(point),
                ..Seam::default()
            };
            let failed = refusal(finalize_with(&register, &seam));
            assert_eq!(failed.code, "storage_failed", "at {point:?}");
            assert_eq!(written(&register), before, "nothing written at {point:?}");
            assert!(!completed(&register, sale), "at {point:?}");
            let expected = if point == FinalizeWritePoint::Journal {
                "tendering"
            } else {
                "finalizing"
            };
            assert_eq!(journal(&register, sale), Some(expected), "at {point:?}");
            assert_eq!(
                basket(&register, sale).as_deref(),
                Some("active"),
                "at {point:?}"
            );
            assert!(still_paying(&register), "at {point:?}");

            let answer = finalize(&register).unwrap();
            assert_eq!(answer.receipt_number, "REG01-000001", "at {point:?}");
            assert!(completed(&register, sale), "at {point:?}");
            assert_eq!(journal(&register, sale), None, "at {point:?}");
        }
    }

    /// The commit, its whole manifest and its delivery rows are written before
    /// any fact the schema guards, and the completion is the last write. Temp
    /// triggers on the register's own connection log every insert in the order
    /// SQLite performs it.
    #[test]
    fn manifest_and_delivery_rows_precede_guarded_facts() {
        const ENVELOPE: [&str; 3] = ["sync_commit", "fact_commit_member", "sync_outbox"];
        const GUARDED: [&str; 11] = [
            "sale",
            "sale_supply_tax_context",
            "sale_line",
            "sale_line_tax",
            "sale_tax_summary",
            "sale_tender",
            "tender_status_event",
            "stock_ledger",
            "receipt_artifact",
            "print_job",
            "audit_log",
        ];
        let (_dir, register) = shop();
        settled(&register);
        register.with_db(|conn| {
            conn.execute_batch(
                "CREATE TEMP TABLE write_log (n INTEGER PRIMARY KEY, what TEXT NOT NULL);",
            )
            .unwrap();
            for table in ENVELOPE.iter().chain(&GUARDED) {
                conn.execute_batch(&format!(
                    "CREATE TEMP TRIGGER log_{table} AFTER INSERT ON main.{table}
                     BEGIN INSERT INTO write_log (what) VALUES ('{table}'); END;"
                ))
                .unwrap();
            }
            conn.execute_batch(
                "CREATE TEMP TRIGGER log_completion AFTER UPDATE OF status ON main.sale
                 BEGIN INSERT INTO write_log (what) VALUES ('completion'); END;",
            )
            .unwrap();
        });
        finalize(&register).unwrap();
        let log: Vec<String> = register.with_db(|conn| {
            conn.prepare("SELECT what FROM write_log ORDER BY n")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect()
        });
        let last_envelope = log
            .iter()
            .rposition(|what| ENVELOPE.contains(&what.as_str()))
            .unwrap();
        let first_guarded = log
            .iter()
            .position(|what| GUARDED.contains(&what.as_str()))
            .unwrap();
        assert!(
            last_envelope < first_guarded,
            "every envelope row precedes every guarded fact: {log:?}"
        );
        assert_eq!(
            log.last().map(String::as_str),
            Some("completion"),
            "{log:?}"
        );
        assert_eq!(
            log.iter().filter(|what| *what == "sync_commit").count(),
            1,
            "one commit"
        );
    }

    /// A printer that refuses the receipt, and a drawer that does not open,
    /// never un-complete the sale: the receipt waits in the queue for its
    /// retry, the drawer is not kicked again, and the answer says both.
    #[test]
    fn hardware_failure_after_commit_leaves_sale_complete() {
        /// The simulator, behind a drawer that is jammed.
        struct Jammed(Arc<SimulatedPrinter>);

        impl ReceiptPrinter for Jammed {
            fn print(&self, doc: &RenderedReceipt) -> PrintOutcome {
                self.0.print(doc)
            }
            fn open_drawer(&self) -> Result<(), HwError> {
                Err(HwError::Offline)
            }
            fn status(&self) -> PrinterStatus {
                self.0.status()
            }
        }

        let printer = Arc::new(SimulatedPrinter::new());
        *printer.force_status.lock().unwrap() = PrinterStatus::PaperOut;
        let (_dir, register) = shop_with(Arc::new(Jammed(Arc::clone(&printer))));
        let sale = settled(&register);
        let answer = finalize(&register).unwrap();
        assert_eq!(answer.receipt, ReceiptOutcome::Refused);
        assert_eq!(answer.drawer, DrawerOutcome::Failed);
        assert!(completed(&register, sale));
        assert!(printer.printed.lock().unwrap().is_empty());
        let job = register.with_db(|conn| {
            ReceiptRepository::new(conn)
                .unprinted()
                .unwrap()
                .into_iter()
                .next()
                .unwrap()
        });
        assert_eq!(
            job.state,
            JobState::Failed,
            "the printer took no byte of it"
        );
        assert!(
            job.next_attempt_at.is_some(),
            "the refused receipt waits in the queue for its retry"
        );

        // A printer that goes silent mid-receipt: paper may exist, so a
        // person answers, and the drawer, which works, opens once.
        let printer = Arc::new(SimulatedPrinter::new());
        printer.script(Fault::SilentAfter(16));
        let (_dir, register) = shop_with(Arc::clone(&printer) as Arc<dyn ReceiptPrinter>);
        let sale = settled(&register);
        let answer = finalize(&register).unwrap();
        assert_eq!(answer.receipt, ReceiptOutcome::Unconfirmed);
        assert_eq!(answer.drawer, DrawerOutcome::Opened);
        assert_eq!(*printer.drawer_opens.lock().unwrap(), 1);
        assert!(completed(&register, sale));
    }

    /// A completed sale's facts are named by exactly one commit, and that
    /// commit is ready: its manifest names the sale, its supply context, each
    /// line, tax component, summary row, tender and first tender event, each
    /// line's stock event, the original receipt and the audit row, and each
    /// member has its delivery row.
    #[test]
    fn a_complete_sale_has_one_ready_commit_manifest() {
        let (_dir, register) = shop();
        let sale = settled(&register);
        finalize(&register).unwrap();
        register.with_db(|conn| {
            let id = sale.as_uuid().as_bytes().to_vec();
            let commit: Vec<u8> = conn
                .query_row("SELECT sync_commit_id FROM sale WHERE id = ?1", [&id], |row| {
                    row.get(0)
                })
                .unwrap();
            let ready: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sync_commit_ready WHERE id = ?1",
                    [&commit],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(ready, 1, "the sale's commit is ready");
            let mut by_entity: BTreeMap<String, i64> = BTreeMap::new();
            let mut statement = conn
                .prepare("SELECT entity, count(*) FROM fact_commit_member WHERE commit_id = ?1 GROUP BY entity")
                .unwrap();
            for row in statement
                .query_map([&commit], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
            {
                let (entity, n) = row.unwrap();
                by_entity.insert(entity, n);
            }
            let expected: BTreeMap<String, i64> = [
                ("audit_log", 1),
                ("receipt_artifact", 1),
                ("sale", 1),
                ("sale_line", 2),
                ("sale_line_tax", 2),
                ("sale_supply_tax_context", 1),
                ("sale_tax_summary", 1),
                ("sale_tender", 2),
                ("stock_ledger", 2),
                ("tender_status_event", 2),
            ]
            .into_iter()
            .map(|(entity, n)| (entity.to_owned(), n))
            .collect();
            assert_eq!(by_entity, expected);
            let members: i64 = expected.values().sum();
            let delivered: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sync_outbox o
                       JOIN fact_commit_member m ON m.change_id = o.change_id
                      WHERE m.commit_id = ?1",
                    [&commit],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(delivered, members, "every member has its delivery row");
            for (table, key) in [
                ("sale_line", "sale_id"),
                ("receipt_artifact", "sale_id"),
                ("stock_ledger", "ref_id"),
            ] {
                let missing: i64 = conn
                    .query_row(
                        &format!(
                            "SELECT count(*) FROM {table} f WHERE f.{key} = ?1 AND NOT EXISTS (
                               SELECT 1 FROM fact_commit_member m
                                WHERE m.commit_id = ?2 AND m.entity = '{table}'
                                  AND m.entity_id = f.id)"
                        ),
                        params![&id, &commit],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert_eq!(missing, 0, "every {table} row of the sale is in its manifest");
            }
            let commits: i64 = conn
                .query_row(
                    "SELECT count(DISTINCT commit_id) FROM fact_commit_member WHERE entity_id = ?1",
                    [&id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(commits, 1, "one commit names the sale");
        });
    }

    /// An envelope that leaves out any one of the sale's facts never becomes
    /// a ready commit beside a completed sale: the finalize is refused, and
    /// nothing is written.
    #[test]
    fn no_ready_commit_omits_a_required_member() {
        for entity in [
            "sale",
            "sale_supply_tax_context",
            "sale_line",
            "sale_line_tax",
            "sale_tax_summary",
            "sale_tender",
            "tender_status_event",
            "stock_ledger",
            "receipt_artifact",
            "audit_log",
        ] {
            let (_dir, register) = shop();
            let sale = settled(&register);
            let before = written(&register);
            let seam = Seam {
                drop_entity: Some(entity),
                ..Seam::default()
            };
            assert!(
                finalize_with(&register, &seam).is_err(),
                "an envelope without its {entity} is refused"
            );
            assert_eq!(
                written(&register),
                before,
                "nothing written without {entity}"
            );
            assert!(!completed(&register, sale), "without {entity}");
        }
    }

    /// The finalize reads the acting user again inside its transaction: an
    /// account deactivated, or deleted, after the command's context let it in
    /// is refused before any financial write.
    #[test]
    fn deactivated_user_denied() {
        for meddle in [
            "UPDATE app_user SET is_active = 0 WHERE id = ?1",
            "UPDATE app_user SET deleted_at = '2026-11-05T09:00:00.000Z' WHERE id = ?1",
        ] {
            let (_dir, register) = shop();
            let sale = settled(&register);
            let before = written(&register);
            let seam = Seam {
                meddle_at: Some((FinalizeWritePoint::Journal, meddle)),
                ..Seam::default()
            };
            assert_eq!(
                refusal(finalize_with(&register, &seam)).code,
                "user_inactive",
                "{meddle}"
            );
            assert_eq!(written(&register), before, "{meddle}");
            assert!(!completed(&register, sale), "{meddle}");
        }
    }

    /// The journal's row goes in the transaction that completes the sale:
    /// gone with the sale, and still there, as the finalize left it, when the
    /// commit itself fails.
    #[test]
    fn finalize_removes_the_checkout_operation_in_the_same_commit() {
        let (_dir, register) = shop();
        let sale = settled(&register);
        let seam = Seam {
            fail_at: Some(FinalizeWritePoint::Commit),
            ..Seam::default()
        };
        assert!(finalize_with(&register, &seam).is_err());
        assert_eq!(journal(&register, sale), Some("finalizing"));
        assert!(!completed(&register, sale));

        finalize(&register).unwrap();
        assert_eq!(journal(&register, sale), None);
        assert_eq!(basket(&register, sale), None, "the basket became the sale");
        assert!(completed(&register, sale));
    }

    /// The tender suite's cash sale, end to end with no printer attached: the
    /// sale reads back as it was paid for, numbered, with its stock moved, its
    /// original receipt rendered for the register's profile and queued, its
    /// audit row recording the drawer it asked for, and the till free.
    #[test]
    fn a_cash_sale_completes_with_its_receipt_rendered_and_queued() {
        let (_dir, register) = shop();
        let sale = settled(&register);
        let answer = finalize(&register).unwrap();
        assert_eq!(
            answer,
            CompletedSaleRef {
                sale_id: sale,
                receipt_number: "REG01-000001".to_owned(),
                currency: "JOD",
                total_minor: 2_110,
                change_minor: 3_890,
                receipt: ReceiptOutcome::Queued,
                drawer: DrawerOutcome::NoDevice,
            }
        );
        assert_eq!(current(&register).unwrap(), None, "the till is free");

        register.with_db(|conn| {
            let record = SaleRepository::new(conn).by_id(sale).unwrap().unwrap();
            let done = record.sale();
            assert_eq!(done.receipt_number(), "REG01-000001");
            assert_eq!(done.change().minor(), 3_890);
            assert_eq!(done.tenders().len(), 2);

            let stock = StockRepository::new(conn);
            let store = register.store();
            assert_eq!(
                stock
                    .position(product_id(OIL), store)
                    .unwrap()
                    .on_hand
                    .milli(),
                9_000
            );
            assert_eq!(
                stock
                    .position(product_id(CHEESE), store)
                    .unwrap()
                    .on_hand
                    .milli(),
                399
            );

            let receipts = ReceiptRepository::new(conn);
            let original = receipts
                .artifact(&record.original_receipt().unwrap().into_bytes())
                .unwrap()
                .unwrap()
                .artifact;
            assert_eq!(original.printer_profile, "simulator-80mm");
            assert_eq!(original.template_version, TEMPLATE_VERSION);
            assert!(
                original.content_bytes
                    == rendered(
                        done,
                        &expected_context(Language::Arabic, "متجر الاختبار", "نقدي")
                    ),
                "the original is the sale's own receipt, as the fixture's merchant prints it in \
                 Arabic, rendered for the register's profile"
            );
            assert_eq!(
                receipts.unprinted().unwrap().len(),
                1,
                "queued for a printer"
            );

            let (action, payload): (String, String) = conn
                .query_row(
                    "SELECT action, payload FROM audit_log WHERE entity_id = ?1",
                    [sale.as_uuid().as_bytes().as_slice()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(action, FINALIZE_ACTION);
            let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
            assert_eq!(*field(&payload, "receipt_number"), "REG01-000001");
            assert_eq!(*field(&payload, "total_minor"), 2_110);
            assert_eq!(
                *field(&payload, "drawer"),
                "requested",
                "cash opens the drawer"
            );
        });
    }

    /// The context the fixture's merchant prints under, written out by hand
    /// rather than read through the code under test: its organisation, its
    /// store's name, the register, the cashier, an exempt store, and cash's
    /// printed name.
    fn expected_context(language: Language, store_name: &str, cash: &str) -> ReceiptContext {
        ReceiptContext {
            merchant: MerchantBlock {
                legal_name: "Registered Chain Fixture".to_owned(),
                store_name: store_name.to_owned(),
                address: None,
                phone: None,
                tin: None,
            },
            register_code: "REG01".to_owned(),
            cashier_name: "Synthetic Cashier".to_owned(),
            language,
            fiscal: FiscalStanding::Exempt,
            tender_labels: BTreeMap::from([("cash".to_owned(), cash.to_owned())]),
            discount_labels: BTreeMap::new(),
            loyalty: None,
            footer: FooterBlock {
                return_policy: None,
                thank_you: None,
            },
        }
    }

    /// `sale`'s receipt under `context`, as the 80 mm simulator receives it.
    fn rendered(sale: &CompletedSale, context: &ReceiptContext) -> Vec<u8> {
        let model = build_receipt_model(sale, context).unwrap();
        assert_eq!(
            model.totals.total.minor(),
            2_110,
            "what the customer was charged"
        );
        escpos::encode(
            &render_receipt(&model, PaperWidth::Mm80).unwrap(),
            &PrinterProfile::simulator(PaperWidth::Mm80),
        )
        .unwrap()
    }

    /// The original receipt of `sale`, and the sale as it was completed.
    fn original_of(register: &Register, sale: SaleId) -> (Vec<u8>, CompletedSale) {
        register.with_db(|conn| {
            let record = SaleRepository::new(conn).by_id(sale).unwrap().unwrap();
            let bytes = ReceiptRepository::new(conn)
                .artifact(&record.original_receipt().unwrap().into_bytes())
                .unwrap()
                .unwrap()
                .artifact
                .content_bytes;
            (bytes, record.sale().clone())
        })
    }

    /// A store whose receipts are set in English prints its English name and
    /// each tender under its kind's English name; a kind with none prints its
    /// Arabic one rather than stopping the sale.
    #[test]
    fn an_english_receipt_prints_each_tender_under_its_english_name() {
        for (cash_en, printed) in [(Some("Cash"), "Cash"), (None, "نقدي")] {
            let (_dir, register) = shop();
            register.with_db(|conn| {
                conn.execute(
                    "UPDATE store SET receipt_locale = 'en', name_en = 'Synthetic Store'",
                    [],
                )
                .unwrap();
                conn.execute(
                    "UPDATE tender_type SET name_en = ?1 WHERE code = 'cash'",
                    [cash_en],
                )
                .unwrap();
            });
            let sale = settled(&register);
            finalize(&register).unwrap();
            let (bytes, done) = original_of(&register, sale);
            assert!(
                bytes
                    == rendered(
                        &done,
                        &expected_context(Language::English, "Synthetic Store", printed)
                    ),
                "cash prints as {printed}"
            );
        }
    }

    /// What could never complete is refused before the journal is touched, so
    /// the sale is still being paid for, exactly as it was.
    #[test]
    fn a_finalize_refuses_what_could_never_complete_and_writes_nothing() {
        let (_dir, register) = shop();
        open_cart(&register).unwrap();
        add_product(&register, product_id(OIL), None).unwrap();
        let paying = begin_payment(&register).unwrap();
        add_cash(&register, 1_000).unwrap();
        let before = written(&register);
        assert_eq!(refusal(finalize(&register)).code, "sale_not_settled");
        assert_eq!(written(&register), before);
        assert_eq!(journal(&register, paying.sale_id), Some("tendering"));
        add_cash(&register, 250).unwrap();

        for (meddle, code) in [
            (
                "UPDATE store SET gst_profile = 'unregistered'",
                "store_not_gst_registered",
            ),
            (
                "UPDATE store SET fiscal_obligation = 'pending_evidence', \
                 fiscal_obligation_evidence_ref = NULL",
                "fiscal_obligation_undecided",
            ),
        ] {
            let before = written(&register);
            let saved: (String, String, Option<String>) = register.with_db(|conn| {
                conn.query_row(
                    "SELECT gst_profile, fiscal_obligation, fiscal_obligation_evidence_ref FROM store",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap()
            });
            register.with_db(|conn| conn.execute(meddle, []).unwrap());
            assert_eq!(refusal(finalize(&register)).code, code);
            assert_eq!(written(&register), before, "{code}");
            assert_eq!(
                journal(&register, paying.sale_id),
                Some("tendering"),
                "{code}"
            );
            register.with_db(|conn| {
                conn.execute(
                    "UPDATE store SET gst_profile = ?1, fiscal_obligation = ?2,
                                      fiscal_obligation_evidence_ref = ?3",
                    params![saved.0, saved.1, saved.2],
                )
                .unwrap()
            });
        }
        assert_eq!(finalize(&register).unwrap().total_minor, 1_250);
    }
}

#[cfg(test)]
mod business_date_tests {
    //! Microstep 1.9.4: a sale's business date is its shift's (conventions
    //! §11). The shift's date was resolved once, when it opened, from the
    //! store's zone and day cutover; the finalize copies it and never
    //! recomputes it from the clock's midnight or a stored offset.
    //!
    //! The shop's shift opened on 5 November 2026 in Amman, UTC+3, whose day
    //! cuts over at 04:00. Each register here is opened with its clock at the
    //! instant a test names, so the sale is rung up then, under that shift.
    //! Nothing in the shell, the domain or the database layer reads the
    //! operating system's zone, so the only zone that could move a recomputed
    //! date is the store's own, and that is the one a test moves.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use pos_domain::{BusinessDate, FixedClock};

    use super::*;
    use crate::commands::auth::login_pin;
    use crate::commands::cart::{add_product, open_cart};
    use crate::commands::tender::{add_cash, begin_payment};
    use crate::shop::{CASHIER_CODE, CASHIER_PIN, DAY, KEY, OIL, product_id, seed};
    use crate::state::V7Ids;

    /// A shop whose shift opened on `DAY`, its register's clock at `instant`
    /// and its store as `store` leaves it, the cashier signed in.
    fn shop_at(instant: &str, store: &str) -> (tempfile::TempDir, Register) {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), true);
        let conn = pos_db::open(&dir.path().join(crate::state::DATABASE_FILE), KEY).unwrap();
        conn.execute_batch(store).unwrap();
        drop(conn);
        let register = Register::open_with(
            dir.path(),
            KEY,
            Box::new(FixedClock::new(Timestamp::parse_iso8601(instant).unwrap())),
            Box::new(V7Ids::default()),
        )
        .unwrap();
        login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
        (dir, register)
    }

    /// A bottle of oil, paid exactly in cash and finalized.
    fn sell(register: &Register) -> SaleId {
        open_cart(register).unwrap();
        add_product(register, product_id(OIL), None).unwrap();
        let paying = begin_payment(register).unwrap();
        add_cash(register, paying.total_minor).unwrap();
        finalize(register).unwrap().sale_id
    }

    /// The business date the sale completed under, and the one each of its
    /// stock events was posted under.
    fn dated(register: &Register, sale: SaleId) -> (BusinessDate, Vec<String>) {
        register.with_db(|conn| {
            let record = SaleRepository::new(conn).by_id(sale).unwrap().unwrap();
            let stock: Vec<String> = conn
                .prepare("SELECT business_date FROM stock_ledger WHERE ref_id = ?1")
                .unwrap()
                .query_map([sale.as_uuid().as_bytes().as_slice()], |row| row.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (record.sale().business_date(), stock)
        })
    }

    fn the_shifts_day() -> BusinessDate {
        BusinessDate::parse(DAY).unwrap()
    }

    /// 01:00 in Amman on 6 November is 22:00 UTC on the 5th: past calendar
    /// midnight, before the store's 04:00 cutover. The sale belongs to the
    /// trading day that began the evening before, the shift's.
    #[test]
    fn sale_at_0100_belongs_to_previous_business_date() {
        let (_dir, register) = shop_at("2026-11-05T22:00:00.000Z", "");
        let sale = sell(&register);
        let (date, stock) = dated(&register, sale);
        assert_eq!(date, the_shifts_day());
        assert_eq!(stock, [DAY], "its stock event is posted on the same day");
    }

    /// The store's zone moves fourteen hours, to Kiritimati, and the clock
    /// runs on to 13:00 UTC on 6 November, 03:00 on the 7th there. A date
    /// recomputed from either would move; the sale's stays the shift's.
    #[test]
    fn business_date_survives_timezone_change() {
        let (_dir, register) = shop_at(
            "2026-11-06T13:00:00.000Z",
            "UPDATE store SET time_zone = 'Pacific/Kiritimati';",
        );
        let sale = sell(&register);
        let (date, stock) = dated(&register, sale);
        assert_eq!(date, the_shifts_day());
        assert_eq!(stock, [DAY]);
    }

    /// A clock weeks from the date the shift opened on, as an untrusted
    /// clock or an operator-confirmed date leaves it: the sale is dated by
    /// its shift, and so are its receipt and its stock.
    #[test]
    fn a_sale_uses_its_shifts_business_date() {
        let (_dir, register) = shop_at("2026-12-31T09:00:00.000Z", "");
        let sale = sell(&register);
        let (date, stock) = dated(&register, sale);
        assert_eq!(date, the_shifts_day());
        assert_eq!(stock, [DAY]);
        let completed_at = register.with_db(|conn| {
            SaleRepository::new(conn)
                .by_id(sale)
                .unwrap()
                .unwrap()
                .sale()
                .completed_at()
        });
        assert_eq!(
            completed_at.to_iso8601(),
            "2026-12-31T09:00:00.000Z",
            "the instant is the clock's; only the trading day is the shift's"
        );
    }
}
