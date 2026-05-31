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
 * The event shapes mirror the platform `LiveEvent` contract:
 *   mirrors sunday-contracts; converge once published.
 */

/** Realtime channel name: one channel per live service. */
export function liveChannelName(churchId: string, serviceId: string): string {
  if (!churchId) throw new Error("churchId is required");
  if (!serviceId) throw new Error("serviceId is required");
  return `church:${churchId}:service:${serviceId}`;
}

// ───────────────────────── event shapes (mirrors contract) ────────────────

/** Discriminated union of live events. mirrors sunday-contracts LiveEvent. */
export type LiveEventType =
  | "cue.advanced"
  | "now_playing"
  | "service.live"
  | "service.ended";

interface LiveEventBase {
  type: LiveEventType;
  church_id: string;
  service_id: string;
  /** Monotonic per-service counter; strictly increasing, gap = missed event. */
  seq: number;
  /** When the event was minted (unix ms). */
  at: number;
}

/** The operator advanced/changed the active cue. */
export interface CueAdvancedEvent extends LiveEventBase {
  type: "cue.advanced";
  /** Zero-based index into the service plan. */
  index: number;
  /** Total items in the service plan (for progress UI). */
  total: number;
  /** Localised/humanised section label currently shown, if any. */
  section_label: string | null;
}

/** A song became the active item (subset of cue.advanced for Rec chapters). */
export interface NowPlayingEvent extends LiveEventBase {
  type: "now_playing";
  song_id: string | null;
  variant_id: string | null;
  title: string;
}

/** The service went live (output armed). */
export interface ServiceLiveEvent extends LiveEventBase {
  type: "service.live";
  /** When the session went live (unix ms) — mirrors LiveSessionView.started_at. */
  started_at: number;
}

/** The service ended (output closed). */
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
 * Common envelope assembly. `seq` and `at` are passed in (the caller owns the
 * `LiveSequence` and the clock) so every builder is a pure function of inputs.
 */
function requireIds(churchId: string, serviceId: string): void {
  if (!churchId) throw new Error("churchId is required");
  if (!serviceId) throw new Error("serviceId is required");
}

export function buildCueAdvanced(args: {
  churchId: string;
  serviceId: string;
  seq: number;
  at: number;
  index: number;
  total: number;
  sectionLabel?: string | null;
}): CueAdvancedEvent {
  requireIds(args.churchId, args.serviceId);
  return {
    type: "cue.advanced",
    church_id: args.churchId,
    service_id: args.serviceId,
    seq: args.seq,
    at: args.at,
    index: args.index,
    total: args.total,
    section_label: args.sectionLabel ?? null,
  };
}

export function buildNowPlaying(args: {
  churchId: string;
  serviceId: string;
  seq: number;
  at: number;
  title: string;
  songId?: string | null;
  variantId?: string | null;
}): NowPlayingEvent {
  requireIds(args.churchId, args.serviceId);
  return {
    type: "now_playing",
    church_id: args.churchId,
    service_id: args.serviceId,
    seq: args.seq,
    at: args.at,
    song_id: args.songId ?? null,
    variant_id: args.variantId ?? null,
    title: args.title,
  };
}

export function buildServiceLive(args: {
  churchId: string;
  serviceId: string;
  seq: number;
  at: number;
  startedAt: number;
}): ServiceLiveEvent {
  requireIds(args.churchId, args.serviceId);
  return {
    type: "service.live",
    church_id: args.churchId,
    service_id: args.serviceId,
    seq: args.seq,
    at: args.at,
    started_at: args.startedAt,
  };
}

export function buildServiceEnded(args: {
  churchId: string;
  serviceId: string;
  seq: number;
  at: number;
}): ServiceEndedEvent {
  requireIds(args.churchId, args.serviceId);
  return {
    type: "service.ended",
    church_id: args.churchId,
    service_id: args.serviceId,
    seq: args.seq,
    at: args.at,
  };
}

// ───────────────────────── publish seam (NETWORK-UNVERIFIED) ───────────────

/** A function that broadcasts one event on a channel (injected publisher). */
export type LivePublisher = (
  channel: string,
  event: LiveEvent,
) => Promise<void>;

/**
 * Publish a live event on the per-service channel.
 *
 * NETWORK-UNVERIFIED: the `publish` function is expected to wrap a Supabase
 * Realtime broadcast (or equivalent). It is injected so the pure builders and
 * the channel-name derivation are fully testable; the real transport has never
 * run against a live Realtime backend in this environment.
 */
export async function publishLiveEvent(
  event: LiveEvent,
  publish: LivePublisher,
): Promise<void> {
  const channel = liveChannelName(event.church_id, event.service_id);
  await publish(channel, event);
}
