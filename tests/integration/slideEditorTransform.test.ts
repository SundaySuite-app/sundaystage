// Multi-block layout algebra (Phase 3.2 deepened): reorder/restack, duplicate,
// nudge, align, distribute, plus resize-snapping and the doc-scoped layout
// command. All pure and run inside the drag budget, so they get a thorough
// unit suite here. The canvas UI that *calls* this algebra is GUI-UNVERIFIED
// (see docs/SMOKE-TEST.md).
import { describe, it, expect } from "vitest";

import { docWithText, newTextBlock, addBlock } from "@/lib/slideEditor/doc";
import {
  blocksByIds,
  moveBlockTo,
  restackBlock,
  duplicateBlocks,
  nudgeBlocks,
  alignBlocks,
  distributeBlocks,
  DUPLICATE_OFFSET,
} from "@/lib/slideEditor/transform";
import {
  snapResize,
  MIN_RESIZE,
  type ResizeHandle,
} from "@/lib/slideEditor/snap";
import { layoutCommand } from "@/lib/slideEditor/history";
import type { SlideBlock, SlideDoc } from "@/lib/bindings";

// A deterministic id minter so duplicate tests are stable.
function counter(prefix = "dup"): () => string {
  let n = 0;
  return () => `${prefix}_${(n += 1)}`;
}

/** Three text blocks with explicit rects, in document order a→b→c. */
function tripleDoc(): {
  doc: SlideDoc;
  a: string;
  b: string;
  c: string;
} {
  const ba = newTextBlock("a", { x: 0.0, y: 0.0, w: 0.2, h: 0.1 });
  const bb = newTextBlock("b", { x: 0.3, y: 0.4, w: 0.2, h: 0.1 });
  const bc = newTextBlock("c", { x: 0.7, y: 0.8, w: 0.2, h: 0.1 });
  let doc = docWithText("seed");
  doc = { ...doc, blocks: [] }; // drop the seed block, keep background
  doc = addBlock(addBlock(addBlock(doc, ba), bb), bc);
  return { doc, a: ba.id, b: bb.id, c: bc.id };
}

const rectOf = (doc: SlideDoc, id: string) =>
  doc.blocks.find((x) => x.id === id)!.rect;

// ── selection ────────────────────────────────────────────────────────────────

describe("transform/blocksByIds", () => {
  it("returns blocks in document order, not id order", () => {
    const { doc, a, c } = tripleDoc();
    // Ask in reverse — result must still be document order [a, c].
    const got = blocksByIds(doc, [c, a]).map((x) => x.id);
    expect(got).toEqual([a, c]);
  });

  it("accepts a Set and ignores unknown ids", () => {
    const { doc, b } = tripleDoc();
    expect(blocksByIds(doc, new Set([b, "ghost"])).map((x) => x.id)).toEqual([
      b,
    ]);
  });
});

// ── reorder / restack ──────────────────────────────────────────────────────────

describe("transform/moveBlockTo", () => {
  it("moves a block to an absolute index, clamping out-of-range", () => {
    const { doc, a, b, c } = tripleDoc();
    expect(moveBlockTo(doc, a, 2).blocks.map((x) => x.id)).toEqual([b, c, a]);
    expect(moveBlockTo(doc, c, 0).blocks.map((x) => x.id)).toEqual([c, a, b]);
    expect(moveBlockTo(doc, a, 99).blocks.map((x) => x.id)).toEqual([b, c, a]);
  });

  it("is a no-op (returns same doc) for an unknown id", () => {
    const { doc } = tripleDoc();
    expect(moveBlockTo(doc, "ghost", 0)).toBe(doc);
  });

  it("does not mutate the source array", () => {
    const { doc, a } = tripleDoc();
    const before = doc.blocks.map((x) => x.id);
    moveBlockTo(doc, a, 2);
    expect(doc.blocks.map((x) => x.id)).toEqual(before);
  });
});

describe("transform/restackBlock", () => {
  it("front/back jump to the ends", () => {
    const { doc, a, c } = tripleDoc();
    expect(restackBlock(doc, a, "front").blocks.at(-1)!.id).toBe(a);
    expect(restackBlock(doc, c, "back").blocks[0].id).toBe(c);
  });

  it("forward/backward nudge by one slot", () => {
    const { doc, b } = tripleDoc();
    expect(restackBlock(doc, b, "forward").blocks.map((x) => x.id)[2]).toBe(b);
    expect(restackBlock(doc, b, "backward").blocks.map((x) => x.id)[0]).toBe(b);
  });

  it("returns the SAME doc reference when already at the relevant end", () => {
    const { doc, a, c } = tripleDoc();
    expect(restackBlock(doc, a, "back")).toBe(doc); // already first
    expect(restackBlock(doc, c, "front")).toBe(doc); // already last
    expect(restackBlock(doc, a, "backward")).toBe(doc); // can't go lower
    expect(restackBlock(doc, c, "forward")).toBe(doc); // can't go higher
  });
});

