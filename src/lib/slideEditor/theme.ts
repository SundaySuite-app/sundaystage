/**
 * Theme helpers — Phase 3.2 (frontend side).
 *
 * Pure functions to parse stored theme/template JSON and to stamp a theme's
 * tokens onto a slide document. "Applying a theme" rewrites the slide
 * background and the typographic fields of every text block, while preserving
 * each block's position, size, and alignment — so a slide takes on a church's
 * look without losing its layout.
 */

import type {
  SlideDoc,
  Template,
  TemplateLayout,
  Theme,
  ThemeTokens,
} from "@/lib/bindings";

export function defaultTokens(): ThemeTokens {
  return {
    background: {
      type: "gradient",
      value: "linear-gradient(160deg, #1a2a52, #0b1020)",
    },
    text_color: "#ffffff",
    accent_color: "#e8c069",
    font_family: "Inter, system-ui, sans-serif",
    heading_weight: 700,
    body_size: 64,
    shadow: "0 2px 8px rgba(0,0,0,0.6)",
  };
}

export function parseTokens(theme: Theme): ThemeTokens {
  try {
    const t = JSON.parse(theme.tokens) as ThemeTokens;
    if (t && typeof t === "object" && "background" in t) return t;
  } catch {
    // fall through
  }
  return defaultTokens();
}

export function parseLayout(template: Template): TemplateLayout {
  try {
    const l = JSON.parse(template.slots) as TemplateLayout;
    if (l && Array.isArray(l.slots)) return l;
  } catch {
    // fall through
  }
  return { slots: [] };
}

/**
 * Stamp a theme's tokens onto a slide: new background, and theme typography on
 * every text block (keeping per-block size/rect/alignment).
 */
export function applyThemeToDoc(doc: SlideDoc, tokens: ThemeTokens): SlideDoc {
  return {
    background: tokens.background,
    blocks: doc.blocks.map((b) =>
      b.type === "text"
        ? {
            ...b,
            style: {
              ...b.style,
              family: tokens.font_family,
              weight: tokens.heading_weight,
              color: tokens.text_color,
              shadow: tokens.shadow,
            },
          }
        : b,
    ),
  };
}

/** Combined plain text of a slide's text blocks (for re-rendering via a template). */
export function combinedText(doc: SlideDoc): string {
  return doc.blocks
    .filter((b) => b.type === "text")
    .map((b) => (b.type === "text" ? b.text : ""))
    .filter((t) => t.trim().length > 0)
    .join("\n");
}
