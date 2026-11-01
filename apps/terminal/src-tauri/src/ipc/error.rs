//! What a failed command tells the webview, and what it keeps from it
//! (`ref/ipc-contract.md` §1, microstep 1.6.8).
//!
//! The webview gets a code to branch on, a key to translate, at most a
//! **static** detail, and a trace id. It never gets the source error. A SQL
//! error carrying a bound value, a provider's failure body or a panic message
//! would otherwise reach the screen, the screenshot a cashier sends, and the
//! JavaScript crash reporter, none of which the log scrubber sees. `detail`
//! is a `&'static str`, so building one from a source error does not compile.
//! The source goes to the scrubbed log, under the same trace id, which is what
//! support asks for.
//!
//! `ts_rs::TS` is not derived yet: the TypeScript generator lane does not
//! exist (`ref/ipc-contract.md` §1, rule 3), and the first command that
//! returns this to new screen code adds it.

use serde::Serialize;
use uuid::Uuid;

/// A command's failure, as the webview receives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IpcError {
    /// What the screen branches on.
    pub code: &'static str,
    /// What the screen translates.
    pub message_key: String,
    /// A diagnostic from a fixed set, never a formatted source error.
    pub detail: Option<&'static str>,
    /// Finds this failure in the scrubbed log. It carries nothing of its own.
    pub trace_id: Uuid,
}

/// What every command returns.
pub type IpcResult<T> = Result<T, IpcError>;

impl IpcError {
    /// A failure with a fresh trace id and no detail.
    pub fn new(code: &'static str, message_key: impl Into<String>) -> Self {
        Self {
            code,
            message_key: message_key.into(),
            detail: None,
            trace_id: trace_id(),
        }
    }

    /// The same failure, with a static detail.
    #[must_use]
    pub fn with_detail(mut self, detail: &'static str) -> Self {
        self.detail = Some(detail);
        self
    }

    /// A failure caused by `source`: the source is logged through the
    /// scrubbing layer under the trace id, and the webview gets only the code.
    pub fn from_source(
        code: &'static str,
        message_key: impl Into<String>,
        source: &dyn std::error::Error,
    ) -> Self {
        let failure = Self::new(code, message_key);
        tracing::error!(
            trace_id = %failure.trace_id,
            code = failure.code,
            source = %source,
            "a command failed"
        );
        failure
    }
}

/// A random trace id. An OS with no randomness to give still yields a valid
/// id, so a failure is never hidden by its own reporting.
fn trace_id() -> Uuid {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    uuid::Builder::from_random_bytes(bytes).into_uuid()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::io::{self, Write};
    use std::sync::{Arc, Mutex, PoisonError};

    use tracing_subscriber::layer::SubscriberExt;

    use super::*;
    use crate::telemetry::ScrubbingLayer;

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

    /// A database refusal whose message carries a bound value reaches the
    /// scrubbed log under the trace id, and the webview receives only the code,
    /// the key, a static detail and the same trace id.
    #[test]
    fn ipc_errors_carry_no_source_detail_in_release() {
        let bound = "SYNTH-bound-value-0790000000";
        let source = io::Error::other(format!(
            "UNIQUE constraint failed: customer.phone = {bound}"
        ));
        let sink = Captured::default();
        let subscriber = tracing_subscriber::registry().with(ScrubbingLayer::new(sink.clone()));
        let failure = tracing::subscriber::with_default(subscriber, || {
            IpcError::from_source("customer_save_failed", "error.customer.save", &source)
                .with_detail("a customer with that phone already exists")
        });
        let sent = serde_json::to_value(&failure).unwrap();
        assert!(!sent.to_string().contains(bound), "{sent}");
        assert!(!sent.to_string().contains("UNIQUE"), "{sent}");
        assert_eq!(
            sent,
            serde_json::json!({
                "code": "customer_save_failed",
                "message_key": "error.customer.save",
                "detail": "a customer with that phone already exists",
                "trace_id": failure.trace_id.to_string(),
            })
        );
        let log = String::from_utf8(sink.0.lock().unwrap().clone()).unwrap();
        assert!(log.contains(&failure.trace_id.to_string()), "{log}");
        assert!(log.contains("UNIQUE constraint failed"), "{log}");
    }

    /// Two failures are two trace ids, each a valid random id.
    #[test]
    fn ipc_errors_each_carry_their_own_trace_id() {
        let (a, b) = (IpcError::new("x", "k"), IpcError::new("x", "k"));
        assert_ne!(a.trace_id, b.trace_id);
        assert_eq!(a.trace_id.get_version_num(), 4);
        assert_eq!((a.detail, b.detail), (None, None));
    }
}
