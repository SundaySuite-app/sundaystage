# SundayStage cloud backend (Phase 9)

Supabase (Postgres + Auth + Realtime) backs **Sunday Pro** cloud sync and
**Sunday Team** sharing. The free tier never touches this — the local SQLite is
always the running app's source of truth, and sync is best-effort background
replication that is **suspended while a service is live**.

## What's here

- `supabase/migrations/0001_schema.sql` — the cloud mirror of the syncable
  entities (library, song, section, arrangement, service, item) with sync
  metadata, plus `library_member` (owner/editor/operator) for team sharing.
- `supabase/migrations/0002_rls.sql` — Row Level Security: read requires
  library membership, write requires editor/owner; operators are read-only.

## Setup (when ready)

```sh
cd backend
supabase init          # if not already
supabase start         # local stack (needs Docker)
supabase db reset      # apply migrations + verify RLS
# or against a hosted project:
supabase link --project-ref <ref>
supabase db push
```

## Status — deliberately deferred

The **pure sync engine** (conflict resolution, the live-suspend gate, the status
indicator, and the outbox coalescing) is implemented and unit-tested in
`src-tauri/src/services/sync.rs`. What remains needs a network + an account this
environment can't provide, and so is not built yet:

- **App-side transport**: the outbox/inbox HTTP client against Supabase
  (push coalesced mutations, pull remote changes, apply via `resolve`).
- **Auth**: email magic-link / OAuth via Supabase Auth; storing the session.
- **Realtime**: presence (who else is in the library) + the companion broadcast
  channel `companion:{church}:{service}` that the Phase 12 PWA subscribes to.
- **Team UX**: invite-by-email, soft locks, activity feed, comments.

These migrations are correct-by-construction but **unverified** until run
against a real Supabase instance — apply + test RLS before relying on them.
