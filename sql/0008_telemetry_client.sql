-- 0008 — the telemetry CLIENT half (E5): consent, install id, outbox.
--
-- E3 (migration 0007) gave the counters and the per-session quality rows
-- somewhere durable to live. This migration adds the two things a client needs
-- before anything could ever be sent: a place to record what the user ANSWERED,
-- and a place to hold a built payload until it is delivered.
--
-- Nothing here causes anything to be sent. With no consent UI in this stage
-- (that is E6), the consent row is never written, `evaluate(None)` is
-- `NeverAsked`, and every downstream path — install id, drain, outbox, sender —
-- is inert by construction. That is the point of landing the machinery first:
-- E6 adds a question, not a pipeline.
--
-- PRIVACY (law 2 of docs/TELEMETRY-PROGRAM.md): `telemetry_state` is a
-- key/value bag with a CLOSED key vocabulary (see `telemetry::client::StateKey`)
-- and `telemetry_outbox.payload_json` holds a payload that has ALREADY been
-- through the builder's scrubbers and caps. Storing the rendered JSON rather
-- than a struct is deliberate: a row queued by version N is sent unchanged by
-- version N+1, so the bytes a user could inspect in the preview are the bytes
-- that leave, even across an update that changed the builder.
--
-- ADDITIVE: two new tables. No existing table is touched, so an older build
-- reading this database simply ignores them.

-- ── Client state: consent, install id, watermarks, parked deletions ─────────
-- A key/value bag rather than a wide row, for the same reason SundayRec uses
-- one: each key is written by a different part of the client at a different
-- moment, and a wide row would make every write a read-modify-write over facts
-- its caller has no business touching.
--
-- ABSENCE IS MEANINGFUL HERE. No `telemetry.consent` row means "never asked",
-- which is NOT the same as "no" — see `telemetry::consent`. So there is
-- deliberately no seed row and no default: a fresh install has an empty table.
CREATE TABLE telemetry_state (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- ── The outbox ──────────────────────────────────────────────────────────────
-- Payloads built and waiting for delivery. Bounded to 50 rows in code
-- (`telemetry::outbox::QUEUE_MAX`), dropping the OLDEST — a laptop that spends
-- a winter offline must not accumulate a megabyte of stale reports about a
-- version nobody runs any more.
CREATE TABLE telemetry_outbox (
    id           TEXT PRIMARY KEY,
    -- Unix ms the payload was built and queued.
    created_at   INTEGER NOT NULL,
    -- The payload schema version the row was built at, so a reader can tell an
    -- old queued payload from a new one without parsing the JSON.
    schema_ver   INTEGER NOT NULL,
    -- What this payload is about (`crash:<ts>`, `quality:<ts>`,
    -- `counters:<ts>`). UNIQUE, so two drains racing to report the same batch
    -- produce one row rather than two.
    dedup_key    TEXT NOT NULL UNIQUE,
    payload_json TEXT NOT NULL,
    attempts     INTEGER NOT NULL DEFAULT 0,
    -- Unix ms — the earliest the sender may try this entry.
    next_attempt INTEGER NOT NULL,
    -- The most recent transient failure, for an honest queue-status line. A
    -- PERMANENT rejection never lands here: the row is dropped and the reason
    -- goes to the local log (the ellipsis lesson — never a silent loss).
    last_error   TEXT,
    status       TEXT NOT NULL DEFAULT 'pending'
                   CHECK (status IN ('pending', 'sending', 'failed'))
);

-- The sender's selection query: pending rows whose time has come, earliest
-- first.
CREATE INDEX idx_telemetry_outbox_due
    ON telemetry_outbox(next_attempt) WHERE status = 'pending';
-- The overflow bound's ordering.
CREATE INDEX idx_telemetry_outbox_age ON telemetry_outbox(created_at);

INSERT INTO schema_migrations (version, applied_at, description)
VALUES (8, unixepoch() * 1000, 'telemetry client: consent state + outbox (E5)');