// ── duplicate ──────────────────────────────────────────────────────────────────

describe("transform/duplicateBlocks", () => {
  it("appends nudged clones with fresh ids and reports them", () => {
    const { doc, a } = tripleDoc();
    const { doc: next, newIds } = duplicateBlocks(doc, [a], counter());
    expect(newIds).toEqual(["dup_1"]);
    expect(next.blocks).toHaveLength(4);
    const clone = next.blocks.at(-1)!;
    expect(clone.id).toBe("dup_1");
    expect(clone.rect.x).toBeCloseTo(0.0 + DUPLICATE_OFFSET, 6);
    expect(clone.rect.y).toBeCloseTo(0.0 + DUPLICATE_OFFSET, 6);
  });

  it("preserves block content (text) on the clone", () => {
    const { doc, b } = tripleDoc();
    const { doc: next, newIds } = duplicateBlocks(doc, [b], counter());
    const clone = next.blocks.find((x) => x.id === newIds[0]) as Extract<
      SlideBlock,
      { type: "text" }
    >;
    expect(clone.text).toBe("b");
  });

  it("clamps a clone that would run off the frame back on-screen", () => {
    const { doc, c } = tripleDoc(); // c at x=0.7,w=0.2 → x+offset+w stays < 1
    const big = newTextBlock("edge", { x: 0.95, y: 0.95, w: 0.2, h: 0.2 });
    const withBig = addBlock(doc, big);
    const { doc: next, newIds } = duplicateBlocks(withBig, [big.id], counter());
    const clone = next.blocks.find((x) => x.id === newIds[0])!;
    expect(clone.rect.x + clone.rect.w).toBeLessThanOrEqual(1.0000001);
    expect(clone.rect.y + clone.rect.h).toBeLessThanOrEqual(1.0000001);
    expect(c).toBeTruthy();
  });

  it("duplicates several blocks in one call, document order preserved", () => {
    const { doc, a, c } = tripleDoc();
    const { newIds } = duplicateBlocks(doc, [c, a], counter());
    // a comes first in the document → cloned first.
    expect(newIds).toEqual(["dup_1", "dup_2"]);
  });
});

// ── nudge ──────────────────────────────────────────────────────────────────────

describe("transform/nudgeBlocks", () => {
  it("shifts only the selected blocks, clamping on-frame", () => {
    const { doc, a, b } = tripleDoc();
    const next = nudgeBlocks(doc, [b], 0.1, -0.1);
    expect(rectOf(next, b).x).toBeCloseTo(0.4, 6);
    expect(rectOf(next, b).y).toBeCloseTo(0.3, 6);
    expect(rectOf(next, a)).toEqual(rectOf(doc, a)); // untouched
  });

  it("clamps a nudge past the top edge to 0", () => {
    const { doc, a } = tripleDoc(); // a at y=0
    expect(rectOf(nudgeBlocks(doc, [a], 0, -0.5), a).y).toBe(0);
  });

  it("moves a multi-selection rigidly: the group stops at the frame edge without deforming", () => {
    // a is at x=0 (already on the left wall), b at x=0.3. Nudging the *group*
    // left by 0.2 must move nothing — a can't go past 0, and the group moves as
    // one unit, so the a→b gap (0.3) is preserved. The bug clamped each block
    // independently: a stayed at 0 while b slid to 0.1, shrinking the gap.
    const { doc, a, b } = tripleDoc();
    const next = nudgeBlocks(doc, [a, b], -0.2, 0);
    expect(rectOf(next, a).x).toBeCloseTo(0.0, 6);
    expect(rectOf(next, b).x).toBeCloseTo(0.3, 6); // gap intact, not 0.1
  });
});

// ── align ──────────────────────────────────────────────────────────────────────

