// Live-session → bridge driver. Confirms the pure state-diff turns a session
// transition into the right cue.advanced / now_playing / usage events: go-live
// emits the opening triplet, cue moves emit per-move events, the song-change
// gate keeps now_playing from churning within one song, usage logs once per
// service item, non-song cues stay quiet, blackout (same index) emits nothing,
// and end emits service.ended. The transports are NETWORK-UNVERIFIED seams and
// are not exercised here — only the decision layer.
import { describe, it, expect } from "vitest";

import { LiveSequence, type LiveEvent } from "@/lib/liveEmitter";
import {
  bridgeOnGoLive,
  bridgeOnCueChange,
  bridgeOnEnd,
  type BridgeCue,
  type LiveBridgeContext,
} from "@/lib/liveBridge";

const ctx = (
  overrides: Partial<LiveBridgeContext> = {},
): LiveBridgeContext => ({
  churchId: "ch-1",
  serviceId: "svc-2",
  serviceDate: "2026-06-14",
  wasStreamed: true,
  songsByItem: {
    "item-a": { songId: "song-a", title: "Amazing Grace", variantId: "arr-1" },
    "item-b": { songId: "song-b", title: "Oceans" },
  },
  ...overrides,
});

// Two slides of song A (item-a), then one slide of song B (item-b), then a
// non-song cue (empty serviceItemId).
const cues: BridgeCue[] = [
  {
    serviceItemId: "item-a",
    displayLabel: "Grace V1",
    sectionLabel: "Verse 1",
  },
  { serviceItemId: "item-a", displayLabel: "Grace C", sectionLabel: "Chorus" },
  {
    serviceItemId: "item-b",
    displayLabel: "Oceans V1",
    sectionLabel: "Verse 1",
  },
  { serviceItemId: "", displayLabel: "Kollekt", sectionLabel: null },
];

const types = (events: LiveEvent[]) => events.map((e) => e.type);

describe("bridgeOnGoLive", () => {
  it("emits service.live, then the opening cue.advanced + now_playing + usage", () => {
    const seq = new LiveSequence();
    const shown = new Set<string>();
    const out = bridgeOnGoLive(ctx(), cues, 0, seq, 1000, shown);

    expect(types(out.liveEvents)).toEqual([
      "service.live",
      "cue.advanced",
      "now_playing",
    ]);
    // Sequence is strictly increasing across the whole emission.
    expect(out.liveEvents.map((e) => e.sequence)).toEqual([1, 2, 3]);

    const live = out.liveEvents[0];
    expect(live.type).toBe("service.live");
    expect(live.emitted_at).toBe(new Date(1000).toISOString());

    expect(out.usageEvents).toHaveLength(1);
    const u = out.usageEvents[0];
    expect(u.song_id).toBe("song-a");
    expect(u.variant_id).toBe("arr-1");
    expect(u.service_date).toBe("2026-06-14");
    expect(u.was_streamed).toBe(true);
    expect(u.idempotency_key).toBe("usage:ch-1:item-a:2026-06-14");
    // The item is now marked shown.
    expect(shown.has("item-a")).toBe(true);
  });
});

describe("bridgeOnCueChange", () => {
  it("emits nothing when the index does not move (blackout/logo)", () => {
    const seq = new LiveSequence();
    const out = bridgeOnCueChange(ctx(), cues, 1, 1, seq, 0, new Set());
    expect(out.liveEvents).toEqual([]);
    expect(out.usageEvents).toEqual([]);
    expect(seq.current()).toBe(0); // sequence untouched
  });

  it("advancing within one song emits cue.advanced only (no now_playing, no usage)", () => {
    const seq = new LiveSequence();
    const shown = new Set<string>(["item-a"]); // song A already shown
    const out = bridgeOnCueChange(ctx(), cues, 0, 1, seq, 0, shown);
    // The song under the cursor is unchanged, so no now_playing churns (the
    // contract that keeps SundayRec chapters from advancing slide-by-slide),
    // and the usage one-shot guard suppresses a second log.
    expect(types(out.liveEvents)).toEqual(["cue.advanced"]);
    expect(out.usageEvents).toEqual([]);
  });

  it("moving to a new song logs usage once for the new item", () => {
    const seq = new LiveSequence();
    const shown = new Set<string>(["item-a"]);
    const out = bridgeOnCueChange(ctx(), cues, 1, 2, seq, 5000, shown);
    expect(types(out.liveEvents)).toEqual(["cue.advanced", "now_playing"]);
    expect(out.usageEvents).toHaveLength(1);
    expect(out.usageEvents[0].song_id).toBe("song-b");
    expect(out.usageEvents[0].variant_id).toBeNull();
    expect(shown.has("item-b")).toBe(true);
  });

  it("returning to an already-shown song does not re-log usage", () => {
    const seq = new LiveSequence();
    const shown = new Set<string>();
    // First show of item-a logs usage.
    bridgeOnCueChange(ctx(), cues, -1, 0, seq, 0, shown);
    expect(shown.has("item-a")).toBe(true);
    // Scrub away and back: cue.advanced fires, usage does not.
    const back = bridgeOnCueChange(ctx(), cues, 2, 0, seq, 0, shown);
    expect(back.usageEvents).toEqual([]);
  });

  it("a non-song cue emits cue.advanced only (no now_playing, no usage)", () => {
    const seq = new LiveSequence();
    const out = bridgeOnCueChange(ctx(), cues, 2, 3, seq, 0, new Set());
    expect(types(out.liveEvents)).toEqual(["cue.advanced"]);
    expect(out.usageEvents).toEqual([]);
    // The cue.advanced carries the (null) label + position faithfully, and a
    // cue with no service item id carries item_id null.
    const adv = out.liveEvents[0];
    if (adv.type === "cue.advanced") {
      expect(adv.item_position).toBe(3);
      expect(adv.item_id).toBeNull();
      expect(adv.label).toBeNull();
    }
  });

  it("an out-of-range index emits nothing (end of list)", () => {
    const seq = new LiveSequence();
    const out = bridgeOnCueChange(ctx(), cues, 3, 99, seq, 0, new Set());
    expect(out.liveEvents).toEqual([]);
    expect(out.usageEvents).toEqual([]);
  });

  it("carries the section label onto cue.advanced", () => {
    const seq = new LiveSequence();
    const out = bridgeOnCueChange(ctx(), cues, 0, 1, seq, 0, new Set());
    const adv = out.liveEvents[0];
    if (adv.type === "cue.advanced") expect(adv.label).toBe("Chorus");
  });
});

describe("bridgeOnEnd", () => {
  it("emits a single service.ended", () => {
    const seq = new LiveSequence();
    const out = bridgeOnEnd(ctx(), seq, 9000);
    expect(types(out.liveEvents)).toEqual(["service.ended"]);
    expect(out.usageEvents).toEqual([]);
    expect(out.liveEvents[0].sequence).toBe(1);
  });
});

describe("service-date variants", () => {
  it("normalises a unix-ms service date into the usage payload", () => {
    const seq = new LiveSequence();
    const out = bridgeOnGoLive(
      ctx({ serviceDate: 1718352000000 }),
      cues,
      0,
      seq,
      0,
      new Set(),
    );
    expect(out.usageEvents[0].service_date).toBe("2024-06-14");
  });
});
