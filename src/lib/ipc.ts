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
  CounterTotal,
  CrashKind,
  ArrangementItem,
  AvailableTranslation,
  BibleBook,
  BiblePassage,
  BibleTranslation,
  BibleVerse,
  ChapterMarker,
  ClaudeModel,
  CueList,
  CueSpec,
  CueSummary,
  CustomDeck,
  DemoSummary,
  EasyWorshipImportResult,
  FormattedSong,
  ImportResult,
  LocaleInfo,
  Library,
  LibraryInput,
  LiveAction,
  LiveSessionView,
  MediaAsset,
  MediaStatus,
  MonitorInfo,
  OutputAppearance,
  OutputConfig,
  OutputDisplayConfig,
  PlanImportResult,
  ProblemReport,
  ReportContext,
  ReportOutcome,
  SearchResult,
  StageDisplayConfig,
  Service,
  ServiceItem,
  ServiceItemSong,
  ServicePlan,
  ServiceTemplate,
  ServiceTemplateInput,
  Slide,
  SlideDoc,
  Song,
  SongArrangement,
  SongInput,
  SongSection,
  QualityRow,
  SyncStatus,
  TelemetryConsent,
  TelemetryFlush,
  TelemetryPreview,
  TelemetryQueueStatus,
  Template,
  Theme,
  ThemeTokens,
  TranslationResult,
  UniversalHit,
  UpdateChannel,
  UpdateInfo,
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

/** Result of publishing the local library to the shared church cloud. */
export interface PublishResult {
  upserted: number;
  deleted: number;
  churchId: string;
  songCount: number;
}

export const library = {
  create: (input: LibraryInput) => call<Library>("library_create", { input }),
  get: (id: string) => call<Library>("library_get", { id }),
  list: () => call<Library[]>("library_list"),
  rename: (id: string, name: string) =>
    call<Library>("library_rename", { id, name }),
  /** Publish this library to the shared church cloud (desktop → web, one-way). */
  publish: (libraryId: string) =>
    call<PublishResult>("library_publish", { libraryId }),
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
  importFile: (libraryId: string, filename: string, content: string) =>
    call<ImportResult>("import_song_file", { libraryId, filename, content }),
  /** Export a song to an open interchange format (Spor B5, the lock-in fix).
   *  Returns the serialised OpenLyrics XML / ChordPro text for the frontend to
   *  preview, copy or download. */
  export: (songId: string, format: "openlyrics" | "chordpro") =>
    call<string>("export_song", { songId, format }),
  /** Import a whole EasyWorship library from its `Databases/Data` folder path.
   *  Reads the SQLite files natively (the renderer can't), so it takes a folder
   *  path rather than file content. */
  importEasyWorship: (libraryId: string, folderPath: string) =>
    call<EasyWorshipImportResult>("import_easyworship_library", {
      libraryId,
      folderPath,
    }),
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
  // serviceItemId → song behind it (Phase 3 usage bridge). Non-song items absent.
  songsByItem: (serviceId: string) =>
    call<Record<string, ServiceItemSong>>("songs_by_item", { serviceId }),
  rename: (id: string, name: string) =>
    call<Service>("service_rename", { id, name }),
  setNotes: (id: string, notes: string) =>
    call<Service>("service_set_notes", { id, notes }),
  // Phase 11.2 — set/clear the live translation overlay's target language.
  // `null` clears it. The backend pre-resolves translations at Go-Live time.
  setSecondaryLanguage: (id: string, language: string | null) =>
    call<Service>("service_set_secondary_language", { id, language }),
  setStartsAt: (id: string, startsAt: number) =>
    call<Service>("service_set_starts_at", { id, startsAt }),
  delete: (id: string) => call<void>("service_delete", { id }),
  addSong: (
    serviceId: string,
    songId: string,
    arrangementId: string | null = null,
    keyOverride: string | null = null,
  ) =>
    call<ServiceItem>("service_add_song", {
      serviceId,
      songId,
      arrangementId,
      keyOverride,
    }),
  addItem: (
    serviceId: string,
    kind: string,
    label: string | null = null,
    customDeckId: string | null = null,
  ) =>
    call<ServiceItem>("service_add_item", {
      serviceId,
      kind,
      label,
      customDeckId,
    }),
  updateItem: (
    itemId: string,
    arrangementId: string | null,
    keyOverride: string | null,
    notes: string | null,
  ) =>
    call<ServiceItem>("service_update_item", {
      itemId,
      arrangementId,
      keyOverride,
      notes,
    }),
  removeItem: (itemId: string) => call<void>("service_remove_item", { itemId }),
  reorderItems: (serviceId: string, orderedIds: string[]) =>
    call<ServiceItem[]>("service_reorder_items", { serviceId, orderedIds }),
  cueSummary: (serviceId: string) =>
    call<CueSummary>("service_cue_summary", { serviceId }),
  importSundayPlan: (libraryId: string, json: string) =>
    call<PlanImportResult>("service_import_sundayplan", { libraryId, json }),
};

// ── Live engine ──────────────────────────────────────────────────────────────

