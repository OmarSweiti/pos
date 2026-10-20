//! Device profiles as data (microstep 1.7.11): `ref/hardware-and-receipts.md`
//! §6b.2's versioned files, read by the Rust emulators and suites — and, the
//! format being JSON, by the register's web tests alike.
//!
//! # The file
//!
//! `profiles/<class>/<profile_id>.v<version>.json`, one per version:
//!
//! ```json
//! {
//!   "schema": 1,
//!   "class": "receipt_printer",
//!   "profile_id": "epson-tm-t20iii-ethernet-80mm",
//!   "version": 1,
//!   "describes": "model",
//!   "values": {
//!     "dot_width": { "value": 576, "source": "Epson, TM-T20III Technical Reference Guide, …" },
//!     "cut_feed_rows": { "value": "unknown", "source": "Not stated by any seed source …" }
//!   }
//! }
//! ```
//!
//! # Three rules, and where each is enforced
//!
//! 1. **Every value names its source.** [`Sourced`] has no form without one,
//!    and an empty one does not load. That the source is a citation rather
//!    than an adjective is `tests/profiles.rs`'s
//!    `every_device_profile_names_a_source_for_every_value`.
//! 2. **`unknown` is a value, never an omitted field.** Each class's values are
//!    a closed set: a file missing one does not load, and neither does a file
//!    naming one its class does not have. The string `"unknown"` is therefore
//!    reserved — no value may be that literal string.
//! 3. **A changed value is a new version, and the old one stays.** The version
//!    is in the file name, and `profiles/versions.blake3` commits one hash per
//!    version, so an edit in place fails
//!    `a_changed_profile_value_is_a_new_profile_version`.
//!
//! A file states no qualification. Qualification is §6a's matrix, which needs a
//! device somebody held, so every profile read from a file is a candidate.

use core::fmt;
use core::num::NonZeroU32;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Deserializer};

use super::{
    CutCommand, DrawerPin, DrawerPulse, Fact, PrinterProfile, Qualification, RasterCommand,
    StatusProtocol, Transport,
};

/// The file format this build reads.
pub const SCHEMA: u32 = 1;

/// The value a file writes for a fact nobody has stated.
pub const UNKNOWN: &str = "unknown";

/// The directory, beside this crate's manifest, the files live in.
pub const DIR: &str = "profiles";

/// One value and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sourced<T> {
    pub fact: Fact<T>,
    /// A citation: a maker's document, a seller's written answer, a capture
    /// from a device — or, for an unknown, where it was looked for.
    pub source: String,
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Sourced<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Repr {
            value: serde_json::Value,
            source: String,
        }

        let repr = Repr::deserialize(deserializer)?;
        if repr.source.trim().is_empty() {
            return Err(D::Error::custom(
                "every value names its source, and this one's is empty",
            ));
        }
        let fact = if repr.value.as_str() == Some(UNKNOWN) {
            Fact::Unknown
        } else {
            Fact::Known(T::deserialize(repr.value).map_err(D::Error::custom)?)
        };
        Ok(Self {
            fact,
            source: repr.source,
        })
    }
}

/// One value as the rules see it: its name in the file, whether it is
/// unknown, and its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry<'a> {
    pub key: &'static str,
    pub unknown: bool,
    pub source: &'a str,
}

/// A class's closed set of values: a struct that refuses a missing or an
/// unrecognised value, and the list of its entries in declaration order, from
/// one field list so the two cannot drift apart.
macro_rules! class_values {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$field_meta:meta])* $field:ident: $ty:ty, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            $( $(#[$field_meta])* pub $field: Sourced<$ty>, )+
        }

        impl $name {
            /// Every value, by its name in the file, in declaration order.
            #[must_use]
            pub fn entries(&self) -> Vec<Entry<'_>> {
                vec![$(Entry {
                    key: stringify!($field),
                    unknown: self.$field.fact.is_unknown(),
                    source: &self.$field.source,
                },)+]
            }
        }
    };
}

class_values! {
    /// A receipt printer: §6a's record and §6b.2's parameters.
    ReceiptPrinterValues {
        maker: String,
        model: String,
        firmware_qualified: String,
        transport: Transport,
        paper_mm: u32,
        dot_width: NonZeroU32,
        raster_command: RasterCommand,
        raster_max_rows: NonZeroU32,
        code_pages: Vec<String>,
        auto_cutter: bool,
        cut_command: CutCommand,
        cut_feed_rows: u32,
        status_protocol: StatusProtocol,
        drawer_port_volts: u32,
        drawer_pulse: DrawerPulse,
        supports_raster_arabic: bool,
    }
}

