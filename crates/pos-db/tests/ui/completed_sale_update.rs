// A completed sale is never updated and never deleted (I-4). `SaleRepository`
// has no method for either, not a private one, and not the transition its own
// insert makes. A correction is a new document. This file must fail to compile.

use pos_db::repo::sale::SaleRepository;
use pos_domain::SaleId;
use rusqlite::{Connection, Transaction};

fn rewrite_a_completed_sale(conn: &Connection, tx: &Transaction<'_>, id: SaleId) {
    let sales = SaleRepository::new(conn);
    let _ = sales.update(tx, id);
    let _ = sales.delete(tx, id);
    let _ = sales.void(tx, id);
    let _ = sales.complete(tx, id);
}

fn main() {
    let _ = rewrite_a_completed_sale;
}
