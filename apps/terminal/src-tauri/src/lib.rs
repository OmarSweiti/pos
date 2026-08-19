// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// First proof of the blueprint's core boundary: UI → Tauri IPC → pure Rust domain.
#[tauri::command]
fn split_tender(total_minor: i64, parts: u32) -> Result<Vec<i64>, String> {
    pos_domain::Money::from_minor(total_minor)
        .split_evenly(parts)
        .map(|v| v.into_iter().map(|m| m.minor()).collect())
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, split_tender])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
