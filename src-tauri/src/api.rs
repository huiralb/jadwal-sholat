use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use tauri::Manager;

use crate::praytimes::{cache_filename, Kota};

pub const BASE: &str = "https://api.myquran.com/v2/sholat";
const JADWAL_TTL: Duration = Duration::from_secs(6 * 3600);
const KOTA_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
const FETCH_BACKOFF: Duration = Duration::from_secs(300);
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const KOTA_FILE: &str = "kota.json";

static LAST_FETCH_ATTEMPT: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static LAST_FETCH_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .unwrap_or_default()
}

pub async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} untuk {url}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}

fn is_valid_jadwal(text: &str) -> bool {
    crate::praytimes::parse_month(text).is_ok()
}

fn is_valid_kota(text: &str) -> bool {
    crate::praytimes::parse_kota_list(text).is_ok()
}

/// Tulis cache hanya bila lolos validasi — respons portal tangkap/kirim
/// salah tidak boleh menimpa cache bagus (janji offline).
fn persist_if_valid(dir: &Path, filename: &str, text: &str, validate: fn(&str) -> bool) -> bool {
    if !validate(text) {
        return false;
    }
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    fs::write(dir.join(filename), text).is_ok()
}

/// True bila key belum pernah dicoba atau terakhir dicoba ≥ backoff lalu.
fn fetch_allowed(
    map: &HashMap<String, Instant>,
    key: &str,
    now: Instant,
    backoff: Duration,
) -> bool {
    map.get(key)
        .map_or(true, |t| now.saturating_duration_since(*t) >= backoff)
}

/// Catat percobaan fetch sekarang; false bila masih dalam jeda backoff.
fn attempt_allowed(key: &str) -> bool {
    let mut map = LAST_FETCH_ATTEMPT
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if fetch_allowed(&map, key, Instant::now(), FETCH_BACKOFF) {
        map.insert(key.to_string(), Instant::now());
        true
    } else {
        false
    }
}

fn log_error_once(prev: &mut Option<String>, msg: &str) -> bool {
    if prev.as_deref() == Some(msg) {
        return false;
    }
    *prev = Some(msg.to_string());
    true
}

fn report_fetch_error(url: &str, err: &str) {
    let mut prev = LAST_FETCH_ERROR
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if log_error_once(&mut prev, err) {
        println!("[jadwal-sholat] fetch {url} gagal: {err}");
    }
}

fn cache_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|e| format!("config dir: {e}"))
}

fn read_cache(app: &tauri::AppHandle, filename: &str) -> Option<String> {
    let dir = cache_dir(app).ok()?;
    fs::read_to_string(dir.join(filename)).ok()
}

fn read_valid_cache(
    app: &tauri::AppHandle,
    filename: &str,
    validate: fn(&str) -> bool,
) -> Option<String> {
    let text = read_cache(app, filename)?;
    validate(&text).then_some(text)
}

fn cache_age(app: &tauri::AppHandle, filename: &str) -> Option<Duration> {
    let dir = cache_dir(app).ok()?;
    let modified = fs::metadata(dir.join(filename)).ok()?.modified().ok()?;
    modified.elapsed().ok()
}

fn cache_is_fresh(app: &tauri::AppHandle, filename: &str, ttl: Duration) -> bool {
    cache_age(app, filename).is_some_and(|age| age < ttl)
}

/// Teks respons bulanan (mentah, untuk parse_month). Cache-first dengan TTL;
/// cache segar dinilai valid dulu; gagal fetch / respons invalid → fallback
/// cache valid; dalam jeda backoff tidak menyentuh jaringan.
pub async fn load_month(
    app: &tauri::AppHandle,
    kota_id: &str,
    year: u32,
    month: u32,
) -> Result<String, String> {
    let filename = cache_filename(kota_id, year, month);
    if cache_is_fresh(app, &filename, JADWAL_TTL) {
        if let Some(cached) = read_valid_cache(app, &filename, is_valid_jadwal) {
            return Ok(cached);
        }
    }
    if !attempt_allowed(&filename) {
        return read_valid_cache(app, &filename, is_valid_jadwal)
            .ok_or_else(|| "jeda retry jaringan dan tidak ada cache valid".to_string());
    }
    let url = format!("{BASE}/jadwal/{kota_id}/{year}/{month}");
    match fetch_text(&client(), &url).await {
        Ok(text) if is_valid_jadwal(&text) => {
            if let Ok(dir) = cache_dir(app) {
                let _ = persist_if_valid(&dir, &filename, &text, is_valid_jadwal);
            }
            Ok(text)
        }
        Ok(_) => {
            report_fetch_error(&url, "respons jadwal tidak valid");
            read_valid_cache(app, &filename, is_valid_jadwal)
                .ok_or_else(|| "respons jadwal tidak valid dan tidak ada cache".to_string())
        }
        Err(err) => {
            report_fetch_error(&url, &err);
            read_valid_cache(app, &filename, is_valid_jadwal).ok_or(err)
        }
    }
}

