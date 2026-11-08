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

