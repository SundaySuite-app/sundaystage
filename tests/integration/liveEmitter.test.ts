// Stage → Rec live-cue bridge. Confirms the channel-name helper, the monotonic
// per-service sequence, and each LiveEvent builder produce the mirrored shapes.
// The publish step is a NETWORK-UNVERIFIED seam, exercised via an injected
// publisher to verify it routes to the right channel.
import { describe, it, expect, vi } from "vitest";

import {
  liveChannelName,
  LiveSequence,
  buildCueAdvanced,
  buildNowPlaying,
  buildServiceLive,
  buildServiceEnded,
  publishLiveEvent,
  type LiveEvent,
} from "@/lib/liveEmitter";

describe("liveChannelName", () => {
  it("formats church:{id}:service:{id}", () => {
    expect(liveChannelName("ch-1", "svc-2")).toBe("church:ch-1:service:svc-2");
  });

  it("requires both ids", () => {
    expect(() => liveChannelName("", "svc-2")).toThrow();
    expect(() => liveChannelName("ch-1", "")).toThrow();
  });
});

describe("LiveSequence", () => {
  it("starts at 0 and increases strictly", () => {
    const seq = new LiveSequence();
    expect(seq.current()).toBe(0);
    expect(seq.next()).toBe(1);
    expect(seq.next()).toBe(2);
    expect(seq.next()).toBe(3);
    expect(seq.current()).toBe(3);
  });
});

describe("live event builders", () => {
  const ids = { churchId: "ch-1", serviceId: "svc-2" };

  it("builds cue.advanced with progress and label", () => {
    expect(
      buildCueAdvanced({
        ...ids,
        seq: 5,
        at: 1000,
        index: 2,
        total: 9,
        sectionLabel: "Verse 1",
      }),
    ).toEqual({
      type: "cue.advanced",
      church_id: "ch-1",
      service_id: "svc-2",
      seq: 5,
      at: 1000,
      index: 2,
      total: 9,
      section_label: "Verse 1",
    });
  });

  it("defaults cue.advanced section label to null", () => {
    expect(
      buildCueAdvanced({ ...ids, seq: 1, at: 0, index: 0, total: 1 })
        .section_label,
    ).toBeNull();
  });

  it("builds now_playing with optional song/variant", () => {
    expect(
      buildNowPlaying({
        ...ids,
        seq: 6,
        at: 1100,
        title: "Amazing Grace",
        songId: "song-9",
      }),
    ).toEqual({
      type: "now_playing",
      church_id: "ch-1",
      service_id: "svc-2",
      seq: 6,
      at: 1100,
      song_id: "song-9",
      variant_id: null,
      title: "Amazing Grace",
    });
  });

  it("builds service.live with started_at", () => {
    expect(
      buildServiceLive({ ...ids, seq: 1, at: 500, startedAt: 499 }),
    ).toEqual({
      type: "service.live",
      church_id: "ch-1",
      service_id: "svc-2",
      seq: 1,
      at: 500,
      started_at: 499,
    });
  });

  it("builds service.ended", () => {
    expect(buildServiceEnded({ ...ids, seq: 99, at: 9000 })).toEqual({
      type: "service.ended",
      church_id: "ch-1",
      service_id: "svc-2",
      seq: 99,
      at: 9000,
    });
  });

  it("all builders require both ids", () => {
    expect(() =>
      buildServiceEnded({ churchId: "", serviceId: "s", seq: 1, at: 0 }),
    ).toThrow();
  });

  it("a session's events carry a strictly increasing sequence", () => {
    const seq = new LiveSequence();
    const at = 0;
    const events: LiveEvent[] = [
      buildServiceLive({ ...ids, seq: seq.next(), at, startedAt: at }),
      buildCueAdvanced({ ...ids, seq: seq.next(), at, index: 0, total: 2 }),
      buildNowPlaying({ ...ids, seq: seq.next(), at, title: "Song A" }),
      buildServiceEnded({ ...ids, seq: seq.next(), at }),
    ];
    expect(events.map((e) => e.seq)).toEqual([1, 2, 3, 4]);
  });
});

describe("publishLiveEvent (NETWORK-UNVERIFIED seam)", () => {
  it("routes the event to its per-service channel", async () => {
    const publish = vi.fn(async () => {});
    const event = buildServiceLive({
      churchId: "ch-1",
      serviceId: "svc-2",
      seq: 1,
      at: 0,
      startedAt: 0,
    });
    await publishLiveEvent(event, publish);
    expect(publish).toHaveBeenCalledWith("church:ch-1:service:svc-2", event);
  });
});
