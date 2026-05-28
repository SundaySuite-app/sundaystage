/**
 * Typed wrappers around Tauri's `invoke()`.
 *
 * One function per Rust command. Wraps `invoke<T>(name, args)` so:
 *   - The TypeScript caller has a stable signature
 *   - Rust `AppError` is rethrown as a JS `IPCError` the React code can
 *     catch
 *   - Dev-mode logs every call for debugging (toggle via `VITE_IPC_LOG`)
 *
 * Convention: command names are `entity_verb` (e.g. `song_list`,
 * `library_create`). Matches `commands::*` in Rust.
 */

import { invoke } from "@tauri-apps/api/core";

import type {
  AiKeyStatus,
  AiTestResult,
  AppError,
  ArrangementItem,
  BibleBook,
  BiblePassage,
  BibleTranslation,
  BibleVerse,
  ChapterMarker,
  ClaudeModel,
  CueList,
  CustomDeck,
  DemoSummary,
  FormattedSong,
  LocaleInfo,
  Library,
  LibraryInput,
  LiveAction,
  LiveSessionView,
  MediaAsset,
  MediaStatus,
  MonitorInfo,
  OutputConfig,
  SearchResult,
  StageDisplayConfig,
  Service,
  ServiceItem,
  ServicePlan,
  Slide,
  SlideDoc,
  Song,
  SongArrangement,
  SongInput,
  SongSection,
  SyncStatus,
  Template,
  Theme,
  ThemeTokens,
  TranslationResult,
  UniversalHit,
} from "./bindings";

const DEV = import.meta.env.DEV;
const LOG_IPC = DEV && import.meta.env.VITE_IPC_LOG !== "false";

/** Wrapper around Tauri's error that preserves the Rust `code` field. */
export class IPCError extends Error {
  readonly code: AppError["code"];
  constructor(err: AppError) {
    super(err.message);
    this.code = err.code;
    this.name = "IPCError";
  }
}

async function call<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (LOG_IPC) {
    console.debug(`[ipc] → ${cmd}`, args);
  }
  try {
    const out = await invoke<T>(cmd, args);
    if (LOG_IPC) console.debug(`[ipc] ← ${cmd}`, out);
    return out;
  } catch (raw) {
    // Tauri rethrows serialised AppError as plain object
    if (raw && typeof raw === "object" && "code" in raw && "message" in raw) {
      throw new IPCError(raw as AppError);
    }
    if (raw instanceof Error) throw raw;
    throw new Error(String(raw), { cause: raw });
  }
}

// ── Library ──────────────────────────────────────────────────────────────────

export const library = {
  create: (input: LibraryInput) => call<Library>("library_create", { input }),
  get: (id: string) => call<Library>("library_get", { id }),
  list: () => call<Library[]>("library_list"),
  rename: (id: string, name: string) =>
    call<Library>("library_rename", { id, name }),
};

// ── Song ─────────────────────────────────────────────────────────────────────

export const song = {
  create: (input: SongInput) => call<Song>("song_create", { input }),
  get: (id: string) => call<Song>("song_get", { id }),
  list: (libraryId: string, limit = 100, offset = 0) =>
    call<Song[]>("song_list", { libraryId, limit, offset }),
  delete: (id: string) => call<void>("song_delete", { id }),
  search: (libraryId: string, query: string, limit = 50) =>
    call<SearchResult[]>("song_search", { libraryId, query, limit }),
  sections: (songId: string) =>
    call<SongSection[]>("song_sections", { songId }),
  addSection: (songId: string, label: string, lyrics: string) =>
    call<SongSection>("song_add_section", { songId, label, lyrics }),
  updateSection: (id: string, label: string, lyrics: string) =>
    call<SongSection>("song_update_section", { id, label, lyrics }),
  deleteSection: (id: string) => call<void>("song_delete_section", { id }),
  reorderSections: (songId: string, orderedIds: string[]) =>
    call<SongSection[]>("song_reorder_sections", { songId, orderedIds }),
};

// ── Arrangements (Phase 3.3) ───────────────────────────────────────────────────

export const arrangement = {
  create: (songId: string, name: string) =>
    call<SongArrangement>("arrangement_create", { songId, name }),
  list: (songId: string) =>
    call<SongArrangement[]>("arrangement_list", { songId }),
  rename: (id: string, name: string) =>
    call<SongArrangement>("arrangement_rename", { id, name }),
  delete: (id: string) => call<void>("arrangement_delete", { id }),
  setDefault: (songId: string, arrangementId: string) =>
    call<void>("arrangement_set_default", { songId, arrangementId }),
  duplicate: (id: string) =>
    call<SongArrangement>("arrangement_duplicate", { id }),
  items: (arrangementId: string) =>
    call<ArrangementItem[]>("arrangement_items", { arrangementId }),
  setItems: (arrangementId: string, sectionIds: string[]) =>
    call<ArrangementItem[]>("arrangement_set_items", {
      arrangementId,
      sectionIds,
    }),
  sections: (arrangementId: string) =>
    call<SongSection[]>("arrangement_sections", { arrangementId }),
};

