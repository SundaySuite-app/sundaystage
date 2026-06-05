/**
 * Plan → Stage cue mapper (the planner-side handoff into the live engine).
 *
 * SundayPlan is the master service planner; SundayStage presents it live. When a
 * church builds a running order in Plan and hands it to Stage, this **pure**
 * adapter turns the incoming platform `ServicePlan` (a `SetlistItem[]`) into the
 * flat ordered `Cue[]` the operator advances through.
 *
 * It is the wire-side counterpart to the DB-backed Rust `CueCompiler` (which
 * compiles Stage's *own* `service_item` rows out of the local catalogue). This
 * adapter never touches a database: it works only from the handoff payload plus
 * a `songsByItem` variant-lookup map — so it is fully unit/fixture-testable with
 * no I/O, exactly like the other cross-app bridges (`liveBridge`/`usageEmitter`).
 *
 * Contract sources (mirrors `sunday-platform`; converge once published):
 *   - input  `SetlistItem` / `ServicePlan` — `contracts/src/service.ts`
 *   - kind   mapping                       — `contracts/src/mapping.ts`
 *            (reused via `./serviceItemKind`, NOT reinvented here)
 *   - output `Cue` / `CueSource`           — generated `@/lib/bindings`
 *
 * Resolution rules per item, in order:
 *   1. Map the Plan kind → Stage rendering kind via the shared mapping.
 *   2. A `song` item resolves its variant through `songsByItem[itemKey]`:
 *        - exact arrangement variant when present in the variant map,
 *        - else FALL BACK to the song's default arrangement (still one cue,
 *          flagged `arrangementFallback`) so a missing arrangement never drops
 *          the song from the live order.
 *   3. Non-song items map to their single placeholder cue (scripture → a slide
 *      carrying the reference; everything else → a `Pause` the operator clears).
 *   4. Order is preserved exactly (items are sorted by `position`).
 *
 * Index keys: each item's `service_item_id` / `songsByItem` key is derived the
 * SAME way the rest of Stage keys items — `item-{position}` (see the platform
 * `usage_event.json` / `live_cue.json` golden fixtures, e.g. `item-7`). Callers
 * that hold real ids can pass an `itemKey` override; absent that, the positional
 * key keeps the planner, the usage ledger and the live-bridge in lock-step.
 */

import type { Cue } from "@/lib/bindings";
import {
  planKindToStage,
  type StageServiceItemKind,
} from "@/lib/serviceItemKind";

/**
 * The cross-app song reference carried on a `song` SetlistItem. Mirrors the
 * platform `SongRef` contract (`contracts/src/song.ts`) — only the fields this
 * adapter reads are required.
 */
export interface PlanSongRef {
  /** Canonical SundaySong catalog id, or null if not yet linked. */
  sundaysong_id: string | null;
  /** The originating app's own row id (Plan/Stage local song), or null. */
  local_id?: string | null;
  title: string;
  default_key?: string | null;
}

/**
 * One row of an incoming SundayPlan running order. Mirrors the platform
 * `SetlistItem` contract (`contracts/src/service.ts`); only the fields the cue
 * mapper consumes are typed here.
 */
export interface PlanSetlistItem {
  position: number;
  /** SundayPlan local kind (`welcome` | `worship_set` | `song` | …). */
  kind: string;
  title: string | null;
  song_ref: PlanSongRef | null;
  scripture_ref: string | null;
  key_override: string | null;
  notes: string | null;
  /**
   * Optional explicit item id. When omitted the positional `item-{position}`
   * key is used (the convention the usage ledger + live bridge already follow).
   */
  item_id?: string | null;
}

/** The incoming handoff: a service plus its ordered items (platform `ServicePlan`). */
export interface PlanServicePlan {
  service: { id: string };
  items: PlanSetlistItem[];
}

/**
 * One arrangement variant Stage can resolve a song item to. Keyed in the
 * `songsByItem` map by the variant id the plan item asks for.
 */
export interface SongVariant {
  /** Arrangement / translation variant id (matches `key_override`-style hints). */
  variantId: string;
  /** Section labels in play order — drives the per-section cue this item gets. */
  label: string;
}

/**
 * The resolvable song behind one `song` service item: its canonical id, the
 * arrangement variants Stage knows about, and which one is the default fallback.
 * The map is keyed by the SAME item key the cue's `service_item_id` carries, so
 * a lookup here lines up 1:1 with `songsByItem` everywhere else in Stage.
 */
export interface SongItemEntry {
  songId: string;
  title: string;
  /** Available arrangement variants for this song, by `variantId`. */
  variants?: SongVariant[];
  /** The variant id to use when the item names none / names a missing one. */
  defaultVariantId?: string | null;
}

/** The variant-lookup map: item key → resolvable song. Mirrors `songsByItem`. */
export type SongsByItem = Readonly<Record<string, SongItemEntry>>;

/**
 * Derive the stable item key for a plan item — explicit `item_id` when the
 * producer supplied one, else the positional `item-{position}` convention the
 * usage ledger / live bridge already key on. Exported so callers that build a
 * `songsByItem` map for this adapter key it identically.
 */
export function planItemKey(item: PlanSetlistItem): string {
  const explicit = item.item_id?.trim();
  return explicit && explicit.length > 0 ? explicit : `item-${item.position}`;
}

