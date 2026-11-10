import { describe, expect, it } from "vitest";
import { formatCountdown, msUntil, nextPrayer, prayerDate } from "../src/schedule";
import type { PrayerTimes } from "../src/schedule";

const times: PrayerTimes = {
  subuh: "04:12",
  dzuhur: "11:31",
  ashar: "14:52",
  maghrib: "17:41",
  isya: "18:53",
};

describe("nextPrayer", () => {
  it("returns Subuh when now is before Subuh", () => {
    const next = nextPrayer(times, "04:15", new Date(2026, 10, 10, 3, 0, 0));
    expect(next).toEqual({ name: "Subuh", time: "04:12" });
  });

  it("returns the prayer after now between two prayers", () => {
    const next = nextPrayer(times, "04:15", new Date(2026, 10, 10, 12, 0, 0));
    expect(next).toEqual({ name: "Ashar", time: "14:52" });
  });

  it("skips to tomorrow Subuh after Isya", () => {
    const next = nextPrayer(times, "04:15", new Date(2026, 10, 10, 19, 30, 0));
    expect(next).toEqual({ name: "Subuh", time: "04:15" });
  });

  it("treats an exact prayer time as already passed", () => {
    const next = nextPrayer(times, "04:15", new Date(2026, 10, 10, 14, 52, 0));
    expect(next).toEqual({ name: "Maghrib", time: "17:41" });
  });

  it("returns null when tomorrow Subuh is unknown after Isya", () => {
    const next = nextPrayer(times, null, new Date(2026, 10, 10, 19, 30, 0));
    expect(next).toBeNull();
  });
});

describe("prayerDate", () => {
  it("returns today at the given prayer time", () => {
    const d = prayerDate(new Date(2026, 10, 10, 12, 0, 0), "14:52");
    expect(d.getFullYear()).toBe(2026);
    expect(d.getMonth()).toBe(10);
    expect(d.getDate()).toBe(10);
    expect(d.getHours()).toBe(14);
    expect(d.getMinutes()).toBe(52);
  });
});

describe("msUntil", () => {
  it("counts down to a later time today", () => {
    const now = new Date(2026, 10, 10, 14, 0, 0);
    expect(msUntil(now, "14:52")).toBe(52 * 60 * 1000);
  });

  it("rolls to tomorrow when the time already passed", () => {
    const now = new Date(2026, 10, 10, 19, 30, 0);
    expect(msUntil(now, "04:15")).toBe(8 * 3600_000 + 45 * 60_000);
  });
});

describe("formatCountdown", () => {
  it("formats hours, minutes, seconds with zero padding", () => {
    expect(formatCountdown(5_050_000)).toBe("01:24:10");
  });

  it("formats durations over 24 hours", () => {
    expect(formatCountdown(90_000_000)).toBe("25:00:00");
  });

  it("formats remaining time under one minute", () => {
    expect(formatCountdown(45_000)).toBe("00:00:45");
  });
});
