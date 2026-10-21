//! The emulated network printer (microstep 1.7.12), driven by 1.7.10's
//! driver over a real socket and by the seeded device profiles of 1.7.11.

#![cfg(feature = "emulator")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use pos_hardware::emulator::{ConnectionFault, DrawerModel, Emulator, EmulatorConfig, Event, png};
use pos_hardware::escpos::encode;
use pos_hardware::net::{Deadlines, NetworkPrinter};
use pos_hardware::profile::data::{self, Bounds, ClassValues, DeviceProfile, SensorPolarity};
use pos_hardware::profile::{
    DrawerPin, DrawerPulse, Fact, PrinterProfile, StatusProtocol, Transport as ProfileTransport,
};
use pos_hardware::render::{Bitmap, PaperWidth, render_receipt};
use pos_hardware::transport::TcpTransport;
use pos_hardware::{FailReason, PrintOutcome, PrinterStatus, ReceiptPrinter, RenderedReceipt};

use pos_domain::catalog::UnitOfMeasure;
use pos_domain::ids::{RegisterId, SaleId};
use pos_domain::money::{Currency, Money, Percent, Qty};
use pos_domain::receipt::{
    BuyerBlock, DocKind, FiscalBlock, FooterBlock, Language, LoyaltyBlock, MerchantBlock,
    ReceiptDiscount, ReceiptHeader, ReceiptLine, ReceiptLocale, ReceiptModel, ReceiptTender,
    ReceiptTotals,
};
use pos_domain::tax::{TaxSummaryRow, TaxTreatment};
use pos_domain::time::{BusinessDate, Timestamp};
use uuid::Uuid;

// The synthetic receipt the crate's own tests render (`render::tests_support`),
// built through the public API: an integration test cannot see the fixture.
fn jod(minor: i64) -> Money {
    Money::from_minor(minor, Currency::JOD)
}

fn line(name: &str, qty_milli: i64, unit_minor: i64) -> ReceiptLine {
    ReceiptLine {
        name: name.to_owned(),
        qty: Qty::from_milli(qty_milli),
        unit: UnitOfMeasure::Each,
        is_weighed: false,
        unit_price: jod(unit_minor),
        line_total: jod(unit_minor * qty_milli / 1000),
        discounts: Vec::new(),
    }
}

/// The same document the crate's own tests render (`render::tests_support::model`).
fn receipt() -> ReceiptModel {
    let mut first = line("خبز عربي طازج", 2_000, 400);
    first.discounts.push(ReceiptDiscount {
        label: "عرض الخبز".to_owned(),
        amount: jod(50),
    });
    ReceiptModel {
        doc_kind: DocKind::Sale,
        watermark: None,
        merchant: MerchantBlock {
            legal_name: "مؤسسة النور التجارية".to_owned(),
            store_name: "النور".to_owned(),
            address: Some("عمان، الأردن".to_owned()),
            phone: Some("+962 79 000 0000".to_owned()),
            tin: Some("123456789".to_owned()),
        },
        buyer: Some(BuyerBlock {
            name: Some("شركة الأمانة للتجارة".to_owned()),
            tin: "987654321".to_owned(),
        }),
        header: ReceiptHeader {
            sale_id: SaleId::from_uuid(Uuid::from_u128(1)),
            receipt_no: "REG01-000042".to_owned(),
            register_id: RegisterId::from_uuid(Uuid::from_u128(2)),
            register_code: "REG01".to_owned(),
            cashier_name: "ليلى".to_owned(),
            issued_at: Timestamp::from_epoch_milliseconds(1_758_000_000_000).expect("instant"),
            business_date: BusinessDate::new(2026, 9, 22).expect("date"),
        },
        lines: vec![first],
        totals: ReceiptTotals {
            subtotal: jod(2_166),
            discount_total: jod(50),
            tax_summary: vec![TaxSummaryRow {
                code: "GST16".to_owned(),
                treatment: TaxTreatment::Standard,
                rate: Percent::from_ppm(160_000),
                per_unit: None,
                reason: None,
                net: jod(1_867),
                tax: jod(299),
                gross: jod(2_166),
            }],
            rounding_adjustment: Some(jod(-3)),
            total: jod(2_163),
        },
        tenders: vec![ReceiptTender {
            label: "نقدًا".to_owned(),
            amount: jod(2_163),
            masked_pan: None,
            scheme: None,
        }],
        change: Some(jod(0)),
        loyalty: Some(LoyaltyBlock {
            balance_points: 120,
        }),
        fiscal: Some(FiscalBlock {
            uuid: "9f1c0f7a-0000-4000-8000-000000000000".to_owned(),
            qr_payload: "AQIDBAU=".to_owned(),
        }),
        footer: FooterBlock {
            return_policy: Some("الإرجاع خلال أربعة عشر يومًا".to_owned()),
            thank_you: Some("شكرًا لتسوقكم".to_owned()),
        },
        locale: ReceiptLocale::new(Language::Arabic),
    }
}

