/**
 * Undo for "Clear" (Spor A) — the one destructive button on the console.
 *
 * Clear drops whatever override is on the output (blackout, logo, or the
 * operator message) and returns to the running cue. Until now that was final:
 * an operator who typed "Barnevakt til rom 2", showed it, and then hit Clear one
 * beat too early had no way back except retyping it in front of a congregation.
 *
 * So Clear now captures what it is about to throw away, and for a short window
 * ⌘Z / Ctrl+Z puts *exactly that* back. The buffer is a single small object in
 * memory — never disk, never telemetry — and it is dropped the moment any other
 * action moves the show on, because "restore" must mean the screen the operator
 * remembers, not an override re-applied over a different cue.
 */
import type { LiveAction, LiveSessionView, OutputState } from "@/lib/bindings";

/** How long the restore offer stands. Long enough to notice, short enough that
 *  ⌘Z never resurrects something from three cues ago. */
export const CLEAR_UNDO_WINDOW_MS = 7000;

/** An override that Clear removed, captured verbatim. */
export interface ClearedOverride {
  /** The output state Clear replaced. Never `normal` — that has nothing to lose. */
  output: Exclude<OutputState, "normal">;
  /** The message text, when a message was what got cleared. */
  text: string | null;
}

/**
 * What a Clear is about to discard, or `null` when the output is already
 * showing its cue normally and Clear would be a no-op.
 */
export function captureOverride(
  session: LiveSessionView | null,
): ClearedOverride | null {
  if (!session) return null;
  switch (session.output) {
    case "normal":
      return null;
    case "message": {
      const text =
        session.frame.kind === "message" ? session.frame.text.trim() : "";
      // An empty message cannot be restored (Rust treats empty text as Clear),
      // so there is nothing to offer.
      return text ? { output: "message", text } : null;
    }
    case "blackout":
      return { output: "blackout", text: null };
    case "logo":
      return { output: "logo", text: null };
  }
}

/**
 * The single action that puts `cleared` back exactly as it was.
 *
 * `blackout` and `show_logo` are toggles in Rust, and Clear always leaves the
 * session at `normal` — so one toggle from `normal` lands on precisely the
 * state that was captured.
 */
export function restoreAction(cleared: ClearedOverride): LiveAction {
  switch (cleared.output) {
    case "blackout":
      return { type: "blackout" };
    case "logo":
      return { type: "show_logo" };
    case "message":
      return { type: "show_message", text: cleared.text ?? "" };
  }
}

/** Seconds left in the restore window, rounded up for the countdown label. */
export function secondsLeft(startedAt: number, now: number): number {
  return Math.max(
    0,
    Math.ceil((startedAt + CLEAR_UNDO_WINDOW_MS - now) / 1000),
  );
}
