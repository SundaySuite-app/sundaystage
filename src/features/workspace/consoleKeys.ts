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
 *
 * The printed cheat sheet (`?`) is generated from `CONSOLE_SHORTCUTS` at the
 * bottom of this file, and every row of it carries the literal keystrokes it
 * advertises. `consoleKeys.test.ts` replays those keystrokes through
 * `resolveConsoleKey`: a row that lies fails the build. The hand-written copy
 * this replaces was wrong the day it was written.
 */
import type { TKey } from "@/lib/i18n";
import { isApplePlatform, modChord, shiftChord } from "@/lib/platform";

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
  /** A character that feeds the pending section sequence (`V`, `2`, …). */
  | "section-seq"
  /** Enter/Space while a sequence is pending: take the jump it is showing. */
  | "section-commit"
  /** Escape while a sequence is pending: drop it, touch nothing. */
  | "section-cancel"
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
  /**
   * A section sequence is half-typed. It owns Escape (cancel) and Enter/Space
   * (commit) for as long as it stands, and it is what lets a bare digit mean
   * "verse 2" rather than nothing at all.
   */
  seqActive: boolean;
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

  // A half-typed section sequence answers Escape first: the operator is looking
  // straight at it, and cancelling what you can see beats closing what you were
  // not thinking about.
  if (e.key === "Escape" && ctx.seqActive) return "section-cancel";
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

  // A pending sequence borrows Go's key to confirm itself. It is on screen
  // while it does, and it lapses on its own after a beat, so the borrow is
  // visible and short.
  if (ctx.seqActive && (e.key === " " || e.key === "Enter"))
    return "section-commit";

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
  }
  if (lower === "g") return "go";

  // Section sequences (A4). Every remaining letter starts one — including
  // letters no section answers to, because "nothing happened" is the one answer
  // a live operator cannot read. Digits only ever *extend* a sequence, so a
  // stray number key stays silent.
  //
  // Two exclusions: `b` is held free (the owner may want bare B back as
  // blackout), and sequences need a running service — off air there is nothing
  // to jump.
  if (!ctx.isLive || e.key.length !== 1) return "none";
  if (lower === "b") return "none";
  // Matched on the character the layout produced, not on a US key position:
  // æ/ø/å and every other national letter address a section like any other.
  if (/\p{L}/u.test(lower)) return "section-seq";
  if (ctx.seqActive && /\d/.test(lower)) return "section-seq";
  return "none";
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

// ── The printed cheat sheet ─────────────────────────────────────────────────
//
// One table, two readers: `ShortcutsModal` renders it for the operator, and
// `consoleKeys.test.ts` replays every `strokes` entry through
// `resolveConsoleKey` and asserts it produces the `action` the row claims. The
// keycaps the operator reads are formatted *from those same stroke objects*, so
// there is no second copy of the key table anywhere to drift.

/** How a stroke is drawn on a keycap. Anything else prints as its own key. */
const KEY_CAPS: Record<string, string> = {
  " ": "Space",
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
  PageUp: "PgUp",
  PageDown: "PgDn",
  Escape: "Esc",
};

/** The keycap for one stroke — `⌘L` on a Mac, `Ctrl+L` everywhere else. */
export function keyCap(
  s: KeyStroke,
  apple: boolean = isApplePlatform(),
): string {
  const base =
    KEY_CAPS[s.key] ?? (s.key.length === 1 ? s.key.toUpperCase() : s.key);
  if (s.metaKey || s.ctrlKey) return modChord(base, apple);
  if (s.shiftKey) return shiftChord(base, apple);
  return base;
}

export interface ShortcutRow {
  /** The literal strokes this row advertises. The tests replay them. */
  strokes: KeyStroke[];
  /** What `resolveConsoleKey` must answer for each of them. */
  action: ConsoleAction;
  /** The state the row is documented under (defaults: live, nothing open). */
  ctx?: Partial<ConsoleKeyContext>;
  /** The sentence next to the keys. */
  label: TKey;
  /**
   * The strokes are typed one after another (`V` then `2`), not alternatives.
   * Only the drawing changes; the assertions are per-stroke either way.
   */
  typed?: boolean;
}

export interface ShortcutGroup {
  heading: TKey;
  rows: ShortcutRow[];
}

function k(key: string, mods: Partial<KeyStroke> = {}): KeyStroke {
  return { key, metaKey: false, ctrlKey: false, shiftKey: false, ...mods };
}

export const CONSOLE_SHORTCUTS: ShortcutGroup[] = [
  {
    heading: "kbGroupPlayback",
    rows: [
      { strokes: [k(" "), k("Enter"), k("g")], action: "go", label: "kbGo" },
      {
        strokes: [k("ArrowLeft"), k("ArrowUp"), k("PageUp")],
        action: "preview-prev",
        label: "kbPrev",
      },
      {
        strokes: [k("ArrowRight"), k("ArrowDown"), k("PageDown")],
        action: "preview-next",
        label: "kbNext",
      },
      { strokes: [k("Home")], action: "preview-first", label: "kbFirst" },
      { strokes: [k("End")], action: "preview-last", label: "kbLast" },
    ],
  },
  {
    // A4. The keys are the section's own initial in the operator's language, so
    // the row shows the shape of a sequence rather than an exhaustive list.
    heading: "kbGroupSections",
    rows: [
      {
        strokes: [k("v"), k("2")],
        action: "section-seq",
        ctx: { seqActive: true },
        label: "kbSectionJump",
        typed: true,
      },
      {
        strokes: [k("Enter"), k(" ")],
        action: "section-commit",
        ctx: { seqActive: true },
        label: "kbSectionCommit",
      },
      {
        strokes: [k("Escape")],
        action: "section-cancel",
        ctx: { seqActive: true },
        label: "kbSectionCancel",
      },
    ],
  },
  {
    heading: "kbGroupOutput",
    rows: [
      // Blackout moved off Escape deliberately — see the table above.
      {
        strokes: [k("b", { shiftKey: true })],
        action: "blackout",
        label: "kbBlackout",
      },
      { strokes: [k("l")], action: "logo", label: "kbLogo" },
      {
        strokes: [k("l", { metaKey: true })],
        action: "toggle-lock",
        label: "kbLock",
      },
      {
        strokes: [k("z", { metaKey: true })],
        action: "undo-clear",
        label: "kbUndoClear",
      },
    ],
  },
  {
    heading: "kbGroupWorkspace",
    rows: [
      { strokes: [k("j", { metaKey: true })], action: "jump", label: "kbJump" },
      {
        // The one row that documents a key the console must NOT take: ⌘K
        // belongs to cmdk, and this asserts the console keeps its hands off it.
        strokes: [k("k", { metaKey: true })],
        action: "none",
        label: "kbPalette",
      },
      {
        strokes: [k("b", { metaKey: true })],
        action: "toggle-browser",
        label: "kbBrowse",
      },
      {
        strokes: [k("Escape")],
        action: "close-browser",
        ctx: { browserOpen: true },
        label: "kbCloseBrowser",
      },
      { strokes: [k("?")], action: "shortcuts", label: "kbShortcutsHelp" },
    ],
  },
];
