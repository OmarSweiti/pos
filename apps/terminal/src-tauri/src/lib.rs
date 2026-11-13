//! Tauri shell: the UI → IPC → pure-domain boundary.
//! Business rules live in `pos-domain`; this crate only marshals.

pub mod backup_service;
pub mod commands;
pub mod health;
pub mod ipc;
pub(crate) mod payload;
pub mod print_queue;
pub mod print_service;
pub mod provisioning;
pub mod recovery;
pub mod scan;
pub mod state;
pub mod telemetry;
pub mod time;

// The integration suites' shop, for the finalize's own tests, which must
// reach the crate's fault seam (1.8.3). The shop names this crate as the
// suites do, `terminal_lib`.
#[cfg(test)]
extern crate self as terminal_lib;
#[cfg(test)]
#[path = "../tests/shop/mod.rs"]
mod shop;

use ipc::{IpcError, IpcResult};
use tauri::Manager;

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

// Every command the webview can invoke, named once: the handler `run` installs
// and the list the IPC contract test walks are both made from this list
// (`ipc::registry`, 1.6.7). A command added here needs its row in
// `ipc::registry::COMMANDS`.
ipc::registry::commands![
    split_tender,
    commands::auth::session_state,
    commands::auth::auth_login_pin,
    commands::auth::auth_logout,
    commands::cart::cart_open,
    commands::cart::cart_current,
    commands::cart::cart_add_line,
    commands::cart::cart_add_scan,
    commands::cart::cart_confirm_age,
    commands::cart::cart_override_price,
    commands::cart::cart_park,
    commands::cart::cart_list_parked,
    commands::cart::cart_resume,
    commands::tender::tender_begin,
    commands::tender::tender_add_cash,
    commands::tender::tender_remove,
    commands::tender::tender_back_to_cart,
    commands::tender::sale_in_flight_state,
    commands::tender::sale_resume_in_flight,
    commands::sale::sale_finalize,
    commands::shift::shift_open,
    commands::shift::shift_current,
    commands::shift::shift_close,
    commands::user::user_reset_pin,
    commands::stock::stock_on_hand,
    commands::stock::stock_adjust_now,
    commands::stock::stock_adjust_prepare,
    commands::stock::stock_adjust,
    health::health_status,
    provisioning::provision_recovery_code,
    provisioning::print_recovery_code,
    provisioning::acknowledge_recovery_code,
];

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Every log line passes through the scrubber, before anything can log.
    telemetry::init();
    // `expect` is permitted here: a failure to start the webview is
    // unrecoverable and must be loud (conventions §4 exempts main/run).
    #[allow(clippy::expect_used)]
    tauri::Builder::default()
        // The register's state is opened once, before the first command
        // (1.8.2c). A refusal keeps the till closed and says why: every
        // command answers with it rather than with an empty till.
        .setup(|app| {
            let data_dir = app.path().app_data_dir().ok();
            let till = match &data_dir {
                Some(data_dir) => state::Till::open(data_dir),
                None => state::Till::from_result(Err(state::StartupError::DataDirUnavailable)),
            };
            // The print worker, off the command path (1.7.14c). A worker that
            // cannot start leaves every receipt kept and queued, as with no
            // printer: selling never stops for one.
            if let state::Till::Open(register) = &till {
                match print_service::start(register, print_service::POLL) {
                    Ok(worker) => {
                        app.manage(worker);
                    }
                    Err(error) => tracing::error!(error = %error, "the print worker did not start"),
                }
            }
            // The backup worker, off the command path, hourly (1.8.6). One
            // that cannot start stops no sale; the log says so.
            if let (state::Till::Open(register), Some(data_dir)) = (&till, &data_dir) {
                match backup_service::start(
                    register,
                    data_dir,
                    provisioning::KeyCustody::os(),
                    backup_service::EVERY,
                ) {
                    Ok(worker) => {
                        app.manage(worker);
                    }
                    Err(error) => {
                        tracing::error!(error = %error, "the backup worker did not start")
                    }
                }
            }
            app.manage(till);
            Ok(())
        })
        .invoke_handler(invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
