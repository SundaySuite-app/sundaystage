-- Migration 0004: Service template system
-- A service template is a reusable cue-spec list that can be applied to any
-- service. Each cue spec describes a slot in the template (kind + label).

CREATE TABLE IF NOT EXISTS service_template (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT,
    cue_specs    TEXT NOT NULL DEFAULT '[]', -- JSON: CueSpec[]
    is_builtin   INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

INSERT INTO schema_migrations (version, applied_at, description)
VALUES (4, unixepoch() * 1000, 'Service template system');
