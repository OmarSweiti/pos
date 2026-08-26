// The proof of microstep 1.1.8, and the only kind of proof available for it:
// `SaleId` and `SaleLineId` are both a `Uuid` underneath, and this file must
// still FAIL to compile. `tests/typed_ids_ui.rs` runs it through `trybuild`, and
// `typed_ids_do_not_interconvert.stderr` beside it is the golden of rustc's exact
// wording.
//
// KEEP THIS FIXTURE MINIMAL. Every diagnostic this file produces lands in that
// golden, so one unrelated warning here becomes an unrelated golden diff for
// whoever touches the file next.
//
// THE GOLDEN IS RUSTC-VERSION-SENSITIVE. A compiler release may reword E0308 or
// move a label. `rust-toolchain.toml` pins the compiler, so the golden is stable
// today; whoever bumps that pin regenerates it in the same change with
//
//     TRYBUILD=overwrite cargo test -p pos-domain --test typed_ids_ui
//
// and reads the diff rather than trusting it.

use pos_domain::{SaleId, SaleLineId, SeqIdSource};

fn refund(_sale: SaleId, _line: SaleLineId) {}

fn main() {
    let ids = SeqIdSource::new(1_767_225_600_000, 0);
    let sale = SaleId::next_from(&ids);
    let line = SaleLineId::next_from(&ids);

    refund(line, sale);
}
