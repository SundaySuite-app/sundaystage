/**
 * Platform-correct keyboard chord labels.
 *
 * Mac and Windows are first-class equals (core promise #5), and a shortcut we
 * print wrong is a shortcut the volunteer does not find. The console advertises
 * ⌘L / ⇧B / ⌘Z on macOS and Ctrl+L / Shift+B / Ctrl+Z everywhere else, so the
 * label has to be computed, never hard-coded in a component.
 *
 * The detection is deliberately conservative: only a platform string that
 * actually says "Mac"/"iPhone"/"iPad" counts. jsdom reports `linux`/`darwin`
 * (the Node platform, not a browser platform token), so tests and CI resolve to
 * the Ctrl labels identically on every machine — the same string the Windows
 * build shows. Real macOS WKWebView reports "MacIntel" / "Mac OS X".
 */

interface PlatformNavigator {
  platform?: string;
  userAgent?: string;
  userAgentData?: { platform?: string };
}

/** True when the UI runs on an Apple platform, where ⌘ is the console modifier. */
export function isApplePlatform(): boolean {
  try {
    const nav = (globalThis as { navigator?: PlatformNavigator }).navigator;
    if (!nav) return false;
    const declared = nav.userAgentData?.platform ?? nav.platform ?? "";
    if (declared) return /^(mac|iphone|ipad|ipod)/i.test(declared);
    return /mac os x|iphone|ipad|ipod/i.test(nav.userAgent ?? "");
  } catch {
    return false;
  }
}

/** `⌘L` on Apple, `Ctrl+L` elsewhere. `apple` is injectable for tests. */
export function modChord(key: string, apple: boolean = isApplePlatform()) {
  const k = key.length === 1 ? key.toUpperCase() : key;
  return apple ? `⌘${k}` : `Ctrl+${k}`;
}

/** `⇧B` on Apple, `Shift+B` elsewhere. `apple` is injectable for tests. */
export function shiftChord(key: string, apple: boolean = isApplePlatform()) {
  const k = key.length === 1 ? key.toUpperCase() : key;
  return apple ? `⇧${k}` : `Shift+${k}`;
}

/** The bare modifier label, for prose ("hold ⌘"). */
export function modLabel(apple: boolean = isApplePlatform()): string {
  return apple ? "⌘" : "Ctrl";
}
