/**
 * themeActions — pure helpers for theme CRUD (Phase 3.2, headless-1).
 *
 * Kept free of React/IPC so the naming/validation logic can be unit-tested
 * without rendering the panel or mocking Tauri.
 */

/**
 * Produce a name that does not collide with any existing theme name.
 * Returns `base` if free, otherwise `base 2`, `base 3`, … (case-insensitive
 * comparison, matching how a user perceives duplicate names).
 */
export function uniqueThemeName(existing: string[], base: string): string {
  const taken = new Set(existing.map((n) => n.trim().toLowerCase()));
  const root = base.trim() || base;
  if (!taken.has(root.toLowerCase())) return root;
  let n = 2;
  while (taken.has(`${root} ${n}`.toLowerCase())) n += 1;
  return `${root} ${n}`;
}

/**
 * Normalise a user-entered theme name. Returns the trimmed value, or `null`
 * when it is empty/whitespace-only (caller should treat that as "cancel").
 */
export function cleanThemeName(raw: string | null | undefined): string | null {
  if (raw == null) return null;
  const trimmed = raw.trim();
  return trimmed.length > 0 ? trimmed : null;
}
