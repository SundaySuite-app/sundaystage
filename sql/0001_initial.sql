-- SundayStage — initial schema
-- Refer to docs/ARCHITECTURE.md for the ERD and entity descriptions.

PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

-- ── Library ─────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS library (
    id                TEXT PRIMARY KEY,
    name              TEXT    NOT NULL,
    default_locale    TEXT    NOT NULL DEFAULT 'no',
    default_theme_id  TEXT,
    default_template_id TEXT,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);

-- ── Person ──────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS person (
    id            TEXT PRIMARY KEY,
    library_id    TEXT NOT NULL REFERENCES library(id) ON DELETE CASCADE,
    display_name  TEXT NOT NULL,
    sort_name     TEXT,
    external_ids  TEXT,  -- JSON
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);
CREATE INDEX idx_person_library      ON person(library_id);
CREATE INDEX idx_person_sort_name    ON person(library_id, sort_name);

-- ── Tag ─────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS tag (
    id          TEXT PRIMARY KEY,
    library_id  TEXT NOT NULL REFERENCES library(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    color       TEXT,
    UNIQUE (library_id, name)
);
CREATE INDEX idx_tag_library ON tag(library_id);

-- ── Song ────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS song (
    id                TEXT PRIMARY KEY,
    library_id        TEXT NOT NULL REFERENCES library(id) ON DELETE CASCADE,
    title             TEXT NOT NULL,
    ccli_song_id      TEXT,
    tono_work_id      TEXT,
    copyright_notice  TEXT,
    default_key       TEXT,
    tempo_bpm         INTEGER,
    language          TEXT NOT NULL DEFAULT 'no',
    last_used_at      INTEGER,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    deleted_at        INTEGER
);
CREATE INDEX idx_song_library            ON song(library_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_song_library_lastused   ON song(library_id, last_used_at) WHERE deleted_at IS NULL;
CREATE INDEX idx_song_ccli               ON song(ccli_song_id) WHERE ccli_song_id IS NOT NULL;
CREATE INDEX idx_song_tono               ON song(tono_work_id) WHERE tono_work_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS song_author (
    song_id    TEXT NOT NULL REFERENCES song(id)   ON DELETE CASCADE,
    person_id  TEXT NOT NULL REFERENCES person(id) ON DELETE CASCADE,
    role       TEXT NOT NULL,  -- 'composer' | 'lyricist' | 'translator'
    PRIMARY KEY (song_id, person_id, role)
);

CREATE TABLE IF NOT EXISTS song_tag (
    song_id  TEXT NOT NULL REFERENCES song(id) ON DELETE CASCADE,
    tag_id   TEXT NOT NULL REFERENCES tag(id)  ON DELETE CASCADE,
    PRIMARY KEY (song_id, tag_id)
);

-- ── SongSection ─────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS song_section (
    id             TEXT PRIMARY KEY,
    song_id        TEXT NOT NULL REFERENCES song(id) ON DELETE CASCADE,
    label          TEXT NOT NULL,
    lyrics         TEXT NOT NULL,
    chord_chart    TEXT,
    display_order  INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);
CREATE INDEX idx_section_song ON song_section(song_id, display_order);

-- ── FTS5 — full-text search across song_section.lyrics + song.title ────────
CREATE VIRTUAL TABLE IF NOT EXISTS song_search USING fts5 (
    song_id   UNINDEXED,
    title,
    lyrics,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- Keep search index in sync with songs + sections.
CREATE TRIGGER IF NOT EXISTS trg_song_after_insert AFTER INSERT ON song BEGIN
    INSERT INTO song_search(song_id, title, lyrics)
    VALUES (NEW.id, NEW.title, '');
END;

CREATE TRIGGER IF NOT EXISTS trg_song_after_update AFTER UPDATE OF title ON song BEGIN
    UPDATE song_search SET title = NEW.title WHERE song_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_song_after_delete AFTER DELETE ON song BEGIN
    DELETE FROM song_search WHERE song_id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_section_after_insert AFTER INSERT ON song_section BEGIN
    UPDATE song_search
       SET lyrics = (
         SELECT GROUP_CONCAT(lyrics, char(10))
           FROM song_section
          WHERE song_id = NEW.song_id
       )
     WHERE song_id = NEW.song_id;
END;

CREATE TRIGGER IF NOT EXISTS trg_section_after_update AFTER UPDATE OF lyrics ON song_section BEGIN
    UPDATE song_search
       SET lyrics = (
         SELECT GROUP_CONCAT(lyrics, char(10))
           FROM song_section
          WHERE song_id = NEW.song_id
       )
     WHERE song_id = NEW.song_id;
END;

CREATE TRIGGER IF NOT EXISTS trg_section_after_delete AFTER DELETE ON song_section BEGIN
    UPDATE song_search
       SET lyrics = (
         SELECT COALESCE(GROUP_CONCAT(lyrics, char(10)), '')
           FROM song_section
          WHERE song_id = OLD.song_id
       )
     WHERE song_id = OLD.song_id;
END;

-- ── SongArrangement ─────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS song_arrangement (
    id          TEXT PRIMARY KEY,
    song_id     TEXT NOT NULL REFERENCES song(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    is_default  INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0,1)),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX idx_arrangement_song ON song_arrangement(song_id);

-- At most one default arrangement per song
CREATE UNIQUE INDEX uniq_arrangement_default
    ON song_arrangement(song_id) WHERE is_default = 1;

CREATE TABLE IF NOT EXISTS arrangement_item (
    arrangement_id  TEXT NOT NULL REFERENCES song_arrangement(id) ON DELETE CASCADE,
    position        INTEGER NOT NULL,
    section_id      TEXT NOT NULL REFERENCES song_section(id) ON DELETE RESTRICT,
    PRIMARY KEY (arrangement_id, position)
);

-- ── Theme ───────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS theme (
    id          TEXT PRIMARY KEY,
    library_id  TEXT REFERENCES library(id) ON DELETE CASCADE, -- nullable for built-in
    name        TEXT NOT NULL,
    tokens      TEXT NOT NULL,  -- JSON
    is_builtin  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX idx_theme_library ON theme(library_id);

-- ── Template ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS template (
    id          TEXT PRIMARY KEY,
    library_id  TEXT REFERENCES library(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    slots       TEXT NOT NULL,  -- JSON
    is_builtin  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX idx_template_library ON template(library_id);

-- ── BibleReference (cached verses) ──────────────────────────────────────────
CREATE TABLE IF NOT EXISTS bible_reference (
    id           TEXT PRIMARY KEY,
    book         TEXT NOT NULL,
    chapter      INTEGER NOT NULL,
    verse_start  INTEGER NOT NULL,
    verse_end    INTEGER,
    translation  TEXT NOT NULL,
    text         TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_bibleref_passage ON bible_reference(translation, book, chapter, verse_start);

-- ── Service ─────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS service (
    id          TEXT PRIMARY KEY,
    library_id  TEXT NOT NULL REFERENCES library(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    starts_at   INTEGER NOT NULL,
    notes       TEXT,
    state       TEXT NOT NULL DEFAULT 'planned',  -- 'planned' | 'in_progress' | 'played' | 'archived'
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);
CREATE INDEX idx_service_library_date ON service(library_id, starts_at) WHERE deleted_at IS NULL;

-- ── CustomDeck ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS custom_deck (
    id          TEXT PRIMARY KEY,
    library_id  TEXT NOT NULL REFERENCES library(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX idx_deck_library ON custom_deck(library_id);

-- ── Slide ───────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS slide (
    id              TEXT PRIMARY KEY,
    custom_deck_id  TEXT REFERENCES custom_deck(id) ON DELETE CASCADE,
    position        INTEGER NOT NULL DEFAULT 0,
    content         TEXT NOT NULL,  -- JSON
    theme_id        TEXT REFERENCES theme(id)    ON DELETE SET NULL,
    template_id     TEXT REFERENCES template(id) ON DELETE SET NULL,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
CREATE INDEX idx_slide_deck ON slide(custom_deck_id, position);

-- ── MediaAsset ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS media_asset (
    id              TEXT PRIMARY KEY,
    library_id      TEXT NOT NULL REFERENCES library(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('image','video','audio')),
    original_path   TEXT NOT NULL,
    content_hash    TEXT NOT NULL,
    thumbnail_path  TEXT,
    width           INTEGER,
    height          INTEGER,
    duration_ms     INTEGER,
    tags            TEXT,  -- JSON array
    imported_at     INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
CREATE INDEX idx_media_library ON media_asset(library_id);
CREATE INDEX idx_media_hash    ON media_asset(content_hash);

-- ── ServiceItem ─────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS service_item (
    id                  TEXT PRIMARY KEY,
    service_id          TEXT NOT NULL REFERENCES service(id) ON DELETE CASCADE,
    position            INTEGER NOT NULL,
    kind                TEXT NOT NULL CHECK (kind IN ('song','scripture','custom_deck','video','announcement','gap')),
    song_id             TEXT REFERENCES song(id),
    arrangement_id      TEXT REFERENCES song_arrangement(id),
    key_override        TEXT,
    bible_reference_id  TEXT REFERENCES bible_reference(id),
    custom_deck_id      TEXT REFERENCES custom_deck(id),
    media_asset_id      TEXT REFERENCES media_asset(id),
    notes               TEXT,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    UNIQUE (service_id, position)
);
CREATE INDEX idx_serviceitem_service ON service_item(service_id, position);

-- ── SyncMeta (Phase 9 placeholder) ──────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sync_meta (
    entity_type     TEXT NOT NULL,
    entity_id       TEXT NOT NULL,
    server_id       TEXT,
    updated_at      INTEGER NOT NULL,
    last_synced_at  INTEGER,
    device_id       TEXT,
    conflict_state  TEXT NOT NULL DEFAULT 'none',
    PRIMARY KEY (entity_type, entity_id)
);

-- ── Schema version marker ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS schema_migrations (
    version    INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL,
    description TEXT
);

INSERT INTO schema_migrations (version, applied_at, description)
VALUES (1, unixepoch() * 1000, 'Initial schema — Phase 1.1');
