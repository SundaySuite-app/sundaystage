/**
 * Phase 5.2 — the operator → output signal bus.
 *
 * The output windows render whatever the operator UI broadcasts. We drive this
 * over Tauri's event bus rather than polling so cue advance reaches pixels
 * fast, and so a frozen operator UI is *observable*: when this UI stops
 * heart-beating, each output window's watchdog holds the last frame instead of
 * blanking the congregation.
 */
import { useEffect, useRef } from "react";
import { emit } from "@tauri-apps/api/event";

import type { LiveFrame } from "@/lib/bindings";

export const OUTPUT_RENDER = "ss://render";
export const OUTPUT_HEARTBEAT = "ss://heartbeat";
const HEARTBEAT_MS = 250;

function safeEmit(event: string, payload?: unknown) {
  // Fire-and-forget; outside Tauri (browser tests) emit rejects — ignore it.
  void emit(event, payload).catch(() => {});
}

/**
 * Re-broadcast the current frame on every change and send a heartbeat while a
 * session is live. Mount this in the operator console.
 */
export function useOutputBridge(frame: LiveFrame | null, active: boolean) {
  const seq = useRef(0);

  useEffect(() => {
    if (!active || !frame) return;
    seq.current += 1;
    safeEmit(OUTPUT_RENDER, { frame, seq: seq.current });
  }, [frame, active]);

  useEffect(() => {
    if (!active) return;
    const id = setInterval(
      () => safeEmit(OUTPUT_HEARTBEAT, { at: Date.now() }),
      HEARTBEAT_MS,
    );
    return () => clearInterval(id);
  }, [active]);
}
