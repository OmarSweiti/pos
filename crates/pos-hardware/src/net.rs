//! The network printer: ESC/POS bytes out over TCP 9100, real-time status back
//! (microstep 1.7.10).
//!
//! Nothing else in this crate writes a byte to a real printer or reads one
//! back. [`NetworkPrinter`] is that driver, built on [`crate::transport`] so a
//! serial or USB transport can replace the socket later without touching what
//! is here: the four outcomes, the status protocol and the drawer.
//!
//! # `print` classifies; it never retries
//!
//! | What the socket saw | Outcome |
//! |---|---|
//! | no connection, or the device closed it before taking a byte | `Failed(Unreachable)` |
//! | a pre-flight status of paper-out, cover-open or offline | `Failed` with that reason — nothing was sent |
//! | every byte taken | `Printed` |
//! | bytes taken, then the connection closed or reset | `Partial`, with the count taken |
//! | the device stopped taking bytes until the deadline passed | `Unknown` |
//!
//! These are `ref/hardware-and-receipts.md` §3's outcomes, and its rules decide
//! what follows: only `Failed` is retried, and only by the queue.
//!
//! # `status` admits when a device has none
//!
//! It asks `DLE EOT` 1 to 4 — printer, offline cause, error cause, paper
//! sensor — each under a short deadline. A device that never answers the first
//! reports [`PrinterStatus::Unsupported`]: neither ready nor offline, because
//! that is a fact about the device rather than about this moment — Xprinter's
//! 80-series manual marks its `DLE EOT` valid on serial only. A profile that
//! already says so is never asked.
//!
//! # The drawer is its own connection
//!
//! `open_drawer` sends `ESC p m t1 t2` from the profile — the pin, and the on-
//! and off-times in Epson's 2 ms units — on a connection of its own, never
//! inside a document (§4: a retried document that carried a pulse would open
//! the drawer again with nobody watching).

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::profile::{
    DrawerPin, DrawerPulse, Fact, Parameter, PrinterProfile, ProfileError, StatusProtocol,
    Transport as ProfileTransport,
};
use crate::transport::{Connection, TcpTransport, Transport, WriteError, WriteFailure};
use crate::{FailReason, HwError, PrintOutcome, PrinterStatus, ReceiptPrinter, RenderedReceipt};

/// `DLE EOT` — the real-time status request. The function number follows.
pub const DLE_EOT: [u8; 2] = [0x10, 0x04];

/// `ESC p` — the drawer pulse. Pin, on-time and off-time follow.
pub const DRAWER_PULSE: [u8; 2] = [0x1B, 0x70];

/// Every status reply byte has bits 1 and 4 set and bits 0 and 7 clear. It is
/// how a reply is told from noise.
const REPLY_MASK: u8 = 0b1001_0011;
const REPLY_FIXED: u8 = 0b0001_0010;

/// The four `DLE EOT` replies, in the order they are asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusReplies {
    /// `DLE EOT 1`: bit 2 is the drawer connector's pin 3, bit 3 offline.
    pub printer: u8,
    /// `DLE EOT 2`: bit 2 cover open, bit 5 stopped at paper end, bit 6 an
    /// error.
    pub offline_cause: u8,
    /// `DLE EOT 3`: bits 2, 3, 5 and 6 are the recoverable, cutter,
    /// unrecoverable and auto-recoverable errors.
    pub error_cause: u8,
    /// `DLE EOT 4`: bits 2–3 paper near its end, bits 5–6 paper out.
    pub paper: u8,
}

impl StatusReplies {
    /// Whether `byte` has the fixed bits every status reply carries.
    #[must_use]
    pub const fn is_reply(byte: u8) -> bool {
        byte & REPLY_MASK == REPLY_FIXED
    }

    /// What the device's replies say about it now.
    #[must_use]
    pub const fn classify(self) -> PrinterStatus {
        if self.offline_cause & 0b0000_0100 != 0 {
            PrinterStatus::CoverOpen
        } else if self.paper & 0b0110_0000 == 0b0110_0000 || self.offline_cause & 0b0010_0000 != 0 {
            PrinterStatus::PaperOut
        } else if self.printer & 0b0000_1000 != 0
            || self.offline_cause & 0b0100_0000 != 0
            || self.error_cause & 0b0110_1100 != 0
        {
            PrinterStatus::Offline
        } else {
            PrinterStatus::Ready
        }
    }

