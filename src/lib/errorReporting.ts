/**
 * E3 — renderer-side crash capture.
 *
 * ## The gap this closes
 *
 * Half of SundayStage is a webview, and until now nothing in it left a trace.
 * A `TypeError` in the operator workspace unmounted a panel and wrote a line to
 * a console nobody has open on a Sunday morning; an unhandled promise rejection
 * did not even do that. Meanwhile the Rust side has had a panic hook since
 * Phase 6.1. So the record of "what went wrong" was systematically missing
 * exactly the half the operator actually looks at.
 *
 * Everything captured here goes through the SAME bounded, path-scrubbed ring
 * the Rust panic hook writes to (`telemetry::crash_ring`), as
 * `webview_error` / `unhandled_rejection`. One ring, one cap, one scrubber —
 * so a frontend error can never be the thing that leaks a path.
 *
 * ## Two rules this module lives by
 *
 * 1. **It must never throw.** This is the last handler in the chain; an error
 *    raised inside an error handler is either an infinite loop or a silent
 *    hole. Every entry point is wrapped, and the IPC promise's rejection is
 *    swallowed on purpose.
 * 2. **It must not amplify.** A render loop that throws on every frame would
 *    otherwise fill the twenty-record ring in a second and evict the first, most
 *    diagnostic occurrence. Duplicate messages inside {@link DEDUPE_WINDOW_MS}
 *    collapse to one entry.
 *
 * Nothing here sends anything anywhere. Local capture is still gated by the
 * existing opt-in flag, checked in Rust.
 */

import { ipc } from "@/lib/ipc";
import type { CrashKind } from "@/lib/bindings";

/**
 * Repeats of the same message inside this window collapse into one record.
 *
 * Ten seconds is chosen against the failure this exists for: a React render
 * loop that throws produces hundreds of identical errors per second, and the
 * useful information is "this started happening", not "it happened 843 times".
 * It is short enough that a genuinely recurring problem still leaves several
 * marks across a service.
 */
export const DEDUPE_WINDOW_MS = 10_000;

/**
 * How many distinct messages are remembered for de-duplication. Bounded so the
 * de-duplicator cannot itself become the leak: an app throwing errors with a
 * unique id in each message would otherwise grow this map forever.
 */
const DEDUPE_MAX_KEYS = 50;

/** Cap applied before the message crosses IPC. Rust caps again at 200. */
const MESSAGE_MAX = 400;

/** Last-seen timestamps, keyed by message. */
const lastSeen = new Map<string, number>();

/** Installed listeners, so {@link installErrorReporting} is idempotent. */
let installed = false;

/** Clock seam, so the de-duplication window is testable without waiting. */
let now = () => Date.now();

/** Whether `message` should be recorded, or is a repeat inside the window. */
function shouldRecord(message: string): boolean {
  const at = now();
  const previous = lastSeen.get(message);
  if (previous !== undefined && at - previous < DEDUPE_WINDOW_MS) {
    return false;
  }
  // Evict the oldest key rather than growing without bound. Insertion order is
  // Map's iteration order, and a re-set moves the key to the end below.
  if (!lastSeen.has(message) && lastSeen.size >= DEDUPE_MAX_KEYS) {
    const oldest = lastSeen.keys().next().value;
    if (oldest !== undefined) lastSeen.delete(oldest);
  }
  lastSeen.delete(message);
  lastSeen.set(message, at);
  return true;
}

/** A readable one-line message for anything JavaScript can throw. */
export function describe(value: unknown): string {
  let text: string;
  if (value instanceof Error) {
    text = value.stack?.split("\n")[0] ?? `${value.name}: ${value.message}`;
  } else if (typeof value === "string") {
    text = value;
  } else {
    try {
      text = JSON.stringify(value) ?? String(value);
    } catch {
      // A circular object, or a getter that throws.
      text = Object.prototype.toString.call(value);
    }
  }
  return text.slice(0, MESSAGE_MAX);
}

/**
 * The `file:line:col` an Error's first stack frame names, if it has one.
 *
 * Scrubbed on the Rust side, so a `file:///Users/…` frame is safe to send —
 * this only has to find it.
 */
export function firstFrame(value: unknown): string | null {
  if (!(value instanceof Error) || !value.stack) return null;
  const frames = value.stack.split("\n").slice(1);
  const match = frames
    .map((line) => /\(?((?:[a-z]+:\/\/|\/)[^\s()]+:\d+:\d+)\)?/i.exec(line))
    .find((m) => m !== null);
  return match ? match[1] : null;
}

/**
 * Record one renderer error. Never throws, never rejects, never awaits.
 *
 * Returns whether the error was NEW (i.e. not swallowed by de-duplication), so
 * callers and tests can reason about the rate limit without reaching inside.
 */
export function reportError(
  kind: CrashKind,
  value: unknown,
  options: { location?: string | null; component?: string | null } = {},
): boolean {
  try {
    const message = describe(value);
    if (!message || !shouldRecord(message)) return false;
    void ipc.crash
      .report({
        kind,
        message,
        location: options.location ?? firstFrame(value),
        component: options.component ?? null,
      })
      // The whole point: a failure to record a failure is not itself an event.
      // In a browser (Vitest, `pnpm dev` without Tauri) there is no `invoke` at
      // all, and that must be a no-op rather than an uncaught rejection.
      .catch(() => {});
    return true;
  } catch {
    return false;
  }
}

/**
 * Attach `window.onerror` and `unhandledrejection`.
 *
 * Idempotent — React 19's StrictMode double-invokes effects in development, and
 * installing twice would double every record.
 */
export function installErrorReporting(): void {
  if (installed || typeof window === "undefined") return;
  installed = true;

  window.addEventListener("error", (event: ErrorEvent) => {
    const location =
      event.filename && event.lineno
        ? `${event.filename}:${event.lineno}:${event.colno ?? 0}`
        : null;
    reportError("webview_error", event.error ?? event.message, { location });
  });

  window.addEventListener(
    "unhandledrejection",
    (event: PromiseRejectionEvent) => {
      reportError("unhandled_rejection", event.reason);
    },
  );
}

/** Test seam: reset the de-duplication state and the installed flag. */
export function __resetForTests(clock?: () => number): void {
  lastSeen.clear();
  installed = false;
  now = clock ?? (() => Date.now());
}
