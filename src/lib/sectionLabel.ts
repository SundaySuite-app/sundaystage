/**
 * Section-label localisation.
 *
 * Song sections carry canonical snake_case labels (`verse_1`, `pre_chorus`,
 * `chorus`, `bridge`, …) produced by the Rust lyric formatter
 * (`normalize_label`). The UI used to render them with a plain `humanize()`
 * that only title-cased the words, so they always read in English regardless
 * of locale. This maps the known section *types* through `t()` while keeping
 * any trailing number, and falls back to title-casing for unrecognised
 * (user-authored) labels.
 *
 *   localizeSectionLabel("verse_1", t)    // "Verse 1" / "Vers 1" / "Couplet 1"
 *   localizeSectionLabel("pre_chorus_2")  // "Pre-Chorus 2" / "Pré-refrain 2"
 *   localizeSectionLabel("my_section")    // "My Section" (fallback)
 */

import type { TKey, TParams } from "./i18n";

type TFunc = (key: TKey, params?: TParams) => string;

/** Maps a canonical section base (and common synonyms) to its catalog key. */
const SECTION_BASE_KEYS: Record<string, TKey> = {
  intro: "sectionIntro",
  verse: "sectionVerse",
  pre_chorus: "sectionPreChorus",
  prechorus: "sectionPreChorus",
  chorus: "sectionChorus",
  refrain: "sectionChorus",
  bridge: "sectionBridge",
  instrumental: "sectionInstrumental",
  tag: "sectionTag",
  ending: "sectionEnding",
  outro: "sectionEnding",
};

/** Title-case fallback for labels we don't recognise. */
export function humanizeLabel(label: string): string {
  return label
    .split("_")
    .map((p) => (p ? p[0].toUpperCase() + p.slice(1) : ""))
    .join(" ");
}

/**
 * Render a section label in the current locale. Splits an optional trailing
 * `_<number>` from the base, translates the base if known, and re-appends the
 * number (e.g. `verse_1` → "Verse 1"). Unknown bases fall back to title-case.
 *
 * Separators are normalised so this accepts both the raw canonical label
 * (`pre_chorus`, `verse_1`) and the already-title-cased form the cue compiler
 * emits (`Pre Chorus`, `Verse 1`) — the function is the single place that
 * decides how a section type reads in the current language.
 */
export function localizeSectionLabel(label: string, t: TFunc): string {
  const trimmed = label.trim();
  if (!trimmed) return "";

  // "Pre Chorus" / "pre-chorus" / "pre_chorus" → "pre_chorus"
  const normalized = trimmed.toLowerCase().replace(/[\s-]+/g, "_");
  const match = /^(.*?)(?:_(\d+))?$/.exec(normalized);
  const base = match?.[1] ?? normalized;
  const num = match?.[2];

  const key = SECTION_BASE_KEYS[base];
  if (key) {
    const word = t(key);
    return num ? `${word} ${num}` : word;
  }
  return humanizeLabel(trimmed);
}
