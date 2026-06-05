/**
 * Plan → Stage cue mapper — golden-fixture test.
 *
 * Drives the platform golden `service_plan.json` (the canonical Plan→Stage
 * handoff payload, copied verbatim into `tests/fixtures/`) through the pure
 * `mapPlanToCues` adapter and asserts the four things the handoff must get
 * right, each of which would break on a regression:
 *
 *   1. ORDER — items map to cues in `position` order, even when the payload is
 *      shuffled (defensive re-sort).
 *   2. SONG-VARIANT RESOLUTION — a song item resolves the exact arrangement
 *      variant it names through the `songsByItem` lookup.
 *   3. ARRANGEMENT FALLBACK — when the named arrangement is missing, the song
 *      still produces a cue (default arrangement, flagged), never dropped.
 *   4. INDEX-KEY ALIGNMENT — each cue's `source.service_item_id` is the SAME
 *      key the `songsByItem` map is keyed by, so the planner, the usage ledger
 *      (`usage_event.json` → `item-7`) and the live bridge stay in lock-step.
 */
import { describe, it, expect } from "vitest";

import {
  mapPlanToCues,
  planItemKey,
  type PlanServicePlan,
  type SongsByItem,
} from "@/lib/planToStage";
import goldenPlan from "../fixtures/service_plan.json";

// The golden fixture (platform `service_plan.json`): welcome (pos 0), song
// "Amazing Grace" (pos 1, key_override "A"), scripture "John 3:16-21" (pos 2).
const plan = goldenPlan as unknown as PlanServicePlan;

// The song item is at position 1 → positional key "item-1". Stage knows two
// arrangements for that song: an "A" arrangement (matching the item's
// key_override) and a default "G" arrangement.
const SONG_KEY = "item-1";
const songsByItem: SongsByItem = {
  [SONG_KEY]: {
    songId: "22222222-2222-2222-2222-222222222222",
    title: "Amazing Grace",
    defaultVariantId: "arr-G",
    variants: [
      { variantId: "arr-G", label: "Default (G)" },
      { variantId: "A", label: "Arrangement A" },
    ],
  },
};

describe("mapPlanToCues — Plan→Stage golden handoff", () => {
  it("preserves running order and maps each kind to its Stage cue", () => {
    const { serviceId, cues, items } = mapPlanToCues(plan, songsByItem);

    expect(serviceId).toBe("33333333-3333-3333-3333-333333333333");
    // One cue per item, in plan order: welcome, song, scripture.
    expect(cues).toHaveLength(3);
    expect(items.map((i) => i.stageKind)).toEqual([
      "custom_deck", // welcome → custom_deck (canonical mapping)
      "song",
      "scripture",
    ]);

    // welcome → Pause placeholder labelled from its title.
    const welcome = cues[0];
    expect(welcome.kind).toBe("pause");
    if (welcome.kind === "pause") {
      expect(welcome.label).toBe("Welcome & notices");
    }

    // scripture → slide carrying the reference, self-identifying label.
    const scripture = cues[2];
    expect(scripture.kind).toBe("show_slide");
    if (scripture.kind === "show_slide") {
      expect(scripture.slide_content.reference).toBe("John 3:16-21");
      expect(scripture.source.display_label).toBe("John 3:16-21");
    }
  });

  it("resolves the exact arrangement variant the song item names", () => {
    const { cues, items } = mapPlanToCues(plan, songsByItem);

    const songItem = items[1];
    expect(songItem.stageKind).toBe("song");
    // key_override "A" matched the "A" arrangement → no fallback.
    expect(songItem.arrangementFallback).toBe(false);

    const songCue = cues[1];
    expect(songCue.kind).toBe("show_slide");
    if (songCue.kind === "show_slide") {
      expect(songCue.slide_content.section_label).toBe("Arrangement A");
      expect(songCue.source.display_label).toBe(
        "Amazing Grace — Arrangement A",
      );
    }
  });

  it("falls back to the default arrangement when the named one is missing", () => {
    // Same fixture, but Stage no longer knows the "A" arrangement the item asks
    // for — only the default "G". The song must NOT be dropped: it resolves to
    // the default arrangement and is flagged as a fallback.
    const onlyDefault: SongsByItem = {
      [SONG_KEY]: {
        songId: "22222222-2222-2222-2222-222222222222",
        title: "Amazing Grace",
        defaultVariantId: "arr-G",
        variants: [{ variantId: "arr-G", label: "Default (G)" }],
      },
    };

    const { cues, items } = mapPlanToCues(plan, onlyDefault);

    const songItem = items[1];
    expect(songItem.arrangementFallback).toBe(true);

    const songCue = cues[1];
    expect(songCue.kind).toBe("show_slide");
    if (songCue.kind === "show_slide") {
      expect(songCue.slide_content.section_label).toBe("Default (G)");
      expect(songCue.source.display_label).toBe("Amazing Grace — Default (G)");
    }
  });

  it("aligns every cue's index key with the songsByItem convention", () => {
    const { cues } = mapPlanToCues(plan, songsByItem);

    // The song cue's source key is exactly the key songsByItem is keyed by.
    const songCue = cues[1];
    expect(songCue.kind).toBe("show_slide");
    if (songCue.kind === "show_slide") {
      expect(songCue.source.service_item_id).toBe(SONG_KEY);
      expect(songCue.source.service_item_id in songsByItem).toBe(true);
    }

    // The scripture cue carries its own positional key, not the song's.
    const scriptureCue = cues[2];
    if (scriptureCue.kind === "show_slide") {
      expect(scriptureCue.source.service_item_id).toBe("item-2");
    }

    // planItemKey is the single source of that convention.
    expect(planItemKey(plan.items[1])).toBe(SONG_KEY);
  });

  it("keeps the live order stable even when the payload is shuffled", () => {
    const shuffled: PlanServicePlan = {
      ...plan,
      items: [plan.items[2], plan.items[0], plan.items[1]],
    };
    const { items } = mapPlanToCues(shuffled, songsByItem);
    // Re-sorted by position → welcome, song, scripture, regardless of input order.
    expect(items.map((i) => i.itemKey)).toEqual(["item-0", "item-1", "item-2"]);
    expect(items.map((i) => i.stageKind)).toEqual([
      "custom_deck",
      "song",
      "scripture",
    ]);
  });

  it("keeps a song in the order as a placeholder when it has no catalogue entry", () => {
    // No songsByItem map at all → the song item must still appear, as a Pause
    // the operator can advance through (never silently dropped).
    const { cues, items } = mapPlanToCues(plan);
    expect(cues).toHaveLength(3);
    const songItem = items[1];
    expect(songItem.stageKind).toBe("song");
    expect(songItem.cue.kind).toBe("pause");
    if (songItem.cue.kind === "pause") {
      expect(songItem.cue.label).toBe("Amazing Grace");
    }
  });
});
