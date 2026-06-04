/**
 * Pure-logic tests for theme CRUD helpers (headless-1).
 *
 * Naming/validation only — no IPC or DOM.
 */
import { describe, it, expect } from "vitest";

import { uniqueThemeName, cleanThemeName } from "./themeActions";

describe("uniqueThemeName", () => {
  it("returns the base when no collision", () => {
    expect(uniqueThemeName([], "New theme")).toBe("New theme");
    expect(uniqueThemeName(["Sunday", "Easter"], "New theme")).toBe(
      "New theme",
    );
  });

  it("appends a counter on collision", () => {
    expect(uniqueThemeName(["New theme"], "New theme")).toBe("New theme 2");
    expect(uniqueThemeName(["New theme", "New theme 2"], "New theme")).toBe(
      "New theme 3",
    );
  });

  it("compares names case-insensitively and trims", () => {
    expect(uniqueThemeName(["  NEW THEME "], "New theme")).toBe("New theme 2");
  });

  it("skips gaps in the existing numbering", () => {
    // "New theme 2" missing -> first free slot is 2.
    expect(uniqueThemeName(["New theme", "New theme 3"], "New theme")).toBe(
      "New theme 2",
    );
  });
});

describe("cleanThemeName", () => {
  it("trims and accepts a non-empty name", () => {
    expect(cleanThemeName("  My theme ")).toBe("My theme");
  });

  it("treats empty/whitespace/null as cancel", () => {
    expect(cleanThemeName("")).toBeNull();
    expect(cleanThemeName("   ")).toBeNull();
    expect(cleanThemeName(null)).toBeNull();
    expect(cleanThemeName(undefined)).toBeNull();
  });
});
