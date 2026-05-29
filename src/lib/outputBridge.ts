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

import type { LiveFrame, OutputAppearance } from "@/lib/bindings";

export const OUTPUT_RENDER = "ss://render";
export const OUTPUT_HEARTBEAT = "ss://heartbeat";
/** Broadcast when the operator changes output appearance in Settings, so the
 *  open output windows restyle live without reopening. */
export const OUTPUT_APPEARANCE = "ss://appearance";
const HEARTBEAT_MS = 250;

/** Mirror of Rust `OutputAppearance::default()` — used before the saved config
 *  loads and as the preview/fallback baseline. */
export const DEFAULT_OUTPUT_APPEARANCE: OutputAppearance = {
  text_scale: 1.0,
  text_color: "#ffffff",
  bg_color: "#0a1730",
  h_align: "center",
  show_section_label: true,
  uppercase: false,
  line_height: 1.1,
};

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
