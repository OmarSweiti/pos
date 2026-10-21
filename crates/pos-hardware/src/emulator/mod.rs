//! The emulated network printer (microstep 1.7.12): a process that listens on
//! TCP and behaves as one printer profile and one drawer profile say a device
//! does — the second rung of `ref/hardware-and-receipts.md` §6b.1's ladder.
//!
//! It is a tool, not product. It exists behind the `emulator` feature, which
//! nothing a register ships enables (`the_register_bundle_never_enables_the_emulator`).
//!
//! # What it does
//!
//! - **It accepts** `ESC @`, `GS v 0`, `GS ( L` (store and print), `GS V`,
//!   `ESC p`, `DLE EOT` and `ESC d`, and reports every other byte rather than
//!   guessing what it meant.
//! - **It refuses a raster the profile says the device would not take**: one
//!   taller than its row limit, wider than its line, or framed by the command
//!   it does not use.
//! - **Where the profile says `unknown`, it reports unknown behaviour rather
//!   than inventing one** — a raster against an unknown limit, a status request
//!   to a device whose status protocol nobody stated, a pulse to a drawer whose
//!   pin nobody measured. Each is an [`Event::UnknownBehaviour`] naming the
//!   value, and none of them prints or opens anything.
//! - **It answers `DLE EOT`** with the replies a device in its state gives —
//!   the bits are Epson's — late, or not at all, as the profile and the fault
//!   script say.
//! - **It records each `ESC p`** with its pin and timings, opens the drawer only
//!   when the drawer profile's pin and voltage agree, and reports the drawer's
//!   sensor at the drawer profile's polarity.
//! - **It renders every raster it accepts** into a [`Bitmap`], and with an
//!   output directory writes each document as a PNG beside the raw bytes it
//!   received — what a manual pass reads (`02-development-workflow.md` §5).
//!
//! # Faults, per connection
//!
//! [`Emulator::script`] queues a [`ConnectionFault`] for the next connection:
//! paper-out, cover-open, offline, slow, or a peer that goes silent after *n*
//! bytes — 1.7.8's in-process sweep, with a twin on a real socket.

mod parse;
pub mod png;

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parse::{Command, Parsed, parse};

use crate::profile::data::{Bounds, CashDrawerValues, SensorPolarity};
use crate::profile::{CutCommand, DrawerPin, Fact, PrinterProfile, RasterCommand, StatusProtocol};
use crate::render::Bitmap;

/// What the emulator knows about the drawer on its port — a drawer profile's
/// values without their sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawerModel {
    pub profile_id: String,
    pub pin: Fact<DrawerPin>,
    pub solenoid_volts: Fact<Bounds>,
    pub sensor: Fact<bool>,
    pub polarity: Fact<SensorPolarity>,
}

impl DrawerModel {
    #[must_use]
    pub fn from_profile(profile_id: &str, values: &CashDrawerValues) -> Self {
        Self {
            profile_id: profile_id.to_owned(),
            pin: values.pin.fact,
            solenoid_volts: values.solenoid_volts.fact,
            sensor: values.sensor.fact,
            polarity: values.sensor_polarity.fact,
        }
    }
}

/// A fault the next connection meets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionFault {
    /// Out of paper: status says so, and a document is held, not printed.
    PaperOut,
    /// The cover is open: status says so, and a document is held.
    CoverOpen,
    /// Offline: status says so, and a document is held.
    Offline,
    /// Every reply, and every read, waits this long first.
    Slow(Duration),
    /// The device stops reading after this many bytes, and never closes.
    SilentAfter(usize),
}

