//! Monitor klik global X11 (ekstensi XRecord).
//!
//! Fokus window adalah jalur utama untuk menutup panel (focus-out), tetapi
//! window manager bisa menolak memberi fokus — misalnya saat ada dialog lain
//! yang memegang fokus/grab. Pada kondisi itu klik di luar panel tidak pernah
//! menghasilkan focus-out dan panel "nyangkut" terbuka.
//!
//! XRecord merekam ButtonPress dari seluruh klien tanpa mengambil alih event
//! (tidak seperti pointer grab, yang membuat klik di dalam WebView mati).
//! Saat klik terjadi di luar batas panel, panel ditutup setelah jeda pendek;
//! jeda ini memberi kesempatan handler tray lebih dulu menutup panel, sehingga
//! klik ikon tray tidak memicu buka-tutup ganda.

use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use gtk::prelude::*;
use tauri::{AppHandle, Manager};
use x11::{xlib, xrecord};

static APP: OnceLock<AppHandle> = OnceLock::new();
static OPEN: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static BOUNDS: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);
static OPENED_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// Jeda setelah panel tampil; klik yang membuka panel (masih di antrean
/// XRecord) tidak boleh langsung menutupnya.
const OPEN_GRACE: Duration = Duration::from_millis(400);
/// Tunda tutup agar handler tray (Activate) sempat menutup panel lebih dulu.
const CLICK_HIDE_DELAY: Duration = Duration::from_millis(150);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Mulai perekam XRecord. Dipanggil sekali dari setup (main thread).
pub fn install(app: &AppHandle) {
    if APP.set(app.clone()).is_err() {
        return;
    }
    let _ = app.run_on_main_thread(start);
}

/// Catat posisi & ukuran panel (koordinat root/fisik) saat panel ditampilkan.
pub fn panel_shown(x: i32, y: i32, w: i32, h: i32) {
    if w <= 0 || h <= 0 {
        return;
    }
    *BOUNDS.lock().unwrap() = Some((x, y, w, h));
    let was_open = OPEN.swap(true, Ordering::SeqCst);
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if !was_open {
        *OPENED_AT.lock().unwrap() = Some(Instant::now());
    }
}

/// Panel ditutup lewat jalur mana pun (focus-out, tray, dsb.).
pub fn panel_hidden() {
    OPEN.store(false, Ordering::SeqCst);
}

fn start() {
    // Di Wayland XRecord hanya melihat klien XWayland; biarkan focus-out
    // yang menangani.
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return;
    }
    let dpy = unsafe { xlib::XOpenDisplay(std::ptr::null()) };
    if dpy.is_null() {
        return;
    }

    let mut major = 0;
    let mut minor = 0;
    if unsafe { xrecord::XRecordQueryVersion(dpy, &mut major, &mut minor) } == 0 {
        unsafe { xlib::XCloseDisplay(dpy) };
        return;
    }

    let range = unsafe { xrecord::XRecordAllocRange() };
    if range.is_null() {
        unsafe { xlib::XCloseDisplay(dpy) };
        return;
    }
    unsafe {
        (*range).device_events.first = xlib::ButtonPress as u8;
        (*range).device_events.last = xlib::ButtonPress as u8;
    }

    let mut clients = [xrecord::XRecordAllClients];
    let mut ranges = [range];
    let context = unsafe {
        xrecord::XRecordCreateContext(
            dpy,
            0,
            clients.as_mut_ptr(),
            1,
            ranges.as_mut_ptr(),
            1,
        )
    };
    if context == 0 {
        unsafe {
            xlib::XFree(range as *mut c_void);
            xlib::XCloseDisplay(dpy);
        }
        return;
    }
    if unsafe {
        xrecord::XRecordEnableContextAsync(dpy, context, Some(on_record), std::ptr::null_mut())
    } == 0
    {
        unsafe {
            xrecord::XRecordFreeContext(dpy, context);
            xlib::XFree(range as *mut c_void);
            xlib::XCloseDisplay(dpy);
        }
        return;
    }
    // Permintaan enable baru dikirim ke server setelah di-flush.
    unsafe { xlib::XFlush(dpy) };

    // XRecordEnableContextAsync hanya mengantre data; ambil berkala dari
    // main loop GTK (koneksi X ini hanya dipakai di main thread).
    gtk::glib::timeout_add_local(POLL_INTERVAL, move || {
        unsafe { xrecord::XRecordProcessReplies(dpy) };
        gtk::glib::ControlFlow::Continue
    });
}

