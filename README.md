# SundayStage

Live presentation app for churches — companion to [SundayRec](https://github.com/richardfossland/sundayrec). Together they are the **Sunday suite**.

> **Status (June 2026):** feature-complete for local-only churches — phases 3–11 + 13.1 are implemented and unit-tested. The slide editor, AI lyric formatter (with an offline heuristic and OS-keychain key storage), the live engine with **real crash-isolated output processes**, crash recovery, Bible + media, stage-display presets, the SundayRec bridge (chapter markers + SRT), and the planning assistant all work today. Still ahead: cloud sync transport (9.2), distribution / auto-update (13.2), semantic search + live translation overlay (11), and video thumbnails. The network companion (phones/extra screens following along) now lives in **SundayStage Web** (`stage.sundaysuite.app`), which supersedes the old in-repo companion PWA. See `docs/ARCHITECTURE.md` for the canonical phase-status table and `CLAUDE.md` for the design principles.

## Why SundayStage exists

|                            | ProPresenter | EasyWorship | FreeShow | **SundayStage**           |
| -------------------------- | ------------ | ----------- | -------- | ------------------------- |
| Price                      | $29-59/mo    | One-time    | Free OSS | **Free core, cheap Pro**  |
| Cross-platform             | Mac+Win      | Win-leaning | Yes      | **Mac+Win first-class**   |
| AI-native lyric formatting | No           | No          | No       | **Yes**                   |
| Stable on Sunday morning   | Has crashed  | OK          | Decent   | **Crash-isolated output** |
| Pair with church recorder  | No           | No          | No       | **SundayRec bridge**      |

## Stack

- **Tauri 2** (Rust backend) + React 19 + TypeScript
- **Tailwind v4** with `@theme` design tokens — see `src/styles/tokens.css`
- **shadcn/ui** primitives (copied not installed, per shadcn philosophy)
- **TanStack Query** for server state, **Zustand** for UI state
- **SQLite** via `sqlx` with compile-time-checked queries
- **cmdk** for the ⌘K command palette
- **ts-rs** for auto-generated TypeScript bindings from Rust models

## Repository layout

```
src/                   React frontend
├── components/        Shared UI primitives (Sidebar, CommandPalette, ...)
├── features/          Feature modules (library, editor, live, bible, ...)
├── lib/               IPC client, bindings, hooks, utils
└── styles/            Tokens + globals (Tailwind v4 @theme)

src-tauri/             Rust backend
└── src/
    ├── commands/      Tauri command handlers (thin — call into db/)
    ├── db/
    │   ├── models.rs       sqlx + serde + ts-rs derives
    │   └── repositories/   one file per aggregate root
    ├── services/      Business logic (live engine, AI, sync — later phases)
    ├── output/        Isolated live-output binary (Phase 5.2)
    ├── error.rs       Centralised AppError
    └── lib.rs         Tauri runtime entry

sql/                   Database migrations (sqlx::migrate!("../sql"))
docs/
├── ARCHITECTURE.md    Mermaid ERD + entity reference + hardest queries
└── DECISIONS.md       ADRs
```

## Development

Requires:

- Rust stable (`rustup`)
- Node 24+ + pnpm (the `@sunday/contracts` git dependency uses pnpm's `&path:`
  git-subdir syntax, which npm cannot resolve — use pnpm, not npm)
- macOS: Xcode command-line tools

```bash
cd sundaystage
pnpm install
pnpm run tauri dev          # builds Rust + starts Vite + opens app
```

Frontend-only dev (no Rust hot reload):

```bash
pnpm run dev
```

Run Rust unit tests:

```bash
cd src-tauri
cargo test --lib
```

Regenerate TypeScript bindings from Rust models:

```bash
cd src-tauri
cargo test --lib export_bindings    # writes to src/lib/bindings/*.ts
```

## What works today

- ✅ **Slide editor** — canvas with sections, arrangements, theme/template cascade (Phase 3)
- ✅ **AI lyric formatter** — Anthropic-backed + an offline heuristic fallback; key in the OS keychain (Phase 4)
- ✅ **Live engine** — cue compiler + O(1) runtime, **crash-isolated output processes** over local IPC with a hold-last-frame watchdog (Phase 5)
- ✅ **Theme/template cascade on output** — each cue paints its resolved colour/font/scale (audit 2c)
- ✅ **Crash recovery** — append-only session log, resume at the same cue after a UI crash; stress-tested (Phase 6)
- ✅ **Bible + media** — cached translations, fingerprint-based media relink (Phase 7)
- ✅ **Stage-display presets** — Worship Leader / Musician / Pastor views (Phase 8)
- ✅ **SundayRec bridge** — chapter markers + SRT export from the cue log (Phase 10)
- ✅ **Planning assistant** — AI service-plan draft that never invents unknown songs (Phase 11.2)
- ✅ **Onboarding + i18n machinery** — 7 locales, demo content, first-run flow (Phase 13.1)
- ✅ 411 Rust unit tests + output-process isolation/stress suites, all green

## What's next

See `docs/ARCHITECTURE.md` — the phase-status table is the canonical roadmap. Remaining:

- **Phase 9.2** — Supabase sync transport (the decision/conflict core is done; the cloud wire is not)
- **Phase 11** — semantic song search (embeddings) + live translation overlay
- **Phase 13.2** — distribution: signed bundles + auto-update CI, landing site
- **Media** — ffprobe-backed video thumbnails
- **Network companion** — phones and extra screens follow along via **SundayStage Web** (`stage.sundaysuite.app`), not an in-repo PWA

## License

TBD — likely AGPL-3.0 to align with the open philosophy of the Sunday suite. Final decision before public launch.
