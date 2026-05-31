// Stage → Song usage bridge. Confirms the pure builder mirrors the UsageEvent
// contract, the idempotency key is stable per service-item (replay-safe), and
// service-date normalisation handles both plan strings and unix-millis. The
// POST seam is NETWORK-UNVERIFIED and only exercised against an injected fetch.
import { describe, it, expect, vi } from "vitest";

import {
  buildUsageEvent,
  normalizeServiceDate,
  usageIdempotencyKey,
  postUsageEvent,
  type DisplayedSongCue,
} from "@/lib/usageEmitter";

const baseCue: DisplayedSongCue = {
  churchId: "ch-1",
  songId: "song-9",
  variantId: "var-3",
  serviceItemId: "item-7",
  serviceDate: "2026-05-31",
  wasStreamed: true,
};

describe("normalizeServiceDate", () => {
  it("passes through a YYYY-MM-DD string verbatim", () => {
    expect(normalizeServiceDate("2026-05-31")).toBe("2026-05-31");
    expect(normalizeServiceDate("  2026-01-04  ")).toBe("2026-01-04");
  });

  it("renders a unix-millis timestamp as its UTC civil date", () => {
    const ms = Date.UTC(2026, 4, 31, 9, 30); // 2026-05-31T09:30Z
    expect(normalizeServiceDate(ms)).toBe("2026-05-31");
  });

  it("throws on a malformed string", () => {
    expect(() => normalizeServiceDate("31/05/2026")).toThrow();
    expect(() => normalizeServiceDate("2026-5-31")).toThrow();
  });

  it("throws on a non-finite timestamp", () => {
    expect(() => normalizeServiceDate(NaN)).toThrow();
  });
});

describe("usageIdempotencyKey", () => {
  it("is deterministic for identical inputs", () => {
    expect(usageIdempotencyKey("ch-1", "item-7", "2026-05-31")).toBe(
      usageIdempotencyKey("ch-1", "item-7", "2026-05-31"),
    );
  });

  it("differs across church / item / date", () => {
    const a = usageIdempotencyKey("ch-1", "item-7", "2026-05-31");
    expect(a).not.toBe(usageIdempotencyKey("ch-2", "item-7", "2026-05-31"));
    expect(a).not.toBe(usageIdempotencyKey("ch-1", "item-8", "2026-05-31"));
    expect(a).not.toBe(usageIdempotencyKey("ch-1", "item-7", "2026-06-01"));
  });
});

describe("buildUsageEvent", () => {
  it("mirrors the UsageEvent contract field-for-field", () => {
    expect(buildUsageEvent(baseCue)).toEqual({
      church_id: "ch-1",
      song_id: "song-9",
      variant_id: "var-3",
      service_date: "2026-05-31",
      was_streamed: true,
      idempotency_key: "usage:ch-1:item-7:2026-05-31",
    });
  });

  it("defaults a missing variant to null", () => {
    const { variantId: _omit, ...rest } = baseCue;
    expect(buildUsageEvent(rest).variant_id).toBeNull();
  });

  it("is replay-stable: two displays of the same item collapse to one key", () => {
    const first = buildUsageEvent(baseCue);
    const replay = buildUsageEvent({ ...baseCue, variantId: "var-OTHER" });
    // Variant change must NOT change the key — it's still one performance.
    expect(replay.idempotency_key).toBe(first.idempotency_key);
  });

  it("normalises a timestamp service date into the key", () => {
    const ev = buildUsageEvent({
      ...baseCue,
      serviceDate: Date.UTC(2026, 4, 31),
    });
    expect(ev.service_date).toBe("2026-05-31");
    expect(ev.idempotency_key).toBe("usage:ch-1:item-7:2026-05-31");
  });

  it("rejects missing required ids", () => {
    expect(() => buildUsageEvent({ ...baseCue, churchId: "" })).toThrow();
    expect(() => buildUsageEvent({ ...baseCue, songId: "" })).toThrow();
    expect(() => buildUsageEvent({ ...baseCue, serviceItemId: "" })).toThrow();
  });
});

describe("postUsageEvent (NETWORK-UNVERIFIED seam)", () => {
  it("POSTs to /v1/usage/log with the key as header and body", async () => {
    const fetchMock = vi.fn(async () => new Response(null, { status: 200 }));
    const payload = buildUsageEvent(baseCue);

    await postUsageEvent(payload, {
      baseUrl: "https://api.sundaysong.app/",
      token: "tok",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("https://api.sundaysong.app/v1/usage/log");
    expect(init.method).toBe("POST");
    const headers = init.headers as Record<string, string>;
    expect(headers.authorization).toBe("Bearer tok");
    expect(headers["idempotency-key"]).toBe(payload.idempotency_key);
    expect(JSON.parse(init.body as string)).toEqual(payload);
  });

  it("throws on a non-OK response", async () => {
    const fetchImpl = vi.fn(
      async () => new Response(null, { status: 500, statusText: "boom" }),
    ) as unknown as typeof fetch;
    await expect(
      postUsageEvent(buildUsageEvent(baseCue), {
        baseUrl: "https://api.sundaysong.app",
        fetchImpl,
      }),
    ).rejects.toThrow(/500/);
  });
});
