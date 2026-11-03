//! A register a sale can be completed on, and the finalize transaction 1.8.3
//! will write, shared by the suites that need a real completed sale
//! (`sale_repository_api.rs`, `parked_cart.rs`).
//!
//! Every sale here is made by the domain: a cart, priced by `begin_tender`
//! with its store's own pack and policy, paid for, and completed. Every
//! finalize is written the way 1.8.3's will be: the sale prepared, one envelope
//! naming every fact, then the sale's rows, the facts that wait on them, and the
//! completion, in one transaction. Nothing reads `ref/schema.md`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use pos_db::DbError;
use pos_db::fixtures::registered_chain::RegisteredChain;
use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::audit::AuditRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::receipt::{ArtifactKind, ReceiptArtifact, ReceiptRepository};
use pos_db::repo::sale::{PreparedSale, SaleRepository};
use pos_db::repo::stock::{PreparedStockEvent, StockRepository};
use pos_db::tax_pack::TaxPackRepository;
use pos_db::tax_policy::TaxPolicyRepository;
use pos_domain::cart::{add_tender, begin_finalize, begin_tender, complete};
use pos_domain::{
    ApprovalBinding, ApprovalHandle, ApprovalId, AuditIntent, BasketDiscount, BusinessDate,
    Capability, Cart, CartLine, CashRoundingPolicy, CompletedSale, Currency, CustomerId,
    DerivedWeight, DiscountKind, DiscountReason, EscalationPolicy, FinalizeEffects, GrantSet,
    LineDiscount, Money, NewStockEvent, OverrideReason, Percent, PriceMode, PriceOrigin,
    PriceOverride, PricingSettings, ProductId, Qty, RegisterId, Role, SaleId, SaleLineId,
    SeqIdSource, ShiftId, StockEventId, StockEventKind, StockSource, StoreId, StoreTaxProfile,
    SupplyDestination, SupplyTaxContext, TaxBands, TaxCategoryId, Tender, TenderId, TenderState,
    Timestamp, UnitOfMeasure, UserId, authorize, cap, remaining_due,
};
use rusqlite::{Connection, Transaction, params};
use serde_json::json;
use uuid::Uuid;

pub const KEY: &str = "test-key";
/// The fixture's instant and the business date of its shift.
pub const AT: &str = "2026-08-25T10:00:00.000Z";
pub const DAY: &str = "2026-08-25";
pub const REGISTER: u8 = 0x51;
pub const OTHER_REGISTER: u8 = 0x52;
pub const SHIFT: u8 = 0x53;
pub const MANAGER: u8 = 0x54;
/// Synthetic throughout: an invented business buyer and an invented card.
pub const BUYER_TIN: &str = "100200300";
pub const BUYER_NAME: &str = "Synthetic Trading Est.";
pub const MASKED_PAN: &str = "************4242";
/// What the manager's approval of the basket discount is bound to: its share,
/// in ppm, which is the amount `discount.manual` is escalated on.
pub const BASKET_APPROVAL_MINOR: i64 = 50_000;

pub fn uuid(byte: u8) -> Uuid {
    Uuid::from_bytes([byte; 16])
}

pub fn at() -> Timestamp {
    Timestamp::parse_iso8601(AT).unwrap()
}

pub fn day() -> BusinessDate {
    BusinessDate::parse(DAY).unwrap()
}

pub fn jod(minor: i64) -> Money {
    Money::from_minor(minor, Currency::JOD)
}

/// A register on the reference chain, with an open shift, a manager who can
/// approve, and three products: an item, a tobacco pack, and cheese sold by a
/// scale's price label.
pub struct World {
    pub conn: Connection,
    pub _dir: tempfile::TempDir,
    pub chain: RegisteredChain,
    pub item: ProductId,
    pub tobacco: ProductId,
    pub cheese: ProductId,
    /// The price mode every sale here is priced, and written, under.
    pub mode: PriceMode,
}

