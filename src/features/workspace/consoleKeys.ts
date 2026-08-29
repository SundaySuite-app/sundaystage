/**
 * Keyboard scoping and key resolution for the operator console.
 *
 * The transport hotkeys (Space/arrows/G/⇧B/L) are global by design — a
 * volunteer mid-service must never hunt for focus before blacking out. But
 * they must not steal keystrokes from text entry, and the docked resource
 * browser needs Space/Enter/arrows for its own navigation. So every keydown
 * is classified by its target:
 *
 * - `"text"`   — typing in a field: the console gets nothing.
 * - `"dock"`   — focus inside an element marked `data-console-dock` (the
 *   docked library/bible browser): only the panic keys (⇧B/L blackout/logo)
 *   reach the console; navigation/activation keys stay local so browsing
 *   never accidentally fires Go.
 * - `"console"`— everywhere else: the full transport.
 *
 * `resolveConsoleKey` is the whole key table as one pure function. The console
 * calls it and nothing else, so what the tests assert IS what runs — a table
 * duplicated into a test mirror would agree with itself while disagreeing with
 * the console.
 */
export type KeyScope = "text" | "dock" | "console";

export function keyScope(target: EventTarget | null): KeyScope {
  if (!(target instanceof Element)) return "console";
  const el = target as HTMLElement;
  // isContentEditable plus the attribute walk: jsdom (tests) doesn't implement
  // the property, and the attribute inherits to children either way.
  const editable = el.closest("[contenteditable]");
  if (
    el.isContentEditable ||
    (editable && editable.getAttribute("contenteditable") !== "false")
  )
    return "text";
  const tag = el.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return "text";
  if (el.closest("[data-console-dock]") != null) return "dock";
  return "console";
}

/** Everything a console keystroke can mean. */
export type ConsoleAction =
  | "go"
  | "preview-next"
  | "preview-prev"
  | "preview-first"
  | "preview-last"
  | "blackout"
  | "logo"
  | "toggle-lock"
  | "undo-clear"
  | "jump"
  | "toggle-browser"
  | "close-browser"
  | "shortcuts"
  | "none";

/** The parts of a KeyboardEvent the table reads. */
export interface KeyStroke {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
}

export interface ConsoleKeyContext {
  scope: KeyScope;
  isLive: boolean;
  /** The docked resource browser is open (Escape closes it first). */
  browserOpen: boolean;
  /** A true modal owns the keyboard — the console gets nothing behind it. */
  modalOpen: boolean;
}

/**
 * The console key table.
 *
 * Deliberate change (Spor A): **Escape no longer blacks out.** Escape is the
 * key every human presses to dismiss a dialog, and on a Sunday morning that
 * reflex was aimed straight at the congregation screen. Blackout now has a
 * dedicated two-key chord, ⇧B, that nobody hits by reflex; Escape closes the
 * docked browser and otherwise does nothing at all.
 */
export function resolveConsoleKey(
  e: KeyStroke,
  ctx: ConsoleKeyContext,
): ConsoleAction {
  // Never hijack typing in a form field.
  if (ctx.scope === "text") return "none";
  // A modal owns the keyboard. ⌘K is left alone here so cmdk still gets it.
  if (ctx.modalOpen) return "none";

  const lower = e.key.toLowerCase();

  if (e.metaKey || e.ctrlKey) {
    switch (lower) {
      case "j":
        return ctx.isLive ? "jump" : "none";
      case "b":
        return "toggle-browser";
      case "l":
        return "toggle-lock";
      case "z":
        return "undo-clear";
      default:
        // Leave ⌘K and every browser/system chord to their own handlers.
        return "none";
    }
  }

  // Escape closes the docked browser — and does nothing else, ever.
  if (e.key === "Escape") return ctx.browserOpen ? "close-browser" : "none";
  if (e.key === "?") return "shortcuts";

  // ⇧B — the dedicated blackout chord. Matched on the physical letter plus the
  // shift flag so Caps Lock (which flips `key` without setting `shiftKey`)
  // neither fires it by accident nor blocks it.
  if (e.shiftKey && lower === "b") return ctx.isLive ? "blackout" : "none";
  if (lower === "l") return ctx.isLive ? "logo" : "none";

  // Focus inside the docked browser: only the panic keys reach the console —
  // Space/Enter/arrows keep doing browser navigation.
  if (ctx.scope === "dock") return "none";

  switch (e.key) {
    case "ArrowRight":
    case "ArrowDown":
    case "PageDown":
      return "preview-next";
    case "ArrowLeft":
    case "ArrowUp":
    case "PageUp":
      return "preview-prev";
    case " ":
    case "Enter":
      return "go";
    case "Home":
      return "preview-first";
    case "End":
      return "preview-last";
    default:
      return lower === "g" ? "go" : "none";
  }
}

/** Actions the console consumes the keystroke for (preventDefault). */
export function consumesKey(action: ConsoleAction): boolean {
  switch (action) {
    case "none":
    case "logo":
      // Logo never scrolled anything and never has; keep the historic behaviour
      // of leaving the event alone.
      return false;
    default:
      return true;
  }
}
