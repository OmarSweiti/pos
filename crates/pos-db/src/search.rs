//! The query side of Arabic catalogue search (microsteps 1.2.5 and 1.2.3).
//!
//! Migration `0003` gives every product two generated search projections of
//! its Arabic name, and `0007` indexes both:
//!
//! * `name_ar_exact` strips only the marks nobody types into a search box —
//!   tashkeel, the superscript alef and tatweel. Spelling is preserved, so the
//!   spelling a cashier typed can outrank a near-miss.
//! * `name_ar_fold` strips the same marks and also collapses the spelling
//!   variants that must find each other: hamza forms of alef to alef, alef
//!   maqsura and yaa-hamza to yaa, taa marbuta to haa, waw-hamza to waw.
//!
//! **Both sides must transform.** A folded index searched with an unfolded
//! string returns nothing, so a query is put through the same two functions
//! before it reaches FTS5. These are a second implementation of a rule SQL
//! already implements, which is exactly how two implementations drift:
//! `prop_sql_and_rust_folding_agree` (`tests/migration_0007_search_and_seed.rs`)
//! holds them to the generated columns for any input.
//!
//! Character for character, these are `0003`'s `replace()` chains. SQLite's
//! `replace` works on UTF-8 bytes, and UTF-8 is self-synchronising, so replacing
//! a whole character's encoding is replacing that character.

/// What `name_ar_exact` removes: U+064B–U+0655 (tashkeel and the hamza marks),
/// U+0670 (superscript alef) and U+0640 (tatweel) — `0003`'s `char(1611)`
/// through `char(1621)`, `char(1648)` and `char(1600)`.
const STRIPPED: [char; 13] = [
    '\u{064B}', '\u{064C}', '\u{064D}', '\u{064E}', '\u{064F}', '\u{0650}', '\u{0651}', '\u{0652}',
    '\u{0653}', '\u{0654}', '\u{0655}', '\u{0670}', '\u{0640}',
];

/// What `name_ar_fold` maps after stripping, in `0003`'s order. No target is
/// also a source, so applying them one character at a time is the same as
/// SQLite applying them one `replace` at a time.
const FOLDED: [(char, char); 8] = [
    ('\u{0623}', '\u{0627}'), // alef with hamza above → alef
    ('\u{0625}', '\u{0627}'), // alef with hamza below → alef
    ('\u{0622}', '\u{0627}'), // alef with maddah → alef
    ('\u{0671}', '\u{0627}'), // alef wasla → alef
    ('\u{0649}', '\u{064A}'), // alef maqsura → yaa
    ('\u{0629}', '\u{0647}'), // taa marbuta → haa
    ('\u{0624}', '\u{0648}'), // waw with hamza → waw
    ('\u{0626}', '\u{064A}'), // yaa with hamza → yaa
];

/// A query as `name_ar_exact` stores a name: the untyped marks removed, the
/// spelling kept.
#[must_use]
pub fn exact(text: &str) -> String {
    text.chars().filter(|c| !STRIPPED.contains(c)).collect()
}

/// A query as `name_ar_fold` stores a name: the untyped marks removed and the
/// spelling variants collapsed.
#[must_use]
pub fn fold(text: &str) -> String {
    text.chars()
        .filter(|c| !STRIPPED.contains(c))
        .map(|c| {
            FOLDED
                .iter()
                .find(|(from, _)| *from == c)
                .map_or(c, |(_, to)| *to)
        })
        .collect()
}

