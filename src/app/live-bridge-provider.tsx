/**
 * live-bridge-provider — the app-level seam that constructs the real
 * Stage → Rec / Stage → Song transports and injects them into the operator
 * console (`LivePreview` → `useLiveBridge`).
 *
 * The pure driver (`liveBridge.ts`) and the builders (`liveEmitter.ts`,
 * `usageEmitter.ts`) had no caller that supplied real transports; the hook
 * defaulted both sides OFF so nothing was ever published. This provider is the
 * missing wiring: it owns the *construction* of the transports (a Realtime
 * broadcaster for live cues + a SundaySong usage-API config) so the rest of the
 * tree just reads them out of context.
 *
 * Two hard rules shape this file:
 *  - The live output is sacrosanct: if no config is supplied the transports stay
 *    empty (`{}`), so the bridge runs but publishes nowhere — it can never crash
 *    a Sunday morning.
 *  - It must stay fully testable without a network. The transports are built by
 *    *injectable factories* (`buildTransports`), so a test can hand in mock
 *    publishers/loggers that capture emissions while the real factories wrap a
 *    Supabase Realtime broadcast and a `postUsageEvent` POST (both
 *    NETWORK-UNVERIFIED — they have never run against a live backend here).
 */

import { createContext, useContext, useMemo, type ReactNode } from "react";

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

const Ctx = createContext<LiveBridgeTransports | null>(null);

interface ProviderProps {
  /** Host-supplied connection settings; omit for the default OFF transports. */
  config?: LiveBridgeConfig;
  children: ReactNode;
}

/**
 * Provide the constructed transports to the tree. Wrap the app (or just the
 * live route) so `LivePreview` can read them via `useLiveBridgeTransports`.
 */
export function LiveBridgeProvider({ config = {}, children }: ProviderProps) {
  const transports = useMemo(() => buildTransports(config), [config]);
  return <Ctx.Provider value={transports}>{children}</Ctx.Provider>;
}

/**
 * Read the injected transports. Returns `{}` (everything off) when no provider
 * is mounted, so callers never crash for lack of wiring — they just publish
 * nowhere, which is the safe default.
 */
export function useLiveBridgeTransports(): LiveBridgeTransports {
  return useContext(Ctx) ?? {};
}