/// Daftar kota lengkap; cache-first TTL 30 hari, cache segar divalidasi
/// (korup → fetch ulang), backoff antar percobaan.
pub async fn load_kota_list(app: &tauri::AppHandle) -> Result<Vec<Kota>, String> {
    if cache_is_fresh(app, KOTA_FILE, KOTA_TTL) {
        if let Some(cached) = read_cache(app, KOTA_FILE) {
            if let Ok(list) = crate::praytimes::parse_kota_list(&cached) {
                return Ok(list);
            }
        }
    }
    if !attempt_allowed(KOTA_FILE) {
        return read_cache(app, KOTA_FILE)
            .and_then(|cached| crate::praytimes::parse_kota_list(&cached).ok())
            .ok_or_else(|| "jeda retry jaringan dan tidak ada daftar kota valid".to_string());
    }
    let url = format!("{BASE}/kota/semua");
    match fetch_text(&client(), &url).await {
        Ok(text) => match crate::praytimes::parse_kota_list(&text) {
            Ok(list) => {
                if let Ok(dir) = cache_dir(app) {
                    let _ = persist_if_valid(&dir, KOTA_FILE, &text, is_valid_kota);
                }
                Ok(list)
            }
            Err(_) => {
                report_fetch_error(&url, "daftar kota tidak valid");
                read_cache(app, KOTA_FILE)
                    .and_then(|cached| crate::praytimes::parse_kota_list(&cached).ok())
                    .ok_or_else(|| "daftar kota tidak valid dan tidak ada cache".to_string())
            }
        },
        Err(err) => {
            report_fetch_error(&url, &err);
            read_cache(app, KOTA_FILE)
                .and_then(|cached| crate::praytimes::parse_kota_list(&cached).ok())
                .ok_or(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    fn tempdir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jadwal-sholat-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("buat tempdir");
        dir
    }

    #[test]
    fn persist_writes_valid_jadwal_body() {
        let dir = tempdir("valid");
        let json = r#"{"data":{"lokasi":"KOTA YOGYAKARTA","jadwal":[]}}"#;
        let wrote = persist_if_valid(&dir, "a.json", json, |t| {
            crate::praytimes::parse_month(t).is_ok()
        });
        assert!(wrote);
        assert!(dir.join("a.json").exists());
    }

    #[test]
    fn persist_rejects_captive_portal_body_without_touching_disk() {
        let dir = tempdir("invalid");
        let wrote = persist_if_valid(&dir, "b.json", "<html>login portal</html>", |t| {
            crate::praytimes::parse_month(t).is_ok()
        });
        assert!(!wrote);
        assert!(!dir.join("b.json").exists());
    }

    #[test]
    fn fetch_backoff_gates_repeated_attempts_per_key() {
        let now = Instant::now();
        let mut map = HashMap::new();
        assert!(fetch_allowed(&map, "k1", now, Duration::from_secs(300)));
        map.insert("k1".to_string(), now);
        assert!(!fetch_allowed(
            &map,
            "k1",
            now + Duration::from_secs(60),
            Duration::from_secs(300)
        ));
        assert!(fetch_allowed(
            &map,
            "k1",
            now + Duration::from_secs(301),
            Duration::from_secs(300)
        ));
        assert!(fetch_allowed(
            &map,
            "k2",
            now + Duration::from_secs(60),
            Duration::from_secs(300)
        ));
    }

    #[test]
    fn logs_fetch_error_only_when_message_changes() {
        let mut prev: Option<String> = None;
        assert!(log_error_once(&mut prev, "gagal: timeout"));
        assert!(!log_error_once(&mut prev, "gagal: timeout"));
        assert!(log_error_once(&mut prev, "gagal: dns"));
    }

    #[tokio::test]
    async fn fetch_text_times_out_instead_of_hanging() {
        // Server yang menerima koneksi tapi tidak pernah menjawab.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                std::mem::forget(stream);
                std::thread::park();
            }
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .expect("client");
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            fetch_text(&client, &format!("http://{addr}")),
        )
        .await;
        assert!(
            result.is_ok(),
            "fetch tidak mengembalikan apa pun dalam 2 dtk — timeout tidak diterapkan"
        );
        assert!(result.expect("selesai").is_err());
    }
}
