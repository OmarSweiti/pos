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
}
