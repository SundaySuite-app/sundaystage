/**
 * A7 — the period a song-usage report covers.
 *
 * Pure date arithmetic, kept out of the card so it can be tested without a DOM.
 * Everything here is LOCAL civil time: the log stores the date a service
 * actually ran in the operator's own timezone, so a report for "January" must
 * mean January where the church is, not January in UTC.
 */

/** An inclusive period, as the two `YYYY-MM-DD` values the date inputs hold. */
export interface Period {
  from: string;
  to: string;
}

/** The quick choices offered next to the two date fields. */
export type PeriodPreset = "thisYear" | "lastYear" | "lastQuarter";

export const PERIOD_PRESETS: PeriodPreset[] = [
  "thisYear",
  "lastYear",
  "lastQuarter",
];

function iso(year: number, month: number, day: number): string {
  const mm = String(month).padStart(2, "0");
  const dd = String(day).padStart(2, "0");
  return `${year}-${mm}-${dd}`;
}

/** `YYYY-MM-DD` for a local date. */
export function toIsoDate(d: Date): string {
  return iso(d.getFullYear(), d.getMonth() + 1, d.getDate());
}

/**
 * The period a preset means, relative to `today`.
 *
 * - `thisYear` — 1 January until today. The default: a report is almost always
 *   written about the year it is being written in.
 * - `lastYear` — the whole previous calendar year, which is what the annual
 *   report submitted in January actually covers.
 * - `lastQuarter` — the last 90 days, for a church that reports quarterly.
 */
export function presetPeriod(preset: PeriodPreset, today: Date): Period {
  const to = toIsoDate(today);
  switch (preset) {
    case "thisYear":
      return { from: iso(today.getFullYear(), 1, 1), to };
    case "lastYear": {
      const y = today.getFullYear() - 1;
      return { from: iso(y, 1, 1), to: iso(y, 12, 31) };
    }
    case "lastQuarter": {
      const start = new Date(
        today.getFullYear(),
        today.getMonth(),
        today.getDate() - 89,
      );
      return { from: toIsoDate(start), to };
    }
  }
}

/**
 * The unix-ms window the period means: local midnight at the start of `from`
 * through the last millisecond of `to`.
 *
 * Inclusive on both ends. A service held at 19:00 on the last day of the period
 * belongs in the report — an exclusive upper bound would silently drop the last
 * evening of every quarter.
 *
 * A malformed or reversed period yields an empty window rather than a wild
 * query: an empty report is honest, a report over the wrong months is not.
 */
export function periodBounds(period: Period): { fromMs: number; toMs: number } {
  const from = parseLocalDate(period.from);
  const to = parseLocalDate(period.to);
  if (!from || !to) return { fromMs: 0, toMs: 0 };
  const fromMs = from.getTime();
  const toMs =
    new Date(to.getFullYear(), to.getMonth(), to.getDate() + 1).getTime() - 1;
  if (toMs < fromMs) return { fromMs: 0, toMs: 0 };
  return { fromMs, toMs };
}

/** `YYYY-MM-DD` → local midnight, or `null` if it isn't a real date. */
function parseLocalDate(value: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value.trim());
  if (!m) return null;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const date = new Date(y, mo - 1, d);
  // Reject 2026-02-31 and friends — `Date` rolls them over silently.
  if (
    date.getFullYear() !== y ||
    date.getMonth() !== mo - 1 ||
    date.getDate() !== d
  ) {
    return null;
  }
  return date;
}
