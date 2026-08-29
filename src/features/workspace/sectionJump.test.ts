import { describe, expect, it } from "vitest";

import type { Cue } from "@/lib/bindings";
import {
  buildSectionIndex,
  matchSequence,
  sectionsForCue,
  type ItemSections,
} from "./sectionJump";

/**
 * The section index is built from the cue list the Rust compiler actually
 * emits: `section_label` carries the *humanized canonical* label ("Verse 1",
 * "Chorus", "Pre Chorus") for anything the lyric formatter recognised, and the
 * church's own word verbatim ("Stikk") for anything it did not.
 */
function slide(item: string, label: string | null, n: number): Cue {
  return {
    kind: "show_slide",
    cue_id: `${item}-${label}-${n}`,
    slide_content: {
      section_label: label,
      text_lines: ["linje"],
      translation_lines: null,
      reference: null,
      sensitive_slide: false,
    },
    theme_id: null,
    template_id: null,
    source: {
      service_item_id: item,
      item_cue_index: n,
      display_label: `${label ?? ""}`,
    },
  } as Cue;
}

function blackout(n: number): Cue {
  return { kind: "black_out", cue_id: `bo-${n}` } as Cue;
}

/** The Norwegian operator's view of the canonical labels. */
const NO: Record<string, string> = {
  "Verse 1": "Vers 1",
  "Verse 2": "Vers 2",
  "Verse 3": "Vers 3",
  "Verse 10": "Vers 10",
  "Verse 12": "Vers 12",
  Chorus: "Refreng",
  "Chorus 2": "Refreng 2",
  "Pre Chorus": "Pre-refreng",
  Bridge: "Bro",
  Ending: "Slutt",
};
const localizeNo = (label: string) => NO[label] ?? label;

/**
 * A real arrangement: verse 1, chorus, verse 2, chorus, bridge, chorus. The
 * chorus is ONE section played three times — exactly the shape `assemble` in
 * Rust produces after deduping identical lyrics.
 */
const SONG: Cue[] = [
  slide("song-1", "Verse 1", 0),
  slide("song-1", "Verse 1", 1),
  slide("song-1", "Chorus", 2),
  slide("song-1", "Verse 2", 3),
  slide("song-1", "Chorus", 4),
  slide("song-1", "Bridge", 5),
  slide("song-1", "Chorus", 6),
];

function sections(cues: Cue[], item = "song-1"): ItemSections {
  return buildSectionIndex(cues, localizeNo).get(item)!;
}

describe("buildSectionIndex", () => {
  it("collapses a repeated section into one group with several runs", () => {
    const item = sections(SONG);
    expect(item.groups.map((g) => g.label)).toEqual([
      "Verse 1",
      "Chorus",
      "Verse 2",
      "Bridge",
    ]);
    const chorus = item.groups.find((g) => g.label === "Chorus")!;
    expect(chorus.runs).toEqual([
      { start: 2, end: 2 },
      { start: 4, end: 4 },
      { start: 6, end: 6 },
    ]);
  });

  it("keeps a multi-slide section as one run", () => {
    const verse = sections(SONG).groups[0];
    expect(verse.runs).toEqual([{ start: 0, end: 1 }]);
  });

  it("addresses a section by its own word AND the operator's word", () => {
    const chorus = sections(SONG).groups[1];
    expect(chorus.words).toEqual(["chorus", "refreng"]);
    expect(chorus.display).toBe("Refreng");
  });

  it("numbers the members of a family in play order", () => {
    const item = sections(SONG);
    const verses = item.groups.filter((g) => g.family === "verse");
    expect(verses.map((g) => [g.number, g.ordinal])).toEqual([
      [1, 1],
      [2, 2],
    ]);
  });

  it("keeps each song's sections to itself", () => {
    const index = buildSectionIndex(
      [...SONG, slide("song-2", "Verse 1", 0)],
      localizeNo,
    );
    expect(index.get("song-1")!.groups.length).toBe(4);
    expect(index.get("song-2")!.groups.length).toBe(1);
  });

  it("ignores cues that carry no section at all", () => {
    const index = buildSectionIndex([blackout(0), ...SONG], localizeNo);
    expect(index.size).toBe(1);
  });

  it("does not join two runs across a blackout cue", () => {
    const index = buildSectionIndex(
      [slide("song-1", "Chorus", 0), blackout(1), slide("song-1", "Chorus", 2)],
      localizeNo,
    );
    expect(index.get("song-1")!.groups[0].runs).toEqual([
      { start: 0, end: 0 },
      { start: 2, end: 2 },
    ]);
  });
});

describe("sectionsForCue", () => {
  const cues = [...SONG, blackout(7), slide("song-2", "Verse 1", 0)];
  const index = buildSectionIndex(cues, localizeNo);

  it("scopes to the song the cue belongs to", () => {
    expect(sectionsForCue(index, cues, 3)!.groups.length).toBe(4);
    expect(sectionsForCue(index, cues, 8)!.groups.length).toBe(1);
  });

  it("falls back to the song before a blackout cue", () => {
    // Sitting on a blackout between songs, "R" should still mean the song the
    // operator just came out of, not nothing at all.
    expect(sectionsForCue(index, cues, 7)!.groups.length).toBe(4);
  });

  it("returns null when nothing before the cue has a section", () => {
    expect(sectionsForCue(index, [blackout(0)], 0)).toBeNull();
  });
});

