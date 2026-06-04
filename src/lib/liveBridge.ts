/**
 * Live-session → bridge driver (Phase 3 bridge consumer).
 *
 * The emitters in `liveEmitter.ts` (Stage → Rec live cues) and
 * `usageEmitter.ts` (Stage → Song usage) are pure *builders*. They had no
 * callers. This module is the missing caller: a **pure** state-diff that turns
 * a live-session transition (the operator advancing/jumping/blacking-out, or
 * the service going live / ending) into the exact set of events to publish.
 *
 * It is deliberately pure — no clock, no network, no React. The caller owns:
 *   - the `LiveSequence` (one per live session),
 *   - the wall clock (`at`, passed in),
 *   - the transports (the NETWORK-UNVERIFIED publish/POST seams in the emitters).
 *
 * So this whole decision layer is unit-testable; the I/O is a thin hook
 * (`useLiveBridge`) that feeds it real inputs and forwards the result to the
 * injected transports.
 *
 * Design choices:
 *  - A `cue.advanced` live event fires on every index change (and on go-live).
 *  - A `now_playing` live event fires only when the *song* under the cursor
 *    changes, so SundayRec chapters don't churn slide-by-slide within one song.
 *  - A usage event fires at most ONCE per service item per session: the first
 *    time a song item is actually shown. `usageEmitter`'s idempotency key makes
 *    a retry / re-broadcast safe even if we somehow emit twice.
 *  - Blackout/logo do not change the active index, so they emit nothing here
 *    (the Rec lower-third simply keeps the last cue, by design).
 */

import {
  buildCueAdvanced,
  buildNowPlaying,
  buildServiceEnded,
  buildServiceLive,
  type LiveEvent,
  type LiveSequence,
} from "./liveEmitter";
import {
  buildUsageEvent,
  type UsageEventPayload,
  type DisplayedSongCue,
} from "./usageEmitter";

/** Minimal cue shape the driver needs — a subset of the generated `Cue`. */
export interface BridgeCue {
  /** `service_item_id` + display label, mirrored from `Cue.source`. */
  serviceItemId: string;
  /** Humanised "you are here" label, e.g. "Amazing Grace — Verse 2". */
  displayLabel: string;
  /** Section label of the shown slide, if any (e.g. "Verse 1"). */
  sectionLabel: string | null;
}

/** Per-service-item song metadata the cue itself doesn't carry. */
export interface ServiceItemSong {
  /** Canonical SundaySong catalog id (UUID). */
  songId: string;
  /** Display title (for the `now_playing` event). */
  title: string;
  /** Optional arrangement/translation variant shown. */
  variantId?: string | null;
}

/** Static config for one live session — fixed at "Go Live". */
export interface LiveBridgeContext {
  churchId: string;
  serviceId: string;
  /** Civil date or unix-ms of the service (for the usage ledger). */
  serviceDate: string | number;
  /** Whether SundayRec is streaming/recording this service. */
  wasStreamed: boolean;
  /** serviceItemId → song metadata. Items absent here are non-song cues. */
  songsByItem: Readonly<Record<string, ServiceItemSong>>;
}

/**
 * The `songs_by_item` IPC wire shape (the ts-rs `ServiceItemSong` binding,
 * snake_case). Kept local so the bridge module owns the snake→camel mapping in
 * one place rather than scattering it across the React layer.
 */
export interface WireServiceItemSong {
  song_id: string;
  title: string;
  variant_id: string | null;
}

/** Map the `songs_by_item` IPC record onto the driver's camelCase shape. */
export function serviceSongsToBridge(
  rec: Readonly<Record<string, WireServiceItemSong>>,
): Record<string, ServiceItemSong> {
  const out: Record<string, ServiceItemSong> = {};
  for (const [itemId, s] of Object.entries(rec)) {
    out[itemId] = {
      songId: s.song_id,
      title: s.title,
      variantId: s.variant_id,
    };
  }
  return out;
}

/**
 * Assemble the per-session `LiveBridgeContext` from the service being taken
 * live plus its `songs_by_item` map. Pure: the caller owns the IPC fetch.
 *
 * There is no church/tenant id locally yet, so we fall back to the library id
 * (`churchId`); `serviceDate` is the service start (unix-ms), and `wasStreamed`
 * defaults false until SundayRec reports it back over the bridge.
 */
export function buildLiveBridgeContext(
  service: { id: string; library_id: string; starts_at: number },
  songsByItem: Readonly<Record<string, WireServiceItemSong>>,
): LiveBridgeContext {
  return {
    churchId: service.library_id,
    serviceId: service.id,
    serviceDate: service.starts_at,
    wasStreamed: false,
    songsByItem: serviceSongsToBridge(songsByItem),
  };
}

