/**
 * Slide-document helpers — Phase 3.1.
 *
 * Pure, framework-free functions over the `SlideDoc` model (generated from
 * the Rust `services::slide_doc`). Two responsibilities:
 *
 *   1. Immutable construction/editing of a `SlideDoc` (never mutate in place;
 *      every helper returns a new doc so React state stays predictable and
 *      the undo history can hold references safely).
 *   2. Turning the design model into CSS so the editor preview renders the
 *      *same* pixels the live output will (the "preview == output" promise).
 *
 * Geometry is normalized (0–1) as a fraction of the frame; font `size` is px
 * on a virtual 1920×1080 stage, scaled to the actual canvas height at render.
 */

import type { CSSProperties } from "react";
import type {
  SlideBackground,
  SlideBlock,
  SlideDoc,
  SlideRect,
  TextStyle,
} from "@/lib/bindings";

/** The text variant of a block — the only kind the v1 editor authors. */
export type TextBlock = Extract<SlideBlock, { type: "text" }>;

/** Virtual stage the design is authored against. */
export const STAGE_HEIGHT = 1080;
export const STAGE_ASPECT = 16 / 9;

export function isTextBlock(b: SlideBlock): b is TextBlock {
  return b.type === "text";
}

// ── Construction ───────────────────────────────────────────────────────────

export function blankDoc(): SlideDoc {
  return { background: { type: "color", value: "#0b1020" }, blocks: [] };
}

export function defaultTextStyle(): TextStyle {
  return {
    family: null,
    size: 64,
    weight: 700,
    color: "#ffffff",
    italic: false,
    shadow: "0 2px 8px rgba(0,0,0,0.6)",
  };
}

let blockSeq = 0;
export function newBlockId(): string {
  blockSeq += 1;
  return `blk_${Date.now().toString(36)}_${blockSeq.toString(36)}`;
}

export function newTextBlock(
  text = "Ny tekst",
  rect?: Partial<SlideRect>,
): TextBlock {
  return {
    type: "text",
    id: newBlockId(),
    text,
    rect: { x: 0.1, y: 0.4, w: 0.8, h: 0.2, ...rect },
    align: "center",
    valign: "middle",
    style: defaultTextStyle(),
  };
}

/** Seed a slide with one centered text block (used for "new slide"). */
export function docWithText(text: string): SlideDoc {
  return { background: blankDoc().background, blocks: [newTextBlock(text)] };
}

/**
 * Parse a `slide.content` JSON string into a `SlideDoc`. Mirrors the Rust
 * `SlideDoc::from_json`: a corrupt value yields a blank doc rather than
 * throwing, so a bad slide still opens for the user to fix.
 */
export function parseDoc(content: string): SlideDoc {
  try {
    const v = JSON.parse(content) as unknown;
    if (
      v &&
      typeof v === "object" &&
      "background" in v &&
      "blocks" in v &&
      Array.isArray((v as SlideDoc).blocks)
    ) {
      return v as SlideDoc;
    }
  } catch {
    // fall through to blank
  }
  return blankDoc();
}

// ── Immutable edits ──────────────────────────────────────────────────────────

export function findBlock(doc: SlideDoc, id: string): SlideBlock | undefined {
  return doc.blocks.find((b) => b.id === id);
}

export function addBlock(doc: SlideDoc, block: SlideBlock): SlideDoc {
  return { ...doc, blocks: [...doc.blocks, block] };
}

export function replaceBlock(doc: SlideDoc, block: SlideBlock): SlideDoc {
  return {
    ...doc,
    blocks: doc.blocks.map((b) => (b.id === block.id ? block : b)),
  };
}

export function removeBlocks(
  doc: SlideDoc,
  ids: ReadonlySet<string> | string[],
): SlideDoc {
  const set = ids instanceof Set ? ids : new Set(ids);
  return { ...doc, blocks: doc.blocks.filter((b) => !set.has(b.id)) };
}

export function setBackground(
  doc: SlideDoc,
  background: SlideBackground,
): SlideDoc {
  return { ...doc, background };
}

/** Patch a text block's top-level fields (rect/align/text). */
export function patchTextBlock(
  block: TextBlock,
  patch: Partial<Omit<TextBlock, "type" | "id" | "style">>,
): TextBlock {
  return { ...block, ...patch };
}

/** Patch a text block's typographic style. */
export function patchStyle(
  block: TextBlock,
  patch: Partial<TextStyle>,
): TextBlock {
  return { ...block, style: { ...block.style, ...patch } };
}

// ── Geometry ───────────────────────────────────────────────────────────────

export function clamp01(v: number): number {
  return Math.min(1, Math.max(0, v));
}

/** Clamp a rect so it stays at least partly on-frame and never inverts. */
export function clampRect(rect: SlideRect): SlideRect {
  const w = Math.min(1, Math.max(0.02, rect.w));
  const h = Math.min(1, Math.max(0.02, rect.h));
  return {
    w,
    h,
    x: Math.min(1 - w, Math.max(0, rect.x)),
    y: Math.min(1 - h, Math.max(0, rect.y)),
  };
}

// ── Rendering (preview == output) ─────────────────────────────────────────────

export function backgroundStyle(bg: SlideBackground): CSSProperties {
  switch (bg.type) {
    case "color":
      return { background: bg.value };
    case "gradient":
      return { background: bg.value };
    case "image":
      return {
        backgroundImage: `url(${bg.value})`,
        backgroundSize: "cover",
        backgroundPosition: "center",
      };
    case "video":
      // Static poster colour in the editor; the live renderer plays the video.
      return { background: "#000" };
  }
}

/** Absolute-position box for a block on a canvas of `canvasW`×`canvasH` px. */
export function blockBoxStyle(
  rect: SlideRect,
  canvasW: number,
  canvasH: number,
): CSSProperties {
  return {
    position: "absolute",
    left: `${rect.x * canvasW}px`,
    top: `${rect.y * canvasH}px`,
    width: `${rect.w * canvasW}px`,
    height: `${rect.h * canvasH}px`,
  };
}

const H_TO_FLEX: Record<TextBlock["align"], CSSProperties["alignItems"]> = {
  left: "flex-start",
  center: "center",
  right: "flex-end",
};
const V_TO_FLEX: Record<TextBlock["valign"], CSSProperties["justifyContent"]> =
  {
    top: "flex-start",
    middle: "center",
    bottom: "flex-end",
  };

/** Inner text styling for a text block, scaled to the canvas height. */
export function textBlockStyle(
  block: TextBlock,
  canvasH: number,
): CSSProperties {
  const scale = canvasH / STAGE_HEIGHT;
  return {
    display: "flex",
    flexDirection: "column",
    height: "100%",
    width: "100%",
    overflow: "hidden",
    justifyContent: V_TO_FLEX[block.valign],
    alignItems: H_TO_FLEX[block.align],
    textAlign: block.align,
    color: block.style.color,
    fontFamily: block.style.family ?? "var(--font-sans)",
    fontSize: `${block.style.size * scale}px`,
    fontWeight: block.style.weight,
    fontStyle: block.style.italic ? "italic" : "normal",
    lineHeight: 1.15,
    textShadow: block.style.shadow ?? "none",
    whiteSpace: "pre-wrap",
    wordBreak: "break-word",
  };
}