describe("matchSequence — the Norwegian console", () => {
  const item = sections(SONG);

  it("V2 lands on verse 2 and says so in Norwegian", () => {
    expect(matchSequence("v2", item, 0)).toEqual({
      index: 3,
      label: "Vers 2",
      ambiguous: false,
    });
  });

  it("R means refreng — the operator's word, not the file's", () => {
    expect(matchSequence("r", item, 0)?.label).toBe("Refreng");
  });

  it("C means the same section — the file's word still works", () => {
    expect(matchSequence("c", item, 0)?.index).toBe(2);
  });

  it("is case-insensitive", () => {
    expect(matchSequence("V2", item, 0)?.index).toBe(3);
    expect(matchSequence("R", item, 0)?.index).toBe(2);
  });

  it("V alone waits, because a second key could still mean verse 2", () => {
    const m = matchSequence("v", item, 0)!;
    expect(m.ambiguous).toBe(true);
    expect(m.index).toBe(0);
  });

  it("R alone fires straight away — there is only one chorus", () => {
    expect(matchSequence("r", item, 0)!.ambiguous).toBe(false);
  });

  it("a longer prefix works too", () => {
    expect(matchSequence("ref", item, 0)?.index).toBe(2);
    expect(matchSequence("verse", item, 0)?.ambiguous).toBe(true);
  });

  it("answers nothing for a letter no section here starts with", () => {
    expect(matchSequence("q", item, 0)).toBeNull();
    expect(matchSequence("v9", item, 0)).toBeNull();
  });

  it("answers nothing when there is no song in scope", () => {
    expect(matchSequence("v", null, 0)).toBeNull();
  });

  it("rejects a buffer that is not letters-then-digits", () => {
    expect(matchSequence("2", item, 0)).toBeNull();
    expect(matchSequence("", item, 0)).toBeNull();
  });
});

describe("matchSequence — a repeated section resolves forward", () => {
  const item = sections(SONG);

  it("picks the next chorus, not the first one", () => {
    // Live on verse 2 (index 3): the band takes the chorus again. Sending the
    // show back to index 2 would look right and then replay verse 2.
    expect(matchSequence("r", item, 3)?.index).toBe(4);
  });

  it("picks the run the show is already inside", () => {
    expect(matchSequence("r", item, 4)?.index).toBe(4);
  });

  it("wraps to the first run when nothing is left ahead", () => {
    expect(matchSequence("r", item, 6)?.index).toBe(6);
    expect(matchSequence("v", item, 6)?.index).toBe(0);
  });
});

describe("matchSequence — labels that are not the ideal case", () => {
  it("handles a song with two different choruses", () => {
    const item = sections([
      slide("song-1", "Verse 1", 0),
      slide("song-1", "Chorus", 1),
      slide("song-1", "Chorus 2", 2),
    ]);
    expect(matchSequence("r", item, 0)?.label).toBe("Refreng");
    expect(matchSequence("r2", item, 0)?.label).toBe("Refreng 2");
    // With two of them, a bare R has to wait for the second key.
    expect(matchSequence("r", item, 0)?.ambiguous).toBe(true);
  });

  it("keeps pre-chorus off the chorus key", () => {
    // "Pre Chorus" is addressed by P, never by C or R: the first word decides,
    // so the key an operator reaches for under pressure is unambiguous.
    const item = sections([
      slide("song-1", "Pre Chorus", 0),
      slide("song-1", "Chorus", 1),
    ]);
    expect(matchSequence("p", item, 0)?.label).toBe("Pre-refreng");
    expect(matchSequence("c", item, 0)?.label).toBe("Refreng");
    expect(matchSequence("r", item, 0)?.label).toBe("Refreng");
  });

  it("reaches a church's own label with no translation behind it", () => {
    // What EasyWorship/.pro6/FreeShow imports actually leave behind when the
    // formatter did not recognise the word.
    const item = sections([
      slide("song-1", "Stikk", 0),
      slide("song-1", "Chorus", 1),
    ]);
    expect(matchSequence("s", item, 0)?.label).toBe("Stikk");
    expect(matchSequence("s", item, 0)?.ambiguous).toBe(false);
  });

  it("handles a section already written in Norwegian in the file", () => {
    const item = sections([
      slide("song-1", "Vers 1", 0),
      slide("song-1", "Refreng", 1),
    ]);
    expect(matchSequence("v", item, 0)?.label).toBe("Vers 1");
    expect(matchSequence("r", item, 0)?.label).toBe("Refreng");
  });

  it("trusts the label's own number over its position", () => {
    // An import that starts numbering at 2 must still answer to V2.
    const item = sections([
      slide("song-1", "Verse 2", 0),
      slide("song-1", "Verse 5", 1),
    ]);
    expect(matchSequence("v2", item, 0)?.index).toBe(0);
    expect(matchSequence("v5", item, 0)?.index).toBe(1);
  });

  it("waits on a digit that could still grow — a 12-verse hymn", () => {
    const item = sections([
      slide("song-1", "Verse 1", 0),
      slide("song-1", "Verse 10", 1),
      slide("song-1", "Verse 12", 2),
    ]);
    expect(matchSequence("v1", item, 0)!.ambiguous).toBe(true);
    expect(matchSequence("v12", item, 0)).toEqual({
      index: 2,
      label: "Vers 12",
      ambiguous: false,
    });
  });

  it("waits when one letter reaches two different section types", () => {
    const item = sections([
      slide("song-1", "Stikk", 0),
      slide("song-1", "Ending", 1),
    ]);
    // Norwegian: "Stikk" and "Slutt" both answer to S.
    const m = matchSequence("s", item, 0)!;
    expect(m.ambiguous).toBe(true);
    expect(m.label).toBe("Stikk");
    expect(matchSequence("sl", item, 0)?.label).toBe("Slutt");
  });

  it("survives a section with no letters in its label at all", () => {
    const item = sections([slide("song-1", "1", 0)]);
    expect(matchSequence("v", item, 0)).toBeNull();
  });
});
