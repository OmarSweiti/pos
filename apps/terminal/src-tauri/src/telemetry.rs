//! The scrubbing layer: every log line the register writes passes through it
//! (gap G-8, microstep 1.6.8, `ref/security-compliance.md` §6).
//!
//! It redacts, at any depth, the value of every field whose name
//! [`pos_domain::SENSITIVE_FIELD_RULES`] lists: the one registry, which the
//! audit-payload guard reads too. The layer is the only formatter installed, so
//! nothing reaches a sink, stdout, a file or a crash reporter, without passing
//! through it.
//!
//! # What a name cannot catch
//!
//! A field's *name* is matched, so three channels need their own control:
//!
//! - **`IpcError.detail`** is serialized to the webview and never logged here,
//!   so it is a `&'static str` (`crate::ipc`).
//! - **A `Debug` rendering** is one opaque string. A type that holds a secret
//!   or a name writes its own `Debug` that redacts it, as `Cart` does for its
//!   buyer.
//! - **Free text** (a message, an error's text, a remote body) has no name to
//!   match. It is never built from a customer's details, and the canary test
//!   `no_pii_in_a_full_sale_trace` is what notices one that is.
//!
//! A payload is logged with `%`, which writes JSON, never `?`: a JSON string is
//! scrubbed as JSON, and `serde_json::Value`'s `Debug` is not JSON.

use std::fmt;
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};

use pos_domain::SENSITIVE_FIELD_RULES;
use serde_json::{Map, Value, json};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

/// What a redacted value is replaced with.
pub const REDACTED: &str = "[redacted]";

