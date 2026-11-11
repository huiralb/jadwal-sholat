use std::time::Duration as StdDuration;

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::praytimes::TodaySchedule;
use crate::{api, store, toast};

fn at(day: chrono::NaiveDate, time: &str) -> Option<DateTime<Local>> {
    let (h, m) = time.split_once(':')?;
    let t = NaiveTime::from_hms_opt(h.parse().ok()?, m.parse().ok()?, 0)?;
    Local.from_local_datetime(&day.and_time(t)).single()
}

/// Jenis tembakan notifikasi.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NotifKind {
    /// 5 menit sebelum waktu sholat.
    Pengingat,
    /// Tepat waktu sholat.
    Waktu,
}

impl NotifKind {
    /// Label untuk log: "pengingat " atau "" (waktu sholat).
    pub fn log_label(self) -> &'static str {
        match self {
            NotifKind::Pengingat => "pengingat ",
            NotifKind::Waktu => "",
        }
    }
}

/// Satu target notifikasi: kapan ditembak, sholat apa, dan jenisnya.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotifTarget {
    pub fire_at: DateTime<Local>,
    pub prayer_at: DateTime<Local>,
    pub name: &'static str,
    pub kind: NotifKind,
}

/// Pengingat ditembak N menit sebelum waktu sholat.
pub const PENGINGAT_MENIT: i64 = 5;

/// Target notifikasi berikutnya setelah `now`: pengingat 5 menit sebelum
/// sholat berikutnya, atau sholat itu sendiri bila pengingatnya sudah lewat.
/// Setelah Isya: Subuh besok bila datanya tersedia; selain itu None.
pub fn next_notification(
    sched: &TodaySchedule,
    now: DateTime<Local>,
) -> Option<NotifTarget> {
    let today = now.date_naive();
    let times = &sched.times;
    let mut events: Vec<NotifTarget> = Vec::with_capacity(12);
    for (name, time) in [
        ("Subuh", times.subuh.as_str()),
        ("Dzuhur", times.dzuhur.as_str()),
        ("Ashar", times.ashar.as_str()),
        ("Maghrib", times.maghrib.as_str()),
        ("Isya", times.isya.as_str()),
    ] {
        if let Some(prayer_at) = at(today, time) {
            push_events(&mut events, name, prayer_at, now);
        }
    }
    if let Some(tomorrow) = sched.tomorrow_subuh.as_deref() {
        if let Some(prayer_at) = at(today + Duration::days(1), tomorrow) {
            push_events(&mut events, "Subuh", prayer_at, now);
        }
    }
    events.into_iter().min_by_key(|e| e.fire_at)
}

/// Tambahkan pasangan pengingat (T−5 menit) dan tembakan tepat waktu bila
/// masih akan datang.
fn push_events(
    events: &mut Vec<NotifTarget>,
    name: &'static str,
    prayer_at: DateTime<Local>,
    now: DateTime<Local>,
) {
    for (kind, fire_at) in [
        (
            NotifKind::Pengingat,
            prayer_at - Duration::minutes(PENGINGAT_MENIT),
        ),
        (NotifKind::Waktu, prayer_at),
    ] {
        if fire_at > now {
            events.push(NotifTarget {
                fire_at,
                prayer_at,
                name,
                kind,
            });
        }
    }
}

/// Teks notifikasi: judul sesuai jenis + isi kota & jam sholat.
pub fn notification_text(ev: &NotifTarget, kota: &str) -> (String, String) {
    let title = match ev.kind {
        NotifKind::Pengingat => format!("5 Menit Lagi: {}", ev.name),
        NotifKind::Waktu => format!("Waktu {}", ev.name),
    };
    (title, format!("{kota} • {}", ev.prayer_at.format("%H:%M")))
}

/// Format tooltip tray sesuai spec: "KOTA YOGYAKARTA — Ashar 14:52".
pub fn tooltip_text(kota: &str, prayer: &str, hhmm: &str) -> String {
    format!("{kota} — {prayer} {hhmm}")
}

/// True tepat saat `target` tercapai dan belum pernah ditembak untuk target itu.
pub fn notification_due(
    now: DateTime<Local>,
    target: DateTime<Local>,
    last_fired: Option<DateTime<Local>>,
) -> bool {
    now >= target && last_fired != Some(target)
}

