//! How bytes reach a device (microstep 1.7.10).
//!
//! `ref/hardware-and-receipts.md` §2 names three: TCP port 9100 for network
//! printers, a serial port for RS-232, and USB. They share one shape — connect
//! within a deadline, write every byte, read within a deadline — and that shape
//! is [`Transport`] and [`Connection`], so a serial port, a USB endpoint or an
//! Android maker's print service can later replace the socket without touching
//! the outcomes, the status protocol or the drawer that `crate::net` builds on
//! them. TCP is the only one written: it needs no driver on any operating
//! system, and the others wait for a qualified device that needs them.
//!
//! # What a write can know
//!
//! A socket accepts bytes into the operating system's buffer, and that is all a
//! write can report: not that the device read them, still less that it printed
//! them. [`WriteError::accepted`] is therefore the count the transport took
//! before it failed — the most any driver can say — and `crate::net` reads it
//! the safe way round: a device that dies early is reported as having taken
//! bytes it may not have, which costs a person one question, never a second
//! original.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// Why a connection could not be made or used.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// Nothing answered at the address, or it refused the connection.
    #[error("the device at {address} could not be reached: {source}")]
    Unreachable { address: String, source: io::Error },
    /// The deadline passed before the device answered.
    #[error("the device did not answer before the deadline")]
    TimedOut,
    /// The device closed the connection.
    #[error("the device closed the connection")]
    Closed,
    #[error("io: {0}")]
    Io(#[from] io::Error),
}

/// How a write ended before its last byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteFailure {
    /// The deadline passed: the device stopped taking bytes and said nothing.
    TimedOut,
    /// The device closed or reset the connection.
    Closed,
}

/// A write that did not finish, and how far it got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the write ended ({failure:?}) after the transport accepted {accepted} bytes")]
pub struct WriteError {
    /// Bytes the transport accepted before it failed.
    pub accepted: usize,
    pub failure: WriteFailure,
}

/// A way to reach one device.
pub trait Transport: Send + Sync {
    type Connection: Connection;

    /// Open a connection, waiting at most `deadline`.
    ///
    /// # Errors
    ///
    /// [`TransportError::Unreachable`] when nothing answers or the device
    /// refuses; [`TransportError::TimedOut`] when it answers too late.
    fn connect(&self, deadline: Duration) -> Result<Self::Connection, TransportError>;
}

/// One open connection to a device.
pub trait Connection {
    /// Write every byte, within `deadline` for the whole of them.
    ///
    /// # Errors
    ///
    /// [`WriteError`], carrying the bytes the transport accepted first.
    fn write_all(&mut self, bytes: &[u8], deadline: Duration) -> Result<(), WriteError>;

    /// Read what the device has sent, waiting at most `deadline`. `Ok(0)`
    /// means the device closed the connection.
    ///
    /// # Errors
    ///
    /// [`TransportError::TimedOut`] when nothing arrives in time.
    fn read(&mut self, buffer: &mut [u8], deadline: Duration) -> Result<usize, TransportError>;

    /// End the document: tell the device no more bytes are coming, and drain
    /// what it sends back for at most `deadline`, so that closing the
    /// connection never resets it over bytes still on their way.
    fn finish(&mut self, deadline: Duration);
}

/// Raw TCP to one address — port 9100 on every ESC/POS network printer.
///
/// It connects only to the address it is given. Port 9100 has no
/// authentication, so a register that printed to whatever answered a broadcast
/// could print a customer's receipt on a stranger's device; the address is the
/// one bound in the register's configuration, and nothing here discovers one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TcpTransport {
    address: SocketAddr,
}

impl TcpTransport {
    #[must_use]
    pub const fn new(address: SocketAddr) -> Self {
        Self { address }
    }

    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }
}

impl Transport for TcpTransport {
    type Connection = TcpConnection;

    fn connect(&self, deadline: Duration) -> Result<TcpConnection, TransportError> {
        let stream = TcpStream::connect_timeout(&self.address, deadline).map_err(|source| {
            if is_timeout(&source) {
                TransportError::TimedOut
            } else {
                TransportError::Unreachable {
                    address: self.address.to_string(),
                    source,
                }
            }
        })?;
        // A receipt is written in a few large writes and a status query is
        // three bytes that want an answer now: neither gains from Nagle.
        stream.set_nodelay(true)?;
        Ok(TcpConnection { stream })
    }
}

/// An open TCP connection to a printer.
#[derive(Debug)]
pub struct TcpConnection {
    stream: TcpStream,
}

/// The most one write hands the socket: small enough that an empty send buffer
/// always takes it whole.
const WRITE_PIECE: usize = 16 * 1024;

fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

/// What is left of `deadline` since `start`, or `None` once it has passed.
fn remaining(start: Instant, deadline: Duration) -> Option<Duration> {
    deadline
        .checked_sub(start.elapsed())
        .filter(|left| !left.is_zero())
}

impl Connection for TcpConnection {
    fn write_all(&mut self, bytes: &[u8], deadline: Duration) -> Result<(), WriteError> {
        let start = Instant::now();
        let mut accepted = 0;
        let fail = |accepted, failure| WriteError { accepted, failure };
        while let Some(rest) = bytes.get(accepted..).filter(|rest| !rest.is_empty()) {
            let Some(left) = remaining(start, deadline) else {
                return Err(fail(accepted, WriteFailure::TimedOut));
            };
            if self.stream.set_write_timeout(Some(left)).is_err() {
                return Err(fail(accepted, WriteFailure::Closed));
            }
            // One piece at a time, each smaller than any socket buffer. A
            // single blocking write of the whole document that meets a reset
            // fails without saying how much it had already queued, which would
            // report a device that took bytes as one that took none: `Failed`,
            // the outcome the queue retries, over paper that exists.
            let piece = rest.get(..rest.len().min(WRITE_PIECE)).unwrap_or(rest);
            match self.stream.write(piece) {
                Ok(0) => return Err(fail(accepted, WriteFailure::Closed)),
                Ok(written) => accepted += written,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if is_timeout(&error) => {
                    return Err(fail(accepted, WriteFailure::TimedOut));
                }
                Err(_) => return Err(fail(accepted, WriteFailure::Closed)),
            }
        }
        Ok(())
    }

    fn read(&mut self, buffer: &mut [u8], deadline: Duration) -> Result<usize, TransportError> {
        if deadline.is_zero() {
            return Err(TransportError::TimedOut);
        }
        self.stream.set_read_timeout(Some(deadline))?;
        loop {
            match self.stream.read(buffer) {
                Ok(read) => return Ok(read),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if is_timeout(&error) => return Err(TransportError::TimedOut),
                Err(error) => return Err(TransportError::Io(error)),
            }
        }
    }

    fn finish(&mut self, deadline: Duration) {
        // The document's bytes are all accepted; what follows only lets them
        // leave cleanly, so its errors change nothing about the outcome.
        if self.stream.shutdown(Shutdown::Write).is_err() {
            return;
        }
        let start = Instant::now();
        let mut sink = [0_u8; 256];
        while let Some(left) = remaining(start, deadline) {
            match self.read(&mut sink, left) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
        }
    }
}
