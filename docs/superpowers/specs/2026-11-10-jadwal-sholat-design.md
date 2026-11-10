# Jadwal Sholat — Design Doc + Implementation Plan

Tanggal: 2026-11-10
Status: disetujui — plan review 2026-11-10
Catatan: doc ini ditulis di lokasi plan mode (`~/.commandcode/plans/`); saat implementasi, salin ke
`docs/superpowers/specs/2026-11-10-jadwal-sholat-design.md` di dalam repo dan commit.

## Intent (why)

Aplikasi desktop pribadi "Jadwal Sholat": ringan, native, hidup sepenuhnya di system tray
— meniru pengalaman MEGAsync (Qt) yang sudah terpasang di mesin ini: ikon tray, panel kecil
saat diklik, autostart saat login, tanpa window utama. Tujuannya memberi tahu waktu sholat
(lewat countdown dan notifikasi) tanpa mengganggu.

## Goals

- Tray-only: tanpa window utama; ikon tray + panel popup + context menu.
- Panel popup saat klik kiri: jadwal 5 waktu sholat hari ini + countdown ke sholat
  berikutnya, posisi muncul dekat ikon tray (seperti panel MEGA).
- Notifikasi desktop senyap saat masuk waktu sholat.
- Jadwal: API myquran (jadwal Kemenag) + cache bulanan lokal; kota dipilih sekali;
  notifikasi tetap jalan saat offline (dari cache).
- Autostart saat login (bisa di-toggle, default mati).
- Ringan: binary kecil, RAM rendah, tanpa telemetry/iklan.
- Linux dulu (GNOME/Ubuntu); arsitektur siap lintas-platform (Windows menyusul).

## Non-goals (v1)

- Arah/kiblat, kompas, imsak/puasa khusus, multi-lokasi, tampilan 30 hari.
- Packaging Windows di v1 (hanya dipastikan tidak ada yang Linux-spesifik di logika inti).

## Menyusul (direncanakan, bukan dibuang)

- **Audio adzan (bunyi saat waktu sholat): menyusul di rilis berikutnya.** v1 notifikasi
  senyap. Arsitektur sudah disiapkan: scheduler sudah mengetahui momen persis masuknya
  waktu sholat, jadi penambahan audio tinggal melakukan playback di momen yang sama —
  tanpa mengubah struktur data, settings, atau alur notifikasi. Settings nanti cukup
  menambah `audioEnabled` (+ opsi volume/urutan) dan file audio disertakan di bundling.

## Anti-goals

- Jangan ada window utama atau ikon dock/taskbar selain tray.
- Jangan wajib internet untuk notifikasi (cache harus cukup).
- Jangan berat: target RAM < 100 MB saat idle, start cepat.
- Jangan ada telemetri, iklan, akun, atau ijin yang tidak perlu.

## Constraints

- Mesin ini: toolchain `cargo`/`rustc` + `node` terpasang (bahkan `tauri-driver`);
  Qt dan Go TIDAK terpasang → pilihan terbatas pada yang bisa dibangun di sini.
- Referensi UX: MEGAsync di `/opt/megasync` = Qt5 + QML, tray icon + panel + autostart.
- Lingkungan terverifikasi (builder check): sesi **X11** (positioning window bekerja),
  desktop **Cinnamon** (tray native, tanpa ekstensi), zona waktu **WIB** (`chrono::Local`
  cukup), tanpa ekstensi GNOME.

## Builder check — hasil verifikasi (2026-11-10)

Reviewer subagent terhenti karena izin, jadi doc ini dibaca ulang sebagai builder;
semua titik rawan diverifikasi langsung (read-only):

- Endpoint myquran hidup dan bentuknya sesuai doc:
  - `GET /v2/sholat/kota/semua` → daftar lengkap kab/kota se-Indonesia
    (`data[] = {id, lokasi}`) — untuk dropdown Pengaturan, di-cache juga.
  - `GET /v2/sholat/kota/cari/yogya` → `{"data":[{"id":"1505","lokasi":"KOTA YOGYAKARTA"}]}`
  - `GET /v2/sholat/jadwal/1505/2026/11` → `data.jadwal[]` per hari berisi
    `tanggal, imsak, subuh, terbit, dhuha, dzuhur, ashar, maghrib, isya, date` (field yang
    dipakai: subuh/dzuhur/ashar/maghrib/isya + `date` untuk lookup harian).
