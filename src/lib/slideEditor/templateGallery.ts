/**
 * templateGallery — pure selection/apply logic for the deck template gallery
 * (deep-stage-2). React-free so the picking + apply-plan rules are unit-tested
 * without rendering the gallery or mocking IPC.
 */

import type { SlideContentPayload, Template } from "@/lib/bindings";
import { parseLayout } from "./theme";
import { mapContentToSlots } from "./slotMapping";

export interface GallerySection {
  /** "builtin" | "custom" */
  kind: "builtin" | "custom";
  templates: Template[];
}

/**
 * Partition templates into built-in and custom sections (built-ins first),
 * dropping empty sections. `is_builtin` is SQLite's 0/1 integer.
 */
export function gallerySections(templates: Template[]): GallerySection[] {
  const builtin = templates.filter((t) => Number(t.is_builtin) !== 0);
  const custom = templates.filter((t) => Number(t.is_builtin) === 0);
  const out: GallerySection[] = [];
  if (builtin.length > 0) out.push({ kind: "builtin", templates: builtin });
  if (custom.length > 0) out.push({ kind: "custom", templates: custom });
  return out;
}

/** The render arguments for applying a template to a payload. */
export interface ApplyPlan {
  templateId: string;
  slotText: Record<string, string>;
}

/**
 * Build the render plan for applying `template` to `payload`. The `slotText`
 * is produced by the same pure mapping the backend uses, so the preview and
 * the persisted slide agree.
 */
export function buildApplyPlan(
  template: Template,
  payload: SlideContentPayload,
): ApplyPlan {
  const layout = parseLayout(template);
  return {
    templateId: template.id,
    slotText: mapContentToSlots(layout, payload),
  };
}

/** Case-insensitive name filter for the gallery search box. */
export function filterTemplates(
  templates: Template[],
  query: string,
): Template[] {
  const q = query.trim().toLowerCase();
  if (!q) return templates;
  return templates.filter((t) => t.name.toLowerCase().includes(q));
}