fn profiles() -> Vec<(PathBuf, DeviceProfile)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(data::DIR);
    data::load_dir(&root).expect("the seeded profiles load")
}

fn printer_file(id: &str) -> PrinterProfile {
    profiles()
        .into_iter()
        .find(|(_, profile)| profile.profile_id == id)
        .and_then(|(_, profile)| profile.printer_profile())
        .unwrap_or_else(|| panic!("no printer profile {id}"))
}

fn drawer_file(id: &str) -> DrawerModel {
    let (_, profile) = profiles()
        .into_iter()
        .find(|(_, profile)| profile.profile_id == id)
        .unwrap_or_else(|| panic!("no drawer profile {id}"));
    let ClassValues::CashDrawer(values) = &profile.values else {
        panic!("{id} is not a drawer")
    };
    DrawerModel::from_profile(id, values)
}

/// The simulator's profile, on the network: what a test needs to be known.
fn network(paper: PaperWidth) -> PrinterProfile {
    let mut profile = PrinterProfile::simulator(paper);
    profile.transport = Fact::Known(ProfileTransport::Tcp9100);
    profile.drawer_port_volts = Fact::Known(24);
    profile
}

fn start(
    printer: PrinterProfile,
    drawer: Option<DrawerModel>,
    out_dir: Option<PathBuf>,
) -> Emulator {
    Emulator::start(EmulatorConfig {
        printer,
        drawer,
        listen: "127.0.0.1:0".parse().unwrap(),
        out_dir,
    })
    .expect("the emulator starts")
}

fn driver(address: SocketAddr, profile: PrinterProfile) -> NetworkPrinter {
    NetworkPrinter::new(
        TcpTransport::new(address),
        profile,
        Deadlines {
            connect: Duration::from_secs(2),
            write: Duration::from_secs(10),
            status: Duration::from_millis(500),
        },
    )
    .expect("a network profile")
}

fn settle(emulator: &Emulator, connections: usize) {
    assert!(
        emulator.settled(connections, Duration::from_secs(5)),
        "the emulator did not close {connections} connections: {:?}",
        emulator.events()
    );
}

fn unknowns(emulator: &Emulator) -> Vec<String> {
    emulator
        .events()
        .into_iter()
        .filter_map(|event| match event {
            Event::UnknownBehaviour { what } => Some(what),
            _ => None,
        })
        .collect()
}

/// Until 1.7.5 commits the goldens, the receipt they will be rendered from:
/// sent through the driver at both widths, it comes back dot for dot, and
/// the PNG the emulator writes reads back as the same page.
#[test]
fn emulated_printer_renders_what_the_driver_sends_dot_for_dot() {
    let out = std::env::temp_dir().join(format!("pos-emulator-{}", std::process::id()));
    for paper in PaperWidth::ALL {
        let profile = network(paper);
        let emulator = start(profile.clone(), None, Some(out.join(format!("{paper:?}"))));
        let page = render_receipt(&receipt(), paper).expect("renders");
        let doc = RenderedReceipt {
            bytes: encode(&page, &profile).expect("encodes"),
        };
        assert_eq!(
            driver(emulator.address(), profile).print(&doc),
            PrintOutcome::Printed
        );
        settle(&emulator, 1);

        let jobs = emulator.jobs();
        let [job] = jobs.as_slice() else {
            panic!("one job, got {}", jobs.len())
        };
        assert!(job.cut, "{paper:?}: the cut ended it");
        assert_eq!(job.page, page, "{paper:?}: dot for dot");
        assert_eq!(job.bytes, doc.bytes, "{paper:?}: the bytes it received");
        let png_path = job.png.as_ref().expect("a PNG beside the bytes");
        assert_eq!(png::read(png_path).expect("reads back"), page, "{paper:?}");
        assert!(png_path.with_extension("bin").exists());
    }
    let _ = std::fs::remove_dir_all(out);
}

