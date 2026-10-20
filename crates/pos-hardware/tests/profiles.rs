//! Device profiles as data (microstep 1.7.11): the three rules of
//! `ref/hardware-and-receipts.md` §6b.2, and the seed the files start from.
//!
//! Every test here reads the committed files under `profiles/`, so a file that
//! changes is a file these tests see.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use pos_hardware::profile::data::{self, Describes, DeviceClass, DeviceProfile};
use pos_hardware::profile::{BINDING, Fact, Parameter, PrinterProfile, ProfileError};
use pos_hardware::render::PaperWidth;

/// The committed hash of every profile version, one line each:
/// `<blake3>  <class>/<file>`, sorted by path.
const MANIFEST: &str = "versions.blake3";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(data::DIR)
}

fn profiles() -> Vec<(PathBuf, DeviceProfile)> {
    data::load_dir(&root()).unwrap_or_else(|error| panic!("a profile does not load: {error}"))
}

/// A profile's path relative to `profiles/`, with `/` on every platform.
fn relative(path: &Path) -> String {
    path.strip_prefix(root())
        .expect("a profile under profiles/")
        .to_string_lossy()
        .replace('\\', "/")
}

/// The named test of rule 1, and of the file's shape.
///
/// That every file loads already proves most of it: every value its class has
/// is present, nothing else is, and each carries a non-empty source. What a
/// loader cannot judge is whether the source is a citation rather than an
/// adjective, so this does: every source names a section of a document in this
/// repository or an address outside it. "The datasheet" alone does not pass.
#[test]
fn every_device_profile_names_a_source_for_every_value() {
    let profiles = profiles();
    assert!(
        !profiles.is_empty(),
        "no profile under {}",
        root().display()
    );

    let mut faults = Vec::new();
    for (path, profile) in &profiles {
        for entry in profile.values.entries() {
            let place = format!("{} {}", relative(path), entry.key);
            if !(entry.source.contains('§') || entry.source.contains("https://")) {
                faults.push(format!("{place}: cites nothing: {:?}", entry.source));
            }
            // A class is synthetic by definition and must say so in every
            // value; a model never is, so a synthetic value there is a guess.
            let synthetic =
                entry.source.starts_with("Synthetic") || entry.source.starts_with("Not applicable");
            match profile.describes {
                Describes::Class if !synthetic => {
                    faults.push(format!(
                        "{place}: a class's value that does not say it is synthetic"
                    ));
                }
                Describes::Model if synthetic => {
                    faults.push(format!("{place}: a model's value that is synthetic"));
                }
                _ => {}
            }
        }
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));

    // Nothing lives beside the classes but this directory's own two files: a
    // stray file is a profile nobody loads.
    let mut stray = Vec::new();
    for entry in fs::read_dir(root()).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        let known = DeviceClass::ALL.iter().any(|class| class.dir() == name)
            || name == "README.md"
            || name == MANIFEST;
        if !known {
            stray.push(name);
        }
    }
    assert!(
        stray.is_empty(),
        "not a class, the README or the manifest: {stray:?}"
    );
}

/// Forget one parameter of a profile, as a file that leaves it unknown would.
fn forget(profile: &mut PrinterProfile, parameter: Parameter) {
    match parameter {
        Parameter::Transport => profile.transport = Fact::Unknown,
        Parameter::DotWidth => profile.dot_width = Fact::Unknown,
        Parameter::RasterCommand => profile.raster_command = Fact::Unknown,
        Parameter::RasterMaxRows => profile.raster_max_rows = Fact::Unknown,
        Parameter::AutoCutter => profile.auto_cutter = Fact::Unknown,
        Parameter::CutCommand => profile.cut_command = Fact::Unknown,
        Parameter::CutFeedRows => profile.cut_feed_rows = Fact::Unknown,
        Parameter::StatusProtocol => profile.status_protocol = Fact::Unknown,
        Parameter::DrawerPulse => profile.drawer_pulse = Fact::Unknown,
        Parameter::SupportsRasterArabic => profile.supports_raster_arabic = Fact::Unknown,
    }
}

