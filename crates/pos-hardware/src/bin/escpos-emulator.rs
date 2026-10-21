//! `escpos-emulator`: the emulated network printer (microstep 1.7.12) as a
//! process a developer prints to before any printer exists.
//!
//! ```text
//! escpos-emulator --printer <profile> [--drawer <profile>]
//!                 [--listen 127.0.0.1:9100] [--out target/printer-emulator]
//!                 [--profiles <dir>]
//! ```
//!
//! `<profile>` is `simulator-80mm`, `simulator-58mm`, or the id of a profile
//! under `crates/pos-hardware/profiles/` — a printer for `--printer`, a cash
//! drawer for `--drawer`. It listens on `127.0.0.1` unless told otherwise,
//! because port 9100 has no authentication and an emulator is nobody's
//! printer; it writes each document's PNG and raw bytes under `--out`, and
//! prints every event as it happens. `just printer-emulator <profile>` runs it.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use pos_hardware::emulator::{DrawerModel, Emulator, EmulatorConfig};
use pos_hardware::profile::PrinterProfile;
use pos_hardware::profile::data::{self, ClassValues};
use pos_hardware::render::PaperWidth;

struct Args {
    printer: String,
    drawer: Option<String>,
    listen: SocketAddr,
    out: PathBuf,
    profiles: PathBuf,
}

const USAGE: &str = "usage: escpos-emulator --printer <profile> [--drawer <profile>] \
                     [--listen 127.0.0.1:9100] [--out <dir>] [--profiles <dir>]";

fn args() -> Result<Args, String> {
    let mut printer = None;
    let mut drawer = None;
    let mut listen = SocketAddr::from(([127, 0, 0, 1], 9100));
    let mut out = PathBuf::from("target/printer-emulator");
    let mut profiles = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/profiles"));
    let mut words = std::env::args().skip(1);
    while let Some(flag) = words.next() {
        let value = words
            .next()
            .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))?;
        match flag.as_str() {
            "--printer" => printer = Some(value),
            // An empty drawer is no drawer: `just printer-emulator` passes one.
            "--drawer" => drawer = Some(value).filter(|id| !id.is_empty()),
            "--listen" => {
                listen = value
                    .parse()
                    .map_err(|error| format!("--listen {value}: {error}"))?;
            }
            "--out" => out = PathBuf::from(value),
            "--profiles" => profiles = PathBuf::from(value),
            _ => return Err(format!("unknown flag {flag}\n{USAGE}")),
        }
    }
    Ok(Args {
        printer: printer.ok_or_else(|| USAGE.to_owned())?,
        drawer,
        listen,
        out,
        profiles,
    })
}

fn run() -> Result<(), String> {
    let args = args()?;
    let profiles = data::load_dir(&args.profiles).map_err(|error| error.to_string())?;
    let printer = match args.printer.as_str() {
        "simulator-80mm" => PrinterProfile::simulator(PaperWidth::Mm80),
        "simulator-58mm" => PrinterProfile::simulator(PaperWidth::Mm58),
        id => profiles
            .iter()
            .find(|(_, profile)| profile.profile_id == id)
            .and_then(|(_, profile)| profile.printer_profile())
            .ok_or_else(|| format!("no printer profile {id} under {}", args.profiles.display()))?,
    };
    let drawer = match &args.drawer {
        None => None,
        Some(id) => {
            let values = profiles
                .iter()
                .find_map(|(_, profile)| match &profile.values {
                    ClassValues::CashDrawer(values) if profile.profile_id == *id => Some(values),
                    _ => None,
                })
                .ok_or_else(|| {
                    format!("no drawer profile {id} under {}", args.profiles.display())
                })?;
            Some(DrawerModel::from_profile(id, values))
        }
    };
    let out = args.out.join(&printer.profile_id);
    let emulator = Emulator::start(EmulatorConfig {
        printer: printer.clone(),
        drawer,
        listen: args.listen,
        out_dir: Some(out.clone()),
    })
    .map_err(|error| format!("cannot listen on {}: {error}", args.listen))?;
    println!(
        "escpos-emulator: {} on {}, documents to {}",
        printer.profile_id,
        emulator.address(),
        out.display()
    );
    let mut shown = 0;
    loop {
        thread::sleep(Duration::from_millis(200));
        let events = emulator.events();
        for event in events.iter().skip(shown) {
            println!("{event:?}");
        }
        shown = events.len();
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("escpos-emulator: {message}");
            ExitCode::from(2)
        }
    }
}