/// The named test: status follows the profile. A device that answers
/// `DLE EOT` reports each fault it is scripted with; a profile that has no
/// status is never asked; and where the profile does not know, the emulator
/// answers nothing and says why rather than inventing a reply.
#[test]
fn emulated_printer_answers_status_as_its_profile_declares() {
    // The TM-T20III's file says DLE EOT over the socket.
    let tm_t20iii = printer_file("epson-tm-t20iii-ethernet-80mm");
    assert_eq!(
        tm_t20iii.status_protocol,
        Fact::Known(StatusProtocol::DleEot)
    );
    let emulator = start(tm_t20iii.clone(), None, None);
    let printer = driver(emulator.address(), tm_t20iii);
    assert_eq!(printer.status(), PrinterStatus::Ready);
    for (fault, status) in [
        (ConnectionFault::PaperOut, PrinterStatus::PaperOut),
        (ConnectionFault::CoverOpen, PrinterStatus::CoverOpen),
        (ConnectionFault::Offline, PrinterStatus::Offline),
    ] {
        emulator.script(fault);
        assert_eq!(printer.status(), status, "{fault:?}");
    }
    // Late: replies slower than the driver waits read as no status at all.
    emulator.script(ConnectionFault::Slow(Duration::from_millis(900)));
    assert_eq!(
        printer.status(),
        PrinterStatus::Unsupported,
        "too late to count"
    );
    settle(&emulator, 5);

    // The Xprinter's file says none over Ethernet: the driver never asks.
    let xprinter = printer_file("xprinter-xp-n160ii-ethernet-80mm");
    let emulator = start(xprinter.clone(), None, None);
    assert_eq!(
        driver(emulator.address(), xprinter).status(),
        PrinterStatus::Unsupported
    );
    assert!(
        emulator.events().is_empty(),
        "never asked: {:?}",
        emulator.events()
    );

    // The Bixolon's file does not know: asked, it answers nothing, and says why.
    let bixolon = printer_file("bixolon-srp-332iii-80mm");
    assert_eq!(bixolon.status_protocol, Fact::Unknown);
    let emulator = start(bixolon.clone(), None, None);
    assert_eq!(
        driver(emulator.address(), bixolon).status(),
        PrinterStatus::Unsupported
    );
    settle(&emulator, 1);
    assert_eq!(unknowns(&emulator), vec!["status_protocol".to_owned()]);
}

/// `GS v 0` of `rows` rows at the TM-T20III's 576 dots: one raw command, as
/// a driver that ignored the limit would send it.
fn raw_raster(rows: u32) -> Vec<u8> {
    let mut stream = vec![0x1B, 0x40, 0x1D, 0x76, 0x30, 0x00, 72, 0];
    stream.extend_from_slice(&u16::try_from(rows).unwrap().to_le_bytes());
    stream.extend(std::iter::repeat_n(0xAA, 72 * rows as usize));
    stream.extend_from_slice(&[0x1D, 0x56, 0x01]);
    stream
}