    /// The level of the drawer connector's pin 3, where a drawer's open-sensor
    /// reports; which level means *open* is the drawer profile's polarity.
    #[must_use]
    pub const fn drawer_pin3_high(self) -> bool {
        self.printer & 0b0000_0100 != 0
    }
}

/// How long each step may take before it is given up on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadlines {
    pub connect: Duration,
    /// For a whole document, not for each write.
    pub write: Duration,
    /// For each status reply.
    pub status: Duration,
}

impl Default for Deadlines {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(3),
            // A long basket is a few hundred kilobytes, which a printer takes
            // only as fast as it prints.
            write: Duration::from_secs(30),
            status: Duration::from_millis(500),
        }
    }
}

/// `ESC p m t1 t2` for one pulse.
///
/// # Errors
///
/// [`HwError::PulseOutOfRange`] when a time does not fit the command's one
/// byte of 2 ms units, 510 ms at most.
pub fn drawer_command(pulse: DrawerPulse) -> Result<[u8; 5], HwError> {
    let pin = match pulse.pin {
        DrawerPin::Pin2 => 0,
        DrawerPin::Pin5 => 1,
    };
    // Rounded up: a pulse shorter than the drawer asks for may not open it.
    let units = |ms: u16| u8::try_from(ms.div_ceil(2)).map_err(|_| HwError::PulseOutOfRange { ms });
    let [escape, p] = DRAWER_PULSE;
    Ok([escape, p, pin, units(pulse.on_ms)?, units(pulse.off_ms)?])
}

/// A receipt printer on the network, bound to one address and one profile.
#[derive(Debug)]
pub struct NetworkPrinter<T: Transport = TcpTransport> {
    transport: T,
    profile: PrinterProfile,
    deadlines: Deadlines,
}

impl NetworkPrinter<TcpTransport> {
    /// The printer at `address` — the one bound in the register's
    /// configuration; nothing here discovers a printer.
    ///
    /// # Errors
    ///
    /// As [`NetworkPrinter::new`].
    pub fn tcp(address: SocketAddr, profile: PrinterProfile) -> Result<Self, HwError> {
        Self::new(TcpTransport::new(address), profile, Deadlines::default())
    }
}

impl<T: Transport> NetworkPrinter<T> {
    /// # Errors
    ///
    /// [`HwError::WrongTransport`] for a profile that says its device is
    /// reached some other way than TCP 9100. A profile that does not know is
    /// allowed: binding it to a register is refused elsewhere, and a developer
    /// may still print to it.
    pub fn new(
        transport: T,
        profile: PrinterProfile,
        deadlines: Deadlines,
    ) -> Result<Self, HwError> {
        match profile.transport {
            Fact::Known(ProfileTransport::Tcp9100) | Fact::Unknown => Ok(Self {
                transport,
                profile,
                deadlines,
            }),
            Fact::Known(other) => Err(HwError::WrongTransport {
                profile_id: profile.profile_id.clone(),
                transport: format!("{other:?}"),
            }),
        }
    }

    #[must_use]
    pub const fn profile(&self) -> &PrinterProfile {
        &self.profile
    }

    /// Whether the device is worth asking for status: a profile that says it
    /// has none is believed.
    const fn asks_status(&self) -> bool {
        !matches!(
            self.profile.status_protocol,
            Fact::Known(StatusProtocol::None)
        )
    }

    /// Ask `DLE EOT` 1 to 4 on an open connection. No answer to the first is a
    /// device without status; an answer that stops partway is a device that
    /// cannot be trusted to take a document, so it reads as offline.
    fn read_status(&self, connection: &mut T::Connection) -> PrinterStatus {
        let mut replies = [0_u8; 4];
        for (index, reply) in replies.iter_mut().enumerate() {
            let function = u8::try_from(index + 1).unwrap_or(1);
            match ask(connection, function, self.deadlines.status) {
                Some(byte) => *reply = byte,
                None if index == 0 => return PrinterStatus::Unsupported,
                None => return PrinterStatus::Offline,
            }
        }
        let [printer, offline_cause, error_cause, paper] = replies;
        StatusReplies {
            printer,
            offline_cause,
            error_cause,
            paper,
        }
        .classify()
    }
}