impl World {
    pub fn new() -> World {
        let dir = tempfile::tempdir().unwrap();
        let conn = pos_db::open(&dir.path().join("sales.db"), KEY).unwrap();
        let chain = RegisteredChain::seed(&conn);
        chain.add_register(&conn, &[REGISTER; 16], "REG01");
        chain.add_register(&conn, &[OTHER_REGISTER; 16], "REG02");
        chain.open_shift(&conn, &[SHIFT; 16], &[REGISTER; 16], DAY);
        conn.execute(
            "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
             VALUES (?1, ?2, 'MGR-1', 'Fixture Manager', 'placeholder-not-a-hash', ?3)",
            params![uuid(MANAGER).as_bytes().as_slice(), &chain.org, AT],
        )
        .unwrap();
        let product = |byte: u8, sku: &str, unit: &str, step: i64, weighed: i64| {
            conn.execute(
                "INSERT INTO product
                   (id, sku, name, name_ar, price_minor, currency, is_active,
                    tax_category_id, unit, qty_step_milli, is_weighed)
                 VALUES (?1, ?2, ?2, ?2, 1000, 'JOD', 1, ?3, ?4, ?5, ?6)",
                params![
                    uuid(byte).as_bytes().as_slice(),
                    sku,
                    &chain.tax_category,
                    unit,
                    step,
                    weighed
                ],
            )
            .unwrap();
            ProductId::from_uuid(uuid(byte))
        };
        let item = product(0x61, "SYN-ITEM", "each", 1_000, 0);
        let tobacco = product(0x62, "SYN-TOBACCO", "each", 1_000, 0);
        let cheese = product(0x63, "SYN-CHEESE", "weight", 5, 1);
        World {
            conn,
            _dir: dir,
            chain,
            item,
            tobacco,
            cheese,
            mode: PriceMode::Inclusive,
        }
    }

    /// The same register in a store whose prices exclude tax.
    pub fn exclusive() -> World {
        World {
            mode: PriceMode::Exclusive,
            ..World::new()
        }
    }

    pub fn store(&self) -> StoreId {
        StoreId::from_uuid(Uuid::from_slice(&self.chain.store).unwrap())
    }

    pub fn cashier(&self) -> UserId {
        UserId::from_uuid(Uuid::from_slice(&self.chain.cashier).unwrap())
    }

    pub fn category(&self) -> TaxCategoryId {
        TaxCategoryId::from_uuid(Uuid::from_slice(&self.chain.tax_category).unwrap())
    }

