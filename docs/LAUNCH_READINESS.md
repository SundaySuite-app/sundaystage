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
- [ ] Localise section-type labels on the **live output** (`SlideView` /
      operator preview). These read `slide_content.section_label`, which the
      Rust cue compiler pre-title-cases via `humanize_section_label` (asserted as
      "Verse 1" in tests). Doing it right means making the cue compiler
      locale-aware (single source for operator preview + real output + cue-list
      `display_label`) rather than coupling the deliberately hook-free output
      renderer to the locale store. `localizeSectionLabel()` already normalises
      the title-cased form, so the frontend half is ready once the backend emits
      canonical labels (or a locale).

## Remaining before 1.0 (deferred / needs infra this environment can't provide)

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
