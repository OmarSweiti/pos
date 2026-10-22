//! The second opinion (`ref/hardware-and-receipts.md` §6b.8): the suite's
//! byte streams replayed through emupos, an emulator nobody here wrote,
//! pinned in `infra/emulators/emupos/` and run by CI's cross-check job. Our
//! emulator and our encoder read the same specification, so one misreading
//! passes both; a reader that is not ours is how that is caught.
//!
//! Where emupos models nothing — row limits, width agreement — 1.7.12 stays
//! the only judge, so the over-limit raster and the silent device are not
//! replayed here. A disagreement fails the build and is investigated, never
//! suppressed.

use std::io::{Cursor, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use pos_hardware::emulator::ConnectionFault;
use pos_hardware::escpos::encode;
use pos_hardware::profile::{Fact, PrinterProfile, RasterCommand};
use pos_hardware::render::{Bitmap, PaperWidth, render_receipt};
use pos_hardware::{PrintOutcome, ReceiptPrinter, RenderedReceipt};

use crate::printer_profiles;
use crate::receipt::receipt;
use crate::target::{EmulatorTarget, driver};

/// Where CI's cross-check job publishes the pinned emupos: its printer port
/// and its control API, both on the runner's loopback.
const PRINTER: &str = "EMUPOS_PRINTER";
const API: &str = "EMUPOS_API";
/// The device emupos runs, as `infra/emulators/emupos/emupos.yaml` names it.
const DEVICE: &str = "front";
/// The profile emupos's own `epson-tm-t20iii` models.
const PROFILE: &str = "epson-tm-t20iii-ethernet-80mm";

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!(
            "{name} is not set: this test runs against the pinned emupos, from CI's cross-check job"
        )
    })
}

/// One HTTP/1.1 request to emupos's control API, and its status and body.
fn http(api: &str, method: &str, path: &str) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(api).unwrap_or_else(|e| panic!("emupos at {api}: {e}"));
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("a timeout");
    write!(
        stream,
        "{method} /api/v1/devices/{DEVICE}{path} HTTP/1.1\r\nHost: {api}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    )
    .expect("sends");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("reads");
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("an HTTP response");
    let head = String::from_utf8_lossy(raw.get(..split).unwrap_or_default()).to_ascii_lowercase();
    let body = raw.get(split + 4..).unwrap_or_default().to_vec();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("a status code");
    let body = if head.contains("transfer-encoding: chunked") {
        dechunk(&body)
    } else {
        body
    };
    (status, body)
}

fn dechunk(mut body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let end = body
            .windows(2)
            .position(|w| w == b"\r\n")
            .expect("a chunk size");
        let size = usize::from_str_radix(
            String::from_utf8_lossy(body.get(..end).unwrap_or_default()).trim(),
            16,
        )
        .expect("a hex chunk size");
        if size == 0 {
            return out;
        }
        out.extend_from_slice(body.get(end + 2..end + 2 + size).expect("a whole chunk"));
        body = body.get(end + 2 + size + 2..).unwrap_or_default();
    }
}

fn receipts(api: &str) -> Vec<serde_json::Value> {
    let (status, body) = http(api, "GET", "/receipts");
    assert_eq!(status, 200, "listing emupos's receipts");
    serde_json::from_slice(&body).expect("a JSON list")
}

/// emupos's render of the document it completed after the `before`th, as
/// black dots.
fn printed(api: &str, before: usize) -> (u32, u32, Vec<bool>) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let id = loop {
        let now = receipts(api);
        if now.len() > before {
            break now
                .first()
                .and_then(|receipt| receipt.get("id"))
                .and_then(serde_json::Value::as_str)
                .expect("a receipt id")
                .to_owned();
        }
        assert!(Instant::now() < deadline, "emupos completed no receipt");
        std::thread::sleep(Duration::from_millis(200));
    };
    let (status, png) = http(api, "GET", &format!("/receipts/{id}/image"));
    assert_eq!(status, 200, "receipt {id}'s image");
    let mut decoder = png::Decoder::new(Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().expect("a PNG");
    let mut buffer = vec![0; reader.output_buffer_size().expect("a page size")];
    let frame = reader.next_frame(&mut buffer).expect("a frame");
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => panic!("emupos returned a {other:?} PNG"),
    };
    let black = buffer
        .chunks_exact(channels)
        .take(frame.width as usize * frame.height as usize)
        .map(|pixel| {
            let luma = match pixel {
                [g, ..] if channels < 3 => u32::from(*g),
                [r, g, b, ..] => {
                    (u32::from(*r) * 299 + u32::from(*g) * 587 + u32::from(*b) * 114) / 1000
                }
                _ => 255,
            };
            luma < 128
        })
        .collect();
    (frame.width, frame.height, black)
}

