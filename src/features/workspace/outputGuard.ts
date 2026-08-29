/**
 * The output lock (Spor A) — the policy half.
 *
 * A locked output means "I am working, the congregation screen must not move".
 * The operator plans, browses, stages slides and rehearses freely; nothing that
 * would put new content on the projector gets through until the lock comes off
 * (⌘L / Ctrl+L).
 *
 * Two rules make it safe rather than merely convenient:
 *
 *  1. **The lock is a policy on `LiveAction`, not on a button.** Every route to
 *     the projector — click, Space/Enter/G, the Jump modal, the transport bar,
 *     the network remote, "show now" after adding a passage — funnels through
 *     one `dispatch`. Guarding the action type there means a new route cannot
 *     quietly bypass the lock: it has to invent a new way to reach Rust first.
 *  2. **The emergency stops are never locked out.** Blackout and Clear only ever
 *     take things off the screen or return to the cue that was already there.
 *     A verger reaching for the panic key during a fire alarm must not meet a
 *     lock. Everything that *adds* — advancing, jumping, the logo, an operator
 *     message — is content, and content is what the lock exists to stop.
 *
 * Pure and allocation-free: this runs on the live path, so it is one `switch`
 * over a tag and nothing else.
 */
import type { LiveAction } from "@/lib/bindings";

/**
 * True when the action puts something new in front of the congregation.
 * These are the actions the lock stops.
 */
export function isContentAction(action: LiveAction): boolean {
  switch (action.type) {
    case "next":
    case "previous":
    case "go_to":
    case "show_logo":
    case "show_message":
      return true;
    case "blackout":
    case "clear":
      return false;
  }
}

/**
 * True for the panic actions that only ever remove an override. Always allowed,
 * lock or no lock — a blackout is a fire escape, not a content decision.
 */
export function isEmergencyAction(action: LiveAction): boolean {
  return !isContentAction(action);
}

export type LockVerdict = "allow" | "blocked";

/** The whole lock, in one cheap state check. */
export function guardAction(locked: boolean, action: LiveAction): LockVerdict {
  return locked && isContentAction(action) ? "blocked" : "allow";
}

/**
 * Going live is its own route to the projector — it puts cue 1 on the screen
 * without ever passing through `dispatch`. A locked output blocks it too.
 */
export function guardGoLive(locked: boolean): LockVerdict {
  return locked ? "blocked" : "allow";
}
