//! The ESC/POS commands the emulated printer understands, read from a byte
//! stream that may stop mid-command.
//!
//! It knows each command's length before it acts on it — a raster command's
//! payload is counted, never scanned — so a payload whose bytes happen to spell
//! `ESC p` or `DLE EOT` is a payload, exactly as it is to a device.

use core::ops::Range;

/// One command, and the bytes of the stream it spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `ESC @`.
    Initialise,
    /// `GS v 0 m xL xH yL yH d…`: `width_bytes` across, `rows` down.
    RasterBitImage {
        mode: u8,
        width_bytes: u32,
        rows: u32,
        data: Range<usize>,
    },
    /// `GS ( L` function 112: store a raster in the graphics buffer.
    GraphicsStore {
        /// `a bx by c` — tone, horizontal and vertical scale, colour.
        format: [u8; 4],
        width_dots: u32,
        rows: u32,
        data: Range<usize>,
    },
    /// `GS ( L` function 50: print the graphics buffer.
    GraphicsPrint,
    /// Any other `GS ( L` function, skipped by its own count.
    GraphicsOther { function: u8 },
    /// `GS V m [n]`.
    Cut { mode: u8 },
    /// `ESC p m t1 t2`.
    DrawerPulse { mode: u8, on: u8, off: u8 },
    /// `DLE EOT n`.
    StatusRequest { function: u8 },
    /// `ESC d n` — feed `n` lines.
    Feed { lines: u8 },
    /// A byte that begins nothing this device knows.
    Unrecognised { byte: u8 },
}

/// What the start of a stream holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// A whole command, `length` bytes long.
    Command { command: Command, length: usize },
    /// The stream stops inside a command: read more.
    Incomplete,
}

fn byte(stream: &[u8], at: usize) -> Option<u8> {
    stream.get(at).copied()
}

fn word(stream: &[u8], at: usize) -> Option<u32> {
    let low = u32::from(byte(stream, at)?);
    let high = u32::from(byte(stream, at + 1)?);
    Some(low | high << 8)
}

const fn whole(command: Command, length: usize) -> Parsed {
    Parsed::Command { command, length }
}

/// Read the command at the start of `stream`.
#[must_use]
pub fn parse(stream: &[u8]) -> Parsed {
    let Some(first) = byte(stream, 0) else {
        return Parsed::Incomplete;
    };
    let parsed = match first {
        0x1B => parse_escape(stream),
        0x1D => parse_group_separator(stream),
        0x10 => match (byte(stream, 1), byte(stream, 2)) {
            (Some(0x04), Some(function)) => Some(whole(Command::StatusRequest { function }, 3)),
            (Some(0x04), None) | (None, _) => None,
            (Some(_), _) => Some(whole(Command::Unrecognised { byte: first }, 1)),
        },
        _ => Some(whole(Command::Unrecognised { byte: first }, 1)),
    };
    parsed.unwrap_or(Parsed::Incomplete)
}

/// `ESC …`; `None` while the command is incomplete.
fn parse_escape(stream: &[u8]) -> Option<Parsed> {
    let parsed = match byte(stream, 1)? {
        0x40 => whole(Command::Initialise, 2),
        0x64 => whole(
            Command::Feed {
                lines: byte(stream, 2)?,
            },
            3,
        ),
        0x70 => whole(
            Command::DrawerPulse {
                mode: byte(stream, 2)?,
                on: byte(stream, 3)?,
                off: byte(stream, 4)?,
            },
            5,
        ),
        _ => whole(Command::Unrecognised { byte: 0x1B }, 1),
    };
    Some(parsed)
}

