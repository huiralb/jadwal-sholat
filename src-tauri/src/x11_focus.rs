//! Aktivasi panel di X11 (Cinnamon/Muffin).
//!
//! `WebviewWindow::set_focus()` di Linux memakai
//! `gtk_window_present_with_time(GDK_CURRENT_TIME)` (lihat tao). Muffin
//! menolak/mengembalikan permintaan fokus tanpa timestamp itu karena
//! focus-stealing prevention: panel sempat tampil lalu kehilangan fokus
//! (panel ikut tersembunyi), atau sama sekali tidak pernah mendapat fokus
//! sehingga klik di luar tidak pernah memicu focus-out.
//!
//! Solusinya: kirim `_NET_ACTIVE_WINDOW` (EWMH) dengan `CurrentTime` dan
//! source `0` (unspecified) ke root window — sama seperti `wmctrl -a`.
//! Setelah itu window manager memegang fokus panel sampai pengguna benar-
//! benar mengklik di luar panel, sehingga:
//! - klik di dalam panel (termasuk halaman Pengaturan) → panel tetap;
//! - klik di luar panel → focus-out → panel tersembunyi oleh lib.rs.

use std::time::Duration;

use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;
use tauri::WebviewWindow;

/// Panel baru benar-benar di-map beberapa iterasi glib setelah `show()`.
/// Coba aktivasi tiap 30 ms, maksimal 20 kali (~600 ms).
const RETRY_MS: u64 = 30;
const MAX_ATTEMPTS: u32 = 20;

/// Fokuskan panel dengan permintaan aktivasi EWMH setelah window ter-map.
pub fn focus(win: &WebviewWindow) {
    let win_main = win.clone();
    let scheduled = win.run_on_main_thread(move || {
        let Ok(gtk_win) = win_main.gtk_window() else {
            let _ = win_main.set_focus();
            return;
        };
        let mut attempt = 0u32;
        let win_fallback = win_main.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(RETRY_MS), move || {
            attempt += 1;
            if gtk_win.is_mapped() {
                match send_activate(&gtk_win) {
                    Activate::Sent => {
                        if gtk_win.is_active() {
                            return gtk::glib::ControlFlow::Break;
                        }
                    }
                    Activate::NotX11 => {
                        // Wayland: pakai jalur fokus GTK.
                        let _ = win_fallback.set_focus();
                        return gtk::glib::ControlFlow::Break;
                    }
                    Activate::Failed => {}
                }
            }
            if attempt >= MAX_ATTEMPTS {
                gtk::glib::ControlFlow::Break
            } else {
                gtk::glib::ControlFlow::Continue
            }
        });
    });
    if scheduled.is_err() {
        let _ = win.set_focus();
    }
}

enum Activate {
    Sent,
    Failed,
    NotX11,
}

fn send_activate(gtk_win: &gtk::ApplicationWindow) -> Activate {
    let Some(gdk_win) = gtk_win.window() else {
        return Activate::Failed;
    };
    let Ok(display) = gdk_win.display().downcast::<gdkx11::X11Display>() else {
        // Wayland (atau backend GDK lain): biarkan `set_focus()` yang bekerja.
        return Activate::NotX11;
    };
    let xdisplay = unsafe {
        gdkx11::ffi::gdk_x11_display_get_xdisplay(display.to_glib_none().0)
    };
    if xdisplay.is_null() {
        return Activate::NotX11;
    }

    let root = unsafe { x11::xlib::XDefaultRootWindow(xdisplay) };
    let atom = unsafe {
        x11::xlib::XInternAtom(
            xdisplay,
            c"_NET_ACTIVE_WINDOW".as_ptr(),
            x11::xlib::False,
        )
    };

    let mut data = x11::xlib::ClientMessageData::new();
    data.set_long(0, 0); // CurrentTime
    data.set_long(1, 0); // source: unspecified

    let mut event: x11::xlib::XEvent = unsafe { std::mem::zeroed() };
    event.client_message = x11::xlib::XClientMessageEvent {
        type_: x11::xlib::ClientMessage,
        serial: 0,
        send_event: x11::xlib::True,
        display: xdisplay,
        window: root,
        message_type: atom,
        format: 32,
        data,
    };

    let sent = unsafe {
        let status = x11::xlib::XSendEvent(
            xdisplay,
            root,
            x11::xlib::False,
            x11::xlib::SubstructureRedirectMask | x11::xlib::SubstructureNotifyMask,
            &mut event,
        );
        x11::xlib::XFlush(xdisplay);
        status
    };
    if sent != 0 {
        Activate::Sent
    } else {
        Activate::Failed
    }
}