export const live = {
  compileCueList: (serviceId: string) =>
    call<CueList>("live_compile_cue_list", { serviceId }),
  start: (serviceId: string) =>
    call<LiveSessionView>("live_start", { serviceId }),
  /** Recompile the running service and hot-swap the session's cue list
   *  (mid-service edits). The cue on air stays on air. */
  reload: () => call<LiveSessionView>("live_reload_cue_list"),
  dispatch: (action: LiveAction) =>
    call<LiveSessionView>("live_dispatch", { action }),
  state: () => call<LiveSessionView | null>("live_state"),
  end: () => call<void>("live_end"),
  recover: () => call<LiveSessionView | null>("live_recover"),
  /** Drop a crash-recovery offer. Clears the recovery log and ends the offer
   *  session if the operator never accepted it — but never touches a service
   *  that is actually running (that is `end`, and it blacks the outputs). */
  discardRecovery: () => call<void>("live_discard_recovery"),
  stagePresets: () => call<StageDisplayConfig[]>("stage_presets"),
  // SundayRec bridge (Phase 10)
  bridgeVersion: () => call<string>("bridge_protocol_version"),
  chapterMarkers: () => call<ChapterMarker[]>("bridge_chapter_markers"),
  exportSrt: (endedAt?: number) =>
    call<string>("bridge_export_srt", { endedAt: endedAt ?? null }),
  /** Export the running session as a SundayRec `service-manifest.json` string
   *  (setlist + chapters with CCLI/TONO ids). */
  exportManifest: (endedAt?: number) =>
    call<string>("bridge_export_manifest", { endedAt: endedAt ?? null }),
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
  appearance: () => call<OutputAppearance>("output_appearance"),
  setAppearance: (appearance: OutputAppearance) =>
    call<OutputAppearance>("output_set_appearance", { appearance }),
  displayConfig: () => call<OutputDisplayConfig>("output_display_config"),
  setDisplayConfig: (config: OutputDisplayConfig) =>
    call<OutputDisplayConfig>("output_set_display_config", { config }),
};

// ── Service templates ─────────────────────────────────────────────────────────

export const serviceTemplate = {
  list: () => call<ServiceTemplate[]>("svc_template_list"),
  create: (input: ServiceTemplateInput) =>
    call<ServiceTemplate>("svc_template_create", { input }),
  delete: (id: string) => call<void>("svc_template_delete", { id }),
  apply: (templateId: string, serviceId: string) =>
    call<ServiceItem[]>("svc_template_apply", { templateId, serviceId }),
};

/** Parse the JSON `cue_specs` field of a `ServiceTemplate` into typed objects. */
export function parseCueSpecs(template: ServiceTemplate): CueSpec[] {
  try {
    return JSON.parse(template.cue_specs) as CueSpec[];
  } catch {
    return [];
  }
}

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
  // Full-corpus downloader (Spor C, C1/C2). `download` resolves with the
  // installed translation; watch progress via the `bible://download-progress`
  // event (payload: BibleDownloadProgress).
  availableTranslations: () =>
    call<AvailableTranslation[]>("bible_available_translations"),
  download: (code: string) =>
    call<BibleTranslation>("bible_download_translation", { code }),
};

/** The event a running Bible download emits progress on (BibleDownloadProgress
 *  payloads). Mirrors `services::bible_download::PROGRESS_EVENT` in Rust. */
export const BIBLE_DOWNLOAD_PROGRESS_EVENT = "bible://download-progress";

// ── Crash reporting (Phase 6.1) ─────────────────────────────────────────────────
//
// Local capture is ALWAYS ON since E5 — hence no `status`/`set` pair. The ring
// never leaves the machine, which is exactly why it needs no permission; what
// needs permission is sending, and that is `telemetry.consent` below.

export const crash = {
  count: () => call<number>("crash_reports_count"),
  clear: () => call<void>("crash_reports_clear"),
  /**
   * Report an error the RENDERER caught into the same bounded, scrubbed ring
   * the Rust panic hook writes to.
   *
   * `kind` is a closed set on the Rust side, so an invented value is rejected
   * at the IPC boundary rather than widening the vocabulary. Resolves to
   * nothing and — see `lib/errorReporting.ts` — must never be awaited in a way
   * that lets a failure here become a second error.
   */
  report: (entry: {
    kind: CrashKind;
    message: string;
    location?: string | null;
    component?: string | null;
  }) =>
    call<void>("crash_report_frontend", {
      kind: entry.kind,
      message: entry.message,
      location: entry.location ?? null,
      component: entry.component ?? null,
    }),
};

// ── Local observation (E3) + the client (E5) ───────────────────────────────────
//
// The E3 half reads what this machine has accumulated, plus the log tail.
// `logTail` takes a LINE COUNT and no path, deliberately: the renderer cannot
// aim the reader at a file.
//
// The E5 half is the machinery that COULD send. It sends nothing in this
// release, for two independent reasons: no UI calls `consent.set`, so the record
// stays `never-asked`; and no build has a telemetry endpoint baked in, so no
// sender is ever constructed. E6 builds the consent UX; until then these exist
// so that E6 adds a question rather than a pipeline.

