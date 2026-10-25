//! The scan handler (microstep 1.2.4's shell half, `ref/domain-api.md` §4.3,
//! `ref/ipc-contract.md` "Catalog and search").
//!
//! Raw scanner bytes in; what the till does next out. The shell's part is the
//! reading and the looking-up. Every decision is the domain's — `parse_scan`
//! reads the code, `claiming_rule` names the layout that read a label,
//! `scale_pricing_allowed` judges the scale behind it, `resolve_scan` turns
//! the candidates into a `ScanLookup` — so nothing here can price a line.
//!
//! The order is the point:
//!
//! 1. **Parse with every layout the store has, switched on or not.** A label
//!    is a label whatever its layout's state. Parsed with the active layouts
//!    alone, a label from a switched-off layout would be looked up as an
//!    ordinary barcode, found by nothing, and offered to the cashier as an
//!    unknown code to quick-add — a product that already exists, at a price
//!    the cashier types.
//! 2. **A damaged label is the label's fault**, and is reported as such before
//!    anything is asked about its scale: a checksum failure is the domain's
//!    `ScanLookup::ChecksumFailed` (E.40), anything else unreadable is
//!    [`ScanOutcome::Unreadable`]. The cashier rescans.
//! 3. **A label then answers for its scale.** The layout that claimed it names
//!    the scale; the scale's current evidence must allow pricing now, on a
//!    trusted clock, or the scan is [`ScanOutcome::ScaleRefused`] with the
//!    domain's reason — a commissioning matter the operator must see as such.
//!    A scale whose evidence is current but whose layout is switched off is
//!    [`ScanOutcome::LayoutSwitchedOff`]: switching it back on is a person's
//!    act.
//! 4. **Only then is the item code looked up**, by barcode and by PLU, and every
//!    hit handed to `resolve_scan` with the opaque parse. The shell reads
//!    nothing of the parse but its item code (§4.3).
//!
//! Time, clock confidence, the store and its currency are arguments (I-8): the
//! caller has them, and a test chooses them.

use pos_db::DbError;
use pos_db::repo::product::ProductRepository;
use pos_db::repo::trade_scale::{Id, TradeScaleRepository};
use pos_domain::{
    ClockConfidence, Currency, EmbeddedBarcodeRule, MatchedOn, ProductHit, ScalePricingRefusal,
    ScanError, ScanLookup, Timestamp, claiming_rule, parse_scan_bytes, resolve_scan,
    scale_pricing_allowed,
};
use rusqlite::Connection;

/// What the till does with a scan.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanOutcome {
    /// The domain's answer: a product with its pack quantity, a weighed or a
    /// priced label, an ambiguous or an unknown code, or a failed checksum.
    /// Boxed, because it carries whole products and the other answers carry a
    /// prefix and a reason.
    Lookup(Box<ScanLookup>),
    /// A store layout reads this code as a scale label, and the scale's
    /// evidence does not let its labels price a line now.
    ScaleRefused {
        prefix: String,
        refusal: ScalePricingRefusal,
    },
    /// A store layout reads this code as a scale label and the scale's
    /// evidence is current, but the layout is switched off.
    LayoutSwitchedOff { prefix: String },
    /// The scan cannot be read: empty, not text, a label of the wrong length
    /// or with a stray character, or a layout that does not fit the code.
    Unreadable(ScanError),
}

