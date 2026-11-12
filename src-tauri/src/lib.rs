// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod api;
mod praytimes;
mod scheduler;
mod store;
mod toast;
mod tray;
#[cfg(target_os = "linux")]
mod x11_click;
#[cfg(target_os = "linux")]
mod x11_focus;

use tauri_plugin_autostart::ManagerExt;

use crate::praytimes::{Kota, NotifikasiMonitor, Settings, TodaySchedule};

/// Terapkan keputusan autostart ke OS (idempotent).
fn apply_autostart(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let plugin = app.autolaunch();
    if enabled {
        plugin.enable()
    } else {
        plugin.disable()
    }
    .map_err(|e| e.to_string())
}

/// Simpan + terapkan + sinkronkan centang menu tray.
fn set_autostart(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    store::set(app, "autostart", serde_json::Value::Bool(enabled))?;
    apply_autostart(app, enabled)?;
    tray::sync_autostart_checked(enabled);
    Ok(())
}

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
    if key == "autostart" {
        let enabled = value
            .as_bool()
            .ok_or_else(|| "nilai autostart harus boolean".to_string())?;
        return set_autostart(&app, enabled);
    }
    if key == "notifikasiMonitor" {
        let mode: NotifikasiMonitor = serde_json::from_value(value.clone())
            .map_err(|_| "nilai notifikasiMonitor harus all/primary".to_string())?;
        store::set(&app, &key, value)?;
        if mode.is_primary() {
            // Halaman toast dimuat sekali di awal agar siap sebelum notifikasi pertama.
            if let Err(e) = toast::ensure_window(&app) {
                println!("[jadwal-sholat] jendela notifikasi gagal: {e}");
            }
        }
        return Ok(());
    }
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
            search_kota,
            toast::get_toast
        ])
        .setup(|app| {
            tray::setup(app.handle())?;
            #[cfg(target_os = "linux")]
            x11_click::install(app.handle());
            // Rekonsiliasi: store = sumber kebenaran autostart.
            if let Err(e) = apply_autostart(app.handle(), store::load(app.handle()).autostart) {
                println!("[jadwal-sholat] autostart gagal: {e}");
            }
            // Mode monitor utama: siapkan jendela toast lebih awal agar halaman
            // sempat dimuat sebelum notifikasi pertama.
            if store::load(app.handle()).notifikasi_monitor.is_primary() {
                if let Err(e) = toast::ensure_window(app.handle()) {
                    println!("[jadwal-sholat] jendela notifikasi gagal: {e}");
                }
            }
            scheduler::spawn(app.handle().clone());
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => tray::open_panel(app, "jadwal"),
            "settings" => tray::open_panel(app, "settings"),
            "autostart" => {
                let next = !store::load(app).autostart;
                if let Err(e) = set_autostart(app, next) {
                    println!("[jadwal-sholat] autostart gagal: {e}");
                }
            }
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
                tray::hide_panel(app);
            }
        }
    });
}