describe("transform/alignBlocks", () => {
  it("aligns multiple blocks to the selection's left edge", () => {
    const { doc, a, b, c } = tripleDoc(); // leftmost edge is a.x = 0
    const next = alignBlocks(doc, [a, b, c], "left");
    expect(rectOf(next, a).x).toBeCloseTo(0, 6);
    expect(rectOf(next, b).x).toBeCloseTo(0, 6);
    expect(rectOf(next, c).x).toBeCloseTo(0, 6);
  });

  it("aligns multiple blocks to the selection's right edge", () => {
    const { doc, a, b, c } = tripleDoc(); // rightmost edge is c.x+w = 0.9
    const next = alignBlocks(doc, [a, b, c], "right");
    for (const id of [a, b, c]) {
      expect(rectOf(next, id).x + rectOf(next, id).w).toBeCloseTo(0.9, 6);
    }
  });

  it("horizontal-centres a selection on its bounding box centre", () => {
    const { doc, a, b, c } = tripleDoc(); // bbox x in [0, 0.9] → centre 0.45
    const next = alignBlocks(doc, [a, b, c], "hcenter");
    for (const id of [a, b, c]) {
      const r = rectOf(next, id);
      expect(r.x + r.w / 2).toBeCloseTo(0.45, 6);
    }
  });

  it("for a SINGLE block aligns against the frame, not itself", () => {
    const { doc, b } = tripleDoc(); // b w=0.2 → centre on slide → x=0.4
    expect(rectOf(alignBlocks(doc, [b], "hcenter"), b).x).toBeCloseTo(0.4, 6);
    expect(rectOf(alignBlocks(doc, [b], "left"), b).x).toBeCloseTo(0, 6);
    expect(
      rectOf(alignBlocks(doc, [b], "right"), b).x +
        rectOf(alignBlocks(doc, [b], "right"), b).w,
    ).toBeCloseTo(1, 6);
  });

  it("vcenter / top / bottom move y, never w/h", () => {
    const { doc, a, b, c } = tripleDoc(); // bbox y in [0, 0.9]
    const next = alignBlocks(doc, [a, b, c], "vcenter");
    for (const id of [a, b, c]) {
      expect(rectOf(next, id).h).toBeCloseTo(0.1, 6);
      expect(rectOf(next, id).y + rectOf(next, id).h / 2).toBeCloseTo(0.45, 6);
    }
  });

  it("is a no-op for an empty selection", () => {
    const { doc } = tripleDoc();
    expect(alignBlocks(doc, [], "left")).toBe(doc);
  });
});

// ── distribute ──────────────────────────────────────────────────────────────────

describe("transform/distributeBlocks", () => {
  it("equalises horizontal gaps between three blocks, extremes fixed", () => {
    // a:[0,0.2] b:[0.3,0.5] c:[0.7,0.9]. span=[0,0.9], sizes=0.6, gap=0.15.
    const { doc, a, b, c } = tripleDoc();
    const next = distributeBlocks(doc, [a, b, c], "horizontal");
    expect(rectOf(next, a).x).toBeCloseTo(0, 6); // extreme unchanged
    expect(rectOf(next, c).x).toBeCloseTo(0.7, 6); // extreme unchanged
    // b starts after a (0.2) + gap (0.15) = 0.35.
    expect(rectOf(next, b).x).toBeCloseTo(0.35, 6);
    // Gaps are equal: a→b gap == b→c gap.
    const gapAB = rectOf(next, b).x - (rectOf(next, a).x + rectOf(next, a).w);
    const gapBC = rectOf(next, c).x - (rectOf(next, b).x + rectOf(next, b).w);
    expect(gapAB).toBeCloseTo(gapBC, 6);
  });

  it("distributes vertically along the y axis", () => {
    const { doc, a, b, c } = tripleDoc(); // y: a=0,b=0.4,c=0.8, h=0.1 each
    const next = distributeBlocks(doc, [a, b, c], "vertical");
    const gapAB = rectOf(next, b).y - (rectOf(next, a).y + rectOf(next, a).h);
    const gapBC = rectOf(next, c).y - (rectOf(next, b).y + rectOf(next, b).h);
    expect(gapAB).toBeCloseTo(gapBC, 6);
  });

  it("sorts by visual position so id order does not matter", () => {
    const { doc, a, b, c } = tripleDoc();
    const fwd = distributeBlocks(doc, [a, b, c], "horizontal");
    const rev = distributeBlocks(doc, [c, b, a], "horizontal");
    expect(rectOf(rev, b).x).toBeCloseTo(rectOf(fwd, b).x, 6);
  });

  it("is a no-op with fewer than three blocks", () => {
    const { doc, a, b } = tripleDoc();
    expect(distributeBlocks(doc, [a, b], "horizontal")).toBe(doc);
    expect(distributeBlocks(doc, [a], "horizontal")).toBe(doc);
  });
});

