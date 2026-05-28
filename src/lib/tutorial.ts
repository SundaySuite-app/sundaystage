/**
 * First-run tutorial gate (Phase 13.1) — per device, localStorage-backed.
 */
const KEY = "ss-tutorial-done";

export function hasSeenTutorial(): boolean {
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return true; // if storage is unavailable, don't nag
  }
}

export function markTutorialSeen(): void {
  try {
    localStorage.setItem(KEY, "1");
  } catch {
    /* ignore */
  }
}

export function resetTutorial(): void {
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* ignore */
  }
}
