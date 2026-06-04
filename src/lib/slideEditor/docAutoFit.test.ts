import { describe, expect, it } from "vitest";

import type { SlideBlock } from "@/lib/bindings";

import { autoFitSize, autoFitTextBlockStyle, LINE_HEIGHT } from "./doc";
import { averageAdvanceMeasurer, fitText, type FitParams } from "./textFit";

type TextBlock = Extract<SlideBlock, { type: "text" }>;

function textBlock(text: string, w = 0.84, h = 0.5): TextBlock {
  return {
    type: "text",
    id: "b1",
    text,
    rect: { x: 0.08, y: 0.25, w, h },
    align: "center",
    valign: "middle",
    style: {
      family: null,
      size: 64,
      weight: 700,
      color: "#fff",
      italic: false,
      shadow: null,
    },
  };
}

// Deterministic measurer (no canvas) so the test is browser-free.
const measure = averageAdvanceMeasurer(0.52, LINE_HEIGHT);

describe("editor auto-fit (doc.autoFitSize)", () => {
  it("keeps the base size for short text", () => {
    expect(autoFitSize(textBlock("Hi"), measure)).toBe(64);
  });

  it("shrinks long text below the base", () => {
    const long = "the quick brown fox jumps over the lazy dog ".repeat(8);
    const size = autoFitSize(textBlock(long), measure);
    expect(size).toBeLessThan(64);
    expect(size).toBeGreaterThanOrEqual(22); // clamped to the editor min
  });

  it("delegates to the SHARED fitText with the block's box", () => {
    // Assert the helper produces exactly what fitText would for the same box —
    // i.e. the editor uses the same algorithm the live output uses, the
    // executable form of the preview==output promise.
    const block = textBlock("Amazing grace how sweet the sound", 0.84, 0.5);
    const params: FitParams = { base: 64, min: 22, step: 2 };
    const box = {
      width: 0.84 * 1080 * (16 / 9),
      height: 0.5 * 1080,
      maxLines: null,
    };
    const expected = fitText(block.text, box, params, measure).size;
    expect(autoFitSize(block, measure)).toBe(expected);
  });
});

describe("autoFitTextBlockStyle", () => {
  it("emits the fitted size scaled to the canvas height", () => {
    const block = textBlock("Hi");
    const canvasH = 540; // half the 1080 stage
    const style = autoFitTextBlockStyle(block, canvasH, measure);
    // 64px @1080 scaled to 540 = 32px.
    expect(style.fontSize).toBe("32px");
  });

  it("shrinks the rendered size for overflowing text", () => {
    const long = "the quick brown fox jumps over the lazy dog ".repeat(8);
    const short = autoFitTextBlockStyle(textBlock("Hi"), 1080, measure)
      .fontSize as string;
    const big = autoFitTextBlockStyle(textBlock(long), 1080, measure)
      .fontSize as string;
    expect(parseFloat(big)).toBeLessThan(parseFloat(short));
  });
});
