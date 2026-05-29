/**
 * i18n — Phase 13.1.
 *
 * A tiny, dependency-light translation layer: a per-language catalog, a `t()`
 * that falls back to English for any missing key, and a persisted locale store.
 * English + Norwegian are complete; the other five locales (matching SundayRec:
 * sv, da, de, fr, pl) ship the navigation/chrome strings and fall back to
 * English for the rest — full translation is tracked in docs/LAUNCH_READINESS.md.
 *
 * The machinery is what matters here: every user-visible string can go through
 * `t()`, and adding a language is just another catalog entry.
 */

import { create } from "zustand";

export type Lang = "no" | "en" | "sv" | "da" | "de" | "fr" | "pl";

export const LANGS: Lang[] = ["no", "en", "sv", "da", "de", "fr", "pl"];

type Catalog = Record<string, string>;

const en: Catalog = {
  appTagline: "Live Presentation",
  navDashboard: "Dashboard",
  navLibrary: "Library",
  navDecks: "Decks",
  navServices: "Services",
  navBible: "Bible",
  navMedia: "Media",
  navSettings: "Settings",
  goLive: "Go live",
  loadingLibrary: "Loading library…",
  welcomeTitle: "Welcome to SundayStage",
  welcomeIntro:
    "Pick your language and we'll prepare a demo service so you have something to play with right away.",
  pickLanguage: "Language",
  seedDemo: "Add demo content",
  skip: "Start empty",
  seeding: "Preparing…",
  syncLocalOnly: "Local",
  syncSynced: "Synced",
  syncSyncing: "Syncing…",
  syncOffline: "Offline",
  syncConflict: "Conflict",
  syncPausedLive: "Paused (live)",
};

const no: Catalog = {
  appTagline: "Live Presentasjon",
  navDashboard: "Dashbord",
  navLibrary: "Bibliotek",
  navDecks: "Decks",
  navServices: "Gudstjenester",
  navBible: "Bibel",
  navMedia: "Media",
  navSettings: "Innstillinger",
  goLive: "Gå live",
  loadingLibrary: "Laster bibliotek…",
  welcomeTitle: "Velkommen til SundayStage",
  welcomeIntro:
    "Velg språk, så lager vi en demo-gudstjeneste du kan leke med med en gang.",
  pickLanguage: "Språk",
  seedDemo: "Legg til demo-innhold",
  skip: "Start tomt",
  seeding: "Forbereder…",
  syncLocalOnly: "Lokal",
  syncSynced: "Synkronisert",
  syncSyncing: "Synkroniserer…",
  syncOffline: "Frakoblet",
  syncConflict: "Konflikt",
  syncPausedLive: "Pauset (live)",
};

// Scandinavian + de/fr/pl: nav/chrome only; everything else falls back to en.
const sv: Catalog = {
  appTagline: "Live-presentation",
  navDashboard: "Översikt",
  navLibrary: "Bibliotek",
  navDecks: "Decks",
  navServices: "Gudstjänster",
  navBible: "Bibel",
  navMedia: "Media",
  navSettings: "Inställningar",
  goLive: "Gå live",
};
const da: Catalog = {
  appTagline: "Live-præsentation",
  navDashboard: "Oversigt",
  navLibrary: "Bibliotek",
  navDecks: "Decks",
  navServices: "Gudstjenester",
  navBible: "Bibel",
  navMedia: "Medier",
  navSettings: "Indstillinger",
  goLive: "Gå live",
};
const de: Catalog = {
  appTagline: "Live-Präsentation",
  navDashboard: "Übersicht",
  navLibrary: "Bibliothek",
  navDecks: "Decks",
  navServices: "Gottesdienste",
  navBible: "Bibel",
  navMedia: "Medien",
  navSettings: "Einstellungen",
  goLive: "Live gehen",
};
const fr: Catalog = {
  appTagline: "Présentation en direct",
  navDashboard: "Tableau de bord",
  navLibrary: "Bibliothèque",
  navDecks: "Decks",
  navServices: "Services",
  navBible: "Bible",
  navMedia: "Médias",
  navSettings: "Paramètres",
  goLive: "Passer en direct",
};
const pl: Catalog = {
  appTagline: "Prezentacja na żywo",
  navDashboard: "Pulpit",
  navLibrary: "Biblioteka",
  navDecks: "Decks",
  navServices: "Nabożeństwa",
  navBible: "Biblia",
  navMedia: "Media",
  navSettings: "Ustawienia",
  goLive: "Na żywo",
};

const CATALOG: Record<Lang, Catalog> = { en, no, sv, da, de, fr, pl };

export type TKey = keyof typeof en;

export function translate(lang: Lang, key: TKey): string {
  return CATALOG[lang]?.[key] ?? en[key] ?? key;
}

// ── Persisted locale store ─────────────────────────────────────────────────────

const STORAGE_KEY = "ss-locale";

function initialLang(): Lang {
  try {
    const saved = localStorage.getItem(STORAGE_KEY) as Lang | null;
    if (saved && LANGS.includes(saved)) return saved;
  } catch {
    /* localStorage may be unavailable */
  }
  return "no";
}

interface LocaleState {
  lang: Lang;
  setLang: (lang: Lang) => void;
}

export const useLocale = create<LocaleState>((set) => ({
  lang: initialLang(),
  setLang: (lang) => {
    try {
      localStorage.setItem(STORAGE_KEY, lang);
    } catch {
      /* ignore */
    }
    set({ lang });
  },
}));

/** Hook returning a `t` bound to the current locale. */
export function useT(): (key: TKey) => string {
  const lang = useLocale((s) => s.lang);
  return (key) => translate(lang, key);
}
