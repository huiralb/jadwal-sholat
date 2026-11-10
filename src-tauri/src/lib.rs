// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod api;
mod praytimes;
mod store;
mod tray;

use chrono::{Datelike, Duration};
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
    let settings = store::load(&app);
    let kota_id = settings.kota_id.ok_or("lokasi belum dipilih")?;
    let today = chrono::Local::now().date_naive();
    let (year, month) = (today.year() as u32, today.month());

    let raw = api::load_month(&app, &kota_id, year, month).await?;
    let body = praytimes::parse_month(&raw).map_err(|e| e.to_string())?;
    let kota_nama = settings
        .kota_nama
        .unwrap_or_else(|| body.lokasi.clone());
    let today_str = today.format("%Y-%m-%d").to_string();
    let mut sched = praytimes::today_schedule(&body, &kota_nama, &today_str)
        .ok_or_else(|| "jadwal hari ini tidak tersedia".to_string())?;

    if sched.tomorrow_subuh.is_none() {
        // Hari terakhir bulan ini: Subuh besok ada di data bulan berikutnya.
        let tomorrow_str = (today + Duration::days(1)).format("%Y-%m-%d").to_string();
        let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
        if let Ok(next_raw) = api::load_month(&app, &kota_id, ny, nm).await {
            if let Ok(next_body) = praytimes::parse_month(&next_raw) {
                sched.tomorrow_subuh = next_body
                    .jadwal
                    .iter()
                    .find(|d| d.date == tomorrow_str)
                    .map(|d| d.subuh.clone());
            }
        }
    }
    Ok(sched)
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
