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
