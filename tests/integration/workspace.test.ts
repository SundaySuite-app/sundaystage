/**
 * Tests for workspace utilities:
 *   - parseBibleRef: extracts book/chapter/verse from a cue display_label
 *   - isBibleCue: identifies scripture cues in the cue list
 *   - Keyboard shortcut key-mapping logic (pure logic, no DOM needed)
 */
import { describe, it, expect } from "vitest";

import { parseBibleRef, isBibleCue } from "@/features/workspace/cueUtils";
import type { Cue } from "@/lib/bindings";

// ── parseBibleRef ─────────────────────────────────────────────────────────────

describe("parseBibleRef", () => {
  it("parses a bare chapter reference", () => {
    const ref = parseBibleRef("John 3");
    expect(ref).toEqual({
      book: "John",
      chapter: 3,
      verseStart: null,
      verseEnd: null,
    });
  });

  it("parses a single-verse reference", () => {
    const ref = parseBibleRef("John 3:16");
    expect(ref).toEqual({
      book: "John",
      chapter: 3,
      verseStart: 16,
      verseEnd: null,
    });
  });

  it("parses a verse-range reference", () => {
    const ref = parseBibleRef("John 3:16-17");
    expect(ref).toEqual({
      book: "John",
      chapter: 3,
      verseStart: 16,
      verseEnd: 17,
    });
  });

  it("handles a numbered book (1 John)", () => {
    const ref = parseBibleRef("1 John 4:8");
    expect(ref).toEqual({
      book: "1 John",
      chapter: 4,
      verseStart: 8,
      verseEnd: null,
    });
  });

  it("handles multi-word book names", () => {
    const ref = parseBibleRef("1 Corinthians 13:4-7");
    expect(ref).toEqual({
      book: "1 Corinthians",
      chapter: 13,
      verseStart: 4,
      verseEnd: 7,
    });
  });

  it("handles Psalms with a range", () => {
    const ref = parseBibleRef("Psalms 23:1-6");
    expect(ref).toEqual({
      book: "Psalms",
      chapter: 23,
      verseStart: 1,
      verseEnd: 6,
    });
  });

  it("returns null for a song display label", () => {
    expect(parseBibleRef("Amazing Grace — Verse 1")).toBeNull();
    expect(parseBibleRef("")).toBeNull();
  });

  it("returns null for a label that is just a number", () => {
    expect(parseBibleRef("42")).toBeNull();
  });

  it("trims surrounding whitespace", () => {
    const ref = parseBibleRef("  Romans 8:28  ");
    expect(ref?.book).toBe("Romans");
    expect(ref?.chapter).toBe(8);
    expect(ref?.verseStart).toBe(28);
  });
});

// ── isBibleCue ────────────────────────────────────────────────────────────────

/** Build a minimal show_slide Cue for testing. */
function makeSlideCue(displayLabel: string): Cue {
  return {
    kind: "show_slide",
    cue_id: "test-cue-id",
    slide_content: {
      section_label: null,
      text_lines: ["line 1"],
      translation_lines: null,
      reference: null,
    },
    theme_id: null,
    template_id: null,
    source: {
      service_item_id: "item-1",
      item_cue_index: 0,
      display_label: displayLabel,
    },
  };
}

describe("isBibleCue", () => {
  it("returns true for a scripture cue whose label parses as a bible ref", () => {
    expect(isBibleCue(makeSlideCue("John 3:16"))).toBe(true);
    expect(isBibleCue(makeSlideCue("1 Corinthians 13:4-7"))).toBe(true);
    expect(isBibleCue(makeSlideCue("Psalms 23"))).toBe(true);
  });

  it("returns false for a song cue whose label does not match", () => {
    expect(isBibleCue(makeSlideCue("Amazing Grace — Verse 1"))).toBe(false);
    expect(isBibleCue(makeSlideCue("Chorus"))).toBe(false);
  });

  it("returns false for non-slide cues", () => {
    const blackOut: Cue = { kind: "black_out", cue_id: "b1" };
    const logo: Cue = { kind: "show_logo", cue_id: "l1" };
    const pause: Cue = { kind: "pause", cue_id: "p1", label: "Offering" };
    expect(isBibleCue(blackOut)).toBe(false);
    expect(isBibleCue(logo)).toBe(false);
    expect(isBibleCue(pause)).toBe(false);
  });
});

// ── Bible search hit → deep-link resolution (pure logic) ──────────────────────

import type { BibleDeepLink } from "@/features/bible/BiblePage";
import type { Route } from "@/components/CommandPalette";

/**
 * Mirrors the bible branch of `OperatorWorkspace.onOpenResult`: a bible search
 * hit carries its reference ("John 3:16") as the result id, which we parse into
 * a `BibleDeepLink` so the scripture browser opens that exact passage. Returns
 * the deep-link that would be set, or null when the reference does not parse.
 */
function resolveOpenResult(route: Route, id: string): BibleDeepLink | null {
  if (route !== "bible") return null;
  const ref = parseBibleRef(id);
  if (!ref) return null;
  return {
    book: ref.book,
    chapter: ref.chapter,
    verseStart: ref.verseStart,
    verseEnd: ref.verseEnd,
  };
}

describe("bible search hit deep-link resolution", () => {
  it("turns a single-verse hit into a deep-link", () => {
    expect(resolveOpenResult("bible", "John 3:16")).toEqual({
      book: "John",
      chapter: 3,
      verseStart: 16,
      verseEnd: null,
    });
  });

  it("turns a verse-range hit into a deep-link", () => {
    expect(resolveOpenResult("bible", "1 Corinthians 13:4-7")).toEqual({
      book: "1 Corinthians",
      chapter: 13,
      verseStart: 4,
      verseEnd: 7,
    });
  });

  it("returns null (no deep-link) for an unparseable reference", () => {
    expect(resolveOpenResult("bible", "")).toBeNull();
    expect(resolveOpenResult("bible", "not a reference!")).toBeNull();
  });

  it("does not resolve a deep-link for non-bible routes", () => {
    expect(resolveOpenResult("library", "song-id")).toBeNull();
    expect(resolveOpenResult("services", "service-id")).toBeNull();
  });

  it("does not throw on any input", () => {
    expect(() => resolveOpenResult("bible", "John 3:16")).not.toThrow();
    expect(() => resolveOpenResult("bible", "")).not.toThrow();
  });
});

// ── Keyboard shortcut key mapping ────────────────────────────────────────────
//
// This file used to carry a hand-written `resolveKey` copy of the console's key
// table, and assert against the copy. Two layers that agree with themselves and
// disagree with each other is exactly the seam this codebase keeps getting bitten
// by: the mirror said "Escape → blackout" long after that was a question anyone
// wanted to reopen, and it would have said so just as confidently the day the
// console changed.
//
// The table now lives in `src/features/workspace/consoleKeys.ts`, the console
// calls it directly, and `consoleKeys.test.ts` tests that function — the real
// one — instead.
