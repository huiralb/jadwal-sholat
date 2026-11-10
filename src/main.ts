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

let settings: Settings | null = null;
let schedule: TodaySchedule | null = null;
let view: "jadwal" | "pengaturan" = "jadwal";

function setView(v: "jadwal" | "pengaturan"): void {
  view = v;
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
    return;
  }
  if (settings) {
    settings.kotaId = k.id;
    settings.kotaNama = k.lokasi;
  }
  kotaTerpilih.textContent = k.lokasi;
  kotaHasil.replaceChildren();
  kotaInput.value = "";
  await refresh();
  setView("jadwal");
}

function simpanToggle(key: "notifikasi" | "autostart", value: boolean): void {
  if (settings) settings[key] = value;
  setSetting(key, value).catch(() => {
    // data layer belum siap sampai Task 5
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
  toggleAutostart.addEventListener("change", () =>
    simpanToggle("autostart", toggleAutostart.checked),
  );
  $("#btn-pengaturan").addEventListener("click", () => setView("pengaturan"));
  $("#btn-kembali").addEventListener("click", () => setView("jadwal"));
  $("#btn-keluar").addEventListener("click", () => void quitApp());

  void listen<string>("open-view", (e) => {
    setView(e.payload === "settings" ? "pengaturan" : "jadwal");
    if (view === "jadwal") void refresh();
  });
}

async function init(): Promise<void> {
  try {
    settings = await getSettings();
  } catch {
    settings = null;
  }
  toggleNotifikasi.checked = settings?.notifikasi ?? true;
  toggleAutostart.checked = settings?.autostart ?? false;
  kotaTerpilih.textContent = settings?.kotaNama ?? "";

  bindEvents();

  if (settings?.kotaId) {
    await refresh();
    setView("jadwal");
  } else {
    setView("pengaturan");
  }

  window.setInterval(() => {
    if (view === "jadwal") renderJadwal(new Date());
  }, 1000);
}

window.addEventListener("DOMContentLoaded", () => void init());
