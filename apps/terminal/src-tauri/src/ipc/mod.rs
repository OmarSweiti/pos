//! The typed boundary between the webview and the core (`ref/ipc-contract.md`).
//!
//! Its error type arrived first, with the log scrubber (1.6.8); the command
//! registry is 1.6.7's.

pub mod error;

pub use error::{IpcError, IpcResult};
