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
//! the command path (1.7.14c), and answers that the drawer's pulse was sent,
//! never that the drawer opened (1.7.14d, #414).

mod shop;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pos_hardware::PrinterAtPay;
use pos_hardware::emulator::{ConnectionFault, DrawerModel, Emulator, EmulatorConfig, Event};
use pos_hardware::profile::data::Bounds;
use pos_hardware::profile::{DrawerPin, Fact, PrinterProfile};
use pos_hardware::render::PaperWidth;
use rusqlite::params;
use shop::{CASHIER_CODE, CASHIER_PIN, KEY, OIL, REGISTER_ID, open, product_id, seed};
use terminal_lib::commands::auth::login_pin;
use terminal_lib::commands::cart::{add_product, open_cart};
use terminal_lib::commands::sale::{DrawerOutcome, ReceiptOutcome, finalize};
use terminal_lib::commands::stock::on_hand;
use terminal_lib::commands::tender::{add_cash, begin_payment};
use terminal_lib::health::{Alarm, health};
use terminal_lib::print_service::{self, drain};
use terminal_lib::state::{BindingRefusal, DATABASE_FILE, PrinterBindingState, Register};

/// An address in TEST-NET-1, which no test ever connects to: a binding it
/// names is refused before any device is reached.
const NOWHERE: &str = "192.0.2.10:9100";

/// 1.7.12's emulated printer, under the emulator's own 80 mm profile.
fn emulator() -> Emulator {
    emulator_with(None)
}