/// The named test: a raster taller than the profile's limit is refused, one at
/// the limit is printed, and against a limit nobody measured the emulator
/// prints nothing and says why.
#[test]
fn emulated_printer_refuses_a_raster_taller_than_its_profile_allows() {
    let mut quiet = printer_file("epson-tm-t20iii-ethernet-80mm");
    // Skip the pre-flight so the stream is exactly what is sent.
    quiet.status_protocol = Fact::Known(StatusProtocol::None);
    let emulator = start(quiet.clone(), None, None);
    let printer = driver(emulator.address(), quiet.clone());

    let send = |rows| {
        printer.print(&RenderedReceipt {
            bytes: raw_raster(rows),
        })
    };
    assert_eq!(
        send(2304),
        PrintOutcome::Printed,
        "the socket took it; the device did not"
    );
    assert_eq!(send(2303), PrintOutcome::Printed);
    settle(&emulator, 2);
    let refused: Vec<Event> = emulator
        .events()
        .into_iter()
        .filter(|event| matches!(event, Event::RasterTooTall { .. }))
        .collect();
    assert_eq!(
        refused,
        vec![Event::RasterTooTall {
            command: pos_hardware::profile::RasterCommand::GsV0,
            rows: 2304,
            limit: 2303
        }]
    );
    let jobs = emulator.jobs();
    let [job] = jobs.as_slice() else {
        panic!("only the page at the limit printed, got {}", jobs.len())
    };
    assert_eq!(job.page.height_px, 2303);

    // A page taller than the limit, sent through the driver in strips at the
    // file's limit, prints whole.
    let tall = Bitmap {
        width_px: 576,
        height_px: 5000,
        bits: vec![0x0F; 72 * 5000],
    };
    let strips = RenderedReceipt {
        bytes: encode(&tall, &quiet).expect("encodes"),
    };
    let emulator = start(quiet.clone(), None, None);
    assert_eq!(
        driver(emulator.address(), quiet).print(&strips),
        PrintOutcome::Printed
    );
    settle(&emulator, 1);
    assert_eq!(
        emulator.jobs().first().map(|job| job.page.clone()),
        Some(tall)
    );

    // The Bixolon's file does not know its limit: nothing prints.
    let mut bixolon = printer_file("bixolon-srp-332iii-80mm");
    bixolon.status_protocol = Fact::Known(StatusProtocol::None);
    let emulator = start(bixolon.clone(), None, None);
    let _ = driver(emulator.address(), bixolon).print(&RenderedReceipt {
        bytes: raw_raster(10),
    });
    settle(&emulator, 1);
    assert!(emulator.jobs().is_empty());
    // Its cut command is unknown too, and the stream ends with one.
    assert_eq!(
        unknowns(&emulator),
        vec!["raster_command".to_owned(), "cut_command".to_owned()]
    );
}

/// The named test: each `ESC p` is recorded with the pin and timings it
/// carried; the drawer opens only when the drawer profile's pin and voltage
/// agree; its sensor reads at the drawer profile's polarity; and against a
/// seeded drawer whose pin nobody measured, the emulator opens nothing and
/// says why.
#[test]
fn emulated_printer_records_each_drawer_pulse_with_its_pin_and_timings() {
    let mut printer_profile = network(PaperWidth::Mm80);
    printer_profile.drawer_pulse = Fact::Known(DrawerPulse {
        pin: DrawerPin::Pin2,
        on_ms: 60,
        off_ms: 240,
    });
    let drawer = DrawerModel {
        profile_id: "test-drawer".to_owned(),
        pin: Fact::Known(DrawerPin::Pin2),
        solenoid_volts: Fact::Known(Bounds { min: 12, max: 24 }),
        sensor: Fact::Known(true),
        polarity: Fact::Known(SensorPolarity::OpenIsHigh),
    };
    let emulator = start(printer_profile.clone(), Some(drawer.clone()), None);
    let printer = driver(emulator.address(), printer_profile.clone());

    assert!(!emulator.drawer_open());
    printer.open_drawer().expect("pulses");
    settle(&emulator, 1);
    assert!(emulator.drawer_open());
    assert!(emulator.events().contains(&Event::DrawerPulse {
        pin: DrawerPin::Pin2,
        on_ms: 60,
        off_ms: 240,
        opened: Fact::Known(true),
    }));
    // The sensor, read back through DLE EOT 1 at the profile's polarity.
    assert!(sensor_high(&emulator), "open reads high");
    emulator.close_drawer();
    assert!(!sensor_high(&emulator), "closed reads low");

    // The other pin does not open it.
    let mut wrong_pin = printer_profile.clone();
    wrong_pin.drawer_pulse = Fact::Known(DrawerPulse {
        pin: DrawerPin::Pin5,
        on_ms: 60,
        off_ms: 240,
    });
    let emulator = start(wrong_pin.clone(), Some(drawer), None);
    driver(emulator.address(), wrong_pin)
        .open_drawer()
        .expect("pulses");
    settle(&emulator, 1);
    assert!(!emulator.drawer_open());

    // The seeded Posiflex drawer: nobody measured its pin.
    let emulator = start(
        printer_profile.clone(),
        Some(drawer_file("posiflex-cr-4000-g2")),
        None,
    );
    driver(emulator.address(), printer_profile)
        .open_drawer()
        .expect("pulses");
    settle(&emulator, 1);
    assert!(!emulator.drawer_open());
    assert_eq!(unknowns(&emulator), vec!["the drawer's pin".to_owned()]);
}

