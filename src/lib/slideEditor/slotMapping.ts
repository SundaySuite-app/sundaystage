/**
 * slotMapping — pure apply-template logic (deep-stage-2).
 *
 * The frontend mirror of Rust `services::theme::map_content_to_slots`. Given a
 * template layout and a content payload, it resolves the `slot_name -> text`
 * map that the backend's `render_slide` (via `template_render`) turns into a
 * SlideDoc. Kept free of React/IPC so the mapping rules can be unit-tested
 * deterministically — the exact contract the live engine depends on.
 *
 * Rules (must match the Rust side byte-for-byte in behaviour):
 *   - each slot pulls from the payload field matching its SlotRole;
 *   - a missing/blank field leaves that slot out of the map (empty, not broken);
 *   - when one role appears in multiple slots, the text is split paragraph-wise
 *     (blank-line separated) left-to-right; unsplittable text goes to slot 0.
 */

import type {
  SlideContentPayload,
  SlideDoc,
  SlotRole,
  TemplateLayout,
} from "@/lib/bindings";
import { isTextBlock } from "./doc";

/** An empty payload (all fields null). */
export function emptyPayload(): SlideContentPayload {
  return {
    title: null,
    body: null,
    lyrics: null,
    reference: null,
    footer: null,
    image: null,
  };
}

/** The payload field that feeds a given slot role. */
function fieldFor(payload: SlideContentPayload, role: SlotRole): string | null {
  switch (role) {
    case "title":
      return payload.title;
    case "body":
      return payload.body;
    case "lyrics":
      return payload.lyrics;
    case "reference":
      return payload.reference;
    case "footer":
      return payload.footer;
    case "image":
      return payload.image;
    default:
      return null;
  }
}

/**
 * Split `text` into at most `n` chunks on blank-line paragraph boundaries,
 * balancing paragraphs left-to-right. Fewer paragraphs than chunks → leading
 * chunks fill first; no blank-line breaks → chunk 0 gets everything.
 */
function splitInto(text: string, n: number): string[] {
  if (n <= 1) return [text];
  const paras = text
    .split("\n\n")
    .map((p) => p.trim())
    .filter((p) => p.length > 0);
  if (paras.length <= 1) {
    const v = Array.from({ length: n }, () => "");
    v[0] = text;
    return v;
  }
  const chunks: string[][] = Array.from({ length: n }, () => []);
  const per = Math.ceil(paras.length / n);
  paras.forEach((para, i) => {
    const idx = Math.min(Math.floor(i / per), n - 1);
    chunks[idx].push(para);
  });
  return chunks.map((c) => c.join("\n\n"));
}

/**
 * Map a content payload onto a template's named slots → `slot_name -> text`.
 * Deterministic and total: blank/missing fields simply omit their slot.
 */
export function mapContentToSlots(
  layout: TemplateLayout,
  payload: SlideContentPayload,
): Record<string, string> {
  const out: Record<string, string> = {};

  // Group slot names by role, preserving template order.
  const byRole: Array<{ role: SlotRole; names: string[] }> = [];
  for (const s of layout.slots) {
    const existing = byRole.find((g) => g.role === s.role);
    if (existing) existing.names.push(s.name);
    else byRole.push({ role: s.role, names: [s.name] });
  }

  for (const { role, names } of byRole) {
    const raw = fieldFor(payload, role);
    const text = raw?.trim();
    if (!text) continue;
    if (names.length === 1) {
      out[names[0]] = text;
      continue;
    }
    const parts = splitInto(text, names.length);
    names.forEach((name, i) => {
      if (parts[i] && parts[i].trim().length > 0) out[name] = parts[i];
    });
  }

  return out;
}

/**
 * Derive a best-effort content payload from the current slide so that applying
 * a template re-flows the existing text instead of discarding it. The doc model
 * is unlabelled positioned blocks, so this is a heuristic: all text blocks are
 * joined into `body` and `lyrics` (the two free-text roles), and the first
 * image block (if any) becomes `image`. The first line also seeds `title` so
 * title-bearing templates aren't left blank. Operators can refine via the
 * payload form before applying.
 */
export function derivePayloadFromDoc(doc: SlideDoc): SlideContentPayload {
  const text = doc.blocks
    .filter(isTextBlock)
    .map((b) => b.text)
    .filter((t) => t.trim().length > 0)
    .join("\n");
  const image = doc.blocks.find((b) => b.type === "image");
  const firstLine = text.split("\n").find((l) => l.trim().length > 0) ?? null;
  return {
    title: firstLine,
    body: text || null,
    lyrics: text || null,
    reference: null,
    footer: null,
    image: image && image.type === "image" ? image.src : null,
  };
}
