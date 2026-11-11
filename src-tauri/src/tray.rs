use tauri::{
    menu::{CheckMenuItem, MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

pub const PANEL_WIDTH: f64 = 320.0;
pub const PANEL_HEIGHT: f64 = 380.0;
const GAP: f64 = 8.0;

static LAST_ICON: std::sync::Mutex<Option<(f64, f64, f64, f64)>> = std::sync::Mutex::new(None);
static AUTOSTART_ITEM: std::sync::OnceLock<CheckMenuItem<tauri::Wry>> =
    std::sync::OnceLock::new();

/// Sinkronkan centang menu "Autostart" dengan keadaan store.
pub fn sync_autostart_checked(checked: bool) {
    if let Some(item) = AUTOSTART_ITEM.get() {
        let _ = item.set_checked(checked);
    }
}

/// Rect ikon dari event. ksni (Linux) selalu mengirim rect kosong dan
/// menaruh koordinat pada posisi klik; host yang tidak mengirim koordinat
/// sama sekali (0,0) → asumsi pojok kanan bawah (area tray Cinnamon).
pub fn icon_rect_from_click(
    rect_pos: (f64, f64),
    rect_size: (f64, f64),
    click: (f64, f64),
    monitor: (f64, f64),
) -> (f64, f64, f64, f64) {
    if rect_size.0 > 0.0 && rect_size.1 > 0.0 {
        return (rect_pos.0, rect_pos.1, rect_size.0, rect_size.1);
    }
    if click.0 > 1.0 || click.1 > 1.0 {
        return (click.0, click.1, 24.0, 24.0);
    }
    (
        (monitor.0 - 24.0).max(0.0),
        (monitor.1 - 24.0).max(0.0),
        24.0,
        24.0,
    )
}

/// Hanya klik kiri pada saat tombol lepas (Up): ksni hanya mengirim Up;
/// Windows/macOS mengirim Down DAN Up — tanpa filter ini panel toggle dobel.
pub fn should_handle_click(button: &MouseButton, state: &MouseButtonState) -> bool {
    *button == MouseButton::Left && *state == MouseButtonState::Up
}

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

/// Posisi panel dalam koordinat GLOBAL multi-monitor: monitor yang mengandung
/// ikon (rect global) menjadi acuan, perhitungan lokal memakai `panel_position`,
/// hasilnya dikembalikan ke koordinat global. Ikon di luar semua monitor →
/// monitor `primary`.
pub fn panel_position_multi(
    monitors: &[(f64, f64, f64, f64)], // (x, y, w, h) global, bisa negatif
    primary: usize,
    icon: (f64, f64, f64, f64),
    panel: (f64, f64),
    gap: f64,
) -> (f64, f64) {
    if monitors.is_empty() {
        return (icon.0, icon.1);
    }
    let idx = monitor_index_at(monitors, primary, (icon.0 + icon.2 / 2.0, icon.1 + icon.3 / 2.0));
    let (ox, oy, mw, mh) = monitors[idx];
    let local_icon = (icon.0 - ox, icon.1 - oy, icon.2, icon.3);
    let (lx, ly) = panel_position(local_icon, (mw, mh), panel, gap);
    (ox + lx, oy + ly)
}

/// Index monitor yang mengandung titik global; titik di luar semua monitor →
/// monitor `primary`.
pub fn monitor_index_at(
    monitors: &[(f64, f64, f64, f64)],
    primary: usize,
    point: (f64, f64),
) -> usize {
    let (cx, cy) = point;
    monitors
        .iter()
        .position(|(x, y, w, h)| cx >= *x && cx < x + w && cy >= *y && cy < y + h)
        .unwrap_or(primary.min(monitors.len().saturating_sub(1)))
}

/// Monitor tujuan notifikasi mode primary: monitor yang mengandung ikon tray
/// bila sudah pernah diketahui; selain itu monitor `primary`.
pub fn target_monitor(
    monitors: &[(f64, f64, f64, f64)],
    primary: usize,
    icon: Option<(f64, f64, f64, f64)>,
) -> usize {
    match icon {
        Some((x, y, w, h)) => monitor_index_at(monitors, primary, (x + w / 2.0, y + h / 2.0)),
        None => primary.min(monitors.len().saturating_sub(1)),
    }
}

/// Rect global ikon tray dari klik terakhir (None bila belum pernah ada klik).
pub fn last_icon() -> Option<(f64, f64, f64, f64)> {
    LAST_ICON.lock().ok().and_then(|last| *last)
}

pub fn setup(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Buka Jadwal", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Pengaturan", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Autostart",
        true,
        crate::store::load(app).autostart,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;
    let _ = AUTOSTART_ITEM.set(autostart.clone());
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
            if let TrayIconEvent::Click {
                button,
                button_state,
                position,
                rect,
                ..
            } = event
            {
                if !should_handle_click(&button, &button_state) {
                    return;
                }
                let app = tray.app_handle();
                let (monitors, primary, pscale) = monitor_geometry(app)
                    .unwrap_or((vec![(0.0, 0.0, 1920.0, 1080.0)], 0, 1.0));
                let (ox, oy, mw, mh) = monitors[primary];
                let (rp, rs) = match (&rect.position, &rect.size) {
                    (tauri::Position::Physical(p), tauri::Size::Physical(s)) => {
                        ((p.x as f64, p.y as f64), (s.width as f64, s.height as f64))
                    }
                    (tauri::Position::Logical(p), tauri::Size::Logical(s)) => {
                        ((p.x * pscale, p.y * pscale), (s.width * pscale, s.height * pscale))
                    }
                    _ => ((0.0, 0.0), (0.0, 0.0)),
                };
                let mut icon = icon_rect_from_click(rp, rs, (position.x, position.y), (mw, mh));
                if rs == (0.0, 0.0) && position.x <= 1.0 && position.y <= 1.0 {
                    // Host SNI tidak memberi koordinat → pojok kanan bawah
                    // monitor primary, dalam koordinat global.
                    icon = (ox + mw - 24.0, oy + mh - 24.0, 24.0, 24.0);
                }
                if let Ok(mut last) = LAST_ICON.lock() {
                    *last = Some(icon);
                }
                toggle_panel(app);
            }
        })
        .build(app)?;
    Ok(())
}

