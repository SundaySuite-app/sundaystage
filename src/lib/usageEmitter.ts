/**
 * Stage → Song usage bridge (sender side).
 *
 * When a song cue is displayed on the live output, churches that report to
 * CCLI/TONO need a usage record. SundaySong owns the usage ledger; SundayStage
 * is the *source* of "this song was actually shown to a congregation on this
 * date". This module is the pure builder that turns a displayed-cue fact into
 * the wire payload, plus a thin POST seam to SundaySong's `/v1/usage/log`.
 *
 * The payload shape mirrors the platform `UsageEvent` contract:
 *   mirrors sunday-contracts; converge once published.
 *
 * Design rules:
 *  - The builder is *pure* and fully unit-tested (no clock, no network).
 *  - `idempotency_key` is stable *per service item*: replaying the same live
 *    session (e.g. a crash-recover, a re-broadcast, an operator scrubbing back
 *    to a slide) must NOT double-count usage. The key is derived only from
 *    inputs that identify the one performance: church + service + item + date.
 *    It deliberately excludes the variant/arrangement so re-keying a song mid
 *    service still collapses to one usage event for the item.
 */

/**
 * The displayed-cue fact, as SundayStage knows it. All fields come from the
 * service plan / live session — no I/O needed to build a payload.
 */
export interface DisplayedSongCue {
  /** Tenant / congregation id (UUID). */
  churchId: string;
  /** Canonical song id in SundaySong's catalog (UUID). */
  songId: string;
  /** Optional arrangement/translation variant actually shown. */
  variantId?: string | null;
  /** The service plan item id — the stable unit of one performance. */
  serviceItemId: string;
  /**
   * Service date. Accepts a `YYYY-MM-DD` string (already-local civil date) or
   * a unix-millis timestamp, which is converted to a UTC civil date. We avoid
   * `Date` formatting for the string case so the operator's local plan date is
   * preserved verbatim.
   */
  serviceDate: string | number;
  /** Whether this service was being streamed/recorded (SundayRec live). */
  wasStreamed: boolean;
}

/**
 * The wire payload for SundaySong `/v1/usage/log`.
 * mirrors sunday-contracts UsageEvent; converge once published.
 */
export interface UsageEventPayload {
  church_id: string;
  song_id: string;
  variant_id: string | null;
  /** Always normalised to `YYYY-MM-DD`. */
  service_date: string;
  was_streamed: boolean;
  /** Stable per service-item; safe to retry. */
  idempotency_key: string;
}

const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;

/**
 * Normalise a service date to a `YYYY-MM-DD` civil-date string.
 * - A matching `YYYY-MM-DD` string is returned as-is (operator's plan date).
 * - A unix-millis number is rendered as its UTC civil date.
 * Throws on anything else so a malformed plan can't silently corrupt the
 * ledger.
 */
export function normalizeServiceDate(input: string | number): string {
  if (typeof input === "number") {
    if (!Number.isFinite(input)) {
      throw new Error(`invalid service date timestamp: ${input}`);
    }
    return new Date(input).toISOString().slice(0, 10);
  }
  const trimmed = input.trim();
  if (!DATE_RE.test(trimmed)) {
    throw new Error(`service date must be YYYY-MM-DD, got: ${input}`);
  }
  return trimmed;
}

/**
 * Build the deterministic idempotency key for one displayed song item.
 *
 * Stable per (church, service-item, date). Identical inputs → identical key,
 * so the POST can be retried freely and a re-broadcast collapses to one event.
 * Format is human-debuggable rather than hashed; the server treats it opaquely.
 */
export function usageIdempotencyKey(
  churchId: string,
  serviceItemId: string,
  serviceDate: string,
): string {
  return `usage:${churchId}:${serviceItemId}:${serviceDate}`;
}

/**
 * Pure builder: displayed cue → usage-log payload. No clock, no network.
 */
export function buildUsageEvent(cue: DisplayedSongCue): UsageEventPayload {
  if (!cue.churchId) throw new Error("churchId is required");
  if (!cue.songId) throw new Error("songId is required");
  if (!cue.serviceItemId) throw new Error("serviceItemId is required");

  const service_date = normalizeServiceDate(cue.serviceDate);
  return {
    church_id: cue.churchId,
    song_id: cue.songId,
    variant_id: cue.variantId ?? null,
    service_date,
    was_streamed: cue.wasStreamed,
    idempotency_key: usageIdempotencyKey(
      cue.churchId,
      cue.serviceItemId,
      service_date,
    ),
  };
}

// ───────────────────────── network seam (NETWORK-UNVERIFIED) ──────────────

export interface UsageClientConfig {
  /** Base URL of the SundaySong API, e.g. `https://api.sundaysong.app`. */
  baseUrl: string;
  /** Bearer token / API key for the church tenant. */
  token?: string;
  /** Injectable fetch (defaults to global) — keeps the seam testable. */
  fetchImpl?: typeof fetch;
}

/**
 * POST a usage event to SundaySong `/v1/usage/log`.
 *
 * NETWORK-UNVERIFIED: compiles and is shaped against the (mirrored) contract,
 * but has never run against a live SundaySong instance — there is no network
 * in this environment. The `idempotency_key` makes retries safe. The pure
 * `buildUsageEvent` above is the fully-tested part; this is just plumbing.
 */
export async function postUsageEvent(
  payload: UsageEventPayload,
  config: UsageClientConfig,
): Promise<void> {
  const f = config.fetchImpl ?? fetch;
  const url = `${config.baseUrl.replace(/\/+$/, "")}/v1/usage/log`;
  const res = await f(url, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...(config.token ? { authorization: `Bearer ${config.token}` } : {}),
      // Echo the key as a header too; many idempotent APIs read it there.
      "idempotency-key": payload.idempotency_key,
    },
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    throw new Error(`usage log failed: ${res.status} ${res.statusText}`);
  }
}
