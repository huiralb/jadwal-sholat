// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod tray;

use tauri::Manager;

#[tauri::command]
fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![quit])
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
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|app, event| {
        // Panel hanya hidup saat fokus: focus hilang → sembunyikan.
        if let tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::Focused(false),
            ..
        } = event
        {
            if label == "main" {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.hide();
                }
            }
        }
    });
}
