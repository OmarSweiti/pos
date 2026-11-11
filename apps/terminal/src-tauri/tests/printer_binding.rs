#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A register bound to its printer (microstep 1.7.14), on a register file
//! built through `pos_db::open`, the registered migration chain, against
//! 1.7.12's emulated printer.
//!
//! The binding is the register's `printer.binding` setting, read when the
//! till opens (1.7.14b). By the owner's decisions of 11 November 2026, a
//! binding the register can honour becomes its printer, and one it cannot
//! opens the till with no device and an alarm saying why, never a closed till.
//! The finalize hands each receipt to the print worker, which prints it off
//! the command path (1.7.14c).

mod shop;

use std::path::Path;
use std::time::Duration;

use pos_hardware::PrinterAtPay;
use pos_hardware::emulator::{Emulator, EmulatorConfig};
use pos_hardware::profile::PrinterProfile;
use pos_hardware::render::PaperWidth;
use rusqlite::params;
use shop::{CASHIER_CODE, CASHIER_PIN, KEY, OIL, REGISTER_ID, open, product_id, seed};
use terminal_lib::commands::auth::login_pin;
use terminal_lib::commands::cart::{add_product, open_cart};
use terminal_lib::commands::sale::{ReceiptOutcome, finalize};
use terminal_lib::commands::tender::{add_cash, begin_payment};
use terminal_lib::health::{Alarm, health};
use terminal_lib::print_service::drain;
use terminal_lib::state::{BindingRefusal, DATABASE_FILE, PrinterBindingState, Register};

/// An address in TEST-NET-1, which no test ever connects to: a binding it
/// names is refused before any device is reached.
const NOWHERE: &str = "192.0.2.10:9100";

/// 1.7.12's emulated printer, under the emulator's own 80 mm profile.
fn emulator() -> Emulator {
    Emulator::start(EmulatorConfig {
        printer: PrinterProfile::emulator(PaperWidth::Mm80),
        drawer: None,
        listen: "127.0.0.1:0".parse().unwrap(),
        out_dir: None,
    })
    .expect("the emulator starts")
}

/// The shop's register file, its `printer.binding` set to `value_json`
/// before the till opens, and the till opened with the cashier signed in.
fn bound_shop(value_json: Option<&str>) -> (tempfile::TempDir, Register) {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), true);
    if let Some(value_json) = value_json {
        bind(dir.path(), value_json);
    }
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    (dir, register)
}

fn bind(dir: &Path, value_json: &str) {
    let conn = pos_db::open(&dir.join(DATABASE_FILE), KEY).unwrap();
    conn.execute(
        "INSERT INTO setting (scope, scope_id, key, value_json)
         VALUES ('register', ?1, 'printer.binding', ?2)",
        params![REGISTER_ID.as_slice(), value_json],
    )
    .unwrap();
}

