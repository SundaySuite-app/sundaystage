//! Bible repository — cached scripture passages.
//!
//! Phase 7 of the build plan covers translation downloads from external
//! sources; this repo is the per-library cache that the live engine reads
//! from at service time (no network during a Sunday service).

use sqlx::SqlitePool;

use crate::db::models::BibleReference;
use crate::db::{new_id, now_ms};
use crate::error::AppResult;

pub struct BibleRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> BibleRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn cache(
        &self,
        book: &str,
        chapter: i64,
        verse_start: i64,
        verse_end: Option<i64>,
        translation: &str,
        text: &str,
    ) -> AppResult<BibleReference> {
        let id = new_id();
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO bible_reference (id, book, chapter, verse_start, verse_end, translation, text, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(&id)
        .bind(book)
        .bind(chapter)
        .bind(verse_start)
        .bind(verse_end)
        .bind(translation)
        .bind(text)
        .bind(now)
        .execute(self.pool)
        .await?;
        Ok(BibleReference {
            id,
            book: book.into(),
            chapter,
            verse_start,
            verse_end,
            translation: translation.into(),
            text: text.into(),
            created_at: now,
        })
    }
}
