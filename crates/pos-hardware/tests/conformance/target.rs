//! The two targets the suite runs against (`ref/hardware-and-receipts.md`
//! §6b.3): 1.7.12's emulator, once per profile, and a physical device — but a
//! device only when it is named.

use std::net::{SocketAddr, TcpListener};
use std::time::{Duration, Instant};

use pos_hardware::HwError;
use pos_hardware::emulator::{ConnectionFault, Emulator, EmulatorConfig, Event, Job};
use pos_hardware::net::{Deadlines, NetworkPrinter};
use pos_hardware::profile::PrinterProfile;
use pos_hardware::transport::TcpTransport;

/// The device target's two inputs. Both are required and neither has a
/// default: port 9100 has no authentication, so a suite that found a printer
/// by itself could print on whatever answered — a stranger's device on a
/// shared network.
pub const DEVICE_ADDR: &str = "HW_DEVICE_ADDR";
pub const DEVICE_PROFILE: &str = "HW_PROFILE";

/// A physical printer the suite may drive, named explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub address: SocketAddr,
    pub profile_id: String,
}

/// A value that was actually given: present, and not only whitespace.
fn given(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// The device the two inputs name, or why the device target refuses to run.
///
/// Takes the values rather than reading the environment, so the refusal is
/// tested without mutating a process-wide variable.
pub fn device(address: Option<&str>, profile: Option<&str>) -> Result<Device, String> {
    let (address, profile_id) = match (given(address), given(profile)) {
        (Some(address), Some(profile)) => (address, profile),
        (None, None) => {
            return Err(format!(
                "the device target runs only against a device named by both {DEVICE_ADDR} \
                 and {DEVICE_PROFILE}; neither is set"
            ));
        }
        (None, Some(_)) => {
            return Err(format!(
                "{DEVICE_ADDR} is not set: the device target never looks for a printer"
            ));
        }
        (Some(_), None) => {
            return Err(format!(
                "{DEVICE_PROFILE} is not set: a device is qualified against one profile, \
                 named, never guessed"
            ));
        }
    };
    let address: SocketAddr = address.parse().map_err(|error| {
        format!("{DEVICE_ADDR}={address} is not an IP address and port: {error}")
    })?;
    if address.ip().is_unspecified() || address.ip().is_multicast() || address.port() == 0 {
        return Err(format!(
            "{DEVICE_ADDR}={address} names no one device; give the printer's own address"
        ));
    }
    Ok(Device {
        address,
        profile_id: profile_id.to_owned(),
    })
}

/// 1.7.12's emulator, behaving as one profile says a device does, with no
/// drawer on its port.
pub struct EmulatorTarget {
    pub profile: PrinterProfile,
    pub emulator: Emulator,
}

impl EmulatorTarget {
    pub fn start(profile: PrinterProfile) -> Self {
        let emulator = Emulator::start(EmulatorConfig {
            printer: profile.clone(),
            drawer: None,
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            out_dir: None,
        })
        .expect("the emulator starts");
        Self { profile, emulator }
    }

    /// 1.7.10's driver, bound to this emulator and to `profile` — or its
    /// refusal, for a profile reached some other way than TCP 9100.
    pub fn driver(&self, profile: &PrinterProfile) -> Result<NetworkPrinter, HwError> {
        driver(self.emulator.address(), profile, Duration::from_secs(10))
    }

    /// The same driver with a write deadline short enough to meet a device
    /// that has stopped reading.
    pub fn impatient(&self) -> NetworkPrinter {
        driver(
            self.emulator.address(),
            &self.profile,
            Duration::from_millis(700),
        )
        .expect("a network profile")
    }

    pub fn script(&self, fault: ConnectionFault) {
        self.emulator.script(fault);
    }

    pub fn events(&self) -> Vec<Event> {
        self.emulator.events()
    }

    pub fn jobs(&self) -> Vec<Job> {
        self.emulator.jobs()
    }

    /// Wait until the events recorded so far satisfy `done`, at most ten seconds.
    pub fn until(&self, done: impl Fn(&[Event]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(&self.events()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Wait until every connection opened so far has been served and closed.
    pub fn settle(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let events = self.events();
            let opened = events
                .iter()
                .filter(|e| matches!(e, Event::Connected { .. }))
                .count();
            let closed = events
                .iter()
                .filter(|e| matches!(e, Event::Closed { .. }))
                .count();
            if closed >= opened || Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

pub fn driver(
    address: SocketAddr,
    profile: &PrinterProfile,
    write: Duration,
) -> Result<NetworkPrinter, HwError> {
    NetworkPrinter::new(
        TcpTransport::new(address),
        profile.clone(),
        Deadlines {
            connect: Duration::from_secs(2),
            write,
            status: Duration::from_millis(500),
        },
    )
}

/// An address nothing listens on: bound, read, and released.
pub fn nobody() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a free port")
}
