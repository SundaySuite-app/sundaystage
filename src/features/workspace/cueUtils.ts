/**
 * Small shared helpers for the operator workspace: turning a compiled `Cue`
 * into the `LiveFrame` that `SlideView` renders, a stable key, and a
 * human-readable label. Kept tiny and i18n-aware via an injected `t`.
 *
 * These mirror the helpers that used to live privately in `LivePreview`; the
 * workspace renders the *same* frames in the slide grid, the preview pane and
 * the live pane, so the operator sees one consistent rendering everywhere.
 */
import type { Cue, LiveFrame } from "@/lib/bindings";
import type { TKey } from "@/lib/i18n";

/** A parsed bible reference extracted from a cue's display_label. */
export interface BibleRef {
  book: string;
  chapter: number;
  verseStart: number | null;
  verseEnd: number | null;
}

/**
 * Attempt to parse a bible reference from a cue's display_label.
 * The Rust renderer produces labels like:
 *   "John 3"        → book="John",  chapter=3, verseStart=null
 *   "John 3:16"     → book="John",  chapter=3, verseStart=16, verseEnd=null
 *   "John 3:16-17"  → book="John",  chapter=3, verseStart=16, verseEnd=17
 *   "1 Corinthians 13:4-7"
 *
 * Returns null when the label does not match the expected format.
 */
export function parseBibleRef(label: string): BibleRef | null {
  // Pattern: optional leading digits for book number, then book name words,
  // then a chapter number, optionally followed by :verse[-verse].
  // Examples: "John 3", "John 3:16", "1 John 3:16-17"
  const m = label
    .trim()
    .match(
      /^((?:\d+\s+)?[A-Za-zÆØÅæøå]+(?:\s+[A-Za-zÆØÅæøå]+)*)\s+(\d+)(?::(\d+)(?:-(\d+))?)?$/,
    );
  if (!m) return null;
  const book = m[1].trim();
  const chapter = parseInt(m[2], 10);
  const verseStart = m[3] != null ? parseInt(m[3], 10) : null;
  const verseEnd = m[4] != null ? parseInt(m[4], 10) : null;
  if (!book || isNaN(chapter)) return null;
  return { book, chapter, verseStart, verseEnd };
}

/**
 * Returns true when the cue is a scripture (bible) slide whose display_label
 * can be parsed as a bible reference.
 */
export function isBibleCue(cue: Cue): boolean {
  if (cue.kind !== "show_slide") return false;
  return parseBibleRef(cue.source.display_label) !== null;
}

type T = (key: TKey, params?: Record<string, string | number>) => string;

/** Project a compiled cue onto the frame the output would show for it. */
export function frameFromCue(cue: Cue): LiveFrame {
  switch (cue.kind) {
    case "show_slide":
      return { kind: "slide", slide_content: cue.slide_content };
    case "black_out":
      return { kind: "black" };
    case "show_logo":
      return { kind: "logo" };
    case "pause":
      return { kind: "message", text: cue.label };
  }
}

export function cueId(cue: Cue): string {
  return cue.cue_id;
}

/** The service item a slide cue belongs to, or null for non-song cues. */
export function cueServiceItemId(cue: Cue): string | null {
  return cue.kind === "show_slide" ? cue.source.service_item_id : null;
}

export function cueDisplayLabel(cue: Cue, t: T): string {
  switch (cue.kind) {
    case "show_slide":
      return cue.source.display_label;
    case "black_out":
      return t("liveBlackout");
    case "show_logo":
      return t("liveShowLogo");
    case "pause":
      return t("livePausePrefix", { label: cue.label });
  }
}
