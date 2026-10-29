// A parked cart is its own type, so each state takes only its own
// transitions: a cart being built cannot be resumed, and a parked cart cannot
// be changed until it is. This file must fail to compile.

use pos_domain::cart::{CartContext, ParkedCart, open, resume, set_qty};
use pos_domain::ids::{SaleId, SaleLineId};
use pos_domain::money::Qty;

fn resume_a_cart_being_built(id: SaleId, till: &CartContext) {
    let _ = resume(open(id, till), till);
}

fn change_a_parked_cart(parked: ParkedCart, line: SaleLineId) {
    let _ = set_qty(parked, line, Qty::ONE);
}

fn main() {
    let _ = (resume_a_cart_being_built, change_a_parked_cart);
}
