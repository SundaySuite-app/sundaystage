// Slide-editor pure core (Phase 3.1/3.2). The editing algebra (immutable doc
// edits + geometry), the command/undo model (apply/invert composition), the
// snap/align math, and the theme-stamping helpers are all pure and run in the
// 16 ms drag budget — so they get a thorough unit suite here. The canvas UI
// that *calls* this algebra is GUI-UNVERIFIED (see docs/SMOKE-TEST.md).
import { describe, it, expect } from "vitest";

import {
  blankDoc,
  docWithText,
  newTextBlock,
  parseDoc,
  findBlock,
  addBlock,
  replaceBlock,
  removeBlocks,
  setBackground,
  patchTextBlock,
  patchStyle,
  clamp01,
  clampRect,
  isTextBlock,
} from "@/lib/slideEditor/doc";
import {
  addBlockCommand,
  updateBlockCommand,
  setBackgroundCommand,
  replaceDocCommand,
  compositeCommand,
  removeBlocksCommand,
} from "@/lib/slideEditor/history";
import { snapMove, buildTargets, SNAP_THRESHOLD } from "@/lib/slideEditor/snap";
import {
  defaultTokens,
  applyThemeToDoc,
  combinedText,
} from "@/lib/slideEditor/theme";
import type { SlideBlock, SlideDoc } from "@/lib/bindings";

// ── doc.ts: construction + immutable edits ───────────────────────────────────

describe("slideEditor/doc construction", () => {
  it("blankDoc has a colour background and no blocks", () => {
    const d = blankDoc();
    expect(d.blocks).toEqual([]);
    expect(d.background).toEqual({ type: "color", value: "#0b1020" });
  });

  it("docWithText seeds exactly one centered text block", () => {
    const d = docWithText("Hallelujah");
    expect(d.blocks).toHaveLength(1);
    const b = d.blocks[0];
    expect(isTextBlock(b)).toBe(true);
    if (b.type === "text") {
      expect(b.text).toBe("Hallelujah");
      expect(b.align).toBe("center");
      expect(b.valign).toBe("middle");
    }
  });

  it("newTextBlock mints unique ids and accepts a rect override", () => {
    const a = newTextBlock("a");
    const b = newTextBlock("b", { x: 0.2, w: 0.5 });
    expect(a.id).not.toBe(b.id);
    // Override merges over the defaults (y/h kept).
    expect(b.rect).toEqual({ x: 0.2, y: 0.4, w: 0.5, h: 0.2 });
  });
});

describe("slideEditor/doc parseDoc", () => {
  it("round-trips a valid serialized doc", () => {
    const d = docWithText("Grace");
    expect(parseDoc(JSON.stringify(d))).toEqual(d);
  });

  it("returns a blank doc for malformed JSON rather than throwing", () => {
    expect(parseDoc("{not json")).toEqual(blankDoc());
  });

  it("returns a blank doc when the shape is wrong (missing blocks)", () => {
    expect(parseDoc(JSON.stringify({ background: {} }))).toEqual(blankDoc());
    expect(parseDoc(JSON.stringify({ blocks: "nope" }))).toEqual(blankDoc());
    expect(parseDoc("null")).toEqual(blankDoc());
  });
});

describe("slideEditor/doc immutable edits", () => {
  const seed = (): SlideDoc => docWithText("one");

  it("addBlock appends without mutating the source", () => {
    const d = seed();
    const extra = newTextBlock("two");
    const next = addBlock(d, extra);
    expect(next.blocks).toHaveLength(2);
    expect(d.blocks).toHaveLength(1); // original untouched
    expect(next).not.toBe(d);
  });

  it("findBlock locates by id and misses gracefully", () => {
    const d = seed();
    const id = d.blocks[0].id;
    expect(findBlock(d, id)).toBe(d.blocks[0]);
    expect(findBlock(d, "nope")).toBeUndefined();
  });

  it("replaceBlock swaps the matching id only", () => {
    const d = addBlock(seed(), newTextBlock("two"));
    const target = d.blocks[1] as Extract<SlideBlock, { type: "text" }>;
    const edited = { ...target, text: "changed" };
    const next = replaceBlock(d, edited);
    expect((next.blocks[1] as { text: string }).text).toBe("changed");
    expect(next.blocks[0]).toBe(d.blocks[0]); // others identical
  });

  it("removeBlocks accepts both an array and a Set", () => {
    const d = addBlock(seed(), newTextBlock("two"));
    const id0 = d.blocks[0].id;
    expect(removeBlocks(d, [id0]).blocks).toHaveLength(1);
    expect(removeBlocks(d, new Set([id0])).blocks).toHaveLength(1);
  });

  it("setBackground replaces the background only", () => {
    const d = seed();
    const next = setBackground(d, { type: "color", value: "#fff" });
    expect(next.background).toEqual({ type: "color", value: "#fff" });
    expect(next.blocks).toBe(d.blocks);
  });

  it("patchTextBlock merges top-level fields, patchStyle merges style", () => {
    const b = newTextBlock("x");
    expect(patchTextBlock(b, { align: "left" }).align).toBe("left");
    const styled = patchStyle(b, { size: 96 });
    expect(styled.style.size).toBe(96);
    expect(styled.style.color).toBe(b.style.color); // other style kept
  });
});

