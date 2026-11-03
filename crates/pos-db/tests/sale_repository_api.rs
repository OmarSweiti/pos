//! Microstep 1.8.2 — `SaleRepository`, against the exact registered chain.
//!
//! Every sale here is made by the domain: a cart, priced by `begin_tender`
//! with its store's own pack and policy, paid for, and completed. Every
//! finalize here is written the way 1.8.3's will be: the sale prepared, one
//! envelope naming every fact, then the sale's rows, the facts that wait on
//! them, and the completion, in one transaction. Nothing reads `ref/schema.md`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::DbError;
use pos_db::fixtures::registered_chain::RegisteredChain;
use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::audit::AuditRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::receipt::{ArtifactKind, ReceiptArtifact, ReceiptRepository};
use pos_db::repo::sale::{PreparedSale, SaleRecord, SaleRepository};
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

const KEY: &str = "test-key";
/// The fixture's instant and the business date of its shift.
const AT: &str = "2026-08-25T10:00:00.000Z";
const DAY: &str = "2026-08-25";
const REGISTER: u8 = 0x51;
const OTHER_REGISTER: u8 = 0x52;
const SHIFT: u8 = 0x53;
const MANAGER: u8 = 0x54;
/// Synthetic throughout: an invented business buyer and an invented card.
const BUYER_TIN: &str = "100200300";
const BUYER_NAME: &str = "Synthetic Trading Est.";
const MASKED_PAN: &str = "************4242";
/// What the manager's approval of the basket discount is bound to: its share,
/// in ppm, which is the amount `discount.manual` is escalated on.
const BASKET_APPROVAL_MINOR: i64 = 50_000;

fn uuid(byte: u8) -> Uuid {
    Uuid::from_bytes([byte; 16])
}

fn at() -> Timestamp {
    Timestamp::parse_iso8601(AT).unwrap()
}

fn day() -> BusinessDate {
    BusinessDate::parse(DAY).unwrap()
}

fn jod(minor: i64) -> Money {
    Money::from_minor(minor, Currency::JOD)
}

/// A register on the reference chain, with an open shift, a manager who can
/// approve, and three products: an item, a tobacco pack, and cheese sold by a
/// scale's price label.
struct World {
    conn: Connection,
    _dir: tempfile::TempDir,
    chain: RegisteredChain,
    item: ProductId,
    tobacco: ProductId,
    cheese: ProductId,
    /// The price mode every sale here is priced, and written, under.
    mode: PriceMode,
}

impl World {
    fn new() -> World {
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
    fn exclusive() -> World {
        World {
            mode: PriceMode::Exclusive,
            ..World::new()
        }
    }

    fn store(&self) -> StoreId {
        StoreId::from_uuid(Uuid::from_slice(&self.chain.store).unwrap())
    }

    fn cashier(&self) -> UserId {
        UserId::from_uuid(Uuid::from_slice(&self.chain.cashier).unwrap())
    }

    fn category(&self) -> TaxCategoryId {
        TaxCategoryId::from_uuid(Uuid::from_slice(&self.chain.tax_category).unwrap())
    }

    fn sales(&self) -> SaleRepository<'_> {
        SaleRepository::new(&self.conn)
    }