/// Why the handler could not answer at all — never anything a cashier typed
/// or a label carried.
#[derive(Debug, thiserror::Error)]
pub enum ScanHandlerError {
    #[error(transparent)]
    Db(#[from] DbError),
    /// `resolve_scan` refused the candidates this handler found for the
    /// scan's own item code. A bug here, not a damaged label.
    #[error("the scan handler's lookup disagrees with the scan: {0}")]
    Lookup(ScanError),
}

/// Where and when a scan happens.
#[derive(Debug, Clone, Copy)]
pub struct ScanContext {
    /// The store whose layouts read labels.
    pub store: Id,
    /// The currency a label's price is in.
    pub currency: Currency,
    /// Business time now, from the shell's clock.
    pub now: Timestamp,
    /// How far the register trusts that clock. Anything short of trusted
    /// stops label pricing (`ref/schema.md` §0007).
    pub clock: ClockConfidence,
}

/// Read one scan.
///
/// # Errors
///
/// [`ScanHandlerError`] only when the database cannot answer, or when the
/// handler's own lookup disagrees with the scan. Every property of the scan
/// itself is a [`ScanOutcome`].
pub fn handle_scan(
    conn: &Connection,
    context: &ScanContext,
    raw: &[u8],
) -> Result<ScanOutcome, ScanHandlerError> {
    let scales = TradeScaleRepository::new(conn);
    let rules = scales.rules(&context.store)?;
    let layouts: Vec<EmbeddedBarcodeRule> = rules.iter().map(|rule| rule.rule.clone()).collect();

    let parsed = match parse_scan_bytes(raw, &layouts) {
        Ok(parsed) => parsed,
        Err(ScanError::ChecksumFailed) => {
            return Ok(ScanOutcome::Lookup(Box::new(ScanLookup::ChecksumFailed {
                code: String::from_utf8_lossy(raw).into_owned(),
            })));
        }
        Err(unreadable) => return Ok(ScanOutcome::Unreadable(unreadable)),
    };

    // The parse succeeded, so the scan is text.
    let code = String::from_utf8_lossy(raw);
    if let Some(rule) = claiming_rule(&code, &layouts).and_then(|index| rules.get(index)) {
        let prefix = rule.rule.prefix.clone();
        let current = scales.current_verification(&rule.trade_scale_id)?;
        if let Err(refusal) = scale_pricing_allowed(current.as_ref(), context.now, context.clock) {
            return Ok(ScanOutcome::ScaleRefused { prefix, refusal });
        }
        if !rule.is_active {
            return Ok(ScanOutcome::LayoutSwitchedOff { prefix });
        }
    }

    let products = ProductRepository::new(conn);
    let item_code = parsed.item_code().to_owned();
    let mut hits = products.by_barcode(&item_code)?;
    if let Some(product) = products.by_plu(&item_code)? {
        hits.push(ProductHit {
            product,
            matched_on: MatchedOn::Plu(item_code),
        });
    }
    resolve_scan(parsed, hits, context.currency)
        .map(|lookup| ScanOutcome::Lookup(Box::new(lookup)))
        .map_err(ScanHandlerError::Lookup)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use pos_db::fixtures::registered_chain::RegisteredChain;
    use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
    use pos_db::repo::trade_scale::{ENTITY, VerificationEvent};
    use pos_domain::{
        Barcode, BarcodeKind, Money, Product, ProductId, Qty, RegulatedSaleForm, SaleForm,
        TaxCategoryId, UnitOfMeasure, VerificationState,
    };
    use rusqlite::params;
    use uuid::Uuid;

    use super::*;

    const KEY: &str = "scan-handler-test-key";
    const SCALE: u128 = 0x40;
    const CHEESE_PLU: &str = "34567";
    const COLA_CAN: &str = "6291041500213";
    const COLA_SIX: &str = "6291041500220";

    struct Till {
        _dir: tempfile::TempDir,
        conn: Connection,
        chain: RegisteredChain,
    }

    fn id(value: u128) -> [u8; 16] {
        Uuid::from_u128(value).into_bytes()
    }

    fn at(iso: &str) -> Timestamp {
        Timestamp::parse_iso8601(iso).unwrap()
    }

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, Currency::JOD)
    }

    /// GS1's check digit, written out here rather than borrowed from the
    /// code under test: from the rightmost data digit leftwards, weights 3,
    /// 1, 3, 1…, and the digit that brings the sum to a multiple of ten.
    fn with_check(first_twelve: &str) -> String {
        assert_eq!(first_twelve.len(), 12);
        let sum: u32 = first_twelve
            .chars()
            .rev()
            .enumerate()
            .map(|(i, c)| c.to_digit(10).unwrap() * if i % 2 == 0 { 3 } else { 1 })
            .sum();
        format!("{first_twelve}{}", (10 - sum % 10) % 10)
    }

    /// `prefix`, five digits of item code, five digits of value, check digit.
    fn label(prefix: &str, item: &str, value: i64) -> String {
        with_check(&format!("{prefix}{item}{value:05}"))
    }

    fn product(till: &Till, n: u128, sku: &str, name_ar: &str) -> Product {
        Product {
            id: ProductId::from_uuid(Uuid::from_u128(n)),
            sku: sku.to_owned(),
            name_ar: name_ar.to_owned(),
            name_en: None,
            category_id: None,
            tax_category_id: TaxCategoryId::from_uuid(
                Uuid::from_slice(&till.chain.tax_category).unwrap(),
            ),
            unit: UnitOfMeasure::Each,
            unit_price: jod(450),
            is_weighed: false,
            is_service: false,
            is_active: true,
            min_age: None,
            max_price: None,
            reorder_point: None,
            regulated_sale_form: RegulatedSaleForm::UNREGULATED_SEALED_PACK,
        }
    }

    fn cheese(till: &Till) -> Product {
        Product {
            unit: UnitOfMeasure::Kilogram,
            is_weighed: true,
            unit_price: jod(8_500),
            regulated_sale_form: RegulatedSaleForm::Unregulated(SaleForm::Bulk),
            ..product(till, 2, "CHEESE-WHITE", "جبنة بيضاء")
        }
    }

    fn context(till: &Till) -> ScanContext {
        ScanContext {
            store: till.chain.store.as_slice().try_into().unwrap(),
            currency: Currency::JOD,
            now: at("2026-10-25T09:00:00.000Z"),
            clock: ClockConfidence::Trusted,
        }
    }

    fn scan(till: &Till, raw: &str) -> ScanOutcome {
        handle_scan(&till.conn, &context(till), raw.as_bytes()).unwrap()
    }

    /// The domain's answer, for a scan that must have one.
    fn lookup(outcome: ScanOutcome) -> ScanLookup {
        match outcome {
            ScanOutcome::Lookup(lookup) => *lookup,
            other => panic!("expected the domain's answer, found {other:?}"),
        }
    }

    /// Record one verification event the way a register will: its envelope
    /// first, then the fact, in one transaction (I-9).
    fn verify(till: &Till, n: u128, event_no: i64, state: VerificationState) {
        let evidence_ref = format!("JSMO-CERT-SYNTHETIC-{event_no}");
        let event = VerificationEvent {
            id: id(n),
            trade_scale_id: id(SCALE),
            event_no,
            state,
            evidence_hash: *blake3::hash(evidence_ref.as_bytes()).as_bytes(),
            evidence_ref,
            seal_or_mark_ref: None,
            effective_at: at("2026-01-01T00:00:00.000Z"),
            valid_until: Some(at("2027-01-01T00:00:00.000Z")),
        };
        let tx = till.conn.unchecked_transaction().unwrap();
        let commit = id(0x9000 + n);
        let change = id(0xA000 + n);
        let payload = event.payload();
        OutboxRepository::new(&tx)
            .write_commit(
                &tx,
                &CommitEnvelope {
                    commit_id: &commit,
                    protocol_version: 1,
                    schema_version: pos_db::SCHEMA_VERSION,
                    producer_version: "scan-handler-test",
                    created_at: "2026-10-25T08:00:00.000Z",
                },
                &[FactMember {
                    change_id: &change,
                    entity: ENTITY,
                    entity_id: &event.id,
                    payload: &payload,
                }],
            )
            .unwrap();
        TradeScaleRepository::new(&till.conn)
            .record_verification(&tx, &event)
            .unwrap();
        tx.commit().unwrap();
    }

    fn layout(till: &Till, n: u128, prefix: &str, kind: &str, active: bool) {
        till.conn
            .execute(
                "INSERT INTO embedded_barcode_rule
                   (id, store_id, trade_scale_id, prefix, item_code_start, item_code_len,
                    value_start, value_len, value_kind, value_scale, verify_checksum, is_active)
                 VALUES (?1, ?2, ?3, ?4, 2, 5, 7, 5, ?5, 1, 1, ?6)",
                params![
                    id(n).as_slice(),
                    till.chain.store,
                    id(SCALE).as_slice(),
                    prefix,
                    kind,
                    active
                ],
            )
            .unwrap();
    }

    /// A store with a cola sold by the can and by the six-pack, a white cheese
    /// sold by the kilogram under PLU 34567, and one verified deli scale whose
    /// layouts read price labels on "22" and weight labels on "23". Layout
    /// "24" exists and is switched off.
    fn till() -> Till {
        let dir = tempfile::tempdir().unwrap();
        let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
        let chain = RegisteredChain::seed(&conn);
        let till = Till {
            _dir: dir,
            conn,
            chain,
        };
        let tx = till.conn.unchecked_transaction().unwrap();
        let products = ProductRepository::new(&till.conn);
        products
            .upsert(&tx, &product(&till, 1, "COLA-330", "كولا"))
            .unwrap();
        products.upsert(&tx, &cheese(&till)).unwrap();
        for (row, code, units) in [(0x100, COLA_CAN, 1), (0x101, COLA_SIX, 6)] {
            products
                .add_barcode(
                    &tx,
                    &id(row),
                    &Barcode {
                        product_id: ProductId::from_uuid(Uuid::from_u128(1)),
                        code: code.to_owned(),
                        kind: BarcodeKind::Ean13,
                        is_primary: units == 1,
                        pack_qty: Qty::from_units(units).unwrap(),
                    },
                )
                .unwrap();
        }
        tx.commit().unwrap();
        till.conn
            .execute(
                "INSERT INTO plu_code (code, product_id) VALUES (?1, ?2)",
                params![CHEESE_PLU, Uuid::from_u128(2).as_bytes().as_slice()],
            )
            .unwrap();
        till.conn
            .execute(
                "INSERT INTO trade_scale (id, store_id, maker, model, serial_number)
                 VALUES (?1, ?2, 'Synthetic Scales', 'Deli-15', 'SN-0001')",
                params![id(SCALE).as_slice(), till.chain.store],
            )
            .unwrap();
        verify(&till, 1, 1, VerificationState::Verified);
        layout(&till, 0x50, "22", "price_minor", true);
        layout(&till, 0x51, "23", "weight_milli", true);
        layout(&till, 0x52, "24", "price_minor", false);
        till
    }

    #[test]
    fn a_plain_barcode_is_found_with_its_pack_quantity() {
        // E.78: the can and the six-pack outer are one product, and the code
        // that matched says how many units the scan means.
        let till = till();
        for (code, units) in [(COLA_CAN, 1), (COLA_SIX, 6)] {
            let ScanLookup::Product { hit, pack_qty } = lookup(scan(&till, code)) else {
                panic!("{code} must resolve to the cola");
            };
            assert_eq!(hit.product.sku, "COLA-330");
            assert_eq!(pack_qty, Qty::from_units(units).unwrap(), "{code}");
        }
    }

    #[test]
    fn a_price_label_from_a_verified_scale_prices_its_line() {
        // The label is what is charged: 0.416 JOD of white cheese, found by
        // the label's own item code as a PLU. The weight it implies at today's
        // 8.500 a kilogram, 49 g, rides along as an estimate for the stock
        // ledger, never as the line's quantity (E.80).
        let till = till();
        let code = label("22", CHEESE_PLU, 416);
        let ScanLookup::PriceEmbedded {
            hit,
            price,
            derived,
            ..
        } = lookup(scan(&till, &code))
        else {
            panic!("{code} must be a priced label");
        };
        assert_eq!(hit.product.sku, "CHEESE-WHITE");
        assert_eq!(hit.matched_on, MatchedOn::Plu(CHEESE_PLU.to_owned()));
        assert_eq!(price.amount(), jod(416));
        assert_eq!(derived.map(|d| d.qty), Some(Qty::from_milli(49)));
    }

    #[test]
    fn a_weight_label_carries_its_weight() {
        let till = till();
        let code = label("23", CHEESE_PLU, 1_250);
        assert_eq!(
            lookup(scan(&till, &code)),
            ScanLookup::WeightEmbedded {
                hit: ProductHit {
                    product: cheese(&till),
                    matched_on: MatchedOn::Plu(CHEESE_PLU.to_owned()),
                },
                qty: Qty::from_milli(1_250),
            }
        );
    }

    #[test]
    fn a_scale_in_maintenance_stops_its_labels_by_name() {
        // The scale goes for maintenance: 0007 switches its layouts off, and
        // a label it printed is refused for the scale's reason — not read as
        // an unknown barcode, and not priced.
        let till = till();
        verify(&till, 2, 2, VerificationState::MaintenancePending);
        assert_eq!(
            scan(&till, &label("22", CHEESE_PLU, 416)),
            ScanOutcome::ScaleRefused {
                prefix: "22".to_owned(),
                refusal: ScalePricingRefusal::MaintenancePending,
            }
        );
        // A plain barcode is untouched by the scale.
        assert!(matches!(
            lookup(scan(&till, COLA_CAN)),
            ScanLookup::Product { .. }
        ));
    }

    #[test]
    fn label_pricing_needs_a_trusted_clock_and_current_evidence() {
        let till = till();
        let code = label("22", CHEESE_PLU, 416);
        let untrusted = ScanContext {
            clock: ClockConfidence::Untrusted,
            ..context(&till)
        };
        assert_eq!(
            handle_scan(&till.conn, &untrusted, code.as_bytes()).unwrap(),
            ScanOutcome::ScaleRefused {
                prefix: "22".to_owned(),
                refusal: ScalePricingRefusal::ClockNotTrusted,
            }
        );
        let expired = ScanContext {
            now: at("2027-01-01T00:00:00.000Z"),
            ..context(&till)
        };
        assert_eq!(
            handle_scan(&till.conn, &expired, code.as_bytes()).unwrap(),
            ScanOutcome::ScaleRefused {
                prefix: "22".to_owned(),
                refusal: ScalePricingRefusal::Expired(at("2027-01-01T00:00:00.000Z")),
            }
        );
    }

    #[test]
    fn a_switched_off_layout_is_not_read_as_a_barcode() {
        // The scale is verified, but layout "24" is off. Its label is still a
        // label, and the till says why it will not price it — rather than
        // calling it unknown and offering a quick-add.
        let till = till();
        assert_eq!(
            scan(&till, &label("24", CHEESE_PLU, 416)),
            ScanOutcome::LayoutSwitchedOff {
                prefix: "24".to_owned()
            }
        );
    }

    #[test]
    fn a_damaged_label_is_a_checksum_failure() {
        // E.40: one digit off, and the label is rejected rather than guessed.
        let till = till();
        let good = label("22", CHEESE_PLU, 416);
        let damaged = format!("{}9{}", &good[..8], &good[9..]);
        assert_ne!(damaged, good);
        assert_eq!(
            lookup(scan(&till, &damaged)),
            ScanLookup::ChecksumFailed { code: damaged }
        );
    }

    #[test]
    fn an_unknown_code_is_unknown_and_the_queue_moves_on() {
        // E.39: a barcode nothing claims, and a verified label for a PLU no
        // product has, both come back as unknown codes — never an error.
        let till = till();
        assert_eq!(
            lookup(scan(&till, "6290000000006")),
            ScanLookup::Unknown {
                code: "6290000000006".to_owned()
            }
        );
        let stranger = label("22", "99999", 416);
        assert_eq!(
            lookup(scan(&till, &stranger)),
            ScanLookup::Unknown { code: stranger }
        );
    }

    #[test]
    fn an_unreadable_scan_is_named() {
        let till = till();
        assert_eq!(scan(&till, ""), ScanOutcome::Unreadable(ScanError::Empty));
        assert_eq!(
            handle_scan(&till.conn, &context(&till), &[0xFF, 0xFE]).unwrap(),
            ScanOutcome::Unreadable(ScanError::NotUtf8)
        );
        // A "22" code too short to be a label is refused as a label, not
        // looked up as a barcode.
        assert_eq!(
            scan(&till, "2234567"),
            ScanOutcome::Unreadable(ScanError::BadLength(7, BarcodeKind::PriceEmbedded))
        );
    }
}
