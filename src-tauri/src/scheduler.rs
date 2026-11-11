use std::time::Duration as StdDuration;

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::praytimes::TodaySchedule;
use crate::{api, store};

fn at(day: chrono::NaiveDate, time: &str) -> Option<DateTime<Local>> {
    let (h, m) = time.split_once(':')?;
    let t = NaiveTime::from_hms_opt(h.parse().ok()?, m.parse().ok()?, 0)?;
    Local.from_local_datetime(&day.and_time(t)).single()
}

/// Waktu sholat berikutnya setelah `now` (menit :00). Setelah Isya: Subuh
/// besok bila datanya tersedia; selain itu None.
pub fn next_notification_at(
    sched: &TodaySchedule,
    now: DateTime<Local>,
) -> Option<DateTime<Local>> {
    let times = &sched.times;
    let today = now.date_naive();
    let order = [
        times.subuh.as_str(),
        times.dzuhur.as_str(),
        times.ashar.as_str(),
        times.maghrib.as_str(),
        times.isya.as_str(),
    ];
    for time in order {
        if let Some(dt) = at(today, time) {
            if dt > now {
                return Some(dt);
            }
        }
    }
    let tomorrow = sched.tomorrow_subuh.as_deref()?;
    at(today + Duration::days(1), tomorrow)
}

/// Nama sholat untuk target: cocokkan waktu target dengan daftar hari ini;
/// target lintas tanggal hanya mungkin Subuh besok.
pub fn prayer_name_at(sched: &TodaySchedule, target: DateTime<Local>) -> &'static str {
    let times = [
        ("Subuh", sched.times.subuh.as_str()),
        ("Dzuhur", sched.times.dzuhur.as_str()),
        ("Ashar", sched.times.ashar.as_str()),
        ("Maghrib", sched.times.maghrib.as_str()),
        ("Isya", sched.times.isya.as_str()),
    ];
    for (name, time) in times {
        if at(target.date_naive(), time).as_ref() == Some(&target) {
            return name;
        }
    }
    "Subuh"
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

/// Loop notifikasi senyap: poll ≤30 detik agar reset saat settings/lokasi
/// berubah dan tidak melewati detik pembukaan; tembak tepat pada waktunya.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_fired: Option<DateTime<Local>> = None;
        let mut last_logged_target: Option<DateTime<Local>> = None;
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
                .and_then(|s| next_notification_at(s, Local::now()));
            if target != last_logged_target {
                match (&sched, target) {
                    (Some(s), Some(t)) => println!(
                        "[jadwal-sholat] target berikutnya: {} {}",
                        prayer_name_at(s, t),
                        t.format("%Y-%m-%d %H:%M")
                    ),
                    _ => println!("[jadwal-sholat] target berikutnya: tidak ada"),
                }
                last_logged_target = target;
                // Tooltip tray: kota + sholat berikutnya (spec Perilaku tray).
                if let Some(tray) = app.tray_by_id("main") {
                    let tip = match (&sched, target) {
                        (Some(s), Some(t)) => tooltip_text(
                            &s.kota_nama,
                            prayer_name_at(s, t),
                            &t.format("%H:%M").to_string(),
                        ),
                        _ => "Jadwal Sholat".to_string(),
                    };
                    let _ = tray.set_tooltip(Some(tip));
                }
            }
            let now = Local::now();

            match target {
                Some(t) if (t - now) > Duration::seconds(30) => {
                    tokio::time::sleep(StdDuration::from_secs(30)).await;
                }
                Some(t) => {
                    let rem = (t - now).to_std().unwrap_or(StdDuration::ZERO);
                    tokio::time::sleep(rem + StdDuration::from_millis(120)).await;
                    if notification_due(Local::now(), t, last_fired) {
                        if let Some(s) = &sched {
                            let name = prayer_name_at(s, t);
                            println!(
                                "[jadwal-sholat] {} {} — notifikasi dikirim",
                                name,
                                t.format("%H:%M")
                            );
                            if let Err(e) = app
                                .notification()
                                .builder()
                                .title(format!("Waktu {name}"))
                                .body(format!("{} • {}", s.kota_nama, t.format("%H:%M")))
                                .show()
                            {
                                println!("[jadwal-sholat] notifikasi gagal: {e}");
                            }
                            last_fired = Some(t);
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
    fn before_subuh_targets_subuh_today() {
        let target = next_notification_at(&sched(Some("04:13")), at(3, 0, 0)).expect("ada");
        assert_eq!(target, at(4, 12, 0));
    }

    #[test]
    fn between_prayers_targets_next_today() {
        let target = next_notification_at(&sched(Some("04:13")), at(12, 0, 0)).expect("ada");
        assert_eq!(target, at(14, 52, 0));
    }

    #[test]
    fn exact_prayer_time_is_already_passed() {
        let target = next_notification_at(&sched(Some("04:13")), at(14, 52, 0)).expect("ada");
        assert_eq!(target, at(17, 41, 0));
    }

    #[test]
    fn after_isya_targets_tomorrow_subuh() {
        let target = next_notification_at(&sched(Some("04:13")), at(19, 30, 0)).expect("ada");
        assert_eq!(target.hour(), 4);
        assert_eq!(target.minute(), 13);
        assert_eq!(target.day(), 11);
    }

    #[test]
    fn after_isya_without_tomorrow_is_none() {
        assert!(next_notification_at(&sched(None), at(19, 30, 0)).is_none());
    }

    #[test]
    fn names_target_prayer_for_notification_title() {
        let s = sched(Some("04:13"));
        assert_eq!(prayer_name_at(&s, at(4, 12, 0)), "Subuh");
        assert_eq!(prayer_name_at(&s, at(14, 52, 0)), "Ashar");
        assert_eq!(prayer_name_at(&s, at(17, 41, 0)), "Maghrib");
        // Target besok (lintas tanggal) selalu Subuh.
        let tomorrow = Local.with_ymd_and_hms(2026, 11, 11, 4, 13, 0).unwrap();
        assert_eq!(prayer_name_at(&s, tomorrow), "Subuh");
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
