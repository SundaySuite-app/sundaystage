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

// ── Keyboard shortcut key mapping (pure logic) ────────────────────────────────

/**
 * The workspace maps certain keys to actions. We test the key-to-action
 * table directly without needing the DOM, by replicating the pure switch
 * logic in a helper that mirrors the real handler.
 */
type ShortcutAction =
  | "go"
  | "prev"
  | "next"
  | "blackout"
  | "logo"
  | "first"
  | "last"
  | "jump"
  | "shortcuts"
  | "none";

function resolveKey(
  key: string,
  metaKey: boolean,
  ctrlKey: boolean,
  isLive: boolean,
): ShortcutAction {
  if ((metaKey || ctrlKey) && key.toLowerCase() === "j") {
    return isLive ? "jump" : "none";
  }
  if (metaKey || ctrlKey) return "none";
  if (key === "?") return "shortcuts";
  switch (key) {
    case "ArrowRight":
    case "ArrowDown":
      return "next";
    case "ArrowLeft":
    case "ArrowUp":
      return "prev";
    case " ":
    case "Enter":
    case "g":
    case "G":
      return "go";
    case "Escape":
    case "b":
    case "B":
      return isLive ? "blackout" : "none";
    case "l":
    case "L":
      return isLive ? "logo" : "none";
    case "Home":
      return "first";
    case "End":
      return "last";
    default:
      return "none";
  }
}

describe("workspace keyboard shortcut resolution", () => {
  describe("playback keys", () => {
    it("Space → go", () => {
      expect(resolveKey(" ", false, false, false)).toBe("go");
    });
    it("Enter → go", () => {
      expect(resolveKey("Enter", false, false, false)).toBe("go");
    });
    it("G → go", () => {
      expect(resolveKey("G", false, false, false)).toBe("go");
      expect(resolveKey("g", false, false, false)).toBe("go");
    });
    it("ArrowRight → next", () => {
      expect(resolveKey("ArrowRight", false, false, false)).toBe("next");
    });
    it("ArrowDown → next", () => {
      expect(resolveKey("ArrowDown", false, false, false)).toBe("next");
    });
    it("ArrowLeft → prev", () => {
      expect(resolveKey("ArrowLeft", false, false, false)).toBe("prev");
    });
    it("ArrowUp → prev", () => {
      expect(resolveKey("ArrowUp", false, false, false)).toBe("prev");
    });
    it("Home → first", () => {
      expect(resolveKey("Home", false, false, false)).toBe("first");
    });
    it("End → last", () => {
      expect(resolveKey("End", false, false, false)).toBe("last");
    });
  });

  describe("output keys (require live session)", () => {
    it("Escape → blackout when live", () => {
      expect(resolveKey("Escape", false, false, true)).toBe("blackout");
    });
    it("Escape → none when not live", () => {
      expect(resolveKey("Escape", false, false, false)).toBe("none");
    });
    it("B → blackout when live", () => {
      expect(resolveKey("B", false, false, true)).toBe("blackout");
      expect(resolveKey("b", false, false, true)).toBe("blackout");
    });
    it("B → none when not live", () => {
      expect(resolveKey("B", false, false, false)).toBe("none");
    });
    it("L → logo when live", () => {
      expect(resolveKey("L", false, false, true)).toBe("logo");
      expect(resolveKey("l", false, false, true)).toBe("logo");
    });
    it("L → none when not live", () => {
      expect(resolveKey("L", false, false, false)).toBe("none");
    });
  });

  describe("workspace keys", () => {
    it("? → shortcuts (always, even when not live)", () => {
      expect(resolveKey("?", false, false, false)).toBe("shortcuts");
      expect(resolveKey("?", false, false, true)).toBe("shortcuts");
    });
    it("⌘J → jump when live", () => {
      expect(resolveKey("j", true, false, true)).toBe("jump");
      expect(resolveKey("j", false, true, true)).toBe("jump");
    });
    it("⌘J → none when not live", () => {
      expect(resolveKey("j", true, false, false)).toBe("none");
    });
    it("⌘-anything else → none (preserve browser shortcuts)", () => {
      expect(resolveKey("k", true, false, true)).toBe("none");
      expect(resolveKey("z", true, false, true)).toBe("none");
    });
  });

  describe("unrecognised keys", () => {
    it("random printable keys → none", () => {
      expect(resolveKey("a", false, false, false)).toBe("none");
      expect(resolveKey("F1", false, false, false)).toBe("none");
    });
  });
});
