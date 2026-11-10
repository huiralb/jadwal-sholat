// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod api;
mod praytimes;
mod scheduler;
mod store;
mod tray;

use tauri::Manager;

use crate::praytimes::{Kota, Settings, TodaySchedule};

#[tauri::command]
fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn get_settings(app: tauri::AppHandle) -> Settings {
    store::load(&app)
}

#[tauri::command]
fn set_setting(
    app: tauri::AppHandle,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    store::set(&app, &key, value)
}

#[tauri::command]
async fn get_today_schedule(app: tauri::AppHandle) -> Result<TodaySchedule, String> {
    scheduler::current_schedule(&app).await
}

#[tauri::command]
async fn search_kota(app: tauri::AppHandle, q: String) -> Result<Vec<Kota>, String> {
    let list = api::load_kota_list(&app).await?;
    Ok(praytimes::filter_kota(&list, &q))
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
        .invoke_handler(tauri::generate_handler![
            quit,
            get_settings,
            set_setting,
            get_today_schedule,
            search_kota
        ])
        .setup(|app| {
            tray::setup(app.handle())?;
            scheduler::spawn(app.handle().clone());
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
