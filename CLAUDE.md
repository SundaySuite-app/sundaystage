# CLAUDE.md — SundayStage

SundayStage is a live presentation application for churches. It is the visual stage companion to SundayRec (sermon recording/streaming). Together they are the "Sunday" suite.

## Core promises

1. **NEVER crash on a Sunday morning.** The live output is sacrosanct.
2. **AI does the boring work** (formatting lyrics, finding songs, structuring services).
3. **Volunteers can run it after 10 minutes of training.**
4. **Free tier is genuinely useful forever**; Pro tier is genuinely cheap.
5. **Mac and Windows are first-class equals.**

## Competitive positioning

- vs **ProPresenter** ($29-59/mo): we are 1/5 the price, more stable, AI-native
- vs **EasyWorship**: we are cross-platform and modern
- vs **FreeShow** (open source): we are faster (Tauri vs Electron), have real AI, integrate with SundayRec
- vs **QLab**: we steal their cue-list reliability philosophy

## Tech principles

- **Local-first.** Cloud is optional sync, never required.
- **The live output process is isolated from the UI process.** If the editor crashes, the stage does not.
- **Privacy:** AI features can run via Anthropic API but offer offline fallback where possible.
- **Languages:** English, Norwegian, Swedish, Danish, German, French, Polish (match SundayRec).

## Stack

- **Tauri 2** (Rust backend) + React 19 + TypeScript + Tailwind CSS v4
- **shadcn/ui** primitives as base (customized)
- **TanStack Query** (server state) + **Zustand** (UI state)
- **TanStack Router** (file-based routing not used — typed routes preferred)
- **SQLite** via `sqlx` (local-first storage)
- **cmdk** for command palette
- **lucide-react** for icons

## Folder structure

```
src/                   React frontend
├── app/               Route/page-level components
├── features/          Feature modules (library, editor, live, bible, ...)
├── components/        Shared UI primitives
├── lib/               Utilities, hooks, IPC client
└── styles/            Globals, design tokens

src-tauri/             Rust backend
└── src/
    ├── commands/      Tauri command handlers
    ├── db/            Database connection + repositories
    │   └── repositories/
    ├── services/      Business logic
    ├── output/        Live output rendering process
    ├── lib.rs
    └── main.rs

sql/                   Database migrations (versioned)
docs/                  Architecture, decisions, protocols
```

## Conventions

- Tauri commands NEVER talk to `sqlx` directly — they go through repositories
- All IDs are UUIDs (v7 for sortability) stored as TEXT in SQLite
- Timestamps are i64 unix millis (avoids timezone bugs)
- Every domain entity has `created_at`, `updated_at`, `deleted_at` (soft delete where appropriate)
- Error handling: `thiserror`-based `AppError`, never `unwrap()` in production code
- All public functions are `async fn` returning `Result<T, AppError>`
- TypeScript: strict mode, no `any`, no unused vars

## Performance budgets

- App start: < 2 seconds cold
- Library open with 10k songs: < 500ms to interactive
- Cue advance (keypress → output change): < 50ms p95
- Search (10k songs): < 100ms
- Multi-output 4×1080p, 4-hour session: < 50MB memory growth
