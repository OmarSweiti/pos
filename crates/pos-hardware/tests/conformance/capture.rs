//! A capture is a fixture (`phase-1-sellable-mvp.md` 1.7.13): what a device
//! was sent and what it answered, replayed through the emulator for the same
//! profile version from then on, so a profile corrected by evidence stays
//! corrected. Until milestone 5.8.3 records a real device, the one capture
//! under `tests/captures/` is synthetic, and its own fields say so.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::printer_profiles;
use crate::target::EmulatorTarget;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    profile_id: String,
    profile_version: u32,
    /// True for a trace written by hand rather than recorded from a device.
    synthetic: bool,
    /// Where the trace came from — the device, its firmware and the date, or
    /// for a synthetic trace, what it was written from.
    source: String,
    exchanges: Vec<Exchange>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Exchange {
    /// What was sent, as hex bytes separated by spaces.
    sent: String,
    /// What came back, the same way; empty when the device answered nothing.
    answered: String,
}

fn hex(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap_or_else(|_| panic!("not hex: {byte}")))
        .collect()
}

fn captures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/captures");
    let mut files = Vec::new();
    for dir in std::fs::read_dir(&root)
        .expect("tests/captures exists")
        .flatten()
    {
        for file in std::fs::read_dir(dir.path())
            .expect("a profile's captures")
            .flatten()
        {
            if file
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                files.push(file.path());
            }
        }
    }
    files.sort();
    files
}

/// Read the `expected` bytes, waiting up to a second for each, then listen
/// briefly for one more, so an answer longer than the capture's shows.
fn answer(stream: &mut TcpStream, expected: usize) -> Vec<u8> {
    let mut got = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        let patience = if got.len() < expected { 1_000 } else { 150 };
        stream
            .set_read_timeout(Some(Duration::from_millis(patience)))
            .expect("a read timeout");
        match stream.read(&mut byte) {
            Ok(1) => got.extend_from_slice(&byte),
            _ => return got,
        }
        if got.len() > expected {
            return got;
        }
    }
}

/// The named test: every capture replays through the emulator, byte for byte.
#[test]
fn a_captured_device_trace_replays_through_the_emulator() {
    let files = captures();
    assert!(!files.is_empty(), "no capture under tests/captures/");
    let profiles = printer_profiles();
    for path in files {
        let capture: Capture = serde_json::from_slice(&std::fs::read(&path).expect("reads"))
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let directory = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str());
        assert_eq!(
            directory,
            Some(capture.profile_id.as_str()),
            "{}: a capture lives under its profile's id",
            path.display()
        );
        if capture.synthetic {
            assert!(
                capture.source.contains("synthetic"),
                "{}: a synthetic capture must say so in its source",
                path.display()
            );
        }
        let profile = profiles
            .iter()
            .find(|p| {
                p.profile_id == capture.profile_id && p.version.get() == capture.profile_version
            })
            .unwrap_or_else(|| {
                panic!(
                    "{}: no profile {} v{}",
                    path.display(),
                    capture.profile_id,
                    capture.profile_version
                )
            });

        let target = EmulatorTarget::start(profile.clone());
        let mut stream = TcpStream::connect(target.emulator.address()).expect("connects");
        for (step, exchange) in capture.exchanges.iter().enumerate() {
            let expected = hex(&exchange.answered);
            stream.write_all(&hex(&exchange.sent)).expect("sends");
            let got = answer(&mut stream, expected.len());
            assert_eq!(
                got,
                expected,
                "{} step {step}: sent {}",
                path.display(),
                exchange.sent
            );
        }
        println!(
            "{}: {} exchanges replayed ({})",
            path.display(),
            capture.exchanges.len(),
            if capture.synthetic {
                "synthetic"
            } else {
                "recorded"
            }
        );
    }
}