fn binding_to(profile_id: &str, address: &str) -> String {
    format!(r#"{{"profile_id": "{profile_id}", "version": 1, "address": "{address}"}}"#)
}

/// One bottle of oil, paid for in cash: what became of its receipt.
fn sell_one(register: &Register) -> ReceiptOutcome {
    open_cart(register).unwrap();
    add_product(register, product_id(OIL), None).unwrap();
    let paying = begin_payment(register).unwrap();
    add_cash(register, paying.cash_due_minor).unwrap();
    finalize(register).unwrap().receipt
}

/// Every print job, oldest first: the profile its receipt was rendered for,
/// its state, and each attempt's outcome and error code.
fn jobs(register: &Register) -> Vec<(String, String, Vec<String>)> {
    register.with_db(|conn| {
        conn.prepare(
            "SELECT a.printer_profile, j.state,
                    coalesce((SELECT group_concat(outcome || ':' || coalesce(error_code, ''), ' ')
                                FROM (SELECT outcome, error_code FROM print_attempt
                                       WHERE job_id = j.id ORDER BY attempt_no)), '')
               FROM print_job j JOIN receipt_artifact a ON a.id = j.artifact_id
              ORDER BY j.rowid",
        )
        .unwrap()
        .query_map([], |row| {
            let attempts: String = row.get(2)?;
            Ok((
                row.get(0)?,
                row.get(1)?,
                attempts
                    .split(' ')
                    .filter(|a| !a.is_empty())
                    .map(str::to_owned)
                    .collect(),
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
    })
}

/// The profile every original receipt was rendered for, oldest first.
fn rendered_for(register: &Register) -> Vec<String> {
    register.with_db(|conn| {
        conn.prepare("SELECT printer_profile FROM receipt_artifact ORDER BY rowid")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    })
}

/// 1.7.14's first named test: a register bound to the emulator's 80 mm profile
/// renders every receipt for that profile and prints it on the device at the
/// bound address. Its health names the binding and raises nothing. The
/// finalize hands the receipt off; one pass of the print worker prints it.
#[test]
fn a_register_renders_for_its_bound_profile() {
    let emulator = emulator();
    let address = emulator.address().to_string();
    let (_dir, register) = bound_shop(Some(&binding_to("emulator-80mm", &address)));

    assert_eq!(
        register.printer_binding(),
        &PrinterBindingState::Bound {
            profile_id: "emulator-80mm".to_owned(),
            version: 1,
            address: address.clone(),
        }
    );
    let device = register.receipt_device();
    assert_eq!(device.profile, PrinterProfile::emulator(PaperWidth::Mm80));
    assert!(device.printer.is_some(), "a device is attached");
    let health = health(&register).unwrap();
    assert_eq!(health.printer.reading, PrinterAtPay::Ready);
    assert_eq!(health.alarms, Vec::new());

    assert_eq!(sell_one(&register), ReceiptOutcome::SentToPrinter);
    assert_eq!(rendered_for(&register), ["emulator-80mm"]);
    drain(&register).unwrap();
    assert_eq!(
        jobs(&register),
        [(
            "emulator-80mm".to_owned(),
            "printed".to_owned(),
            vec!["printed:".to_owned()]
        )]
    );
    assert!(emulator.settled(3, Duration::from_secs(5)));
    let jobs = emulator.jobs();
    let [job] = jobs.as_slice() else {
        panic!("one job, got {}", jobs.len())
    };
    assert!(job.cut, "the receipt arrived whole, and was cut");
}

/// 1.7.14's second named test: a binding to a profile that leaves a parameter
/// the driver needs unknown is refused when the till opens. The till opens
/// anyway, as an unbound register: it renders for the simulator's profile,
/// attaches no device, and keeps and queues every receipt. Its health names
/// the binding, the reason and every unresolved parameter, with the binding's
/// alarm first.
#[test]
fn a_profile_with_an_unknown_the_driver_needs_cannot_be_bound() {
    let (_dir, register) = bound_shop(Some(&binding_to("epson-tm-t20iii-ethernet-80mm", NOWHERE)));
    let refused = PrinterBindingState::Refused {
        reason: BindingRefusal::ProfileUnresolved,
        profile_id: Some("epson-tm-t20iii-ethernet-80mm".to_owned()),
        version: Some(1),
        unresolved: vec![
            "cut_command",
            "cut_feed_rows",
            "drawer_pulse",
            "supports_raster_arabic",
        ],
    };
    assert_eq!(register.printer_binding(), &refused);
    let device = register.receipt_device();
    assert_eq!(device.profile, PrinterProfile::simulator(PaperWidth::Mm80));
    assert!(device.printer.is_none(), "no device is attached");

    let health = health(&register).unwrap();
    assert_eq!(health.printer.binding, refused);
    assert_eq!(health.printer.reading, PrinterAtPay::NoPrinter);
    assert_eq!(
        health.alarms,
        [Alarm::PrinterBindingRefused, Alarm::PrinterUnavailable]
    );
    assert_eq!(
        serde_json::to_value(&health.printer.binding).unwrap(),
        serde_json::json!({
            "state": "refused",
            "reason": "profile_unresolved",
            "profile_id": "epson-tm-t20iii-ethernet-80mm",
            "version": 1,
            "unresolved": ["cut_command", "cut_feed_rows", "drawer_pulse", "supports_raster_arabic"],
        })
    );

    assert_eq!(
        sell_one(&register),
        ReceiptOutcome::Queued,
        "selling never stops"
    );
    assert_eq!(rendered_for(&register), ["simulator-80mm"]);
}

/// 1.7.14b: every binding a register cannot honour opens the till with no
/// device and says why, and so a sale still completes with its receipt kept.
/// A profile the build does not ship, one not reached over TCP 9100, and a
/// value that is not the shape the key promises are each refused by name. No
/// binding at all is no refusal: the register is simply unbound, and only the
/// missing printer is raised.
#[test]
fn a_binding_the_register_cannot_honour_opens_the_till_with_no_device() {
    for (value, expected) in [
        (None, PrinterBindingState::Unbound),
        (
            Some(binding_to("no-such-printer", NOWHERE)),
            PrinterBindingState::Refused {
                reason: BindingRefusal::ProfileNotShipped,
                profile_id: Some("no-such-printer".to_owned()),
                version: Some(1),
                unresolved: Vec::new(),
            },
        ),
        (
            Some(binding_to("simulator-80mm", NOWHERE)),
            PrinterBindingState::Refused {
                reason: BindingRefusal::NotANetworkPrinter,
                profile_id: Some("simulator-80mm".to_owned()),
                version: Some(1),
                unresolved: Vec::new(),
            },
        ),
        (
            Some(binding_to("emulator-80mm", "printer.local:9100")),
            PrinterBindingState::Refused {
                reason: BindingRefusal::SettingUnreadable,
                profile_id: None,
                version: None,
                unresolved: Vec::new(),
            },
        ),
    ] {
        let (_dir, register) = bound_shop(value.as_deref());
        assert_eq!(register.printer_binding(), &expected, "{value:?}");
        assert!(register.receipt_device().printer.is_none(), "{value:?}");
        let alarms = health(&register).unwrap().alarms;
        if expected == PrinterBindingState::Unbound {
            assert_eq!(alarms, [Alarm::PrinterUnavailable], "{value:?}");
        } else {
            assert_eq!(
                alarms,
                [Alarm::PrinterBindingRefused, Alarm::PrinterUnavailable],
                "{value:?}"
            );
        }
        assert_eq!(sell_one(&register), ReceiptOutcome::Queued, "{value:?}");
    }
}