/// The named test of rule 2's consequence: a profile with an unknown in a
/// parameter the driver needs cannot be bound to a register
/// (`ref/hardware-and-receipts.md` §6b.5, rule 4) — 1.7.9's rule for the
/// raster limit, generalised to every such parameter.
///
/// Proven one parameter at a time against the simulator, the one profile that
/// knows them all, and then against every seeded printer: none may be bound
/// before the gate, and each whose raster limit nobody measured says so.
#[test]
fn an_unknown_profile_value_cannot_be_bound_to_a_register() {
    for paper in PaperWidth::ALL {
        let complete = PrinterProfile::simulator(paper);
        assert_eq!(complete.bindable(), Ok(()), "{paper:?}");
        for parameter in BINDING {
            let mut profile = complete.clone();
            forget(&mut profile, parameter);
            assert_eq!(profile.unresolved(), vec![parameter], "{paper:?}");
            assert_eq!(
                profile.bindable(),
                Err(ProfileError::Unknown {
                    profile_id: complete.profile_id.clone(),
                    parameter,
                }),
                "{paper:?}"
            );
        }
    }

    let mut printers = 0;
    for (path, profile) in profiles() {
        let Some(record) = profile.printer_profile() else {
            continue;
        };
        printers += 1;
        let place = relative(&path);
        let unresolved = record.unresolved();
        assert!(
            record.bindable().is_err() && !unresolved.is_empty(),
            "{place} can be bound to a register before any device was held"
        );
        // The parameters the gate must settle are exactly the file's unknowns
        // among the ones a driver needs.
        let unknown: BTreeSet<&str> = profile.unknown_values().into_iter().collect();
        let needed: Vec<Parameter> = BINDING
            .into_iter()
            .filter(|parameter| unknown.contains(parameter.key()))
            .collect();
        assert_eq!(unresolved, needed, "{place}");

        if record.raster_max_rows.is_unknown() {
            assert!(unresolved.contains(&Parameter::RasterMaxRows), "{place}");
            assert_eq!(
                record.row_limit(),
                Err(ProfileError::Unknown {
                    profile_id: profile.profile_id.clone(),
                    parameter: Parameter::RasterMaxRows,
                }),
                "{place}"
            );
        }
    }
    assert_eq!(printers, 8, "the seed's receipt printers");
}

/// The committed manifest, path to hash.
fn committed() -> BTreeMap<String, String> {
    let text = fs::read_to_string(root().join(MANIFEST)).unwrap_or_default();
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (hash, path) = line
                .split_once("  ")
                .unwrap_or_else(|| panic!("{MANIFEST}: not `<hash>  <path>`: {line:?}"));
            (path.to_owned(), hash.to_owned())
        })
        .collect()
}