unsafe extern "C" fn on_record(_closure: *mut c_char, data: *mut xrecord::XRecordInterceptData) {
    if data.is_null() {
        return;
    }
    unsafe {
        let rec = &*data;
        if rec.category == xrecord::XRecordFromServer
            && !rec.data.is_null()
            && rec.data_len >= 2
        {
            // Event perangkat dikirim ringkas: [tipe, detail/tombol, ..., time].
            let bytes = std::slice::from_raw_parts(rec.data as *const u8, rec.data_len as usize);
            if bytes[0] as i32 == xlib::ButtonPress {
                if let Some((x, y)) = pointer_position() {
                    on_button_press(bytes[1] as u32, x, y);
                }
            }
        }
        xrecord::XRecordFreeData(data);
    }
}

/// Posisi pointer saat ini. XRecord tidak menyertakan koordinat pada event
/// perangkat, jadi ambil lewat koneksi GDK (bukan koneksi XRecord, agar tidak
/// saling menunggu di dalam callback).
fn pointer_position() -> Option<(i32, i32)> {
    let display = gtk::gdk::Display::default()?;
    let seat = display.default_seat()?;
    let pointer = seat.pointer()?;
    let (_screen, x, y) = pointer.position();
    Some((x, y))
}

fn on_button_press(button: u32, x: i32, y: i32) {
    // Hanya klik kiri/tengah/kanan; scroll (4/5) tidak menutup panel.
    if button > 3 {
        return;
    }
    if !OPEN.load(Ordering::SeqCst) {
        return;
    }
    if let Some(opened) = *OPENED_AT.lock().unwrap() {
        if opened.elapsed() < OPEN_GRACE {
            return;
        }
    }
    let Some((bx, by, bw, bh)) = *BOUNDS.lock().unwrap() else {
        return;
    };
    if point_inside((bx, by, bw, bh), x, y) {
        return; // klik di dalam panel
    }

    let generation = GENERATION.load(Ordering::SeqCst);
    gtk::glib::timeout_add_local_once(CLICK_HIDE_DELAY, move || {
        if !OPEN.load(Ordering::SeqCst) || GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let Some(app) = APP.get() else {
            return;
        };
        let Some(win) = app.get_webview_window("main") else {
            return;
        };
        OPEN.store(false, Ordering::SeqCst);
        let _ = win.hide();
    });
}

/// True bila titik root `(x, y)` berada di dalam panel `(bx, by, bw, bh)`.
fn point_inside(bounds: (i32, i32, i32, i32), x: i32, y: i32) -> bool {
    let (bx, by, bw, bh) = bounds;
    x >= bx && x < bx + bw && y >= by && y < by + bh
}

#[cfg(test)]
mod tests {
    use super::point_inside;

    const PANEL: (i32, i32, i32, i32) = (100, 200, 320, 380);

    #[test]
    fn point_inside_panel_is_ignored() {
        assert!(point_inside(PANEL, 100, 200)); // pojok kiri-atas
        assert!(point_inside(PANEL, 260, 390)); // tengah
        assert!(point_inside(PANEL, 419, 579)); // pojok kanan-bawah (eksklusif)
    }

    #[test]
    fn point_outside_panel_closes_it() {
        assert!(!point_inside(PANEL, 99, 300));
        assert!(!point_inside(PANEL, 100, 199));
        assert!(!point_inside(PANEL, 420, 300));
        assert!(!point_inside(PANEL, 260, 580));
    }
}
