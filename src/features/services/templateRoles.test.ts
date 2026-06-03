/**
 * Pure-logic tests for per-template stage-display role assignment (Phase 8).
 *
 * Covers the role → panel mapping (the data driving the live preview) and the
 * localStorage persistence round-trip + tolerant parsing. No backend / DOM
 * beyond jsdom's localStorage.
 */
import { describe, it, expect, beforeEach } from "vitest";

import {
  DEFAULT_ROLE,
  TEMPLATE_ROLES,
  getTemplateRole,
  panelsForRole,
  parseRoleMap,
  roleLabelKey,
  setTemplateRole,
  type TemplateRole,
} from "./templateRoles";

describe("panelsForRole", () => {
  it("worship leader sees every panel", () => {
    const p = panelsForRole("worship-leader");
    expect(Object.values(p).every(Boolean)).toBe(true);
  });

  it("musician shows the section label but hides clock/timer/notes", () => {
    const p = panelsForRole("musician");
    expect(p.showSectionLabel).toBe(true);
    expect(p.showClock).toBe(false);
    expect(p.showServiceTimer).toBe(false);
    expect(p.showNotes).toBe(false);
  });

  it("operator keeps timing + notes but not large lyrics", () => {
    const p = panelsForRole("operator");
    expect(p.showServiceTimer).toBe(true);
    expect(p.showNotes).toBe(true);
    expect(p.lyricsLarge).toBe(false);
  });

  it("congregation shows only the current slide", () => {
    const p = panelsForRole("congregation");
    expect(p.showCurrentSlide).toBe(true);
    expect(p.showNextSlide).toBe(false);
    expect(p.showSectionLabel).toBe(false);
    expect(p.showClock).toBe(false);
    expect(p.showNotes).toBe(false);
  });

  it("each role produces a distinct panel signature", () => {
    const sigs = TEMPLATE_ROLES.map((r) => JSON.stringify(panelsForRole(r)));
    expect(new Set(sigs).size).toBe(TEMPLATE_ROLES.length);
  });
});

describe("roleLabelKey", () => {
  it("returns a unique i18n key per role", () => {
    const keys = TEMPLATE_ROLES.map(roleLabelKey);
    expect(new Set(keys).size).toBe(TEMPLATE_ROLES.length);
    expect(keys.every((k) => k.startsWith("tmplRole"))).toBe(true);
  });
});

describe("parseRoleMap", () => {
  it("returns empty for null / invalid JSON", () => {
    expect(parseRoleMap(null)).toEqual({});
    expect(parseRoleMap("not-json")).toEqual({});
    expect(parseRoleMap("123")).toEqual({});
  });

  it("keeps only valid role values", () => {
    const raw = JSON.stringify({
      "tmpl-1": "musician",
      "tmpl-2": "bogus",
      "tmpl-3": "operator",
    });
    expect(parseRoleMap(raw)).toEqual({
      "tmpl-1": "musician",
      "tmpl-3": "operator",
    });
  });
});

describe("localStorage persistence", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("defaults to worship-leader when unassigned", () => {
    expect(getTemplateRole("tmpl-x")).toBe(DEFAULT_ROLE);
    expect(DEFAULT_ROLE).toBe("worship-leader");
  });

  it("persists an assignment and reads it back (survives a reload)", () => {
    setTemplateRole("tmpl-x", "musician");
    // Fresh read — simulates a page reload reading from localStorage.
    expect(getTemplateRole("tmpl-x")).toBe("musician");
  });

  it("keeps assignments independent per template", () => {
    setTemplateRole("a", "operator");
    setTemplateRole("b", "congregation");
    expect(getTemplateRole("a")).toBe("operator");
    expect(getTemplateRole("b")).toBe("congregation");
    expect(getTemplateRole("c")).toBe(DEFAULT_ROLE);
  });

  it("overwrites an existing assignment", () => {
    const seq: TemplateRole[] = ["musician", "operator", "congregation"];
    for (const r of seq) setTemplateRole("a", r);
    expect(getTemplateRole("a")).toBe("congregation");
  });
});
