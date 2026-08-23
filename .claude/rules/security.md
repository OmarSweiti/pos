# Security — the never-list

Full treatment: `docs/implementation/ref/security-compliance.md`.
One-page version: `docs/implementation/01-conventions.md` §12.

- **Never log** a value under any canonical sensitive field name: `pin`, `pin_hash`, `pan`,
  `card_number`, `cvv`, `track`, `phone`, `email`, `customer_name`, `buyer_name`, `secret_key`,
  `client_id`, `db_key`, `token`, `password`, or `entitlement`. JoFotara/fiscal secrets and
  signing material are covered even when a provider gives them another name. This applies at
  every nesting depth and includes `tracing`, `IpcError.detail`, Sentry/crash reports, and test
  fixtures that print. The canonical list lives in
  `docs/implementation/ref/security-compliance.md` §5; update this rule with that source.
- **Never store** anything from a card except the PSP reference, the masked PAN the terminal
  returns for the receipt, and the scheme.
- **The DB key lives in the OS credential store.** Never a file, never an env var in a release
  build. `POS_DB_KEY` is dev/CI only and the release build must refuse to honour it.
- **Permissions are checked in Rust, in the command handler.** Hiding a button is UX. The check
  is the security.
- **Never claim a compliance validation that has not been completed** — not "PCI compliant",
  not "SAQ done", not "JoFotara certified" — in code, comments, docs, UI copy, or a commit
  message. Read `docs/implementation/ref/security-compliance.md` §3 first; the answer is
  probably "not yet".
- **Never commit a secret.** If one is already in the tree, say so and stop. Do not rewrite
  history unasked.
