// The parse is the evidence a label price is derived from, so code outside
// `pos-domain` must not be able to make one up: its fields are private.

use pos_domain::ParsedScan;

fn main() {
    let _ = ParsedScan {
        item_code: String::from("12345"),
        raw_code: String::from("2123457004163"),
    };
}
