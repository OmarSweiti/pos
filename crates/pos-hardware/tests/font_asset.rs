//! Microstep 1.7.2 — the embedded typeface is really there, and really licensed.
//!
//! Both assertions are about a **committed asset**, which is an unusual thing
//! to test and worth saying why. A font is the one dependency this application
//! carries that no package manager resolves, no lockfile pins and no advisory
//! feed watches. It is bytes somebody copied in. The two failure modes are
//! therefore the dull ones — a placeholder that was never replaced, and a
//! licence that was never copied alongside — and neither is visible in a diff
//! that shows `Binary files differ`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use pos_hardware::font::{self, Weight};

/// `assets/fonts/`, from this crate's manifest directory.
///
/// The constants below are compiled in, so they prove the *binary* carries the
/// font. This path is how the tests also speak about the **repository**, which
/// is what the microstep's wording asks for: "the font and its licence in the
/// repository".
fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts")
}

/// The `sfnt` version every TrueType outline file opens with.
///
/// `0x00010000` is TrueType; an OpenType/CFF file would read `OTTO`. Asserting
/// it is what makes "not empty" mean something: a text placeholder, a Git LFS
/// pointer or a truncated download are all non-empty and all fail here.
const TRUETYPE_MAGIC: [u8; 4] = [0x00, 0x01, 0x00, 0x00];

#[test]
fn embedded_font_has_a_repository_licence() {
    // OFL 1.1 permits embedding and redistribution *provided the licence
    // travels with the font*. A binary carrying the glyphs and leaving the
    // licence behind has not satisfied that, so the licence is compiled in too.
    assert!(
        font::LICENCE.contains("SIL OPEN FONT LICENSE Version 1.1"),
        "the embedded licence must be the OFL 1.1 text itself, not a reference to it"
    );
    assert!(
        font::LICENCE.contains("IBM Corp."),
        "the OFL requires the copyright notice to travel with the licence"
    );

    // And it is in the repository beside the faces, not only inside the binary
    // — which is the half a reader can check without building anything.
    let on_disk = assets().join("LICENSE.txt");
    assert!(
        on_disk.is_file(),
        "assets/fonts/LICENSE.txt is missing from the repository: {}",
        on_disk.display()
    );
    assert_eq!(
        std::fs::read_to_string(&on_disk).unwrap(),
        font::LICENCE,
        "the committed licence and the compiled-in one must be the same text"
    );
}

#[test]
fn embedded_font_bytes_are_not_empty() {
    for (weight, bytes) in [(Weight::Regular, font::REGULAR), (Weight::Bold, font::BOLD)] {
        assert!(!bytes.is_empty(), "{weight:?} is empty");
        assert_eq!(
            bytes.get(..4),
            Some(&TRUETYPE_MAGIC[..]),
            "{weight:?} does not begin with the TrueType sfnt version, so it is \
             not a font — a placeholder, an LFS pointer and a truncated \
             download are all non-empty and all land here"
        );
        assert_eq!(font::face(weight), bytes, "face({weight:?}) returns it");
    }

    // Two faces, not one file copied twice. A bold that is byte-identical to
    // the regular renders a receipt whose totals do not stand out, and nothing
    // above would notice.
    assert_ne!(
        font::REGULAR,
        font::BOLD,
        "Regular and Bold are the same bytes"
    );
}

/// The faces are in the repository as well as in the binary, and small enough
/// to commit.
///
/// `scripts/check-staged-policy.py:11` refuses a staged blob over 2,000,000
/// bytes. Both static TTF faces are far inside that, but the family archive and
/// the variable-font build are not necessarily — so this fails loudly if
/// someone swaps a face for something that would be refused at commit time
/// rather than discovering it from a rejected push.
#[test]
fn the_committed_faces_match_the_compiled_ones() {
    const MAX_STAGED_BLOB_BYTES: usize = 2_000_000;

    for (name, compiled) in [
        ("IBMPlexSansArabic-Regular.ttf", font::REGULAR),
        ("IBMPlexSansArabic-Bold.ttf", font::BOLD),
    ] {
        let path = assets().join(name);
        let committed =
            std::fs::read(&path).unwrap_or_else(|e| panic!("{} is missing: {e}", path.display()));
        assert_eq!(
            committed, compiled,
            "{name} on disk differs from the binary"
        );
        assert!(
            committed.len() < MAX_STAGED_BLOB_BYTES,
            "{name} is {} bytes, past check-staged-policy.py's {MAX_STAGED_BLOB_BYTES}",
            committed.len()
        );
    }
}

#[test]
fn the_family_is_the_one_the_plan_chose() {
    assert_eq!(font::FAMILY, "IBM Plex Sans Arabic");
}
