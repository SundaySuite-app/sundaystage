/**
 * SlideCanvas — Phase 3.1.
 *
 * The canvas abstraction layer the build plan asks for: today it renders a
 * slide with absolutely-positioned DOM nodes, but every consumer talks to it
 * through this props contract, so a future Skia/WebGL renderer can drop in
 * without touching the editor. Direct-manipulation editing (select, drag,
 * resize) with snap guides lives here; the parent owns the document + history.
 *
 * All editing math is in normalized (0–1) coordinates so it is resolution
 * independent; the only place pixels enter is converting pointer events
 * against the live bounding box.
 */

import { useCallback, useEffect, useRef } from "react";

import type { SlideBlock, SlideDoc } from "@/lib/bindings";
import {
  autoFitTextBlockStyle,
  backgroundStyle,
  blockBoxStyle,
  clampRect,
  findBlock,
  isTextBlock,
  replaceBlock,
  textBlockStyle,
} from "@/lib/slideEditor/doc";
import type { Rect, ResizeHandle } from "@/lib/slideEditor/snap";
import { snapMove, snapResize } from "@/lib/slideEditor/snap";
import {
  type Command,
  compositeCommand,
  updateBlockCommand,
} from "@/lib/slideEditor/history";
import { cn } from "@/lib/cn";

