# SundayStage — Launch Readiness

Tracks what's done and what remains before a public 1.0. Maintained from
Phase 13.1 onward.

## Done

- [x] First-run onboarding: language picker + one-click demo content
      (`WelcomeScreen` → `onboarding_seed_demo`). Gated by `localStorage`.
- [x] Demo library: 3 public-domain songs (Amazing Grace, Holy Holy Holy, Be
      Thou My Vision) with arrangements, a KJV scripture, a welcome deck, and a
      ready-to-play "Velkomstgudstjeneste" service.
- [x] i18n machinery: `lib/i18n.ts` catalog + `t()` with English fallback +
      persisted locale store; navigation/chrome strings wired through `t()`.
- [x] Supported locales exposed (`app_locales`): no, en, sv, da, de, fr, pl
      (matches SundayRec).

## i18n status per language

| Lang | Nav/chrome | Full app strings          |
| ---- | ---------- | ------------------------- |
| en   | ✅         | ✅ (base)                 |
| no   | ✅         | ✅                        |
| sv   | ✅         | ✅ (machine, AI-reviewed) |
| da   | ✅         | ✅ (machine, AI-reviewed) |
| de   | ✅         | ✅ (machine, AI-reviewed) |
| fr   | ✅         | ✅ (machine, AI-reviewed) |
| pl   | ✅         | ✅ (machine, AI-reviewed) |

- [x] Route **every** user-visible string through `t()`. Done across every
      feature page (library, services/queue, live console, decks/slide editor,
      bible, media, settings) and shared chrome. ~290 keys; `t()` supports
      `{name}`-interpolation. **Exception:** the dev-only `/design` style guide
      (DEV-gated route) is left untranslated.
- [x] Machine-translate sv/da/de/fr/pl catalogs (full, via Claude — all 357
      keys per language).
- [x] AI review pass over all five non-base catalogs (2026-05-30): each value
      checked against the English base for terminology, interpolation-placeholder
      integrity, alignment-abbreviation consistency and English leftovers. Result:
      pl needed one consistency fix (`inspAlignCenter` "C" → "Ś"); sv/da/de/fr
      were already correct and internally consistent (0 changes). A **native
      human review** is still recommended before public release for a few stylistic
      nuances flagged in the change notes (e.g. da `bibSearchPlaceholder` keeps the
      English "(shepherd)" example; sv `inspAlignCenter` "C" vs "M").
- [x] Localise section-type labels (`verse_1` → "Verse 1") on the content-
      authoring surfaces. A shared `localizeSectionLabel()` (`src/lib/sectionLabel.ts`)
      maps canonical section _types_ (intro, verse, pre_chorus, chorus, bridge,
      instrumental, tag, ending + outro/refrain synonyms) through `t()`, keeps a
      trailing number (`verse_1` → "Vers 1"/"Couplet 1"), and falls back to
      title-case for user-authored labels. Eight `sectionX` keys added to all
      seven catalogs. Wired into the song editor, paste-&-format modal, library
      song detail and the services queue editor; replaces the old per-file
      `humanize()`. (8 unit tests in `tests/integration/sectionLabel.test.ts`.)
- [x] Localise section-type labels on the **live output** and stage screens.
      These read `slide_content.section_label`, which the Rust cue compiler
      pre-title-cases via `humanize_section_label` ("Verse 1"). The locale lives
      only in the frontend (`localStorage` "ss-locale"; the backend has no
      locale), so localisation is done at the display layer: `localizeSectionLabel()`
      normalises the title-cased form back to a canonical type and re-localises
      it. `SlideView` takes an optional `localizeLabel` prop (default identity,
      so it stays i18n-agnostic) — the real congregation output (`OutputView`)
      and the services slide preview pass it; the operator preview
      (`LivePreview`) and the musicians' `StageDisplay` localise inline. The
      Settings sample (`setSampleSection`) is already localised, so it keeps the
      identity default. The cue compiler's `section_label`/`display_label`
      contract is unchanged (no risk to the sacrosanct, well-tested compile
      path).
- [x] Operator cue labels (`display_label`) no longer carry hard-coded
      Norwegian prefixes ("Sang — ", "Bibel — ", "Deck — Slide "). Since the
      backend has no locale, each cue now uses its own language-neutral identity:
      song → "{title} — {section}" (also fixes the title the doc promised but the
      code dropped), scripture → the reference ("John 3:16-17"), deck → "{deck
      name} — {n}".
- [x] **Song import (Phase 2.2)** — `services/song_import.rs`: pure,
      dependency-free parsers for plain text, ChordPro, OpenSong and OpenLyrics
      (OpenLP) → `FormattedSong`, reusing the existing `apply_formatted_song`
      insertion path. `import_song_file` command + `ImportModal` (plain
      `<input type=file>` + `FileReader`, so no native dialog plugin needed yet),
      wired into the library header + empty state, 12 import i18n keys ×7
      catalogs. Binary formats (ProPresenter `.pro`, EasyWorship, FreeShow) need
      format-specific decoders — out of scope.
- [x] **Deep-open from search (Phase 2.3 polish)** — selecting a song/service
      hit in ⌘K now opens the item (editor / selected service), not just its
      page, via a small deep-link target threaded through `App`. Bible hits still
      navigate to the page (jumping to a verse needs more in `BiblePage`).

## Remaining before 1.0 (deferred / needs infra this environment can't provide)

- [ ] Bible search deep-open: jump to the matched verse in `BiblePage` (today it
      navigates to the page only).
- [ ] Native file dialog / drag-drop for import (the parser + an in-webview file
      input already work; a native dialog/drag-drop is a nicety).

- [ ] Interactive 5-step tutorial overlay (library → editor → live).
- [~] Multi-display output + per-screen role assignment (Phase 5.2): shipped as
  borderless full-screen Tauri output windows (one per monitor) driven by an
  event-bus render/heartbeat from the operator UI, with a JS watchdog that
  holds the last frame. **Needs native verification** (does a window land on
  monitor 2, fullscreen?) and a true separate-process output binary for full
  crash isolation remains a follow-up.
- [~] Code signing + notarization + installers + auto-update (Phase 13.2):
  pipeline wired — tauri.conf updater config (embedded pubkey), updater +
  process plugins, UpdateBanner, and `release.yml` (tag `v*` → signed/
  notarized mac+win draft release + `latest.json`). **Runs once the repo
  secrets in docs/DISTRIBUTION.md are set.** Pending: GitHub secrets, a
  Windows signing cert, and one native end-to-end update test. Private
  updater key lives at `~/.tauri/sundaystage_updater.key` (outside the repo).
- [ ] Native file dialog / drag-drop import + ffmpeg thumbnails (Phase 7.2).
- [ ] SundayRec bridge transport (loopback HTTP + mDNS + pairing) (Phase 10.1).
- [ ] TONO streaming-licence audit (Phase 10.2 feature 3).
- [ ] Cloud sync + team collaboration (Phase 9), semantic search (Phase 11.1),
      companion PWA (Phase 12).
- [ ] Opt-in crash reporting (Phase 6.1).
- [ ] Bundled, license-cleared background image set + 20-song starter library.
- [ ] Landing site sundaystage.com (Phase 13.3).

## Quality gates in place

- `cargo test --lib` — 189 unit tests.
- `cargo test --test stress` — performance budgets (FTS, cue advance,
  arrangement resolve).
- `cargo check --features ai` — the live Claude client compiles.
- `tsc --noEmit` + `vite build` — frontend type-checks and bundles.