class_values! {
    /// A cash drawer: how it is kicked, what its solenoid takes, and what it
    /// reports back (§6b.2).
    CashDrawerValues {
        maker: String,
        model: String,
        kick_channel: KickChannel,
        pin: DrawerPin,
        pulse_on_ms: u16,
        pulse_off_ms: u16,
        /// The voltages the solenoid takes, which must include the port's.
        solenoid_volts: Bounds,
        /// Epson requires 24 Ω or more; no drawer maker publishes it, so it is
        /// measured (`docs/hardware-lab.md` §4.5).
        solenoid_ohms: u32,
        sensor: bool,
        sensor_polarity: SensorPolarity,
    }
}

class_values! {
    /// A barcode scanner: what it sends, how fast, and how it survives an
    /// Arabic keyboard layout (§5, §6b.2).
    ScannerValues {
        maker: String,
        model: String,
        interface: ScannerInterface,
        /// Keys sent before the code, as `KeyboardEvent.key` names.
        prefix: Vec<String>,
        /// Keys sent after it, as `KeyboardEvent.key` names.
        suffix: Vec<String>,
        /// The gap between two key presses of one scan, in milliseconds — what
        /// tells a scan from typing (§5's 30 ms).
        inter_key_ms: Bounds,
        /// The Arabic keyboard countries it can be set to — the device-side
        /// answer to the layout trap (§5).
        arabic_keyboard_countries: Vec<String>,
        /// Whether it can type a code as ALT or keypad sequences instead.
        alt_or_keypad_emulation: bool,
        /// Whether it reads 2D codes at all — the fiscal QR a receipt carries.
        reads_2d: bool,
        symbologies: Vec<String>,
    }
}

class_values! {
    /// The screen of a register: what 1.11.14b renders the sale screen at
    /// (§6b.2).
    RegisterScreenValues {
        maker: String,
        model: String,
        resolution: Resolution,
        /// Every display scale, in percent, the register may run at.
        display_scales_percent: Vec<u32>,
        pointer: Pointer,
        /// The panel's diagonal, in tenths of an inch: 150 for 15″.
        panel_tenths_inch: u32,
    }
}

/// How a drawer is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KickChannel {
    /// Through a receipt printer's drawer port.
    PrinterPort,
    /// Through an interface of its own.
    OwnInterface,
}

/// What a drawer's sensor reports for an open drawer in the status reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorPolarity {
    OpenIsHigh,
    OpenIsLow,
}

/// How a scanner reaches the register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScannerInterface {
    /// A keyboard wedge: key presses, which the active layout re-maps (§5).
    UsbHidKeyboard,
    /// A virtual serial port over USB.
    UsbCom,
    /// RS-232.
    Serial,
    /// USB HID POS: scans as data, never as key presses (§5, 1.11.6d).
    HidPos,
}

/// What the register's screen is touched or pointed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pointer {
    CapacitiveTouch,
    /// Needs a press rather than a tap (`docs/hardware-lab.md` §4.1).
    ResistiveTouch,
    Mouse,
}

/// An inclusive range, `min` no greater than `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "BoundsRepr")]
pub struct Bounds {
    pub min: u32,
    pub max: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundsRepr {
    min: u32,
    max: u32,
}

impl TryFrom<BoundsRepr> for Bounds {
    type Error = String;

    fn try_from(repr: BoundsRepr) -> Result<Self, Self::Error> {
        if repr.min <= repr.max {
            Ok(Self {
                min: repr.min,
                max: repr.max,
            })
        } else {
            Err(format!(
                "a range whose min {} is above its max {}",
                repr.min, repr.max
            ))
        }
    }
}

/// A screen's size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
}

/// The four classes 1.7.11 starts; every later class starts in the microstep
/// that builds its trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceClass {
    ReceiptPrinter,
    CashDrawer,
    Scanner,
    RegisterScreen,
}

impl DeviceClass {
    pub const ALL: [Self; 4] = [
        Self::ReceiptPrinter,
        Self::CashDrawer,
        Self::Scanner,
        Self::RegisterScreen,
    ];

    /// The class's directory under [`DIR`], which is also its name in a file.
    #[must_use]
    pub const fn dir(self) -> &'static str {
        match self {
            Self::ReceiptPrinter => "receipt_printer",
            Self::CashDrawer => "cash_drawer",
            Self::Scanner => "scanner",
            Self::RegisterScreen => "register_screen",
        }
    }
}

