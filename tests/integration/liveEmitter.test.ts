// Stage → Rec live-cue bridge. Confirms the channel-name helper, the monotonic
// per-service sequence, and each LiveEvent builder produce the canonical
// contract shapes (sunday-contracts v0.4.0 LiveEvent — field-identical mirror).
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

const iso = (ms: number) => new Date(ms).toISOString();

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

describe("live event builders (canonical wire shapes)", () => {
  const ids = { serviceId: "svc-2" };

  it("builds cue.advanced with item/position/label", () => {
    expect(
      buildCueAdvanced({
        ...ids,
        seq: 5,
        at: 1000,
        itemId: "item-7",
        itemPosition: 2,
        label: "Verse 1",
        slideIndex: 0,
      }),
    ).toEqual({
      type: "cue.advanced",
      schema_version: 1,
      service_id: "svc-2",
      emitted_at: iso(1000),
      sequence: 5,
      item_id: "item-7",
      item_position: 2,
      label: "Verse 1",
      slide_index: 0,
    });
  });

  it("defaults cue.advanced optional fields to null", () => {
    const e = buildCueAdvanced({ ...ids, seq: 1, at: 0 });
    expect(e.item_id).toBeNull();
    expect(e.item_position).toBeNull();
    expect(e.label).toBeNull();
    expect(e.slide_index).toBeNull();
  });

  it("builds now_playing with a canonical song_ref", () => {
    expect(
      buildNowPlaying({
        ...ids,
        seq: 6,
        at: 1100,
        title: "Amazing Grace",
        songId: "song-9",
        itemPosition: 3,
      }),
    ).toEqual({
      type: "now_playing",
      schema_version: 1,
      service_id: "svc-2",
      emitted_at: iso(1100),
      sequence: 6,
      song_ref: {
        sundaysong_id: null,
        local_id: "song-9",
        title: "Amazing Grace",
        ccli_song_id: null,
        tono_work_id: null,
        default_key: null,
        language: "und",
      },
      item_position: 3,
      title: "Amazing Grace",
    });
  });

  it("builds now_playing without a song_ref when no songId is known", () => {
    const e = buildNowPlaying({ ...ids, seq: 6, at: 1100, title: "Preken" });
    expect(e.song_ref).toBeNull();
    expect(e.title).toBe("Preken");
  });

  it("builds service.live", () => {
    expect(buildServiceLive({ ...ids, seq: 1, at: 500 })).toEqual({
      type: "service.live",
      schema_version: 1,
      service_id: "svc-2",
      emitted_at: iso(500),
      sequence: 1,
    });
  });

  it("builds service.ended", () => {
    expect(buildServiceEnded({ ...ids, seq: 99, at: 9000 })).toEqual({
      type: "service.ended",
      schema_version: 1,
      service_id: "svc-2",
      emitted_at: iso(9000),
      sequence: 99,
    });
  });

  it("all builders require a service id", () => {
    expect(() => buildServiceEnded({ serviceId: "", seq: 1, at: 0 })).toThrow();
  });

  it("a session's events carry a strictly increasing sequence", () => {
    const seq = new LiveSequence();
    const at = 0;
    const events: LiveEvent[] = [
      buildServiceLive({ ...ids, seq: seq.next(), at }),
      buildCueAdvanced({ ...ids, seq: seq.next(), at, itemPosition: 0 }),
      buildNowPlaying({ ...ids, seq: seq.next(), at, title: "Song A" }),
      buildServiceEnded({ ...ids, seq: seq.next(), at }),
    ];
    expect(events.map((e) => e.sequence)).toEqual([1, 2, 3, 4]);
  });
});

describe("publishLiveEvent (NETWORK-UNVERIFIED seam)", () => {
  it("routes the event to its per-service channel", async () => {
    const publish = vi.fn(async () => {});
    const event = buildServiceLive({ serviceId: "svc-2", seq: 1, at: 0 });
    await publishLiveEvent("ch-1", event, publish);
    expect(publish).toHaveBeenCalledWith("church:ch-1:service:svc-2", event);
  });
});
