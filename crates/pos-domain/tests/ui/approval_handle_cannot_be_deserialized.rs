// Persisted rows cross through `StoredApprovalHandle` and the validating
// `ApprovalHandle::restore` seam. Direct deserialization would bypass that
// validation, so this file must fail to compile.

use pos_domain::ApprovalHandle;

fn main() {
    let _: ApprovalHandle = serde_json::from_str("{}").unwrap();
}
