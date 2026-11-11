use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::praytimes::{NotifikasiMonitor, Settings};

const FILE: &str = "settings.json";

/// Baca settings (kunci terpisah, camelCase sesuai kontrak JS) dengan
/// default desain: notifikasi on di semua monitor, autostart off, lokasi
/// belum dipilih.
pub fn load(app: &AppHandle) -> Settings {
    let Some(store) = app.store(FILE).ok() else {
        return Settings::default();
    };
    let notifikasi_monitor = store
        .get("notifikasiMonitor")
        .and_then(|v| v.as_str().map(String::from));
    Settings {
        kota_id: store
            .get("kotaId")
            .and_then(|v| v.as_str().map(String::from)),
        kota_nama: store
            .get("kotaNama")
            .and_then(|v| v.as_str().map(String::from)),
        notifikasi: store.get("notifikasi").and_then(|v| v.as_bool()).unwrap_or(true),
        notifikasi_monitor: NotifikasiMonitor::parse(notifikasi_monitor.as_deref()),
        autostart: store.get("autostart").and_then(|v| v.as_bool()).unwrap_or(false),
    }
}

pub fn set(app: &AppHandle, key: &str, value: serde_json::Value) -> Result<(), String> {
    let store = app.store(FILE).map_err(|e| e.to_string())?;
    store.set(key, value);
    store.save().map_err(|e| e.to_string())
}