/// Something the emulated device did, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Connected {
        connection: usize,
    },
    Initialised,
    /// A raster command accepted onto the page.
    Raster {
        command: RasterCommand,
        width_dots: u32,
        rows: u32,
    },
    /// A raster command taller than the profile's limit, refused.
    RasterTooTall {
        command: RasterCommand,
        rows: u32,
        limit: u32,
    },
    /// A raster whose width is not the profile's line, refused.
    RasterWrongWidth {
        width_dots: u32,
        dot_width: u32,
    },
    /// A document received while the device could not print it.
    Held {
        fault: ConnectionFault,
    },
    Cut {
        mode: u8,
    },
    /// An `ESC p`, and whether the drawer opened — `Unknown` when the drawer
    /// profile does not say.
    DrawerPulse {
        pin: DrawerPin,
        on_ms: u16,
        off_ms: u16,
        opened: Fact<bool>,
    },
    /// A `DLE EOT`, and the reply, if one was sent.
    StatusAsked {
        function: u8,
        reply: Option<u8>,
    },
    /// The profile does not say how the device behaves here, so the emulator
    /// does nothing and says so.
    UnknownBehaviour {
        what: String,
    },
    /// Bytes that begin no command this device knows, counted in a run.
    Unrecognised {
        first: u8,
        count: usize,
    },
    Closed {
        connection: usize,
        bytes: usize,
    },
}

/// One document the device received and printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub connection: usize,
    /// Every accepted raster row, as the device would have printed them.
    pub page: Bitmap,
    /// Whether a cut ended it, rather than the connection closing.
    pub cut: bool,
    /// The bytes the device received for this document.
    pub bytes: Vec<u8>,
    /// Where the PNG and the raw bytes were written, with an output directory.
    pub png: Option<PathBuf>,
}

/// How to start an emulator.
#[derive(Debug, Clone)]
pub struct EmulatorConfig {
    pub printer: PrinterProfile,
    /// The drawer on the printer's port, if one is plugged in.
    pub drawer: Option<DrawerModel>,
    /// Where to listen. `127.0.0.1` by default, because port 9100 has no
    /// authentication and an emulator is nobody's printer.
    pub listen: SocketAddr,
    /// Where each document's PNG and raw bytes go; `None` keeps them in memory.
    pub out_dir: Option<PathBuf>,
}

