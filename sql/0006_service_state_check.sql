-- 0006 — constrain service.state to the four known lifecycle values (A2).
--
-- service.state has always been documented as one of
--   'planned' | 'in_progress' | 'played' | 'archived'
-- but nothing enforced it at the DB level, so a typo'd write (or a future bug)
-- could silently store a state the live engine / planner never expects. This
-- migration adds a CHECK constraint that rejects any other value.
--
-- SQLite can't ALTER TABLE ... ADD CONSTRAINT, so this rebuilds the table:
-- create a copy WITH the constraint, copy every row verbatim (preserving ids,
-- so the FK from service_item.service_id stays valid), drop the old table,
-- rename. `defer_foreign_keys` postpones FK enforcement to COMMIT — by then the
-- new `service` holds the same ids, so child rows still resolve. This is the
-- documented SQLite procedure (https://www.sqlite.org/lang_altertable.html).
--
-- ADDITIVE + backward-compatible: every existing row is `planned` (the only
-- value the code has ever written), so the constraint can't reject existing
-- data. The column keeps its NOT NULL + DEFAULT 'planned'.

PRAGMA defer_foreign_keys = ON;

CREATE TABLE service_new (
    id                 TEXT PRIMARY KEY,
    library_id         TEXT NOT NULL REFERENCES library(id) ON DELETE CASCADE,
    name               TEXT NOT NULL,
    starts_at          INTEGER NOT NULL,
    notes              TEXT,
    state              TEXT NOT NULL DEFAULT 'planned'
                         CHECK (state IN ('planned', 'in_progress', 'played', 'archived')),
    secondary_language TEXT,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    deleted_at         INTEGER
);

INSERT INTO service_new
    (id, library_id, name, starts_at, notes, state, secondary_language,
     created_at, updated_at, deleted_at)
SELECT
    id, library_id, name, starts_at, notes, state, secondary_language,
    created_at, updated_at, deleted_at
FROM service;

DROP TABLE service;

ALTER TABLE service_new RENAME TO service;

CREATE INDEX idx_service_library_date ON service(library_id, starts_at) WHERE deleted_at IS NULL;

INSERT INTO schema_migrations (version, applied_at, description)
VALUES (6, unixepoch() * 1000, 'CHECK constraint on service.state (planned|in_progress|played|archived)');