fn dots(page: &Bitmap) -> Vec<bool> {
    let row_bytes = page.row_bytes() as usize;
    (0..page.height_px as usize)
        .flat_map(|y| (0..page.width_px as usize).map(move |x| (y, x)))
        .map(|(y, x)| {
            page.bits
                .get(y * row_bytes + x / 8)
                .is_some_and(|byte| byte & (0x80 >> (x % 8)) != 0)
        })
        .collect()
}

/// `DLE EOT` 1 to 4 on one connection.
fn replies(address: SocketAddr) -> Vec<Option<u8>> {
    let mut stream = TcpStream::connect(address).expect("connects");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("a timeout");
    (1..=4)
        .map(|function| {
            stream.write_all(&[0x10, 0x04, function]).expect("asks");
            let mut byte = [0_u8; 1];
            match stream.read(&mut byte) {
                Ok(1) => byte.first().copied(),
                _ => None,
            }
        })
        .collect()
}

/// Every conformance byte stream emupos can judge renders dot-identical
/// through it, and its status bytes agree with ours in every state.
#[test]
#[ignore = "needs the pinned emupos: run by CI's cross-check job (EMUPOS_PRINTER, EMUPOS_API)"]
fn every_conformance_stream_renders_dot_identical_through_the_independent_emulator() {
    let emupos: SocketAddr = required(PRINTER)
        .parse()
        .expect("EMUPOS_PRINTER is host:port");
    let api = required(API);
    let profile = printer_profiles()
        .into_iter()
        .find(|profile| profile.profile_id == PROFILE)
        .expect("the TM-T20III's file");

    // The pages the suite prints, in each framing 1.7.9 produces.
    let page = render_receipt(&receipt(), PaperWidth::Mm80).expect("renders");
    let limit = match profile.raster_max_rows {
        Fact::Known(limit) => limit.get(),
        Fact::Unknown => panic!("{PROFILE}'s limit is known"),
    };
    let tall = Bitmap {
        width_px: page.width_px,
        height_px: limit * 2 + 1,
        bits: (0..(limit * 2 + 1) * page.row_bytes())
            .map(|i| if i % 3 == 0 { 0xA5 } else { 0x0F })
            .collect(),
    };
    let mut paren_l: PrinterProfile = profile.clone();
    paren_l.raster_command = Fact::Known(RasterCommand::GsParenL);
    let streams = [
        ("the receipt, GS v 0", &page, &profile),
        (
            "a page past the row limit, in GS v 0 strips (case 93)",
            &tall,
            &profile,
        ),
        (
            "the receipt, GS ( L store-and-print strips",
            &page,
            &paren_l,
        ),
    ];
    for (name, bitmap, framing) in streams {
        let bytes = encode(bitmap, framing).expect("encodes");
        let before = receipts(&api).len();
        let printer = driver(emupos, &profile, Duration::from_secs(30)).expect("a network profile");
        assert_eq!(
            printer.print(&RenderedReceipt { bytes }),
            PrintOutcome::Printed,
            "{name}"
        );
        let (width, height, theirs) = printed(&api, before);
        assert_eq!(
            (width, height),
            (bitmap.width_px, bitmap.height_px),
            "{name}: size"
        );
        let differing = theirs
            .iter()
            .zip(dots(bitmap))
            .filter(|(a, b)| **a != *b)
            .count();
        assert_eq!(differing, 0, "{name}: dots differing from ours");
        println!("{name}: {width}×{height}, dot-identical");
    }

    // Status, in each state, from both emulators.
    let ours = EmulatorTarget::start(profile);
    for (fault, name) in [
        (None, "ready"),
        (Some(ConnectionFault::PaperOut), "paper-out"),
        (Some(ConnectionFault::CoverOpen), "cover-open"),
        (Some(ConnectionFault::Offline), "offline"),
    ] {
        if let Some(fault) = fault {
            ours.script(fault);
            assert_eq!(
                http(&api, "PUT", &format!("/faults/{name}")).0,
                204,
                "setting {name}"
            );
        }
        let (mine, theirs) = (replies(ours.emulator.address()), replies(emupos));
        if fault.is_some() {
            assert_eq!(
                http(&api, "DELETE", &format!("/faults/{name}")).0,
                204,
                "clearing {name}"
            );
        }
        assert_eq!(mine, theirs, "{name}: DLE EOT 1–4, ours against emupos");
        println!("{name}: DLE EOT 1–4 agree: {mine:02X?}");
    }
}