/// Daftar monitor (x, y, w, h) global + index primary + skala primary.
pub fn monitor_geometry(app: &tauri::AppHandle) -> Option<(Vec<(f64, f64, f64, f64)>, usize, f64)> {
    let monitors: Vec<(f64, f64, f64, f64)> = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            (
                m.position().x as f64,
                m.position().y as f64,
                m.size().width as f64,
                m.size().height as f64,
            )
        })
        .collect();
    if monitors.is_empty() {
        return None;
    }
    let primary_pos = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|p| (p.position().x as f64, p.position().y as f64));
    let primary = primary_pos
        .and_then(|(px, py)| {
            monitors
                .iter()
                .position(|(x, y, _, _)| *x == px && *y == py)
        })
        .unwrap_or(0);
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    Some((monitors, primary, scale))
}

pub fn open_panel(app: &tauri::AppHandle, view: &str) {
    show_panel(app);
    let _ = app.emit("open-view", view);
}

fn show_panel(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let Some((monitors, primary, pscale)) = monitor_geometry(app) else {
        let _ = win.show();
        let _ = win.set_focus();
        return;
    };
    let icon = match last_icon() {
        Some(icon) => icon,
        None => {
            // Belum pernah ada klik: pojok kanan bawah monitor primary (global).
            let (ox, oy, mw, mh) = monitors[primary];
            (ox + mw - 24.0, oy + mh - 24.0, 24.0, 24.0)
        }
    };
    // Skala monitor yang mengandung ikon (physical px).
    let cx = icon.0 + icon.2 / 2.0;
    let cy = icon.1 + icon.3 / 2.0;
    let scale = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .find(|m| {
            let (x, y) = (m.position().x as f64, m.position().y as f64);
            let (w, h) = (m.size().width as f64, m.size().height as f64);
            cx >= x && cx < x + w && cy >= y && cy < y + h
        })
        .map(|m| m.scale_factor())
        .unwrap_or(pscale);
    let panel = (PANEL_WIDTH * scale, PANEL_HEIGHT * scale);
    let (x, y) = panel_position_multi(&monitors, primary, icon, panel, GAP * scale);
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
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
        // Klik ikon tray = buka segar dari halaman jadwal (reset view + dropdown).
        let _ = app.emit("open-view", "jadwal");
    }
}

#[cfg(test)]
mod tests {
    use super::panel_position;
    use super::{icon_rect_from_click, panel_position_multi, should_handle_click};
    use tauri::tray::{MouseButton, MouseButtonState};

