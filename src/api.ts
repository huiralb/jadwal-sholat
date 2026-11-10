import { invoke } from "@tauri-apps/api/core";
import type { PrayerTimes } from "./schedule";

export interface Settings {
  kotaId: string | null;
  kotaNama: string | null;
  notifikasi: boolean;
  autostart: boolean;
}

export interface TodaySchedule {
  kotaNama: string;
  tanggal: string;
  times: PrayerTimes;
  tomorrowSubuh: string | null;
}

export interface Kota {
  id: string;
  lokasi: string;
}

export function getSettings(): Promise<Settings | null> {
  return invoke<Settings | null>("get_settings");
}

export function setSetting(key: string, value: unknown): Promise<void> {
  return invoke<void>("set_setting", { key, value });
}

export function getTodaySchedule(): Promise<TodaySchedule | null> {
  return invoke<TodaySchedule | null>("get_today_schedule");
}

export function searchKota(q: string): Promise<Kota[]> {
  return invoke<Kota[]>("search_kota", { q });
}

export function quitApp(): Promise<void> {
  return invoke<void>("quit");
}
