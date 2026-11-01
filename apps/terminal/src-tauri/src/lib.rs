//! Tauri shell: the UI → IPC → pure-domain boundary.
//! Business rules live in `pos-domain`; this crate only marshals.

pub mod ipc;
pub mod print_queue;
pub mod scan;
pub mod telemetry;
pub mod time;

use ipc::{IpcError, IpcResult};

/// First proof of the blueprint's core boundary: UI → Tauri IPC → pure Rust domain.
#[tauri::command]
fn split_tender(total_minor: i64, parts: u32) -> IpcResult<Vec<i64>> {
    // This Phase-0 smoke command proves the existing UI → IPC → domain boundary.
    // Use the product's home currency explicitly without widening that boundary.
    pos_domain::Money::from_minor(total_minor, pos_domain::Currency::JOD)
        .split_evenly(parts)
        .map(|v| v.into_iter().map(|m| m.minor()).collect())
        .map_err(|e| IpcError::from_source("money_split_failed", "error.money.split", &e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Every log line passes through the scrubber, before anything can log.
    telemetry::init();
    // `expect` is permitted here: a failure to start the webview is
    // unrecoverable and must be loud (conventions §4 exempts main/run).
    #[allow(clippy::expect_used)]
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![split_tender])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
