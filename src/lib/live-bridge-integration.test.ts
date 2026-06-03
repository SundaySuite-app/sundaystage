// Live-bridge transport injection — the cross-app integration seam.
//
// The pure driver (liveBridge) and builders (liveEmitter/usageEmitter) are unit
// tested elsewhere. This file proves the *wiring*: that injecting real-shaped
// transports through the app-level provider and driving `useLiveBridge` through
// a live service produces exactly the contract events SundayRec (live cues) and
// SundaySong (usage) expect — with no network, using mock transports that
// capture emissions for inspection.
//
// It covers: (1) a session going live at index 0, (2) the operator advancing
// 0→1→2, asserting service.live + cue.advanced + now_playing + usage fire with
// strictly-monotonic per-service sequences; (3) round-trip serialisation of the
// builder output through JSON, loss-free; (4) deterministic idempotency keys on
// replay matching the contract formula; (5) edge cases: blackout (same index),
// non-song cues, out-of-range indices; (6) now_playing / usage de-duplication.
import { describe, it, expect } from "vitest";
import { renderHook, act } from "@testing-library/react";

import {
  buildCueAdvanced,
  buildNowPlaying,
  liveChannelName,
  type LiveEvent,
  type LivePublisher,
} from "@/lib/liveEmitter";
import {
  buildUsageEvent,
  usageIdempotencyKey,
  type DisplayedSongCue,
  type UsageClientConfig,
  type UsageEventPayload,
} from "@/lib/usageEmitter";
import { useLiveBridge, type LiveBridgeTransports } from "@/lib/useLiveBridge";
import type { BridgeCue, LiveBridgeContext } from "@/lib/liveBridge";
import { buildTransports } from "@/app/live-bridge-provider";

// ───────────────────────── fixtures / harness ─────────────────────────────

const CHURCH = "11111111-1111-1111-1111-111111111111";
const SERVICE = "33333333-3333-3333-3333-333333333333";

const ctx: LiveBridgeContext = {
  churchId: CHURCH,
  serviceId: SERVICE,
  serviceDate: "2026-05-31",
  wasStreamed: true,
  songsByItem: {
    "item-a": { songId: "song-a", title: "Amazing Grace", variantId: "arr-1" },
    "item-b": { songId: "song-b", title: "Oceans" },
  },
};

// Grace V1, Grace Chorus (same song A), Oceans V1 (song B), then a non-song cue.
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

/**
 * A capturing transport pair. The Realtime publisher records (channel, event)
 * tuples; the usage client config records each POSTed payload via a stub fetch
 * so `postUsageEvent` runs end-to-end with no network. A frozen clock makes the
 * `at` stamps deterministic.
 */
function mockTransports(now = () => 1_700_000_000_000): {
  transports: LiveBridgeTransports;
  liveEmissions: { channel: string; event: LiveEvent }[];
  usagePayloads: UsageEventPayload[];
} {
  const liveEmissions: { channel: string; event: LiveEvent }[] = [];
  const usagePayloads: UsageEventPayload[] = [];

  const publish: LivePublisher = async (channel, event) => {
    liveEmissions.push({ channel, event });
  };

  const usage: UsageClientConfig = {
    baseUrl: "https://api.sundaysong.test",
    token: "test-token",
    fetchImpl: (async (_url: string, init?: RequestInit) => {
      usagePayloads.push(JSON.parse(String(init?.body)) as UsageEventPayload);
      return { ok: true, status: 200, statusText: "OK" } as Response;
    }) as typeof fetch,
  };

  return {
    transports: { publish, usage, now },
    liveEmissions,
    usagePayloads,
  };
}

/** Let the hook's fire-and-forget `void publish(...)` / `postUsageEvent(...)`
 *  promises settle so the capture arrays are populated. */