/// Send `DLE EOT function` and read its one-byte reply within `deadline`.
fn ask<C: Connection>(connection: &mut C, function: u8, deadline: Duration) -> Option<u8> {
    let start = Instant::now();
    let [dle, eot] = DLE_EOT;
    connection.write_all(&[dle, eot, function], deadline).ok()?;
    let left = deadline.checked_sub(start.elapsed())?;
    let mut reply = [0_u8; 1];
    match connection.read(&mut reply, left) {
        Ok(1) => {
            let [byte] = reply;
            StatusReplies::is_reply(byte).then_some(byte)
        }
        _ => None,
    }
}

impl<T: Transport> ReceiptPrinter for NetworkPrinter<T> {
    fn print(&self, doc: &RenderedReceipt) -> PrintOutcome {
        let Ok(mut connection) = self.transport.connect(self.deadlines.connect) else {
            return PrintOutcome::Failed(FailReason::Unreachable);
        };
        if self.asks_status() {
            let refused = match self.read_status(&mut connection) {
                PrinterStatus::Ready | PrinterStatus::Unsupported => None,
                PrinterStatus::PaperOut => Some(FailReason::PaperOut),
                PrinterStatus::CoverOpen => Some(FailReason::CoverOpen),
                PrinterStatus::Offline => Some(FailReason::Offline),
            };
            if let Some(reason) = refused {
                return PrintOutcome::Failed(reason);
            }
        }
        match connection.write_all(&doc.bytes, self.deadlines.write) {
            Ok(()) => {
                connection.finish(self.deadlines.status);
                PrintOutcome::Printed
            }
            Err(WriteError {
                failure: WriteFailure::TimedOut,
                ..
            }) => PrintOutcome::Unknown,
            Err(WriteError {
                accepted: 0,
                failure: WriteFailure::Closed,
            }) => PrintOutcome::Failed(FailReason::Unreachable),
            Err(WriteError {
                accepted,
                failure: WriteFailure::Closed,
            }) => PrintOutcome::Partial {
                acknowledged: accepted,
            },
        }
    }

    fn open_drawer(&self) -> Result<(), HwError> {
        let pulse = self.profile.drawer_pulse.known().copied().ok_or_else(|| {
            HwError::Profile(ProfileError::Unknown {
                profile_id: self.profile.profile_id.clone(),
                parameter: Parameter::DrawerPulse,
            })
        })?;
        let command = drawer_command(pulse)?;
        let mut connection = self
            .transport
            .connect(self.deadlines.connect)
            .map_err(|_| HwError::Offline)?;
        connection
            .write_all(&command, self.deadlines.write)
            .map_err(|error| HwError::Io(error.to_string()))?;
        connection.finish(self.deadlines.status);
        Ok(())
    }

