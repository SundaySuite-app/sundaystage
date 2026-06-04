/**
 * i18n parity tests (headless-1).
 *
 * The theme-controls keys are user-visible in every supported locale, so each
 * one must be present (and non-empty) in all catalogs — a fall-back to English
 * would leak an untranslated string into the operator UI.
 */
import { describe, it, expect } from "vitest";

import { CATALOG, LANGS } from "./i18n";

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