/// `GS …`; `None` while the command is incomplete.
fn parse_group_separator(stream: &[u8]) -> Option<Parsed> {
    let parsed = match byte(stream, 1)? {
        // GS v 0 m xL xH yL yH d1…dk
        0x76 => {
            if byte(stream, 2)? != 0x30 {
                return Some(whole(Command::Unrecognised { byte: 0x1D }, 1));
            }
            let mode = byte(stream, 3)?;
            let width_bytes = word(stream, 4)?;
            let rows = word(stream, 6)?;
            let payload = usize::try_from(width_bytes * rows).ok()?;
            let end = 8 + payload;
            if stream.len() < end {
                return None;
            }
            whole(
                Command::RasterBitImage {
                    mode,
                    width_bytes,
                    rows,
                    data: 8..end,
                },
                end,
            )
        }
        // GS ( L pL pH m fn …: the count is every byte after pH.
        0x28 if byte(stream, 2)? == 0x4C => {
            let count = usize::try_from(word(stream, 3)?).ok()?;
            let end = 5 + count;
            if stream.len() < end {
                return None;
            }
            let function = if count >= 2 { byte(stream, 6)? } else { 0 };
            let command = match (byte(stream, 5), function) {
                (Some(0x30), 0x70) if count >= 10 => Command::GraphicsStore {
                    format: [
                        byte(stream, 7)?,
                        byte(stream, 8)?,
                        byte(stream, 9)?,
                        byte(stream, 10)?,
                    ],
                    width_dots: word(stream, 11)?,
                    rows: word(stream, 13)?,
                    data: 15..end,
                },
                (Some(0x30), 0x32) => Command::GraphicsPrint,
                _ => Command::GraphicsOther { function },
            };
            whole(command, end)
        }
        // GS V m, or GS V m n for the feed-and-cut modes.
        0x56 => {
            let mode = byte(stream, 2)?;
            match mode {
                0 | 1 | 48 | 49 => whole(Command::Cut { mode }, 3),
                65 | 66 => {
                    byte(stream, 3)?;
                    whole(Command::Cut { mode }, 4)
                }
                _ => whole(Command::Unrecognised { byte: 0x1D }, 1),
            }
        }
        _ => whole(Command::Unrecognised { byte: 0x1D }, 1),
    };
    Some(parsed)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn each_command_is_read_whole_or_not_at_all() {
        let raster = [
            0x1D, 0x76, 0x30, 0x00, 0x02, 0x00, 0x03, 0x00, 1, 2, 3, 4, 5, 6,
        ];
        for cut in 1..raster.len() {
            assert_eq!(
                parse(raster.get(..cut).unwrap()),
                Parsed::Incomplete,
                "{cut}"
            );
        }
        assert_eq!(
            parse(&raster),
            whole(
                Command::RasterBitImage {
                    mode: 0,
                    width_bytes: 2,
                    rows: 3,
                    data: 8..14
                },
                14
            )
        );
        assert_eq!(parse(&[0x1B, 0x40]), whole(Command::Initialise, 2));
        assert_eq!(
            parse(&[0x1B, 0x70, 0x01, 0x19, 0xFA]),
            whole(
                Command::DrawerPulse {
                    mode: 1,
                    on: 25,
                    off: 250
                },
                5
            )
        );
        assert_eq!(
            parse(&[0x10, 0x04, 0x04]),
            whole(Command::StatusRequest { function: 4 }, 3)
        );
        assert_eq!(
            parse(&[0x1D, 0x56, 0x01]),
            whole(Command::Cut { mode: 1 }, 3)
        );
        assert_eq!(
            parse(&[0x1D, 0x56, 0x42, 0x05]),
            whole(Command::Cut { mode: 66 }, 4)
        );
        assert_eq!(
            parse(&[0x1D, 0x28, 0x4C, 0x02, 0x00, 0x30, 0x32]),
            whole(Command::GraphicsPrint, 7)
        );
        assert_eq!(
            parse(&[0x41]),
            whole(Command::Unrecognised { byte: 0x41 }, 1)
        );
    }

    #[test]
    fn a_graphics_store_is_counted_by_its_parameters() {
        // count = 10 + 2 data bytes: 16 dots across, 1 row.
        let store = [
            0x1D, 0x28, 0x4C, 0x0C, 0x00, 0x30, 0x70, 0x30, 0x01, 0x01, 0x31, 0x10, 0x00, 0x01,
            0x00, 0xAA, 0x55,
        ];
        assert_eq!(
            parse(&store),
            whole(
                Command::GraphicsStore {
                    format: [0x30, 0x01, 0x01, 0x31],
                    width_dots: 16,
                    rows: 1,
                    data: 15..17
                },
                17
            )
        );
        assert_eq!(parse(store.get(..16).unwrap()), Parsed::Incomplete);
    }
}
