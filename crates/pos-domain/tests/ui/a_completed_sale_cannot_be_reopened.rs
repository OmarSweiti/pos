// A completed sale is immutable (I-4): no transition takes one, and nothing it
// holds can be reached to change. This file must fail to compile.

use pos_domain::cart::{CompletedSale, add_tender, back_to_building};
use pos_domain::tender::Tender;

fn take_another_tender(sale: CompletedSale, tender: Tender) {
    let _ = add_tender(sale, tender);
}

fn build_it_again(sale: CompletedSale) {
    let _ = back_to_building(sale);
}

fn rewrite_its_receipt(sale: &mut CompletedSale) {
    sale.receipt_number = String::new();
}

fn clear_its_lines(sale: &CompletedSale) {
    sale.cart().lines.clear();
}

fn main() {
    let _ = (take_another_tender, build_it_again, rewrite_its_receipt, clear_its_lines);
}
