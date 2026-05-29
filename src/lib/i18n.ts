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

  // ── Common actions (reused everywhere) ──────────────────────────────────
  actionCancel: "Cancel",
  actionClose: "Close",
  actionSave: "Save",
  actionDelete: "Delete",
  actionEdit: "Edit",
  actionAdd: "Add",
  actionDone: "Done",
  actionNext: "Next",
  actionBack: "Back",
  actionSearch: "Search",
  actionSkip: "Skip",

  // ── Result groups (command palette, search) ─────────────────────────────
  groupSongs: "Songs",
  groupBible: "Bible",
  groupServices: "Services",

  // ── Tutorial overlay ────────────────────────────────────────────────────
  tutLibraryTitle: "The song library",
  tutLibraryBody:
    "All your songs live here. Search lyric lines, filter by language or licence, and see a preview on the right. We've added a small starter library for you to play with.",
  tutEditTitle: "Edit a song",
  tutEditBody:
    "Double-click a song to open the editor — split it into verses/choruses and build arrangements. Slides are generated automatically from the sections.",
  tutAiTitle: "Let AI do the boring work",
  tutAiBody:
    "Paste raw lyrics and press «Format» — AI structures verses, chorus and arrangement. Without an API key it formats locally. Add a key under Settings.",
  tutLiveTitle: "Go live",
  tutLiveBody:
    "Press «Go live» at the bottom left. Run the cue list with the arrow keys; Esc = blackout, L = logo. Connect a projector under «Screens».",
  tutSearchTitle: "Search everywhere with ⌘K",
  tutSearchBody:
    "Press ⌘K anywhere to jump between pages or search across songs, Bible verses and services.",

  // ── AI consent dialog ───────────────────────────────────────────────────
  consentTitle: "Use AI features?",
  consentDescription:
    "AI features send content to Anthropic (Claude) for processing.",
  consentAccept: "Accept and continue",
  consentIntro:
    "When you use an AI feature, the following is sent to Anthropic:",
  consentBullet1:
    "the text you ask to have processed (e.g. pasted lyrics or a planning description)",
  consentBullet2:
    "no songs, services or media beyond what the individual action needs",
  consentNote:
    "Your API key is stored in the system keychain, never in plaintext. Features with a local fallback (such as lyric formatting) work without AI. You can withdraw consent in Settings.",

  // ── Update banner ───────────────────────────────────────────────────────
  updateAvailable: "New version available",
  updateBody: "Download and restart to update SundayStage.",
  updateInProgress: "Updating…",
  updateDownload: "Download and restart",

  // ── Command palette ─────────────────────────────────────────────────────
  cmdPaletteLabel: "Command palette",
  cmdSearchPlaceholder:
    "Search for songs, Bible verses, services — or type a command…",
  cmdSearching: "Searching…",
  cmdNoHits: "No matches for «{q}».",
  cmdGroupNavigate: "Navigate",
  cmdGroupActions: "Actions",
  cmdGroupDeveloper: "Developer",
  cmdSongLibrary: "Song library",
  cmdNewSong: "New song…",
  cmdNewService: "New service…",
  cmdDesignSystem: "Design system",

  // ── Theme toggle ────────────────────────────────────────────────────────
  themeLabel: "Theme",
  themeSystem: "System",
  themeLight: "Light",
  themeDark: "Dark",

  // ── More common actions ─────────────────────────────────────────────────
  actionNew: "New",
  actionDuplicate: "Duplicate",
  dragReorder: "Drag to reorder",
  deletedMarker: "(deleted)",
  previewLabel: "Preview",
  loadingShort: "Loading…",

  // ── Service-item kinds (reused) ─────────────────────────────────────────
  kindSong: "Song",
  kindScripture: "Scripture",
  kindNote: "Note",

  // ── Plan-with-AI modal ──────────────────────────────────────────────────
  planTitle: "Plan a service with AI",
  planBriefPlaceholder:
    "E.g.: 25 min of worship about forgiveness for a youth service, calm ending.",
  planApiKeyRequired: "Anthropic API key (required)",
  planThinking: "Thinking…",
  planSuggest: "Suggest a plan",
  planEmptyHint:
    "Describe the service above and AI will suggest songs from your library, readings and transitions.",
  planThemePrefix: "Theme: {theme}",
  planCreating: "Creating…",
  planCreateService: "Create service",

  // ── Paste & format modal ────────────────────────────────────────────────
  pasteTitle: "Paste & format with AI",
  pasteRawPlaceholder:
    "Paste text from anywhere — chord lines, «x2», and odd formatting are fine.",
  pasteApiKeyOptional:
    "Anthropic API key (optional — without it, formatting runs locally)",
  pasteFormatting: "Formatting…",
  pasteFormat: "Format",
  pasteUsingStoredKey:
    "Uses the stored key from Settings if the field is empty.",
  pasteNoKeyHint:
    "Without a key, formatting runs locally. Save a key in Settings for AI.",
  pasteTitleSuggestion: "Title suggestion: ",
  pasteResultHint: "The result appears here after formatting.",
  pasteApplying: "Applying…",
  pasteApply: "Apply to the song",

  // ── Library page ────────────────────────────────────────────────────────
  librarySearchPlaceholder: "Search lyric lines…",
  libraryPlanWithAi: "Plan with AI",
  libraryNewSong: "New song",
  filterAllLanguages: "All languages",
  filterAllLicensing: "All licensing",
  licenseUnknown: "Unknown",
  colTitle: "Title",
  colKey: "Key",
  colTempo: "Tempo",
  colLanguage: "Language",
  colLicense: "Licence",
  libraryNoSongs: "No songs.",
  librarySelectForPreview: "Select a song to preview.",
  libraryNoLyricsYet: "No lyrics yet. Press «Edit» to add sections.",
  libraryEmptyTitle: "Empty library — let's get started",
  libraryEmptyBody:
    "Add your first song manually. Import from ProPresenter, EasyWorship, FreeShow, OpenLP and lyric folders is coming in a later version.",
  libraryCreateFirst: "Create your first song",
  toastServiceCreated: "Service created: {name}",
  newSongDefaultTitle: "New song {time}",
  songCountOne: "{n} song",
  songCountMany: "{n} songs",

  // ── Song editor ─────────────────────────────────────────────────────────
  songSectionsTitle: "Sections",
  songAddSection: "Add section",
  songNoSections: "No sections yet. Add a verse or chorus to start.",
  songDeleteSection: "Delete section",
  songLyricsPlaceholder: "Lyric lines…",
  slideCountOne: "{n} slide",
  slideCountMany: "{n} slides",
  arrTitle: "Arrangement",
  arrNewNamePrompt: "New name",
  arrRename: "Rename",
  arrSetDefault: "Default",
  arrAddSectionTo: "Add a section to the arrangement",
  arrCreateSectionsFirst: "Create sections first.",
  arrSequence: "Sequence",
  arrEmpty: "Empty arrangement. Click a section above to add it.",
  arrNone: "No arrangements yet.",
  arrCreate: "Create arrangement",

  // ── Service-item kinds (extra) ──────────────────────────────────────────
  kindGap: "Break",
  kindAnnouncement: "Announcement",
  kindCustomDeck: "Slides",
  kindVideo: "Video",
  actionPreview: "Preview",

  // ── Services / queue editor ─────────────────────────────────────────────
  svcNewDefaultTitle: "New service {date}",
  svcNewService: "New service",
  svcImporting: "Importing…",
  svcImportFromSundayPlan: "Import from SundayPlan",
  svcListEmpty: "No services yet. Create one or import from SundayPlan.",
  svcSelectOrCreate: "Select or create a service to build the queue.",
  svcDeletedToast: "Service deleted",
  importMatchedSongs: "{n} song(s) matched",
  importCreatedEmpty: "{n} created empty",
  importWarnings: "{n} warning(s)",
  importToast: "Imported «{name}» — {details}",
  importFailed: "Import failed: {error}",
  importReadError: "Could not read the file",
  svcElementCountOne: "{n} item",
  svcElementCountMany: "{n} items",
  svcCuesInQueue: "{n} cues in the queue",
  svcLoadingQueue: "Loading queue…",
  svcNotes: "Notes",
  svcDeleteService: "Delete service",
  svcQueueEmptyTooltip: "The queue is empty — add content first",
  svcGoLiveTooltip: "Go live with this service",
  svcEmptyQueueTitle: "Empty queue",
  svcEmptyQueueBody:
    "Add songs and you'll see exactly which slides each song becomes, and how many cues the queue gets in total.",
  svcAddSong: "Add song",
  svcConfirmDeleteTitle: "Delete this service?",
  svcConfirmDeleteBody:
    "«{name}» is removed from the list. The songs in the library are kept.",
  svcMoveUp: "Move up",
  svcMoveDown: "Move down",
  svcRemoveFromQueue: "Remove from queue",
  svcDefaultAllSections: "Default (all sections)",
  svcKeyPlaceholder: "e.g. G",
  svcLabelText: "Text",
  svcLabelPlaceholder: "e.g. Offering",
  svcClickToRename: "Click to rename",
  svcNotesPlaceholder: "Notes for this service (shown in the live console)…",
  svcSaveNotes: "Save notes",
  svcSearchSongToAdd: "Search for a song to add…",
  svcNoSongsInLibrary: "No songs in the library.",
  svcBackToSearch: "Back to search",
  svcKeyOptional: "Key (optional)",
  svcAddToQueue: "Add to queue",
  svcTranslation: "Translation",
  svcBook: "Book",
  svcChapter: "Chapter",
  svcVerseFrom: "Verse from",
  svcVerseTo: "Verse to",
  svcSelectEllipsis: "Select…",
  svcAll: "all",
  svcCompiling: "Compiling…",
  svcNoSlidesToShow: "No slides to show.",
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

  // ── Common actions (reused everywhere) ──────────────────────────────────
  actionCancel: "Avbryt",
  actionClose: "Lukk",
  actionSave: "Lagre",
  actionDelete: "Slett",
  actionEdit: "Rediger",
  actionAdd: "Legg til",
  actionDone: "Ferdig",
  actionNext: "Neste",
  actionBack: "Tilbake",
  actionSearch: "Søk",
  actionSkip: "Hopp over",

  // ── Result groups (command palette, search) ─────────────────────────────
  groupSongs: "Sanger",
  groupBible: "Bibel",
  groupServices: "Gudstjenester",

  // ── Tutorial overlay ────────────────────────────────────────────────────
  tutLibraryTitle: "Sangbiblioteket",
  tutLibraryBody:
    "Alle sangene dine bor her. Søk i tekstlinjer, filtrer på språk eller lisens, og se forhåndsvisning til høyre. Vi har lagt inn et lite startbibliotek du kan leke med.",
  tutEditTitle: "Rediger en sang",
  tutEditBody:
    "Dobbeltklikk en sang for å åpne editoren — del opp i vers/refreng og bygg arrangementer. Lysbildene genereres automatisk fra seksjonene.",
  tutAiTitle: "La AI gjøre det kjedelige",
  tutAiBody:
    "Lim inn rå lyrikk og trykk «Formater» — AI strukturerer vers, refreng og arrangement. Uten API-nøkkel formateres det lokalt. Legg inn nøkkel under Innstillinger.",
  tutLiveTitle: "Gå live",
  tutLiveBody:
    "Trykk «Gå live» nede til venstre. Cue-listen kjøres med piltastene; Esc = blackout, L = logo. Koble til en projektor under «Skjermer».",
  tutSearchTitle: "Søk overalt med ⌘K",
  tutSearchBody:
    "Trykk ⌘K hvor som helst for å hoppe mellom sider eller søke på tvers av sanger, bibelvers og tjenester.",

  // ── AI consent dialog ───────────────────────────────────────────────────
  consentTitle: "Bruke AI-funksjoner?",
  consentDescription:
    "AI-funksjoner sender innhold til Anthropic (Claude) for behandling.",
  consentAccept: "Godta og fortsett",
  consentIntro: "Når du bruker en AI-funksjon, sendes følgende til Anthropic:",
  consentBullet1:
    "teksten du ber om å få behandlet (f.eks. limt-inn lyrikk eller en planleggings-beskrivelse)",
  consentBullet2:
    "ingen sanger, tjenester eller medier utover det den enkelte handlingen trenger",
  consentNote:
    "API-nøkkelen din lagres i systemets nøkkelring, aldri i klartekst. Funksjoner med lokal fallback (som lyrikkformatering) virker uten AI. Du kan trekke samtykket tilbake i Innstillinger.",

  // ── Update banner ───────────────────────────────────────────────────────
  updateAvailable: "Ny versjon tilgjengelig",
  updateBody: "Last ned og start på nytt for å oppdatere SundayStage.",
  updateInProgress: "Oppdaterer…",
  updateDownload: "Last ned og start på nytt",

  // ── Command palette ─────────────────────────────────────────────────────
  cmdPaletteLabel: "Kommandopalett",
  cmdSearchPlaceholder:
    "Søk etter sanger, bibelvers, tjenester — eller skriv en kommando…",
  cmdSearching: "Søker…",
  cmdNoHits: "Ingen treff på «{q}».",
  cmdGroupNavigate: "Naviger",
  cmdGroupActions: "Handlinger",
  cmdGroupDeveloper: "Utvikler",
  cmdSongLibrary: "Sangbibliotek",
  cmdNewSong: "Ny sang…",
  cmdNewService: "Ny gudstjeneste…",
  cmdDesignSystem: "Designsystem",

  // ── Theme toggle ────────────────────────────────────────────────────────
  themeLabel: "Tema",
  themeSystem: "System",
  themeLight: "Lyst",
  themeDark: "Mørkt",

  // ── More common actions ─────────────────────────────────────────────────
  actionNew: "Ny",
  actionDuplicate: "Dupliser",
  dragReorder: "Dra for å endre rekkefølge",
  deletedMarker: "(slettet)",
  previewLabel: "Forhåndsvisning",
  loadingShort: "Laster…",

  // ── Service-item kinds (reused) ─────────────────────────────────────────
  kindSong: "Sang",
  kindScripture: "Skrift",
  kindNote: "Notat",

  // ── Plan-with-AI modal ──────────────────────────────────────────────────
  planTitle: "Planlegg tjeneste med AI",
  planBriefPlaceholder:
    "F.eks.: 25 min lovsang om tilgivelse for en ungdomsgudstjeneste, rolig avslutning.",
  planApiKeyRequired: "Anthropic API-nøkkel (påkrevd)",
  planThinking: "Tenker…",
  planSuggest: "Foreslå plan",
  planEmptyHint:
    "Beskriv tjenesten over, så foreslår AI sanger fra biblioteket ditt, lesninger og overganger.",
  planThemePrefix: "Tema: {theme}",
  planCreating: "Oppretter…",
  planCreateService: "Opprett tjeneste",

  // ── Paste & format modal ────────────────────────────────────────────────
  pasteTitle: "Lim inn & formater med AI",
  pasteRawPlaceholder:
    "Lim inn tekst fra hvor som helst — akkordlinjer, «x2», og rar formatering er greit.",
  pasteApiKeyOptional:
    "Anthropic API-nøkkel (valgfri — uten den formateres lokalt)",
  pasteFormatting: "Formaterer…",
  pasteFormat: "Formater",
  pasteUsingStoredKey:
    "Bruker den lagrede nøkkelen fra Innstillinger hvis feltet er tomt.",
  pasteNoKeyHint:
    "Uten nøkkel formateres det lokalt. Lagre en nøkkel i Innstillinger for AI.",
  pasteTitleSuggestion: "Tittelforslag: ",
  pasteResultHint: "Resultatet vises her etter formatering.",
  pasteApplying: "Bruker…",
  pasteApply: "Bruk på sangen",

  // ── Library page ────────────────────────────────────────────────────────
  librarySearchPlaceholder: "Søk i tekstlinjer…",
  libraryPlanWithAi: "Planlegg med AI",
  libraryNewSong: "Ny sang",
  filterAllLanguages: "Alle språk",
  filterAllLicensing: "All lisensiering",
  licenseUnknown: "Ukjent",
  colTitle: "Tittel",
  colKey: "Toneart",
  colTempo: "Tempo",
  colLanguage: "Språk",
  colLicense: "Lisens",
  libraryNoSongs: "Ingen sanger.",
  librarySelectForPreview: "Velg en sang for forhåndsvisning.",
  libraryNoLyricsYet:
    "Ingen tekst enda. Trykk «Rediger» for å legge til seksjoner.",
  libraryEmptyTitle: "Tomt bibliotek — la oss starte",
  libraryEmptyBody:
    "Legg til din første sang manuelt. Import fra ProPresenter, EasyWorship, FreeShow, OpenLP og tekstmapper kommer i en senere versjon.",
  libraryCreateFirst: "Lag din første sang",
  toastServiceCreated: "Gudstjeneste opprettet: {name}",
  newSongDefaultTitle: "Ny sang {time}",
  songCountOne: "{n} sang",
  songCountMany: "{n} sanger",

  // ── Song editor ─────────────────────────────────────────────────────────
  songSectionsTitle: "Deler",
  songAddSection: "Legg til del",
  songNoSections:
    "Ingen deler enda. Legg til et vers eller refreng for å starte.",
  songDeleteSection: "Slett del",
  songLyricsPlaceholder: "Tekstlinjer…",
  slideCountOne: "{n} lysbilde",
  slideCountMany: "{n} lysbilder",
  arrTitle: "Arrangement",
  arrNewNamePrompt: "Nytt navn",
  arrRename: "Gi nytt navn",
  arrSetDefault: "Standard",
  arrAddSectionTo: "Legg til del i arrangementet",
  arrCreateSectionsFirst: "Lag deler først.",
  arrSequence: "Rekkefølge",
  arrEmpty: "Tomt arrangement. Klikk en del over for å legge den til.",
  arrNone: "Ingen arrangementer enda.",
  arrCreate: "Lag arrangement",

  // ── Service-item kinds (extra) ──────────────────────────────────────────
  kindGap: "Pause",
  kindAnnouncement: "Kunngjøring",
  kindCustomDeck: "Lysbilder",
  kindVideo: "Video",
  actionPreview: "Forhåndsvis",

  // ── Services / queue editor ─────────────────────────────────────────────
  svcNewDefaultTitle: "Ny gudstjeneste {date}",
  svcNewService: "Ny gudstjeneste",
  svcImporting: "Importerer…",
  svcImportFromSundayPlan: "Importer fra SundayPlan",
  svcListEmpty:
    "Ingen gudstjenester enda. Lag en ny eller importer fra SundayPlan.",
  svcSelectOrCreate: "Velg eller lag en gudstjeneste for å bygge køen.",
  svcDeletedToast: "Gudstjeneste slettet",
  importMatchedSongs: "{n} sang(er) matchet",
  importCreatedEmpty: "{n} opprettet som tom",
  importWarnings: "{n} varsel",
  importToast: "Importert «{name}» — {details}",
  importFailed: "Import feilet: {error}",
  importReadError: "Kunne ikke lese filen",
  svcElementCountOne: "{n} element",
  svcElementCountMany: "{n} elementer",
  svcCuesInQueue: "{n} cues i køen",
  svcLoadingQueue: "Laster kø…",
  svcNotes: "Notater",
  svcDeleteService: "Slett gudstjeneste",
  svcQueueEmptyTooltip: "Køen er tom — legg til innhold først",
  svcGoLiveTooltip: "Gå live med denne gudstjenesten",
  svcEmptyQueueTitle: "Tom kø",
  svcEmptyQueueBody:
    "Legg til sanger så ser du her nøyaktig hvilke lysbilder hver sang blir, og hvor mange cues køen får totalt.",
  svcAddSong: "Legg til sang",
  svcConfirmDeleteTitle: "Slette denne gudstjenesten?",
  svcConfirmDeleteBody:
    "«{name}» fjernes fra listen. Sangene i biblioteket beholdes.",
  svcMoveUp: "Flytt opp",
  svcMoveDown: "Flytt ned",
  svcRemoveFromQueue: "Fjern fra kø",
  svcDefaultAllSections: "Standard (alle seksjoner)",
  svcKeyPlaceholder: "f.eks. G",
  svcLabelText: "Tekst",
  svcLabelPlaceholder: "f.eks. Kollekt",
  svcClickToRename: "Klikk for å gi nytt navn",
  svcNotesPlaceholder:
    "Notater for denne gudstjenesten (vises i live-konsollen)…",
  svcSaveNotes: "Lagre notater",
  svcSearchSongToAdd: "Søk etter sang å legge til…",
  svcNoSongsInLibrary: "Ingen sanger i biblioteket.",
  svcBackToSearch: "Tilbake til søk",
  svcKeyOptional: "Toneart (valgfri)",
  svcAddToQueue: "Legg til i kø",
  svcTranslation: "Oversettelse",
  svcBook: "Bok",
  svcChapter: "Kapittel",
  svcVerseFrom: "Vers fra",
  svcVerseTo: "Vers til",
  svcSelectEllipsis: "Velg…",
  svcAll: "alle",
  svcCompiling: "Kompilerer…",
  svcNoSlidesToShow: "Ingen lysbilder å vise.",
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

/** Optional `{name}`-style interpolation values. */
export type TParams = Record<string, string | number>;

export function translate(lang: Lang, key: TKey, params?: TParams): string {
  let s = CATALOG[lang]?.[key] ?? en[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      s = s.split(`{${k}}`).join(String(v));
    }
  }
  return s;
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
export function useT(): (key: TKey, params?: TParams) => string {
  const lang = useLocale((s) => s.lang);
  return (key, params) => translate(lang, key, params);
}
