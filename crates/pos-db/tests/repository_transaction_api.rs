//! Microstep 1.8.1 — every fact write takes the caller's transaction.
//!
//! I-9 commits a sale's facts, its `sync_commit`, its whole
//! `fact_commit_member` manifest and its `sync_outbox` rows in one `BEGIN`
//! and one `COMMIT`. That holds only if no write decides for itself where a
//! transaction begins or ends, so every write in this crate takes a
//! `&Transaction` its caller opened, and the type system refuses one handed
//! only a `&Connection`. These tests hold the crate to that: one reads its own
//! source, one rolls a write back, and one is a fixture that must not compile.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::fixtures::registered_chain;

/// The module name is load-bearing: nextest matches a filter against the
/// test's own name, so `repository_transaction_api::` selects these.
mod repository_transaction_api {
    use std::path::{Path, PathBuf};

    use pos_db::repo::clock::{ClockRepository, StoredClock};
    use pos_domain::{ClockState, Timestamp};

    use crate::registered_chain::RegisteredChain;

    /// The dev-only seeding fixtures write a reference world straight onto a
    /// connection, because they stand in for a whole application's worth of
    /// commands. They compile only under the `fixtures` feature, which no
    /// build that ships enables (`fixtures_feature.rs` holds that).
    const DEV_ONLY: &str = "fixtures";

    /// The functions allowed to begin and end transactions of their own: the
    /// migration runner, which applies each file in a transaction of its own
    /// and records the version with it, and its replay (1.8.1b), which applies
    /// the same files the same way to an in-memory database it owns, never a
    /// register's, to learn the schema each version creates.
    const OWNS_ITS_TRANSACTIONS: [(&str, &str); 2] = [("lib.rs", "migrate"), ("lib.rs", "replay")];

    /// One function of the crate's source: where it is, what it takes, and
    /// what its body says once comments are taken out.
    struct Function {
        file: String,
        line: usize,
        name: String,
        params: String,
        body: String,
    }

    /// The byte at `i`, or nothing past the end. Read through this rather than
    /// indexed, as the workspace asks of tests too.
    fn at(b: &[u8], i: usize) -> u8 {
        b.get(i).copied().unwrap_or(0)
    }

    /// `text[from..to]`, or nothing where that is not a slice of it.
    fn slice(text: &str, from: usize, to: usize) -> &str {
        text.get(from..to).unwrap_or_default()
    }

    /// Where the literal that starts at `i` ends, one past its last byte, or
    /// `None` when no literal starts there. A string, a byte string, a raw
    /// string with any number of `#`, a character or a byte; a lifetime is no
    /// literal. The SQL a function runs lives in these, and a quote or a
    /// bracket inside one is not code.
    fn literal_end(b: &[u8], i: usize) -> Option<usize> {
        let before = i.checked_sub(1).map_or(0, |p| at(b, p));
        let after_ident = before.is_ascii_alphanumeric() || before == b'_';
        match at(b, i) {
            b'b' if !after_ident && matches!(at(b, i + 1), b'"' | b'\'' | b'r') => {
                literal_end(b, i + 1)
            }
            b'r' if !after_ident || before == b'b' => {
                let mut j = i + 1;
                while at(b, j) == b'#' {
                    j += 1;
                }
                if at(b, j) != b'"' {
                    return None;
                }
                let hashes = j - i - 1;
                let closes = |k: usize| (1..=hashes).all(|h| at(b, k + h) == b'#');
                let close = (j + 1..b.len()).find(|k| at(b, *k) == b'"' && closes(*k))?;
                Some(close + 1 + hashes)
            }
            b'"' => {
                let mut k = i + 1;
                while k < b.len() && at(b, k) != b'"' {
                    if at(b, k) == b'\\' {
                        k += 1;
                    }
                    k += 1;
                }
                Some((k + 1).min(b.len()))
            }
            b'\'' => {
                if at(b, i + 1) == b'\\' {
                    let close = (i + 2..b.len()).find(|k| at(b, *k) == b'\'')?;
                    return Some(close + 1);
                }
                let rest = b.get(i + 1..).unwrap_or_default();
                let width = rest
                    .utf8_chunks()
                    .next()
                    .and_then(|chunk| chunk.valid().chars().next())
                    .map_or(1, char::len_utf8);
                (at(b, i + 1 + width) == b'\'').then_some(i + 2 + width)
            }
            _ => None,
        }
    }

