# SundayStage

Live presentation app for churches — companion to [SundayRec](https://github.com/richardfossland/sundayrec). Together they are the **Sunday suite**.

> ⚠️ **Status:** early alpha. Phase 0–2 scaffold complete. Live engine, AI lyric formatter, multi-output isolation, and SundayRec integration not yet implemented. See `docs/ARCHITECTURE.md` for the full roadmap and `CLAUDE.md` for the design principles.

## Why SundayStage exists

|                            | ProPresenter | EasyWorship | FreeShow | **SundayStage**               |
| -------------------------- | ------------ | ----------- | -------- | ----------------------------- |
| Price                      | $29-59/mo    | One-time    | Free OSS | **Free core, cheap Pro**      |
| Cross-platform             | Mac+Win      | Win-leaning | Yes      | **Mac+Win first-class**       |
| AI-native lyric formatting | No           | No          | No       | **Yes — Phase 4**             |
| Stable on Sunday morning   | Has crashed  | OK          | Decent   | **Crash isolation by design** |
| Pair with church recorder  | No           | No          | No       | **SundayRec — Phase 10**      |

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
- Node 24+ + npm
- macOS: Xcode command-line tools

```bash
cd sundaystage
npm install
npm run tauri dev           # builds Rust + starts Vite + opens app
```

Frontend-only dev (no Rust hot reload):

```bash
npm run dev
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

## What works today (Phase 0–2 scaffold)

- ✅ Tauri 2 + React 19 boots
- ✅ SQLite database opens in app-local data dir; runs migrations
- ✅ All 16 schema entities (Library, Song, Section, Service, ...) modelled in Rust
- ✅ Repositories: Library, Song (incl. FTS5 search), Service, Media, Bible
- ✅ 14 Tauri commands wired and callable from React
- ✅ Typed IPC client (`src/lib/ipc.ts`) with stable error shape
- ✅ Auto-generated TS bindings from Rust (17 types)
- ✅ App shell: sidebar nav, ⌘K command palette, dark-first theme
- ✅ Library page: list + create + search (FTS5-backed full-text)
- ✅ 30 Rust unit tests, all green

## What's next (Phase 3 → 13)

See `docs/ARCHITECTURE.md` — the phase-status table at the bottom is the canonical roadmap. Highlights:

- **Phase 3** — Slide editor (Figma-like canvas, snap guides, undo/redo)
- **Phase 4** — AI lyric formatter (the killer feature)
- **Phase 5** — Live engine with isolated output processes ⚠ **critical**
- **Phase 6** — Stress testing + crash recovery (the moat)
- **Phase 10** — SundayRec integration (chapter markers, SRT captions, TONO licensing flag)
- **Phase 12** — Companion PWA (follow-along for accessibility)

## License

TBD — likely AGPL-3.0 to align with the open philosophy of the Sunday suite. Final decision before public launch.
