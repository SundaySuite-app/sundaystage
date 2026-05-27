//! Song repository — CRUD + cross-cutting queries.
//!
//! The hardest query lives here: `search_by_lyric` uses the FTS5 virtual
//! table set up in migration 0001 to do fast full-text search across
//! every song section in a library.

use sqlx::SqlitePool;

use crate::db::models::{SearchResult, Song, SongInput, SongSection};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

pub struct SongRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> SongRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, input: SongInput) -> AppResult<Song> {
        let id = new_id();
        let now = now_ms();
        let language = input.language.unwrap_or_else(|| "no".to_string());

        sqlx::query(
            r#"
            INSERT INTO song (id, library_id, title, ccli_song_id, tono_work_id,
                              copyright_notice, default_key, tempo_bpm, language,
                              last_used_at, created_at, updated_at, deleted_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?10, NULL)
            "#,
        )
        .bind(&id)
        .bind(&input.library_id)
        .bind(&input.title)
        .bind(&input.ccli_song_id)
        .bind(&input.tono_work_id)
        .bind(&input.copyright_notice)
        .bind(&input.default_key)
        .bind(input.tempo_bpm)
        .bind(&language)
        .bind(now)
        .execute(self.pool)
        .await?;

        self.get(&id).await
    }

    pub async fn get(&self, id: &str) -> AppResult<Song> {
        sqlx::query_as::<_, Song>(
            "SELECT * FROM song WHERE id = ?1 AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound {
            entity: "song",
            id: id.to_string(),
        })
    }

    /// List songs in a library, most-recently-used first then alphabetical.
    pub async fn list(&self, library_id: &str, limit: i64, offset: i64) -> AppResult<Vec<Song>> {
        let rows = sqlx::query_as::<_, Song>(
            r#"
            SELECT * FROM song
            WHERE library_id = ?1 AND deleted_at IS NULL
            ORDER BY last_used_at DESC NULLS LAST, title ASC
            LIMIT ?2 OFFSET ?3
            "#,
        )
        .bind(library_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Soft delete — marks `deleted_at = now`, preserves history.
    pub async fn soft_delete(&self, id: &str) -> AppResult<()> {
        let now = now_ms();
        let result = sqlx::query(
            "UPDATE song SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
        )
        .bind(now)
        .bind(id)
        .execute(self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                entity: "song",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    /// Restore a soft-deleted song.
    pub async fn restore(&self, id: &str) -> AppResult<Song> {
        let now = now_ms();
        sqlx::query(
            "UPDATE song SET deleted_at = NULL, updated_at = ?1 WHERE id = ?2",
        )
        .bind(now)
        .bind(id)
        .execute(self.pool)
        .await?;
        self.get(id).await
    }

    /// FTS5-backed search across title + every section's lyrics.
    ///
    /// The query string supports SQLite FTS5 syntax (phrase quoting,
    /// prefix `*`, NEAR operators). For end-user search we recommend
    /// preprocessing: quote phrases, strip stop words for very short
    /// queries.
    pub async fn search(
        &self,
        library_id: &str,
        query: &str,
        limit: i64,
    ) -> AppResult<Vec<SearchResult>> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        let rows = sqlx::query_as::<_, SearchResult>(
            r#"
            SELECT
                s.id AS song_id,
                s.title AS title,
                snippet(song_search, 2, '<mark>', '</mark>', '…', 12) AS snippet,
                bm25(song_search) AS rank
            FROM song_search
            JOIN song s ON s.id = song_search.song_id
            WHERE song_search MATCH ?1
              AND s.library_id = ?2
              AND s.deleted_at IS NULL
            ORDER BY rank
            LIMIT ?3
            "#,
        )
        .bind(query)
        .bind(library_id)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Touch `last_used_at` — called when a service that includes this
    /// song is marked as `played`. Used by SundayPlan's rotation-fairness
    /// scoring and by the editor's "songs used this month" filter.
    pub async fn mark_used(&self, id: &str, when: i64) -> AppResult<()> {
        sqlx::query(
            "UPDATE song SET last_used_at = ?1, updated_at = ?1 WHERE id = ?2",
        )
        .bind(when)
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    // ── Section helpers ────────────────────────────────────────────────────

    pub async fn add_section(
        &self,
        song_id: &str,
        label: &str,
        lyrics: &str,
    ) -> AppResult<SongSection> {
        let id = new_id();
        let now = now_ms();
        // Determine display_order = max + 1
        let next_order: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(display_order), -1) + 1 FROM song_section WHERE song_id = ?1",
        )
        .bind(song_id)
        .fetch_one(self.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO song_section (id, song_id, label, lyrics, chord_chart, display_order, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?6)
            "#,
        )
        .bind(&id)
        .bind(song_id)
        .bind(label)
        .bind(lyrics)
        .bind(next_order)
        .bind(now)
        .execute(self.pool)
        .await?;

        Ok(SongSection {
            id,
            song_id: song_id.to_string(),
            label: label.to_string(),
            lyrics: lyrics.to_string(),
            chord_chart: None,
            display_order: next_order,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn sections(&self, song_id: &str) -> AppResult<Vec<SongSection>> {
        let rows = sqlx::query_as::<_, SongSection>(
            "SELECT * FROM song_section WHERE song_id = ?1 ORDER BY display_order",
        )
        .bind(song_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::LibraryRepo;
    use crate::db::Database;
    use crate::db::models::LibraryInput;

    async fn fixture() -> (Database, String) {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "Test".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        (db, lib.id)
    }

    #[tokio::test]
    async fn create_and_get_song() {
        let (db, library_id) = fixture().await;
        let repo = SongRepo::new(&db.pool);
        let song = repo
            .create(SongInput {
                library_id: library_id.clone(),
                title: "Amazing Grace".into(),
                language: Some("en".into()),
                default_key: Some("G".into()),
                tempo_bpm: Some(72),
                ccli_song_id: Some("22025".into()),
                tono_work_id: None,
                copyright_notice: Some("Public Domain".into()),
            })
            .await
            .unwrap();
        assert_eq!(song.title, "Amazing Grace");
        assert_eq!(song.default_key.as_deref(), Some("G"));
        let fetched = repo.get(&song.id).await.unwrap();
        assert_eq!(fetched.id, song.id);
    }

    #[tokio::test]
    async fn list_orders_by_last_used_then_title() {
        let (db, library_id) = fixture().await;
        let repo = SongRepo::new(&db.pool);
        let a = repo
            .create(SongInput {
                library_id: library_id.clone(),
                title: "Beta".into(),
                language: None, default_key: None, tempo_bpm: None,
                ccli_song_id: None, tono_work_id: None, copyright_notice: None,
            })
            .await.unwrap();
        let _b = repo
            .create(SongInput {
                library_id: library_id.clone(),
                title: "Alpha".into(),
                language: None, default_key: None, tempo_bpm: None,
                ccli_song_id: None, tono_work_id: None, copyright_notice: None,
            })
            .await.unwrap();
        // Mark Beta as recently used → should appear before Alpha
        repo.mark_used(&a.id, now_ms()).await.unwrap();
        let list = repo.list(&library_id, 10, 0).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].title, "Beta");
        assert_eq!(list[1].title, "Alpha");
    }

    #[tokio::test]
    async fn soft_delete_hides_from_get_and_list() {
        let (db, library_id) = fixture().await;
        let repo = SongRepo::new(&db.pool);
        let song = repo
            .create(SongInput {
                library_id: library_id.clone(),
                title: "Goodbye".into(),
                language: None, default_key: None, tempo_bpm: None,
                ccli_song_id: None, tono_work_id: None, copyright_notice: None,
            })
            .await.unwrap();
        repo.soft_delete(&song.id).await.unwrap();
        assert_eq!(repo.get(&song.id).await.unwrap_err().code(), "not_found");
        let list = repo.list(&library_id, 10, 0).await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn search_finds_song_by_lyric_phrase() {
        let (db, library_id) = fixture().await;
        let repo = SongRepo::new(&db.pool);
        let song = repo
            .create(SongInput {
                library_id: library_id.clone(),
                title: "Amazing Grace".into(),
                language: Some("en".into()),
                default_key: None, tempo_bpm: None,
                ccli_song_id: None, tono_work_id: None, copyright_notice: None,
            })
            .await.unwrap();
        repo.add_section(&song.id, "verse_1",
            "Amazing grace how sweet the sound\nThat saved a wretch like me").await.unwrap();
        repo.add_section(&song.id, "verse_2",
            "I once was lost but now am found\nWas blind but now I see").await.unwrap();

        let results = repo.search(&library_id, "wretch", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].song_id, song.id);
        assert!(results[0].snippet.contains("<mark>wretch</mark>"));
    }

    #[tokio::test]
    async fn search_returns_empty_for_blank_query() {
        let (db, library_id) = fixture().await;
        let repo = SongRepo::new(&db.pool);
        let results = repo.search(&library_id, "   ", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn sections_returned_in_display_order() {
        let (db, library_id) = fixture().await;
        let repo = SongRepo::new(&db.pool);
        let song = repo
            .create(SongInput {
                library_id, title: "Test".into(),
                language: None, default_key: None, tempo_bpm: None,
                ccli_song_id: None, tono_work_id: None, copyright_notice: None,
            })
            .await.unwrap();
        repo.add_section(&song.id, "verse_1", "first").await.unwrap();
        repo.add_section(&song.id, "chorus", "second").await.unwrap();
        repo.add_section(&song.id, "verse_2", "third").await.unwrap();
        let sections = repo.sections(&song.id).await.unwrap();
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].label, "verse_1");
        assert_eq!(sections[2].label, "verse_2");
    }
}
