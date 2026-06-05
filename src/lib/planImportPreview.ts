/**
 * Plan-import preview seam — the wiring around the pure `mapPlanToCues` adapter.
 *
 * `mapPlanToCues` (see `./planToStage`) is pure: it turns an incoming SundayPlan
 * `ServicePlan` into a flat `Cue[]`, but only resolves song arrangements through
 * a `songsByItem` lookup map it is *handed*. Nothing in Stage built that map from
 * Stage's OWN song/arrangement catalogue — that is the gap this file closes.
 *
 * The actual catalogue read (find a song by title, list its arrangements) is an
 * I/O call the caller makes through the existing IPC (`song.search` /
 * `arrangement.list`) — exactly the same commands the queue editor already uses.
 * To keep this seam unit-testable with no Tauri, the catalogue is injected as a
 * small `CatalogueResolver` interface, and `buildSongsByItem` is pure over it.
 *
 * This is the *preview / lightweight* counterpart to the DB-backed Rust
 * `service_import_sundayplan` command (which persists matched/stubbed rows). It
 * never writes anything and never reaches the network — it consumes a plan the
 * operator already holds (pasted JSON) and shows the cue list it would produce.
 */

import {
  mapPlanToCues,
  planItemKey,
  type PlanServicePlan,
  type PlanSetlistItem,
  type PlanMapResult,
  type SongItemEntry,
  type SongsByItem,
  type SongVariant,
} from "@/lib/planToStage";
import { planKindToStage } from "@/lib/serviceItemKind";

/**
 * What a song item resolves to in Stage's catalogue. Mirrors the fields the
 * queue editor reads: the local song id + title, and the arrangements Stage
 * knows for it (each becomes a selectable variant). `null` when no local song
 * matches the plan item's title — the item is then kept as a titled placeholder
 * by the pure adapter (never dropped).
 */
export interface ResolvedCatalogueSong {
  songId: string;
  title: string;
  /** Stage arrangements for this song, in catalogue order. */
  arrangements: ResolvedArrangement[];
}

export interface ResolvedArrangement {
  /** Stage arrangement id — the stable variant key. */
  id: string;
  /** Operator-facing arrangement name (the variant label). */
  name: string;
  /** Whether this is the song's default arrangement. */
  isDefault: boolean;
}

/**
 * The catalogue read the caller injects. Async because the real implementation
 * goes through IPC; the test passes a synchronous fake. Returns `null` when the
 * plan item names no song Stage can match.
 */
export interface CatalogueResolver {
  /**
   * Resolve one plan *song* item against Stage's catalogue. `title` is the
   * match key (Stage has no SundayPlan ids); implementations match by title the
   * same way the Rust importer does.
   */
  resolveSong(item: PlanSetlistItem): Promise<ResolvedCatalogueSong | null>;
}

/**
 * Turn a resolved catalogue song into the `SongItemEntry` shape `mapPlanToCues`
 * resolves against. Each Stage arrangement becomes a variant; the requested
 * arrangement (the item's `key_override` / song default key) is matched against
 * arrangement *name* OR id, so a plan that names "Acoustic" or an explicit id
 * both resolve, and a miss falls back to the song's default arrangement.
 */
export function catalogueSongToEntry(
  resolved: ResolvedCatalogueSong,
): SongItemEntry {
  const variants: SongVariant[] = resolved.arrangements.map((a) => ({
    variantId: a.id,
    label: a.name,
  }));
  // Also let the plan name an arrangement by its display name, not just its id,
  // since a SundayPlan author types names, not Stage's internal ids.
  for (const a of resolved.arrangements) {
    if (a.name && a.name !== a.id) {
      variants.push({ variantId: a.name, label: a.name });
    }
  }
  const defaultVariantId =
    resolved.arrangements.find((a) => a.isDefault)?.id ??
    resolved.arrangements[0]?.id ??
    null;
  return {
    songId: resolved.songId,
    title: resolved.title,
    variants,
    defaultVariantId,
  };
}

/**
 * Build the `songsByItem` map `mapPlanToCues` needs, by resolving every *song*
 * item in the plan against Stage's catalogue. Keyed by the SAME `planItemKey`
 * the adapter and the usage ledger use, so the lookup lines up 1:1. Items that
 * resolve to no local song are simply absent from the map — the adapter keeps
 * them in the running order as titled placeholders.
 *
 * Pure over the injected `CatalogueResolver`: no IPC, no network, no DB.
 */
export async function buildSongsByItem(
  plan: PlanServicePlan,
  catalogue: CatalogueResolver,
): Promise<SongsByItem> {
  const map: Record<string, SongItemEntry> = {};
  for (const item of plan.items) {
    // Only song-kind items resolve a catalogue song; everything else maps to a
    // placeholder cue inside the adapter and needs no entry here. Uses the SAME
    // kind mapping the adapter does, so the two never disagree about what a song
    // is (e.g. SundayPlan's `worship_set` → `song`).
    if (planKindToStage(item.kind) !== "song") continue;
    const resolved = await catalogue.resolveSong(item);
    if (resolved) map[planItemKey(item)] = catalogueSongToEntry(resolved);
  }
  return map;
}

/**
 * One-shot preview: resolve the plan's songs against Stage's catalogue and map
 * the whole plan to cues. The single entry point a UI flow calls after the
 * operator pastes a plan. Returns the same `PlanMapResult` the pure adapter
 * yields (ordered cues + per-item resolution detail incl. fallback flags).
 */
export async function previewPlanImport(
  plan: PlanServicePlan,
  catalogue: CatalogueResolver,
): Promise<PlanMapResult> {
  const songsByItem = await buildSongsByItem(plan, catalogue);
  return mapPlanToCues(plan, songsByItem);
}