    pub fn sales(&self) -> SaleRepository<'_> {
        SaleRepository::new(&self.conn)
    }

    /// A cart opened on the register's shift, with no line.
    pub fn cart(&self, sale: u8) -> Cart {
        Cart {
            id: SaleId::from_uuid(uuid(sale)),
            register_id: RegisterId::from_uuid(uuid(REGISTER)),
            shift_id: ShiftId::from_uuid(uuid(SHIFT)),
            cashier_id: self.cashier(),
            currency: Currency::JOD,
            lines: Vec::new(),
            basket_discount: None,
            customer_id: None,
            buyer_tin: None,
            buyer_name: None,
            supply: SupplyTaxContext {
                destination: SupplyDestination::Domestic,
                reason: None,
                evidence_ref: None,
            },
            is_training: false,
            opened_at: Timestamp::parse_iso8601("2026-08-25T09:58:30.250Z").unwrap(),
        }
    }

    /// A line of `product`, captured from the catalogue, with nothing on it.
    pub fn line(&self, id: u8, product: ProductId, price: i64, qty_milli: i64) -> CartLine {
        CartLine {
            id: SaleLineId::from_uuid(uuid(id)),
            product_id: Some(product),
            name_snapshot: format!("Synthetic item {id:#04x}"),
            unit_price: jod(price),
            price_origin: PriceOrigin::Catalog {
                product_id: product,
            },
            max_price: None,
            qty: Qty::from_milli(qty_milli),
            unit: UnitOfMeasure::Each,
            is_weighed: false,
            tax_category_id: self.category(),
            discount: None,
            price_override: None,
            min_age: None,
            age_confirmed: false,
            label: None,
        }
    }

    /// `cart` priced under its store's pack and policy, paid for with
    /// `tenders` and then cash for whatever is left, and completed.
    pub fn complete(
        &self,
        cart: Cart,
        tenders: Vec<Tender>,
        cash: i64,
        receipt: &str,
    ) -> CompletedSale {
        let policy = TaxPolicyRepository::new(&self.conn)
            .approved_for_store(self.store())
            .unwrap();
        let bands: TaxBands = [(
            self.category(),
            TaxPackRepository::new(&self.conn)
                .components_for(self.store(), self.category(), day())
                .unwrap(),
        )]
        .into_iter()
        .collect();
        let settings = PricingSettings {
            price_mode: self.mode,
            profile: StoreTaxProfile::Standard,
            rule: policy.rounding_rule(),
        };
        let cash_policy = CashRoundingPolicy {
            step_minor: policy.cash_round_step_minor(),
            direction: policy.cash_round_direction(),
        };
        let mut t = begin_tender(cart, &bands, &settings, cash_policy).unwrap();
        for tender in tenders {
            t = add_tender(t, tender).unwrap();
        }
        if !remaining_due(&t).unwrap().is_zero() {
            t = add_tender(t, tender(0xC9, "cash", cash)).unwrap();
        }
        complete(
            begin_finalize(t).unwrap(),
            FinalizeEffects {
                receipt_number: receipt.to_owned(),
                business_date: day(),
                completed_at: at(),
            },
        )
        .unwrap()
    }

    /// The sale with every optional fact a completed sale can carry: a basket
    /// discount a manager approved, a line discount, an override, an age gate,
    /// a price ceiling, a scale label, a business buyer, a customer, a card
    /// tender and a cash one that is rounded and gives change.
    pub fn everything(&self) -> CompletedSale {
        let mut cart = self.cart(0x70);
        let mut item = self.line(0x71, self.item, 1_247, 2_000);
        item.max_price = Some(jod(1_500));
        item.discount = Some(LineDiscount {
            kind: DiscountKind::Amount(jod(100)),
            reason: DiscountReason("loyal_customer".to_owned()),
            by: self.cashier(),
            approver: None,
        });
        let mut tobacco = self.line(0x72, self.tobacco, 2_500, 1_000);
        tobacco.min_age = Some(18);
        tobacco.age_confirmed = true;
        tobacco.price_override = Some(PriceOverride {
            to: jod(2_350),
            reason: OverrideReason::DisplayedPrice,
            by: self.cashier(),
            approver: Some(UserId::from_uuid(uuid(MANAGER))),
        });
        let mut cheese = self.line(0x73, self.cheese, 416, 1_000);
        cheese.price_origin = PriceOrigin::Label {
            rule_prefix: "20".to_owned(),
            raw_code: "2012345004165".to_owned(),
        };
        cheese.unit = UnitOfMeasure::Package;
        cheese.is_weighed = true;
        cheese.label = Some(DerivedWeight {
            qty: Qty::from_milli(210),
            catalog_unit_price: jod(1_999),
        });
        cart.lines = vec![item, tobacco, cheese];
        cart.basket_discount = Some(BasketDiscount {
            kind: DiscountKind::Percent(Percent::from_ppm(50_000)),
            reason: DiscountReason("store_promotion".to_owned()),
            by: self.cashier(),
            approver: Some(UserId::from_uuid(uuid(MANAGER))),
        });
        cart.customer_id = Some(CustomerId::from_uuid(uuid(0x7C)));
        cart.buyer_tin = Some(BUYER_TIN.to_owned());
        cart.buyer_name = Some(BUYER_NAME.to_owned());
        let card = Tender {
            psp_ref: Some("SYN-PSP-0001".to_owned()),
            masked_pan: Some(MASKED_PAN.to_owned()),
            scheme: Some("visa".to_owned()),
            ..tender(0xC1, "card", 2_000)
        };
        self.complete(cart, vec![card], 10_000, "REG01-000001")
    }

    /// One item, paid exactly by card: no discount, override, age gate, label,
    /// buyer, customer, rounding or change.
    pub fn plain(&self, sale: u8, receipt: &str) -> CompletedSale {
        let mut cart = self.cart(sale);
        cart.lines = vec![self.line(sale.wrapping_add(1), self.item, 1_000, 1_000)];
        let card = tender(sale.wrapping_add(2), "card", 1_000);
        self.complete(cart, vec![card], 0, receipt)
    }

    /// The finalize transaction, written as 1.8.3's will be. `sabotage` may
    /// change the envelope's members, and `skip_receipt` leaves the original
    /// receipt unwritten, so a test can watch what each guard refuses.
    pub fn finalize(
        &self,
        sale: &CompletedSale,
        commit: u8,
        sabotage: impl FnOnce(&mut Vec<(String, [u8; 16], String)>),
        skip_receipt: bool,
    ) -> Result<PreparedSale, DbError> {
        self.finalize_with(sale, commit, sabotage, skip_receipt, |_| Ok(()))
    }

    /// [`Self::finalize`], with `extra` run inside the same transaction just
    /// before the sale completes: what else a finalize writes, such as
    /// removing the working cart a resumed sale came from (1.8.2b).
    pub fn finalize_with(
        &self,
        sale: &CompletedSale,
        commit: u8,
        sabotage: impl FnOnce(&mut Vec<(String, [u8; 16], String)>),
        skip_receipt: bool,
        extra: impl FnOnce(&Transaction<'_>) -> Result<(), DbError>,
    ) -> Result<PreparedSale, DbError> {
        let tx = self.conn.unchecked_transaction().unwrap();
        let ids = SeqIdSource::new(1_788_000_000_000, u16::from(commit));
        let prepared = self
            .sales()
            .prepare(&tx, sale, self.mode, &[commit; 16], &ids)?;
        let sale_id = sale.cart().id.as_uuid();
        let register = sale.cart().register_id;

        let movements: Vec<NewStockEvent> = sale
            .cart()
            .lines
            .iter()
            .zip(1_u8..)
            .map(|(line, n)| NewStockEvent {
                id: StockEventId::from_uuid(Uuid::from_bytes([
                    commit, 0x5E, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                ])),
                product_id: line.product_id.unwrap(),
                store_id: self.store(),
                kind: StockEventKind::Sale,
                qty_delta: Qty::from_milli(-line.label.map_or(line.qty, |l| l.qty).milli()),
                incoming_cost: None,
                is_weight_derived: line.label.is_some(),
                source: Some(StockSource::SaleLine {
                    sale_id: sale.cart().id,
                    line_id: line.id,
                }),
                actor_id: Some(sale.cart().cashier_id),
                occurred_at: at(),
                business_date: day(),
            })
            .collect();
        let stock = StockRepository::new(&self.conn)
            .prepare(&tx, register, &movements)
            .unwrap();

        let artifact = ReceiptArtifact {
            id: [commit, 0xA7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            sale_id: *sale_id.as_bytes(),
            kind: ArtifactKind::Original,
            source_artifact_id: None,
            format: "escpos".to_owned(),
            template_version: "synthetic-v1".to_owned(),
            printer_profile: "80mm".to_owned(),
            fiscal_version: None,
            content_bytes: b"synthetic receipt bytes".to_vec(),
            generated_at: at(),
        };
        let completion_audit =
            Uuid::from_bytes([commit, 0xAD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        let approval = sale
            .cart()
            .basket_discount
            .as_ref()
            .and_then(|d| d.approver)
            .map(|manager| self.approval(sale_id, manager, commit));

        let mut members: Vec<(String, [u8; 16], String)> = prepared
            .members()
            .iter()
            .map(|m| {
                (
                    m.entity().to_owned(),
                    *m.entity_id(),
                    m.payload().to_owned(),
                )
            })
            .collect();
        members.extend(stock.iter().map(|e| {
            (
                "stock_ledger".to_owned(),
                *e.event().id.as_uuid().as_bytes(),
                e.payload(),
            )
        }));
        members.push(("receipt_artifact".to_owned(), artifact.id, "{}".to_owned()));
        members.push((
            "audit_log".to_owned(),
            *completion_audit.as_bytes(),
            "{}".to_owned(),
        ));
        if let Some((handle, audit)) = &approval {
            let handle_id = *handle.id().as_uuid().as_bytes();
            members.push(("approval_handle".to_owned(), handle_id, "{}".to_owned()));
            members.push(("audit_log".to_owned(), *audit.as_bytes(), "{}".to_owned()));
            members.push((
                "approval_consumption".to_owned(),
                handle_id,
                "{}".to_owned(),
            ));
        }
        sabotage(&mut members);
        write_envelope(&self.conn, &tx, commit, &members);

        self.sales()
            .insert_complete(&tx, &prepared, |tx| -> Result<(), DbError> {
                append_stock(&self.conn, tx, &stock)?;
                if !skip_receipt {
                    let receipts = ReceiptRepository::new(&self.conn);
                    receipts.append_artifact(tx, &artifact)?;
                    receipts.queue_initial(
                        tx,
                        &[commit, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
                        &artifact.id,
                        at(),
                    )?;
                }
                let audit = AuditRepository::new(&self.conn);
                audit.append(
                    tx,
                    register,
                    completion_audit,
                    &AuditIntent {
                        actor: sale.cart().cashier_id,
                        approver: None,
                        approval: None,
                        action: "sale.complete",
                        entity: "sale",
                        entity_id: sale_id,
                        reason: None,
                        payload: json!({ "total_minor": sale.priced().total.minor() }),
                        at: at(),
                    },
                )?;
                if let Some((handle, audit_id)) = &approval {
                    let approvals = ApprovalRepository::new(&self.conn);
                    approvals.insert(tx, handle)?;
                    audit.append(
                        tx,
                        register,
                        *audit_id,
                        &AuditIntent {
                            actor: sale.cart().cashier_id,
                            approver: Some(handle.approver()),
                            approval: Some(handle.id()),
                            action: cap::DiscountManual::NAME,
                            entity: "sale",
                            entity_id: sale_id,
                            reason: Some(handle.reason().to_owned()),
                            payload: json!({ "amount_minor": BASKET_APPROVAL_MINOR }),
                            at: at(),
                        },
                    )?;
                    approvals.consume(tx, handle.id(), sale_id, *audit_id, at())?;
                }
                extra(tx)
            })?;
        tx.commit().unwrap();
        Ok(prepared)
    }

    /// The manager's approval of the basket discount, bound to the sale. Its
    /// nonce is derived from the sale's id rather than written as a literal.
    pub fn approval(&self, sale: Uuid, manager: UserId, commit: u8) -> (ApprovalHandle, Uuid) {
        let binding = ApprovalBinding {
            entity_id: sale,
            amount_minor: BASKET_APPROVAL_MINOR,
            content_hash: None,
        };
        let authorized = authorize::<cap::DiscountManual>(
            manager,
            &GrantSet::of_role(Role::Manager),
            None,
            &binding,
            &EscalationPolicy::empty(),
            at(),
        )
        .unwrap();
        let handle = ApprovalHandle::issue(
            ApprovalId::from_uuid(Uuid::from_bytes([
                commit, 0xAA, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
            ])),
            self.cashier(),
            &authorized,
            &binding,
            "store_promotion".to_owned(),
            at(),
            5 * 60 * 1_000,
            sale.as_bytes().map(|b| b ^ commit),
        )
        .unwrap();
        (
            handle,
            Uuid::from_bytes([commit, 0xAD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]),
        )
    }
}

pub fn tender(byte: u8, code: &str, minor: i64) -> Tender {
    Tender {
        id: TenderId::from_uuid(uuid(byte)),
        type_code: code.to_owned(),
        amount: jod(minor),
        psp_ref: None,
        masked_pan: None,
        scheme: None,
        state: TenderState::Collected,
    }
}

pub fn append_stock(
    conn: &Connection,
    tx: &Transaction<'_>,
    stock: &[PreparedStockEvent],
) -> Result<(), DbError> {
    StockRepository::new(conn).append(tx, stock)
}

/// The envelope, through the shipped writer, with a change id per member.
pub fn write_envelope(
    conn: &Connection,
    tx: &Transaction<'_>,
    commit: u8,
    members: &[(String, [u8; 16], String)],
) {
    let changes: Vec<[u8; 16]> = (0_u8..)
        .take(members.len())
        .map(|n| [commit, 0xCC, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        .collect();
    let facts: Vec<FactMember<'_>> = members
        .iter()
        .zip(&changes)
        .map(|((entity, entity_id, payload), change_id)| FactMember {
            change_id,
            entity,
            entity_id,
            payload,
        })
        .collect();
    OutboxRepository::new(conn)
        .write_commit(
            tx,
            &CommitEnvelope {
                commit_id: &[commit; 16],
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "sale-repository-test",
                created_at: AT,
            },
            &facts,
        )
        .unwrap();
}

pub fn no_sabotage(_: &mut Vec<(String, [u8; 16], String)>) {}