// ── doc.ts: geometry ─────────────────────────────────────────────────────────

describe("slideEditor/doc geometry", () => {
  it("clamp01 keeps values within 0–1", () => {
    expect(clamp01(-0.5)).toBe(0);
    expect(clamp01(0.5)).toBe(0.5);
    expect(clamp01(2)).toBe(1);
  });

  it("clampRect keeps a rect on-frame and prevents inversion", () => {
    // Off the right/bottom edge → pulled back so x+w <= 1, y+h <= 1.
    const r = clampRect({ x: 0.9, y: 0.95, w: 0.5, h: 0.4 });
    expect(r.x + r.w).toBeLessThanOrEqual(1.0000001);
    expect(r.y + r.h).toBeLessThanOrEqual(1.0000001);
  });

  it("clampRect enforces a minimum size", () => {
    const r = clampRect({ x: 0.5, y: 0.5, w: 0, h: -1 });
    expect(r.w).toBeGreaterThanOrEqual(0.02);
    expect(r.h).toBeGreaterThanOrEqual(0.02);
  });
});

// ── history.ts: command/undo model (apply/invert round-trips) ────────────────

describe("slideEditor/history commands", () => {
  const base = (): SlideDoc => docWithText("base");

  it("addBlockCommand applies forward and inverts back to the original", () => {
    const doc = base();
    const block = newTextBlock("added");
    const cmd = addBlockCommand(block);
    const applied = cmd.apply(doc);
    expect(applied.blocks).toHaveLength(2);
    expect(cmd.invert(applied)).toEqual(doc); // perfect undo
  });

  it("updateBlockCommand swaps after, and invert restores before", () => {
    const doc = base();
    const before = doc.blocks[0] as Extract<SlideBlock, { type: "text" }>;
    const after = { ...before, text: "edited" };
    const cmd = updateBlockCommand(before, after);
    const applied = cmd.apply(doc);
    expect((applied.blocks[0] as { text: string }).text).toBe("edited");
    expect(cmd.invert(applied)).toEqual(doc);
  });

  it("setBackgroundCommand round-trips", () => {
    const doc = base();
    const before = doc.background;
    const after = { type: "color", value: "#abc" } as const;
    const cmd = setBackgroundCommand(before, after);
    const applied = cmd.apply(doc);
    expect(applied.background).toEqual(after);
    expect(cmd.invert(applied).background).toEqual(before);
  });

  it("replaceDocCommand swaps the whole doc both ways", () => {
    const before = base();
    const after = docWithText("brand new");
    const cmd = replaceDocCommand(before, after);
    expect(cmd.apply(before)).toBe(after);
    expect(cmd.invert(after)).toBe(before);
  });

  it("removeBlocksCommand restores blocks AND their original stacking order", () => {
    let doc = docWithText("first");
    const mid = newTextBlock("middle");
    const last = newTextBlock("last");
    doc = addBlock(addBlock(doc, mid), last);
    // Remove the middle block, then undo.
    const cmd = removeBlocksCommand(doc, [mid.id]);
    const applied = cmd.apply(doc);
    expect(applied.blocks.map((b) => b.id)).toEqual([
      doc.blocks[0].id,
      last.id,
    ]);
    const reverted = cmd.invert(applied);
    expect(reverted.blocks.map((b) => b.id)).toEqual([
      doc.blocks[0].id,
      mid.id,
      last.id,
    ]); // middle reinserted at its original index
  });

  it("compositeCommand applies in order and inverts in reverse", () => {
    const doc = base();
    const b1 = newTextBlock("c1");
    const b2 = newTextBlock("c2");
    const cmd = compositeCommand("multi", [
      addBlockCommand(b1),
      addBlockCommand(b2),
    ]);
    const applied = cmd.apply(doc);
    expect(applied.blocks.map((b) => b.id)).toEqual([
      doc.blocks[0].id,
      b1.id,
      b2.id,
    ]);
    expect(cmd.invert(applied)).toEqual(doc); // both undone, order-safe
  });
});

