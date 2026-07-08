// Runtime-contract guard for the Tauri IPC boundary.
//
// The `invoke(cmd, args)` call site is DYNAMICALLY typed: a wrong command
// name, a wrong arg KEY, or the wrong case (Tauri maps Rust snake_case args
// to/from JS camelCase automatically — but a hand-written wrapper can still
// pass the wrong literal key) compiles green yet fails silently at runtime.
//
// Each case below pins exactly what one `ipc.*` wrapper sends to `invoke`:
//   - the command name (must match a `#[tauri::command]` registered in
//     `src-tauri/src/lib.rs`)
//   - the arg object's KEYS (must be the camelCase of the Rust fn params)
//
// If a wrapper or a Rust signature drifts, this test breaks loudly instead of
// shipping a dead button to the Sunday-morning rig.
import { describe, it, expect, vi } from "vitest";

const invoke = vi.fn(async () => undefined);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

// Import AFTER the mock is registered.
import { ipc } from "@/lib/ipc";

/** Assert the single `invoke(cmd, args)` produced by running `fn`. */
async function expectCall(
  fn: () => Promise<unknown>,
  cmd: string,
  argKeys: string[],
) {
  invoke.mockClear();
  await fn();
  expect(invoke).toHaveBeenCalledTimes(1);
  const [calledCmd, calledArgs] = invoke.mock.calls[0];
  expect(calledCmd).toBe(cmd);
  if (argKeys.length === 0) {
    // No-arg commands: Tauri tolerates undefined; our wrapper omits the arg.
    expect(calledArgs).toBeUndefined();
  } else {
    expect(Object.keys(calledArgs as object).sort()).toEqual(
      [...argKeys].sort(),
    );
  }
}

