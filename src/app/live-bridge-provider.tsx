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
 *
 * The non-component half (`buildTransports`, `LiveBridgeConfig`, the context,
 * and `useLiveBridgeTransports`) lives in `live-bridge-provider.utils.ts` so
 * this file exports *only* the component — React Fast Refresh requires that.
 */

import { useMemo, type ReactNode } from "react";

import {
  buildTransports,
  LiveBridgeContext,
  type LiveBridgeConfig,
} from "./live-bridge-provider.utils";

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
  return (
    <LiveBridgeContext.Provider value={transports}>
      {children}
    </LiveBridgeContext.Provider>
  );
}