// ── snap.ts: snap/align math ─────────────────────────────────────────────────

describe("slideEditor/snap", () => {
  it("buildTargets always includes the frame edges and centre", () => {
    expect(buildTargets([], "x")).toEqual([0, 0.5, 1]);
  });

  it("buildTargets adds each sibling's three salient lines on the axis", () => {
    const sib = { x: 0.2, y: 0, w: 0.4, h: 0.2 };
    // Frame guides then sibling salient-x [left, centre, right] = [0.2,0.4,0.6].
    const targets = buildTargets([sib], "x");
    expect(targets.slice(0, 3)).toEqual([0, 0.5, 1]);
    expect(targets).toHaveLength(6);
    expect(targets[3]).toBeCloseTo(0.2, 6);
    expect(targets[4]).toBeCloseTo(0.4, 6);
    expect(targets[5]).toBeCloseTo(0.6, 6); // 0.2 + 0.4 (FP-safe)
  });

  it("snaps a near-centre block onto the 0.5 guide", () => {
    // Block centred at 0.495 (w=0.2 → x=0.395); within threshold of 0.5.
    const res = snapMove({ x: 0.395, y: 0.4, w: 0.2, h: 0.2 }, []);
    // Centre lands exactly on 0.5 → x = 0.4.
    expect(res.rect.x).toBeCloseTo(0.4, 6);
    expect(res.guidesX).toEqual([0.5]);
  });

  it("does not snap when nothing is within the threshold", () => {
    const rect = { x: 0.31, y: 0.33, w: 0.2, h: 0.1 };
    const res = snapMove(rect, []);
    expect(res.rect).toEqual(rect); // unchanged
    expect(res.guidesX).toEqual([]);
    expect(res.guidesY).toEqual([]);
  });

  it("snaps an edge to a sibling's edge and reports the guide", () => {
    const sib = { x: 0.5, y: 0, w: 0.3, h: 0.2 };
    // Moving block's left edge at 0.503 → snaps to sibling left edge 0.5.
    const res = snapMove({ x: 0.503, y: 0.6, w: 0.2, h: 0.2 }, [sib]);
    expect(res.rect.x).toBeCloseTo(0.5, 6);
    expect(res.guidesX).toContain(0.5);
  });

  it("preserves width and height while snapping (only x/y move)", () => {
    const rect = { x: 0.495, y: 0.495, w: 0.2, h: 0.3 };
    const res = snapMove(rect, []);
    expect(res.rect.w).toBe(0.2);
    expect(res.rect.h).toBe(0.3);
  });

  it("SNAP_THRESHOLD is the documented ~1% of the frame", () => {
    expect(SNAP_THRESHOLD).toBeCloseTo(0.01, 6);
  });
});

// ── theme.ts: theme stamping ─────────────────────────────────────────────────

describe("slideEditor/theme", () => {
  it("applyThemeToDoc rewrites background + text typography, keeps layout", () => {
    const doc = docWithText("styled");
    const before = doc.blocks[0] as Extract<SlideBlock, { type: "text" }>;
    const tokens = defaultTokens();
    const next = applyThemeToDoc(doc, tokens);
    expect(next.background).toEqual(tokens.background);
    const after = next.blocks[0] as Extract<SlideBlock, { type: "text" }>;
    // Typography adopts the theme...
    expect(after.style.family).toBe(tokens.font_family);
    expect(after.style.weight).toBe(tokens.heading_weight);
    expect(after.style.color).toBe(tokens.text_color);
    // ...but position/size/alignment are preserved.
    expect(after.rect).toEqual(before.rect);
    expect(after.style.size).toBe(before.style.size);
    expect(after.align).toBe(before.align);
  });

  it("combinedText joins non-empty text blocks with newlines", () => {
    let doc = docWithText("line one");
    doc = addBlock(doc, newTextBlock("line two"));
    doc = addBlock(doc, newTextBlock("   ")); // blank → dropped
    expect(combinedText(doc)).toBe("line one\nline two");
  });
});