// ── resize snapping ──────────────────────────────────────────────────────────────

describe("snap/snapResize", () => {
  it("snaps the dragged SE corner to the frame edge, anchor (nw) fixed", () => {
    // Right edge near 1.0 (x=0.6,w=0.395 → right=0.995, within threshold) → 1.
    const r = { x: 0.6, y: 0.6, w: 0.395, h: 0.35 };
    const res = snapResize(r, "se", []);
    expect(res.rect.x).toBeCloseTo(0.6, 6); // anchor x unchanged
    expect(res.rect.y).toBeCloseTo(0.6, 6); // anchor y unchanged
    expect(res.rect.x + res.rect.w).toBeCloseTo(1, 6);
    expect(res.guidesX).toContain(1);
  });

  it("snaps the dragged NW corner, holding the SE anchor fixed", () => {
    // x near 0 (0.008) and y near 0.5 — both snap; bottom/right stay put.
    const r = { x: 0.008, y: 0.495, w: 0.4, h: 0.4 };
    const res = snapResize(r, "nw", []);
    const right = r.x + r.w; // 0.408
    const bottom = r.y + r.h; // 0.895
    expect(res.rect.x).toBeCloseTo(0, 6);
    expect(res.rect.y).toBeCloseTo(0.5, 6);
    expect(res.rect.x + res.rect.w).toBeCloseTo(right, 6); // anchor fixed
    expect(res.rect.y + res.rect.h).toBeCloseTo(bottom, 6);
  });

  it("never inverts: a dragged edge crossing the anchor floors at MIN_RESIZE", () => {
    // Drag the left handle (nw) way past the right anchor.
    const r = { x: 0.8, y: 0.4, w: 0.1, h: 0.2 };
    const res = snapResize(r, "nw", []);
    expect(res.rect.w).toBeGreaterThanOrEqual(MIN_RESIZE - 1e-9);
    expect(res.rect.h).toBeGreaterThanOrEqual(MIN_RESIZE - 1e-9);
  });

  it("leaves a rect unchanged when no edge is within threshold", () => {
    const r = { x: 0.31, y: 0.33, w: 0.22, h: 0.21 };
    const res = snapResize(r, "se", []);
    expect(res.rect.x).toBeCloseTo(r.x, 6);
    expect(res.rect.y).toBeCloseTo(r.y, 6);
    expect(res.rect.w).toBeCloseTo(r.w, 6);
    expect(res.rect.h).toBeCloseTo(r.h, 6);
    expect(res.guidesX).toEqual([]);
    expect(res.guidesY).toEqual([]);
  });

  it("snaps a resize edge onto a sibling's edge", () => {
    const sib = { x: 0.5, y: 0, w: 0.3, h: 0.2 };
    // Resize NE: top + right are dragged. Right edge near sib left 0.5.
    const r = { x: 0.2, y: 0.6, w: 0.305, h: 0.2 };
    const res = snapResize(r, "ne", [sib]);
    expect(res.rect.x).toBeCloseTo(0.2, 6); // sw anchor x fixed
    expect(res.rect.x + res.rect.w).toBeCloseTo(0.5, 6);
    expect(res.guidesX).toContain(0.5);
  });

  it("covers all four handles without inverting", () => {
    const handles: ResizeHandle[] = ["nw", "ne", "sw", "se"];
    for (const h of handles) {
      const res = snapResize({ x: 0.4, y: 0.4, w: 0.2, h: 0.2 }, h, []);
      expect(res.rect.w).toBeGreaterThan(0);
      expect(res.rect.h).toBeGreaterThan(0);
    }
  });
});

// ── layoutCommand (doc-scoped undo) ──────────────────────────────────────────────

describe("history/layoutCommand", () => {
  it("apply yields after, invert restores before (exact references)", () => {
    const { doc, a, b, c } = tripleDoc();
    const after = distributeBlocks(doc, [a, b, c], "horizontal");
    const cmd = layoutCommand("Fordel vannrett", doc, after);
    expect(cmd.apply(doc)).toBe(after);
    expect(cmd.invert(after)).toBe(doc);
    expect(cmd.label).toBe("Fordel vannrett");
  });

  it("round-trips an align transform through the command", () => {
    const { doc, a, b, c } = tripleDoc();
    const after = alignBlocks(doc, [a, b, c], "left");
    const cmd = layoutCommand("Juster venstre", doc, after);
    expect(cmd.invert(cmd.apply(doc))).toBe(doc);
  });
});
