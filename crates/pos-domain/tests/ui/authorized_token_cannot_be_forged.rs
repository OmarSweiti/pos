// `Authorized<C>` is evidence only while callers cannot write the evidence
// themselves. This fixture supplies already-correct field types so its failure
// is field privacy, not an unrelated constructor or import mistake.

use std::marker::PhantomData;

use pos_domain::{ApprovalId, Authorized, Timestamp, UserId, cap};

fn forge(
    actor: UserId,
    approver: Option<UserId>,
    approval: Option<ApprovalId>,
    at: Timestamp,
) -> Authorized<cap::SaleVoid> {
    Authorized {
        actor,
        approver,
        approval,
        at,
        _capability: PhantomData,
    }
}

fn main() {
    let _proof: fn(
        UserId,
        Option<UserId>,
        Option<ApprovalId>,
        Timestamp,
    ) -> Authorized<cap::SaleVoid> = forge;
}
