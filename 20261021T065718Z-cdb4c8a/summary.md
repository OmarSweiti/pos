**Runtime change: yes, for one binary.** `Cargo.lock` moves `rustls-webpki` 0.103.14 → 0.103.16 and `webpki-roots` 1.0.9 → 1.0.10. Both reach `pos-server` only, through `sqlx`'s Postgres TLS; the register does not link them. The evidence is therefore a smoke run of `pos-server` itself.

**Setup**
- **Code:** PR head `cdb4c8a`, taken with `git archive`, so nothing outside the commit (no local `.env`) entered the build.
- **Build:** a throwaway `rust:1.97.1-slim-bookworm` container, the repository's pinned toolchain, running `cargo build --locked --release -p pos-server`.
- **Database:** a throwaway `postgres:16-alpine` on a private Docker network, with a synthetic password. Both containers were removed afterwards.
- **Why inside a container:** `pos-server` binds a hard-coded `127.0.0.1:8080`, which on this machine belongs to an unrelated service. Inside the container's own network namespace the port is free, so the server runs unmodified.

| # | Step | Expected | Observed |
|---|---|---|---|
| 1 | Diff `Cargo.lock` against `development` | exactly the two advisory packages | ✅ two versions and two checksums, nothing else (01 §1) |
| 2 | Build `pos-server` from the commit | it builds, `--locked` | ✅ `Finished release … in 3m 03s` (01 §2) |
| 3 | `cargo tree -i` for each crate | the fixed versions | ✅ `rustls-webpki v0.103.16` under `rustls`, and `webpki-roots v1.0.10` under `sqlx-core` (01 §3) |
| 4 | Start it beside Postgres, then `GET /health` and `GET /health/db` | both 200 | ✅ `{"status":"ok"}` and `{"db":"ok"}` (01 §4) |
| 5 | `just audit` | clean | ✅ `advisories ok, bans ok, licenses ok, sources ok`, and pnpm finds no known vulnerabilities (01 §6) |

**Not validated here, and why**
- **Certificate-path validation against public roots,** the behaviour the advisories concern. The throwaway Postgres has no publicly chained certificate, so its connection runs without TLS. What this run proves is that the patched crates build, link and serve. The advisory database confirms that the vulnerable versions are gone.
- **The register app:** it links neither crate.
