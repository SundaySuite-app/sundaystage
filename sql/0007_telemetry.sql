-- 0007 — local observation ground floor (E3).
--
-- Two tables, both write-only from the app's point of view until E5 adds the
-- sender. Nothing here is transmitted anywhere; this migration only gives the
-- counters and the per-session quality rows somewhere durable to live.
--
-- PRIVACY (law 2 of docs/TELEMETRY-PROGRAM.md): every column below is a
-- number, a boolean, or a value from a CLOSED enum. There is deliberately no
-- column that can hold a service name, a song title, a display name or a path
-- — not because the writer promises not to put one there, but because there is
-- nowhere to put it.
--
-- ADDITIVE: two new tables and their indexes. No existing table is touched, so
-- an older build reading this database simply ignores them.

-- ── Counters ────────────────────────────────────────────────────────────────
-- One row per name from the closed vocabulary in
-- `src-tauri/src/telemetry/counters.rs`. Rows appear the first time a counter
-- is folded in, so a fresh install has an empty table rather than 19 zeroes.
--
-- `sent_value` is the watermark E5 drains against: everything at or below it
-- has already been reported. Added now (as 0, meaning "nothing reported yet")
-- so E5 needs no second migration and an old build's totals are not
-- misread as unsent by a newer one.
CREATE TABLE telemetry_counter (
    name       TEXT PRIMARY KEY,
    value      INTEGER NOT NULL DEFAULT 0,
    sent_value INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);

-- ── Quality: one row per live session ───────────────────────────────────────
-- Written ONLY when no live session is running (see
-- `telemetry::quality::drain_if_quiet`). Booleans are 0/1 INTEGERs, the SQLite
-- convention this schema already uses for `is_builtin`.
CREATE TABLE telemetry_quality (
    id                    TEXT PRIMARY KEY,
    -- Stable key for rows that can be produced more than once: the startup
    -- reconstruction sees the same leftover `live_session.log` on every launch
    -- until the operator resumes or ends it. NULL for ordinary rows, so the
    -- partial unique index below only constrains the reconstructed ones.
    dedupe_key            TEXT,
    at                    INTEGER NOT NULL,
    duration_sec          INTEGER NOT NULL,
    cue_count             INTEGER NOT NULL,
    output_child_restarts INTEGER NOT NULL,
    connect_timeouts      INTEGER NOT NULL,
    watchdog_holds        INTEGER NOT NULL,
    dispatch_errors       INTEGER NOT NULL,
    companion_failures    INTEGER NOT NULL,
    fallback_used         INTEGER NOT NULL,
    stale_child_reaped    INTEGER NOT NULL,
    abnormal_end          INTEGER NOT NULL,
    recovered             INTEGER NOT NULL,
    -- NULL when no cue was ACKed: no isolated outputs were open, or the
    -- service had no cues. Absence of a measurement is not a measurement of 0.
    cue_latency_p95_ms    INTEGER,
    verdict               TEXT NOT NULL
                            CHECK (verdict IN ('pass', 'warn', 'fail')),
    -- JSON array of the closed reason codes. Never prose, never empty: a clean
    -- session carries ["clean"].
    reasons               TEXT NOT NULL,
    -- Unix ms this row was PERSISTED, which is not `at` — a row written after
    -- a service ends can sit in the in-memory buffer until the drain's live
    -- gate opens.
    created_at            INTEGER NOT NULL,
    -- E5's send watermark. NULL = never reported.
    sent_at               INTEGER
);

CREATE UNIQUE INDEX idx_telemetry_quality_dedupe
    ON telemetry_quality(dedupe_key) WHERE dedupe_key IS NOT NULL;
CREATE INDEX idx_telemetry_quality_at ON telemetry_quality(at);
CREATE INDEX idx_telemetry_quality_unsent ON telemetry_quality(at) WHERE sent_at IS NULL;

INSERT INTO schema_migrations (version, applied_at, description)
VALUES (7, unixepoch() * 1000, 'telemetry counters + per-session quality rows (E3)');
