// Both tokens may have been obtained legitimately; their marker types still
// make an approval for a manual discount unusable where a sale void is required.

use pos_domain::{Authorized, cap};

fn void_sale(_authorization: &Authorized<cap::SaleVoid>) {}

fn substitute(authorization: &Authorized<cap::DiscountManual>) {
    void_sale(authorization);
}

fn main() {
    let _proof: fn(&Authorized<cap::DiscountManual>) = substitute;
}
