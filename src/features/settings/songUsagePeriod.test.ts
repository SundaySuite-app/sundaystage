import { describe, it, expect } from "vitest";

import {
  periodBounds,
  presetPeriod,
  toIsoDate,
  type Period,
} from "./songUsagePeriod";

describe("songUsagePeriod", () => {
  const today = new Date(2026, 7, 30); // 30. august 2026, lokal tid

  it("«hittil i år» går fra 1. januar til i dag", () => {
    expect(presetPeriod("thisYear", today)).toEqual({
      from: "2026-01-01",
      to: "2026-08-30",
    });
  });

  it("«i fjor» er hele forrige kalenderår — det årsrapporten dekker", () => {
    expect(presetPeriod("lastYear", today)).toEqual({
      from: "2025-01-01",
      to: "2025-12-31",
    });
  });

  it("«siste kvartal» er 90 dager bakover, inkludert i dag", () => {
    const p = presetPeriod("lastQuarter", today);
    expect(p.to).toBe("2026-08-30");
    expect(p.from).toBe("2026-06-02");
  });

  it("bunnen er lokal midnatt og toppen er siste millisekund av sluttdagen", () => {
    const { fromMs, toMs } = periodBounds({
      from: "2026-01-01",
      to: "2026-03-31",
    });
    expect(new Date(fromMs).getHours()).toBe(0);
    expect(toIsoDate(new Date(toMs))).toBe("2026-03-31");
    // En kveldsgudstjeneste på siste dag i perioden hører med.
    const evening = new Date(2026, 2, 31, 19, 0).getTime();
    expect(evening).toBeLessThanOrEqual(toMs);
    expect(evening).toBeGreaterThanOrEqual(fromMs);
  });

  it("én enkelt dag er en gyldig periode", () => {
    const { fromMs, toMs } = periodBounds({
      from: "2026-08-30",
      to: "2026-08-30",
    });
    expect(toMs - fromMs).toBe(24 * 60 * 60 * 1000 - 1);
  });

  it.each<[string, Period]>([
    ["tom", { from: "", to: "2026-08-30" }],
    ["ugyldig format", { from: "30.08.2026", to: "2026-08-30" }],
    ["dato som ikke finnes", { from: "2026-02-31", to: "2026-08-30" }],
    ["baklengs", { from: "2026-08-30", to: "2026-01-01" }],
  ])("%s periode gir et tomt vindu, ikke et vilt spørsmål", (_label, p) => {
    expect(periodBounds(p)).toEqual({ fromMs: 0, toMs: 0 });
  });
});
