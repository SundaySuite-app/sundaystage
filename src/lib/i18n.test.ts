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

// The update-channel card (E2) decides whether a machine gets beta builds. An
// English fall-back here would leave an operator guessing about what they are
// opting into, so full parity is required — and the two ring labels must stay
// distinguishable in every locale.
const UPDATE_CHANNEL_KEYS = [
  "setUpdateChannel",
  "setUpdateChannelDesc",
  "setUpdateChannelCurrent",
  "setChannelStable",
  "setChannelBeta",
  "setBetaUpdates",
  "setBetaUpdatesDesc",
] as const;

describe("update-channel i18n parity", () => {
  for (const lang of LANGS) {
    it(`${lang} carries every update-channel key`, () => {
      const cat = CATALOG[lang];
      for (const key of UPDATE_CHANNEL_KEYS) {
        expect(cat[key], `${lang}.${key}`).toBeTruthy();
        expect(cat[key].trim().length, `${lang}.${key}`).toBeGreaterThan(0);
      }
    });
  }

  it("stable and beta never render as the same label", () => {
    for (const lang of LANGS) {
      expect(CATALOG[lang].setChannelStable, lang).not.toBe(
        CATALOG[lang].setChannelBeta,
      );
    }
  });

  it("the beta description is honest about the trade-off", () => {
    // Not a wording test — a length floor, so the toggle can never ship with a
    // one-word description that hides the "may have rough edges" part.
    for (const lang of LANGS) {
      expect(CATALOG[lang].setBetaUpdatesDesc.length, lang).toBeGreaterThan(40);
    }
  });
});

// The telemetry consent question (E6) is the one string in the app that asks
// permission. An English fall-back here would mean an operator agreeing to
// something they were not asked in their own language, so it gets its own suite
// on top of the whole-catalog check below — including floors on the two lines
// the promise actually rests on.
const CONSENT_KEYS = [
  "telConsentTitle",
  "telConsentBody",
  "telConsentWhatIsSent",
  "telConsentCatCrashes",
  "telConsentCatQuality",
  "telConsentCatUsage",
  "telConsentNever",
  "telConsentYes",
  "telConsentNo",
  "telConsentPrivacyLink",
  "telConsentDismissLabel",
] as const;

describe("telemetry consent i18n parity", () => {
  for (const lang of LANGS) {
    it(`${lang} carries every consent key`, () => {
      const cat = CATALOG[lang];
      for (const key of CONSENT_KEYS) {
        expect(cat[key], `${lang}.${key}`).toBeTruthy();
        expect(cat[key].trim().length, `${lang}.${key}`).toBeGreaterThan(0);
      }
    });
  }

  it("the yes and no buttons are never the same words", () => {
    // They are deliberately equally weighted in the UI, which only works if
    // they are also distinguishable.
    for (const lang of LANGS) {
      expect(CATALOG[lang].telConsentYes, lang).not.toBe(
        CATALOG[lang].telConsentNo,
      );
    }
  });

  it("the 'never' line stays long enough to name what is never sent", () => {
    // Not a wording test — a floor, so the promise cannot quietly shrink to
    // "we respect your privacy" in a locale nobody on the team reads.
    for (const lang of LANGS) {
      expect(CATALOG[lang].telConsentNever.length, lang).toBeGreaterThan(80);
      expect(CATALOG[lang].telConsentBody.length, lang).toBeGreaterThan(80);
    }
  });
});

// The five report outcomes are the app's only answer to "did my words reach
// anyone". Each one has to say something DIFFERENT in every locale, or the
// honest distinction between "sent" and "saved, but this build sends nothing"
// collapses into a single reassuring sentence.
const REPORT_OUTCOME_KEYS = [
  "reportOutcomeQueued",
  "reportOutcomeSent",
  "reportOutcomeDeferredLive",
  "reportOutcomeDeferredOffline",
  "reportOutcomeNoEndpoint",
] as const;

describe("problem-report outcome i18n parity", () => {
  for (const lang of LANGS) {
    it(`${lang} distinguishes all five outcomes`, () => {
      const cat = CATALOG[lang];
      const seen = new Set<string>();
      for (const key of REPORT_OUTCOME_KEYS) {
        expect(cat[key], `${lang}.${key}`).toBeTruthy();
        seen.add(cat[key]);
      }
      expect(seen.size, `${lang} reuses an outcome sentence`).toBe(
        REPORT_OUTCOME_KEYS.length,
      );
    });
  }
});

// The Bible-download card (Spor C) is operator-facing in every locale, and the
// two placeholder-bearing strings must keep their tokens or the size label and
// error message break. Full parity, not the English fall-back.
const BIBLE_DOWNLOAD_KEYS = [
  "bibleDlTitle",
  "bibleDlDesc",
  "bibleDlInstalled",
  "bibleDlDownload",
  "bibleDlRedownload",
  "bibleDlDownloading",
  "bibleDlVerifying",
  "bibleDlInstalling",
  "bibleDlSizeMb",
  "bibleDlFailed",
] as const;

describe("bible-download i18n parity", () => {
  for (const lang of LANGS) {
    it(`${lang} carries every bible-download key`, () => {
      const cat = CATALOG[lang];
      for (const key of BIBLE_DOWNLOAD_KEYS) {
        expect(cat[key], `${lang}.${key}`).toBeTruthy();
        expect(cat[key].trim().length, `${lang}.${key}`).toBeGreaterThan(0);
      }
    });
  }

  it("keeps the {mb} and {error} placeholders in every locale", () => {
    for (const lang of LANGS) {
      expect(CATALOG[lang].bibleDlSizeMb, lang).toContain("{mb}");
      expect(CATALOG[lang].bibleDlFailed, lang).toContain("{error}");
    }
  });
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
