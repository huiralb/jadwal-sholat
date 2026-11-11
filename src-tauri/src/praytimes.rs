use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Kota {
    pub id: String,
    pub lokasi: String,
}

/// Monitor tempat notifikasi ditampilkan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NotifikasiMonitor {
    /// Notifikasi sistem — penempatan dipegang desktop, muncul di semua monitor.
    #[default]
    All,
    /// Toast milik aplikasi — hanya monitor yang mengandung ikon tray.
    Primary,
}

impl NotifikasiMonitor {
    /// Nilai dari store: selain "primary" dianggap "all" (perilaku lama).
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("primary") => Self::Primary,
            _ => Self::All,
        }
    }

    pub fn is_primary(self) -> bool {
        matches!(self, Self::Primary)
    }

    pub fn is_all(self) -> bool {
        matches!(self, Self::All)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub kota_id: Option<String>,
    pub kota_nama: Option<String>,
    pub notifikasi: bool,
    pub notifikasi_monitor: NotifikasiMonitor,
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            kota_id: None,
            kota_nama: None,
            notifikasi: true,
            notifikasi_monitor: NotifikasiMonitor::All,
            autostart: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PrayerTimes {
    pub subuh: String,
    pub dzuhur: String,
    pub ashar: String,
    pub maghrib: String,
    pub isya: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodaySchedule {
    pub kota_nama: String,
    pub tanggal: String,
    pub times: PrayerTimes,
    pub tomorrow_subuh: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DayJadwal {
    pub date: String,
    pub subuh: String,
    pub dzuhur: String,
    pub ashar: String,
    pub maghrib: String,
    pub isya: String,
}

#[derive(Debug, Deserialize)]
pub struct MonthBody {
    pub lokasi: String,
    pub jadwal: Vec<DayJadwal>,
}

#[derive(Debug, Deserialize)]
struct MyQuranMonth {
    data: MonthBody,
}

#[derive(Debug, Deserialize)]
struct MyQuranKota {
    data: Vec<Kota>,
}

/// Parse respons daftar kota (`kota/semua` / `kota/cari`).
pub fn parse_kota_list(json: &str) -> Result<Vec<Kota>, serde_json::Error> {
    Ok(serde_json::from_str::<MyQuranKota>(json)?.data)
}

/// Parse respons bulanan myquran (`data.lokasi` + `data.jadwal[]`).
pub fn parse_month(json: &str) -> Result<MonthBody, serde_json::Error> {
    Ok(serde_json::from_str::<MyQuranMonth>(json)?.data)
}

/// Jadwal hari `date` (YYYY-MM-DD) beserta Subuh besok bila tersedia
/// di data bulan yang sama.
pub fn today_schedule(
    body: &MonthBody,
    kota_nama: &str,
    date: &str,
) -> Option<TodaySchedule> {
    let today = body.jadwal.iter().find(|d| d.date == date)?;
    let tomorrow: Option<String> = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .map(|d| d + Duration::days(1))
        .map(|d| d.format("%Y-%m-%d").to_string())
        .and_then(|next| {
            body.jadwal
                .iter()
                .find(|d| d.date == next)
                .map(|d| d.subuh.clone())
        });
    Some(TodaySchedule {
        kota_nama: kota_nama.to_string(),
        tanggal: today.date.clone(),
        times: PrayerTimes {
            subuh: today.subuh.clone(),
            dzuhur: today.dzuhur.clone(),
            ashar: today.ashar.clone(),
            maghrib: today.maghrib.clone(),
            isya: today.isya.clone(),
        },
        tomorrow_subuh: tomorrow,
    })
}

pub fn cache_filename(kota_id: &str, year: u32, month: u32) -> String {
    format!("jadwal-{kota_id}-{year}-{month:02}.json")
}

pub fn filter_kota(list: &[Kota], q: &str) -> Vec<Kota> {
    let needle = q.to_lowercase();
    list.iter()
        .filter(|k| k.lokasi.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
      "status": true,
      "data": {
        "id": 1505,
        "lokasi": "KOTA YOGYAKARTA",
        "daerah": "D.I. YOGYAKARTA",
        "jadwal": [
          {"tanggal":"Selasa, 10/11/2026","imsak":"03:40","subuh":"03:50","terbit":"05:06","dhuha":"05:34","dzuhur":"11:26","ashar":"14:44","maghrib":"17:39","isya":"18:52","date":"2026-11-10"},
          {"tanggal":"Rabu, 11/11/2026","imsak":"03:40","subuh":"03:50","terbit":"05:06","dhuha":"05:34","dzuhur":"11:26","ashar":"14:44","maghrib":"17:39","isya":"18:52","date":"2026-11-11"}
        ]
      }
    }"#;

    #[test]
    fn parses_myquran_month_response() {
        let body = parse_month(FIXTURE).expect("parse");
        assert_eq!(body.lokasi, "KOTA YOGYAKARTA");
        assert_eq!(body.jadwal.len(), 2);
        assert_eq!(body.jadwal[0].subuh, "03:50");
        assert_eq!(body.jadwal[1].date, "2026-11-11");
    }

    #[test]
    fn builds_today_schedule_with_tomorrow_subuh() {
        let body = parse_month(FIXTURE).expect("parse");
        let s = today_schedule(&body, "KOTA YOGYAKARTA", "2026-11-10").expect("ada");
        assert_eq!(s.kota_nama, "KOTA YOGYAKARTA");
        assert_eq!(s.tanggal, "2026-11-10");
        assert_eq!(s.times.ashar, "14:44");
        assert_eq!(s.times.isya, "18:52");
        assert_eq!(s.tomorrow_subuh.as_deref(), Some("03:50"));
    }

    #[test]
    fn today_schedule_is_none_when_date_missing() {
        let body = parse_month(FIXTURE).expect("parse");
        assert!(today_schedule(&body, "X", "2026-12-01").is_none());
    }

    #[test]
    fn tomorrow_subuh_is_none_on_last_available_day() {
        let body = parse_month(FIXTURE).expect("parse");
        let s = today_schedule(&body, "X", "2026-11-11").expect("ada");
        assert_eq!(s.tomorrow_subuh, None);
    }

    #[test]
    fn cache_filename_includes_kota_and_month() {
        assert_eq!(cache_filename("1505", 2026, 11), "jadwal-1505-2026-11.json");
    }

    #[test]
    fn filter_kota_matches_case_insensitively() {
        let list = vec![
            Kota { id: "1505".into(), lokasi: "KOTA YOGYAKARTA".into() },
            Kota { id: "1501".into(), lokasi: "KAB. BANTUL".into() },
        ];
        let hits = filter_kota(&list, "yogya");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "1505");
        assert_eq!(filter_kota(&list, "bantul").len(), 1);
        assert_eq!(filter_kota(&list, "zzz").len(), 0);
    }

    #[test]
    fn parses_kota_list_response() {
        let json = r#"{"status":true,"request":"/sholat/kota/semua","data":[
            {"id":"1505","lokasi":"KOTA YOGYAKARTA"},
            {"id":"1501","lokasi":"KAB. BANTUL"}]}"#;
        let list = parse_kota_list(json).expect("parse");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "1505");
        assert_eq!(list[1].lokasi, "KAB. BANTUL");
    }

    #[test]
    fn settings_defaults_match_design() {
        let s = Settings::default();
        assert_eq!(s.kota_id, None);
        assert_eq!(s.kota_nama, None);
        assert!(s.notifikasi);
        assert!(s.notifikasi_monitor.is_all());
        assert!(!s.autostart);
    }

    #[test]
    fn settings_serializes_camel_case_for_js() {
        let v = serde_json::to_value(Settings::default()).expect("serialize");
        assert!(v.get("kotaId").is_some(), "kotaId hilang: {v}");
        assert!(v.get("kotaNama").is_some(), "kotaNama hilang: {v}");
        assert!(v.get("kota_id").is_none(), "kota_id tidak boleh bocor: {v}");
        assert_eq!(
            v.get("notifikasiMonitor").and_then(|m| m.as_str()),
            Some("all"),
            "notifikasiMonitor hilang: {v}"
        );
    }

    #[test]
    fn notifikasi_monitor_parse_defaults_to_all() {
        assert!(NotifikasiMonitor::parse(None).is_all());
        assert!(NotifikasiMonitor::parse(Some("all")).is_all());
        assert!(NotifikasiMonitor::parse(Some("primary")).is_primary());
        assert!(NotifikasiMonitor::parse(Some("acak")).is_all());
    }
}