    fn status(&self) -> PrinterStatus {
        if !self.asks_status() {
            return PrinterStatus::Unsupported;
        }
        match self.transport.connect(self.deadlines.connect) {
            Ok(mut connection) => {
                let status = self.read_status(&mut connection);
                connection.finish(self.deadlines.status);
                status
            }
            Err(_) => PrinterStatus::Offline,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;

    use super::*;
    use crate::render::PaperWidth;

    /// A profile for a network printer that answers `DLE EOT` and drives a
    /// drawer on pin 5 for 50 ms on, 500 ms off.
    fn profile() -> PrinterProfile {
        let mut profile = PrinterProfile::simulator(PaperWidth::Mm80);
        profile.profile_id = "test-network-80mm".to_owned();
        profile.transport = Fact::Known(ProfileTransport::Tcp9100);
        profile.status_protocol = Fact::Known(StatusProtocol::DleEot);
        profile.drawer_pulse = Fact::Known(DrawerPulse {
            pin: DrawerPin::Pin5,
            on_ms: 50,
            off_ms: 500,
        });
        profile
    }

    fn deadlines(write: Duration) -> Deadlines {
        Deadlines {
            connect: Duration::from_secs(2),
            write,
            status: Duration::from_millis(400),
        }
    }

    fn printer_at(address: SocketAddr, profile: PrinterProfile, write: Duration) -> NetworkPrinter {
        NetworkPrinter::new(TcpTransport::new(address), profile, deadlines(write)).expect("tcp")
    }

    /// An in-process device: a listener on 127.0.0.1 that hands every
    /// connection to `serve`, and keeps what each connection sent it.
    struct Fake {
        address: SocketAddr,
        received: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl Fake {
        fn received(&self) -> Vec<Vec<u8>> {
            // A connection is recorded when its handler returns; give the
            // handler its moment after the client has finished.
            for _ in 0..50 {
                thread::sleep(Duration::from_millis(10));
            }
            self.received.lock().unwrap().clone()
        }
    }

    fn fake<F>(serve: F) -> Fake
    where
        F: Fn(&mut TcpStream, &mut Vec<u8>) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let received = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&received);
        let serve = Arc::new(serve);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let record = Arc::clone(&record);
                let serve = Arc::clone(&serve);
                thread::spawn(move || {
                    let mut bytes = Vec::new();
                    serve(&mut stream, &mut bytes);
                    record.lock().unwrap().push(bytes);
                });
            }
        });
        Fake { address, received }
    }

    /// Serve like a device that answers each `DLE EOT n` with `replies[n-1]`
    /// and takes every other byte, until the client closes.
    fn answering(replies: [u8; 4]) -> impl Fn(&mut TcpStream, &mut Vec<u8>) + Send + Sync {
        move |stream, bytes| {
            let mut chunk = [0_u8; 8192];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => return,
                    Ok(read) => {
                        let start = bytes.len().saturating_sub(2);
                        bytes.extend_from_slice(chunk.get(..read).unwrap());
                        // Answer every query that completed in this chunk.
                        let window = bytes.get(start..).unwrap().to_vec();
                        for query in window.windows(3) {
                            if let [0x10, 0x04, function @ 1..=4] = *query {
                                let reply = replies.get(usize::from(function) - 1).copied();
                                if stream.write_all(&[reply.unwrap()]).is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    const READY: [u8; 4] = [0x12, 0x12, 0x12, 0x12];

    fn document(bytes: usize) -> RenderedReceipt {
        RenderedReceipt {
            bytes: (0..bytes).map(|index| (index % 251) as u8 | 0x80).collect(),
        }
    }

    /// Larger than any loopback socket's buffers, so a device that stops
    /// reading, or closes, is felt by the writer mid-document.
    const LARGE: usize = 32 * 1024 * 1024;

    #[test]
    fn a_document_the_device_takes_whole_is_printed() {
        let device = fake(answering(READY));
        let printer = printer_at(device.address, profile(), Duration::from_secs(5));
        let doc = document(100_000);
        assert_eq!(printer.print(&doc), PrintOutcome::Printed);

        let received = device.received();
        let [connection] = received.as_slice() else {
            panic!("one connection, got {}", received.len())
        };
        let queries: Vec<u8> = [1, 2, 3, 4]
            .iter()
            .flat_map(|function| [0x10, 0x04, *function])
            .collect();
        assert_eq!(
            connection.get(..12),
            Some(queries.as_slice()),
            "pre-flight first"
        );
        assert_eq!(
            connection.get(12..),
            Some(doc.bytes.as_slice()),
            "then the document, whole"
        );
    }

    #[test]
    fn a_refused_connection_is_failed_and_nothing_is_sent() {
        let address = {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            listener.local_addr().expect("address")
        };
        let printer = printer_at(address, profile(), Duration::from_secs(1));
        assert_eq!(
            printer.print(&document(10)),
            PrintOutcome::Failed(FailReason::Unreachable)
        );
        assert_eq!(printer.status(), PrinterStatus::Offline);
        assert!(matches!(printer.open_drawer(), Err(HwError::Offline)));
    }

    #[test]
    fn a_device_that_closes_mid_document_is_partial() {
        let device = fake(|stream, bytes| {
            // Answer the pre-flight as ready, take some of the document, and
            // drop the connection with the rest unread — the reset a printer
            // that loses power mid-receipt leaves behind.
            let mut taken = 0;
            let mut chunk = [0_u8; 8192];
            while taken < 12 + 64 * 1024 {
                let Ok(read) = stream.read(&mut chunk) else {
                    return;
                };
                if read == 0 {
                    return;
                }
                bytes.extend_from_slice(chunk.get(..read).unwrap());
                if taken < 12 {
                    let before = taken / 3;
                    let after = (taken + read).min(12) / 3;
                    for _ in before..after {
                        let _ = stream.write_all(&[0x12]);
                    }
                }
                taken += read;
            }
        });
        let printer = printer_at(device.address, profile(), Duration::from_secs(10));
        match printer.print(&document(LARGE)) {
            PrintOutcome::Partial { acknowledged } => {
                assert!(acknowledged > 0 && acknowledged < LARGE, "{acknowledged}");
            }
            other => panic!("expected Partial, got {other:?}"),
        }
    }

    #[test]
    fn a_device_that_falls_silent_is_unknown() {
        let device = fake(|stream, bytes| {
            // Answer the pre-flight, then stop reading for good: the device
            // is there and says nothing.
            let mut query = [0_u8; 3];
            for _ in 0..4 {
                if stream.read_exact(&mut query).is_err() {
                    return;
                }
                bytes.extend_from_slice(&query);
                let _ = stream.write_all(&[0x12]);
            }
            thread::sleep(Duration::from_secs(3));
        });
        let printer = printer_at(device.address, profile(), Duration::from_millis(500));
        assert_eq!(printer.print(&document(LARGE)), PrintOutcome::Unknown);
    }

    #[test]
    fn a_paper_out_pre_flight_fails_the_document_before_a_byte() {
        // `DLE EOT 4` answering 0x7E: paper near its end and out.
        let device = fake(answering([0x12, 0x12, 0x12, 0x7E]));
        let printer = printer_at(device.address, profile(), Duration::from_secs(5));
        assert_eq!(
            printer.print(&document(10_000)),
            PrintOutcome::Failed(FailReason::PaperOut)
        );
        let received = device.received();
        let [connection] = received.as_slice() else {
            panic!("one connection")
        };
        assert_eq!(
            connection.len(),
            12,
            "only the four queries: no byte of the document"
        );
    }

    #[test]
    fn a_device_that_never_answers_dle_eot_reports_status_unsupported() {
        // Takes every byte and answers none: Xprinter's Ethernet models.
        let device = fake(|stream, bytes| {
            let _ = stream.read_to_end(bytes);
        });
        let printer = printer_at(device.address, profile(), Duration::from_secs(5));
        assert_eq!(printer.status(), PrinterStatus::Unsupported);
        // And a document still prints: no status is not a refusal.
        assert_eq!(printer.print(&document(1_000)), PrintOutcome::Printed);
    }

    #[test]
    fn a_profile_that_says_it_has_no_status_is_never_asked() {
        let device = fake(|stream, bytes| {
            let _ = stream.read_to_end(bytes);
        });
        let mut quiet = profile();
        quiet.status_protocol = Fact::Known(StatusProtocol::None);
        let printer = printer_at(device.address, quiet, Duration::from_secs(5));
        assert_eq!(printer.status(), PrinterStatus::Unsupported);
        let doc = document(1_000);
        assert_eq!(printer.print(&doc), PrintOutcome::Printed);
        assert_eq!(
            device.received(),
            vec![doc.bytes],
            "no status connection, and no query before the document"
        );
    }

    /// The named rule: each `DLE EOT` reply is parsed to the status it means.
    #[test]
    fn the_status_bits_are_parsed() {
        let ready = StatusReplies {
            printer: 0x12,
            offline_cause: 0x12,
            error_cause: 0x12,
            paper: 0x12,
        };
        assert_eq!(ready.classify(), PrinterStatus::Ready);
        // Paper near its end is still ready; out is not.
        assert_eq!(
            StatusReplies {
                paper: 0x1E,
                ..ready
            }
            .classify(),
            PrinterStatus::Ready
        );
        assert_eq!(
            StatusReplies {
                paper: 0x72,
                ..ready
            }
            .classify(),
            PrinterStatus::PaperOut
        );
        assert_eq!(
            StatusReplies {
                offline_cause: 0x32,
                ..ready
            }
            .classify(),
            PrinterStatus::PaperOut,
            "stopped at paper end"
        );
        assert_eq!(
            StatusReplies {
                offline_cause: 0x16,
                ..ready
            }
            .classify(),
            PrinterStatus::CoverOpen
        );
        assert_eq!(
            StatusReplies {
                printer: 0x1A,
                ..ready
            }
            .classify(),
            PrinterStatus::Offline
        );
        assert_eq!(
            StatusReplies {
                error_cause: 0x1A,
                ..ready
            }
            .classify(),
            PrinterStatus::Offline,
            "a cutter error"
        );
        assert!(
            StatusReplies {
                printer: 0x16,
                ..ready
            }
            .drawer_pin3_high()
        );
        assert!(!ready.drawer_pin3_high());
        assert!(StatusReplies::is_reply(0x12) && StatusReplies::is_reply(0x7E));
        assert!(!StatusReplies::is_reply(0x00) && !StatusReplies::is_reply(0x92));

        // And read from a device: every reason a printer refuses a document.
        for (replies, status) in [
            (READY, PrinterStatus::Ready),
            ([0x12, 0x16, 0x12, 0x12], PrinterStatus::CoverOpen),
            ([0x12, 0x12, 0x12, 0x7E], PrinterStatus::PaperOut),
            ([0x1A, 0x12, 0x12, 0x12], PrinterStatus::Offline),
        ] {
            let device = fake(answering(replies));
            let printer = printer_at(device.address, profile(), Duration::from_secs(5));
            assert_eq!(printer.status(), status, "{replies:02X?}");
        }
    }

    /// The named rule: the pulse is the profile's pin and timings, on a
    /// connection of its own.
    #[test]
    fn the_drawer_pulse_is_the_profiles_pin_and_timings() {
        let device = fake(|stream, bytes| {
            let _ = stream.read_to_end(bytes);
        });
        let mut quiet = profile();
        quiet.status_protocol = Fact::Known(StatusProtocol::None);
        let printer = printer_at(device.address, quiet, Duration::from_secs(5));
        let doc = document(1_000);
        assert_eq!(printer.print(&doc), PrintOutcome::Printed);
        printer.open_drawer().expect("pulses");
        // Pin 5 is m = 1; 50 ms is 25 units of 2 ms, 500 ms is 250.
        assert_eq!(
            device.received(),
            vec![doc.bytes, vec![0x1B, 0x70, 0x01, 0x19, 0xFA]],
            "the document, and then the pulse alone on its own connection"
        );

        assert_eq!(
            drawer_command(DrawerPulse {
                pin: DrawerPin::Pin2,
                on_ms: 101,
                off_ms: 0
            })
            .unwrap(),
            [0x1B, 0x70, 0x00, 51, 0],
            "rounded up: a pulse shorter than asked may not open the drawer"
        );
        assert!(matches!(
            drawer_command(DrawerPulse {
                pin: DrawerPin::Pin2,
                on_ms: 511,
                off_ms: 0
            }),
            Err(HwError::PulseOutOfRange { ms: 511 })
        ));
    }

    #[test]
    fn an_unknown_drawer_pulse_is_refused_not_guessed() {
        let device = fake(|stream, bytes| {
            let _ = stream.read_to_end(bytes);
        });
        let mut unknown = profile();
        unknown.drawer_pulse = Fact::Unknown;
        let printer = printer_at(device.address, unknown, Duration::from_secs(5));
        assert!(matches!(
            printer.open_drawer(),
            Err(HwError::Profile(ProfileError::Unknown {
                parameter: Parameter::DrawerPulse,
                ..
            }))
        ));
        assert!(device.received().is_empty(), "nothing was sent");
    }

    #[test]
    fn a_profile_for_another_transport_is_refused() {
        let mut usb = profile();
        usb.transport = Fact::Known(ProfileTransport::Usb);
        let address: SocketAddr = "127.0.0.1:9100".parse().unwrap();
        assert!(matches!(
            NetworkPrinter::tcp(address, usb),
            Err(HwError::WrongTransport { .. })
        ));
    }
}
