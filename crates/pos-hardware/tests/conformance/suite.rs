//! The receipt-printer contract — `ref/hardware-and-receipts.md` §3 and §4 —
//! written once, against `ReceiptPrinter` and a profile. Every expectation is
//! read from the profile's own facts, never from a list of exceptions: what a
//! file knows, the emulator must do; what it does not know, the emulator must
//! refuse and name.

use std::time::Duration;

use pos_hardware::emulator::{ConnectionFault, Event};
use pos_hardware::escpos::encode;
use pos_hardware::profile::{
    DrawerPin, DrawerPulse, Fact, PrinterProfile, RasterCommand, StatusProtocol,
    Transport as ProfileTransport,
};
use pos_hardware::render::{Bitmap, PaperWidth, render_receipt};
use pos_hardware::{
    FailReason, HwError, PrintOutcome, PrinterStatus, ReceiptPrinter, RenderedReceipt,
};

use crate::receipt::receipt;
use crate::target::{EmulatorTarget, driver, nobody};

/// One case's result for one profile, as the report prints it.
pub struct Case {
    pub name: &'static str,
    pub observed: String,
}

/// The paper a profile's page is drawn for.
pub fn paper(profile: &PrinterProfile) -> PaperWidth {
    match profile.paper_mm {
        Fact::Known(58) => PaperWidth::Mm58,
        _ => PaperWidth::Mm80,
    }
}

/// The parameters a profile leaves unknown, by the names the emulator uses.
fn unresolved(profile: &PrinterProfile) -> Vec<String> {
    profile
        .unresolved()
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn unknowns(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::UnknownBehaviour { what } => Some(what.clone()),
            _ => None,
        })
        .collect()
}

fn connections(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::Connected { .. }))
        .count()
}

fn pulses(events: &[Event]) -> Vec<&Event> {
    events
        .iter()
        .filter(|e| matches!(e, Event::DrawerPulse { .. }))
        .collect()
}

/// The document a profile is sent: framed for the profile where its file
/// allows, and otherwise framed as the simulator frames it (in the profile's
/// own raster command where that is known), so the emulator still meets a
/// real raster and has to say what it does not know.
pub fn document(profile: &PrinterProfile, page: &Bitmap) -> (RenderedReceipt, Option<String>) {
    match encode(page, profile) {
        Ok(bytes) => (RenderedReceipt { bytes }, None),
        Err(refused) => {
            let mut framing = PrinterProfile::simulator(paper(profile));
            if let Fact::Known(command) = profile.raster_command {
                framing.raster_command = Fact::Known(command);
            }
            let bytes = encode(page, &framing).expect("the simulator frames any page");
            (RenderedReceipt { bytes }, Some(refused.to_string()))
        }
    }
}

