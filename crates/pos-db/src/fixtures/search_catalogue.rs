//! The catalogue search is benchmarked over: 50 000 products with Arabic names
//! (microstep 1.2.7, gap G-9).
//!
//! `benches/search.rs` measures [`crate::repo::product::ProductRepository::search`]
//! over it, and `tests/search_benchmark.rs` holds it to what the benchmark
//! claims to measure. Both read this one module, so the catalogue a test
//! counts and the queries it checks are the ones the benchmark times — a
//! second copy in the bench target would be a fixture no test had ever seen.
//!
//! **Every product goes in through [`ProductRepository::upsert`].** The index a
//! search reads is built by `0007`'s triggers from the projections `0003`
//! generates, so a catalogue written any other way would be benchmarking an
//! index the register never builds.
//!
//! **The names are built, not drawn.** Every combination of 40 items, 25
//! brands, 10 qualifiers and 5 sizes is one product — 40 · 25 · 10 · 5 =
//! 50 000 — so every name is distinct, every run builds the same catalogue,
//! and nothing here needs randomness. Every tenth product's item is written
//! with a fatha, so `name_ar_exact` has tashkeel to strip, and the vocabulary
//! carries hamza, alef maqsura and taa marbuta, so `name_ar_fold` has
//! spellings to collapse.
//!
//! **The first keystroke is the worst one, on purpose.** Every name carries a
//! brand that begins with ال, so the one-letter query `ا` matches every product
//! in the catalogue, and no letter can match more. One letter is shorter than
//! any prefix `0007`'s `prefix='2 3'` index holds, and `search` ranks every
//! match before it keeps a page, so this is the costliest keystroke a cashier
//! makes and the likeliest place to lose a 50 ms budget. A benchmark that
//! typed an uncommon letter first would report a till faster than the one a
//! cashier uses.
use pos_domain::{
    Currency, Money, Product, ProductId, RegulatedSaleForm, TaxCategoryId, UnitOfMeasure,
};
use rusqlite::Connection;
use uuid::Uuid;

use crate::fixtures::registered_chain::RegisteredChain;
use crate::repo::product::ProductRepository;

/// How many products the catalogue holds: one per combination, which is §7's
/// "50k SKUs" — `search_benchmark_fixture_contains_50000_products` holds it to
/// the literal, so a shorter vocabulary cannot shrink the benchmark quietly.
pub const PRODUCTS: usize = ITEMS.len() * BRANDS.len() * QUALIFIERS.len() * SIZES.len();

/// What a cashier types, one keystroke at a time: 1, 2, 3 and 5 characters of
/// the brand الوادي, the lengths 1.2.7 benchmarks.
pub const QUERIES: [&str; 4] = ["ا", "ال", "الو", "الواد"];

/// The page a search asks for. `catalog_search` takes its `limit` from the
/// caller (`ref/ipc-contract.md`), and 20 is the page the repository's own
/// tests ask for. `search` ranks every match before it applies the limit.
pub const RESULT_LIMIT: u32 = 20;

/// The fixture's product ids start with this byte, clear of every id
/// [`RegisteredChain`] and the `pos-db` tests mint.
const ID_PREFIX: u128 = 0xB7 << 120;

const ITEMS: [(&str, &str); 40] = [
    ("حليب", "Milk"),
    ("لبن", "Yoghurt"),
    ("لبنة", "Labneh"),
    ("جبنة", "Cheese"),
    ("زبدة", "Butter"),
    ("خبز", "Bread"),
    ("أرز", "Rice"),
    ("سكر", "Sugar"),
    ("شاي", "Tea"),
    ("قهوة", "Coffee"),
    ("زيت", "Oil"),
    ("عصير", "Juice"),
    ("ماء", "Water"),
    ("شوكولاتة", "Chocolate"),
    ("بسكويت", "Biscuits"),
    ("معكرونة", "Pasta"),
    ("طحين", "Flour"),
    ("عدس", "Lentils"),
    ("حمص", "Chickpeas"),
    ("فول", "Fava beans"),
    ("تمر", "Dates"),
    ("عسل", "Honey"),
    ("مربى", "Jam"),
    ("صابون", "Soap"),
    ("شامبو", "Shampoo"),
    ("منظف", "Detergent"),
    ("مناديل", "Tissues"),
    ("بيض", "Eggs"),
    ("دجاج", "Chicken"),
    ("تونة", "Tuna"),
    ("زيتون", "Olives"),
    ("ملح", "Salt"),
    ("فلفل", "Pepper"),
    ("كاتشب", "Ketchup"),
    ("مايونيز", "Mayonnaise"),
    ("شيبس", "Crisps"),
    ("بوظة", "Ice cream"),
    ("طحينة", "Tahini"),
    ("حلاوة", "Halva"),
    ("كعك", "Ka'ak"),
];

