// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod tray;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![greet])
        .setup(|app| {
            tray::setup(app.handle())?;
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => tray::open_panel(app, "jadwal"),
            "settings" => tray::open_panel(app, "settings"),
            "autostart" => {} // dihubungkan pada Task 7
            "quit" => app.exit(0),
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
