// A line's price is a `PriceSource`, which has no `Deserialize`, so neither
// has `AddLine`: a command arriving from the screen cannot carry a price into a
// cart line. This file must fail to compile.

use pos_domain::cart::AddLine;

fn main() {
    let _: AddLine = serde_json::from_str("{}").unwrap();
}