// ── Service ──────────────────────────────────────────────────────────────────

export const service = {
  create: (libraryId: string, name: string, startsAt: number) =>
    call<Service>("service_create", { libraryId, name, startsAt }),
  get: (id: string) => call<Service>("service_get", { id }),
  upcoming: (libraryId: string, from = 0, limit = 20) =>
    call<Service[]>("service_upcoming", { libraryId, from, limit }),
  items: (serviceId: string) =>
    call<ServiceItem[]>("service_items", { serviceId }),
};

// ── Live engine ──────────────────────────────────────────────────────────────

export const live = {
  compileCueList: (serviceId: string) =>
    call<CueList>("live_compile_cue_list", { serviceId }),
  start: (serviceId: string) =>
    call<LiveSessionView>("live_start", { serviceId }),
  dispatch: (action: LiveAction) =>
    call<LiveSessionView>("live_dispatch", { action }),
  state: () => call<LiveSessionView | null>("live_state"),
  end: () => call<void>("live_end"),
  recover: () => call<LiveSessionView | null>("live_recover"),
  stagePresets: () => call<StageDisplayConfig[]>("stage_presets"),
  // SundayRec bridge (Phase 10)
  bridgeVersion: () => call<string>("bridge_protocol_version"),
  chapterMarkers: () => call<ChapterMarker[]>("bridge_chapter_markers"),
  exportSrt: (endedAt?: number) =>
    call<string>("bridge_export_srt", { endedAt: endedAt ?? null }),
};

// ── Custom decks + slides (Phase 3.1 slide editor) ─────────────────────────────

export const deck = {
  create: (libraryId: string, name: string) =>
    call<CustomDeck>("deck_create", { libraryId, name }),
  get: (id: string) => call<CustomDeck>("deck_get", { id }),
  list: (libraryId: string) => call<CustomDeck[]>("deck_list", { libraryId }),
  rename: (id: string, name: string) =>
    call<CustomDeck>("deck_rename", { id, name }),
  delete: (id: string) => call<void>("deck_delete", { id }),

  slides: (deckId: string) => call<Slide[]>("slide_list", { deckId }),
  createSlide: (deckId: string, doc: SlideDoc) =>
    call<Slide>("slide_create", { deckId, doc }),
  updateSlide: (id: string, doc: SlideDoc) =>
    call<Slide>("slide_update_content", { id, doc }),
  duplicateSlide: (id: string) => call<Slide>("slide_duplicate", { id }),
  deleteSlide: (id: string) => call<void>("slide_delete", { id }),
  reorderSlides: (deckId: string, orderedIds: string[]) =>
    call<Slide[]>("slide_reorder", { deckId, orderedIds }),
  setSlideTheme: (id: string, themeId: string | null) =>
    call<Slide>("slide_set_theme", { id, themeId }),
  setSlideTemplate: (id: string, templateId: string | null) =>
    call<Slide>("slide_set_template", { id, templateId }),
};

// ── Themes + templates (Phase 3.2) ─────────────────────────────────────────────

export const theme = {
  listThemes: (libraryId: string) => call<Theme[]>("theme_list", { libraryId }),
  listTemplates: (libraryId: string) =>
    call<Template[]>("template_list", { libraryId }),
  create: (libraryId: string, name: string, tokens: ThemeTokens) =>
    call<Theme>("theme_create", { libraryId, name, tokens }),
  duplicate: (sourceId: string, libraryId: string) =>
    call<Theme>("theme_duplicate", { sourceId, libraryId }),
  updateTokens: (id: string, tokens: ThemeTokens) =>
    call<Theme>("theme_update_tokens", { id, tokens }),
  rename: (id: string, name: string) =>
    call<Theme>("theme_rename", { id, name }),
  delete: (id: string) => call<void>("theme_delete", { id }),
  setLibraryDefaultTheme: (libraryId: string, themeId: string | null) =>
    call<Library>("library_set_default_theme", { libraryId, themeId }),
  setLibraryDefaultTemplate: (libraryId: string, templateId: string | null) =>
    call<Library>("library_set_default_template", { libraryId, templateId }),
  render: (
    libraryId: string,
    templateId: string,
    themeId: string,
    slotText: Record<string, string>,
  ) =>
    call<SlideDoc>("template_render", {
      libraryId,
      templateId,
      themeId,
      slotText,
    }),
};

// ── AI (Phase 4) ───────────────────────────────────────────────────────────────

