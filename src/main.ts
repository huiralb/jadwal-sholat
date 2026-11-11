import { listen } from "@tauri-apps/api/event";
import {
  getSettings,
  getTodaySchedule,
  quitApp,
  searchKota,
  setSetting,
  type Kota,
  type Settings,
  type TodaySchedule,
} from "./api";
import {
  formatCountdown,
  msUntil,
  nextPrayer,
  PRAYER_LABELS,
} from "./schedule";

const $ = <T extends HTMLElement>(sel: string): T =>
  document.querySelector(sel) as T;

const viewJadwal = $("#view-jadwal");
const viewPengaturan = $("#view-pengaturan");
const kotaEl = $("#kota");
const tanggalEl = $("#tanggal");
const daftarEl = $("#daftar");
const countdownEl = $("#countdown");
const kotaInput = $<HTMLInputElement>("#kota-input");
const kotaHasil = $("#kota-hasil");
const kotaTerpilih = $("#kota-terpilih");
const toggleNotifikasi = $<HTMLInputElement>("#toggle-notifikasi");
const toggleAutostart = $<HTMLInputElement>("#toggle-autostart");
const monitorDropdown = $("#monitor-dropdown");
const monitorValue = $("#monitor-value");
const monitorMenu = $("#monitor-menu");
const simpanError = $("#simpan-error");

function tampilkanError(pesan: string): void {
  simpanError.textContent = pesan;
}

function bersihkanError(): void {
  simpanError.textContent = "";
}

let settings: Settings | null = null;
let schedule: TodaySchedule | null = null;
let view: "jadwal" | "pengaturan" = "jadwal";

function setView(v: "jadwal" | "pengaturan"): void {
  view = v;
  bersihkanError();
  viewJadwal.classList.toggle("hidden", v !== "jadwal");
  viewPengaturan.classList.toggle("hidden", v !== "pengaturan");
}

function formatTanggal(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString("id-ID", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  });
}

function renderJadwal(now: Date = new Date()): void {
  kotaEl.textContent =
    schedule?.kotaNama ?? settings?.kotaNama ?? "Belum ada lokasi";
  tanggalEl.textContent = schedule ? formatTanggal(schedule.tanggal) : "";
  daftarEl.replaceChildren();

  if (!schedule) {
    countdownEl.textContent =
      "Jadwal belum tersedia — hubungkan internet sekali.";
    return;
  }

  const next = nextPrayer(schedule.times, schedule.tomorrowSubuh, now);
  for (const { key, name } of PRAYER_LABELS) {
    const li = document.createElement("li");
    const nama = document.createElement("span");
    nama.textContent = name;
    const jam = document.createElement("span");
    jam.textContent = schedule.times[key];
    li.append(nama, jam);
    if (next?.name === name) li.classList.add("aktif");
    daftarEl.append(li);
  }
  countdownEl.textContent = next
    ? `${next.name} • ${formatCountdown(msUntil(now, next.time))} lagi`
    : "—";
}

async function refresh(): Promise<void> {
  if (!settings?.kotaId) {
    schedule = null;
    renderJadwal();
    return;
  }
  try {
    schedule = await getTodaySchedule();
  } catch {
    schedule = null;
  }
  renderJadwal();
}

async function doSearch(): Promise<void> {
  const q = kotaInput.value.trim();
  kotaHasil.replaceChildren();
  if (q.length < 2) return;
  let results: Kota[] = [];
  try {
    results = await searchKota(q);
  } catch {
    return; // data layer belum siap / offline
  }
  for (const k of results.slice(0, 8)) {
    const li = document.createElement("li");
    li.textContent = k.lokasi;
    li.addEventListener("click", () => void pilihKota(k));
    kotaHasil.append(li);
  }
}

async function pilihKota(k: Kota): Promise<void> {
  try {
    await setSetting("kotaId", k.id);
    await setSetting("kotaNama", k.lokasi);
  } catch {
    tampilkanError("Gagal menyimpan lokasi — coba lagi.");
    return;
  }
  if (settings) {
    settings.kotaId = k.id;
    settings.kotaNama = k.lokasi;
  }
  bersihkanError();
  kotaTerpilih.textContent = k.lokasi;
  kotaHasil.replaceChildren();
  kotaInput.value = "";
  await refresh();
  setView("jadwal");
}