#[derive(Debug, Default)]
struct Shared {
    events: Vec<Event>,
    jobs: Vec<Job>,
    faults: VecDeque<ConnectionFault>,
    drawer_open: bool,
    connections: usize,
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A running emulator. It stops when dropped.
#[derive(Debug)]
pub struct Emulator {
    address: SocketAddr,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    acceptor: Option<JoinHandle<()>>,
}

impl Emulator {
    /// Bind and start accepting connections.
    ///
    /// # Errors
    ///
    /// The address cannot be bound, or the output directory cannot be made.
    pub fn start(config: EmulatorConfig) -> io::Result<Self> {
        if let Some(dir) = &config.out_dir {
            std::fs::create_dir_all(dir)?;
        }
        let listener = TcpListener::bind(config.listen)?;
        let address = listener.local_addr()?;
        let shared = Arc::new(Mutex::new(Shared::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let acceptor = {
            let shared = Arc::clone(&shared);
            let stop = Arc::clone(&stop);
            let config = Arc::new(config);
            thread::spawn(move || accept(&listener, &config, &shared, &stop))
        };
        Ok(Self {
            address,
            shared,
            stop,
            acceptor: Some(acceptor),
        })
    }

    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    /// Queue a fault for the next connection the emulator accepts.
    pub fn script(&self, fault: ConnectionFault) {
        lock(&self.shared).faults.push_back(fault);
    }

    #[must_use]
    pub fn events(&self) -> Vec<Event> {
        lock(&self.shared).events.clone()
    }

    #[must_use]
    pub fn jobs(&self) -> Vec<Job> {
        lock(&self.shared).jobs.clone()
    }

    #[must_use]
    pub fn drawer_open(&self) -> bool {
        lock(&self.shared).drawer_open
    }

    /// Push the drawer shut, as a cashier does.
    pub fn close_drawer(&self) {
        lock(&self.shared).drawer_open = false;
    }

    /// Wait until `connections` have been served and closed, at most `deadline`.
    #[must_use]
    pub fn settled(&self, connections: usize, deadline: Duration) -> bool {
        let start = std::time::Instant::now();
        loop {
            let closed = lock(&self.shared)
                .events
                .iter()
                .filter(|event| matches!(event, Event::Closed { .. }))
                .count();
            if closed >= connections {
                return true;
            }
            if start.elapsed() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the acceptor so it sees the flag.
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_millis(200));
        if let Some(acceptor) = self.acceptor.take() {
            let _ = acceptor.join();
        }
    }
}

fn accept(
    listener: &TcpListener,
    config: &Arc<EmulatorConfig>,
    shared: &Arc<Mutex<Shared>>,
    stop: &Arc<AtomicBool>,
) {
    for stream in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = stream else { continue };
        let (connection, fault) = {
            let mut state = lock(shared);
            state.connections += 1;
            let connection = state.connections;
            state.events.push(Event::Connected { connection });
            (connection, state.faults.pop_front())
        };
        let config = Arc::clone(config);
        let shared = Arc::clone(shared);
        let stop = Arc::clone(stop);
        thread::spawn(move || {
            let mut session = Session::new(&config, &shared, connection, fault);
            session.serve(stream, &stop);
        });
    }
}

/// One connection: the bytes received so far, and the page they are building.
struct Session<'a> {
    config: &'a EmulatorConfig,
    shared: &'a Mutex<Shared>,
    connection: usize,
    fault: Option<ConnectionFault>,
    received: Vec<u8>,
    /// Where in `received` the current document began: its first command
    /// other than a status request. `None` between documents.
    document_start: Option<usize>,
    rows: Vec<u8>,
    width_dots: Option<u32>,
    graphics: Option<(u32, u32, Range)>,
    held: bool,
    told: Vec<String>,
}

type Range = core::ops::Range<usize>;

impl<'a> Session<'a> {
    fn new(
        config: &'a EmulatorConfig,
        shared: &'a Mutex<Shared>,
        connection: usize,
        fault: Option<ConnectionFault>,
    ) -> Self {
        Self {
            config,
            shared,
            connection,
            fault,
            received: Vec::new(),
            document_start: None,
            rows: Vec::new(),
            width_dots: None,
            graphics: None,
            held: false,
            told: Vec::new(),
        }
    }

    fn event(&self, event: Event) {
        lock(self.shared).events.push(event);
    }

    /// Report unknown behaviour once per connection for each value.
    fn unknown(&mut self, what: &str) {
        if !self.told.iter().any(|told| told == what) {
            self.told.push(what.to_owned());
            self.event(Event::UnknownBehaviour {
                what: what.to_owned(),
            });
        }
    }

    const fn blocking_fault(&self) -> Option<ConnectionFault> {
        match self.fault {
            Some(
                fault @ (ConnectionFault::PaperOut
                | ConnectionFault::CoverOpen
                | ConnectionFault::Offline),
            ) => Some(fault),
            _ => None,
        }
    }

    fn pause(&self) {
        if let Some(ConnectionFault::Slow(delay)) = self.fault {
            thread::sleep(delay);
        }
    }

