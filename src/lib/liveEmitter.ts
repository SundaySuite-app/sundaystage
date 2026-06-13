/**
 * Stage → Rec live-cue bridge (sender side).
 *
 * SundayRec (and any other live consumer) subscribes to "what is on the stage
 * right now" so its lower-thirds / recording chapters / stream overlay stay in
 * sync with the congregation's screen. SundayStage publishes those facts over
 * a Realtime channel.
 *
 * This module is the *pure* shape layer: it builds the `LiveEvent` payloads
 * and the channel-name helper, and stamps a monotonic per-service sequence so
 * a subscriber can detect gaps / reordering. The actual publish (Supabase
 * Realtime broadcast) is a seam marked NETWORK-UNVERIFIED below.
 *
 * The wire shapes (`LiveEvent` and its members `CueAdvanced`/`NowPlaying`/
 * `ServiceLive`/`ServiceEnded`, plus `SongRef` and `SCHEMA_VERSION`) are now
 * imported from the canonical `@sunday/contracts` package — this module no
 * longer re-declares a field-identical mirror. Add or rename a wire field by
 * changing the contract, not here.
 *
 * The builders below intentionally keep their existing hand-written bodies
 * (rather than calling the contract's Zod `liveCueEvent`/`nowPlayingEvent`
 * builders) so behaviour is unchanged: the canonical Zod builders validate
 * `service_id` as a UUID and would reject Stage's non-UUID service ids, and
 * there are no canonical builders for the bare `service.live`/`service.ended`
 * envelopes. The return types are the canonical contract types, so any future
 * field drift in the contract surfaces here as a typecheck error.
 */

import {
  SCHEMA_VERSION,
  type SongRef,
  type CueAdvanced,
  type NowPlaying,
  type ServiceLive,
  type ServiceEnded,
  type LiveEvent,
} from "@sunday/contracts";

export {
  SCHEMA_VERSION,
  type SongRef,
  type CueAdvanced,
  type NowPlaying,
  type ServiceLive,
  type ServiceEnded,
  type LiveEvent,
};

/** Realtime channel name: one channel per live service. Stage-local helper that
 *  produces the same string as the canonical `liveChannel(churchId, serviceId)`
 *  but additionally rejects empty ids — the operator UI must never broadcast on
 *  a half-formed channel. */
export function liveChannelName(churchId: string, serviceId: string): string {
  if (!churchId) throw new Error("churchId is required");
  if (!serviceId) throw new Error("serviceId is required");
  return `church:${churchId}:service:${serviceId}`;
}

/** Common envelope fields shared by every live signal (canonical `liveBase`). */
type LiveEventEnvelope = Pick<
  LiveEvent,
  "type" | "schema_version" | "service_id" | "emitted_at" | "sequence"
>;

// ───────────────────────── monotonic sequence ─────────────────────────────

/**
 * A monotonic per-service sequence counter. Each `next()` returns a strictly
 * increasing integer starting at 1, so subscribers can detect dropped or
 * reordered broadcasts. One instance lives per live session.
 */
export class LiveSequence {
  private value = 0;
  next(): number {
    this.value += 1;
    return this.value;
  }
  /** Current value without advancing (0 before the first event). */
  current(): number {
    return this.value;
  }
}

// ───────────────────────── pure builders ──────────────────────────────────

/**
 * Common envelope assembly. `seq` and `at` (unix ms, converted to the
 * canonical ISO-8601 `emitted_at`) are passed in — the caller owns the
 * `LiveSequence` and the clock — so every builder is a pure function of
 * inputs.
 */
function baseEnvelope(
  serviceId: string,
  seq: number,
  at: number,
): Omit<LiveEventEnvelope, "type"> {
  if (!serviceId) throw new Error("serviceId is required");
  return {
    schema_version: SCHEMA_VERSION,
    service_id: serviceId,
    emitted_at: new Date(at).toISOString(),
    sequence: seq,
  };
}

export function buildCueAdvanced(args: {
  serviceId: string;
  seq: number;
  /** Emit time, unix ms (becomes the ISO `emitted_at`). */
  at: number;
  itemId?: string | null;
  itemPosition?: number | null;
  label?: string | null;
  slideIndex?: number | null;
}): CueAdvanced {
  return {
    type: "cue.advanced",
    ...baseEnvelope(args.serviceId, args.seq, args.at),
    item_id: args.itemId ?? null,
    item_position: args.itemPosition ?? null,
    label: args.label ?? null,
    slide_index: args.slideIndex ?? null,
  };
}

export function buildNowPlaying(args: {
  serviceId: string;
  seq: number;
  /** Emit time, unix ms (becomes the ISO `emitted_at`). */
  at: number;
  title: string;
  /** Stage-local song row id — lands in `song_ref.local_id`. */
  songId?: string | null;
  itemPosition?: number | null;
}): NowPlaying {
  const songRef: SongRef | null = args.songId
    ? {
        sundaysong_id: null,
        local_id: args.songId,
        title: args.title,
        ccli_song_id: null,
        tono_work_id: null,
        default_key: null,
        language: "und",
      }
    : null;
  return {
    type: "now_playing",
    ...baseEnvelope(args.serviceId, args.seq, args.at),
    song_ref: songRef,
    item_position: args.itemPosition ?? null,
    title: args.title,
  };
}

export function buildServiceLive(args: {
  serviceId: string;
  seq: number;
  /** Emit time, unix ms (becomes the ISO `emitted_at`). */
  at: number;
}): ServiceLive {
  return {
    type: "service.live",
    ...baseEnvelope(args.serviceId, args.seq, args.at),
  };
}

export function buildServiceEnded(args: {
  serviceId: string;
  seq: number;
  /** Emit time, unix ms (becomes the ISO `emitted_at`). */
  at: number;
}): ServiceEnded {
  return {
    type: "service.ended",
    ...baseEnvelope(args.serviceId, args.seq, args.at),
  };
}

// ───────────────────────── publish seam (NETWORK-UNVERIFIED) ───────────────

/** A function that broadcasts one event on a channel (injected publisher). */
export type LivePublisher = (
  channel: string,
  event: LiveEvent,
) => Promise<void>;

/**
 * Publish a live event on the per-service channel. The canonical event no
 * longer carries `church_id` (the channel itself is the tenant scope), so the
 * caller supplies it for the channel-name derivation.
 *
 * NETWORK-UNVERIFIED: the `publish` function is expected to wrap a Supabase
 * Realtime broadcast (or equivalent). It is injected so the pure builders and
 * the channel-name derivation are fully testable; the real transport has never
 * run against a live Realtime backend in this environment.
 */
export async function publishLiveEvent(
  churchId: string,
  event: LiveEvent,
  publish: LivePublisher,
): Promise<void> {
  const channel = liveChannelName(churchId, event.service_id);
  await publish(channel, event);
}
