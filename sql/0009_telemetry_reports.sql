-- 0009 — manual problem reports (E6).
--
-- The one part of the payload an operator WRITES rather than one the app
-- observes. E5 shipped `reports: []` on the wire from day one (the Worker
-- requires every top-level key); this migration gives those reports somewhere
-- to live between "the operator pressed send" and "the bytes were actually
-- included in a payload".
--
-- WHY A TABLE AT ALL, when a report could be handed straight to the sender:
-- because the byte cap can trim a payload, and `telemetry::payload`'s trimming
-- order treats reports as "free to defer" — free ONLY IF a deferred report is
-- still on disk to go in the next one. A report that lived solely in the
-- outgoing buffer would be the one place an operator's hand-written words could
-- vanish without a trace, which is precisely what law 2's ellipsis lesson is
-- about.
--
-- PRIVACY (law 2 of docs/TELEMETRY-PROGRAM.md): `message` and `log_tail` are
-- the only free-text columns in the whole telemetry schema. Both are written
-- ALREADY SCRUBBED and ALREADY CAPPED (`ProblemReport::new` →
-- `scrub::free_text`), so the row on disk is byte-identical to what would be
-- sent — there is no second truncation pass that could disagree with the
-- Worker's validator, and the dialog's preview shows these exact bytes.
--
-- ADDITIVE: one new table. No existing table is touched.

CREATE TABLE telemetry_report (
    id         TEXT PRIMARY KEY,
    -- Unix ms the operator submitted it. This is the `at` that goes on the wire.
    at         INTEGER NOT NULL,
    -- Where the operator was. The one field of a report that is not free text,
    -- and a CLOSED set both here and in `telemetry::payload::ReportContext`.
    context    TEXT NOT NULL
                 CHECK (context IN ('live', 'editor', 'settings', 'other')),
    -- Scrubbed and capped at MESSAGE_MAX_CHARS (200) before it got here.
    message    TEXT NOT NULL,
    -- Scrubbed and capped at LOG_TAIL_MAX_CHARS (4000) before it got here. The
    -- most dangerous free-text line in the system — see the module docs for the
    -- three defences that stand in front of it.
    log_tail   TEXT NOT NULL,
    -- 1 when standing consent was OFF at submit time. Such a report is sent on
    -- its own, under a ONE-SHOT random id that is generated at send time and
    -- never stored anywhere — not in this table, not in `telemetry_state`, not
    -- in the outbox. Pressing send is consent for this one report and nothing
    -- else, so it must not be possible to tie two of them together afterwards.
    ephemeral  INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    -- Unix ms the report was actually INCLUDED in a payload (queued for the
    -- durable path, delivered for the ephemeral one). NULL = still owed.
    sent_at    INTEGER
);

-- The two selection queries: what the drain owes, and what the one-shot sender
-- owes.
CREATE INDEX idx_telemetry_report_unsent
    ON telemetry_report(at) WHERE sent_at IS NULL;

INSERT INTO schema_migrations (version, applied_at, description)
VALUES (9, unixepoch() * 1000, 'telemetry: manual problem reports (E6)');
