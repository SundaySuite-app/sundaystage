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

| Lang | Nav/chrome | Full app strings    |
| ---- | ---------- | ------------------- |
| en   | ✅         | ✅ (base)           |
| no   | ✅         | ✅                  |
| sv   | ✅         | ⛔ falls back to en |
| da   | ✅         | ⛔ falls back to en |
| de   | ✅         | ⛔ falls back to en |
| fr   | ✅         | ⛔ falls back to en |
| pl   | ✅         | ⛔ falls back to en |

- [ ] Route **every** user-visible string through `t()` (large mechanical pass;
      currently the shell/nav is done, feature pages are still mostly Norwegian
      literals).
- [ ] Machine-translate sv/da/de/fr/pl catalogs (via Claude) **and human-review
      each** before public release.

## Remaining before 1.0 (deferred / needs infra this environment can't provide)

- [ ] Interactive 5-step tutorial overlay (library → editor → live).
- [~] Multi-display output + per-screen role assignment (Phase 5.2): shipped as
  borderless full-screen Tauri output windows (one per monitor) driven by an
  event-bus render/heartbeat from the operator UI, with a JS watchdog that
  holds the last frame. **Needs native verification** (does a window land on
  monitor 2, fullscreen?) and a true separate-process output binary for full
  crash isolation remains a follow-up.
- [ ] Code signing + notarization + installers + auto-update (Phase 13.2):
      Apple Developer ID, Windows cert, Tauri updater, GitHub Releases CI.
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