describe("ipc arg-shape contract", () => {
  it("library", async () => {
    await expectCall(() => ipc.library.create({} as never), "library_create", [
      "input",
    ]);
    await expectCall(() => ipc.library.get("x"), "library_get", ["id"]);
    await expectCall(() => ipc.library.list(), "library_list", []);
    await expectCall(() => ipc.library.rename("x", "n"), "library_rename", [
      "id",
      "name",
    ]);
  });

  it("song", async () => {
    await expectCall(() => ipc.song.create({} as never), "song_create", [
      "input",
    ]);
    await expectCall(() => ipc.song.get("x"), "song_get", ["id"]);
    await expectCall(() => ipc.song.list("lib"), "song_list", [
      "libraryId",
      "limit",
      "offset",
    ]);
    await expectCall(() => ipc.song.delete("x"), "song_delete", ["id"]);
    await expectCall(() => ipc.song.search("lib", "q"), "song_search", [
      "libraryId",
      "query",
      "limit",
    ]);
    await expectCall(() => ipc.song.sections("s"), "song_sections", ["songId"]);
    await expectCall(
      () => ipc.song.addSection("s", "l", "ly"),
      "song_add_section",
      ["songId", "label", "lyrics"],
    );
    await expectCall(
      () => ipc.song.updateSection("i", "l", "ly"),
      "song_update_section",
      ["id", "label", "lyrics"],
    );
    await expectCall(() => ipc.song.deleteSection("i"), "song_delete_section", [
      "id",
    ]);
    await expectCall(
      () => ipc.song.reorderSections("s", []),
      "song_reorder_sections",
      ["songId", "orderedIds"],
    );
    await expectCall(
      () => ipc.song.importFile("lib", "f.txt", "c"),
      "import_song_file",
      ["libraryId", "filename", "content"],
    );
  });

  it("arrangement", async () => {
    await expectCall(
      () => ipc.arrangement.create("s", "n"),
      "arrangement_create",
      ["songId", "name"],
    );
    await expectCall(() => ipc.arrangement.list("s"), "arrangement_list", [
      "songId",
    ]);
    await expectCall(
      () => ipc.arrangement.rename("i", "n"),
      "arrangement_rename",
      ["id", "name"],
    );
    await expectCall(() => ipc.arrangement.delete("i"), "arrangement_delete", [
      "id",
    ]);
    await expectCall(
      () => ipc.arrangement.setDefault("s", "a"),
      "arrangement_set_default",
      ["songId", "arrangementId"],
    );
    await expectCall(
      () => ipc.arrangement.duplicate("i"),
      "arrangement_duplicate",
      ["id"],
    );
    await expectCall(() => ipc.arrangement.items("a"), "arrangement_items", [
      "arrangementId",
    ]);
    await expectCall(
      () => ipc.arrangement.setItems("a", []),
      "arrangement_set_items",
      ["arrangementId", "sectionIds"],
    );
    await expectCall(
      () => ipc.arrangement.sections("a"),
      "arrangement_sections",
      ["arrangementId"],
    );
  });

  it("service", async () => {
    await expectCall(
      () => ipc.service.create("lib", "n", 0),
      "service_create",
      ["libraryId", "name", "startsAt"],
    );
    await expectCall(() => ipc.service.get("x"), "service_get", ["id"]);
    await expectCall(() => ipc.service.upcoming("lib"), "service_upcoming", [
      "libraryId",
      "from",
      "limit",
    ]);
    await expectCall(() => ipc.service.items("s"), "service_items", [
      "serviceId",
    ]);
    await expectCall(() => ipc.service.songsByItem("s"), "songs_by_item", [
      "serviceId",
    ]);
    await expectCall(() => ipc.service.rename("i", "n"), "service_rename", [
      "id",
      "name",
    ]);
    await expectCall(
      () => ipc.service.setNotes("i", "n"),
      "service_set_notes",
      ["id", "notes"],
    );
    await expectCall(
      () => ipc.service.setStartsAt("i", 0),
      "service_set_starts_at",
      ["id", "startsAt"],
    );
    await expectCall(() => ipc.service.delete("i"), "service_delete", ["id"]);
    await expectCall(
      () => ipc.service.addSong("s", "song"),
      "service_add_song",
      ["serviceId", "songId", "arrangementId", "keyOverride"],
    );
    await expectCall(
      () => ipc.service.addItem("s", "kind"),
      "service_add_item",
      ["serviceId", "kind", "label", "customDeckId"],
    );
    await expectCall(
      () => ipc.service.updateItem("i", null, null, null),
      "service_update_item",
      ["itemId", "arrangementId", "keyOverride", "notes"],
    );
    await expectCall(() => ipc.service.removeItem("i"), "service_remove_item", [
      "itemId",
    ]);
    await expectCall(
      () => ipc.service.reorderItems("s", []),
      "service_reorder_items",
      ["serviceId", "orderedIds"],
    );
    await expectCall(() => ipc.service.cueSummary("s"), "service_cue_summary", [
      "serviceId",
    ]);
    await expectCall(
      () => ipc.service.importSundayPlan("lib", "{}"),
      "service_import_sundayplan",
      ["libraryId", "json"],
    );
  });

  it("live", async () => {
    await expectCall(
      () => ipc.live.compileCueList("s"),
      "live_compile_cue_list",
      ["serviceId"],
    );
    await expectCall(() => ipc.live.start("s"), "live_start", ["serviceId"]);
    await expectCall(
      () => ipc.live.dispatch({ type: "next" }),
      "live_dispatch",
      ["action"],
    );
    await expectCall(() => ipc.live.state(), "live_state", []);
    await expectCall(() => ipc.live.end(), "live_end", []);
    await expectCall(() => ipc.live.recover(), "live_recover", []);
    await expectCall(() => ipc.live.stagePresets(), "stage_presets", []);
    await expectCall(
      () => ipc.live.bridgeVersion(),
      "bridge_protocol_version",
      [],
    );
    await expectCall(
      () => ipc.live.chapterMarkers(),
      "bridge_chapter_markers",
      [],
    );
    await expectCall(() => ipc.live.exportSrt(), "bridge_export_srt", [
      "endedAt",
    ]);
  });

  it("deck + slides", async () => {
    await expectCall(() => ipc.deck.create("lib", "n"), "deck_create", [
      "libraryId",
      "name",
    ]);
    await expectCall(() => ipc.deck.get("x"), "deck_get", ["id"]);
    await expectCall(() => ipc.deck.list("lib"), "deck_list", ["libraryId"]);
    await expectCall(() => ipc.deck.rename("i", "n"), "deck_rename", [
      "id",
      "name",
    ]);
    await expectCall(() => ipc.deck.delete("i"), "deck_delete", ["id"]);
    await expectCall(() => ipc.deck.slides("d"), "slide_list", ["deckId"]);
    await expectCall(
      () => ipc.deck.createSlide("d", {} as never),
      "slide_create",
      ["deckId", "doc"],
    );
    await expectCall(
      () => ipc.deck.updateSlide("i", {} as never),
      "slide_update_content",
      ["id", "doc"],
    );
    await expectCall(() => ipc.deck.duplicateSlide("i"), "slide_duplicate", [
      "id",
    ]);
    await expectCall(() => ipc.deck.deleteSlide("i"), "slide_delete", ["id"]);
    await expectCall(() => ipc.deck.reorderSlides("d", []), "slide_reorder", [
      "deckId",
      "orderedIds",
    ]);
    await expectCall(
      () => ipc.deck.setSlideTheme("i", null),
      "slide_set_theme",
      ["id", "themeId"],
    );
    await expectCall(
      () => ipc.deck.setSlideTemplate("i", null),
      "slide_set_template",
      ["id", "templateId"],
    );
  });

  it("theme + templates", async () => {
    await expectCall(() => ipc.theme.listThemes("lib"), "theme_list", [
      "libraryId",
    ]);
    await expectCall(() => ipc.theme.listTemplates("lib"), "template_list", [
      "libraryId",
    ]);
    await expectCall(
      () => ipc.theme.create("lib", "n", {} as never),
      "theme_create",
      ["libraryId", "name", "tokens"],
    );
    await expectCall(
      () => ipc.theme.duplicate("src", "lib"),
      "theme_duplicate",
      ["sourceId", "libraryId"],
    );
    await expectCall(
      () => ipc.theme.updateTokens("i", {} as never),
      "theme_update_tokens",
      ["id", "tokens"],
    );
    await expectCall(() => ipc.theme.rename("i", "n"), "theme_rename", [
      "id",
      "name",
    ]);
    await expectCall(() => ipc.theme.delete("i"), "theme_delete", ["id"]);
    await expectCall(
      () => ipc.theme.setLibraryDefaultTheme("lib", null),
      "library_set_default_theme",
      ["libraryId", "themeId"],
    );
    await expectCall(
      () => ipc.theme.setLibraryDefaultTemplate("lib", null),
      "library_set_default_template",
      ["libraryId", "templateId"],
    );
    await expectCall(
      () => ipc.theme.render("lib", "tpl", "thm", {}),
      "template_render",
      ["libraryId", "templateId", "themeId", "slotText"],
    );
  });

  it("ai", async () => {
    await expectCall(() => ipc.ai.models(), "ai_models", []);
    await expectCall(
      () => ipc.ai.formatLyrics("raw", null, null),
      "ai_format_lyrics",
      ["raw", "apiKey", "model"],
    );
    await expectCall(
      () => ipc.ai.applyFormat("s", {} as never),
      "ai_apply_format",
      ["songId", "formatted"],
    );
    await expectCall(
      () => ipc.ai.planService("lib", "p", null, null),
      "ai_plan_service",
      ["libraryId", "prompt", "apiKey", "model"],
    );
    await expectCall(
      () => ipc.ai.applyPlan("lib", {} as never),
      "ai_apply_plan",
      ["libraryId", "plan"],
    );
    await expectCall(
      () => ipc.ai.translate([], "no", null, null),
      "ai_translate",
      ["lines", "target", "apiKey", "model"],
    );
    await expectCall(() => ipc.ai.keyStatus(), "ai_key_status", []);
    await expectCall(() => ipc.ai.keySet("k"), "ai_key_set", ["key"]);
    await expectCall(() => ipc.ai.keyClear(), "ai_key_clear", []);
    await expectCall(() => ipc.ai.testConnection(null), "ai_test_connection", [
      "model",
    ]);
  });

  it("media", async () => {
    await expectCall(() => ipc.media.list("lib"), "media_list", ["libraryId"]);
    await expectCall(() => ipc.media.import("lib", "/p"), "media_import", [
      "libraryId",
      "path",
    ]);
    await expectCall(() => ipc.media.delete("i"), "media_delete", ["id"]);
    await expectCall(() => ipc.media.relink("i", []), "media_relink", [
      "id",
      "searchDirs",
    ]);
  });

  it("onboarding + sync + crash", async () => {
    await expectCall(() => ipc.onboarding.locales(), "app_locales", []);
    await expectCall(
      () => ipc.onboarding.seedDemo("lib"),
      "onboarding_seed_demo",
      ["libraryId"],
    );
    await expectCall(() => ipc.sync.status(), "sync_status", []);
    await expectCall(() => ipc.crash.status(), "crash_reporting_status", []);
    await expectCall(() => ipc.crash.set(true), "crash_reporting_set", [
      "enabled",
    ]);
    await expectCall(() => ipc.crash.count(), "crash_reports_count", []);
    await expectCall(() => ipc.crash.clear(), "crash_reports_clear", []);
  });

  it("output", async () => {
    await expectCall(() => ipc.output.monitors(), "output_monitors", []);
    await expectCall(() => ipc.output.config(), "output_config", []);
    await expectCall(
      () => ipc.output.setConfig({} as never),
      "output_set_config",
      ["config"],
    );
    await expectCall(() => ipc.output.open(), "output_open", []);
    await expectCall(() => ipc.output.close(), "output_close", []);
    await expectCall(() => ipc.output.isOpen(), "output_is_open", []);
    await expectCall(() => ipc.output.appearance(), "output_appearance", []);
    await expectCall(
      () => ipc.output.setAppearance({} as never),
      "output_set_appearance",
      ["appearance"],
    );
    await expectCall(
      () => ipc.output.displayConfig(),
      "output_display_config",
      [],
    );
    await expectCall(
      () => ipc.output.setDisplayConfig({} as never),
      "output_set_display_config",
      ["config"],
    );
  });

  it("service templates", async () => {
    await expectCall(() => ipc.serviceTemplate.list(), "svc_template_list", []);
    await expectCall(
      () => ipc.serviceTemplate.create({} as never),
      "svc_template_create",
      ["input"],
    );
    await expectCall(
      () => ipc.serviceTemplate.delete("i"),
      "svc_template_delete",
      ["id"],
    );
    await expectCall(
      () => ipc.serviceTemplate.apply("t", "s"),
      "svc_template_apply",
      ["templateId", "serviceId"],
    );
  });

  it("search", async () => {
    await expectCall(() => ipc.search.all("lib", "q"), "search_all", [
      "libraryId",
      "query",
    ]);
  });

  it("bible", async () => {
    await expectCall(() => ipc.bible.translations(), "bible_translations", []);
    await expectCall(() => ipc.bible.books("t"), "bible_books", [
      "translationId",
    ]);
    await expectCall(() => ipc.bible.chapters("t", "b"), "bible_chapters", [
      "translationId",
      "book",
    ]);
    await expectCall(
      () => ipc.bible.passage("t", "b", 1, null, null),
      "bible_passage",
      ["translationId", "book", "chapter", "verseStart", "verseEnd"],
    );
    await expectCall(() => ipc.bible.lookup("t", "q"), "bible_lookup", [
      "translationId",
      "query",
    ]);
    await expectCall(() => ipc.bible.search("q", null), "bible_search", [
      "query",
      "translationId",
    ]);
    await expectCall(
      () => ipc.bible.addToService("s", "t", "b", 1, null, null),
      "bible_add_to_service",
      [
        "serviceId",
        "translationId",
        "book",
        "chapter",
        "verseStart",
        "verseEnd",
      ],
    );
  });
});
