/**
 * i18n parity tests (headless-1).
 *
 * The theme-controls keys are user-visible in every supported locale, so each
 * one must be present (and non-empty) in all catalogs — a fall-back to English
 * would leak an untranslated string into the operator UI.
 */
import { describe, it, expect } from "vitest";

import { CATALOG, LANGS, type Lang } from "./i18n";

// Keys that drive the theme CRUD panel (ThemeControls). Localising these is the
// point of this change, so we hard-require full parity rather than allowing the
// generic English fall-back.
const THEME_CRUD_KEYS = [
  "tcNewTheme",
  "tcNewThemeName",
  "tcNewThemePrompt",
  "tcRenamePrompt",
  "tcRenameTitle",
  "tcDeleteTitle",
  "tcDeleteConfirm",
  "tcSetDefaultTemplateTitle",
] as const;

describe("theme-controls i18n parity", () => {
  for (const lang of LANGS) {
    it(`${lang} carries every theme CRUD key`, () => {
      const cat = CATALOG[lang];
      for (const key of THEME_CRUD_KEYS) {
        expect(cat[key], `${lang}.${key}`).toBeTruthy();
        expect(cat[key].trim().length, `${lang}.${key}`).toBeGreaterThan(0);
      }
    });
  }

  it("tcDeleteConfirm keeps the {name} placeholder in every locale", () => {
    for (const lang of LANGS) {
      expect(CATALOG[lang].tcDeleteConfirm, lang).toContain("{name}");
    }
  });
});

// The template gallery (deep-stage-2) is operator-facing in every locale, so
// its strings must have full parity rather than leaking the English fall-back.
const GALLERY_KEYS = [
  "galBrowse",
  "galOpenTitle",
  "galTitle",
  "galSearch",
  "galEmpty",
  "galBuiltins",
  "galCustom",
  "galApplying",
  "galApplyTitle",
] as const;

describe("template-gallery i18n parity", () => {
  for (const lang of LANGS) {
    it(`${lang} carries every gallery key`, () => {
      const cat = CATALOG[lang];
      for (const key of GALLERY_KEYS) {
        expect(cat[key], `${lang}.${key}`).toBeTruthy();
        expect(cat[key].trim().length, `${lang}.${key}`).toBeGreaterThan(0);
      }
    });
  }

  it("galApplyTitle keeps the {name} placeholder in every locale", () => {
    for (const lang of LANGS) {
      expect(CATALOG[lang].galApplyTitle, lang).toContain("{name}");
    }
  });
});

// The plan-preview flow (paste a SundayPlan → preview cue list) is fully
// operator-facing in every locale, so its strings require full parity rather
// than leaking the English fall-back.
const PLAN_PREVIEW_KEYS = [
  "planPreviewButton",
  "planPreviewTitle",
  "planPreviewDescription",
  "planPreviewPasteLabel",
  "planPreviewPastePlaceholder",
  "planPreviewBuild",
  "planPreviewBuilding",
  "planPreviewInvalidJson",
  "planPreviewNoItems",
  "planPreviewCueCountOne",
  "planPreviewCueCountMany",
  "planPreviewFallbacks",
  "planPreviewFallbackBadge",
  "planPreviewFallbackHint",
] as const;

describe("plan-preview i18n parity", () => {
  for (const lang of LANGS) {
    it(`${lang} carries every plan-preview key`, () => {
      const cat = CATALOG[lang];
      for (const key of PLAN_PREVIEW_KEYS) {
        expect(cat[key], `${lang}.${key}`).toBeTruthy();
        expect(cat[key].trim().length, `${lang}.${key}`).toBeGreaterThan(0);
      }
    });
  }

  it("placeholder-bearing keys keep their {error}/{n} tokens in every locale", () => {
    for (const lang of LANGS) {
      expect(CATALOG[lang].planPreviewInvalidJson, lang).toContain("{error}");
      expect(CATALOG[lang].planPreviewCueCountOne, lang).toContain("{n}");
      expect(CATALOG[lang].planPreviewCueCountMany, lang).toContain("{n}");
      expect(CATALOG[lang].planPreviewFallbacks, lang).toContain("{n}");
    }
  });
});

// The settings-save error banner (headless-2) is the only signal an operator
// gets when a disk write fails, so it must be fully localized — falling back to
// English here would be a confusing mid-Sunday surprise.
describe("settings save-error i18n parity", () => {
  for (const lang of LANGS) {
    it(`${lang} carries a non-empty setSaveFailed`, () => {
      const cat = CATALOG[lang];
      expect(cat.setSaveFailed, `${lang}.setSaveFailed`).toBeTruthy();
      expect(
        cat.setSaveFailed.trim().length,
        `${lang}.setSaveFailed`,
      ).toBeGreaterThan(0);
    });
  }
});

// ── Whole-catalog parity ──────────────────────────────────────────────────────
//
// The targeted suites above guard individual feature areas. This suite enforces
// the global invariant: every locale carries *exactly* English's key set (no
// missing keys → no silent English fall-back; no extra keys → no dead strings),
// and every value preserves *exactly* English's `{placeholder}` tokens per key.
// English (`en`) is the source of truth and the runtime fall-back.

const en = CATALOG.en;
const enKeys = Object.keys(en).sort();
const enKeySet = new Set(enKeys);

/** Extract the set of `{token}` placeholders from a catalog string. */
function placeholders(value: string): Set<string> {
  const out = new Set<string>();
  const re = /\{(\w+)\}/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(value)) !== null) out.add(m[1]);
  return out;
}

const otherLangs = LANGS.filter((l) => l !== "en");

describe("whole-catalog i18n parity", () => {
  it("LANGS and CATALOG agree on the locale set", () => {
    expect(Object.keys(CATALOG).sort()).toEqual([...LANGS].sort());
  });

  it("English has a non-trivial number of keys", () => {
    // Sanity floor so the parity checks below can't pass against an empty `en`.
    expect(enKeys.length).toBeGreaterThan(400);
  });

  for (const lang of otherLangs as Lang[]) {
    describe(`locale ${lang}`, () => {
      const cat = CATALOG[lang];
      const keys = Object.keys(cat).sort();
      const keySet = new Set(keys);

      it("has exactly the same key set as en", () => {
        const missing = enKeys.filter((k) => !keySet.has(k));
        const extra = keys.filter((k) => !enKeySet.has(k));
        expect({ missing, extra }).toEqual({ missing: [], extra: [] });
      });

      it("preserves the {placeholder} tokens of every key", () => {
        const mismatches: Record<string, { en: string[]; got: string[] }> = {};
        for (const key of enKeys) {
          if (!keySet.has(key)) continue; // key-set test already reports this
          const want = [...placeholders(en[key])].sort();
          const got = [...placeholders(cat[key])].sort();
          if (want.join("|") !== got.join("|")) {
            mismatches[key] = { en: want, got };
          }
        }
        expect(mismatches).toEqual({});
      });
    });
  }
});