/// Ask `DLE EOT 1` directly and read the drawer bit.
fn sensor_high(emulator: &Emulator) -> bool {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(emulator.address()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(&[0x10, 0x04, 0x01]).unwrap();
    let mut reply = [0_u8; 1];
    stream.read_exact(&mut reply).unwrap();
    let [byte] = reply;
    byte & 0b0000_0100 != 0
}

/// 1.7.8's in-process sweep, with a twin on a real socket: each fault the
/// emulator is scripted with reaches the driver as its one outcome.
#[test]
fn each_scripted_fault_reaches_the_driver_as_its_outcome() {
    let profile = network(PaperWidth::Mm80);
    let emulator = start(profile.clone(), None, None);
    let printer = driver(emulator.address(), profile.clone());
    let page = render_receipt(&receipt(), PaperWidth::Mm80).expect("renders");
    let doc = RenderedReceipt {
        bytes: encode(&page, &profile).expect("encodes"),
    };
    for (fault, outcome) in [
        (
            ConnectionFault::PaperOut,
            PrintOutcome::Failed(FailReason::PaperOut),
        ),
        (
            ConnectionFault::CoverOpen,
            PrintOutcome::Failed(FailReason::CoverOpen),
        ),
        (
            ConnectionFault::Offline,
            PrintOutcome::Failed(FailReason::Offline),
        ),
    ] {
        emulator.script(fault);
        assert_eq!(printer.print(&doc), outcome, "{fault:?}");
    }
    assert!(emulator.jobs().is_empty(), "nothing printed past a refusal");

    // Silent after a few bytes, against a document larger than any socket
    // buffer: the driver's deadline passes, and the outcome is Unknown.
    let large = RenderedReceipt {
        bytes: vec![0x1B; 32 * 1024 * 1024],
    };
    let impatient = NetworkPrinter::new(
        TcpTransport::new(emulator.address()),
        profile,
        Deadlines {
            connect: Duration::from_secs(2),
            write: Duration::from_millis(700),
            status: Duration::from_millis(500),
        },
    )
    .unwrap();
    emulator.script(ConnectionFault::SilentAfter(4096));
    assert_eq!(impatient.print(&large), PrintOutcome::Unknown);
}

/// The named test: nothing a register ships enables the emulator. Cargo's own
/// resolution of the terminal's dependency graph, with the features it turns
/// on, must not contain `pos-hardware`'s `emulator` feature.
#[test]
fn the_register_bundle_never_enables_the_emulator() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .expect("the workspace");
    let output = Command::new(env!("CARGO"))
        .current_dir(workspace)
        .args([
            "tree",
            "--locked",
            "--offline",
            "-p",
            "terminal",
            "-e",
            "features",
            "--prefix",
            "none",
        ])
        .output()
        .expect("cargo runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8_lossy(&output.stdout);
    assert!(
        tree.lines().any(|line| line.starts_with("terminal v")),
        "the terminal's own graph"
    );
    assert!(
        !tree.contains("feature \"emulator\""),
        "the register's graph enables the emulator:\n{tree}"
    );
}