    #[test]
    fn icon_uses_rect_when_backend_provides_it() {
        let r = icon_rect_from_click((1800.0, 1060.0), (24.0, 24.0), (1805.0, 1065.0), (1920.0, 1080.0));
        assert_eq!(r, (1800.0, 1060.0, 24.0, 24.0));
    }

    #[test]
    fn icon_falls_back_to_click_position_when_rect_empty() {
        // ksni (Linux): rect selalu default, koordinat ada di posisi klik.
        let r = icon_rect_from_click((0.0, 0.0), (0.0, 0.0), (1810.0, 1050.0), (1920.0, 1080.0));
        assert_eq!(r, (1810.0, 1050.0, 24.0, 24.0));
    }

    #[test]
    fn icon_falls_back_to_bottom_right_when_host_gives_no_coordinates() {
        // Host SNI yang tidak mengirim koordinat (0,0) → asumsi area tray kanan-bawah.
        let r = icon_rect_from_click((0.0, 0.0), (0.0, 0.0), (0.0, 0.0), (1920.0, 1080.0));
        assert_eq!(r, (1896.0, 1056.0, 24.0, 24.0));
    }

    #[test]
    fn handles_only_left_click_in_up_state() {
        // ksni mengirim Up; Windows/macOS mengirim Down DAN Up (wajib filter agar tidak dobel).
        assert!(should_handle_click(&MouseButton::Left, &MouseButtonState::Up));
        assert!(!should_handle_click(&MouseButton::Left, &MouseButtonState::Down));
        assert!(!should_handle_click(&MouseButton::Right, &MouseButtonState::Up));
        // ksni secondary_activate = tombol tengah; biar menu yang menangani.
        assert!(!should_handle_click(&MouseButton::Middle, &MouseButtonState::Up));
    }

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

    // --- Multi-monitor: koordinat ikon bersifat global (virtual desktop) ---

    #[test]
    fn multi_panel_lands_on_horizontal_secondary_monitor() {
        // Monitor kanan ber-offset x=1920; panel HARUS di monitor kanan (bukan
        // ter-clamp ke monitor utama seperti perilaku lama).
        let monitors = [
            (0.0, 0.0, 1920.0, 1080.0),
            (1920.0, 0.0, 1920.0, 1080.0),
        ];
        let (x, y) = panel_position_multi(
            &monitors,
            0,
            (1920.0 + 1878.0, 1060.0, 24.0, 24.0),
            PANEL,
            GAP,
        );
        assert_eq!((x, y), (1920.0 + 1582.0, 672.0));
    }

    #[test]
    fn multi_panel_lands_on_left_monitor_with_negative_origin() {
        let monitors = [
            (-1920.0, 0.0, 1920.0, 1080.0),
            (0.0, 0.0, 1920.0, 1080.0),
        ];
        let (x, y) = panel_position_multi(&monitors, 1, (-42.0, 1060.0, 24.0, 24.0), PANEL, GAP);
        assert_eq!((x, y), (-1920.0 + 1582.0, 672.0));
    }

    #[test]
    fn multi_panel_lands_on_vertical_monitor_below_primary() {
        // Tata letak mesin uji: HDMI (0,0), eDP primary (0,1080) bertumpuk.
        // Ikon di panel bawah eDP → panel harus di eDP (y global 1752),
        // bukan ter-clamp ke y=700 di monitor HDMI.
        let monitors = [
            (0.0, 0.0, 1920.0, 1080.0),
            (0.0, 1080.0, 1920.0, 1080.0),
        ];
        let (x, y) = panel_position_multi(&monitors, 1, (1878.0, 2140.0, 24.0, 24.0), PANEL, GAP);
        assert_eq!((x, y), (1582.0, 1080.0 + 672.0));
    }

    #[test]
    fn multi_unknown_icon_coords_fall_back_to_primary_monitor() {
        let monitors = [
            (0.0, 0.0, 1920.0, 1080.0),
            (1920.0, 0.0, 1920.0, 1080.0),
        ];
        // Koordinat di luar semua monitor → pakai monitor primary (index 1).
        let (x, y) = panel_position_multi(&monitors, 1, (99999.0, 99999.0, 24.0, 24.0), PANEL, GAP);
        assert_eq!((x, y), (1920.0 + 1600.0, 700.0));
    }
}
