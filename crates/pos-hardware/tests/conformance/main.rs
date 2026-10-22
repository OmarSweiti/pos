//! One conformance suite, two targets (microstep 1.7.13,
//! `ref/hardware-and-receipts.md` §6b.3).
//!
//! The receipt printer's contract — §3's four outcomes and status rule, §4's
//! drawer, case 93's strips — written once against `ReceiptPrinter` and a
//! profile. It runs against 1.7.12's emulator once per receipt-printer profile
//! on every commit, and against a physical device at milestone 5.8 — only one
//! named explicitly, never one found on the network.
//!
//! Its byte streams are also replayed through an emulator nobody here wrote
//! (`cross_check`, §6b.8), so the bytes are never graded only by this
//! project's own reading of the specification.
//!
//! One outcome is not reached here: `Partial`, bytes taken and then a reset.
//! The emulator has no fault that closes mid-document, so 1.7.10's
//! in-process fake proves it (`a_device_that_closes_mid_document_is_partial`).
#![cfg(feature = "emulator")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../support/receipt.rs"]
mod receipt;

mod capture;
mod cross_check;
mod suite;
mod target;

use std::path::Path;

use pos_hardware::profile::PrinterProfile;
use pos_hardware::profile::data::{self, ClassValues};

/// Every receipt-printer profile file, in a stable order.
fn printer_profiles() -> Vec<PrinterProfile> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("profiles");
    let mut printers: Vec<PrinterProfile> = data::load_dir(&root)
        .expect("the committed profiles load")
        .into_iter()
        .filter(|(_, profile)| matches!(profile.values, ClassValues::ReceiptPrinter(_)))
        .filter_map(|(_, profile)| profile.printer_profile())
        .collect();
    printers.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
    printers
}

/// The named test: the contract, against the emulator, once for every profile
/// under `profiles/receipt_printer/`. Run with `--nocapture` for the report.
#[test]
fn the_printer_conformance_suite_passes_for_every_profile() {
    let profiles = printer_profiles();
    assert!(
        profiles.len() >= 8,
        "the suite found {} printer profiles; every seeded one must run",
        profiles.len()
    );
    for profile in &profiles {
        println!("\n{} (v{})", profile.profile_id, profile.version);
        for case in suite::run(profile) {
            println!("  {:32} {}", case.name, case.observed);
        }
    }
}

/// The named test: the device target never runs against a printer it was not
/// given — both the address and the profile, explicitly, or nothing.
#[test]
fn the_device_target_refuses_to_run_without_an_explicit_device_and_profile() {
    for (address, profile, refusal) in [
        (None, None, "neither is set"),
        (
            None,
            Some("epson-tm-t20iii-ethernet-80mm"),
            "HW_DEVICE_ADDR is not set",
        ),
        (Some("192.168.1.50:9100"), None, "HW_PROFILE is not set"),
        (Some("  "), Some("  "), "neither is set"),
        (
            Some("printer.local:9100"),
            Some("epson-tm-t20iii-ethernet-80mm"),
            "not an IP address",
        ),
        (
            Some("0.0.0.0:9100"),
            Some("epson-tm-t20iii-ethernet-80mm"),
            "names no one device",
        ),
        (
            Some("192.168.1.50:0"),
            Some("epson-tm-t20iii-ethernet-80mm"),
            "names no one device",
        ),
    ] {
        let refused = target::device(address, profile)
            .expect_err("the device target ran without an explicit device and profile");
        assert!(
            refused.contains(refusal),
            "{address:?} / {profile:?}: {refused}"
        );
    }
    assert_eq!(
        target::device(
            Some("192.168.1.50:9100"),
            Some("epson-tm-t20iii-ethernet-80mm")
        ),
        Ok(target::Device {
            address: "192.168.1.50:9100".parse().unwrap(),
            profile_id: "epson-tm-t20iii-ethernet-80mm".to_owned(),
        })
    );
}

/// The device target, at milestone 5.8 (5.8.3): the cases a device can answer
/// without a fault script, against the printer and profile named in
/// `HW_DEVICE_ADDR` and `HW_PROFILE`. What the paper shows is read by the
/// person running it; that reading is the gate's record, not this test's.
#[test]
#[ignore = "the device target: run at the hardware gate with HW_DEVICE_ADDR and HW_PROFILE set"]
fn the_printer_conformance_suite_passes_against_the_named_device() {
    let device = target::device(
        std::env::var(target::DEVICE_ADDR).ok().as_deref(),
        std::env::var(target::DEVICE_PROFILE).ok().as_deref(),
    )
    .unwrap_or_else(|refused| panic!("{refused}"));
    let profile = printer_profiles()
        .into_iter()
        .find(|profile| profile.profile_id == device.profile_id)
        .unwrap_or_else(|| panic!("no printer profile {}", device.profile_id));
    for case in suite::run_on_device(&profile, device.address) {
        println!("  {:32} {}", case.name, case.observed);
    }
}
