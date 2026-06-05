/**
 * Multi-block layout algebra — Phase 3.2 (deepened).
 *
 * Pure, framework-free functions that move/duplicate/restack/align/distribute
 * blocks within a `SlideDoc`. They sit *under* the command layer in
 * `history.ts`: a UI action computes a new doc here, then wraps the
 * before/after in a command so it is undoable. Keeping the geometry here (and
 * out of the command factories) means it is exhaustively unit-testable without
 * a React tree or the history reducer.
 *
 * Every function is referentially transparent: inputs are never mutated and a
 * fresh `SlideDoc`/array is returned, so React state and the undo stack can
 * hold the references safely. All rect coordinates are normalized 0–1
 * fractions of the frame (see `doc.ts`).
 */

import type { SlideBlock, SlideDoc, SlideRect } from "@/lib/bindings";

import { clampRect } from "./doc";

/** Every block carries a rect + id regardless of its `type`. */
type BlockRect = Pick<SlideBlock, "id"> & { rect: SlideRect };

function rectOf(b: SlideBlock): SlideRect {
  return b.rect;
}

// ── Selection helpers ────────────────────────────────────────────────────────

/** Resolve ids → blocks, preserving the *document* order (not the id order). */
export function blocksByIds(
  doc: SlideDoc,
  ids: ReadonlySet<string> | string[],
): SlideBlock[] {
  const set = ids instanceof Set ? ids : new Set(ids);
  return doc.blocks.filter((b) => set.has(b.id));
}

// ── Reorder / restack ────────────────────────────────────────────────────────

/**
 * Move the block with `id` to absolute index `to` (clamped into range). Indices
 * are measured in the *current* array, so dragging a block down past its own
 * slot behaves like the platform file managers users already know.
 */
export function moveBlockTo(doc: SlideDoc, id: string, to: number): SlideDoc {
  const from = doc.blocks.findIndex((b) => b.id === id);
  if (from === -1) return doc;
  const blocks = [...doc.blocks];
  const [moved] = blocks.splice(from, 1);
  const dest = Math.min(Math.max(0, to), blocks.length);
  blocks.splice(dest, 0, moved);
  return { ...doc, blocks };
}

/** Stacking deltas. "front"/"back" jump to an end; ±1 nudge by one slot. */
export type RestackTo = "front" | "back" | "forward" | "backward";

/**
 * Restack a block in the z-order (the array order *is* the paint order: last
 * block is on top). A no-op (already at the relevant end) returns the same doc
 * reference so callers can skip pushing an empty undo step.
 */
export function restackBlock(
  doc: SlideDoc,
  id: string,
  to: RestackTo,
): SlideDoc {
  const from = doc.blocks.findIndex((b) => b.id === id);
  if (from === -1) return doc;
  const last = doc.blocks.length - 1;
  let dest: number;
  switch (to) {
    case "front":
      dest = last;
      break;
    case "back":
      dest = 0;
      break;
    case "forward":
      dest = Math.min(last, from + 1);
      break;
    case "backward":
      dest = Math.max(0, from - 1);
      break;
  }
  if (dest === from) return doc;
  return moveBlockTo(doc, id, dest);
}

// ── Duplicate ─────────────────────────────────────────────────────────────────

/** Pixels (normalized) a duplicate is nudged so it doesn't hide its original. */
export const DUPLICATE_OFFSET = 0.02;

/**
 * Clone the given blocks, mint fresh ids via `mintId`, nudge each clone down-
 * right (clamped on-frame) and append them after the originals. Returns the
 * new doc *and* the ids of the clones so the caller can re-select them — the
 * usual "duplicate then drag the copy" flow.
 */
export function duplicateBlocks(
  doc: SlideDoc,
  ids: ReadonlySet<string> | string[],
  mintId: () => string,
  offset = DUPLICATE_OFFSET,
): { doc: SlideDoc; newIds: string[] } {
  const originals = blocksByIds(doc, ids);
  const newIds: string[] = [];
  const clones = originals.map((b) => {
    const id = mintId();
    newIds.push(id);
    const rect = clampRect({
      ...b.rect,
      x: b.rect.x + offset,
      y: b.rect.y + offset,
    });
    return { ...b, id, rect } as SlideBlock;
  });
  return { doc: { ...doc, blocks: [...doc.blocks, ...clones] }, newIds };
}

// ── Bulk geometry: nudge ──────────────────────────────────────────────────────

/**
 * Shift a set of blocks by (dx, dy). The whole group moves *rigidly* — used for
 * arrow-key nudging of a multi-selection. The delta is clamped once against the
 * selection's bounding box so the group stops as a unit at the frame edge
 * instead of each block clamping independently (which would shear the layout).
 */