function simpanToggle(key: "notifikasi" | "autostart", value: boolean): void {
  const sebelum = settings ? settings[key] : null;
  if (settings) settings[key] = value;
  setSetting(key, value)
    .then(bersihkanError)
    .catch(() => {
      // Gagal menyimpan: kembalikan state lokal agar UI tidak berbohong.
      if (settings && sebelum !== null) settings[key] = sebelum;
      toggleNotifikasi.checked = settings?.notifikasi ?? true;
      toggleAutostart.checked = settings?.autostart ?? false;
      tampilkanError("Gagal menyimpan pengaturan.");
    });
}

function renderMonitor(value: "all" | "primary"): void {
  monitorValue.textContent =
    value === "primary" ? "Monitor utama (ikon tray)" : "Semua monitor";
  monitorMenu.querySelectorAll("li").forEach((opt) => {
    opt.classList.toggle("aktif", opt.dataset.value === value);
  });
}

function simpanMonitor(value: "all" | "primary"): void {
  const sebelum = settings?.notifikasiMonitor ?? "all";
  if (settings) settings.notifikasiMonitor = value;
  renderMonitor(value);
  setSetting("notifikasiMonitor", value)
    .then(bersihkanError)
    .catch(() => {
      // Gagal menyimpan: kembalikan state lokal agar UI tidak berbohong.
      if (settings) settings.notifikasiMonitor = sebelum;
      renderMonitor(settings?.notifikasiMonitor ?? "all");
      tampilkanError("Gagal menyimpan pengaturan.");
    });
}

function bindEvents(): void {
  let timer: number | undefined;
  kotaInput.addEventListener("input", () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => void doSearch(), 250);
  });
  toggleNotifikasi.addEventListener("change", () =>
    simpanToggle("notifikasi", toggleNotifikasi.checked),
  );
  const tutupMenuMonitor = (): void => {
    monitorMenu.classList.add("hidden");
    monitorDropdown.setAttribute("aria-expanded", "false");
  };
  monitorDropdown.addEventListener("click", (e) => {
    e.stopPropagation();
    const terbuka = !monitorMenu.classList.contains("hidden");
    monitorMenu.classList.toggle("hidden", terbuka);
    monitorDropdown.setAttribute("aria-expanded", String(!terbuka));
  });
  monitorMenu.addEventListener("click", (e) => {
    const value = (e.target as HTMLElement).closest<HTMLElement>("li")?.dataset.value;
    if (value === "all" || value === "primary") {
      simpanMonitor(value);
      tutupMenuMonitor();
    }
  });
  monitorDropdown.addEventListener("keydown", (e) => {
    if (e.key === "Escape") tutupMenuMonitor();
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      monitorDropdown.click();
    }
  });
  document.addEventListener("click", tutupMenuMonitor);
  toggleAutostart.addEventListener("change", () =>
    simpanToggle("autostart", toggleAutostart.checked),
  );
  $("#btn-pengaturan").addEventListener("click", () => setView("pengaturan"));
  $("#btn-kembali").addEventListener("click", () => setView("jadwal"));
  $("#btn-keluar").addEventListener("click", () => void quitApp());

  void listen<string>("open-view", (e) => {
    tutupMenuMonitor();
    // Halaman jadwal hanya bermakna bila lokasi sudah dipilih (seperti init).
    setView(
      e.payload === "settings" || !settings?.kotaId ? "pengaturan" : "jadwal",
    );
    if (view === "jadwal") void refresh();
  });
}

async function syncFromBackend(): Promise<void> {
  try {
    settings = await getSettings();
  } catch {
    settings = null;
  }
  toggleNotifikasi.checked = settings?.notifikasi ?? true;
  toggleAutostart.checked = settings?.autostart ?? false;
  renderMonitor(settings?.notifikasiMonitor ?? "all");
  kotaTerpilih.textContent = settings?.kotaNama ?? "";
}

async function init(): Promise<void> {
  await syncFromBackend();

  bindEvents();

  if (settings?.kotaId) {
    await refresh();
    setView("jadwal");
  } else {
    setView("pengaturan");
  }

  // Panel ditampilkan ulang (tray/menu) → tarik state terbaru, termasuk
  // perubahan yang dibuat lewat menu tray selama panel tersembunyi.
  window.addEventListener("focus", () => {
    void (async () => {
      await syncFromBackend();
      if (view === "jadwal") await refresh();
    })();
  });

  window.setInterval(() => {
    if (view === "jadwal") renderJadwal(new Date());
  }, 1000);
}

window.addEventListener("DOMContentLoaded", () => void init());