/// `value` with the value under every never-log name replaced by
/// [`REDACTED`], at any depth. A string that holds a JSON object or array is
/// scrubbed as JSON and written back, so a payload logged as text is not a way
/// around the registry.
pub fn scrub_json(value: &mut Value) {
    match value {
        Value::Object(members) => {
            for (name, member) in members.iter_mut() {
                if SENSITIVE_FIELD_RULES.matches(name) {
                    *member = Value::String(REDACTED.to_owned());
                } else {
                    scrub_json(member);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(scrub_json),
        Value::String(text) => {
            if let Some(mut inner) = embedded_json(text) {
                scrub_json(&mut inner);
                *text = inner.to_string();
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// The JSON object or array `text` holds, if it holds one.
fn embedded_json(text: &str) -> Option<Value> {
    let start = text.trim_start();
    if !(start.starts_with('{') || start.starts_with('[')) {
        return None;
    }
    serde_json::from_str::<Value>(text)
        .ok()
        .filter(|v| v.is_object() || v.is_array())
}

/// One event's fields, each redacted by name or scrubbed as JSON.
struct Fields<'a>(&'a mut Map<String, Value>);

impl Fields<'_> {
    fn put(&mut self, field: &Field, mut value: Value) {
        if SENSITIVE_FIELD_RULES.matches(field.name()) {
            value = Value::String(REDACTED.to_owned());
        } else {
            scrub_json(&mut value);
        }
        self.0.insert(field.name().to_owned(), value);
    }
}

impl Visit for Fields<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.put(field, Value::String(format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, Value::String(value.to_owned()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, json!(value));
    }
}

/// A `tracing` layer that writes each event as one scrubbed JSON line: its
/// level, target, the names of the spans it happened in, and its fields.
/// Span fields are never written, so a span is not a way around it either.
pub struct ScrubbingLayer {
    sink: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl ScrubbingLayer {
    /// A layer writing to `sink`.
    pub fn new(sink: impl Write + Send + 'static) -> Self {
        Self {
            sink: Arc::new(Mutex::new(Box::new(sink))),
        }
    }
}

impl<S> Layer<S> for ScrubbingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut fields = Map::new();
        event.record(&mut Fields(&mut fields));
        let spans: Vec<&str> = ctx
            .event_scope(event)
            .map(|scope| scope.from_root().map(|span| span.name()).collect())
            .unwrap_or_default();
        let meta = event.metadata();
        let line = json!({
            "level": meta.level().as_str(),
            "target": meta.target(),
            "spans": spans,
            "fields": fields,
        });
        let mut sink = self.sink.lock().unwrap_or_else(PoisonError::into_inner);
        // A log line that cannot be written is dropped: logging never stops a
        // sale.
        let _ = writeln!(sink, "{line}");
    }
}

/// The register's logging: [`ScrubbingLayer`] in front of standard error, at
/// `INFO` and above, installed once. A second call changes nothing.
pub fn init() {
    let subscriber = tracing_subscriber::registry()
        .with(ScrubbingLayer::new(std::io::stderr()).with_filter(LevelFilter::INFO));
    let _ = tracing::subscriber::set_global_default(subscriber);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::io;

    use pos_domain::cart::{self, AddLine, CartContext, add_line};
    use pos_domain::tax::{PriceMode, StoreTaxProfile};
    use pos_domain::{
        Currency, Money, Product, ProductId, Qty, RegisterId, RegulatedSaleForm, RoundingRule,
        SaleForm, SaleId, SaleLineId, ShiftId, TaxCategoryId, Timestamp, UnitOfMeasure, UserId,
    };
    use tracing_subscriber::layer::SubscriberExt;
    use uuid::Uuid;

    use super::*;

    /// A sink tests read back.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Captured {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    /// Everything `run` logs, through the layer.
    fn logged(run: impl FnOnce()) -> String {
        let sink = Captured::default();
        let subscriber = tracing_subscriber::registry().with(ScrubbingLayer::new(sink.clone()));
        tracing::subscriber::with_default(subscriber, run);
        sink.text()
    }

    const SECRET: &str = "SYNTH-secret-0790000000";

    /// Every name the registry lists, as an event field, is redacted: the test
    /// is generated from the registry, so a name added there is covered here.
    #[test]
    fn scrubber_redacts_every_known_pii_field() {
        for name in SENSITIVE_FIELD_RULES.names {
            let mut payload = json!({ "sale_id": "SYNTH-sale", *name: SECRET });
            scrub_json(&mut payload);
            assert_eq!(payload.get(*name), Some(&json!(REDACTED)), "{name}");
            assert_eq!(payload.get("sale_id"), Some(&json!("SYNTH-sale")), "{name}");
            let log = logged(|| {
                tracing::info!(payload = %json!({ *name: SECRET }), "a payload");
            });
            assert!(!log.contains(SECRET), "{name}: {log}");
        }
        // As a field of the event itself, by its own name.
        let log = logged(|| {
            tracing::info!(
                phone = SECRET,
                email = SECRET,
                pin = SECRET,
                sale = "SYNTH-sale"
            );
        });
        assert!(!log.contains(SECRET), "{log}");
        assert!(
            log.contains("SYNTH-sale") && log.contains(REDACTED),
            "{log}"
        );
    }

    /// Nesting does not evade it, in an object, an array, or a JSON string
    /// inside a JSON string.
    #[test]
    fn scrubber_redacts_nested_json() {
        let inner = json!({ "customer": { "name_ar": "x", "phone": SECRET } }).to_string();
        let mut payload = json!({
            "lines": [{ "sku": "SYNTH-1", "buyer": { "buyer_name": SECRET } }],
            "deep": { "a": { "b": { "c": [{ "card_number": SECRET }] } } },
            "as_text": inner,
        });
        scrub_json(&mut payload);
        let shown = payload.to_string();
        assert!(!shown.contains(SECRET), "{shown}");
        assert!(
            shown.contains("SYNTH-1") && shown.contains("name_ar"),
            "{shown}"
        );
        let log = logged(|| {
            tracing::info!(payload = %payload_with_secrets(), "nested");
        });
        assert!(!log.contains(SECRET), "{log}");
    }

    fn payload_with_secrets() -> Value {
        json!({ "a": [{ "b": { "email": SECRET, "device_token": SECRET } }] })
    }

    /// Every suffix and the contains rule, under names no exact rule lists.
    #[test]
    fn scrubber_redacts_every_suffix_rule() {
        let mut names: Vec<String> = SENSITIVE_FIELD_RULES
            .suffixes
            .iter()
            .map(|suffix| format!("device{suffix}"))
            .collect();
        names.extend(
            [
                "client_secret",
                "manager_pin",
                "pin_hash",
                "old_password_at",
                "cardNumber",
            ]
            .map(str::to_owned),
        );
        for name in &names {
            let mut payload = json!({ name.clone(): SECRET });
            scrub_json(&mut payload);
            assert_eq!(payload.get(name.as_str()), Some(&json!(REDACTED)), "{name}");
        }
        let log = logged(|| {
            tracing::info!(
                device_token = SECRET,
                client_secret = SECRET,
                pin_hash = SECRET
            );
        });
        assert!(!log.contains(SECRET), "{log}");
        // And innocent names are left alone.
        let mut kept = json!({ "masked_pan": "**** 1234", "pinned": true, "psp_ref": "SYNTH-psp" });
        let before = kept.clone();
        scrub_json(&mut kept);
        assert_eq!(kept, before);
    }

    /// `init` makes the layer the process's logger, at `INFO` and above, and
    /// a second call changes nothing. Each test runs in its own process under
    /// nextest, so this is the only test that installs a global default.
    #[test]
    fn init_installs_the_layer_as_the_global_default() {
        init();
        init();
        assert!(tracing::dispatcher::has_been_set());
        tracing::dispatcher::get_default(|dispatch| {
            assert!(dispatch.is::<ScrubbingLayer>());
        });
        assert!(tracing::enabled!(tracing::Level::INFO));
        assert!(!tracing::enabled!(tracing::Level::DEBUG));
    }

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, Currency::JOD)
    }

    /// A complete sale through the domain, with a named business buyer and a
    /// phone, logged the way the commands will log it: the cart by its `Debug`,
    /// the buyer by name, the payload and the audit intent as JSON, an error
    /// by its code. Not one of the fixture's personal details reaches the
    /// trace, while the sale itself plainly does. The commands that will emit
    /// these lines are 1.4.11's and 1.8.3's, and they log the same values.
    #[test]
    fn no_pii_in_a_full_sale_trace() {
        let name = "SYNTH-buyer-Sami-Haddad";
        let phone = "SYNTH-0790000000";
        let email = "synth.buyer@example.invalid";
        let at = Timestamp::from_epoch_milliseconds(978_307_320_000).unwrap();
        let id = |n: u128| Uuid::from_u128(n);
        let log = logged(|| {
            let ctx = CartContext {
                register_id: RegisterId::from_uuid(id(2)),
                shift_id: ShiftId::from_uuid(id(3)),
                cashier_id: UserId::from_uuid(id(4)),
                currency: Currency::JOD,
                is_training: false,
                opened_at: at,
            };
            let mut cart = cart::open(SaleId::from_uuid(id(1)), &ctx);
            cart.buyer_tin = Some("SYNTH-TIN-999".to_owned());
            cart.buyer_name = Some(name.to_owned());
            tracing::info!(sale_id = %cart.id.as_uuid(), buyer_name = name, phone, email, "buyer attached");
            let tea = Product {
                id: ProductId::from_uuid(id(0x20)),
                sku: "SYNTH-TEA".to_owned(),
                name_ar: "شاي اصطناعي".to_owned(),
                name_en: None,
                category_id: None,
                tax_category_id: TaxCategoryId::from_uuid(id(0x12)),
                unit: UnitOfMeasure::Each,
                unit_price: jod(1_250),
                is_weighed: false,
                is_service: false,
                is_active: true,
                min_age: None,
                max_price: None,
                reorder_point: None,
                regulated_sale_form: RegulatedSaleForm::Unregulated(SaleForm::SealedPack),
            };
            let line =
                AddLine::from_product(SaleLineId::from_uuid(id(0x1001)), &tea, Qty::ONE).unwrap();
            cart = add_line(cart, line).unwrap();
            tracing::info!(cart = ?cart, "line added");
            let receipt = json!({
                "sale_id": cart.id.as_uuid().to_string(),
                "buyer": { "tin": "SYNTH-TIN-999", "buyer_name": name, "phone": phone },
                "customer": { "customer_name": name, "email": email },
                "total_minor": 1_250,
            });
            tracing::info!(receipt = %receipt, price_mode = ?PriceMode::Inclusive, profile = ?StoreTaxProfile::Standard, rule = ?RoundingRule::HalfAwayFromZero, "receipt built");
            // The finalize's own line (1.8.3), as it logs a completed sale.
            crate::commands::sale::trace_finalized(&crate::commands::sale::CompletedSaleRef {
                sale_id: cart.id,
                receipt_number: "REG01-000001".to_owned(),
                currency: "JOD",
                total_minor: 1_250,
                change_minor: 0,
                receipt: crate::commands::sale::ReceiptOutcome::Queued,
                drawer: crate::commands::sale::DrawerOutcome::NoDevice,
            });
            let failure = crate::ipc::IpcError::from_source(
                "sale_finalize_failed",
                "error.sale.finalize",
                &io::Error::other("the printer refused"),
            );
            tracing::info!(code = failure.code, trace_id = %failure.trace_id, "sale finished");
        });
        for detail in [name, phone, email] {
            assert!(!log.contains(detail), "{detail} reached the trace: {log}");
        }
        assert!(
            log.contains("00000000-0000-0000-0000-000000000001"),
            "{log}"
        );
        assert!(
            log.contains("SYNTH-TIN-999") && log.contains("sale_finalize_failed"),
            "{log}"
        );
        assert!(
            log.contains("sale finalized") && log.contains("REG01-000001"),
            "the finalize's own line is in the trace: {log}"
        );
        assert!(log.lines().count() >= 5, "{log}");
    }
}