impl fmt::Display for DeviceClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.dir())
    }
}

/// Whether a profile describes one model, or a class a suite sweeps — the
/// seed's rows marked *(class)*, which say so in their sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Describes {
    Model,
    Class,
}

/// A profile's values, by class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassValues {
    ReceiptPrinter(ReceiptPrinterValues),
    CashDrawer(CashDrawerValues),
    Scanner(ScannerValues),
    RegisterScreen(RegisterScreenValues),
}

impl ClassValues {
    #[must_use]
    pub const fn class(&self) -> DeviceClass {
        match self {
            Self::ReceiptPrinter(_) => DeviceClass::ReceiptPrinter,
            Self::CashDrawer(_) => DeviceClass::CashDrawer,
            Self::Scanner(_) => DeviceClass::Scanner,
            Self::RegisterScreen(_) => DeviceClass::RegisterScreen,
        }
    }

    /// Every value, by its name in the file, in declaration order.
    #[must_use]
    pub fn entries(&self) -> Vec<Entry<'_>> {
        match self {
            Self::ReceiptPrinter(values) => values.entries(),
            Self::CashDrawer(values) => values.entries(),
            Self::Scanner(values) => values.entries(),
            Self::RegisterScreen(values) => values.entries(),
        }
    }
}

/// One version of one device profile, as its file states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceProfile {
    pub profile_id: String,
    pub version: NonZeroU32,
    pub describes: Describes,
    pub values: ClassValues,
}

impl DeviceProfile {
    #[must_use]
    pub const fn class(&self) -> DeviceClass {
        self.values.class()
    }

    /// Where this version lives, relative to [`DIR`].
    #[must_use]
    pub fn relative_path(&self) -> PathBuf {
        Path::new(self.class().dir()).join(file_name(&self.profile_id, self.version))
    }

    /// The names of every value this version leaves unknown, in declaration
    /// order.
    #[must_use]
    pub fn unknown_values(&self) -> Vec<&'static str> {
        self.values
            .entries()
            .into_iter()
            .filter(|entry| entry.unknown)
            .map(|entry| entry.key)
            .collect()
    }

    /// The record a driver works from, when this is a receipt printer.
    #[must_use]
    pub fn printer_profile(&self) -> Option<PrinterProfile> {
        match &self.values {
            ClassValues::ReceiptPrinter(values) => {
                Some(values.printer_profile(&self.profile_id, self.version))
            }
            _ => None,
        }
    }
}

impl ReceiptPrinterValues {
    /// These values without their sources, as the record a driver works from.
    /// A file states no qualification, so the record is a candidate's.
    #[must_use]
    pub fn printer_profile(&self, profile_id: &str, version: NonZeroU32) -> PrinterProfile {
        PrinterProfile {
            profile_id: profile_id.to_owned(),
            version,
            maker: self.maker.fact.clone(),
            model: self.model.fact.clone(),
            firmware_qualified: self.firmware_qualified.fact.clone(),
            transport: self.transport.fact,
            paper_mm: self.paper_mm.fact,
            dot_width: self.dot_width.fact,
            raster_command: self.raster_command.fact,
            raster_max_rows: self.raster_max_rows.fact,
            code_pages: self.code_pages.fact.clone(),
            auto_cutter: self.auto_cutter.fact,
            cut_command: self.cut_command.fact,
            cut_feed_rows: self.cut_feed_rows.fact,
            status_protocol: self.status_protocol.fact,
            drawer_port_volts: self.drawer_port_volts.fact,
            drawer_pulse: self.drawer_pulse.fact,
            supports_raster_arabic: self.supports_raster_arabic.fact,
            qualification: Qualification::NotQualified,
        }
    }
}

/// Why a profile file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ProfileFileError {
    #[error("{}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("schema {found} is not schema {SCHEMA}, the one this build reads")]
    Schema { found: u32 },
    #[error("profile id {profile_id:?} is not lower-case letters and digits joined by hyphens")]
    Id { profile_id: String },
    #[error("this file belongs at {expected}, under {DIR}")]
    Misplaced { expected: String },
    #[error("{}: {source}", path.display())]
    At {
        path: PathBuf,
        source: Box<ProfileFileError>,
    },
}

/// The file name a version of a profile lives under.
#[must_use]
pub fn file_name(profile_id: &str, version: NonZeroU32) -> String {
    format!("{profile_id}.v{version}.json")
}

/// The fields every file carries, read first to learn the class.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    schema: u32,
    class: DeviceClass,
    profile_id: String,
    version: NonZeroU32,
    describes: Describes,
    #[serde(rename = "values")]
    _values: serde::de::IgnoredAny,
}

