// Platform-correct chord labels. Mac and Windows are first-class equals, so a
// label that is right on one and wrong on the other is a bug in both.
import { describe, expect, it } from "vitest";

import { modChord, modLabel, shiftChord, isApplePlatform } from "./platform";

describe("chord labels", () => {
  it("uses the Apple glyphs on a Mac", () => {
    expect(modChord("L", true)).toBe("⌘L");
    expect(modChord("z", true)).toBe("⌘Z");
    expect(shiftChord("b", true)).toBe("⇧B");
    expect(modLabel(true)).toBe("⌘");
  });

  it("spells the modifier out everywhere else", () => {
    expect(modChord("L", false)).toBe("Ctrl+L");
    expect(modChord("z", false)).toBe("Ctrl+Z");
    expect(shiftChord("b", false)).toBe("Shift+B");
    expect(modLabel(false)).toBe("Ctrl");
  });

  it("leaves multi-character key names alone", () => {
    expect(modChord("Enter", true)).toBe("⌘Enter");
    expect(shiftChord("Escape", false)).toBe("Shift+Escape");
  });
});

describe("platform detection", () => {
  it("never throws, whatever the host reports", () => {
    expect(() => isApplePlatform()).not.toThrow();
    expect(typeof isApplePlatform()).toBe("boolean");
  });

  it("does not mistake jsdom's Node platform token for macOS", () => {
    // jsdom's user agent reads "(darwin)" on a Mac dev machine and "(linux)" on
    // CI. Treating either as Apple would make the printed chords depend on
    // whose machine ran the build.
    expect(isApplePlatform()).toBe(false);
  });
});
