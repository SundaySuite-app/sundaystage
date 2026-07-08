/**
 * Keyboard scoping for the operator console.
 *
 * The transport hotkeys (Space/arrows/G/B/L/Esc) are global by design — a
 * volunteer mid-service must never hunt for focus before blacking out. But
 * they must not steal keystrokes from text entry, and the docked resource
 * browser needs Space/Enter/arrows for its own navigation. So every keydown
 * is classified by its target:
 *
 * - `"text"`   — typing in a field: the console gets nothing.
 * - `"dock"`   — focus inside an element marked `data-console-dock` (the
 *   docked library/bible browser): only the panic keys (B/L blackout/logo,
 *   Esc) reach the console; navigation/activation keys stay local so browsing
 *   never accidentally fires Go.
 * - `"console"`— everywhere else: the full transport.
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
