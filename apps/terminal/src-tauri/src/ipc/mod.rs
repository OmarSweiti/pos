//! The typed boundary between the webview and the core (`ref/ipc-contract.md`).
//!
//! Its error type arrived first, with the log scrubber (1.6.8). The command
//! registry, which says what each command's handler must prove, is 1.6.7's.
//! The context every command takes its proof from is 1.8.2c's.

pub mod context;
pub mod error;
pub mod registry;

pub use error::{IpcError, IpcResult};
