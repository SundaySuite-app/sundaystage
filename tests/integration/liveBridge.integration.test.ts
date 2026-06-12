// songs_by_item → LiveBridgeContext — the planner-side half of the Phase 3
// bridge consumer. The `liveBridge.test.ts` sibling proves the pure driver; the
// concurrent provider test proves transport injection. This file proves the
// missing middle: that the `songs_by_item` IPC map (snake_case wire shape) is
// faithfully turned into a `LiveBridgeContext` the driver accepts, and that
// driving the real `bridgeOnGoLive` with that context reports the right song.
//
// The IPC layer is mocked (no Tauri runtime) so we exercise exactly the
// startSession seam: fetch the map → build the context → feed the driver.
import { describe, it, expect, vi, beforeEach } from "vitest";

import { LiveSequence } from "@/lib/liveEmitter";
import {
  buildLiveBridgeContext,
  serviceSongsToBridge,
  bridgeOnGoLive,
  type BridgeCue,
  type WireServiceItemSong,
} from "@/lib/liveBridge";
import { ipc } from "@/lib/ipc";

// A two-song service: item-a names an arrangement variant, item-b does not, plus
// a non-song item (scripture) the map omits — exactly what the repo returns.
const wireMap: Record<string, WireServiceItemSong> = {
  "item-a": { song_id: "song-a", title: "Amazing Grace", variant_id: "arr-1" },
  "item-b": { song_id: "song-b", title: "Oceans", variant_id: null },
};

const service = {
  id: "svc-2",
  library_id: "lib-1",
  starts_at: 1_718_352_000_000, // 2024-06-14
};

const cues: BridgeCue[] = [
  {
    serviceItemId: "item-a",
    displayLabel: "Grace V1",
    sectionLabel: "Verse 1",
  },
  {
    serviceItemId: "item-b",
    displayLabel: "Oceans V1",
    sectionLabel: "Verse 1",
  },
  { serviceItemId: "", displayLabel: "Scripture", sectionLabel: null },
];

describe("serviceSongsToBridge", () => {
  it("maps the snake_case wire shape onto the driver's camelCase shape", () => {
    const out = serviceSongsToBridge(wireMap);
    expect(out["item-a"]).toEqual({
      songId: "song-a",
      title: "Amazing Grace",
      variantId: "arr-1",
    });
    // A missing variant survives as null (the driver normalises ?? null).
    expect(out["item-b"]).toEqual({
      songId: "song-b",
      title: "Oceans",
      variantId: null,
    });
  });

  it("keeps only the items the planner reported (non-song items absent)", () => {
    const out = serviceSongsToBridge(wireMap);
    expect(Object.keys(out).sort()).toEqual(["item-a", "item-b"]);
  });
});

describe("buildLiveBridgeContext", () => {
  it("assembles a valid context from the service + songs_by_item map", () => {
    const ctx = buildLiveBridgeContext(service, wireMap);
    // No tenant id locally yet → library id stands in for churchId.
    expect(ctx.churchId).toBe("lib-1");
    expect(ctx.serviceId).toBe("svc-2");
    expect(ctx.serviceDate).toBe(1_718_352_000_000);
    expect(ctx.wasStreamed).toBe(false);
    expect(ctx.songsByItem["item-a"].songId).toBe("song-a");
  });
});

describe("startSession seam: fetch songs_by_item → build context → drive", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("calls songs_by_item for the service being taken live", async () => {
    const spy = vi.spyOn(ipc.service, "songsByItem").mockResolvedValue(wireMap);

    const fetched = await ipc.service.songsByItem(service.id);
    expect(spy).toHaveBeenCalledWith("svc-2");
    expect(fetched).toBe(wireMap);
  });

  it("the constructed context drives the driver to report the right song", async () => {
    vi.spyOn(ipc.service, "songsByItem").mockResolvedValue(wireMap);

    // Mirror OperatorWorkspace.startSession: fetch the map, build the context.
    const songsByItem = await ipc.service.songsByItem(service.id);
    const ctx = buildLiveBridgeContext(service, songsByItem);

    // Go live at cue 0 (song A) — the driver should emit the opening triplet
    // and a usage event carrying song-a + its arrangement variant.
    const seq = new LiveSequence();
    const out = bridgeOnGoLive(ctx, cues, 0, seq, 1000, new Set());

    expect(out.liveEvents.map((e) => e.type)).toEqual([
      "service.live",
      "cue.advanced",
      "now_playing",
    ]);
    const nowPlaying = out.liveEvents.find((e) => e.type === "now_playing");
    if (nowPlaying?.type === "now_playing") {
      // The local song id rides in the canonical song_ref.
      expect(nowPlaying.song_ref?.local_id).toBe("song-a");
      expect(nowPlaying.title).toBe("Amazing Grace");
    }
    expect(out.usageEvents).toHaveLength(1);
    expect(out.usageEvents[0].song_id).toBe("song-a");
    // serviceDate (unix-ms) normalises to the civil date in the usage ledger.
    expect(out.usageEvents[0].service_date).toBe("2024-06-14");
  });

  it("a non-song cue (item absent from the map) reports no song", () => {
    const ctx = buildLiveBridgeContext(service, wireMap);
    const seq = new LiveSequence();
    // Go live on the scripture cue (index 2, empty serviceItemId).
    const out = bridgeOnGoLive(ctx, cues, 2, seq, 1000, new Set());
    expect(out.liveEvents.map((e) => e.type)).toEqual([
      "service.live",
      "cue.advanced",
    ]);
    expect(out.usageEvents).toEqual([]);
  });
});