export const ai = {
  models: () => call<ClaudeModel[]>("ai_models"),
  formatLyrics: (raw: string, apiKey: string | null, model: string | null) =>
    call<FormattedSong>("ai_format_lyrics", { raw, apiKey, model }),
  applyFormat: (songId: string, formatted: FormattedSong) =>
    call<SongArrangement>("ai_apply_format", { songId, formatted }),
  planService: (
    libraryId: string,
    prompt: string,
    apiKey: string | null,
    model: string | null,
  ) =>
    call<ServicePlan>("ai_plan_service", { libraryId, prompt, apiKey, model }),
  applyPlan: (libraryId: string, plan: ServicePlan) =>
    call<Service>("ai_apply_plan", { libraryId, plan }),
  translate: (
    lines: string[],
    target: string,
    apiKey: string | null,
    model: string | null,
  ) =>
    call<TranslationResult>("ai_translate", { lines, target, apiKey, model }),
  // API-key management (Phase 4.1) — key lives in the OS keychain.
  keyStatus: () => call<AiKeyStatus>("ai_key_status"),
  keySet: (key: string) => call<void>("ai_key_set", { key }),
  keyClear: () => call<void>("ai_key_clear"),
  testConnection: (model: string | null) =>
    call<AiTestResult>("ai_test_connection", { model }),
};

// ── Media (Phase 7.2) ──────────────────────────────────────────────────────────

export const media = {
  list: (libraryId: string) => call<MediaStatus[]>("media_list", { libraryId }),
  import: (libraryId: string, path: string) =>
    call<MediaAsset>("media_import", { libraryId, path }),
  delete: (id: string) => call<void>("media_delete", { id }),
  relink: (id: string, searchDirs: string[]) =>
    call<MediaAsset | null>("media_relink", { id, searchDirs }),
};

// ── Onboarding + i18n (Phase 13.1) ─────────────────────────────────────────────

export const onboarding = {
  locales: () => call<LocaleInfo[]>("app_locales"),
  seedDemo: (libraryId: string) =>
    call<DemoSummary>("onboarding_seed_demo", { libraryId }),
};

// ── Cloud sync (Phase 9) ───────────────────────────────────────────────────────

export const sync = {
  status: () => call<SyncStatus>("sync_status"),
};

// ── Output displays (Phase 5.2) ─────────────────────────────────────────────────

export const output = {
  monitors: () => call<MonitorInfo[]>("output_monitors"),
  config: () => call<OutputConfig>("output_config"),
  setConfig: (config: OutputConfig) =>
    call<void>("output_set_config", { config }),
  open: () => call<void>("output_open"),
  close: () => call<void>("output_close"),
  isOpen: () => call<boolean>("output_is_open"),
};

// ── Universal search (Phase 2.3) ─────────────────────────────────────────────

export const search = {
  all: (libraryId: string, query: string) =>
    call<UniversalHit[]>("search_all", { libraryId, query }),
};

// ── Bible (Phase 7.1) ────────────────────────────────────────────────────────

export const bible = {
  translations: () => call<BibleTranslation[]>("bible_translations"),
  books: (translationId: string) =>
    call<BibleBook[]>("bible_books", { translationId }),
  chapters: (translationId: string, book: string) =>
    call<number[]>("bible_chapters", { translationId, book }),
  passage: (
    translationId: string,
    book: string,
    chapter: number,
    verseStart: number | null,
    verseEnd: number | null,
  ) =>
    call<BibleVerse[]>("bible_passage", {
      translationId,
      book,
      chapter,
      verseStart,
      verseEnd,
    }),
  lookup: (translationId: string, query: string) =>
    call<BiblePassage>("bible_lookup", { translationId, query }),
  search: (query: string, translationId: string | null) =>
    call<BibleVerse[]>("bible_search", { query, translationId }),
  addToService: (
    serviceId: string,
    translationId: string,
    book: string,
    chapter: number,
    verseStart: number | null,
    verseEnd: number | null,
  ) =>
    call<ServiceItem>("bible_add_to_service", {
      serviceId,
      translationId,
      book,
      chapter,
      verseStart,
      verseEnd,
    }),
};

// ── Crash reporting (Phase 6.1) ─────────────────────────────────────────────────

export const crash = {
  status: () => call<boolean>("crash_reporting_status"),
  set: (enabled: boolean) => call<void>("crash_reporting_set", { enabled }),
  count: () => call<number>("crash_reports_count"),
  clear: () => call<void>("crash_reports_clear"),
};

/** Bundled namespace for ergonomic imports. */
export const ipc = {
  library,
  song,
  service,
  live,
  deck,
  theme,
  arrangement,
  ai,
  media,
  onboarding,
  sync,
  output,
  crash,
  bible,
  search,
};
