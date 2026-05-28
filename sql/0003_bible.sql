-- Phase 7.1 — Bible text storage.
--
-- `bible_reference` (0001) is the per-service CACHE of a chosen passage. These
-- new tables are the BROWSABLE library: full translations made of verses, with
-- FTS5 for "Jesus wept"-style search. A translation is a bundled public-domain
-- text (KJV, Bibelen 1930) or, later, a downloaded one.

CREATE TABLE IF NOT EXISTS bible_translation (
    id            TEXT PRIMARY KEY,
    code          TEXT NOT NULL UNIQUE,      -- e.g. "KJV", "NB1930"
    name          TEXT NOT NULL,
    language      TEXT NOT NULL,             -- ISO-639-1: "en", "no"
    public_domain INTEGER NOT NULL DEFAULT 1,
    created_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS bible_verse (
    id             TEXT PRIMARY KEY,
    translation_id TEXT NOT NULL REFERENCES bible_translation(id) ON DELETE CASCADE,
    book           TEXT NOT NULL,            -- canonical English book name
    book_order     INTEGER NOT NULL,         -- canonical ordering (1..66)
    chapter        INTEGER NOT NULL,
    verse          INTEGER NOT NULL,
    text           TEXT NOT NULL,
    created_at     INTEGER NOT NULL,
    UNIQUE (translation_id, book, chapter, verse)
);
CREATE INDEX IF NOT EXISTS idx_bible_verse_passage
    ON bible_verse(translation_id, book_order, chapter, verse);

-- ── FTS5 — full-text search across verse text ──────────────────────────────
CREATE VIRTUAL TABLE IF NOT EXISTS bible_verse_search USING fts5 (
    verse_id       UNINDEXED,
    translation_id UNINDEXED,
    text,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS trg_bible_verse_after_insert
AFTER INSERT ON bible_verse BEGIN
    INSERT INTO bible_verse_search(verse_id, translation_id, text)
    VALUES (NEW.id, NEW.translation_id, NEW.text);
END;

CREATE TRIGGER IF NOT EXISTS trg_bible_verse_after_delete
AFTER DELETE ON bible_verse BEGIN
    DELETE FROM bible_verse_search WHERE verse_id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_bible_verse_after_update
AFTER UPDATE ON bible_verse BEGIN
    UPDATE bible_verse_search SET text = NEW.text WHERE verse_id = NEW.id;
END;
