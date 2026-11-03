//! Microstep 1.8.2 — `SaleRepository`, against the exact registered chain.
//!
//! The register, the sales and the finalize transaction come from
//! `sale_world`, which `parked_cart.rs` shares.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod sale_world;

use pos_db::DbError;
use pos_db::repo::sale::SaleRecord;
use pos_domain::{
    ApprovalId, CompletedSale, Currency, Money, PriceMode, PriceOrigin, RegisterId, SaleId,
    SeqIdSource, StoreId, TenderId, TenderState,
};
use rusqlite::params;
use sale_world::{BUYER_NAME, BUYER_TIN, OTHER_REGISTER, REGISTER, World, day, no_sabotage, uuid};
use uuid::Uuid;

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
