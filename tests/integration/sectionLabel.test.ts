// Section-label localisation. Confirms canonical labels map through `t()`,
// trailing numbers survive, synonyms and the cue-compiler's title-cased form
// are recognised, and unknown labels fall back to title-case.
import { describe, it, expect } from "vitest";

import { localizeSectionLabel, humanizeLabel } from "@/lib/sectionLabel";
import { translate, type TKey, type TParams } from "@/lib/i18n";

// Bind a `t` to a fixed locale for deterministic assertions.
const tFor =
  (lang: Parameters<typeof translate>[0]) => (key: TKey, params?: TParams) =>
    translate(lang, key, params);

describe("localizeSectionLabel", () => {
  const en = tFor("en");
  const no = tFor("no");
  const fr = tFor("fr");

  it("translates a bare canonical label", () => {
    expect(localizeSectionLabel("chorus", en)).toBe("Chorus");
    expect(localizeSectionLabel("chorus", no)).toBe("Refreng");
    expect(localizeSectionLabel("chorus", fr)).toBe("Refrain");
  });

  it("keeps a trailing number after the localised word", () => {
    expect(localizeSectionLabel("verse_1", en)).toBe("Verse 1");
    expect(localizeSectionLabel("verse_2", no)).toBe("Vers 2");
    expect(localizeSectionLabel("verse_3", fr)).toBe("Couplet 3");
  });

  it("handles multi-word bases with a number (pre_chorus_2)", () => {
    expect(localizeSectionLabel("pre_chorus", en)).toBe("Pre-Chorus");
    expect(localizeSectionLabel("pre_chorus_2", en)).toBe("Pre-Chorus 2");
  });

  it("accepts the cue compiler's already title-cased form", () => {
    // The Rust cue compiler humanises before the frontend ever sees it.
    expect(localizeSectionLabel("Verse 1", no)).toBe("Vers 1");
    expect(localizeSectionLabel("Pre Chorus", no)).toBe("Pre-refreng");
  });

  it("maps synonyms (refrain → chorus, outro → ending)", () => {
    expect(localizeSectionLabel("refrain", en)).toBe("Chorus");
    expect(localizeSectionLabel("outro", en)).toBe("Ending");
  });

  it("falls back to title-case for unknown, user-authored labels", () => {
    expect(localizeSectionLabel("my_special_part", en)).toBe("My Special Part");
    expect(localizeSectionLabel("turnaround_2", en)).toBe("Turnaround 2");
  });

  it("returns an empty string for blank input", () => {
    expect(localizeSectionLabel("", en)).toBe("");
    expect(localizeSectionLabel("   ", en)).toBe("");
  });
});

describe("humanizeLabel", () => {
  it("title-cases snake_case", () => {
    expect(humanizeLabel("verse_1")).toBe("Verse 1");
    expect(humanizeLabel("pre_chorus")).toBe("Pre Chorus");
  });
});