/// The FTS5 `MATCH` expression for what a cashier typed, or `None` when it
/// holds nothing to search for (E.39b, microstep 1.2.3).
///
/// **The typed text never becomes FTS5 syntax.** `MATCH` parses its right-hand
/// side as a query expression however it was bound, so `"`, `(`, `)`, `:`,
/// `*`, `^`, `-` and the barewords `AND`, `OR`, `NOT` and `NEAR` are operators
/// there, not letters. A cashier who types one would get a syntax error — on
/// the only path they have when the scanner fails — or, worse, a different
/// query. So the text is cut into tokens of letters and digits, every other
/// character is a boundary, each token is quoted as an FTS5 string with any
/// `"` doubled, and the tokens are joined with `AND`, the last one a prefix
/// (`"…"*`) so results arrive as the cashier types. A quoted string is a
/// phrase to FTS5, never an operator: `"OR"` is the word *or*.
///
/// A token is a run of `char::is_alphanumeric` characters. FTS5's `unicode61`
/// draws its boundaries from Unicode categories instead, and the two differ at
/// the edges — a rare combining mark Rust counts as alphabetic, a private-use
/// character FTS5 counts as a letter. A difference can only cost a match,
/// never raise an error: whatever is inside the quotes, FTS5 reads it as a
/// phrase, and a phrase with nothing in it matches nothing.
///
/// **Three branches, as `0007` indexes them.** `name_ar_exact` and
/// `name_ar_fold` both carry the Arabic name: a product spelled exactly as
/// typed matches both and so outranks one that matches only through the fold
/// (`exact_spelling_outranks_a_folded_variant`). The third branch covers the
/// English name, the SKU and the barcode digits, which a cashier types when a
/// label will not scan.
#[must_use]
pub fn match_expression(typed: &str) -> Option<String> {
    // Strip first, then cut: removing a mark can join two runs of letters, and
    // the fold maps letters to letters, so every branch cuts in the same places.
    let stripped = exact(typed);
    let tokens: Vec<&str> = stripped
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    let (last, rest) = tokens.split_last()?;
    let branch = |spell: fn(&str) -> String| {
        rest.iter()
            .map(|token| quoted(&spell(token)))
            .chain(core::iter::once(format!("{}*", quoted(&spell(last)))))
            .collect::<Vec<_>>()
            .join(" AND ")
    };
    Some(format!(
        "{{name_ar_exact}} : ({}) OR {{name_ar_fold}} : ({}) OR {{name_en sku barcodes}} : ({})",
        branch(str::to_owned),
        branch(fold),
        branch(str::to_owned),
    ))
}

/// One token as an FTS5 string. The split above never leaves a `"` in a
/// token, so the doubling is a second line rather than the only one.
fn quoted(token: &str) -> String {
    format!("\"{}\"", token.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_keeps_spelling_and_fold_collapses_it() {
        // قَهْوَة: tashkeel removed by both; the taa marbuta kept by one.
        let vocalised = "\u{0642}\u{064E}\u{0647}\u{0652}\u{0648}\u{064E}\u{0629}";
        assert_eq!(exact(vocalised), "\u{0642}\u{0647}\u{0648}\u{0629}");
        assert_eq!(fold(vocalised), "\u{0642}\u{0647}\u{0648}\u{0647}");
        // شـاي: tatweel removed.
        assert_eq!(
            exact("\u{0634}\u{0640}\u{0627}\u{064A}"),
            "\u{0634}\u{0627}\u{064A}"
        );
        // Latin and digits pass through both untouched.
        assert_eq!(fold("Cola 330ml"), "Cola 330ml");
    }

    #[test]
    fn a_match_expression_quotes_every_token_and_prefixes_the_last() {
        // One Arabic word, typed with tashkeel: the exact branch keeps its
        // spelling, the fold branch collapses it, and the third branch carries
        // it unchanged for the English, SKU and barcode columns.
        assert_eq!(
            match_expression("\u{0642}\u{064E}\u{0647}\u{0652}\u{0648}\u{064E}\u{0629}").as_deref(),
            Some(concat!(
                "{name_ar_exact} : (\"\u{0642}\u{0647}\u{0648}\u{0629}\"*) OR ",
                "{name_ar_fold} : (\"\u{0642}\u{0647}\u{0648}\u{0647}\"*) OR ",
                "{name_en sku barcodes} : (\"\u{0642}\u{0647}\u{0648}\u{0629}\"*)"
            ))
        );
        // Operators become words, punctuation becomes a boundary, and only the
        // last token is a prefix.
        assert_eq!(
            match_expression("cola OR (330:*)").as_deref(),
            Some(concat!(
                "{name_ar_exact} : (\"cola\" AND \"OR\" AND \"330\"*) OR ",
                "{name_ar_fold} : (\"cola\" AND \"OR\" AND \"330\"*) OR ",
                "{name_en sku barcodes} : (\"cola\" AND \"OR\" AND \"330\"*)"
            ))
        );
        // Nothing to search for is not a query.
        for nothing in ["", "   ", "\t\n", "\"", "()", "-*^:", "\u{064E}\u{0640}"] {
            assert_eq!(match_expression(nothing), None, "{nothing:?}");
        }
        // The doubling, exercised directly: the split never hands it a quote.
        assert_eq!(quoted("a\"b"), "\"a\"\"b\"");
    }
}