/// The whole contract for one profile. Panics, naming the profile and the
/// case, at the first disagreement.
#[allow(clippy::too_many_lines)]
pub fn run(profile: &PrinterProfile) -> Vec<Case> {
    let id = profile.profile_id.clone();
    let mut cases = Vec::new();
    let target = EmulatorTarget::start(profile.clone());

    // ── 0 · the transport. The network driver drives a network printer, and
    // refuses a profile its device is reached some other way.
    let printer = match target.driver(profile) {
        Ok(printer) => printer,
        Err(HwError::WrongTransport { transport, .. }) => {
            assert!(
                matches!(profile.transport, Fact::Known(t) if t != ProfileTransport::Tcp9100),
                "{id}: refused a transport its file does not state"
            );
            cases.push(Case {
                name: "transport",
                observed: format!(
                    "reached by {transport}: refused by the network driver before a connection; \
                     its cases wait for that transport"
                ),
            });
            assert_eq!(
                connections(&target.events()),
                0,
                "{id}: a refused profile connected"
            );
            return cases;
        }
        Err(other) => panic!("{id}: the driver refused for another reason: {other}"),
    };
    cases.push(Case {
        name: "transport",
        observed: format!("{:?}: driven over TCP", profile.transport),
    });

    // ── 1 · status: read, or reported unsupported (§3 rule 1).
    let from = target.events().len();
    let status = printer.status();
    target.settle();
    let events: Vec<Event> = target.events().into_iter().skip(from).collect();
    let observed = match profile.status_protocol {
        Fact::Known(StatusProtocol::DleEot) => {
            assert_eq!(status, PrinterStatus::Ready, "{id}: status");
            let replies: Vec<Option<u8>> = events
                .iter()
                .filter_map(|e| match e {
                    Event::StatusAsked { reply, .. } => Some(*reply),
                    _ => None,
                })
                .collect();
            assert_eq!(
                replies,
                [Some(0x12); 4],
                "{id}: DLE EOT 1–4 of a ready device"
            );
            "Ready, from DLE EOT 1–4 (12 12 12 12)".to_owned()
        }
        Fact::Known(StatusProtocol::None) => {
            assert_eq!(status, PrinterStatus::Unsupported, "{id}: status");
            assert_eq!(
                connections(&events),
                0,
                "{id}: a device without status was asked"
            );
            "Unsupported, without asking: its file says it has no status".to_owned()
        }
        Fact::Unknown => {
            assert_eq!(status, PrinterStatus::Unsupported, "{id}: status");
            assert_eq!(
                unknowns(&events),
                ["status_protocol"],
                "{id}: unknown status"
            );
            "Unsupported: asked, and the emulator answered nothing and named status_protocol"
                .to_owned()
        }
    };
    cases.push(Case {
        name: "status",
        observed,
    });

    // ── 2 · a document, whole — or, where the file cannot frame one, nothing
    // printed and every unknown named.
    let page = render_receipt(&receipt(), paper(profile)).expect("renders");
    let (doc, refused) = document(profile, &page);
    let from = target.events().len();
    let outcome = printer.print(&doc);
    target.settle();
    let events: Vec<Event> = target.events().into_iter().skip(from).collect();
    assert_eq!(
        outcome,
        PrintOutcome::Printed,
        "{id}: the socket takes a whole document"
    );
    let named = unknowns(&events);
    let open = unresolved(profile);
    for what in &named {
        assert!(
            open.contains(what),
            "{id}: named {what}, which its file states ({open:?} are unknown)"
        );
    }
    let observed = match &refused {
        None => {
            let jobs = target.jobs();
            let [job] = jobs.as_slice() else {
                panic!("{id}: {} jobs for one document", jobs.len())
            };
            assert_eq!(job.page, page, "{id}: the printed page is not the bitmap");
            let limit = match profile.raster_max_rows {
                Fact::Known(limit) => limit.get(),
                Fact::Unknown => panic!("{id}: framed a page against an unknown limit"),
            };
            let strips: Vec<u32> = events
                .iter()
                .filter_map(|e| match e {
                    Event::Raster { rows, .. } => Some(*rows),
                    _ => None,
                })
                .collect();
            assert!(
                strips.iter().all(|rows| *rows <= limit),
                "{id}: a strip over {limit}"
            );
            format!(
                "Printed: one {}×{} page, dot for dot, in {} strip(s); unknowns named: {}",
                page.width_px,
                page.height_px,
                strips.len(),
                if named.is_empty() {
                    "none".to_owned()
                } else {
                    named.join(", ")
                }
            )
        }
        Some(refused) => {
            assert!(
                target.jobs().is_empty(),
                "{id}: printed a page its file cannot frame"
            );
            assert!(
                !named.is_empty(),
                "{id}: printed nothing and named no reason"
            );
            format!(
                "the encoder refused (\"{refused}\"); sent framed as the simulator frames it, \
                 nothing printed, and the emulator named {}",
                named.join(", ")
            )
        }
    };
    cases.push(Case {
        name: "document",
        observed,
    });

    // ── 3 · strips at the profile's limit (case 93).
    if refused.is_none() {
        let limit = match profile.raster_max_rows {
            Fact::Known(limit) => limit.get(),
            Fact::Unknown => unreachable!("a framed page has a known limit"),
        };
        let tall = Bitmap {
            width_px: page.width_px,
            height_px: limit * 2 + 1,
            bits: (0..(limit * 2 + 1) * page.row_bytes())
                .map(|i| if i % 3 == 0 { 0xA5 } else { 0x0F })
                .collect(),
        };
        let strips = RenderedReceipt {
            bytes: encode(&tall, profile).expect("encodes"),
        };
        let from = target.events().len();
        let jobs = target.jobs().len();
        assert_eq!(
            printer.print(&strips),
            PrintOutcome::Printed,
            "{id}: a tall page"
        );
        target.settle();
        let events: Vec<Event> = target.events().into_iter().skip(from).collect();
        let printed = target.jobs();
        assert_eq!(printed.len(), jobs + 1, "{id}: the tall page printed once");
        assert_eq!(
            printed.last().map(|job| &job.page),
            Some(&tall),
            "{id}: the tall page"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::RasterTooTall { .. })),
            "{id}: a strip the device refuses"
        );
        let mut observed = format!(
            "a page of {} rows printed whole in strips of at most {limit}",
            tall.height_px
        );
        if profile.raster_command == Fact::Known(RasterCommand::GsV0) {
            let from = target.events().len();
            let over = raw_gs_v_0(page.row_bytes(), limit + 1);
            assert_eq!(
                printer.print(&over),
                PrintOutcome::Printed,
                "{id}: over the limit"
            );
            target.settle();
            let refused: Vec<Event> = target
                .events()
                .into_iter()
                .skip(from)
                .filter(|e| matches!(e, Event::RasterTooTall { .. }))
                .collect();
            assert_eq!(refused.len(), 1, "{id}: {} rows in one command", limit + 1);
            observed.push_str(&format!("; one GS v 0 of {} rows refused", limit + 1));
        }
        cases.push(Case {
            name: "strips at the limit",
            observed,
        });
    }

    // ── 4 · a device that cannot print: refused before a byte where the
    // device can say so; where it cannot, the socket takes the document and
    // the device does not print it — §3 rule 1's stated limit.
    let mut seen = Vec::new();
    for (fault, reason) in [
        (ConnectionFault::PaperOut, FailReason::PaperOut),
        (ConnectionFault::CoverOpen, FailReason::CoverOpen),
        (ConnectionFault::Offline, FailReason::Offline),
    ] {
        let jobs = target.jobs().len();
        target.script(fault);
        let outcome = printer.print(&doc);
        target.settle();
        assert_eq!(target.jobs().len(), jobs, "{id}: printed under {fault:?}");
        match profile.status_protocol {
            Fact::Known(StatusProtocol::DleEot) => {
                assert_eq!(outcome, PrintOutcome::Failed(reason), "{id}: {fault:?}");
            }
            _ => assert_eq!(outcome, PrintOutcome::Printed, "{id}: {fault:?} unseen"),
        }
        seen.push(format!("{fault:?} → {outcome:?}"));
    }
    let observed = match profile.status_protocol {
        Fact::Known(StatusProtocol::DleEot) => format!("{}; nothing printed", seen.join(", ")),
        _ => format!(
            "{}: a device without readable status cannot refuse before the bytes go; \
             the emulator printed nothing",
            seen.join(", ")
        ),
    };
    cases.push(Case {
        name: "paper-out, cover-open, offline",
        observed,
    });

    // ── 5 · paper pulled mid-print: the device stops taking bytes. The
    // outcome is Unknown — paper may exist — and never retried (§3 rule 4).
    target.script(ConnectionFault::SilentAfter(4096));
    let outcome = target.impatient().print(&RenderedReceipt {
        bytes: vec![0x1B; 32 << 20],
    });
    assert_eq!(
        outcome,
        PrintOutcome::Unknown,
        "{id}: a device that stopped reading"
    );
    cases.push(Case {
        name: "paper pulled mid-print",
        observed: "Unknown: the device stopped reading 4096 bytes in".to_owned(),
    });

    // ── 6 · the drawer: one pulse per opening, and none from any print (§4).
    let from = target.events().len();
    let observed = match printer.open_drawer() {
        Ok(()) => panic!("{id}: opened a drawer its file does not describe"),
        Err(refused) => {
            assert!(
                refused.to_string().contains("drawer_pulse"),
                "{id}: {refused}"
            );
            let since: Vec<Event> = target.events().into_iter().skip(from).collect();
            assert_eq!(connections(&since), 0, "{id}: refused, yet it connected");
            let pulse = DrawerPulse {
                pin: DrawerPin::Pin2,
                on_ms: 75,
                off_ms: 150,
            };
            let mut known = profile.clone();
            known.drawer_pulse = Fact::Known(pulse);
            driver(target.emulator.address(), &known, Duration::from_secs(10))
                .expect("a network profile")
                .open_drawer()
                .expect("a known pulse");
            // Wait for the pulse, not for every connection: the silent device
            // of the case before never closes.
            target.until(|events| !pulses(events).is_empty());
            let events = target.events();
            let all = pulses(&events);
            assert_eq!(all.len(), 1, "{id}: pulses across every case: {all:?}");
            assert!(
                matches!(
                    all.first(),
                    Some(Event::DrawerPulse {
                        pin: DrawerPin::Pin2,
                        on_ms: 76,
                        off_ms: 150,
                        ..
                    })
                ),
                "{id}: the pulse as sent: {all:?}"
            );
            "its file's pulse is unknown, so open_drawer() is refused before a connection; with \
             a harness pulse (pin 2, 75/150 ms) exactly one ESC p arrives, 76/150 ms, and no \
             print emitted one"
                .to_owned()
        }
    };
    cases.push(Case {
        name: "drawer",
        observed,
    });

    // ── 7 · no printer there at all.
    let absent = driver(nobody(), profile, Duration::from_secs(10)).expect("a network profile");
    let outcome = absent.print(&doc);
    let status = absent.status();
    assert_eq!(
        outcome,
        PrintOutcome::Failed(FailReason::Unreachable),
        "{id}: absent"
    );
    let expected = match profile.status_protocol {
        Fact::Known(StatusProtocol::None) => PrinterStatus::Unsupported,
        _ => PrinterStatus::Offline,
    };
    assert_eq!(status, expected, "{id}: status of an absent printer");
    cases.push(Case {
        name: "no printer",
        observed: format!("{outcome:?}; status {status:?}"),
    });

    cases
}