fn hash_of(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Every way the files disagree with the manifest, one line each.
fn drift(committed: &BTreeMap<String, String>, actual: &BTreeMap<String, String>) -> Vec<String> {
    let mut drift = Vec::new();
    for (path, hash) in actual {
        match committed.get(path) {
            None => drift.push(format!("{path}: a version with no committed hash")),
            Some(line) if line != hash => drift.push(format!(
                "{path}: changed in place — restore it, and write the change as a new version beside it"
            )),
            Some(_) => {}
        }
    }
    for path in committed.keys() {
        if !actual.contains_key(path) {
            drift.push(format!(
                "{path}: committed, and gone — an old version stays"
            ));
        }
    }
    drift
}

/// The named test of rule 3: a changed value is a new profile version, and the
/// old one stays.
///
/// `versions.blake3` commits one hash per version. An edit in place changes a
/// file's hash and fails here; deleting a version fails here; and a new version
/// is a new file beside the old one, with a line of its own.
#[test]
fn a_changed_profile_value_is_a_new_profile_version() {
    let profiles = profiles();
    let actual: BTreeMap<String, String> = profiles
        .iter()
        .map(|(path, _)| (relative(path), hash_of(&fs::read(path).unwrap())))
        .collect();
    let committed = committed();
    let found = drift(&committed, &actual);
    if !found.is_empty() {
        let manifest: String = actual
            .iter()
            .map(|(path, hash)| format!("{hash}  {path}\n"))
            .collect();
        panic!(
            "{}\n\nOnce each is a new version, {MANIFEST} reads:\n{manifest}",
            found.join("\n")
        );
    }

    // One line per version, in path order, so a merge cannot hide a duplicate.
    let text = fs::read_to_string(root().join(MANIFEST)).unwrap();
    let paths: Vec<&str> = text
        .lines()
        .filter_map(|line| line.trim_end().split_once("  ").map(|(_, path)| path))
        .collect();
    let mut ordered = paths.clone();
    ordered.sort_unstable();
    ordered.dedup();
    assert_eq!(paths, ordered, "{MANIFEST} is sorted, one line per version");

    // Versions run 1, 2, … with none missing: an old one is never deleted.
    let mut versions: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for (_, profile) in &profiles {
        versions
            .entry(format!("{}/{}", profile.class(), profile.profile_id))
            .or_default()
            .push(profile.version.get());
    }
    for (profile, mut found) in versions {
        found.sort_unstable();
        let expected: Vec<u32> = (1..).take(found.len()).collect();
        assert_eq!(found, expected, "{profile}: a version is missing");
    }

    // And the check bites: one value edited in place is drift.
    let (path, _) = profiles
        .iter()
        .find(|(_, profile)| profile.class() == DeviceClass::ReceiptPrinter)
        .expect("a receipt printer");
    let original = fs::read_to_string(path).unwrap();
    let edited = original
        .replacen(
            "\"paper_mm\": {\n      \"value\": 80",
            "\"paper_mm\": {\n      \"value\": 79",
            1,
        )
        .replacen(
            "\"paper_mm\": {\n      \"value\": 58",
            "\"paper_mm\": {\n      \"value\": 57",
            1,
        );
    assert_ne!(edited, original, "the fixture must change a value");
    assert!(
        data::parse(&edited).is_ok(),
        "the edit is still a valid profile"
    );
    let mut tampered = actual.clone();
    tampered.insert(relative(path), hash_of(edited.as_bytes()));
    assert_eq!(drift(&committed, &tampered).len(), 1);
}

/// The receipt-printer values no seed source states for any candidate — the
/// class sentence under §6b.2's seed table.
const PRINTER_UNSTATED: &[&str] = &[
    "firmware_qualified",
    "code_pages",
    "cut_command",
    "cut_feed_rows",
    "drawer_pulse",
    "supports_raster_arabic",
];
const DRAWER_UNSTATED: &[&str] = &["pulse_on_ms", "pulse_off_ms"];
const SCANNER_MODEL_UNSTATED: &[&str] = &["prefix", "suffix", "symbologies"];

/// `ref/hardware-and-receipts.md` §6b.2's seed table, row by row: each v1 file,
/// what it describes, and what its row leaves unknown beyond its class.
const SEED: &[(&str, Describes, &[&str])] = &[
    (
        "receipt_printer/epson-tm-t20iii-ethernet-80mm.v1.json",
        Describes::Model,
        &[],
    ),
    (
        "receipt_printer/epson-tm-t20iv-80mm.v1.json",
        Describes::Model,
        &["auto_cutter", "status_protocol", "drawer_port_volts"],
    ),
    (
        "receipt_printer/epson-tm-t88vii-80mm.v1.json",
        Describes::Model,
        &["auto_cutter", "status_protocol", "drawer_port_volts"],
    ),
    (
        "receipt_printer/epson-tm-m10-58mm.v1.json",
        Describes::Model,
        &[
            "transport",
            "raster_command",
            "raster_max_rows",
            "auto_cutter",
            "status_protocol",
            "drawer_port_volts",
        ],
    ),
    (
        "receipt_printer/bixolon-srp-332iii-80mm.v1.json",
        Describes::Model,
        &["raster_command", "raster_max_rows", "status_protocol"],
    ),
    (
        "receipt_printer/bixolon-srp-332iii-58mm.v1.json",
        Describes::Model,
        &["raster_command", "raster_max_rows", "status_protocol"],
    ),
    (
        "receipt_printer/xprinter-xp-n160ii-ethernet-80mm.v1.json",
        Describes::Model,
        &[
            "dot_width",
            "raster_command",
            "raster_max_rows",
            "auto_cutter",
            "drawer_port_volts",
        ],
    ),
    (
        "receipt_printer/hprt-tp585-58mm.v1.json",
        Describes::Model,
        &[
            "dot_width",
            "raster_command",
            "raster_max_rows",
            "status_protocol",
        ],
    ),
    (
        "scanner/datalogic-quickscan-qd2590.v1.json",
        Describes::Model,
        &["inter_key_ms"],
    ),
    (
        "scanner/zebra-ds2208.v1.json",
        Describes::Model,
        &["inter_key_ms", "reads_2d"],
    ),
    (
        "scanner/honeywell-voyager-1470g.v1.json",
        Describes::Model,
        &["inter_key_ms", "reads_2d"],
    ),
    (
        "cash_drawer/posiflex-cr-4000-g2.v1.json",
        Describes::Model,
        &["pin", "solenoid_ohms", "sensor", "sensor_polarity"],
    ),
    (
        "cash_drawer/apg-vasario-vb320.v1.json",
        Describes::Model,
        &["pin", "solenoid_ohms", "sensor", "sensor_polarity"],
    ),
    (
        "register_screen/posiflex-xt-3815-g2.v1.json",
        Describes::Model,
        &["display_scales_percent"],
    ),
    (
        "register_screen/msi-pro-ap162t.v1.json",
        Describes::Model,
        &["display_scales_percent"],
    ),
    (
        "register_screen/minimum-window-class.v1.json",
        Describes::Class,
        &["panel_tenths_inch"],
    ),
    ("scanner/wedge-scanner-class.v1.json", Describes::Class, &[]),
];

/// Everything a seeded profile leaves unknown: its row, and — for a model —
/// what no seed source states for any candidate of its class.
fn seeded_unknowns(class: DeviceClass, describes: Describes, row: &[&str]) -> BTreeSet<String> {
    let class_wide: &[&str] = match (describes, class) {
        (Describes::Class, _) | (Describes::Model, DeviceClass::RegisterScreen) => &[],
        (Describes::Model, DeviceClass::ReceiptPrinter) => PRINTER_UNSTATED,
        (Describes::Model, DeviceClass::CashDrawer) => DRAWER_UNSTATED,
        (Describes::Model, DeviceClass::Scanner) => SCANNER_MODEL_UNSTATED,
    };
    row.iter()
        .chain(class_wide)
        .map(|key| (*key).to_owned())
        .collect()
}

/// The `Done when`'s seed half: every seeded profile loads, and leaves a value
/// unknown exactly where the reference's seed table says one is — so a guess
/// that filled a gap fails, and so does a gap left where a source spoke.
///
/// Only the first version of each is held to the seed. Once 1.7.11 lands the
/// files are authoritative, and the gate's evidence moves them on as new
/// versions.
#[test]
fn every_seeded_profile_is_unknown_exactly_where_the_seed_table_says() {
    let profiles = profiles();
    let seeded: BTreeMap<String, &DeviceProfile> = profiles
        .iter()
        .filter(|(_, profile)| profile.version.get() == 1)
        .map(|(path, profile)| (relative(path), profile))
        .collect();
    let rows: BTreeSet<String> = SEED.iter().map(|(file, _, _)| (*file).to_owned()).collect();
    assert_eq!(
        seeded.keys().cloned().collect::<BTreeSet<_>>(),
        rows,
        "the first versions are the seed table's rows, no more and no fewer"
    );

    for (file, describes, row) in SEED {
        let profile = seeded
            .get(*file)
            .unwrap_or_else(|| panic!("{file}: in the seed table, not on disk"));
        assert_eq!(profile.describes, *describes, "{file}");
        let unknown: BTreeSet<String> = profile
            .unknown_values()
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(
            unknown,
            seeded_unknowns(profile.class(), *describes, row),
            "{file}: unknown where the seed table says it is not, or known where it says it is unknown"
        );
    }
}