    fn serve(&mut self, mut stream: TcpStream, stop: &AtomicBool) {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
        let mut parsed_to = 0;
        let mut chunk = [0_u8; 16 * 1024];
        loop {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let budget = match self.fault {
                Some(ConnectionFault::SilentAfter(limit)) => {
                    let left = limit.saturating_sub(self.received.len());
                    if left == 0 {
                        // Silent: neither read nor close, until told to stop.
                        thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                    left.min(chunk.len())
                }
                _ => chunk.len(),
            };
            let Some(buffer) = chunk.get_mut(..budget) else {
                break;
            };
            match stream.read(buffer) {
                Ok(0) => break,
                Ok(read) => {
                    self.received
                        .extend_from_slice(buffer.get(..read).unwrap_or_default());
                    self.pause();
                    parsed_to = self.run(&mut stream, parsed_to);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => break,
            }
        }
        self.end_document(false, self.received.len());
        let _ = stream.shutdown(Shutdown::Both);
        self.event(Event::Closed {
            connection: self.connection,
            bytes: self.received.len(),
        });
    }

    /// Act on every whole command from `from`, and return where parsing stopped.
    fn run(&mut self, stream: &mut TcpStream, from: usize) -> usize {
        let mut at = from;
        let mut unrecognised: Option<(u8, usize)> = None;
        while let Some(rest) = self.received.get(at..) {
            let Parsed::Command { command, length } = parse(rest) else {
                break;
            };
            let start = at;
            at += length;
            if let Command::Unrecognised { byte } = command {
                unrecognised = Some(match unrecognised {
                    Some((first, count)) => (first, count + 1),
                    None => (byte, 1),
                });
                continue;
            }
            if let Some((first, count)) = unrecognised.take() {
                self.event(Event::Unrecognised { first, count });
            }
            if self.document_start.is_none() && !matches!(command, Command::StatusRequest { .. }) {
                self.document_start = Some(start);
            }
            self.act(stream, command, start, at);
        }
        if let Some((first, count)) = unrecognised {
            self.event(Event::Unrecognised { first, count });
        }
        at
    }

    fn act(&mut self, stream: &mut TcpStream, command: Command, start: usize, end: usize) {
        match command {
            Command::Initialise => self.event(Event::Initialised),
            Command::Feed { .. } | Command::GraphicsOther { .. } | Command::Unrecognised { .. } => {
            }
            Command::RasterBitImage {
                mode,
                width_bytes,
                rows,
                data,
            } => {
                if mode != 0 {
                    self.unknown("a scaled GS v 0 (m other than 0)");
                    return;
                }
                let data = offset(&data, start);
                self.accept_raster(RasterCommand::GsV0, width_bytes * 8, rows, data);
            }
            Command::GraphicsStore {
                format,
                width_dots,
                rows,
                data,
            } => {
                if format != [0x30, 0x01, 0x01, 0x31] {
                    self.unknown("a GS ( L store that is not monochrome at normal size");
                    return;
                }
                self.graphics = Some((width_dots, rows, offset(&data, start)));
            }
            Command::GraphicsPrint => match self.graphics.take() {
                Some((width_dots, rows, data)) => {
                    self.accept_raster(RasterCommand::GsParenL, width_dots, rows, data);
                }
                None => self.unknown("a GS ( L print with nothing stored"),
            },
            Command::Cut { mode } => {
                self.event(Event::Cut { mode });
                let expected = match mode {
                    0 | 48 | 65 => CutCommand::Full,
                    _ => CutCommand::Partial,
                };
                match self.config.printer.cut_command {
                    Fact::Known(command) if command != expected => {
                        self.unknown("a cut this device's profile does not use");
                    }
                    Fact::Unknown => self.unknown("cut_command"),
                    Fact::Known(_) => {}
                }
                self.end_document(true, end);
            }
            Command::DrawerPulse { mode, on, off } => self.pulse(mode, on, off),
            Command::StatusRequest { function } => self.answer(stream, function),
        }
    }

    /// A raster command's rows, if the profile says the device takes them.
    fn accept_raster(&mut self, command: RasterCommand, width_dots: u32, rows: u32, data: Range) {
        let profile = &self.config.printer;
        match profile.raster_command {
            Fact::Known(framing) if framing != command => {
                self.unknown("a raster command this device's profile does not use");
                return;
            }
            Fact::Unknown => {
                self.unknown("raster_command");
                return;
            }
            Fact::Known(_) => {}
        }
        match profile.raster_max_rows {
            Fact::Known(limit) if rows > limit.get() => {
                self.event(Event::RasterTooTall {
                    command,
                    rows,
                    limit: limit.get(),
                });
                return;
            }
            Fact::Unknown => {
                self.unknown("raster_max_rows");
                return;
            }
            Fact::Known(_) => {}
        }
        match profile.dot_width {
            Fact::Known(dot_width) if width_dots != dot_width.get() => {
                self.event(Event::RasterWrongWidth {
                    width_dots,
                    dot_width: dot_width.get(),
                });
                return;
            }
            Fact::Unknown => {
                self.unknown("dot_width");
                return;
            }
            Fact::Known(_) => {}
        }
        if let Some(fault) = self.blocking_fault() {
            if !self.held {
                self.held = true;
                self.event(Event::Held { fault });
            }
            return;
        }
        if self.width_dots.is_some_and(|width| width != width_dots) {
            self.unknown("strips of different widths in one document");
            return;
        }
        let Some(bytes) = self.received.get(data) else {
            return;
        };
        self.width_dots = Some(width_dots);
        self.rows.extend_from_slice(bytes);
        self.event(Event::Raster {
            command,
            width_dots,
            rows,
        });
    }

    /// Close the document that ends at `end` in the stream, recording it as a
    /// job when it printed anything.
    fn end_document(&mut self, cut: bool, end: usize) {
        let start = self.document_start.take().unwrap_or(end);
        let bytes = self.received.get(start..end).unwrap_or_default().to_vec();
        self.held = false;
        let Some(width_px) = self.width_dots.take() else {
            return;
        };
        let rows = std::mem::take(&mut self.rows);
        let row_bytes = width_px.div_ceil(8) as usize;
        let height_px = u32::try_from(rows.len() / row_bytes.max(1)).unwrap_or(u32::MAX);
        let page = Bitmap {
            width_px,
            height_px,
            bits: rows,
        };
        let mut state = lock(self.shared);
        let index = state.jobs.len() + 1;
        let png = self.config.out_dir.as_ref().and_then(|dir| {
            let stem = dir.join(format!("job-{index:04}"));
            let png = stem.with_extension("png");
            let written = png::write(&png, &page).is_ok()
                && std::fs::write(stem.with_extension("bin"), &bytes).is_ok();
            written.then_some(png)
        });
        state.jobs.push(Job {
            connection: self.connection,
            page,
            cut,
            bytes,
            png,
        });
    }

    fn pulse(&mut self, mode: u8, on: u8, off: u8) {
        let pin = if mode & 1 == 0 {
            DrawerPin::Pin2
        } else {
            DrawerPin::Pin5
        };
        let opened = self.opens(pin);
        if opened == Fact::Known(true) {
            lock(self.shared).drawer_open = true;
        }
        self.event(Event::DrawerPulse {
            pin,
            on_ms: u16::from(on) * 2,
            off_ms: u16::from(off) * 2,
            opened,
        });
    }

    /// Whether a pulse on `pin` opens the drawer: its pin must be the drawer's,
    /// and the port's voltage within its solenoid's.
    fn opens(&mut self, pin: DrawerPin) -> Fact<bool> {
        let Some(drawer) = self.config.drawer.clone() else {
            return Fact::Known(false);
        };
        let Fact::Known(drawer_pin) = drawer.pin else {
            self.unknown("the drawer's pin");
            return Fact::Unknown;
        };
        if drawer_pin != pin {
            return Fact::Known(false);
        }
        match (self.config.printer.drawer_port_volts, drawer.solenoid_volts) {
            (Fact::Known(volts), Fact::Known(range)) => {
                Fact::Known(range.min <= volts && volts <= range.max)
            }
            _ => {
                self.unknown("whether the drawer port's voltage drives the solenoid");
                Fact::Unknown
            }
        }
    }

    /// The drawer connector's pin 3, as the drawer's sensor drives it.
    fn pin3_high(&mut self) -> bool {
        let open = lock(self.shared).drawer_open;
        match self
            .config
            .drawer
            .as_ref()
            .map(|drawer| (drawer.sensor, drawer.polarity))
        {
            None | Some((Fact::Known(false), _)) => false,
            Some((Fact::Known(true), Fact::Known(SensorPolarity::OpenIsHigh))) => open,
            Some((Fact::Known(true), Fact::Known(SensorPolarity::OpenIsLow))) => !open,
            Some(_) => {
                self.unknown("the drawer's sensor and its polarity");
                false
            }
        }
    }

    fn answer(&mut self, stream: &mut TcpStream, function: u8) {
        let reply = match self.config.printer.status_protocol {
            Fact::Known(StatusProtocol::DleEot) => self.status_byte(function),
            Fact::Known(StatusProtocol::None) => None,
            Fact::Unknown => {
                self.unknown("status_protocol");
                None
            }
        };
        if let Some(byte) = reply {
            self.pause();
            let _ = stream.write_all(&[byte]);
        }
        self.event(Event::StatusAsked { function, reply });
    }

    /// Epson's reply to `DLE EOT function` for a device in this state.
    fn status_byte(&mut self, function: u8) -> Option<u8> {
        const FIXED: u8 = 0b0001_0010;
        let fault = self.blocking_fault();
        let byte = match function {
            1 => {
                let offline = if fault.is_some() { 0b0000_1000 } else { 0 };
                let drawer = if self.pin3_high() { 0b0000_0100 } else { 0 };
                FIXED | offline | drawer
            }
            2 => match fault {
                Some(ConnectionFault::CoverOpen) => FIXED | 0b0000_0100,
                Some(ConnectionFault::PaperOut) => FIXED | 0b0010_0000,
                _ => FIXED,
            },
            3 => FIXED,
            4 => match fault {
                Some(ConnectionFault::PaperOut) => FIXED | 0b0110_1100,
                _ => FIXED,
            },
            _ => return None,
        };
        Some(byte)
    }
}

/// A command's payload range, from its own start to the whole stream's.
fn offset(range: &Range, start: usize) -> Range {
    range.start + start..range.end + start
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::render::PaperWidth;

    /// The four `DLE EOT` replies a device in `fault` gives, byte for byte.
    fn replies(fault: Option<ConnectionFault>) -> [Option<u8>; 4] {
        let config = EmulatorConfig {
            printer: PrinterProfile::simulator(PaperWidth::Mm80),
            drawer: None,
            listen: "127.0.0.1:0".parse().unwrap(),
            out_dir: None,
        };
        let shared = Mutex::new(Shared::default());
        let mut session = Session::new(&config, &shared, 1, fault);
        [1, 2, 3, 4].map(|function| session.status_byte(function))
    }

    /// Each reply byte is pinned on its own: paper-out sets the printer's
    /// offline bit, the offline cause's paper-end stop, and the sensor's near-
    /// end and end bits — Epson's layout, as an independent emulator answered
    /// in the spike (`ref/hardware-and-receipts.md` §6b.8).
    #[test]
    fn each_fault_answers_with_epsons_status_bits() {
        assert_eq!(replies(None), [Some(0x12); 4]);
        assert_eq!(
            replies(Some(ConnectionFault::PaperOut)),
            [Some(0x1A), Some(0x32), Some(0x12), Some(0x7E)]
        );
        assert_eq!(
            replies(Some(ConnectionFault::CoverOpen)),
            [Some(0x1A), Some(0x16), Some(0x12), Some(0x12)]
        );
        assert_eq!(
            replies(Some(ConnectionFault::Offline)),
            [Some(0x1A), Some(0x12), Some(0x12), Some(0x12)]
        );
        // Slow and silent change when a device answers, not what it says.
        assert_eq!(
            replies(Some(ConnectionFault::Slow(Duration::ZERO))),
            [Some(0x12); 4]
        );
    }
}