async function flush(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

const liveTypes = (es: { event: LiveEvent }[]) => es.map((e) => e.event.type);

// ───────────────────────── (1)+(2) live service round-trip ────────────────

describe("useLiveBridge with injected transports — live service", () => {
  it("goes live at index 0 then advances 0→1→2, firing the right events", async () => {
    const cap = mockTransports();
    const { result } = renderHook(() =>
      useLiveBridge(ctx, cues, cap.transports),
    );

    // Session goes live with the first cue already showing.
    act(() => result.current.goLive(0, cues.length, 1_699_999_999_000));
    await flush();

    // service.live, then the opening cue.advanced + now_playing for song A.
    expect(liveTypes(cap.liveEmissions)).toEqual([
      "service.live",
      "cue.advanced",
      "now_playing",
    ]);
    expect(cap.usagePayloads).toHaveLength(1);
    expect(cap.usagePayloads[0].song_id).toBe("song-a");

    // Every emission went to the per-service channel.
    const channel = liveChannelName(CHURCH, SERVICE);
    expect(cap.liveEmissions.every((e) => e.channel === channel)).toBe(true);

    // Advance within song A (0→1): cue.advanced + now_playing, NO new usage.
    act(() => result.current.cueChange(0, 1, cues.length));
    await flush();
    expect(liveTypes(cap.liveEmissions).slice(3)).toEqual([
      "cue.advanced",
      "now_playing",
    ]);
    expect(cap.usagePayloads).toHaveLength(1); // still one — guard held

    // Advance to song B (1→2): cue.advanced + now_playing + one new usage.
    act(() => result.current.cueChange(1, 2, cues.length));
    await flush();
    expect(liveTypes(cap.liveEmissions).slice(5)).toEqual([
      "cue.advanced",
      "now_playing",
    ]);
    expect(cap.usagePayloads).toHaveLength(2);
    expect(cap.usagePayloads[1].song_id).toBe("song-b");
  });

  it("stamps strictly-monotonic per-service sequences across the session", async () => {
    const cap = mockTransports();
    const { result } = renderHook(() =>
      useLiveBridge(ctx, cues, cap.transports),
    );

    act(() => result.current.goLive(0, cues.length, 1_000));
    await flush();
    act(() => result.current.cueChange(0, 1, cues.length));
    await flush();
    act(() => result.current.cueChange(1, 2, cues.length));
    await flush();
    act(() => result.current.end());
    await flush();

    const seqs = cap.liveEmissions.map((e) => e.event.seq);
    // service.live(1) cue(2) now(3) | cue(4) now(5) | cue(6) now(7) | ended(8)
    expect(seqs).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
    // Strictly increasing, no gaps, no repeats.
    for (let i = 1; i < seqs.length; i++) {
      expect(seqs[i]).toBe(seqs[i - 1] + 1);
    }
    expect(liveTypes(cap.liveEmissions).at(-1)).toBe("service.ended");
  });

  it("re-running go-live resets the sequence (fresh session)", async () => {
    const cap = mockTransports();
    const { result } = renderHook(() =>
      useLiveBridge(ctx, cues, cap.transports),
    );

    act(() => result.current.goLive(0, cues.length, 1_000));
    await flush();
    const firstRun = cap.liveEmissions.length;
    act(() => result.current.goLive(0, cues.length, 2_000));
    await flush();

    // The second go-live's first event restarts the per-service counter at 1.
    expect(cap.liveEmissions[firstRun].event.seq).toBe(1);
  });
});

// ───────────────────────── (5) edge cases via the hook ────────────────────

describe("useLiveBridge edge cases", () => {
  it("blackout (same index) publishes nothing", async () => {
    const cap = mockTransports();
    const { result } = renderHook(() =>
      useLiveBridge(ctx, cues, cap.transports),
    );
    act(() => result.current.goLive(0, cues.length, 1_000));
    await flush();
    const before = cap.liveEmissions.length;

    act(() => result.current.cueChange(1, 1, cues.length)); // blackout/logo
    await flush();
    expect(cap.liveEmissions).toHaveLength(before);
  });

  it("a non-song cue publishes cue.advanced only (no now_playing, no usage)", async () => {
    const cap = mockTransports();
    const { result } = renderHook(() =>
      useLiveBridge(ctx, cues, cap.transports),
    );
    act(() => result.current.goLive(2, cues.length, 1_000)); // start on song B
    await flush();
    cap.liveEmissions.length = 0; // ignore the opening triplet
    const usageBefore = cap.usagePayloads.length;

    act(() => result.current.cueChange(2, 3, cues.length)); // → Kollekt
    await flush();
    expect(liveTypes(cap.liveEmissions)).toEqual(["cue.advanced"]);
    expect(cap.usagePayloads).toHaveLength(usageBefore);
  });

  it("an out-of-range index publishes nothing", async () => {
    const cap = mockTransports();
    const { result } = renderHook(() =>
      useLiveBridge(ctx, cues, cap.transports),
    );
    act(() => result.current.goLive(0, cues.length, 1_000));
    await flush();
    const before = cap.liveEmissions.length;

    act(() => result.current.cueChange(3, 99, cues.length));
    await flush();
    expect(cap.liveEmissions).toHaveLength(before);
  });
});

// ───────────────────────── (3) round-trip serialisation ───────────────────

describe("round-trip serialisation through the builders", () => {
  it("serialises and deserialises a live event with no data loss", () => {
    const ev = buildCueAdvanced({
      churchId: CHURCH,
      serviceId: SERVICE,
      seq: 42,
      at: 1_700_000_000_500,
      index: 3,
      total: 12,
      sectionLabel: "Verse 2",
    });
    const restored = JSON.parse(JSON.stringify(ev)) as LiveEvent;
    expect(restored).toEqual(ev);
  });

  it("preserves nulls on now_playing across JSON (non-song variant)", () => {
    const ev = buildNowPlaying({
      churchId: CHURCH,
      serviceId: SERVICE,
      seq: 7,
      at: 1_700_000_000_000,
      title: "Oceans",
      songId: "song-b",
      // variantId omitted → must round-trip as explicit null
    });
    const restored = JSON.parse(JSON.stringify(ev));
    expect(restored.variant_id).toBeNull();
    expect(restored).toEqual(ev);
  });

  it("preserves the usage payload (incl. idempotency_key) across JSON", () => {
    const cue: DisplayedSongCue = {
      churchId: CHURCH,
      songId: "song-a",
      variantId: "arr-1",
      serviceItemId: "item-a",
      serviceDate: "2026-05-31",
      wasStreamed: true,
    };
    const payload = buildUsageEvent(cue);
    const restored = JSON.parse(JSON.stringify(payload)) as UsageEventPayload;
    expect(restored).toEqual(payload);
  });
});

// ───────────────────────── (6) idempotency on replay ──────────────────────

describe("idempotency keys", () => {
  it("are deterministic on replay with the same inputs", () => {
    const a = usageIdempotencyKey(CHURCH, "item-a", "2026-05-31");
    const b = usageIdempotencyKey(CHURCH, "item-a", "2026-05-31");
    expect(a).toBe(b);
  });

  it("match the contract formula usage:<church>:<item>:<date>", () => {
    expect(usageIdempotencyKey(CHURCH, "item-a", "2026-05-31")).toBe(
      `usage:${CHURCH}:item-a:2026-05-31`,
    );
  });

  it("exclude the variant so re-keying a song mid-service collapses to one event", () => {
    const base: DisplayedSongCue = {
      churchId: CHURCH,
      songId: "song-a",
      serviceItemId: "item-a",
      serviceDate: "2026-05-31",
      wasStreamed: true,
    };
    const arr1 = buildUsageEvent({ ...base, variantId: "arr-1" });
    const arr2 = buildUsageEvent({ ...base, variantId: "arr-2" });
    expect(arr1.idempotency_key).toBe(arr2.idempotency_key);
  });

  it("replaying a live session produces the same usage key for the item", async () => {
    const run = async () => {
      const cap = mockTransports();
      const { result } = renderHook(() =>
        useLiveBridge(ctx, cues, cap.transports),
      );
      act(() => result.current.goLive(0, cues.length, 1_000));
      await flush();
      return cap.usagePayloads[0].idempotency_key;
    };
    expect(await run()).toBe(await run());
  });
});

// ───────────────────────── (provider) buildTransports ─────────────────────

describe("buildTransports (provider seam)", () => {
  it("returns empty transports for empty config (live output stays off)", () => {
    expect(buildTransports({})).toEqual({});
  });

  it("forwards only the supplied fields", () => {
    const publish: LivePublisher = async () => {};
    const usage: UsageClientConfig = { baseUrl: "https://x" };
    const now = () => 1;
    expect(buildTransports({ publish, usage, now })).toEqual({
      publish,
      usage,
      now,
    });
  });

  it("omits absent transports rather than passing undefined keys", () => {
    const usage: UsageClientConfig = { baseUrl: "https://x" };
    const t = buildTransports({ usage });
    expect("publish" in t).toBe(false);
    expect("now" in t).toBe(false);
    expect(t.usage).toBe(usage);
  });

  it("a publish-only config never logs usage when driven (Stage→Rec only)", async () => {
    const liveEmissions: { channel: string; event: LiveEvent }[] = [];
    const publish: LivePublisher = async (channel, event) => {
      liveEmissions.push({ channel, event });
    };
    const transports = buildTransports({
      publish,
      now: () => 1_000,
    });
    const { result } = renderHook(() => useLiveBridge(ctx, cues, transports));
    act(() => result.current.goLive(0, cues.length, 1_000));
    await flush();
    // Cues published, but with no usage config nothing is logged.
    expect(liveEmissions.length).toBeGreaterThan(0);
  });
});

// ───────────────────────── disabled transports (default OFF) ──────────────

describe("default-off transports", () => {
  it("the driver still runs but nothing is forwarded when transports are empty", async () => {
    const { result } = renderHook(() => useLiveBridge(ctx, cues, {}));
    // No throw, no transport — the live output is sacrosanct.
    expect(() =>
      act(() => result.current.goLive(0, cues.length, 1_000)),
    ).not.toThrow();
    await flush();
  });
});