/** What the mapper resolved for one item — the cue plus how it was resolved. */
export interface MappedItem {
  /** Stable item key the cue's `source.service_item_id` carries. */
  itemKey: string;
  /** Stage rendering kind this item mapped to. */
  stageKind: StageServiceItemKind;
  /** The single cue this item contributes to the live order. */
  cue: Cue;
  /**
   * For `song` items: true when the requested arrangement variant was missing
   * and the adapter fell back to the song's default arrangement. Always false
   * for non-song items and for an exact-variant hit.
   */
  arrangementFallback: boolean;
}

/** The full result of mapping a plan: ordered cues + per-item resolution detail. */
export interface PlanMapResult {
  serviceId: string;
  cues: Cue[];
  items: MappedItem[];
}

/** Pick the variant a song item asks for, with default-arrangement fallback. */
function resolveSongVariant(
  entry: SongItemEntry,
  requestedVariantId: string | null,
): { variant: SongVariant | null; fallback: boolean } {
  const variants = entry.variants ?? [];
  if (requestedVariantId) {
    const exact = variants.find((v) => v.variantId === requestedVariantId);
    if (exact) return { variant: exact, fallback: false };
  }
  // No request, or the requested arrangement is unknown to Stage → fall back to
  // the song's default arrangement (or the first known variant) rather than
  // dropping the song from the live order. A fallback is flagged whenever a
  // specific arrangement was asked for but not found.
  const def =
    variants.find((v) => v.variantId === entry.defaultVariantId) ??
    variants[0] ??
    null;
  const fallback =
    requestedVariantId != null && def?.variantId !== requestedVariantId;
  return { variant: def, fallback };
}

function songLabel(entry: SongItemEntry, variant: SongVariant | null): string {
  return variant ? `${entry.title} — ${variant.label}` : entry.title;
}

/**
 * Map one plan item → its cue. `index` is the 0-based running-order position the
 * cue lands at (used for stable cue ids), `serviceId` namespaces the cue ids.
 */
function mapItem(
  serviceId: string,
  item: PlanSetlistItem,
  songsByItem: SongsByItem,
): MappedItem {
  const itemKey = planItemKey(item);
  const stageKind = planKindToStage(item.kind);
  const cueBase = `svc:${serviceId}:${itemKey}`;

  if (stageKind === "song") {
    const entry = Object.prototype.hasOwnProperty.call(songsByItem, itemKey)
      ? songsByItem[itemKey]
      : undefined;
    if (entry) {
      const { variant, fallback } = resolveSongVariant(
        entry,
        item.key_override ?? item.song_ref?.default_key ?? null,
      );
      const cue: Cue = {
        kind: "show_slide",
        cue_id: `${cueBase}:song`,
        slide_content: {
          section_label: variant ? variant.label : null,
          text_lines: [],
          translation_lines: null,
          reference: null,
          sensitive_slide: false,
        },
        theme_id: null,
        template_id: null,
        source: {
          service_item_id: itemKey,
          item_cue_index: 0,
          display_label: songLabel(entry, variant),
        },
      };
      return { itemKey, stageKind, cue, arrangementFallback: fallback };
    }
    // Song item with no resolvable catalogue entry — keep it in the order as a
    // titled placeholder the operator can advance through (never drop it).
    const cue: Cue = {
      kind: "pause",
      cue_id: `${cueBase}:pause`,
      label: item.title ?? item.song_ref?.title ?? "Song",
    };
    return { itemKey, stageKind, cue, arrangementFallback: false };
  }

  if (stageKind === "scripture") {
    const ref = item.scripture_ref ?? item.title ?? "Scripture";
    const cue: Cue = {
      kind: "show_slide",
      cue_id: `${cueBase}:scripture`,
      slide_content: {
        section_label: null,
        text_lines: [],
        translation_lines: null,
        reference: ref,
        sensitive_slide: false,
      },
      theme_id: null,
      template_id: null,
      source: {
        service_item_id: itemKey,
        item_cue_index: 0,
        display_label: ref,
      },
    };
    return { itemKey, stageKind, cue, arrangementFallback: false };
  }

  // announcement / video / custom_deck / gap → a placeholder Pause the operator
  // advances manually (mirrors the Rust compiler's placeholder handling).
  const label =
    (item.notes?.trim() ? item.notes.trim() : null) ?? item.title ?? stageKind;
  const cue: Cue = {
    kind: "pause",
    cue_id: `${cueBase}:pause`,
    label,
  };
  return { itemKey, stageKind, cue, arrangementFallback: false };
}

/**
 * Map an incoming SundayPlan `ServicePlan` into Stage cues. Pure: no I/O.
 *
 * Items are taken in `position` order (defensively re-sorted so a shuffled
 * payload can't reorder the live output). Each item yields exactly one cue here
 * — the handoff is a skeleton the live engine later fleshes out with lyrics /
 * verses from the catalogue; what matters at the boundary is order, kind, song
 * resolution and the index-key alignment with `songsByItem`.
 */
export function mapPlanToCues(
  plan: PlanServicePlan,
  songsByItem: SongsByItem = {},
): PlanMapResult {
  const serviceId = plan.service.id;
  const ordered = [...plan.items].sort((a, b) => a.position - b.position);
  const items = ordered.map((item) => mapItem(serviceId, item, songsByItem));
  return { serviceId, cues: items.map((m) => m.cue), items };
}