/// Jadwal hari ini (+Subuh besok) dari settings + cache/API — dipakai command
/// `get_today_schedule` dan loop scheduler agar satu jalur saja.
pub async fn current_schedule(app: &AppHandle) -> Result<TodaySchedule, String> {
    let settings = store::load(app);
    let kota_id = settings.kota_id.ok_or("lokasi belum dipilih")?;
    let today = Local::now().date_naive();
    let (year, month) = (today.year() as u32, today.month());

    let raw = api::load_month(app, &kota_id, year, month).await?;
    let body = crate::praytimes::parse_month(&raw).map_err(|e| e.to_string())?;
    let kota_nama = settings.kota_nama.unwrap_or_else(|| body.lokasi.clone());
    let today_str = today.format("%Y-%m-%d").to_string();
    let mut sched = crate::praytimes::today_schedule(&body, &kota_nama, &today_str)
        .ok_or_else(|| "jadwal hari ini tidak tersedia".to_string())?;

    if sched.tomorrow_subuh.is_none() {
        // Hari terakhir bulan ini: Subuh besok ada di data bulan berikutnya.
        let tomorrow_str = (today + Duration::days(1)).format("%Y-%m-%d").to_string();
        let (ny, nm) = if month == 12 {
            (year + 1, 1)
        } else {
            (year, month + 1)
        };
        if let Ok(next_raw) = api::load_month(app, &kota_id, ny, nm).await {
            if let Ok(next_body) = crate::praytimes::parse_month(&next_raw) {
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

/// Kirim notifikasi lewat jalur sistem (penempatan monitor dipegang desktop).
fn kirim_sistem(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        println!("[jadwal-sholat] notifikasi gagal: {e}");
    }
}

/// Loop notifikasi senyap: poll ≤30 detik agar reset saat settings/lokasi
/// berubah dan tidak melewati detik pembukaan; tembak tepat pada waktunya.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_fired: Option<DateTime<Local>> = None;
        let mut last_logged_target: Option<NotifTarget> = None;
        let mut last_error: Option<String> = None;
        loop {
            let settings = store::load(&app);
            let sched = if settings.notifikasi {
                match current_schedule(&app).await {
                    Ok(s) => {
                        last_error = None;
                        Some(s)
                    }
                    Err(e) => {
                        if last_error.as_deref() != Some(e.as_str()) {
                            println!("[jadwal-sholat] jadwal gagal: {e}");
                            last_error = Some(e);
                        }
                        None
                    }
                }
            } else {
                None
            };
            let target = sched
                .as_ref()
                .and_then(|s| next_notification(s, Local::now()));
            if target != last_logged_target {
                match (&sched, target) {
                    (Some(_), Some(ev)) => println!(
                        "[jadwal-sholat] target berikutnya: {}{} {}",
                        ev.kind.log_label(),
                        ev.name,
                        ev.fire_at.format("%Y-%m-%d %H:%M")
                    ),
                    _ => println!("[jadwal-sholat] target berikutnya: tidak ada"),
                }
                last_logged_target = target;
                // Tooltip tray: kota + sholat berikutnya (spec Perilaku tray).
                if let Some(tray) = app.tray_by_id("main") {
                    let tip = match (&sched, target) {
                        (Some(s), Some(ev)) => tooltip_text(
                            &s.kota_nama,
                            ev.name,
                            &ev.prayer_at.format("%H:%M").to_string(),
                        ),
                        _ => "Jadwal Sholat".to_string(),
                    };
                    let _ = tray.set_tooltip(Some(tip));
                }
            }
            let now = Local::now();

            match target {
                Some(ev) if (ev.fire_at - now) > Duration::seconds(30) => {
                    tokio::time::sleep(StdDuration::from_secs(30)).await;
                }
                Some(ev) => {
                    let rem = (ev.fire_at - now).to_std().unwrap_or(StdDuration::ZERO);
                    tokio::time::sleep(rem + StdDuration::from_millis(120)).await;
                    if notification_due(Local::now(), ev.fire_at, last_fired) {
                        if let Some(s) = &sched {
                            println!(
                                "[jadwal-sholat] {}{} {} — notifikasi dikirim",
                                ev.kind.log_label(),
                                ev.name,
                                ev.prayer_at.format("%H:%M")
                            );
                            let (title, body) = notification_text(&ev, &s.kota_nama);
                            if settings.notifikasi_monitor.is_all() {
                                kirim_sistem(&app, &title, &body);
                            } else {
                                // Mode monitor utama: toast milik aplikasi, penempatan dipegang kita.
                                if let Err(e) = toast::show(&app, &title, &body) {
                                    println!(
                                        "[jadwal-sholat] toast gagal: {e} — jatuh ke notifikasi sistem"
                                    );
                                    kirim_sistem(&app, &title, &body);
                                }
                            }
                            last_fired = Some(ev.fire_at);
                        }
                    }
                }
                None => {
                    tokio::time::sleep(StdDuration::from_secs(30)).await;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::praytimes::PrayerTimes;
    use chrono::{Timelike, TimeZone};

    fn sched(tomorrow: Option<&str>) -> TodaySchedule {
        TodaySchedule {
            kota_nama: "KOTA YOGYAKARTA".into(),
            tanggal: "2026-11-10".into(),
            times: PrayerTimes {
                subuh: "04:12".into(),
                dzuhur: "11:31".into(),
                ashar: "14:52".into(),
                maghrib: "17:41".into(),
                isya: "18:53".into(),
            },
            tomorrow_subuh: tomorrow.map(String::from),
        }
    }

    fn at(h: u32, m: u32, s: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 10, h, m, s).unwrap()
    }

    #[test]
    fn before_subuh_targets_reminder_first() {
        let ev = next_notification(&sched(Some("04:13")), at(3, 0, 0)).expect("ada");
        assert_eq!(ev.kind, NotifKind::Pengingat);
        assert_eq!(ev.name, "Subuh");
        assert_eq!(ev.fire_at, at(4, 7, 0));
        assert_eq!(ev.prayer_at, at(4, 12, 0));
    }

    #[test]
    fn between_prayers_targets_next_reminder() {
        let ev = next_notification(&sched(Some("04:13")), at(12, 0, 0)).expect("ada");
        assert_eq!(ev.kind, NotifKind::Pengingat);
        assert_eq!(ev.name, "Ashar");
        assert_eq!(ev.fire_at, at(14, 47, 0));
        assert_eq!(ev.prayer_at, at(14, 52, 0));
    }

    #[test]
    fn reminder_passed_targets_the_prayer_itself() {
        // 14:50: pengingat Ashar (14:47) sudah lewat → tembakan berikutnya Ashar.
        let ev = next_notification(&sched(Some("04:13")), at(14, 50, 0)).expect("ada");
        assert_eq!(ev.kind, NotifKind::Waktu);
        assert_eq!(ev.name, "Ashar");
        assert_eq!(ev.fire_at, at(14, 52, 0));
    }

    #[test]
    fn exact_prayer_time_targets_next_reminder() {
        let ev = next_notification(&sched(Some("04:13")), at(14, 52, 0)).expect("ada");
        assert_eq!(ev.kind, NotifKind::Pengingat);
        assert_eq!(ev.name, "Maghrib");
        assert_eq!(ev.fire_at, at(17, 36, 0));
        assert_eq!(ev.prayer_at, at(17, 41, 0));
    }

    #[test]
    fn after_isya_targets_tomorrow_subuh_reminder() {
        let ev = next_notification(&sched(Some("04:13")), at(19, 30, 0)).expect("ada");
        assert_eq!(ev.kind, NotifKind::Pengingat);
        assert_eq!(ev.name, "Subuh");
        assert_eq!(ev.fire_at.hour(), 4);
        assert_eq!(ev.fire_at.minute(), 8);
        assert_eq!(ev.fire_at.day(), 11);
        assert_eq!(ev.prayer_at.day(), 11);
    }

    #[test]
    fn after_isya_without_tomorrow_is_none() {
        assert!(next_notification(&sched(None), at(19, 30, 0)).is_none());
    }

    #[test]
    fn notification_text_follows_kind() {
        let s = sched(Some("04:13"));
        let waktu = next_notification(&s, at(14, 50, 0)).expect("ada");
        assert_eq!(
            notification_text(&waktu, "KOTA JAKARTA"),
            (
                "Waktu Ashar".to_string(),
                "KOTA JAKARTA • 14:52".to_string()
            )
        );
        let pengingat = next_notification(&s, at(12, 0, 0)).expect("ada");
        assert_eq!(
            notification_text(&pengingat, "KOTA JAKARTA"),
            (
                "5 Menit Lagi: Ashar".to_string(),
                "KOTA JAKARTA • 14:52".to_string()
            )
        );
    }

    #[test]
    fn tooltip_follows_plan_format() {
        assert_eq!(
            tooltip_text("KOTA YOGYAKARTA", "Ashar", "14:52"),
            "KOTA YOGYAKARTA — Ashar 14:52"
        );
    }

    #[test]
    fn fires_once_per_target() {
        let target = at(14, 52, 0);
        assert!(!notification_due(at(14, 51, 0), target, None));
        assert!(notification_due(at(14, 52, 0), target, None));
        assert!(!notification_due(at(14, 52, 30), target, Some(target)));
    }
}