/// The cases a physical device can answer without a fault script, at the
/// hardware gate. The paper is read by the person running it. Where the
/// profile does not know a fact, the device's answer is reported for the gate
/// to record in a new profile version, not judged.
pub fn run_on_device(profile: &PrinterProfile, address: std::net::SocketAddr) -> Vec<Case> {
    let id = &profile.profile_id;
    let printer = driver(address, profile, Duration::from_secs(30))
        .unwrap_or_else(|refused| panic!("{id}: {refused}"));
    let mut cases = Vec::new();

    let status = printer.status();
    let observed = match profile.status_protocol {
        Fact::Known(StatusProtocol::DleEot) => {
            assert_eq!(
                status,
                PrinterStatus::Ready,
                "{id}: a ready device's status"
            );
            format!("{status:?}")
        }
        Fact::Known(StatusProtocol::None) => {
            assert_eq!(
                status,
                PrinterStatus::Unsupported,
                "{id}: a device without status"
            );
            format!("{status:?}, without asking")
        }
        Fact::Unknown => format!("{status:?} — its file does not know: record what the device did"),
    };
    cases.push(Case {
        name: "status",
        observed,
    });

    let page = render_receipt(&receipt(), paper(profile)).expect("renders");
    let bytes = encode(&page, profile)
        .unwrap_or_else(|refused| panic!("{id}: its file cannot frame a page: {refused}"));
    let outcome = printer.print(&RenderedReceipt { bytes });
    assert_eq!(outcome, PrintOutcome::Printed, "{id}: the receipt");
    cases.push(Case {
        name: "document",
        observed: "Printed — read the paper: one receipt, whole, cut once".to_owned(),
    });

    let observed = match printer.open_drawer() {
        Ok(()) => "one pulse sent — count the drawer's openings: exactly one".to_owned(),
        Err(refused) => {
            assert!(profile.drawer_pulse.is_unknown(), "{id}: {refused}");
            format!("refused, as its file's pulse is unknown: {refused}")
        }
    };
    cases.push(Case {
        name: "drawer",
        observed,
    });
    cases
}

/// One `GS v 0` of `rows` rows, raw: what a driver that ignored the limit
/// would send.
fn raw_gs_v_0(row_bytes: u32, rows: u32) -> RenderedReceipt {
    let width = u16::try_from(row_bytes).expect("a row fits 16 bits");
    let height = u16::try_from(rows).expect("rows fit 16 bits");
    let mut bytes = vec![0x1B, 0x40, 0x1D, 0x76, 0x30, 0x00];
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend(std::iter::repeat_n(
        0xAA,
        usize::from(width) * usize::from(height),
    ));
    bytes.extend_from_slice(&[0x1D, 0x56, 0x01]);
    RenderedReceipt { bytes }
}
