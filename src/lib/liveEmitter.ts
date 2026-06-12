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
 * FIELD-IDENTICAL mirror of the canonical contract:
 *   sunday-platform `@sunday/contracts` v0.4.0 — `src/live.ts` (`LiveEvent`,
 *   `CueAdvanced`, `NowPlaying`, `ServiceLive`, `ServiceEnded`), `src/song.ts`
 *   (`SongRef`) and `src/common.ts` (`liveChannel`). The platform package is
 *   not yet published, so the shapes are re-declared here; converge onto the
 *   real import once it is. Do not add or rename fields without changing the
 *   canonical contract first.
 */

/** Wire schema version every canonical payload carries. */
export const SCHEMA_VERSION = 1;

/** Realtime channel name: one channel per live service. Mirrors the canonical
 *  `liveChannel(churchId, serviceId)`. */
export function liveChannelName(churchId: string, serviceId: string): string {
  if (!churchId) throw new Error("churchId is required");
  if (!serviceId) throw new Error("serviceId is required");
  return `church:${churchId}:service:${serviceId}`;
}

// ───────────────────────── event shapes (canonical mirror) ────────────────

/**
 * A cross-app reference to a song. FIELD-IDENTICAL mirror of the canonical
 * `SongRef` (@sunday/contracts v0.4.0, src/song.ts). Stage's local song row id
 * goes in `local_id`; `sundaysong_id` stays null until the library links to
 * the shared catalog.
 */
export interface SongRef {
  /** Canonical SundaySong catalog id, or null if not yet linked. */
  sundaysong_id: string | null;
  /** The originating app's own row id (Stage-local song), or null. */
  local_id: string | null;
  title: string;
  ccli_song_id: string | null;
  tono_work_id: string | null;
  default_key: string | null;
  /** BCP-47 / ISO-639 language code; "und" when undetermined. */
  language: string;
}

/** Discriminated union tags. Mirrors the canonical `LiveEvent` union. */
export type LiveEventType =
  | "cue.advanced"
  | "now_playing"
  | "service.live"
  | "service.ended";

/** Common envelope on every live signal (canonical `liveBase`). */
interface LiveEventBase {
  type: LiveEventType;
  schema_version: number;
  service_id: string;
  /** ISO 8601 UTC emit time. */
  emitted_at: string;
  /** Monotonic per-service counter; strictly increasing, gap = missed event. */
  sequence: number;
}

/** The operator advanced/changed the active cue (canonical `CueAdvanced`). */
export interface CueAdvancedEvent extends LiveEventBase {
  type: "cue.advanced";
  /** The service item under the cursor, when known. */
  item_id: string | null;
  /** Zero-based position of the item in the running order. */
  item_position: number | null;
  /** Localised/humanised section label currently shown, if any. */
  label: string | null;
  /** Zero-based slide index within the item, when known. */
  slide_index: number | null;
}

/** A song became the active item (canonical `NowPlaying`). */
export interface NowPlayingEvent extends LiveEventBase {
  type: "now_playing";
  song_ref: SongRef | null;
  item_position: number | null;
  title: string | null;
}

/** The service went live (canonical `ServiceLive`). */
export interface ServiceLiveEvent extends LiveEventBase {
  type: "service.live";
}

/** The service ended (canonical `ServiceEnded`). */
export interface ServiceEndedEvent extends LiveEventBase {
  type: "service.ended";
}

export type LiveEvent =
  | CueAdvancedEvent
  | NowPlayingEvent
  | ServiceLiveEvent
  | ServiceEndedEvent;

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
): Omit<LiveEventBase, "type"> {
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
}): CueAdvancedEvent {
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
}): NowPlayingEvent {
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
}): ServiceLiveEvent {
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
}): ServiceEndedEvent {
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
