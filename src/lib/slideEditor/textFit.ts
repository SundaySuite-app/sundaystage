/**
 * Text auto-fit / reflow — Phase deep-stage-1.
 *
 * A 1:1 TypeScript port of the Rust `services::text_fit` search. Fixed-size
 * slides overflow the moment a translation runs long or a volunteer pastes a
 * wall of lyrics; this shrinks the font (and reports the line breaks) so the
 * text stays inside its box instead of spilling off-screen.
 *
 * The point of sharing the algorithm is the "preview == output" promise: the
 * editor preview and the separate live-output process must agree on size and
 * breaks. They do because both run THIS search (the Rust one for the output
 * process, this port for the React preview) over the same parameters; only the
 * injected measurement closure differs (real canvas glyph metrics in the
 * browser, an average-advance model in the headless output).
 *
 * Real glyph metrics are a *runtime* concern — see {@link canvasMeasurer}, which
 * is the only browser-touching part. The pure {@link fitText} search is what the
 * unit tests pin (with deterministic mock measurers), mirroring the Rust tests.
 */

/** The measured footprint of a laid-out string at a given size. */
export interface LaidOut {
  /** The text split into the visual lines it occupies at this size. */
  lines: string[];
  /** Total block height in the same units as the box (lines × line-height). */
  height: number;
}

/**
 * Lay `text` out into `maxWidth` at font `size` and report wrapped lines +
 * height. MUST be pure for a given input so the search is deterministic. Hard
 * `\n` are always honored as breaks.
 */
export type Measure = (text: string, size: number, maxWidth: number) => LaidOut;

/** The box (and optional line cap) the text must fit within. */
export interface FitBox {
  width: number;
  height: number;
  /** Hard cap on visual lines; `null`/`undefined` = bounded only by height. */
  maxLines?: number | null;
}

/** The size-search parameters. */
export interface FitParams {
  base: number;
  min: number;
  step: number;
}

export const DEFAULT_FIT_PARAMS: FitParams = { base: 64, min: 18, step: 2 };

/** The result of fitting: chosen size, line breaks, and whether it clamped. */
export interface FitResult {
  size: number;
  lines: string[];
  /** True when bottomed out at `min` and the text still overflows. */
  clamped: boolean;
}

const EPS = 1e-3;

function sane(v: number, fallback: number): number {
  return Number.isFinite(v) && v > 0 ? v : fallback;
}

function fits(laid: LaidOut, bx: FitBox): boolean {
  if (laid.height > bx.height + EPS) return false;
  if (bx.maxLines != null && laid.lines.length > bx.maxLines) return false;
  return true;
}

/**
 * Compute the largest font size + line breaks that fit `text` in `bx`, measured
 * by `measure`, searching from `params.base` down to `params.min`.
 *
 * Behaviour is identical to the Rust `fit_text` (locked by both test suites):
 * fitting text keeps the base; just-over shrinks by one step; never-fitting
 * clamps to min with `clamped: true` (still returning wrapped lines); max-lines
 * honored at every step; empty text keeps base with no lines; degenerate params
 * terminate safely.
 */
export function fitText(
  text: string,
  bx: FitBox,
  params: FitParams,
  measure: Measure,
): FitResult {
  if (text.trim() === "") {
    return { size: Math.max(0, params.base), lines: [], clamped: false };
  }

  const base = sane(params.base, 1);
  const min = Math.min(sane(params.min, 1), base);
  const rawStep = sane(params.step, 1);
  const step = rawStep <= 0 ? Math.max(1, base - min) : rawStep;

  let size = base;
  let last = measure(text, size, bx.width);
  while (size > min + EPS) {
    if (fits(last, bx)) {
      return { size, lines: last.lines, clamped: false };
    }
    size = Math.max(min, size - step);
    last = measure(text, size, bx.width);
  }

  const clamped = !fits(last, bx);
  return { size: min, lines: last.lines, clamped };
}

/**
 * A deterministic average-advance measurer: each glyph ≈ `size * charW` wide,
 * each line `size * lineH` tall, greedy word-wrap, hard `\n` always break. This
 * is the headless model the Rust output process uses; the editor can use it as a
 * cheap fallback before a real canvas measurer is available. Pure → testable.
 */
export function averageAdvanceMeasurer(charW = 0.52, lineH = 1.2): Measure {
  return (text, size, maxWidth) => {
    const glyph = Math.max(size * charW, 1e-3);
    const maxChars = Math.max(1, Math.floor(maxWidth / glyph));
    return wrap(text, maxChars, size, lineH);
  };
}

/** Greedy word-wrap shared by the mock/average measurers (pure). */
function wrap(
  text: string,
  maxChars: number,
  size: number,
  lineH: number,
): LaidOut {
  const lines: string[] = [];
  for (const hard of text.split("\n")) {
    if (hard === "") {
      lines.push("");
      continue;
    }
    let cur = "";
    for (const word of hard.split(" ")) {
      const candidate = cur === "" ? word : `${cur} ${word}`;
      if ([...candidate].length <= maxChars || cur === "") {
        cur = candidate;
      } else {
        lines.push(cur);
        cur = word;
      }
    }
    lines.push(cur);
  }
  return { lines, height: lines.length * size * lineH };
}

/**
 * RUNTIME measurer backed by real glyph metrics via a 2D canvas. This is the
 * only browser-touching code in the module — isolated here on purpose so the
 * {@link fitText} search stays pure and unit-testable. It feeds the SAME search
 * the output process runs, so the editor preview matches the live output to
 * within font-metric accuracy.
 *
 * Returns `null` when no canvas context is available (SSR / tests), so callers
 * fall back to {@link averageAdvanceMeasurer}.
 */
export function canvasMeasurer(font: string, lineHeight = 1.2): Measure | null {
  if (typeof document === "undefined") return null;
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  return (text, size, maxWidth) => {
    ctx.font = `${size}px ${font}`;
    const lines: string[] = [];
    for (const hard of text.split("\n")) {
      if (hard === "") {
        lines.push("");
        continue;
      }
      let cur = "";
      for (const word of hard.split(" ")) {
        const candidate = cur === "" ? word : `${cur} ${word}`;
        if (ctx.measureText(candidate).width <= maxWidth || cur === "") {
          cur = candidate;
        } else {
          lines.push(cur);
          cur = word;
        }
      }
      lines.push(cur);
    }
    return { lines, height: lines.length * size * lineHeight };
  };
}