const BRANDS: [&str; 25] = [
    "الوادي",
    "النخيل",
    "السنابل",
    "الريف",
    "الواحة",
    "البادية",
    "الجبل",
    "الهلال",
    "النجمة",
    "الأصيل",
    "السهل",
    "الخير",
    "البركة",
    "الكرمة",
    "الصفا",
    "المروج",
    "الياسمين",
    "الغدير",
    "القمة",
    "الندى",
    "الفجر",
    "الضياء",
    "السلام",
    "الزيتونة",
    "الأمل",
];

const QUALIFIERS: [&str; 10] = [
    "طازج",
    "كامل الدسم",
    "قليل الدسم",
    "بالفراولة",
    "بالشوكولاتة",
    "عضوي",
    "بلدي",
    "مستورد",
    "بالليمون",
    "أصلي",
];

const SIZES: [(&str, &str); 5] = [
    ("250 غم", "250 g"),
    ("500 غم", "500 g"),
    ("1 كغم", "1 kg"),
    ("1 لتر", "1 L"),
    ("6 حبات", "6 pcs"),
];

/// Seed the registered chain and the 50 000 products into a database
/// [`crate::open`] just built, in one transaction.
///
/// # Panics
///
/// On any refusal: this is a fixture, and a catalogue that half-loaded would
/// be benchmarked as if it were whole.
pub fn seed(conn: &Connection) {
    let chain = RegisteredChain::seed(conn);
    let tax_category = TaxCategoryId::from_uuid(Uuid::from_slice(&chain.tax_category).unwrap());
    let repository = ProductRepository::new(conn);
    let tx = conn.unchecked_transaction().unwrap();
    // Iterated rather than indexed: `clippy::indexing_slicing` is denied
    // workspace-wide. The item turns fastest, so product `n` is item `n % 40`.
    let names = SIZES.iter().flat_map(|size| {
        QUALIFIERS.iter().flat_map(move |qualifier| {
            BRANDS.iter().flat_map(move |brand| {
                ITEMS.iter().map(move |item| Name {
                    item: *item,
                    brand,
                    qualifier,
                    size: *size,
                })
            })
        })
    });
    for (n, name) in names.enumerate() {
        repository
            .upsert(&tx, &product(n, &name, tax_category))
            .unwrap_or_else(|error| panic!("product {n} was refused: {error}"));
    }
    tx.commit().unwrap();
}

/// One combination: an item and a size with their English, a brand and a
/// qualifier.
struct Name {
    item: (&'static str, &'static str),
    brand: &'static str,
    qualifier: &'static str,
    size: (&'static str, &'static str),
}

/// Product `n`, named for one combination.
fn product(n: usize, name: &Name, tax_category: TaxCategoryId) -> Product {
    let (item, item_en) = name.item;
    let (size, size_en) = name.size;
    let item = if n.is_multiple_of(10) {
        with_fatha(item)
    } else {
        item.to_owned()
    };
    let serial = u32::try_from(n).unwrap();
    Product {
        id: ProductId::from_uuid(Uuid::from_u128(ID_PREFIX | u128::from(serial))),
        sku: format!("SB-{n:05}"),
        name_ar: format!("{item} {} {} {size}", name.brand, name.qualifier),
        name_en: n.is_multiple_of(2).then(|| format!("{item_en} {size_en}")),
        category_id: None,
        tax_category_id: tax_category,
        unit: UnitOfMeasure::Each,
        unit_price: Money::from_minor(250 + i64::from(serial % 400) * 25, Currency::JOD),
        is_weighed: false,
        is_service: false,
        is_active: true,
        min_age: None,
        max_price: None,
        reorder_point: None,
        regulated_sale_form: RegulatedSaleForm::UNREGULATED_SEALED_PACK,
    }
}

/// The word with a fatha after its first letter, as a name typed from a
/// supplier's vocalised list arrives.
fn with_fatha(word: &str) -> String {
    let mut letters = word.chars();
    let first = letters.next().unwrap();
    format!("{first}\u{064E}{}", letters.as_str())
}
