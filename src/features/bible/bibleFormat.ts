/**
 * Pure formatting helpers for the scripture browser (kept out of BiblePage so
 * the component file only exports components, matching the cueUtils pattern).
 */

/**
 * Formats the verse suffix shown after "Book Chapter" in the reading header
 * ("John 3" → ":16" or ":4–7"). Returns an empty string when no verse range is
 * active so the whole chapter reads as just "John 3".
 */
export function formatVerseSuffix(
  range: { start: number; end: number } | null,
): string {
  if (!range) return "";
  if (range.end > range.start) return `:${range.start}–${range.end}`;
  return `:${range.start}`;
}
