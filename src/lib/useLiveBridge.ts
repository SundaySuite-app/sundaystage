/**
 * useLiveBridge — wires the pure `liveBridge` driver into the operator's live
 * cue-advance path (Phase 3 bridge consumer).
 *
 * The hook owns the per-session mutable bits the pure driver must NOT own: the
 * monotonic `LiveSequence`, the previous index (to diff against), the
 * already-logged-usage guard, and the wall clock. On each transition it asks
 * the driver what to emit, then forwards the events to two injected transports.
 *
 * NETWORK-UNVERIFIED: both transports default to DISABLED no-ops. The live
 * publisher would wrap a Supabase Realtime broadcast and the usage poster would
 * `postUsageEvent` to SundaySong's `/v1/usage/log` — neither has run against a
 * live backend in this environment. Passing real transports turns the bridge
 * on; until then the driver still runs (and is fully tested) but its output
 * goes nowhere, so the live output stays sacrosanct (the bridge can never
 * crash a Sunday morning).
 */

import { useCallback, useRef } from "react";

import {
  LiveSequence,
  publishLiveEvent,
  type LivePublisher,
} from "./liveEmitter";
import { postUsageEvent, type UsageClientConfig } from "./usageEmitter";
import {
  bridgeOnCueChange,
  bridgeOnEnd,
  bridgeOnGoLive,
  type BridgeCue,
  type BridgeEmission,
  type LiveBridgeContext,
} from "./liveBridge";

/** Transports the bridge forwards to. Omit a field to leave that side off. */
export interface LiveBridgeTransports {
  /** Realtime broadcaster (Stage → Rec). Omitted ⇒ live cues not published. */
  publish?: LivePublisher;
  /** SundaySong usage API config (Stage → Song). Omitted ⇒ usage not logged. */
  usage?: UsageClientConfig;
  /** Injectable clock — defaults to `Date.now` (overridable in tests). */
  now?: () => number;
}

export interface LiveBridge {
  /** Call once when the session goes live (output armed at `index`). */
  goLive: (index: number, total: number, startedAt: number) => void;
  /** Call on every operator transition; diffs `prevIndex`→`nextIndex`. */
  cueChange: (prevIndex: number, nextIndex: number, total: number) => void;
  /** Call when the session ends. */
  end: () => void;
}

/**
 * @param ctx   Static per-session context (church/service/date/songs).
 * @param cues  The compiled bridge-cue list (parallel to the operator's cues).
 * @param transports  Injected, default-off NETWORK-UNVERIFIED seams.
 */
export function useLiveBridge(
  ctx: LiveBridgeContext | null,
  cues: readonly BridgeCue[],
  transports: LiveBridgeTransports = {},
): LiveBridge {
  // Per-session mutable state the pure driver must not hold.
  const seq = useRef(new LiveSequence());
  const shownItems = useRef(new Set<string>());
  const now = transports.now ?? Date.now;

  // Forward an emission to the (default-off) transports. Failures are swallowed
  // so the bridge can never take down the live output — the core promise.
  const forward = useCallback(
    (emission: BridgeEmission) => {
      const { publish, usage } = transports;
      if (publish) {
        for (const ev of emission.liveEvents) {
          // NETWORK-UNVERIFIED: best-effort Realtime broadcast.
          void publishLiveEvent(ev, publish).catch(() => {});
        }
      }
      if (usage) {
        for (const payload of emission.usageEvents) {
          // NETWORK-UNVERIFIED: best-effort POST to SundaySong.
          void postUsageEvent(payload, usage).catch(() => {});
        }
      }
    },
    [transports],
  );

  const goLive = useCallback(
    (index: number, total: number, startedAt: number) => {
      if (!ctx) return;
      seq.current = new LiveSequence();
      shownItems.current = new Set();
      forward(
        bridgeOnGoLive(
          ctx,
          cues,
          index,
          total,
          seq.current,
          now(),
          startedAt,
          shownItems.current,
        ),
      );
    },
    [ctx, cues, forward, now],
  );

  const cueChange = useCallback(
    (prevIndex: number, nextIndex: number, total: number) => {
      if (!ctx) return;
      forward(
        bridgeOnCueChange(
          ctx,
          cues,
          prevIndex,
          nextIndex,
          total,
          seq.current,
          now(),
          shownItems.current,
        ),
      );
    },
    [ctx, cues, forward, now],
  );

  const end = useCallback(() => {
    if (!ctx) return;
    forward(bridgeOnEnd(ctx, seq.current, now()));
  }, [ctx, forward, now]);

  return { goLive, cueChange, end };
}
