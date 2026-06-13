-- 0005 — live AI translation overlay (Phase 11.2).
--
-- Two additive pieces:
--   1. service.secondary_language — the per-service target language for a
--      translated line shown UNDER the primary line on the output / stage
--      display. NULL = no overlay (today's behaviour); a row deserializes
--      unchanged because the column is nullable with no default.
--   2. translation_cache — an OFFLINE cache of one translated line keyed by its
--      exact source text + target language. Filled at cue-COMPILE time (when a
--      key is present) and reused forever after, so a Sunday with no network
--      still renders every line that was ever translated. Bundled Bible
--      passages are served from `services::bible::bundled_translations()`
--      directly and need no cache row.
--
-- Idempotent + additive: IF NOT EXISTS on the table; the ALTER is guarded by
-- the migration runner only applying 0005 once (sqlx tracks applied versions).

ALTER TABLE service ADD COLUMN secondary_language TEXT;

CREATE TABLE IF NOT EXISTS translation_cache (
    -- The exact source line as it appears on a slide (one cue line).
    source_text      TEXT NOT NULL,
    -- ISO-639-1 target language code (e.g. 'en', 'no').
    target_language  TEXT NOT NULL,
    -- The translated line. Empty string is a legitimate cached value (a blank
    -- source line translates to blank) so presence — not content — is the hit.
    translated_text  TEXT NOT NULL,
    -- Provenance for diagnostics / cache invalidation: 'ai' | 'bundled'.
    source           TEXT NOT NULL DEFAULT 'ai',
    created_at       INTEGER NOT NULL,
    PRIMARY KEY (source_text, target_language)
);

INSERT INTO schema_migrations (version, applied_at, description)
VALUES (5, unixepoch() * 1000, 'Live translation overlay — secondary_language + translation_cache (Phase 11.2)');
