# Architecture Decision Records (ADR)

Sequential. Append new decisions; do not retroactively edit accepted ones —
mark them `Superseded` instead.

---

## ADR-001 — Tauri 2 over Electron

**Status:** Accepted (2026-05-27)

### Context

We need a desktop app for Mac and Windows that:
- Starts fast (volunteers' machines are modest)
- Memory-efficient (4 outputs at 1080p, 4-hour session, no memory growth)
- Can spawn isolated output processes
- Lets us write the live engine in Rust (matters for reliability)

### Decision

Tauri 2 + Rust backend.

### Consequences

**Positive:**
- Smaller bundle (~10 MB vs Electron's ~100+ MB)
- Smaller memory footprint
- Native multi-process from day one
- Rust for the hot path (live engine)

**Negative:**
- Smaller ecosystem than Electron
- Some plugins are less mature
- Steeper Rust learning curve
- Build complexity (cargo + node both required)

### Alternatives considered

- **Electron**: rejected for bundle/memory and the SundayRec team's existing
  knowledge — we want to push the envelope here, not match.
- **Native (Swift + WinUI)**: rejected for cross-platform cost — two
  codebases, two skill sets.
- **Flutter**: rejected — weak desktop story for our specific use case
  (multi-window, system tray, native menus on Mac).

---

## ADR-002 — SQLite as primary store

**Status:** Accepted (2026-05-27)

### Context

We need a local-first database. Cloud sync is optional.

### Decision

SQLite via `sqlx` with compile-time-checked queries.

### Consequences

- One file per library. Easy backup.
- Battle-tested. Every OS has it.
- FTS5 gives us fast lyric search without an external index.
- `sqlx` macros catch query bugs at compile time.

### Alternatives considered

- **SurrealDB embedded**: too young.
- **DuckDB**: great for analytics, overkill for a UI app.
- **PostgreSQL local**: external process to manage — friction.

---

## ADR-003 — UUIDv7 keys, unix-ms timestamps

**Status:** Accepted (2026-05-27)

### Context

Need globally unique, sortable IDs that work across offline devices for
sync. Need timestamps that survive timezone changes without bugs.

### Decision

- IDs: UUIDv7 stored as TEXT
- Timestamps: i64 unix milliseconds

### Consequences

- 26-char keys (overhead acceptable for desktop scale)
- Sort by ID = sort by creation time roughly
- All time math in renderer / Rust uses ms — no Date strings on the wire

---

## ADR-004 — Output processes isolated from UI

**Status:** Accepted (2026-05-27)

### Context

The "never crash on Sunday morning" promise. UI bugs must not take down
the live output.

### Decision

`sundaystage-output` is a separate Rust binary. The main app spawns one
per active display when "Go Live" is pressed. Communication via local
IPC (Tauri's IPC, named pipes / Unix sockets — finalized in Phase 5.2).

### Consequences

- Output keeps last slide if main crashes
- Watchdog heartbeat from main; output kills connection on timeout
  **without blanking** the display
- More complex deployment (two binaries to sign and notarize)
- More complex local dev (`cargo run` spawns child binary)

### Alternatives considered

- **Single process with try/catch**: rejected — a panic in the UI thread
  still takes down the whole app. We need OS-level process boundaries.

---

## ADR-005 — TanStack Query for server state, Zustand for UI state

**Status:** Accepted (2026-05-27)

### Context

Need a state architecture that scales beyond useState but doesn't drag
in Redux baggage.

### Decision

- Server-state (everything that comes from Rust): TanStack Query
- UI-state (selection, panel sizes, modal open): Zustand
- No Redux. No global mutable singletons.

### Consequences

- Query keys are the contract between Rust commands and React
- Optimistic updates via `useMutation.onMutate`
- Zustand store split per feature (no monolithic global store)

---

## ADR-006 — Tailwind v4 with `@theme` design tokens

**Status:** Accepted (2026-05-27)

### Context

Need a design system that's customizable, dark-first, and matches the
SundayRec brand family (gold-on-blue).

### Decision

- Tailwind v4 with `@theme` for tokens
- shadcn/ui primitives as starting point — copied into our repo, not
  npm-installed (per shadcn philosophy)
- Two parallel type scales: `ui-*` for chrome, `stage-*` for projector
  output (where sizes are dramatically larger — 64px → 144px)

### Consequences

- Tokens live in `src/styles/tokens.css`
- Components reference semantic aliases (`--color-bg`, `--color-accent`)
  not raw tokens
- Dark/light mode via system + manual toggle

---

## ADR-007 — TONO `tono_work_id` first-class on `Song`

**Status:** Accepted (2026-05-27)

### Context

Norwegian frikirker need TONO reporting in addition to CCLI. Most
international worship-tech treats TONO as an afterthought. The
Sunday-suite differentiator is making this seamless across SundayStage,
SundayPlan, SundayRec, and SundaySong.

### Decision

`Song.tono_work_id` is a first-class nullable column. Will be populated
from SundaySong's catalog when connected; manually entered otherwise.

### Consequences

- One more column users won't fill in for foreign worship music — that's
  fine, it's nullable
- SundaySong-integration (Phase 7 of SundaySong plan) can sync this
  automatically once Sunday account OIDC is wired

---

## ADR-008 — `last_used_at` denormalized on `Song`

**Status:** Accepted (2026-05-27)

### Context

The variety / fairness scoring engine in SundayPlan queries "songs used
in last N weeks" frequently. Computing from `service_item` joins is
expensive enough that we cache the answer.

### Decision

`Song.last_used_at INTEGER` is updated by a trigger when a `Service`
transitions to `played` state (Phase 5).

### Consequences

- Read-side query is trivial and fast
- Write-side: trigger must handle re-orderings and deletions of service items
- Sync conflict: if two devices play the same service in different time
  zones, last-write-wins on `last_used_at` is fine