/** Everything the driver decided to emit for one transition. */
export interface BridgeEmission {
  liveEvents: LiveEvent[];
  usageEvents: UsageEventPayload[];
}

function emptyEmission(): BridgeEmission {
  return { liveEvents: [], usageEvents: [] };
}

/**
 * The service went live: emit `service.live` and (because the first cue is
 * already showing) the opening `cue.advanced` / `now_playing` / usage for the
 * cue at `index`.
 */
export function bridgeOnGoLive(
  ctx: LiveBridgeContext,
  cues: readonly BridgeCue[],
  index: number,
  total: number,
  seq: LiveSequence,
  at: number,
  startedAt: number,
  shownItems: Set<string>,
): BridgeEmission {
  const out = emptyEmission();
  out.liveEvents.push(
    buildServiceLive({
      churchId: ctx.churchId,
      serviceId: ctx.serviceId,
      seq: seq.next(),
      at,
      startedAt,
    }),
  );
  // Go-live: there is no previous cue, so the opening cue is always a song
  // change (emit the opening now_playing).
  appendCueChange(out, ctx, cues, null, index, total, seq, at, shownItems);
  return out;
}

/**
 * The active cue moved (index changed). Emits `cue.advanced`, a `now_playing`
 * when the underlying song changed, and a one-shot usage event the first time
 * a song item is shown. A same-index transition (blackout/logo) emits nothing.
 */
export function bridgeOnCueChange(
  ctx: LiveBridgeContext,
  cues: readonly BridgeCue[],
  prevIndex: number,
  nextIndex: number,
  total: number,
  seq: LiveSequence,
  at: number,
  shownItems: Set<string>,
): BridgeEmission {
  const out = emptyEmission();
  if (nextIndex === prevIndex) return out; // no movement → nothing to publish
  appendCueChange(
    out,
    ctx,
    cues,
    prevIndex,
    nextIndex,
    total,
    seq,
    at,
    shownItems,
  );
  return out;
}

/** The service ended (output closed). */
export function bridgeOnEnd(
  ctx: LiveBridgeContext,
  seq: LiveSequence,
  at: number,
): BridgeEmission {
  return {
    liveEvents: [
      buildServiceEnded({
        churchId: ctx.churchId,
        serviceId: ctx.serviceId,
        seq: seq.next(),
        at,
      }),
    ],
    usageEvents: [],
  };
}

/**
 * Shared core: assemble the cue.advanced / now_playing / usage triplet for the
 * cue now under the cursor. `shownItems` is mutated to record items we've
 * already logged usage for this session (the one-shot guard).
 */
function appendCueChange(
  out: BridgeEmission,
  ctx: LiveBridgeContext,
  cues: readonly BridgeCue[],
  prevIndex: number | null,
  index: number,
  total: number,
  seq: LiveSequence,
  at: number,
  shownItems: Set<string>,
): void {
  const cue = cues[index];
  if (!cue) return; // index out of range (empty list / end) → nothing

  out.liveEvents.push(
    buildCueAdvanced({
      churchId: ctx.churchId,
      serviceId: ctx.serviceId,
      seq: seq.next(),
      at,
      index,
      total,
      sectionLabel: cue.sectionLabel,
    }),
  );

  const song = ctx.songsByItem[cue.serviceItemId];
  if (!song) return; // non-song cue (scripture/gap/blackout): no song bridge

  // `now_playing` fires only when the *song* under the cursor changes — never
  // slide-by-slide within one song — so SundayRec chapters don't churn. A
  // null prevIndex (go-live) is always a change; a previous non-song cue has
  // no song id and so also counts as a change.
  const prevCue = prevIndex === null ? undefined : cues[prevIndex];
  const prevSongId = prevCue
    ? (ctx.songsByItem[prevCue.serviceItemId]?.songId ?? null)
    : null;
  if (prevSongId !== song.songId) {
    out.liveEvents.push(
      buildNowPlaying({
        churchId: ctx.churchId,
        serviceId: ctx.serviceId,
        seq: seq.next(),
        at,
        title: song.title,
        songId: song.songId,
        variantId: song.variantId ?? null,
      }),
    );
  }

  // Log usage once per service item per session (idempotency-keyed anyway).
  if (!shownItems.has(cue.serviceItemId)) {
    shownItems.add(cue.serviceItemId);
    const displayed: DisplayedSongCue = {
      churchId: ctx.churchId,
      songId: song.songId,
      variantId: song.variantId ?? null,
      serviceItemId: cue.serviceItemId,
      serviceDate: ctx.serviceDate,
      wasStreamed: ctx.wasStreamed,
    };
    out.usageEvents.push(buildUsageEvent(displayed));
  }
}
