use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use tauri::Manager;

use crate::praytimes::{cache_filename, Kota};

pub const BASE: &str = "https://api.myquran.com/v2/sholat";
const JADWAL_TTL: Duration = Duration::from_secs(6 * 3600);
const KOTA_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
const KOTA_FILE: &str = "kota.json";

fn cache_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|e| format!("config dir: {e}"))
}

fn read_cache(app: &tauri::AppHandle, filename: &str) -> Option<String> {
    let dir = cache_dir(app).ok()?;
    fs::read_to_string(dir.join(filename)).ok()
}

fn cache_age(app: &tauri::AppHandle, filename: &str) -> Option<Duration> {
    let dir = cache_dir(app).ok()?;
    let modified = fs::metadata(dir.join(filename)).ok()?.modified().ok()?;
    modified.elapsed().ok()
}

fn cache_is_fresh(app: &tauri::AppHandle, filename: &str, ttl: Duration) -> bool {
    cache_age(app, filename).is_some_and(|age| age < ttl)
}

fn write_cache(app: &tauri::AppHandle, filename: &str, text: &str) -> Result<(), String> {
    let dir = cache_dir(app)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join(filename), text).map_err(|e| e.to_string())
}

async fn fetch_text(url: &str) -> Result<String, String> {
    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} untuk {url}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}

/// Teks respons bulanan (mentah, untuk parse_month). Cache-first dengan TTL;
/// gagal fetch → fallback cache; tanpa cache → error.
pub async fn load_month(
    app: &tauri::AppHandle,
    kota_id: &str,
    year: u32,
    month: u32,
) -> Result<String, String> {
    let filename = cache_filename(kota_id, year, month);
    if cache_is_fresh(app, &filename, JADWAL_TTL) {
        if let Some(cached) = read_cache(app, &filename) {
            return Ok(cached);
        }
    }
    let url = format!("{BASE}/jadwal/{kota_id}/{year}/{month}");
    match fetch_text(&url).await {
        Ok(text) => {
            let _ = write_cache(app, &filename, &text);
            Ok(text)
        }
        Err(err) => {
            println!("[jadwal-sholat] fetch {url} gagal: {err}");
            read_cache(app, &filename).ok_or(err)
        }
    }
}

/// Daftar kota lengkap; cache-first TTL 30 hari, fallback cache saat offline.
pub async fn load_kota_list(app: &tauri::AppHandle) -> Result<Vec<Kota>, String> {
    if cache_is_fresh(app, KOTA_FILE, KOTA_TTL) {
        if let Some(cached) = read_cache(app, KOTA_FILE) {
            return crate::praytimes::parse_kota_list(&cached).map_err(|e| e.to_string());
        }
    }
    match fetch_text(&format!("{BASE}/kota/semua")).await {
        Ok(text) => {
            let list = crate::praytimes::parse_kota_list(&text).map_err(|e| e.to_string())?;
            let _ = write_cache(app, KOTA_FILE, &text);
            Ok(list)
        }
        Err(err) => read_cache(app, KOTA_FILE)
            .and_then(|cached| crate::praytimes::parse_kota_list(&cached).ok())
            .ok_or(err),
    }
}