- Sesi X11 → panel bisa diposisikan persis dekat ikon tray (di Wayland hal ini rawan;
  tidak relevan di mesin ini, tetap dicatat agar versi Windows/Wayland menyusul sadar).
- Tidak ada pertanyaan tersisa yang hanya bisa dijawab user; sisanya minor dan menjadi
  call builder (tercantum di "What's left to the builder").

## Approaches (yang dipertimbangkan)

1. **Tauri 2 (Rust + UI HTML/CSS) — DIPILIH.** Toolchain siap; plugin resmi tray,
   notification, autostart, store; binary ~MB-an dan hemat RAM; panel UI bebas HTML/CSS;
   lintas platform. Kekurangan: UI dirender webview sistem (bukan Qt native), positioning
   popup dihitung sendiri dari koordinat event tray (Tauri 2 `TrayIconEvent` menyediakan
   koordinat — sudah memadai).
2. **Qt/C++/QML — ditolak.** Paling persis dengan MEGA, tapi toolchain Qt tidak terpasang
   (instalasi besar), C++/QML paling lambat dan paling banyak kode untuk fitur sesederhana ini.
3. **Python + PySide6 — ditolak.** Qt juga, tapi butuh `pip install`, packaging ~100 MB,
   start lebih lambat; paling jauh dari kesan "ringan".
4. **Electron — ditolak dari awal:** bukan native (syarat eksplisit).

## Keputusan yang sudah disepakati

- Perilaku: panel popup + countdown + notifikasi desktop (bukan menu saja).
- Sumber jadwal: API + cache lokal, kota manual (dropdown), bukan deteksi IP / hitung offline.
- Teknologi: Tauri 2 (Rust).
- Notifikasi v1 senyap; bunyi audio adzan menyusul di rilis berikutnya (lihat "Menyusul").
- Linux first, arsitektur siap Windows.
- Lokasi proyek: `/home/ir/workspace/jadwal-sholat` (repo baru).

## Design

### Komponen

```
src-tauri/            Rust
  main.rs / lib.rs    setup: tray icon + menu, window panel frameless, plugin init
  scheduler.rs        tokio task: sleep sampai waktu sholat berikutnya → kirim notifikasi
  praytimes.rs        model jadwal + turunan "next prayer" + cache bulanan
  api.rs              fetch kota & jadwal bulanan (myquran), fallback error handling
  store.rs            baca/tulis settings JSON (tauri-plugin-store)
ui/
  index.html/css/js   panel: jadwal, countdown, view Pengaturan (tanpa framework, vanilla TS)
```

- Plugin Tauri: `notification`, `store`, `autostart`, `positioner` (jika perlu),
  HTTP via `reqwest` di Rust (lebih sederhana daripada plugin http untuk kebutuhan ini).
- Satu window frameless, always-on-top, `visible: false` awal; hide saat focus hilang.

### Perilaku tray

- Klik kiri: toggle panel, muncul dekat ikon tray (posisi dari event tray, preferensi
  bottom-right mengikuti lokasi ikon).
- Klik kanan: native context menu — "Buka Jadwal", "Pengaturan", "Autostart (cek)",
  "Keluar".
- Tooltip tray: nama kota + sholat berikutnya + waktunya (mis. "Yogyakarta — Ashar 14:52").

### Panel (sketsa disetujui user)

```
┌────────────────────────────┐
│  Yogyakarta                │
│  Selasa, 10 November 2026  │
├────────────────────────────┤
│  Subuh     04:12           │
│  Dzuhur    11:31           │
│  Ashar     14:52   ← sorot │
│  Maghrib   17:41           │
│  Isya      18:53           │
├────────────────────────────┤
│  Ashar • 01:24:10 lagi     │
├────────────────────────────┤
│  Pengaturan        Keluar │
└────────────────────────────┘
```

- Baris sholat berikutnya disorot; countdown update tiap detik (JS).
- "Pengaturan" = swap view di dalam panel yang sama (bukan window baru):
  dropdown/cari kota (sekali, dari API kota myquran), toggle notifikasi, toggle autostart.
- First-run: belum ada kota → panel langsung membuka view pemilihan kota.
- Bahasa UI: Indonesia.

### Data flow

1. Start: baca settings → baca cache bulan ini → jika tidak ada/kedaluwarsa, fetch API →
   simpan cache.
