/**
 * live-bridge-provider.utils — the non-component half of the live-bridge seam.
 *
 * The transport *construction* (`buildTransports`), the host config shape
 * (`LiveBridgeConfig`), the React context, and the reader hook
 * (`useLiveBridgeTransports`) live here so `live-bridge-provider.tsx` can export
 * *only* the `LiveBridgeProvider` component — that keeps React Fast Refresh
 * happy (a file mixing components with constants/functions breaks HMR).
 *
 * The same two hard rules from the provider apply:
 *  - The live output is sacrosanct: an absent field leaves that transport off,
 *    so the bridge runs but publishes nowhere — it can never crash a Sunday
 *    morning.
 *  - It must stay fully testable without a network. `buildTransports` is a pure
 *    function over injectable factories, so a test can hand in mock
 *    publishers/loggers that capture emissions while the real factories wrap a
 *    Supabase Realtime broadcast and a `postUsageEvent` POST (both
 *    NETWORK-UNVERIFIED — they have never run against a live backend here).
 */

import { createContext, useContext } from "react";

import type { LivePublisher } from "@/lib/liveEmitter";
import type { UsageClientConfig } from "@/lib/usageEmitter";
import type { LiveBridgeTransports } from "@/lib/useLiveBridge";

/**
 * Connection settings the host app feeds in (from env / settings / a Supabase
 * client). Everything is optional: an absent field leaves that transport off.
 */
export interface LiveBridgeConfig {
  /**
   * Realtime broadcaster (Stage → Rec). A function that puts one `LiveEvent`
   * on a channel. NETWORK-UNVERIFIED: the real one wraps a Supabase Realtime
   * `channel(name).send({ type: "broadcast", ... })`. Absent ⇒ cues not
   * published.
   */
  publish?: LivePublisher;
  /**
   * SundaySong usage API config (Stage → Song). Absent ⇒ usage not logged.
   * NETWORK-UNVERIFIED until it runs against a live SundaySong instance.
   */
  usage?: UsageClientConfig;
  /** Injectable clock — defaults to `Date.now` (overridable in tests). */
  now?: () => number;
}

/**
 * Pure: turn host config into the transports the hook consumes. Kept separate
 * from the React layer so it is unit-testable on its own. Fields stay
 * `undefined` (not present) when the host did not supply them, so the hook's
 * "omit ⇒ off" contract holds.
 */
export function buildTransports(
  config: LiveBridgeConfig,
): LiveBridgeTransports {
  const transports: LiveBridgeTransports = {};
  if (config.publish) transports.publish = config.publish;
  if (config.usage) transports.usage = config.usage;
  if (config.now) transports.now = config.now;
  return transports;
}

export const LiveBridgeContext = createContext<LiveBridgeTransports | null>(
  null,
);

/**
 * Read the injected transports. Returns `{}` (everything off) when no provider
 * is mounted, so callers never crash for lack of wiring — they just publish
 * nowhere, which is the safe default.
 */
export function useLiveBridgeTransports(): LiveBridgeTransports {
  return useContext(LiveBridgeContext) ?? {};
}
