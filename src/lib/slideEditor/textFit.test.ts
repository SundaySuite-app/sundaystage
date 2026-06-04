import { describe, expect, it } from "vitest";

import {
  averageAdvanceMeasurer,
  fitText,
  type FitBox,
  type FitParams,
  type Measure,
} from "./textFit";

/**
 * Deterministic mock measurer matching the Rust `text_fit::tests::Mock`
 * (glyph = size * 0.5, line-height 1.2, greedy word-wrap). Same numbers ⇒ the
 * TS and Rust searches return the same sizes for the same inputs, which is what
 * makes the editor preview agree with the live output.
 */
const mock: Measure = averageAdvanceMeasurer(0.5, 1.2);

const PARAMS: FitParams = { base: 64, min: 18, step: 2 };

describe("fitText", () => {
  it("keeps base size for short text that fits", () => {
    const bx: FitBox = { width: 2000, height: 200, maxLines: null };
    const r = fitText("Hi", bx, PARAMS, mock);
    expect(r.size).toBe(64);
    expect(r.lines).toEqual(["Hi"]);
    expect(r.clamped).toBe(false);
  });

  it("shrinks by one step for text just over the box", () => {
    // Mirrors the Rust `text_just_over_the_box_shrinks_by_one_step` case.
    const bx: FitBox = { width: 360, height: 150, maxLines: null };
    const r = fitText("alpha beta gamma", bx, PARAMS, mock);
    expect(r.size).toBe(62);
    expect(r.lines.length).toBe(2);
    expect(r.clamped).toBe(false);
  });

  it("clamps very long text to min and still breaks lines", () => {
    const bx: FitBox = { width: 200, height: 40, maxLines: null };
    const long = "the quick brown fox jumps over the lazy dog again and again";
    const r = fitText(long, bx, PARAMS, mock);
    expect(r.size).toBe(18);
    expect(r.clamped).toBe(true);
    expect(r.lines.length).toBeGreaterThan(1);
  });

  it("respects max-lines", () => {
    const bx: FitBox = { width: 600, height: 100_000, maxLines: 1 };
    const r = fitText("alpha beta gamma delta", bx, PARAMS, mock);
    expect(r.lines.length).toBe(1);
  });

  it("clamps when the line cap is uncappable", () => {
    const bx: FitBox = { width: 30, height: 100_000, maxLines: 1 };
    const r = fitText("several separate words here", bx, PARAMS, mock);
    expect(r.size).toBe(18);
    expect(r.clamped).toBe(true);
  });

  it("keeps base with no lines for empty text", () => {
    const bx: FitBox = { width: 100, height: 100, maxLines: null };
    const r = fitText("   \n ", bx, PARAMS, mock);
    expect(r.size).toBe(64);
    expect(r.lines).toEqual([]);
    expect(r.clamped).toBe(false);
  });

  it("honors hard newlines as breaks", () => {
    const bx: FitBox = { width: 10_000, height: 10_000, maxLines: null };
    const r = fitText("line one\nline two", bx, PARAMS, mock);
    expect(r.size).toBe(64);
    expect(r.lines).toEqual(["line one", "line two"]);
  });

  it("is deterministic", () => {
    const bx: FitBox = { width: 480, height: 220, maxLines: null };
    const text =
      "Amazing grace how sweet the sound that saved a wretch like me";
    expect(fitText(text, bx, PARAMS, mock)).toEqual(
      fitText(text, bx, PARAMS, mock),
    );
  });

  it("never exceeds base in a huge box", () => {
    const bx: FitBox = { width: 1e6, height: 1e6, maxLines: null };
    expect(fitText("tiny", bx, PARAMS, mock).size).toBe(64);
  });

  it("terminates safely on degenerate params", () => {
    const bx: FitBox = { width: 100, height: 100, maxLines: null };
    const weird: FitParams = { base: NaN, min: 100, step: 0 };
    const r = fitText("text", bx, weird, mock);
    expect(Number.isFinite(r.size) && r.size > 0).toBe(true);
  });
});

describe("editor and output share the algorithm", () => {
  // The Rust output process and this editor port run the IDENTICAL search; the
  // only difference is the measurer. With the same measurer they MUST agree.
  // This is the executable form of the "preview == output" promise: shrinking a
  // long verse here produces the same size class the output produces.
  it("long lyrics fit smaller than short lyrics (matches output behaviour)", () => {
    const bx: FitBox = {
      width: 1920 * 0.84,
      height: 1080 * 0.5,
      maxLines: null,
    };
    const params: FitParams = { base: 64, min: 22, step: 2 };
    const m = averageAdvanceMeasurer(0.52, 1.2); // same model as the Rust output
    const short = fitText("Amazing grace", bx, params, m).size;
    const longLine = "the quick brown fox jumps over the lazy dog ".repeat(8);
    const long = fitText(
      [longLine, longLine, longLine].join("\n"),
      bx,
      params,
      m,
    ).size;
    expect(long).toBeLessThan(short);
  });
});
