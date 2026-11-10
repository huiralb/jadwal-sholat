use tauri::{
    menu::{CheckMenuItem, MenuBuilder, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

pub const PANEL_WIDTH: f64 = 320.0;
pub const PANEL_HEIGHT: f64 = 380.0;
const GAP: f64 = 8.0;
const TRAY_FALLBACK: (f64, f64, f64, f64) = (0.0, 0.0, 24.0, 24.0);

static LAST_ICON: std::sync::Mutex<Option<(f64, f64, f64, f64)>> = std::sync::Mutex::new(None);

/// Posisi panel (physical px) relatif ikon tray, edge kanan panel disejajarkan
/// dengan edge kanan ikon; terbuka di atas ikon bila tray di separuh bawah layar,
/// di bawah ikon bila tray di separuh atas; di-clamp agar tetap di dalam monitor.
pub fn panel_position(
    icon: (f64, f64, f64, f64),
    monitor: (f64, f64),
    panel: (f64, f64),
    gap: f64,
) -> (f64, f64) {
    let (ix, iy, iw, ih) = icon;
    let (mw, mh) = monitor;
    let (pw, ph) = panel;

    let x = (ix + iw - pw).clamp(0.0, (mw - pw).max(0.0));

    let below = iy + ih / 2.0 <= mh / 2.0;
    let raw_y = if below { iy + ih + gap } else { iy - ph - gap };
    let y = raw_y.clamp(0.0, (mh - ph).max(0.0));
    (x, y)
}

pub fn setup(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Buka Jadwal", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Pengaturan", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Autostart",
        true,
        false,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;
    let menu = MenuBuilder::new(app)
        .item(&open)
        .item(&settings)
        .item(&autostart)
        .item(&quit)
        .build()?;

    TrayIconBuilder::with_id("main")
        .icon(
            app.default_window_icon()
                .expect("ikon default tidak ada")
                .clone(),
        )
        .tooltip("Jadwal Sholat")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button, rect, .. } = event {
                let app = tray.app_handle();
                let scale = app
                    .get_webview_window("main")
                    .and_then(|w| w.primary_monitor().ok().flatten())
                    .map(|m| m.scale_factor())
                    .unwrap_or(1.0);
                let (px, py) = match rect.position {
                    tauri::Position::Physical(p) => (p.x as f64, p.y as f64),
                    tauri::Position::Logical(l) => (l.x * scale, l.y * scale),
                };
                let (pw, ph) = match rect.size {
                    tauri::Size::Physical(s) => (s.width as f64, s.height as f64),
                    tauri::Size::Logical(s) => (s.width * scale, s.height * scale),
                };
                let icon = (px, py, pw, ph);
                if let Ok(mut last) = LAST_ICON.lock() {
                    *last = Some(icon);
                }
                if button == MouseButton::Left {
                    toggle_panel(app);
                }
            }
        })
        .build(app)?;
    Ok(())
}

fn icon_or_fallback() -> (f64, f64, f64, f64) {
    LAST_ICON
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .unwrap_or(TRAY_FALLBACK)
}

pub fn open_panel(app: &tauri::AppHandle, view: &str) {
    show_panel(app);
    let _ = app.emit("open-view", view);
}

fn show_panel(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if let Ok(Some(monitor)) = win.primary_monitor() {
        let sf = monitor.scale_factor();
        let ms = monitor.size();
        let icon = icon_or_fallback();
        let panel = (PANEL_WIDTH * sf, PANEL_HEIGHT * sf);
        let (x, y) = panel_position(
            icon,
            (ms.width as f64, ms.height as f64),
            panel,
            GAP * sf,
        );
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    }
    let _ = win.show();
    let _ = win.set_focus();
}

fn toggle_panel(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
    } else {
        show_panel(app);
    }
}

#[cfg(test)]
mod tests {
    use super::panel_position;

    const MONITOR: (f64, f64) = (1920.0, 1080.0);
    const PANEL: (f64, f64) = (320.0, 380.0);
    const GAP: f64 = 8.0;

    #[test]
    fn bottom_tray_opens_panel_above_and_aligns_right_edge() {
        let (x, y) = panel_position((1800.0, 1060.0, 24.0, 24.0), MONITOR, PANEL, GAP);
        assert_eq!(y, 1060.0 - 380.0 - GAP);
        assert_eq!(x, 1800.0 + 24.0 - 320.0);
    }

    #[test]
    fn top_tray_opens_panel_below() {
        let (x, y) = panel_position((1800.0, 0.0, 24.0, 24.0), MONITOR, PANEL, GAP);
        assert_eq!(y, 0.0 + 24.0 + GAP);
        assert_eq!(x, 1800.0 + 24.0 - 320.0);
    }

    #[test]
    fn clamps_x_to_left_edge() {
        let (x, _) = panel_position((0.0, 500.0, 24.0, 24.0), MONITOR, PANEL, GAP);
        assert_eq!(x, 0.0);
    }

    #[test]
    fn clamps_x_to_right_edge() {
        let (x, _) = panel_position((1910.0, 500.0, 24.0, 24.0), MONITOR, PANEL, GAP);
        assert_eq!(x, 1920.0 - 320.0);
    }

    #[test]
    fn clamps_y_inside_monitor() {
        // Ikon meluber ke luar layar: panel harus di-clamp ke tepi monitor.
        let (_, y) = panel_position((900.0, 100.0, 24.0, 900.0), MONITOR, PANEL, GAP);
        assert_eq!(y, 0.0);
    }
}
