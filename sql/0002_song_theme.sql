-- 0002 — per-song theme/template overrides (Phase 3.2).
--
-- Completes the cascade documented in docs/ARCHITECTURE.md (Q5):
--   slide override > song override > library default > built-in default
-- Built-in themes/templates live in code (services::theme), so these columns
-- only ever hold library-owned ids; ON DELETE SET NULL drops the override if
-- the referenced theme/template is deleted, degrading to the next cascade level.

ALTER TABLE song ADD COLUMN theme_id    TEXT REFERENCES theme(id)    ON DELETE SET NULL;
ALTER TABLE song ADD COLUMN template_id TEXT REFERENCES template(id) ON DELETE SET NULL;

INSERT INTO schema_migrations (version, applied_at, description)
VALUES (2, unixepoch() * 1000, 'Per-song theme/template overrides — Phase 3.2');
