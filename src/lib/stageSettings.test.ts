// A5 — the stage screen's blackout behaviour.
//
// The rule under test is the one that matters on a Sunday: blackout is aimed at
// the room. Unless the church says otherwise, the band keeps the words.
import { describe, expect, it } from "vitest";

import type { LiveFrame } from "@/lib/bindings";
import { stageFrameFor } from "./stageSettings";

const BLACK: LiveFrame = { kind: "black" };
const LOGO: LiveFrame = { kind: "logo" };
const WORDS: LiveFrame = { kind: "message", text: "Å store Gud" };

describe("stageFrameFor", () => {
  it("keeps the words on stage while the room is blacked out (default)", () => {
    expect(stageFrameFor(BLACK, WORDS, false)).toBe(WORDS);
  });

  it("follows the blackout when the church asked it to", () => {
    expect(stageFrameFor(BLACK, WORDS, true)).toBe(BLACK);
  });

  it("mirrors the main output for everything that is not a blackout", () => {
    for (const followsBlackout of [true, false]) {
      expect(stageFrameFor(WORDS, LOGO, followsBlackout)).toBe(WORDS);
      expect(stageFrameFor(LOGO, WORDS, followsBlackout)).toBe(LOGO);
    }
  });

  it("stays black when the cue itself is a blackout cue", () => {
    // Nothing is being hidden — the plan says black here, so the stage shows
    // black too even though it does not follow overrides.
    expect(stageFrameFor(BLACK, BLACK, false)).toBe(BLACK);
  });

  it("falls back to the main output when there is no cue to fall back to", () => {
    expect(stageFrameFor(BLACK, null, false)).toBe(BLACK);
  });

  it("has nothing to show before a session starts", () => {
    expect(stageFrameFor(null, WORDS, false)).toBeNull();
    expect(stageFrameFor(null, null, true)).toBeNull();
  });
});