export function nudgeBlocks(
  doc: SlideDoc,
  ids: ReadonlySet<string> | string[],
  dx: number,
  dy: number,
): SlideDoc {
  const set = ids instanceof Set ? ids : new Set(ids);
  const sel = doc.blocks.filter((b) => set.has(b.id));
  if (sel.length === 0) return doc;

  // Bounding box of the selection, so the whole group can move by at most the
  // distance that keeps every block on-frame — preserving relative spacing.
  const { minX, maxX, minY, maxY } = selectionBounds(
    sel.map((b) => ({ id: b.id, rect: rectOf(b) })),
  );
  // Allowed travel on each axis: not past 0 on the near edge, not past 1 on the
  // far edge. Clamp the requested delta into [lo, hi] so the rigid group stops.
  const cdx = Math.min(Math.max(dx, -minX), 1 - maxX);
  const cdy = Math.min(Math.max(dy, -minY), 1 - maxY);

  return {
    ...doc,
    blocks: doc.blocks.map((b) =>
      set.has(b.id)
        ? ({
            ...b,
            rect: { ...b.rect, x: b.rect.x + cdx, y: b.rect.y + cdy },
          } as SlideBlock)
        : b,
    ),
  };
}

// ── Align ─────────────────────────────────────────────────────────────────────

export type AlignEdge =
  | "left"
  | "hcenter"
  | "right"
  | "top"
  | "vcenter"
  | "bottom";

function selectionBounds(blocks: BlockRect[]): {
  minX: number;
  maxX: number;
  minY: number;
  maxY: number;
} {
  let minX = Infinity;
  let maxX = -Infinity;
  let minY = Infinity;
  let maxY = -Infinity;
  for (const { rect } of blocks) {
    minX = Math.min(minX, rect.x);
    maxX = Math.max(maxX, rect.x + rect.w);
    minY = Math.min(minY, rect.y);
    maxY = Math.max(maxY, rect.y + rect.h);
  }
  return { minX, maxX, minY, maxY };
}

/**
 * Align the selected blocks to a shared edge/centre. With **2+** selected the
 * reference is the selection's bounding box; with a **single** block the
 * reference is the *frame* (so "centre on slide" works on one block too).
 * Width/height are never changed — only x or y.
 */
export function alignBlocks(
  doc: SlideDoc,
  ids: ReadonlySet<string> | string[],
  edge: AlignEdge,
): SlideDoc {
  const set = ids instanceof Set ? ids : new Set(ids);
  const sel = doc.blocks.filter((b) => set.has(b.id));
  if (sel.length === 0) return doc;

  const b =
    sel.length === 1
      ? { minX: 0, maxX: 1, minY: 0, maxY: 1 }
      : selectionBounds(sel.map((x) => ({ id: x.id, rect: rectOf(x) })));

  const place = (rect: SlideRect): SlideRect => {
    switch (edge) {
      case "left":
        return { ...rect, x: b.minX };
      case "right":
        return { ...rect, x: b.maxX - rect.w };
      case "hcenter":
        return { ...rect, x: (b.minX + b.maxX) / 2 - rect.w / 2 };
      case "top":
        return { ...rect, y: b.minY };
      case "bottom":
        return { ...rect, y: b.maxY - rect.h };
      case "vcenter":
        return { ...rect, y: (b.minY + b.maxY) / 2 - rect.h / 2 };
    }
  };

  return {
    ...doc,
    blocks: doc.blocks.map((blk) =>
      set.has(blk.id)
        ? ({ ...blk, rect: clampRect(place(rectOf(blk))) } as SlideBlock)
        : blk,
    ),
  };
}

// ── Distribute ─────────────────────────────────────────────────────────────────

export type DistributeAxis = "horizontal" | "vertical";

/**
 * Even out the *gaps* between 3+ blocks along an axis. The two extreme blocks
 * stay put (they define the span); the inner blocks are repositioned so the
 * whitespace between consecutive blocks is equal — matching how design tools
 * "distribute spacing". Fewer than 3 blocks → no-op (nothing to distribute).
 */
export function distributeBlocks(
  doc: SlideDoc,
  ids: ReadonlySet<string> | string[],
  axis: DistributeAxis,
): SlideDoc {
  const set = ids instanceof Set ? ids : new Set(ids);
  const sel = doc.blocks.filter((b) => set.has(b.id));
  if (sel.length < 3) return doc;

  const horiz = axis === "horizontal";
  const start = (r: SlideRect) => (horiz ? r.x : r.y);
  const size = (r: SlideRect) => (horiz ? r.w : r.h);

  // Sort by start edge so "first/last" mean the visual extremes.
  const ordered = [...sel].sort((a, b) => start(rectOf(a)) - start(rectOf(b)));
  const totalSize = ordered.reduce((s, b) => s + size(rectOf(b)), 0);
  const spanStart = start(rectOf(ordered[0]));
  const lastRect = rectOf(ordered[ordered.length - 1]);
  const spanEnd = start(lastRect) + size(lastRect);
  // Equal gap between consecutive blocks across the whole span.
  const gap = (spanEnd - spanStart - totalSize) / (ordered.length - 1);

  const newStart = new Map<string, number>();
  let cursor = spanStart;
  for (const b of ordered) {
    newStart.set(b.id, cursor);
    cursor += size(rectOf(b)) + gap;
  }

  return {
    ...doc,
    blocks: doc.blocks.map((blk) => {
      const s = newStart.get(blk.id);
      if (s === undefined) return blk;
      const rect = horiz ? { ...blk.rect, x: s } : { ...blk.rect, y: s };
      return { ...blk, rect: clampRect(rect) } as SlideBlock;
    }),
  };
}