/// The emulated printer, with `drawer` plugged into its drawer port.
fn emulator_with(drawer: Option<DrawerModel>) -> Emulator {
    Emulator::start(EmulatorConfig {
        printer: PrinterProfile::emulator(PaperWidth::Mm80),
        drawer,
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

/// Wait, a bounded while, for the worker to bring the jobs to `done`.
fn until(register: &Register, what: &str, done: impl Fn(&[(String, String, Vec<String>)]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(&jobs(register)) {
        assert!(Instant::now() < deadline, "{what}: {:?}", jobs(register));
        std::thread::sleep(Duration::from_millis(10));
    }
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

/// 1.7.14's third named test, and its `Done when`: a sale finalized on a
/// register bound to the emulator's 80 mm profile prints through the print
/// worker, off the command path. The finalize answers having printed nothing.
/// The worker, started after it, prints the receipt on a printer slow to
/// answer; while the device takes its time, a command that needs the
/// register's database is answered, and the job is still printing.
#[test]
fn the_print_worker_drains_off_the_command_path() {
    let emulator = emulator();
    let address = emulator.address().to_string();
    let (_dir, register) = bound_shop(Some(&binding_to("emulator-80mm", &address)));
    let register = Arc::new(register);

    assert_eq!(sell_one(&register), ReceiptOutcome::SentToPrinter);
    assert_eq!(
        jobs(&register),
        [("emulator-80mm".to_owned(), "queued".to_owned(), Vec::new())],
        "the finalize printed nothing"
    );
    assert!(emulator.jobs().is_empty());

    // A printer slow to answer: every reply and every read waits.
    emulator.script(ConnectionFault::Slow(Duration::from_millis(300)));
    let worker = print_service::start(&register, Duration::from_secs(60)).unwrap();
    until(&register, "the worker leases the job", |jobs| {
        jobs.iter().any(|(_, state, _)| state == "printing")
    });
    // The register's database is free while the device is written to.
    assert_eq!(
        on_hand(&register, product_id(OIL)).unwrap().on_hand_milli,
        9_000
    );
    assert_eq!(
        jobs(&register),
        [(
            "emulator-80mm".to_owned(),
            "printing".to_owned(),
            Vec::new()
        )],
        "the command was not held behind the device"
    );
    until(&register, "the worker prints the job", |jobs| {
        jobs.iter().all(|(_, state, _)| state == "printed")
    });
    worker.stop();
    // Pay's status read, the drawer's pulse, and the worker's print.
    assert!(emulator.settled(3, Duration::from_secs(5)));
    let printed = emulator.jobs();
    let [job] = printed.as_slice() else {
        panic!("one job, got {}", printed.len())
    };
    assert!(job.cut, "the receipt arrived whole, and was cut");
}

/// 1.7.14's fourth named test: a receipt rendered for another profile before
/// the register was bound cannot be printed byte for byte on this device (§3
/// rule 7). The worker withholds it, never sending a byte, and leaves it on
/// the unprinted worklist for a person, where the health counts it; the bound
/// register's own receipts print.
#[test]
fn a_receipt_rendered_for_another_profile_is_withheld_not_printed() {
    let emulator = emulator();
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), true);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(
        sell_one(&register),
        ReceiptOutcome::Queued,
        "no printer yet"
    );
    drop(register);

    bind(
        dir.path(),
        &binding_to("emulator-80mm", &emulator.address().to_string()),
    );
    let register = Arc::new(open(dir.path()));
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    // The worker drains as it starts: the kept receipt is withheld.
    let worker = print_service::start(&register, Duration::from_secs(60)).unwrap();
    until(&register, "the worker withholds the kept receipt", |jobs| {
        jobs.iter().any(|(_, _, attempts)| !attempts.is_empty())
    });
    assert_eq!(sell_one(&register), ReceiptOutcome::SentToPrinter);
    until(&register, "the worker prints the new receipt", |jobs| {
        jobs.iter().any(|(_, state, _)| state == "printed")
    });
    worker.stop();
    assert_eq!(
        jobs(&register),
        [
            (
                "simulator-80mm".to_owned(),
                "failed".to_owned(),
                vec!["failed:profile_mismatch".to_owned()]
            ),
            (
                "emulator-80mm".to_owned(),
                "printed".to_owned(),
                vec!["printed:".to_owned()]
            ),
        ]
    );
    let withheld_retry: Option<String> = register.with_db(|conn| {
        conn.query_row(
            "SELECT next_attempt_at FROM print_job ORDER BY rowid LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap()
    });
    assert_eq!(
        withheld_retry, None,
        "never tried again: it is for a person"
    );
    assert!(emulator.settled(3, Duration::from_secs(5)));
    assert_eq!(
        emulator.jobs().len(),
        1,
        "only the bound register's own receipt reached the device"
    );
    let health = health(&register).unwrap();
    assert_eq!(health.printer.receipts_waiting, 1);
    assert_eq!(health.alarms, [Alarm::ReceiptsWaiting]);
}

/// A drawer on pin 2 that a 12–24 V pulse opens, as the emulator's 24 V port
/// drives it: written out here, not read from a profile, so the test says
/// what opens it.
fn a_drawer_that_opens() -> DrawerModel {
    DrawerModel {
        profile_id: "test-drawer".to_owned(),
        pin: Fact::Known(DrawerPin::Pin2),
        solenoid_volts: Fact::Known(Bounds { min: 12, max: 24 }),
        sensor: Fact::Unknown,
        polarity: Fact::Unknown,
    }
}

/// The pulses the emulator received, once the first has arrived.
fn pulses(emulator: &Emulator) -> Vec<Fact<bool>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let pulses: Vec<Fact<bool>> = emulator
            .events()
            .into_iter()
            .filter_map(|event| match event {
                Event::DrawerPulse { opened, .. } => Some(opened),
                _ => None,
            })
            .collect();
        if !pulses.is_empty() || Instant::now() >= deadline {
            return pulses;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// 1.7.14d (#414): the finalize answers what the driver knows of the drawer,
/// and no more. A drawer is kicked by a pulse on the printer's drawer port and
/// nothing reads back whether it opened, so the answer is `pulse_sent`
/// whatever became of the drawer: here one that opens, a port with no drawer
/// plugged in, and a printer that reports itself offline. The emulator sees
/// one pulse each time, and only the first opens anything. The old answer,
/// `opened`, was wrong for the other two.
#[test]
fn the_drawer_is_answered_pulse_sent_whatever_the_printer_says() {
    for (drawer, fault, opened) in [
        (Some(a_drawer_that_opens()), None, true),
        (None, None, false),
        (None, Some(ConnectionFault::Offline), false),
    ] {
        let case = format!("drawer plugged in: {}, fault: {fault:?}", drawer.is_some());
        let emulator = emulator_with(drawer);
        let address = emulator.address().to_string();
        let (_dir, register) = bound_shop(Some(&binding_to("emulator-80mm", &address)));
        open_cart(&register).unwrap();
        add_product(&register, product_id(OIL), None).unwrap();
        let paying = begin_payment(&register).unwrap();
        add_cash(&register, paying.cash_due_minor).unwrap();
        // The finalize kicks the drawer first, so the next connection is the
        // drawer's.
        if let Some(fault) = fault {
            emulator.script(fault);
        }

        let answer = finalize(&register).unwrap();
        assert_eq!(answer.drawer, DrawerOutcome::PulseSent, "{case}");
        assert_eq!(
            serde_json::to_value(answer.drawer).unwrap(),
            serde_json::json!("pulse_sent"),
            "{case}"
        );
        assert_eq!(answer.receipt, ReceiptOutcome::SentToPrinter, "{case}");
        assert_eq!(pulses(&emulator), [Fact::Known(opened)], "{case}");
        assert_eq!(emulator.drawer_open(), opened, "{case}");
    }
}
