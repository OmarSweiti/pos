//! The synthetic receipt the crate's own tests render
//! (`render::tests_support::model`), built through the public API, shared by
//! the integration tests that print it: an integration test cannot see the
//! crate's own fixture.

use pos_domain::catalog::UnitOfMeasure;
use pos_domain::ids::{RegisterId, SaleId};
use pos_domain::money::{Currency, Money, Percent, Qty};
use pos_domain::receipt::{
    BuyerBlock, DocKind, FiscalBlock, FooterBlock, Language, LoyaltyBlock, MerchantBlock,
    ReceiptDiscount, ReceiptHeader, ReceiptLine, ReceiptLocale, ReceiptModel, ReceiptTender,
    ReceiptTotals,
};
use pos_domain::tax::{TaxSummaryRow, TaxTreatment};
use pos_domain::time::{BusinessDate, Timestamp};
use uuid::Uuid;

fn jod(minor: i64) -> Money {
    Money::from_minor(minor, Currency::JOD)
}

fn line(name: &str, qty_milli: i64, unit_minor: i64) -> ReceiptLine {
    ReceiptLine {
        name: name.to_owned(),
        qty: Qty::from_milli(qty_milli),
        unit: UnitOfMeasure::Each,
        is_weighed: false,
        unit_price: jod(unit_minor),
        line_total: jod(unit_minor * qty_milli / 1000),
        discounts: Vec::new(),
    }
}

/// The same document the crate's own tests render (`render::tests_support::model`).
pub fn receipt() -> ReceiptModel {
    let mut first = line("خبز عربي طازج", 2_000, 400);
    first.discounts.push(ReceiptDiscount {
        label: "عرض الخبز".to_owned(),
        amount: jod(50),
    });
    ReceiptModel {
        doc_kind: DocKind::Sale,
        watermark: None,
        merchant: MerchantBlock {
            legal_name: "مؤسسة النور التجارية".to_owned(),
            store_name: "النور".to_owned(),
            address: Some("عمان، الأردن".to_owned()),
            phone: Some("+962 79 000 0000".to_owned()),
            tin: Some("123456789".to_owned()),
        },
        buyer: Some(BuyerBlock {
            name: Some("شركة الأمانة للتجارة".to_owned()),
            tin: "987654321".to_owned(),
        }),
        header: ReceiptHeader {
            sale_id: SaleId::from_uuid(Uuid::from_u128(1)),
            receipt_no: "REG01-000042".to_owned(),
            register_id: RegisterId::from_uuid(Uuid::from_u128(2)),
            register_code: "REG01".to_owned(),
            cashier_name: "ليلى".to_owned(),
            issued_at: Timestamp::from_epoch_milliseconds(1_758_000_000_000).expect("instant"),
            business_date: BusinessDate::new(2026, 9, 22).expect("date"),
        },
        lines: vec![first],
        totals: ReceiptTotals {
            subtotal: jod(2_166),
            discount_total: jod(50),
            tax_summary: vec![TaxSummaryRow {
                code: "GST16".to_owned(),
                treatment: TaxTreatment::Standard,
                rate: Percent::from_ppm(160_000),
                per_unit: None,
                reason: None,
                net: jod(1_867),
                tax: jod(299),
                gross: jod(2_166),
            }],
            rounding_adjustment: Some(jod(-3)),
            total: jod(2_163),
        },
        tenders: vec![ReceiptTender {
            label: "نقدًا".to_owned(),
            amount: jod(2_163),
            masked_pan: None,
            scheme: None,
        }],
        change: Some(jod(0)),
        loyalty: Some(LoyaltyBlock {
            balance_points: 120,
        }),
        fiscal: Some(FiscalBlock {
            uuid: "9f1c0f7a-0000-4000-8000-000000000000".to_owned(),
            qr_payload: "AQIDBAU=".to_owned(),
        }),
        footer: FooterBlock {
            return_policy: Some("الإرجاع خلال أربعة عشر يومًا".to_owned()),
            thank_you: Some("شكرًا لتسوقكم".to_owned()),
        },
        locale: ReceiptLocale::new(Language::Arabic),
    }
}