export const telemetry = {
  counters: () => call<CounterTotal[]>("telemetry_counters"),
  quality: (limit?: number) =>
    call<QualityRow[]>("telemetry_quality_recent", { limit: limit ?? null }),
  /** Fold the in-memory buffer into SQLite — a no-op while a service is live. */
  flush: () => call<TelemetryFlush>("telemetry_flush"),
  clear: () => call<void>("telemetry_clear"),
  logTail: (lines?: number) =>
    call<string>("log_tail", { lines: lines ?? null }),

  /**
   * The three-state consent record.
   *
   * `status` is `never-asked` | `granted` | `denied` — and `never-asked` is NOT
   * `denied`: the first must be asked exactly once, the second left alone
   * forever. Read `needsPrompt` and `active` rather than deriving them; the
   * "a stale grant is not a grant" rule lives in Rust and has tests.
   */
  consent: {
    get: () => call<TelemetryConsent>("telemetry_consent_get"),
    /**
     * Record an answer. Granting mints the install id and starts the send
     * pump; revoking purges the outbox and the accumulated counters, so "off"
     * means nothing is left to send rather than a paused pile.
     */
    set: (granted: boolean) =>
      call<TelemetryConsent>("telemetry_consent_set", { granted }),
  },

  /**
   * Become a different install: the current id is retired and parked for a
   * remote delete. Resolves to the new id, or `null` when consent is not
   * active — someone who is not consenting must not be handed a fresh durable
   * identifier.
   */
  regenerateInstallId: () =>
    call<string | null>("telemetry_regenerate_install_id"),

  /**
   * "Delete my data": retire the identity, empty the outbox, and delete the
   * local observation copy. Deliberately NOT consent-gated — a deletion is the
   * withdrawal of data already contributed, and is most needed by someone who
   * has already said no.
   */
  deleteMyData: () => call<string | null>("telemetry_delete_my_data"),

  /**
   * What is waiting to be delivered, for an honest settings line.
   *
   * Two numbers: `pending` payloads and `pendingReports`. They are different
   * facts — a report the byte cap deferred is still owed even when every queued
   * payload went out cleanly — so the panel shows both.
   */
  queueStatus: () => call<TelemetryQueueStatus>("telemetry_queue_status"),

  /** The install id this machine reports under, or `null` when it has none. */
  installId: () => call<string | null>("telemetry_install_id"),

  /**
   * The manual "report a problem" path (E6).
   *
   * `preview` returns the EXACT report pressing send would produce — scrubbed,
   * capped, log tail included — and `submit` takes that previewed `logTail`
   * back, so the bytes on screen are the bytes on the wire rather than a second
   * read of a log that has moved on.
   *
   * With standing consent OFF the report is still sendable: pressing send is
   * consent for that one report, it travels alone under a one-shot random id
   * that is stored nowhere, and no durable install id is created. The resolved
   * `ReportOutcome` says which of the five things actually happened — including
   * "this build has no endpoint", which is every development build.
   */
  report: {
    preview: (context: ReportContext, message: string) =>
      call<ProblemReport>("telemetry_report_preview", { context, message }),
    submit: (context: ReportContext, message: string, logTail: string) =>
      call<ReportOutcome>("telemetry_report_submit", {
        context,
        message,
        logTail,
      }),
  },

  /**
   * The exact payload this machine would send, through the REAL builder — a
   * mock would be a promise about code that never runs. Read-only: it mints no
   * id, advances no watermark and spends no counter.
   */
  previewPayload: () => call<TelemetryPreview>("telemetry_preview_payload"),

  /**
   * Mirror the UI locale into the backend. The locale lives in `localStorage`,
   * which Rust cannot read, and a payload that could not name the UI language
   * would be missing the one setting that says which translations get used.
   * Reduced to a primary subtag on the way in (`nb-NO` → `nb`).
   */
  setLanguage: (lang: string) => call<void>("telemetry_set_language", { lang }),
};

// ── Auto-update over the app-scoped rings (E2) ──────────────────────────────────
//
// The check runs in Rust: `UpdaterBuilder::endpoints(..)` is the only seam that
// can pick the ring at runtime — the updater plugin's JS `check()` has no
// `endpoints` option, which is why only the plugin's Rust crate is a dependency
// here and its npm package is not installed at all. `check` resolves to `null`
// when we are up to date, which includes the ring answering 204 (nothing
// promoted / ring paused). Relaunching after `install` stays a JS concern
// (`@tauri-apps/plugin-process`), see `lib/updater.ts`.

export const update = {
  channel: () => call<UpdateChannel>("update_channel_get"),
  setChannel: (channel: UpdateChannel) =>
    call<UpdateChannel>("update_channel_set", { channel }),
  check: () => call<UpdateInfo | null>("update_check"),
  install: () => call<void>("update_install"),
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
  telemetry,
  bible,
  search,
  serviceTemplate,
  update,
  parseCueSpecs,
};
