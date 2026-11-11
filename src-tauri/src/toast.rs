use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const LABEL: &str = "notif";
const EVENT: &str = "show-toast";
const SIZE: (f64, f64) = (360.0, 80.0); // logical px
const MARGIN: f64 = 12.0; // logical px
const VISIBLE: Duration = Duration::from_secs(6);

static LAST: Mutex<Option<ToastPayload>> = Mutex::new(None);
static HIDE_GEN: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize)]
pub struct ToastPayload {
    pub title: String,
    pub body: String,
}

/// Payload notifikasi terakhir — halaman toast meminta ini saat baru dimuat,
/// karena event bisa terkirim sebelum halaman siap mendengarkan.
#[tauri::command]
pub fn get_toast() -> Option<ToastPayload> {
    LAST.lock().ok().and_then(|last| last.clone())
}

/// Posisi toast (koordinat global physical) di pojok kanan atas `monitor`.
pub fn toast_position(
    monitor: (f64, f64, f64, f64),
    scale: f64,
    size: (f64, f64),
    margin: f64,
) -> (f64, f64) {
    let (ox, oy, mw, mh) = monitor;
    let (tw, th) = (size.0 * scale, size.1 * scale);
    let m = margin * scale;
    (
        (ox + mw - tw - m).max(ox),
        (oy + m).min((oy + mh - th).max(oy)),
    )
}

/// Buat jendela toast bila belum ada; dipanggil lebih awal (setup/ganti
/// setting) agar halaman sempat dimuat sebelum notifikasi pertama.
pub fn ensure_window(app: &AppHandle) -> Result<(), String> {
    window(app).map(|_| ())
}

/// Tampilkan toast di monitor ikon tray (fallback: monitor primary), lalu
/// sembunyikan otomatis setelah beberapa detik.
pub fn show(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    let payload = ToastPayload {
        title: title.to_string(),
        body: body.to_string(),
    };
    *LAST.lock().map_err(|e| e.to_string())? = Some(payload.clone());

    let win = window(app)?;
    let (monitors, primary, pscale) = crate::tray::monitor_geometry(app)
        .unwrap_or((vec![(0.0, 0.0, 1920.0, 1080.0)], 0, 1.0));
    let idx = crate::tray::target_monitor(&monitors, primary, crate::tray::last_icon());
    let (ox, oy, mw, mh) = monitors[idx];
    let scale = monitor_scale(app, (ox, oy), pscale);
    let (x, y) = toast_position((ox, oy, mw, mh), scale, SIZE, MARGIN);
    let _ = win.set_position(PhysicalPosition::new(x, y));
    let _ = win.show();
    let _ = win.emit(EVENT, &payload);

    // Sembunyikan setelah VISIBLE, kecuali sudah ada notifikasi baru.
    let seq = HIDE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let hide = win.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(VISIBLE).await;
        if HIDE_GEN.load(Ordering::SeqCst) == seq {
            let _ = hide.hide();
        }
    });
    Ok(())
}

fn window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(win) = app.get_webview_window(LABEL) {
        return Ok(win);
    }
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("notif.html".into()))
        .title("Notifikasi")
        .inner_size(SIZE.0, SIZE.1)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .focused(false)
        .skip_taskbar(true)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())
}

/// Skala monitor yang berawal di `origin` (koordinat global physical).
fn monitor_scale(app: &AppHandle, origin: (f64, f64), fallback: f64) -> f64 {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .find(|m| m.position().x as f64 == origin.0 && m.position().y as f64 == origin.1)
        .map(|m| m.scale_factor())
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::toast_position;
    use crate::tray::{monitor_index_at, target_monitor};

    #[test]
    fn toast_lands_top_right_of_primary_monitor() {
        let (x, y) = toast_position((0.0, 0.0, 1920.0, 1080.0), 1.0, (360.0, 80.0), 12.0);
        assert_eq!((x, y), (1548.0, 12.0));
    }

    #[test]
    fn toast_lands_top_right_of_secondary_monitor() {
        let (x, y) =
            toast_position((1920.0, 0.0, 1920.0, 1080.0), 1.0, (360.0, 80.0), 12.0);
        assert_eq!((x, y), (3468.0, 12.0));
    }

    #[test]
    fn toast_scales_with_monitor_scale() {
        let (x, y) = toast_position((0.0, 0.0, 1920.0, 1080.0), 2.0, (360.0, 80.0), 12.0);
        assert_eq!((x, y), (1176.0, 24.0));
    }

    #[test]
    fn toast_clamps_left_when_monitor_too_small() {
        let (x, y) = toast_position((0.0, 0.0, 300.0, 200.0), 1.0, (360.0, 80.0), 12.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 12.0);
    }

    #[test]
    fn target_monitor_follows_tray_icon_then_primary() {
        let monitors = [
            (0.0, 0.0, 1920.0, 1080.0),
            (1920.0, 0.0, 1920.0, 1080.0),
        ];
        // Ikon tray di monitor kanan → notifikasi ikut ke monitor kanan.
        assert_eq!(
            target_monitor(&monitors, 0, Some((2500.0, 500.0, 24.0, 24.0))),
            1
        );
        // Belum pernah ada klik ikon → monitor primary.
        assert_eq!(target_monitor(&monitors, 1, None), 1);
        assert_eq!(target_monitor(&monitors, 0, None), 0);
    }

    #[test]
    fn monitor_index_at_falls_back_to_primary_outside_all_monitors() {
        let monitors = [
            (0.0, 0.0, 1920.0, 1080.0),
            (1920.0, 0.0, 1920.0, 1080.0),
        ];
        assert_eq!(monitor_index_at(&monitors, 1, (100.0, 100.0)), 0);
        assert_eq!(monitor_index_at(&monitors, 0, (2500.0, 100.0)), 1);
        assert_eq!(monitor_index_at(&monitors, 1, (99999.0, 99999.0)), 1);
    }
}