2. Refresh: cek cache tiap start + harian; fetch ulang jika bulan berganti atau settings
   kota berubah.
3. Scheduler: task terpisah, hitung waktu sholat berikutnya dari jadwal tersimpan, sleep
   sampai saatnya → `notification.show()`; di-reset jika settings/cache berubah.
   Fallback kecepatan timer 30 detik agar tidak melewati detik pembukaan.
4. Offline: fetch gagal → pakai cache; jika tidak ada cache sama sekali → pesan di panel
   "jadwal belum tersedia, hubungkan internet sekali".

### Sumber data (dipilih builder, ditandai sebagai call)

- API myquran v2 (gratis, tanpa kunci, berbasis Kemenag):
  - Daftar kota: `GET https://api.myquran.com/v2/sholat/kota/semua`
  - Jadwal bulanan: `GET https://api.myquran.com/v2/sholat/jadwal/{id}/{tahun}/{bulan}`
    → `data.jadwal` berisi `subuh, dzuhur, ashar, maghrib, isya` (+ imsak/terbit, diabaikan).
- Cache: file JSON per bulan di direktori config Tauri (`~/.config/jadwal-sholat/`).
- Jika API ternyata bermasalah saat implementasi: fallback aladhan.com API
  (`method=20` Kemenag) dengan kota manual — interface data dijaga sama agar bisa tukar.

### Settings (JSON via tauri-plugin-store)

`{ kotaId, kotaNama, notifikasi: bool (default true), autostart: bool (default false) }`

### Packaging

- Icon: SVG/PNG (ikon masjid/interior monokrom, gaya minimalis MEGA).
- `tauri build` → `.deb` + `AppImage`; bundler Tauri membuat desktop entry.
- Autostart via `tauri-plugin-autostart` (default mati, toggle di Pengaturan).

## What's left to the builder (call saya, sudah ditandai)

- Detail endpoint & bentuk cache (di atas).
- Desain pixel panel: mengikuti sketsa, tema gelap minimalis ala panel MEGA (call: gelap).
- Default autostart = mati; default notifikasi = nyala.
- Penanganan notifikasi pertama di GNOME (permission `XDG`): dijalankan uji manual.

## Implementation plan

### Task 1: Scaffold Tauri app

Scaffold: `npm create tauri-app@latest jadwal-sholat` (template vanilla TypeScript) di
`/home/ir/workspace/jadwal-sholat`; pastikan `cargo check` dan `tauri dev` jalan.

### Task 2: Plugin & dependensi

Tambah plugin: `tauri-plugin-notification`, `tauri-plugin-store`, `tauri-plugin-autostart`;
`reqwest` + `serde` + `chrono` di Cargo.

### Task 3: Tray icon & menu

Tray (Rust): icon + tooltip + native menu; event klik kiri → show/hide panel pada
koordinat tray; klik kanan → menu.

### Task 4: Panel UI

Panel UI: sketsa di atas, countdown per detik, view Pengaturan (kota search + 2 toggle),
first-run kota picker; frameless + focus-lost hide + posisi dekat ikon.

### Task 5: Data layer

Data layer: fetch kota & jadwal bulanan, cache JSON, refresh logic, error state offline.

### Task 6: Scheduler notifikasi

Scheduler notifikasi: hitung next prayer, sleep, notifikasi senyap; reset saat data/setting
berubah; log ke stdout untuk verifikasi.

### Task 7: Autostart toggle

Autostart toggle.

### Task 8: Ikon & bundling

Ikon + bundling: `tauri build` → deb/AppImage, desktop entry, uji instal.

### Task 9: Dokumentasi & commit

Copy design doc ini ke `docs/superpowers/specs/` di repo + commit.

## Verifikasi

- `cargo check` dan `npm run tauri build` tanpa error.
- Manual (tauri dev):
  - Ikon tray muncul; klik kiri → panel muncul dekat ikon, countdown berdetak;
  - klik kanan → menu berfungsi; Pengaturan ganti kota → jadwal berganti;
  - set waktu sholat uji beberapa menit ke depan → notifikasi muncul (verifikasi via log
    scheduler + layar);
  - matikan jaringan → start ulang → panel tetap tampil dari cache, notifikasi tetap terjadwal;
  - toggle autostart → cek entri autostart di `~/.config/autostart/`.
- Install: jalankan `.deb`/`AppImage` hasil build, start dari desktop entry, ulangi cek tray.
