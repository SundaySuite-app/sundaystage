/**
 * Phase 4.1 — one-time AI consent + model preference (per device).
 *
 * Cloud AI is opt-in: the first time a feature would send content to Anthropic,
 * the user sees a consent dialog explaining what leaves the machine. The choice
 * (and the preferred model) is kept in localStorage — it's a per-device UI
 * preference, not synced data.
 */

const CONSENT_KEY = "ss-ai-consent";
const MODEL_KEY = "ss-ai-model";

export function hasAiConsent(): boolean {
  try {
    return localStorage.getItem(CONSENT_KEY) === "1";
  } catch {
    return false;
  }
}

export function grantAiConsent(): void {
  try {
    localStorage.setItem(CONSENT_KEY, "1");
  } catch {
    /* ignore */
  }
}

export function revokeAiConsent(): void {
  try {
    localStorage.removeItem(CONSENT_KEY);
  } catch {
    /* ignore */
  }
}

export function preferredModel(): string | null {
  try {
    return localStorage.getItem(MODEL_KEY);
  } catch {
    return null;
  }
}

export function setPreferredModel(id: string): void {
  try {
    localStorage.setItem(MODEL_KEY, id);
  } catch {
    /* ignore */
  }
}