    /// `text` without its comments, every literal kept whole, since the SQL a
    /// statement runs lives in a string.
    fn without_comments(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < bytes.len() {
            if at(bytes, i) == b'/' && at(bytes, i + 1) == b'/' {
                while i < bytes.len() && at(bytes, i) != b'\n' {
                    i += 1;
                }
            } else if at(bytes, i) == b'/' && at(bytes, i + 1) == b'*' {
                i += 2;
                while i + 1 < bytes.len() && !(at(bytes, i) == b'*' && at(bytes, i + 1) == b'/') {
                    if at(bytes, i) == b'\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
                i += 2;
            } else if let Some(end) = literal_end(bytes, i) {
                out.push_str(slice(text, i, end));
                i = end;
            } else {
                let width = slice(text, i, text.len())
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8);
                out.push_str(slice(text, i, i + width));
                i += width;
            }
        }
        out
    }

    /// Where the bracket opened at `open` closes, counting only brackets
    /// outside literals.
    fn closing(text: &[u8], open: usize, left: u8, right: u8) -> usize {
        let (mut depth, mut i) = (0usize, open);
        while i < text.len() {
            if let Some(end) = literal_end(text, i) {
                i = end;
                continue;
            }
            if at(text, i) == left {
                depth += 1;
            } else if at(text, i) == right {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            i += 1;
        }
        panic!("unbalanced brackets");
    }

    /// Every function with a body in `file`, before its test module.
    fn functions(root: &Path, file: &Path) -> Vec<Function> {
        let source = std::fs::read_to_string(file).unwrap();
        let shipped = source
            .split("\n#[cfg(test)]\nmod ")
            .next()
            .unwrap_or_default();
        let text = without_comments(shipped);
        let bytes = text.as_bytes();
        let mut found = Vec::new();
        let mut from = 0;
        while let Some(start) = slice(&text, from, text.len()).find("fn ").map(|i| i + from) {
            from = start + 3;
            let before = start.checked_sub(1).map_or(b' ', |p| at(bytes, p));
            let name: String = slice(&text, start + 3, text.len())
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if before.is_ascii_alphanumeric() || before == b'_' || name.is_empty() {
                continue;
            }
            let Some(paren) = slice(&text, start, text.len()).find('(').map(|i| i + start) else {
                continue;
            };
            let params_end = closing(bytes, paren, b'(', b')');
            let after = slice(&text, params_end, text.len());
            let (brace, semi) = (
                after.find('{').map(|i| i + params_end),
                after.find(';').map(|i| i + params_end),
            );
            let Some(brace) = brace.filter(|b| semi.is_none_or(|s| *b < s)) else {
                continue;
            };
            let end = closing(bytes, brace, b'{', b'}');
            found.push(Function {
                file: file.strip_prefix(root).unwrap().display().to_string(),
                line: slice(&text, 0, start).matches('\n').count() + 1,
                name,
                params: slice(&text, paren, params_end + 1).to_owned(),
                body: slice(&text, brace, end + 1).to_owned(),
            });
        }
        found
    }

    /// Every `.rs` file under `dir`.
    fn sources(dir: &Path, into: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources(&path, into);
            } else if path.extension().is_some_and(|e| e == "rs") {
                into.push(path);
            }
        }
    }

    /// The words of `body`, upper-cased, so SQL is found however it is cased.
    fn words(body: &str) -> Vec<String> {
        body.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|w| !w.is_empty())
            .map(str::to_ascii_uppercase)
            .collect()
    }

    /// Whether `body` holds a statement that writes a row.
    fn writes(body: &str) -> bool {
        let w = words(body);
        w.windows(2).any(|pair| match pair {
            [first, second] => matches!(
                (first.as_str(), second.as_str()),
                ("INSERT", "INTO" | "OR") | ("DELETE", "FROM") | ("REPLACE", "INTO")
            ),
            _ => false,
        }) || w
            .windows(3)
            .any(|three| matches!(three, [update, _, set] if update == "UPDATE" && set == "SET"))
    }

    /// Whether `body` begins, ends or nests a transaction of its own, through
    /// `rusqlite` or in SQL.
    fn controls_a_transaction(body: &str) -> bool {
        let upper = body.to_ascii_uppercase();
        [
            ".transaction(",
            "unchecked_transaction(",
            ".commit(",
            ".rollback(",
            ".savepoint(",
        ]
        .iter()
        .any(|call| body.contains(call))
            || ["BEGIN", "COMMIT", "ROLLBACK", "SAVEPOINT", "RELEASE"]
                .iter()
                .any(|keyword| opens_a_string(&upper, keyword))
    }

    /// Whether some string literal in `upper` opens with `keyword` as a whole
    /// word: `"COMMIT"` or `"BEGIN IMMEDIATE"`, but not `"COMMIT_INDEX"`.
    fn opens_a_string(upper: &str, keyword: &str) -> bool {
        let quoted = format!("\"{keyword}");
        upper.match_indices(&quoted).any(|(at, _)| {
            upper[at + quoted.len()..]
                .chars()
                .next()
                .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'))
        })
    }

    /// Every function of the shipped crate, outside its dev-only fixtures.
    fn shipped_functions() -> Vec<Function> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&root, &mut files);
        files.sort();
        files
            .iter()
            .filter(|f| !f.strip_prefix(&root).unwrap().starts_with(DEV_ONLY))
            .flat_map(|f| functions(&root, f))
            .collect()
    }

    /// I-9's precondition, read from the crate's own source: every shipped
    /// function that writes a row takes the caller's `&Transaction`, and none
    /// but the migration runner begins or ends one of its own.
    #[test]
    fn every_fact_repository_write_requires_a_transaction() {
        let all = shipped_functions();
        let writers: Vec<&Function> = all.iter().filter(|f| writes(&f.body)).collect();
        let without: Vec<String> = writers
            .iter()
            .filter(|f| !f.params.contains("Transaction"))
            .map(|f| format!("{}:{} {}", f.file, f.line, f.name))
            .collect();
        assert!(
            without.is_empty(),
            "these write without the caller's transaction: {without:#?}"
        );
        let controlling: Vec<String> = all
            .iter()
            .filter(|f| controls_a_transaction(&f.body))
            .filter(|f| !OWNS_ITS_TRANSACTIONS.contains(&(f.file.as_str(), f.name.as_str())))
            .map(|f| format!("{}:{} {}", f.file, f.line, f.name))
            .collect();
        assert!(
            controlling.is_empty(),
            "these begin or end a transaction of their own: {controlling:#?}"
        );
        // The scan is not vacuous: it finds the writers this crate is known to
        // have, and the runner it exempts.
        let names: Vec<String> = writers
            .iter()
            .map(|f| format!("{} {}", f.file, f.name))
            .collect();
        for known in [
            "repo/outbox.rs write_commit",
            "repo/approval.rs insert",
            "repo/approval.rs consume",
            "repo/audit.rs append",
            "repo/stock.rs append",
            "repo/sequence.rs next",
            "repo/clock.rs save",
            "tax_pack.rs import_tax_pack",
        ] {
            assert!(names.iter().any(|n| n == known), "{known} was not found");
        }
        assert!(writers.len() >= 15, "only {} writers found", writers.len());
        for owner in OWNS_ITS_TRANSACTIONS {
            assert!(
                all.iter()
                    .any(|f| (f.file.as_str(), f.name.as_str()) == owner
                        && controls_a_transaction(&f.body)),
                "{owner:?} was not found owning its transactions"
            );
        }
    }

    /// The scan's two questions, put to source that answers them each way.
    #[test]
    fn the_scan_sees_a_write_and_a_transaction_in_any_spelling() {
        assert!(writes(
            r#"tx.execute("insert into sale (id) values (?1)", p)"#
        ));
        assert!(writes(
            r#"c.execute("INSERT OR IGNORE INTO x VALUES (1)", [])"#
        ));
        assert!(writes(r#"c.execute("UPDATE stock_cache SET qty = 0", [])"#));
        assert!(writes(
            r#"c.execute("DELETE FROM parked_cart WHERE id = ?1", p)"#
        ));
        assert!(!writes(
            r#"c.query_row("SELECT count(*) FROM sale", [], f)"#
        ));
        assert!(!writes(&without_comments(
            "// INSERT INTO, in a comment\nlet x = 1;"
        )));
        assert!(controls_a_transaction(
            "let tx = conn.unchecked_transaction()?;"
        ));
        assert!(controls_a_transaction(
            r#"conn.execute_batch("BEGIN IMMEDIATE")?;"#
        ));
        assert!(controls_a_transaction("tx.commit()?;"));
        assert!(controls_a_transaction(r#"conn.execute("COMMIT", [])?;"#));
        assert!(!controls_a_transaction(
            r#"f.field("commit_index", &self.commit_index)"#
        ));
        assert!(!controls_a_transaction(
            "fn f(tx: &Transaction<'_>) { tx.execute(q, p) }"
        ));
        let stripped = without_comments("a /* INSERT INTO */ b // UPDATE x SET\n\"// kept\"");
        assert!(!writes(&stripped) && stripped.contains("\"// kept\""));
        // A quote or a bracket inside a literal is not code, in any spelling.
        for literal in [
            r#""a \" { b""#,
            r##"r#"a " { b"#"##,
            r#"b"{""#,
            r#"'"'"#,
            r"'{'",
            r"b'}'",
            r"'\''",
            "'é'",
        ] {
            let source = format!("fn f() {{ let x = {literal}; }}");
            let open = source.find('{').unwrap();
            assert_eq!(
                closing(source.as_bytes(), open, b'{', b'}'),
                source.len() - 1,
                "{literal}"
            );
            assert_eq!(without_comments(&source), source, "{literal}");
        }
        // A lifetime is no literal.
        assert_eq!(literal_end(b"'a>", 0), None);
    }

    /// The boundary is the caller's: a write inside a transaction the caller
    /// rolls back leaves nothing, and the same write committed is there.
    #[test]
    fn a_write_is_kept_or_undone_with_the_callers_transaction() {
        const REGISTER: [u8; 16] = [0x0c; 16];
        let dir = tempfile::tempdir().unwrap();
        let conn = pos_db::open(&dir.path().join("register.db"), "test-key").unwrap();
        RegisteredChain::seed(&conn).add_register(&conn, REGISTER.as_slice(), "REG01");
        let at = Timestamp::parse_iso8601("2026-08-25T10:00:00.000Z").unwrap();
        let stored = StoredClock {
            state: ClockState {
                last_trusted_at: Some(at),
                device_at_trust: Some(at),
                monotonic_since_trust_ms: Some(0),
                high_water: at,
                anomaly: None,
            },
            boot_token: None,
            updated_at: "2026-08-25T10:00:00.000Z".to_owned(),
        };
        let repo = ClockRepository::new(&conn);
        let tx = conn.unchecked_transaction().unwrap();
        repo.save(&tx, &REGISTER, &stored).unwrap();
        tx.rollback().unwrap();
        assert_eq!(repo.load(&REGISTER).unwrap(), None);
        let tx = conn.unchecked_transaction().unwrap();
        repo.save(&tx, &REGISTER, &stored).unwrap();
        tx.commit().unwrap();
        assert_eq!(repo.load(&REGISTER).unwrap(), Some(stored));
    }

    /// A fact write handed only a connection does not compile
    /// (`tests/ui/fact_write_without_transaction.rs`).
    #[test]
    fn a_fact_write_with_only_a_connection_does_not_compile() {
        let cases = trybuild::TestCases::new();
        cases.compile_fail("tests/ui/fact_write_without_transaction.rs");
    }
}
