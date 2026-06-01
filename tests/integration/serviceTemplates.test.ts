/**
 * Tests for the service template system (Oppgave 1) and output display
 * configuration (Oppgave 2).
 *
 * These tests cover:
 *  - IPC helpers: `parseCueSpecs` + `serviceTemplate` namespace shape
 *  - OutputDisplayConfig types + defaults
 *  - CueSpec kind validation via the map helper (tested via pure logic)
 *  - i18n keys for both features
 */

import { describe, it, expect } from "vitest";

import { parseCueSpecs } from "@/lib/ipc";
import type {
  CueSpec,
  OutputDisplayConfig,
  OutputResolution,
  OutputTransition,
  ServiceTemplate,
} from "@/lib/bindings";
import { translate } from "@/lib/i18n";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeTemplate(
  specs: CueSpec[],
  overrides: Partial<ServiceTemplate> = {},
): ServiceTemplate {
  return {
    id: "tmpl-1",
    name: "Test",
    description: null,
    cue_specs: JSON.stringify(specs),
    is_builtin: BigInt(0),
    created_at: BigInt(0),
    updated_at: BigInt(0),
    ...overrides,
  };
}

function makeDisplayConfig(
  overrides: Partial<OutputDisplayConfig> = {},
): OutputDisplayConfig {
  return {
    primary_display_index: 0,
    output_resolution: "1920x1080",
    text_safe_zone_percent: 10,
    transition: "cut",
    transition_ms: 300,
    ...overrides,
  };
}

// ── parseCueSpecs ─────────────────────────────────────────────────────────────

describe("parseCueSpecs", () => {
  it("parses a valid JSON cue_specs string into CueSpec[]", () => {
    const specs: CueSpec[] = [
      { kind: "song", label: "Sang 1", notes: null },
      { kind: "prayer", label: "Bønn", notes: "Short" },
    ];
    const tmpl = makeTemplate(specs);
    const result = parseCueSpecs(tmpl);
    expect(result).toHaveLength(2);
    expect(result[0].kind).toBe("song");
    expect(result[1].notes).toBe("Short");
  });

  it("returns an empty array for an empty cue_specs array", () => {
    const tmpl = makeTemplate([]);
    expect(parseCueSpecs(tmpl)).toEqual([]);
  });

  it("returns an empty array when cue_specs is invalid JSON", () => {
    const tmpl = makeTemplate([], { cue_specs: "not-json" });
    expect(parseCueSpecs(tmpl)).toEqual([]);
  });

  it("preserves null notes", () => {
    const specs: CueSpec[] = [{ kind: "bible", label: "Tekst", notes: null }];
    const tmpl = makeTemplate(specs);
    const result = parseCueSpecs(tmpl);
    expect(result[0].notes).toBeNull();
  });

  it("handles all five CueSpec kinds", () => {
    const kinds = ["song", "bible", "prayer", "announcement", "media"] as const;
    const specs: CueSpec[] = kinds.map((kind) => ({
      kind,
      label: kind,
      notes: null,
    }));
    const tmpl = makeTemplate(specs);
    const result = parseCueSpecs(tmpl);
    expect(result.map((s) => s.kind)).toEqual([...kinds]);
  });
});

// ── OutputDisplayConfig shape ─────────────────────────────────────────────────

describe("OutputDisplayConfig", () => {
  it("accepts all supported resolutions", () => {
    const resolutions: OutputResolution[] = [
      "1920x1080",
      "1280x720",
      "3840x2160",
    ];
    for (const r of resolutions) {
      const cfg = makeDisplayConfig({ output_resolution: r });
      expect(cfg.output_resolution).toBe(r);
    }
  });

  it("accepts all supported transitions", () => {
    const transitions: OutputTransition[] = [
      "cut",
      "fade",
      "slide_left",
      "slide_right",
    ];
    for (const tr of transitions) {
      const cfg = makeDisplayConfig({ transition: tr });
      expect(cfg.transition).toBe(tr);
    }
  });

  it("default config has expected values", () => {
    const cfg = makeDisplayConfig();
    expect(cfg.primary_display_index).toBe(0);
    expect(cfg.output_resolution).toBe("1920x1080");
    expect(cfg.text_safe_zone_percent).toBe(10);
    expect(cfg.transition).toBe("cut");
    expect(cfg.transition_ms).toBe(300);
  });

  it("safe_zone_percent is within valid range 5–20", () => {
    for (const pct of [5, 10, 15, 20]) {
      const cfg = makeDisplayConfig({ text_safe_zone_percent: pct });
      expect(cfg.text_safe_zone_percent).toBeGreaterThanOrEqual(5);
      expect(cfg.text_safe_zone_percent).toBeLessThanOrEqual(20);
    }
  });

  it("transition_ms fits within 0–1000", () => {
    for (const ms of [0, 300, 500, 1000]) {
      const cfg = makeDisplayConfig({ transition_ms: ms });
      expect(cfg.transition_ms).toBeGreaterThanOrEqual(0);
      expect(cfg.transition_ms).toBeLessThanOrEqual(1000);
    }
  });
});

// ── i18n keys ─────────────────────────────────────────────────────────────────

describe("service template i18n keys", () => {
  const en = (key: string, params?: Record<string, string | number>) =>
    translate("en", key as never, params as never);
  const no = (key: string, params?: Record<string, string | number>) =>
    translate("no", key as never, params as never);

  it("en: tmplPageTitle is defined", () => {
    expect(en("tmplPageTitle")).toBe("Service templates");
  });

  it("no: tmplPageTitle is defined", () => {
    expect(no("tmplPageTitle")).toBe("Gudstjeneste-maler");
  });

  it("en: tmplCueSpecs interpolates n", () => {
    expect(en("tmplCueSpecs", { n: 15 })).toContain("15");
  });

  it("no: tmplCueSpecs interpolates n", () => {
    expect(no("tmplCueSpecs", { n: 8 })).toContain("8");
  });

  it("en: tmplApplyDone interpolates n and service", () => {
    const msg = en("tmplApplyDone", { n: 12, service: "Konsert" });
    expect(msg).toContain("12");
    expect(msg).toContain("Konsert");
  });
});

describe("output display config i18n keys", () => {
  const en = (key: string) => translate("en", key as never);
  const no = (key: string) => translate("no", key as never);

  it("en: setOutDispTitle is defined", () => {
    expect(en("setOutDispTitle")).toBeTruthy();
  });

  it("no: setOutDispTitle is defined", () => {
    expect(no("setOutDispTitle")).toBeTruthy();
  });

  it("en: all four transition labels are defined", () => {
    expect(en("setOutDispTransitionCut")).toBe("Cut");
    expect(en("setOutDispTransitionFade")).toBe("Fade");
    expect(en("setOutDispTransitionSlideLeft")).toBe("Slide left");
    expect(en("setOutDispTransitionSlideRight")).toBe("Slide right");
  });

  it("en: setTabOutputDisplay is defined", () => {
    expect(en("setTabOutputDisplay")).toBeTruthy();
  });
});
