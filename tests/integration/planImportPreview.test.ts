/**
 * Plan-import preview seam — the wiring around the pure `mapPlanToCues` adapter.
 *
 * `planToStage.test.ts` proves the pure mapper. This proves the SEAM that the
 * golden-fixture plan, run through the catalogue-resolver + `buildSongsByItem`,
 * actually produces cues — the bit that was missing (nothing built the real
 * `songsByItem` map from Stage's own catalogue and called the adapter).
 *
 * The catalogue read is injected as a fake `CatalogueResolver` (the real one
 * goes through IPC), so this is fully offline. It asserts the three things the
 * seam must get right:
 *   1. CUES CREATED — the golden plan maps to one cue per item, in order.
 *   2. VARIANT RESOLVED — a song whose plan key names a known arrangement
 *      resolves that exact arrangement (no fallback).
 *   3. FALLBACK FLAGGED — a song whose named arrangement Stage doesn't have
 *      still produces a cue (default arrangement), flagged as a fallback.
 */
import { describe, it, expect } from "vitest";

import {
  buildSongsByItem,
  previewPlanImport,
  catalogueSongToEntry,
  type CatalogueResolver,
  type ResolvedCatalogueSong,
} from "@/lib/planImportPreview";
import { planItemKey, type PlanServicePlan } from "@/lib/planToStage";
import goldenPlan from "../fixtures/service_plan.json";

const plan = goldenPlan as unknown as PlanServicePlan;

/**
 * A fake Stage catalogue: "Amazing Grace" exists with two arrangements, an
 * "A" arrangement (matching the golden plan item's `key_override: "A"`) and a
 * default "G". Matches by title like the real importer. Drives the resolver
 * from a per-test arrangement set so the fallback case is just a smaller set.
 */
function fakeCatalogue(
  arrangements: ResolvedCatalogueSong["arrangements"],
): CatalogueResolver {
  return {
    resolveSong: (item) => {
      const title = item.song_ref?.title ?? item.title ?? "";
      if (title.toLowerCase() !== "amazing grace") return Promise.resolve(null);
      return Promise.resolve({
        songId: "22222222-2222-2222-2222-222222222222",
        title: "Amazing Grace",
        arrangements,
      });
    },
  };
}

const BOTH = [
  { id: "A", name: "A", isDefault: false },
  { id: "arr-G", name: "G (default)", isDefault: true },
];
const ONLY_DEFAULT = [{ id: "arr-G", name: "G (default)", isDefault: true }];

describe("plan-import preview seam — buildSongsByItem + mapPlanToCues", () => {
  it("builds songsByItem only for resolvable song items, keyed like the adapter", async () => {
    const map = await buildSongsByItem(plan, fakeCatalogue(BOTH));

    // Only the song item (position 1) resolves; welcome + scripture carry none.
    expect(Object.keys(map)).toEqual(["item-1"]);
    expect(map["item-1"].songId).toBe("22222222-2222-2222-2222-222222222222");
    // The key is the SAME convention the pure adapter keys on.
    expect(planItemKey(plan.items[1])).toBe("item-1");
  });

  it("creates one cue per item and resolves the named arrangement variant", async () => {
    const { cues, items } = await previewPlanImport(plan, fakeCatalogue(BOTH));

    // One cue per plan item, in order: welcome, song, scripture.
    expect(cues).toHaveLength(3);
    expect(items.map((i) => i.stageKind)).toEqual([
      "custom_deck",
      "song",
      "scripture",
    ]);

    // The plan item's key_override "A" matched the "A" arrangement → no fallback.
    const songItem = items[1];
    expect(songItem.stageKind).toBe("song");
    expect(songItem.arrangementFallback).toBe(false);

    const songCue = cues[1];
    expect(songCue.kind).toBe("show_slide");
    if (songCue.kind === "show_slide") {
      expect(songCue.slide_content.section_label).toBe("A");
      expect(songCue.source.display_label).toBe("Amazing Grace — A");
      // Index key lines up with the songsByItem map.
      expect(songCue.source.service_item_id).toBe("item-1");
    }
  });

  it("flags a fallback when Stage lacks the named arrangement", async () => {
    const { cues, items } = await previewPlanImport(
      plan,
      fakeCatalogue(ONLY_DEFAULT),
    );

    // Song still appears (never dropped), resolved to the default arrangement
    // and flagged as a fallback.
    const songItem = items[1];
    expect(songItem.arrangementFallback).toBe(true);

    const songCue = cues[1];
    expect(songCue.kind).toBe("show_slide");
    if (songCue.kind === "show_slide") {
      expect(songCue.slide_content.section_label).toBe("G (default)");
    }
  });

  it("keeps an unmatched song in the order as a placeholder", async () => {
    // Catalogue that matches nothing → every song item is a titled placeholder.
    const empty: CatalogueResolver = {
      resolveSong: () => Promise.resolve(null),
    };
    const { cues, items } = await previewPlanImport(plan, empty);

    expect(cues).toHaveLength(3);
    const songItem = items[1];
    expect(songItem.stageKind).toBe("song");
    expect(songItem.cue.kind).toBe("pause");
    if (songItem.cue.kind === "pause") {
      expect(songItem.cue.label).toBe("Amazing Grace");
    }
  });

  it("catalogueSongToEntry exposes arrangements by id AND name, with a default", () => {
    const entry = catalogueSongToEntry({
      songId: "s1",
      title: "Song",
      arrangements: BOTH,
    });
    // Default is the arrangement flagged isDefault.
    expect(entry.defaultVariantId).toBe("arr-G");
    const variantIds = (entry.variants ?? []).map((v) => v.variantId);
    // Resolvable by id…
    expect(variantIds).toContain("A");
    expect(variantIds).toContain("arr-G");
    // …and by display name (a plan author types names, not Stage ids).
    expect(variantIds).toContain("G (default)");
  });
});
