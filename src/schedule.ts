export interface PrayerTimes {
  subuh: string;
  dzuhur: string;
  ashar: string;
  maghrib: string;
  isya: string;
}

export interface NextPrayer {
  name: string;
  time: string;
}

const ORDER: Array<{ key: keyof PrayerTimes; name: string }> = [
  { key: "subuh", name: "Subuh" },
  { key: "dzuhur", name: "Dzuhur" },
  { key: "ashar", name: "Ashar" },
  { key: "maghrib", name: "Maghrib" },
  { key: "isya", name: "Isya" },
];

export const PRAYER_LABELS = ORDER.map((p) => ({ key: p.key, name: p.name }));

export function parseTime(time: string): number {
  const [h, m] = time.split(":").map(Number);
  return h * 60 + m;
}

/** Sholat berikutnya setelah `now`. Setelah Isya: Subuh besok (butuh
 * `tomorrowSubuh`; null bila tidak diketahui). Waktu sholat yang tepat
 * sama dengan `now` dianggap sudah lewat. */
export function nextPrayer(
  times: PrayerTimes,
  tomorrowSubuh: string | null,
  now: Date,
): NextPrayer | null {
  const nowMin = now.getHours() * 60 + now.getMinutes() + now.getSeconds() / 60;
  for (const { key, name } of ORDER) {
    const time = times[key];
    if (parseTime(time) > nowMin) return { name, time };
  }
  return tomorrowSubuh ? { name: "Subuh", time: tomorrowSubuh } : null;
}

/** Date pada hari yang sama dengan `now` pada jam `time`. */
export function prayerDate(now: Date, time: string): Date {
  const [h, m] = time.split(":").map(Number);
  const d = new Date(now);
  d.setHours(h, m, 0, 0);
  return d;
}

/** Milidetik dari `now` sampai `time`; bila `time` sudah lewat hari ini,
 * dihitung ke hari berikutnya. */
export function msUntil(now: Date, time: string): number {
  let target = prayerDate(now, time);
  if (target.getTime() <= now.getTime()) {
    target = new Date(target);
    target.setDate(target.getDate() + 1);
  }
  return target.getTime() - now.getTime();
}

export function formatCountdown(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(h)}:${pad(m)}:${pad(s)}`;
}
