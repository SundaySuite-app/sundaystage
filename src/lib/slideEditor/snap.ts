/**
 * Snap guides — Phase 3.1.
 *
 * Pure geometry, no React, no IPC: snapping must resolve within the 16 ms
 * frame budget while dragging, so it runs synchronously in the browser on
 * every pointer move. Kept side-effect-free so it stays trivially testable.
 *
 * All coordinates are normalized (0–1) fractions of the frame. A block's
 * three salient lines per axis (near edge, center, far edge) are checked
 * against the frame's thirds-free guides (0, 0.5, 1) and every sibling's
 * salient lines; the closest pairing within `SNAP_THRESHOLD` wins and lights
 * up a guide for the canvas to draw.
 */

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Snap distance in normalized units (~1% of the frame ≈ a few px at 1080p). */
export const SNAP_THRESHOLD = 0.01;

export interface SnapResult {
  rect: Rect;
  /** Normalized x positions of vertical guide lines to render (0–1). */
  guidesX: number[];
  /** Normalized y positions of horizontal guide lines to render (0–1). */
  guidesY: number[];
}

function salientX(r: Rect): number[] {
  return [r.x, r.x + r.w / 2, r.x + r.w];
}
function salientY(r: Rect): number[] {
  return [r.y, r.y + r.h / 2, r.y + r.h];
}

/** Frame edges + center, then every sibling's salient lines. */
export function buildTargets(siblings: Rect[], axis: "x" | "y"): number[] {
  const out = [0, 0.5, 1];
  for (const r of siblings)
    out.push(...(axis === "x" ? salientX(r) : salientY(r)));
  return out;
}

/**
 * Find the delta that snaps one of `candidates` onto the nearest `target`
 * within `threshold`. Returns a zero delta and `null` guide when nothing is
 * close enough.
 */
function bestSnap(
  candidates: number[],
  targets: number[],
  threshold: number,
): { delta: number; guide: number | null } {
  let bestDist = threshold;
  let delta = 0;
  let guide: number | null = null;
  for (const c of candidates) {
    for (const t of targets) {
      const d = Math.abs(c - t);
      if (d < bestDist) {
        bestDist = d;
        delta = t - c;
        guide = t;
      }
    }
  }
  return { delta, guide };
}

/**
 * Snap a moving rect (the whole block shifts) against frame + sibling guides.
 * The width/height are preserved; only x/y move.
 */
export function snapMove(
  rect: Rect,
  siblings: Rect[],
  threshold = SNAP_THRESHOLD,
): SnapResult {
  const sx = bestSnap(salientX(rect), buildTargets(siblings, "x"), threshold);
  const sy = bestSnap(salientY(rect), buildTargets(siblings, "y"), threshold);
  return {
    rect: { ...rect, x: rect.x + sx.delta, y: rect.y + sy.delta },
    guidesX: sx.guide !== null ? [sx.guide] : [],
    guidesY: sy.guide !== null ? [sy.guide] : [],
  };
}

/**
 * Which corner is being dragged during a resize. The *opposite* corner is the
 * anchor and stays fixed; the dragged corner is what snaps to a guide.
 */
export type ResizeHandle = "nw" | "ne" | "sw" | "se";

/** Smallest rect a resize may collapse to (mirrors `clampRect` floor). */
export const MIN_RESIZE = 0.02;

/**
 * Snap a resizing rect by its dragged corner while holding the opposite corner
 * fixed. Only the dragged edges (one x, one y) are candidates, so the anchor
 * never drifts. Returns the resized rect (never inverted, never below
 * `MIN_RESIZE`) plus any guides that lit up.
 */
export function snapResize(
  rect: Rect,
  handle: ResizeHandle,
  siblings: Rect[],
  threshold = SNAP_THRESHOLD,
): SnapResult {
  const movesLeft = handle === "nw" || handle === "sw";
  const movesTop = handle === "nw" || handle === "ne";

  // The dragged edge on each axis (the only candidate to snap).
  const edgeX = movesLeft ? rect.x : rect.x + rect.w;
  const edgeY = movesTop ? rect.y : rect.y + rect.h;
  // The fixed anchor edge on each axis.
  const anchorX = movesLeft ? rect.x + rect.w : rect.x;
  const anchorY = movesTop ? rect.y + rect.h : rect.y;

  const sx = bestSnap([edgeX], buildTargets(siblings, "x"), threshold);
  const sy = bestSnap([edgeY], buildTargets(siblings, "y"), threshold);

  const snappedX = edgeX + sx.delta;
  const snappedY = edgeY + sy.delta;

  // Recompute x/w (and y/h) from the fixed anchor + snapped dragged edge,
  // enforcing the minimum size by clamping the dragged edge if it crossed over.
  let x: number;
  let w: number;
  if (movesLeft) {
    const left = Math.min(snappedX, anchorX - MIN_RESIZE);
    x = left;
    w = anchorX - left;
  } else {
    const right = Math.max(snappedX, anchorX + MIN_RESIZE);
    x = anchorX;
    w = right - anchorX;
  }

  let y: number;
  let h: number;
  if (movesTop) {
    const top = Math.min(snappedY, anchorY - MIN_RESIZE);
    y = top;
    h = anchorY - top;
  } else {
    const bottom = Math.max(snappedY, anchorY + MIN_RESIZE);
    y = anchorY;
    h = bottom - anchorY;
  }

  return {
    rect: { x, y, w, h },
    guidesX: sx.guide !== null ? [sx.guide] : [],
    guidesY: sy.guide !== null ? [sy.guide] : [],
  };
}