/// The whole file, read a second time with the class's own values, so an
/// error in one points at its line in the file.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Typed<V> {
    #[serde(rename = "schema")]
    _schema: serde::de::IgnoredAny,
    #[serde(rename = "class")]
    _class: serde::de::IgnoredAny,
    #[serde(rename = "profile_id")]
    _profile_id: serde::de::IgnoredAny,
    #[serde(rename = "version")]
    _version: serde::de::IgnoredAny,
    #[serde(rename = "describes")]
    _describes: serde::de::IgnoredAny,
    values: V,
}

fn values_of<V: DeserializeOwned>(text: &str) -> Result<V, ProfileFileError> {
    Ok(serde_json::from_str::<Typed<V>>(text)?.values)
}

fn is_profile_id(id: &str) -> bool {
    !id.is_empty()
        && id.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// Read one profile from its text.
///
/// # Errors
///
/// A file that is not JSON, names another schema, has a malformed id, or
/// whose values are not exactly its class's — each with a source, and each
/// either a value or `unknown`.
pub fn parse(text: &str) -> Result<DeviceProfile, ProfileFileError> {
    let header: Header = serde_json::from_str(text)?;
    if header.schema != SCHEMA {
        return Err(ProfileFileError::Schema {
            found: header.schema,
        });
    }
    if !is_profile_id(&header.profile_id) {
        return Err(ProfileFileError::Id {
            profile_id: header.profile_id,
        });
    }
    let values = match header.class {
        DeviceClass::ReceiptPrinter => ClassValues::ReceiptPrinter(values_of(text)?),
        DeviceClass::CashDrawer => ClassValues::CashDrawer(values_of(text)?),
        DeviceClass::Scanner => ClassValues::Scanner(values_of(text)?),
        DeviceClass::RegisterScreen => ClassValues::RegisterScreen(values_of(text)?),
    };
    Ok(DeviceProfile {
        profile_id: header.profile_id,
        version: header.version,
        describes: header.describes,
        values,
    })
}

/// Read one profile from its file, which must sit where its class, id and
/// version say: `<class>/<profile_id>.v<version>.json`.
///
/// # Errors
///
/// [`parse`]'s, and a file that cannot be read or sits anywhere else, each
/// naming the file.
pub fn load(path: &Path) -> Result<DeviceProfile, ProfileFileError> {
    let at = |source: ProfileFileError| ProfileFileError::At {
        path: path.to_path_buf(),
        source: Box::new(source),
    };
    let text = fs::read_to_string(path).map_err(|source| ProfileFileError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let profile = parse(&text).map_err(at)?;
    let expected = profile.relative_path();
    if !path.ends_with(&expected) {
        return Err(at(ProfileFileError::Misplaced {
            expected: expected.display().to_string(),
        }));
    }
    Ok(profile)
}

/// Every profile under `root`, one directory per class, in path order.
///
/// # Errors
///
/// The first file that does not load, as [`load`] reports it — including any
/// file in a class's directory that is not a profile.
pub fn load_dir(root: &Path) -> Result<Vec<(PathBuf, DeviceProfile)>, ProfileFileError> {
    let mut profiles = Vec::new();
    for class in DeviceClass::ALL {
        let dir = root.join(class.dir());
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => return Err(ProfileFileError::Read { path: dir, source }),
        };
        for entry in entries {
            let path = entry
                .map_err(|source| ProfileFileError::Read {
                    path: dir.clone(),
                    source,
                })?
                .path();
            let profile = load(&path)?;
            profiles.push((path, profile));
        }
    }
    profiles.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(profiles)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use serde_json::{Value, json};

    use super::*;

    fn sourced(value: Value) -> Value {
        json!({ "value": value, "source": "a document, §1" })
    }

    /// A complete receipt-printer file, every value present.
    fn printer() -> Value {
        json!({
            "schema": 1,
            "class": "receipt_printer",
            "profile_id": "maker-model-80mm",
            "version": 1,
            "describes": "model",
            "values": {
                "maker": sourced(json!("Maker")),
                "model": sourced(json!("Model")),
                "firmware_qualified": sourced(json!(UNKNOWN)),
                "transport": sourced(json!("tcp_9100")),
                "paper_mm": sourced(json!(80)),
                "dot_width": sourced(json!(576)),
                "raster_command": sourced(json!("gs_v_0")),
                "raster_max_rows": sourced(json!(2303)),
                "code_pages": sourced(json!(UNKNOWN)),
                "auto_cutter": sourced(json!(true)),
                "cut_command": sourced(json!("partial")),
                "cut_feed_rows": sourced(json!(UNKNOWN)),
                "status_protocol": sourced(json!("dle_eot")),
                "drawer_port_volts": sourced(json!(24)),
                "drawer_pulse": sourced(json!({ "pin": "pin_2", "on_ms": 50, "off_ms": 500 })),
                "supports_raster_arabic": sourced(json!(UNKNOWN)),
            }
        })
    }

    fn parse_value(value: &Value) -> Result<DeviceProfile, ProfileFileError> {
        parse(&value.to_string())
    }

    fn set(file: &mut Value, key: &str, value: Value) {
        file.as_object_mut()
            .expect("a file is an object")
            .insert(key.to_owned(), value);
    }

    fn values_mut(file: &mut Value) -> &mut serde_json::Map<String, Value> {
        file.get_mut("values")
            .and_then(Value::as_object_mut)
            .expect("values")
    }

    #[test]
    fn unknown_is_a_value_and_a_known_one_keeps_its_type() {
        let profile = parse_value(&printer()).expect("loads");
        assert_eq!(profile.class(), DeviceClass::ReceiptPrinter);
        assert_eq!(
            profile.unknown_values(),
            vec![
                "firmware_qualified",
                "code_pages",
                "cut_feed_rows",
                "supports_raster_arabic"
            ]
        );
        let record = profile.printer_profile().expect("a printer");
        assert_eq!(record.dot_width, Fact::Known(NonZeroU32::new(576).unwrap()));
        assert_eq!(record.cut_feed_rows, Fact::Unknown);
        assert_eq!(record.qualification, Qualification::NotQualified);
        assert_eq!(
            profile.relative_path(),
            Path::new("receipt_printer/maker-model-80mm.v1.json")
        );
    }

    #[test]
    fn an_omitted_value_does_not_load() {
        let mut file = printer();
        values_mut(&mut file).remove("cut_feed_rows");
        let error = parse_value(&file).unwrap_err().to_string();
        assert!(error.contains("missing field `cut_feed_rows`"), "{error}");
    }

    #[test]
    fn a_value_its_class_does_not_have_does_not_load() {
        let mut file = printer();
        values_mut(&mut file).insert("colour".to_owned(), sourced(json!("black")));
        let error = parse_value(&file).unwrap_err().to_string();
        assert!(error.contains("unknown field `colour`"), "{error}");
    }

    #[test]
    fn a_value_without_a_source_does_not_load() {
        for broken in [
            json!({ "value": 576 }),
            json!({ "value": 576, "source": "" }),
            json!({ "value": 576, "source": "   " }),
        ] {
            let mut file = printer();
            values_mut(&mut file).insert("dot_width".to_owned(), broken.clone());
            assert!(parse_value(&file).is_err(), "{broken} loaded");
        }
    }

    #[test]
    fn a_profile_that_prints_no_dots_does_not_load() {
        let mut file = printer();
        values_mut(&mut file).insert("dot_width".to_owned(), sourced(json!(0)));
        let error = parse_value(&file).unwrap_err().to_string();
        assert!(error.contains("nonzero"), "{error}");
    }

    #[test]
    fn a_range_upside_down_does_not_load() {
        assert!(serde_json::from_value::<Bounds>(json!({ "min": 12, "max": 24 })).is_ok());
        let error = serde_json::from_value::<Bounds>(json!({ "min": 24, "max": 12 }))
            .unwrap_err()
            .to_string();
        assert!(error.contains("above its max"), "{error}");
    }

    #[test]
    fn another_schema_or_a_malformed_id_does_not_load() {
        let mut file = printer();
        set(&mut file, "schema", json!(2));
        assert!(matches!(
            parse_value(&file),
            Err(ProfileFileError::Schema { found: 2 })
        ));

        for id in ["", "Epson-TM", "epson_tm", "-epson", "epson--tm", "epson-"] {
            let mut file = printer();
            set(&mut file, "profile_id", json!(id));
            assert!(
                matches!(parse_value(&file), Err(ProfileFileError::Id { .. })),
                "{id:?} was accepted"
            );
        }
    }

    #[test]
    fn a_file_stating_another_class_is_read_as_that_class() {
        let mut file = printer();
        set(&mut file, "class", json!("scanner"));
        let error = parse_value(&file).unwrap_err().to_string();
        assert!(error.contains("unknown field"), "{error}");
    }
}
