// A price-embedded lookup is the one value that carries a label's price into a
// cart line, so code outside `pos-domain` must not be able to make one. Each
// line below is a way a dependent crate could try, and each must fail to
// compile: the variant is `#[non_exhaustive]`, the label constructor is
// private, and neither type deserializes. A forged parse is its own fixture,
// because rustc reports privacy only after these type errors stop it.

use pos_domain::{Currency, ParsedScan, PriceSource, ProductHit, ScanLookup};

fn forge_the_variant(hit: ProductHit, price: PriceSource) -> ScanLookup {
    ScanLookup::PriceEmbedded { hit, price, derived: None }
}

fn call_the_label_constructor(scan: &ParsedScan) -> PriceSource {
    PriceSource::from_label(scan, Currency::JOD).unwrap()
}

fn deserialize_a_price() -> PriceSource {
    serde_json::from_str(r#"{"amount":{"minor":1,"currency":"JOD"},"origin":"x"}"#).unwrap()
}

fn deserialize_a_lookup() -> ScanLookup {
    serde_json::from_str("{}").unwrap()
}

fn main() {}