type HandleId = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";
const HANDLES: HandleId[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
const MIN_SIZE = 0.02;

/** The 4 corner handles snap (via `snapResize`); edge handles resize freely. */
const CORNER_HANDLES: ReadonlySet<HandleId> = new Set<HandleId>([
  "nw",
  "ne",
  "sw",
  "se",
]);
function isCorner(h: HandleId): h is ResizeHandle {
  return CORNER_HANDLES.has(h);
}

interface Norm {
  nx: number;
  ny: number;
}

type DragState =
  | {
      mode: "move";
      primaryId: string;
      start: Norm;
      baseDoc: SlideDoc;
      startRects: Map<string, Rect>;
      before: Map<string, SlideBlock>;
      moved: boolean;
    }
  | {
      mode: "resize";
      id: string;
      handle: HandleId;
      start: Norm;
      baseDoc: SlideDoc;
      startRect: Rect;
      before: SlideBlock;
      moved: boolean;
    };

interface Guides {
  x: number[];
  y: number[];
}

interface SlideCanvasProps {
  doc: SlideDoc;
  width: number;
  height: number;
  selectedIds?: ReadonlySet<string>;
  interactive?: boolean;
  onSelect?: (id: string | null, additive: boolean) => void;
  onPreview?: (doc: SlideDoc) => void;
  onCommit?: (cmd: Command) => void;
  /**
   * Raised when a direct-manipulation drag/resize starts (true) and ends
   * (false). The parent uses this to suppress undo/redo while a preview is in
   * flight: an undo mid-drag would invert against the transient preview doc and
   * desync the history stack from the live document.
   */
  onInteractingChange?: (interacting: boolean) => void;
}

function resizeRect(
  start: Rect,
  handle: HandleId,
  nx: number,
  ny: number,
): Rect {
  let { x, y, w, h } = start;
  const right = start.x + start.w;
  const bottom = start.y + start.h;
  if (handle.includes("w")) {
    x = Math.min(nx, right - MIN_SIZE);
    w = right - x;
  }
  if (handle.includes("e")) {
    w = Math.max(MIN_SIZE, nx - start.x);
  }
  if (handle.includes("n")) {
    y = Math.min(ny, bottom - MIN_SIZE);
    h = bottom - y;
  }
  if (handle.includes("s")) {
    h = Math.max(MIN_SIZE, ny - start.y);
  }
  return clampRect({ x, y, w, h });
}

export function SlideCanvas({
  doc,
  width,
  height,
  selectedIds,
  interactive = false,
  onSelect,
  onPreview,
  onCommit,
  onInteractingChange,
}: SlideCanvasProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const docRef = useRef(doc);
  docRef.current = doc;
  const dragRef = useRef<DragState | null>(null);
  const guidesRef = useRef<HTMLDivElement>(null);

  const selected = selectedIds ?? EMPTY;

  // Guides are drawn imperatively to avoid a React re-render on every
  // pointer move (keeps drag within the frame budget).
  const drawGuides = useCallback((g: Guides) => {
    const el = guidesRef.current;
    if (!el) return;
    const lines: string[] = [];
    for (const gx of g.x) {
      lines.push(
        `<div style="position:absolute;left:${gx * 100}%;top:0;bottom:0;width:1px;background:var(--color-accent);opacity:.9"></div>`,
      );
    }
    for (const gy of g.y) {
      lines.push(
        `<div style="position:absolute;top:${gy * 100}%;left:0;right:0;height:1px;background:var(--color-accent);opacity:.9"></div>`,
      );
    }
    el.innerHTML = lines.join("");
  }, []);

  const toNorm = useCallback((e: PointerEvent | React.PointerEvent): Norm => {
    const rect = rootRef.current!.getBoundingClientRect();
    return {
      nx: (e.clientX - rect.left) / rect.width,
      ny: (e.clientY - rect.top) / rect.height,
    };
  }, []);

  const computeMove = useCallback(
    (d: Extract<DragState, { mode: "move" }>, p: Norm) => {
      const dnx = p.nx - d.start.nx;
      const dny = p.ny - d.start.ny;
      const startPrimary = d.startRects.get(d.primaryId)!;
      const candidate = clampRect({
        ...startPrimary,
        x: startPrimary.x + dnx,
        y: startPrimary.y + dny,
      });
      const siblings = d.baseDoc.blocks
        .filter((b) => !d.startRects.has(b.id))
        .map((b) => b.rect);
      const snapped = snapMove(candidate, siblings);
      const snapDx = snapped.rect.x - candidate.x;
      const snapDy = snapped.rect.y - candidate.y;

      let next = d.baseDoc;
      for (const [id, sr] of d.startRects) {
        const block = findBlock(next, id);
        if (!block) continue;
        const nr = clampRect({
          ...sr,
          x: sr.x + dnx + snapDx,
          y: sr.y + dny + snapDy,
        });
        next = replaceBlock(next, { ...block, rect: nr });
      }
      return { doc: next, guides: { x: snapped.guidesX, y: snapped.guidesY } };
    },
    [],
  );

  const computeResize = useCallback(
    (d: Extract<DragState, { mode: "resize" }>, p: Norm) => {
      const block = findBlock(d.baseDoc, d.id);
      if (!block) return { doc: d.baseDoc, guides: { x: [], y: [] } };
      // The free-drag rect from the pointer, then (for corner handles) snap the
      // dragged corner to frame/sibling guides while holding the opposite corner
      // fixed. Edge handles (n/s/e/w) resize freely — there is no single corner
      // to snap, so they keep the raw rect and draw no guides.
      const raw = resizeRect(d.startRect, d.handle, p.nx, p.ny);
      if (isCorner(d.handle)) {
        const siblings = d.baseDoc.blocks
          .filter((b) => b.id !== d.id)
          .map((b) => b.rect);
        const snapped = snapResize(raw, d.handle, siblings);
        return {
          doc: replaceBlock(d.baseDoc, {
            ...block,
            rect: clampRect(snapped.rect),
          }),
          guides: { x: snapped.guidesX, y: snapped.guidesY },
        };
      }
      return {
        doc: replaceBlock(d.baseDoc, { ...block, rect: raw }),
        guides: { x: [], y: [] },
      };
    },
    [],
  );

  const onPointerMove = useCallback(
    (e: PointerEvent) => {
      const d = dragRef.current;
      if (!d) return;
      const p = toNorm(e);
      if (Math.abs(p.nx - d.start.nx) + Math.abs(p.ny - d.start.ny) > 0.001) {
        d.moved = true;
      }
      const { doc: next, guides } =
        d.mode === "move" ? computeMove(d, p) : computeResize(d, p);
      drawGuides(guides);
      onPreview?.(next);
    },
    [toNorm, computeMove, computeResize, drawGuides, onPreview],
  );

  // Tear down a drag: drop the window listeners, clear guides, release the
  // drag ref and signal the parent that interaction ended. Stored in a ref so
  // every handler removes the *same* function instances even though the
  // listener callbacks are recreated when their deps change.
  const teardownRef = useRef<() => void>(() => {});
  const cancelRef = useRef<() => void>(() => {});

  const onPointerUp = useCallback(
    (e: PointerEvent) => {
      const d = dragRef.current;
      teardownRef.current();
      if (!d || !d.moved) return;

      const p = toNorm(e);
      if (d.mode === "move") {
        const { doc: next } = computeMove(d, p);
        onPreview?.(next);
        const cmds: Command[] = [];
        for (const [id, before] of d.before) {
          const after = findBlock(next, id);
          if (after) cmds.push(updateBlockCommand(before, after));
        }
        if (cmds.length === 1) onCommit?.(cmds[0]);
        else if (cmds.length > 1)
          onCommit?.(compositeCommand("Flytt elementer", cmds));
      } else {
        const { doc: next } = computeResize(d, p);
        onPreview?.(next);
        const after = findBlock(next, d.id);
        if (after) onCommit?.(updateBlockCommand(d.before, after));
      }
    },
    [toNorm, computeMove, computeResize, onPreview, onCommit],
  );

  // Abandon the in-flight drag without committing: restore the pre-drag doc so
  // the live preview matches what history believes. Used on Escape, on
  // `pointercancel` (e.g. the OS steals the pointer), and on unmount.
  const cancelDrag = useCallback(() => {
    const d = dragRef.current;
    teardownRef.current();
    if (d) onPreview?.(d.baseDoc);
  }, [onPreview]);
  cancelRef.current = cancelDrag;

  // Escape during a drag cancels it; capture phase so we win before the
  // editor's window keydown (which would otherwise undo against the preview).
  const onKeyDownDuringDrag = useCallback((e: KeyboardEvent) => {
    if (!dragRef.current) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      cancelRef.current();
    }
  }, []);

  const teardown = useCallback(() => {
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
    window.removeEventListener("pointercancel", cancelDrag);
    window.removeEventListener("keydown", onKeyDownDuringDrag, true);
    drawGuides({ x: [], y: [] });
    const wasDragging = dragRef.current !== null;
    dragRef.current = null;
    if (wasDragging) onInteractingChange?.(false);
  }, [
    onPointerMove,
    onPointerUp,
    cancelDrag,
    onKeyDownDuringDrag,
    drawGuides,
    onInteractingChange,
  ]);
  teardownRef.current = teardown;

  const beginDrag = useCallback(
    (state: DragState) => {
      // A stray pointerdown while already dragging: cancel the old one first.
      if (dragRef.current) teardownRef.current();
      dragRef.current = state;
      onInteractingChange?.(true);
      window.addEventListener("pointermove", onPointerMove);
      window.addEventListener("pointerup", onPointerUp);
      window.addEventListener("pointercancel", cancelDrag);
      window.addEventListener("keydown", onKeyDownDuringDrag, true);
    },
    [
      onPointerMove,
      onPointerUp,
      cancelDrag,
      onKeyDownDuringDrag,
      onInteractingChange,
    ],
  );

  // Clean up any in-flight drag if the canvas unmounts mid-gesture.
  useEffect(() => () => teardownRef.current(), []);

  const onBlockPointerDown = useCallback(
    (e: React.PointerEvent, block: SlideBlock) => {
      if (!interactive) return;
      e.stopPropagation();
      const additive = e.shiftKey;

      // Resolve the selection this drag operates on.
      let activeIds: Set<string>;
      if (selected.has(block.id)) {
        if (additive) {
          // Shift-click an already-selected block deselects it; no drag.
          onSelect?.(block.id, true);
          return;
        }
        activeIds = new Set(selected);
      } else {
        activeIds = additive
          ? new Set([...selected, block.id])
          : new Set([block.id]);
        onSelect?.(block.id, additive);
      }

      const base = docRef.current;
      const startRects = new Map<string, Rect>();
      const before = new Map<string, SlideBlock>();
      for (const id of activeIds) {
        const b = findBlock(base, id);
        if (b) {
          startRects.set(id, b.rect);
          before.set(id, b);
        }
      }
      beginDrag({
        mode: "move",
        primaryId: block.id,
        start: toNorm(e),
        baseDoc: base,
        startRects,
        before,
        moved: false,
      });
    },
    [interactive, selected, onSelect, beginDrag, toNorm],
  );

  const onHandlePointerDown = useCallback(
    (e: React.PointerEvent, block: SlideBlock, handle: HandleId) => {
      if (!interactive) return;
      e.stopPropagation();
      beginDrag({
        mode: "resize",
        id: block.id,
        handle,
        start: toNorm(e),
        baseDoc: docRef.current,
        startRect: block.rect,
        before: block,
        moved: false,
      });
    },
    [interactive, beginDrag, toNorm],
  );

  const singleSelected = selected.size === 1;

  return (
    <div
      ref={rootRef}
      className="relative overflow-hidden"
      style={{ width, height, ...backgroundStyle(doc.background) }}
      onPointerDown={interactive ? () => onSelect?.(null, false) : undefined}
    >
      {doc.blocks.map((block) => {
        const isSel = selected.has(block.id);
        return (
          <div
            key={block.id}
            style={blockBoxStyle(block.rect, width, height)}
            className={cn(
              interactive && "cursor-move",
              interactive && isSel
                ? "outline outline-2 outline-[var(--color-accent)]"
                : interactive &&
                    "hover:outline hover:outline-1 hover:outline-white/30",
            )}
            onPointerDown={(e) => onBlockPointerDown(e, block)}
          >
            {isTextBlock(block) ? (
              // The non-interactive view mirrors the live output, so it auto-fits
              // (shared `fitText` ⇒ preview == output). While actively editing we
              // show the authored size so manual sizing isn't fought by the fit.
              <div
                style={
                  interactive
                    ? textBlockStyle(block, height)
                    : autoFitTextBlockStyle(block, height)
                }
              >
                {block.text}
              </div>
            ) : (
              <div className="grid h-full w-full place-items-center bg-white/5 text-xs text-white/50">
                {block.type}
              </div>
            )}

            {interactive && isSel && singleSelected
              ? HANDLES.map((h) => (
                  <span
                    key={h}
                    onPointerDown={(e) => onHandlePointerDown(e, block, h)}
                    className="absolute z-10 h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 rounded-[2px] border border-[var(--color-accent)] bg-[var(--color-bg)]"
                    style={handleStyle(h)}
                  />
                ))
              : null}
          </div>
        );
      })}

      {/* Snap guides (drawn imperatively during drag). */}
      <div
        ref={guidesRef}
        className="pointer-events-none absolute inset-0 z-20"
      />
    </div>
  );
}

const EMPTY: ReadonlySet<string> = new Set();

function handleStyle(h: HandleId): React.CSSProperties {
  const left = h.includes("w") ? "0%" : h.includes("e") ? "100%" : "50%";
  const top = h.includes("n") ? "0%" : h.includes("s") ? "100%" : "50%";
  const cursorMap: Record<HandleId, string> = {
    nw: "nwse-resize",
    se: "nwse-resize",
    ne: "nesw-resize",
    sw: "nesw-resize",
    n: "ns-resize",
    s: "ns-resize",
    e: "ew-resize",
    w: "ew-resize",
  };
  return { left, top, cursor: cursorMap[h] };
}