    /// A cart opened on the register's shift, with no line.
    fn cart(&self, sale: u8) -> Cart {
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
    fn line(&self, id: u8, product: ProductId, price: i64, qty_milli: i64) -> CartLine {
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
    fn complete(
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
    fn everything(&self) -> CompletedSale {
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
    fn plain(&self, sale: u8, receipt: &str) -> CompletedSale {
        let mut cart = self.cart(sale);
        cart.lines = vec![self.line(sale.wrapping_add(1), self.item, 1_000, 1_000)];
        let card = tender(sale.wrapping_add(2), "card", 1_000);
        self.complete(cart, vec![card], 0, receipt)
    }

    /// The finalize transaction, written as 1.8.3's will be. `sabotage` may
    /// change the envelope's members, and `skip_receipt` leaves the original
    /// receipt unwritten, so a test can watch what each guard refuses.
    fn finalize(
        &self,
        sale: &CompletedSale,
        commit: u8,
        sabotage: impl FnOnce(&mut Vec<(String, [u8; 16], String)>),
        skip_receipt: bool,
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
                Ok(())
            })?;
        tx.commit().unwrap();
        Ok(prepared)
    }

    /// The manager's approval of the basket discount, bound to the sale.
    fn approval(&self, sale: Uuid, manager: UserId, commit: u8) -> (ApprovalHandle, Uuid) {
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
            [commit, 0x0E, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
        )
        .unwrap();
        (
            handle,
            Uuid::from_bytes([commit, 0xAD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]),
        )
    }
}

fn tender(byte: u8, code: &str, minor: i64) -> Tender {
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

fn append_stock(
    conn: &Connection,
    tx: &Transaction<'_>,
    stock: &[PreparedStockEvent],
) -> Result<(), DbError> {
    StockRepository::new(conn).append(tx, stock)
}

/// The envelope, through the shipped writer, with a change id per member.
fn write_envelope(
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

fn no_sabotage(_: &mut Vec<(String, [u8; 16], String)>) {}

/// 1.8.2: a sale read back is the sale that was written, field for field, and
/// with it the facts its finalize wrote beside it: its store, its tax policy,
/// its commit, its original receipt, the approval spent on it, and where each
/// tender stands.
#[test]
fn roundtrip_preserves_every_field() {
    let world = World::new();
    let sale = world.everything();
    // The fixture is only worth something if it carries what it claims to.
    let cart = sale.cart();
    assert!(cart.basket_discount.is_some() && cart.buyer_tin.is_some());
    assert!(cart.lines.iter().any(|l| l.discount.is_some()));
    assert!(cart.lines.iter().any(|l| l.price_override.is_some()));
    assert!(
        cart.lines
            .iter()
            .any(|l| l.min_age.is_some() && l.max_price.is_none())
    );
    assert!(cart.lines.iter().any(|l| l.label.is_some()));
    assert!(
        sale.cash_rounding()
            .is_some_and(|r| !r.adjustment.is_zero())
    );
    assert!(!sale.change().is_zero() && sale.tenders().len() == 2);

    world.finalize(&sale, 0xE1, no_sabotage, false).unwrap();
    let record = world
        .sales()
        .by_id(sale.cart().id)
        .unwrap()
        .expect("the completed sale reads back");

    assert_eq!(
        record.sale(),
        &sale,
        "the same CompletedSale, field for field"
    );
    assert_eq!(record.store(), world.store());
    assert_eq!(
        record.tax_policy().as_bytes().as_slice(),
        world.chain.tax_policy.as_slice()
    );
    assert_eq!(record.sync_commit(), Uuid::from_bytes([0xE1; 16]));
    assert_eq!(
        record.original_receipt(),
        Some(Uuid::from_bytes([
            0xE1, 0xA7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1
        ]))
    );
    assert_eq!(
        record.approvals(),
        [ApprovalId::from_uuid(Uuid::from_bytes([
            0xE1, 0xAA, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1
        ]))]
    );
    assert_eq!(
        record.tender_states(),
        [
            (TenderId::from_uuid(uuid(0xC1)), TenderState::Collected),
            (TenderId::from_uuid(uuid(0xC9)), TenderState::Collected),
        ]
    );

    // The same record, however it is asked for.
    let by_receipt = world
        .sales()
        .by_receipt_number(RegisterId::from_uuid(uuid(REGISTER)), "REG01-000001")
        .unwrap();
    assert_eq!(by_receipt.as_ref(), Some(&record));
    assert_eq!(
        world
            .sales()
            .for_business_date(world.store(), day())
            .unwrap(),
        [record]
    );
}

/// A round trip that only ever stores `Some` proves half the mapping. A sale
/// with none of the optional facts reads back with none of them, not with a
/// zero, an empty string or a default where an absence was.
#[test]
fn a_sale_with_nothing_optional_round_trips() {
    let world = World::new();
    let sale = world.plain(0x20, "REG01-000001");
    assert_eq!(sale.cash_rounding(), None);
    world.finalize(&sale, 0xE2, no_sabotage, false).unwrap();

    let record = world.sales().by_id(sale.cart().id).unwrap().unwrap();
    assert_eq!(record.sale(), &sale);
    assert!(record.approvals().is_empty());
}

/// The columns `0009` added hold what the cart captured, under the names a
/// report reads. A round trip alone cannot show it: a writer and a reader that
/// agreed on the wrong column would still agree.
#[test]
fn the_capture_columns_hold_what_the_sale_captured() {
    let world = World::new();
    let sale = world.everything();
    world.finalize(&sale, 0xE3, no_sabotage, false).unwrap();
    let conn = &world.conn;
    let id = sale.cart().id.as_uuid();

    let header: (
        String,
        String,
        Option<i64>,
        i64,
        String,
        i64,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT opened_at, price_mode, cash_rounding_original_minor, rounding_adj_minor,
                    basket_discount_kind, basket_discount_ppm, buyer_id_scheme, buyer_id_value
               FROM sale WHERE id = ?1",
            [id.as_bytes().as_slice()],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .unwrap();
    let rounding = sale.cash_rounding().unwrap();
    assert_eq!(
        header,
        (
            "2026-08-25T09:58:30.250Z".to_owned(),
            "inclusive".to_owned(),
            Some(rounding.original.minor()),
            rounding.adjustment.minor(),
            "percent".to_owned(),
            50_000,
            "TN".to_owned(),
            BUYER_TIN.to_owned(),
        )
    );

    // Each line's captured columns, quoted as SQLite quotes them, so a NULL
    // reads as NULL and never as an empty string.
    let lines: Vec<String> = conn
        .prepare(
            "SELECT quote(line_no) || ' ' || quote(unit) || ' ' || quote(price_origin) || ' ' ||
                    quote(label_raw_code) || ' ' || quote(label_qty_milli) || ' ' ||
                    quote(min_age) || ' ' || quote(is_age_confirmed) || ' ' ||
                    quote(max_price_minor) || ' ' || quote(line_discount_kind) || ' ' ||
                    quote(override_price_minor)
               FROM sale_line WHERE sale_id = ?1 ORDER BY line_no",
        )
        .unwrap()
        .query_map([id.as_bytes().as_slice()], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        lines,
        [
            "1 'each' 'catalog' NULL NULL NULL 0 1500 'amount' NULL",
            "2 'each' 'catalog' NULL NULL 18 1 NULL NULL 2350",
            "3 'package' 'label' '2012345004165' 210 NULL 0 NULL NULL NULL",
        ],
        "a label line keeps its package unit beside its product's weight unit"
    );

    let tenders: Vec<(i64, String, i64)> = {
        let mut statement = conn
            .prepare("SELECT tender_no, method, change_minor FROM sale_tender WHERE sale_id = ?1 ORDER BY tender_no")
            .unwrap();
        statement
            .query_map([id.as_bytes().as_slice()], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(
        tenders,
        [
            (1, "card".to_owned(), 0),
            (2, "cash".to_owned(), sale.change().minor()),
        ],
        "only the tender that settled the sale gives change"
    );
}

/// What travels is what was written: the sale's member carries the completed
/// row in canonical form, and the stored payload is exactly it.
#[test]
fn every_fact_of_the_sale_travels_as_it_was_written() {
    let world = World::new();
    let sale = world.plain(0x20, "REG01-000001");
    let prepared = world.finalize(&sale, 0xE4, no_sabotage, false).unwrap();

    let header = prepared.members().first().unwrap();
    assert_eq!(header.entity(), "sale");
    assert!(
        header
            .payload()
            .starts_with(r#"{"basket_discount_amount_minor":null,"#)
    );
    assert!(header.payload().contains(r#""status":"completed""#));
    assert!(
        !header.payload().contains(' '),
        "canonical form has no whitespace"
    );
    let entities: Vec<&str> = prepared.members().iter().map(|m| m.entity()).collect();
    assert_eq!(
        entities,
        [
            "sale",
            "sale_supply_tax_context",
            "sale_line",
            "sale_line_tax",
            "sale_tax_summary",
            "sale_tender",
            "tender_status_event",
        ],
        "parents before children, as the server applies them"
    );
    for member in prepared.members() {
        let stored: String = world
            .conn
            .query_row(
                "SELECT payload FROM fact_commit_member WHERE entity = ?1 AND entity_id = ?2",
                params![member.entity(), member.entity_id().as_slice()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            stored,
            member.payload(),
            "{} travels as written",
            member.entity()
        );
    }
}

/// I-9: a sale is not written without an envelope naming each of its facts
/// exactly. A missing member and a member carrying another payload are each
/// refused by name, and nothing of the sale is left behind.
#[test]
fn a_sale_whose_envelope_does_not_name_it_exactly_is_not_written() {
    let world = World::new();
    let sale = world.plain(0x20, "REG01-000001");

    let missing = world
        .finalize(
            &sale,
            0xE5,
            |members| members.retain(|(entity, _, _)| entity != "sale_tax_summary"),
            false,
        )
        .unwrap_err();
    assert!(
        matches!(
            missing,
            DbError::SaleEnvelopeMissing {
                entity: "sale_tax_summary"
            }
        ),
        "{missing:?}"
    );

    let altered = world
        .finalize(
            &sale,
            0xE6,
            |members| {
                let line = members
                    .iter_mut()
                    .find(|(entity, _, _)| entity == "sale_line")
                    .unwrap();
                line.2 = line.2.replace(r#""qty_milli":1000"#, r#""qty_milli":2000"#);
            },
            false,
        )
        .unwrap_err();
    assert!(
        matches!(
            altered,
            DbError::SaleEnvelopeMismatch {
                entity: "sale_line"
            }
        ),
        "{altered:?}"
    );
    assert_eq!(world.sales().by_id(sale.cart().id).unwrap(), None);
    let rows: i64 = world
        .conn
        .query_row("SELECT count(*) FROM sale", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0, "the refused finalizes rolled back whole");
}

/// The completion is the last write, and `0005`'s gates are still in front of
/// it: a finalize that leaves out the original receipt completes nothing, and
/// the transaction it ran in leaves nothing behind.
#[test]
fn a_sale_missing_a_fact_its_completion_waits_on_does_not_complete() {
    let world = World::new();
    let sale = world.plain(0x20, "REG01-000001");
    let refused = world.finalize(&sale, 0xE7, no_sabotage, true).unwrap_err();
    assert!(
        matches!(&refused, DbError::Sqlite(rusqlite::Error::SqliteFailure(_, Some(m)))
                 if m.contains("original receipt job")),
        "{refused:?}"
    );
    assert_eq!(world.sales().by_id(sale.cart().id).unwrap(), None);
}

/// A sale the schema cannot hold is refused before anything is written,
/// naming why: a department line until #330 is decided, and a sale that would
/// not read back as it was given.
#[test]
fn a_sale_the_schema_cannot_hold_exactly_is_refused_before_it_is_written() {
    let world = World::new();
    let sale = world.everything();
    let tx = world.conn.unchecked_transaction().unwrap();
    let ids = SeqIdSource::new(1_788_000_000_000, 9);
    let prepare = |sale: &CompletedSale| {
        world
            .sales()
            .prepare(&tx, sale, PriceMode::Inclusive, &[0xE8; 16], &ids)
            .unwrap_err()
    };

    let mut department = sale.to_stored();
    let line = department.cart.lines.first_mut().unwrap();
    line.product_id = None;
    line.price_origin = PriceOrigin::Department {
        department_id: pos_domain::CategoryId::from_uuid(uuid(0xDE)),
        tax_category_id: world.category(),
        scanned_code: None,
    };
    let refused = prepare(&CompletedSale::restore(department).unwrap());
    assert!(
        matches!(&refused, DbError::SaleNotStorable { reason } if reason.contains("#330")),
        "{refused:?}"
    );

    // Every amount a sale stores is in its own currency, so a ceiling
    // captured in another would come back as the sale's: refused, not
    // silently converted by a change of label.
    let mut ceiling = sale.to_stored();
    ceiling.cart.lines.first_mut().unwrap().max_price =
        Some(Money::from_minor(1_500, Currency::USD));
    let refused = prepare(&CompletedSale::restore(ceiling).unwrap());
    assert!(
        matches!(&refused, DbError::SaleNotStorable { reason }
                 if reason == "line 1 would not read back as it was captured"),
        "{refused:?}"
    );
}

/// The price mode is the one fact of how a sale was priced that the sale does
/// not carry, so `prepare` is told it. A sale priced with tax added on top
/// reads back with each line's taxable amount its net, and the same sale
/// written as if its prices had included tax is refused before it is written,
/// because its lines would come back taxed on their gross.
#[test]
fn a_sale_priced_without_tax_reads_back_under_its_own_price_mode() {
    let world = World::exclusive();
    let sale = world.everything();
    let taxed = sale.priced().lines.first().unwrap();
    assert_eq!(
        taxed.taxable, taxed.tax.net,
        "the fixture is priced exclusive"
    );
    assert_ne!(taxed.tax.net, taxed.tax.gross);

    let tx = world.conn.unchecked_transaction().unwrap();
    let ids = SeqIdSource::new(1_788_000_000_000, 7);
    let refused = world
        .sales()
        .prepare(&tx, &sale, PriceMode::Inclusive, &[0xEC; 16], &ids)
        .unwrap_err();
    assert!(
        matches!(&refused, DbError::SaleNotStorable { reason }
                 if reason == "line 1's prices would not read back as they were paid"),
        "{refused:?}"
    );
    drop(tx);

    world.finalize(&sale, 0xED, no_sabotage, false).unwrap();
    let price_mode: String = world
        .conn
        .query_row(
            "SELECT price_mode FROM sale WHERE id = ?1",
            [sale.cart().id.as_uuid().as_bytes().as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(price_mode, "exclusive");
    assert_eq!(
        world.sales().by_id(sale.cart().id).unwrap().unwrap().sale(),
        &sale
    );
}

/// The reads: by receipt number on its own register only, and by business
/// day in each register's own numbering, never another store's or day's.
#[test]
fn a_sale_is_found_by_its_register_s_receipt_number_and_by_its_day() {
    let world = World::new();
    let second = world.plain(0x30, "REG01-000002");
    let first = world.plain(0x20, "REG01-000001");
    world.finalize(&second, 0xE9, no_sabotage, false).unwrap();
    world.finalize(&first, 0xEA, no_sabotage, false).unwrap();

    let sales = world.sales();
    let day_sales: Vec<SaleId> = sales
        .for_business_date(world.store(), day())
        .unwrap()
        .iter()
        .map(|r| r.sale().cart().id)
        .collect();
    assert_eq!(
        day_sales,
        [first.cart().id, second.cart().id],
        "in the register's numbering"
    );
    assert!(
        sales
            .for_business_date(world.store(), day().succ())
            .unwrap()
            .is_empty()
    );
    assert!(
        sales
            .for_business_date(StoreId::from_uuid(uuid(0x99)), day())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        sales
            .by_receipt_number(RegisterId::from_uuid(uuid(OTHER_REGISTER)), "REG01-000001")
            .unwrap(),
        None,
        "a receipt number is its register's"
    );
    assert_eq!(sales.by_id(SaleId::from_uuid(uuid(0x99))).unwrap(), None);
}

/// `.claude/rules/security.md`: the buyer's name travels in the sale's payload
/// and is held in its row, and reaches no `Debug` string on the way.
#[test]
fn a_prepared_or_stored_sale_never_prints_its_buyer() {
    let world = World::new();
    let sale = world.everything();
    let prepared = world.finalize(&sale, 0xEB, no_sabotage, false).unwrap();
    let record: SaleRecord = world.sales().by_id(sale.cart().id).unwrap().unwrap();

    for printed in [format!("{prepared:?}"), format!("{record:?}")] {
        assert!(!printed.contains(BUYER_NAME), "{printed}");
    }
    assert!(
        prepared
            .members()
            .first()
            .unwrap()
            .payload()
            .contains(BUYER_NAME),
        "the payload does carry it: the redaction is the Debug's, not an omission"
    );
}

/// I-4: `SaleRepository` has no method that updates or deletes a completed
/// sale, not a private one (`tests/ui/completed_sale_update.rs`).
#[test]
fn sale_repository_exposes_no_mutation() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/completed_sale_update.rs");
}
