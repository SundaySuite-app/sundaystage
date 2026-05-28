//! Bible repository — the browsable text library (Phase 7.1) + the per-service
//! reference cache (`bible_reference`, read by the live engine at service time
//! so there's no lookup during a Sunday service).

use sqlx::SqlitePool;

use crate::db::models::{BibleReference, BibleTranslation, BibleVerse};
use crate::db::{new_id, now_ms};
use crate::error::AppResult;
use crate::services::bible::bundled_translations;

pub struct BibleRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> BibleRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Install the bundled public-domain translations. Idempotent: translations
    /// are keyed by `code`, verses by their unique (translation, book, ch, vs).
    pub async fn seed(&self) -> AppResult<()> {
        for t in bundled_translations() {
            let existing: Option<(String,)> =
                sqlx::query_as("SELECT id FROM bible_translation WHERE code = ?1")
                    .bind(t.code)
                    .fetch_optional(self.pool)
                    .await?;
            let tid = match existing {
                Some((id,)) => id,
                None => {
                    let id = new_id();
                    sqlx::query(
                        r#"INSERT INTO bible_translation (id, code, name, language, public_domain, created_at)
                           VALUES (?1, ?2, ?3, ?4, 1, ?5)"#,
                    )
                    .bind(&id)
                    .bind(t.code)
                    .bind(t.name)
                    .bind(t.language)
                    .bind(now_ms())
                    .execute(self.pool)
                    .await?;
                    id
                }
            };
            for v in t.verses {
                sqlx::query(
                    r#"INSERT OR IGNORE INTO bible_verse
                       (id, translation_id, book, book_order, chapter, verse, text, created_at)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
                )
                .bind(new_id())
                .bind(&tid)
                .bind(v.book)
                .bind(v.book_order)
                .bind(v.chapter)
                .bind(v.verse)
                .bind(v.text)
                .bind(now_ms())
                .execute(self.pool)
                .await?;
            }
        }
        Ok(())
    }

    pub async fn list_translations(&self) -> AppResult<Vec<BibleTranslation>> {
        Ok(sqlx::query_as::<_, BibleTranslation>(
            "SELECT * FROM bible_translation ORDER BY language, name",
        )
        .fetch_all(self.pool)
        .await?)
    }

    /// Distinct books present in a translation, in canonical order.
    pub async fn books(&self, translation_id: &str) -> AppResult<Vec<(String, i64)>> {
        Ok(sqlx::query_as::<_, (String, i64)>(
            r#"SELECT book, book_order FROM bible_verse
               WHERE translation_id = ?1
               GROUP BY book, book_order
               ORDER BY book_order"#,
        )
        .bind(translation_id)
        .fetch_all(self.pool)
        .await?)
    }

    pub async fn chapters(&self, translation_id: &str, book: &str) -> AppResult<Vec<i64>> {
        Ok(sqlx::query_scalar::<_, i64>(
            r#"SELECT DISTINCT chapter FROM bible_verse
               WHERE translation_id = ?1 AND book = ?2 ORDER BY chapter"#,
        )
        .bind(translation_id)
        .bind(book)
        .fetch_all(self.pool)
        .await?)
    }

    /// Verses for a passage. `verse_start`/`verse_end` `None` ⇒ whole chapter.
    pub async fn passage(
        &self,
        translation_id: &str,
        book: &str,
        chapter: i64,
        verse_start: Option<i64>,
        verse_end: Option<i64>,
    ) -> AppResult<Vec<BibleVerse>> {
        let (lo, hi) = match (verse_start, verse_end) {
            (None, _) => (i64::MIN, i64::MAX),
            (Some(s), None) => (s, s),
            (Some(s), Some(e)) => (s, e),
        };
        Ok(sqlx::query_as::<_, BibleVerse>(
            r#"SELECT * FROM bible_verse
               WHERE translation_id = ?1 AND book = ?2 AND chapter = ?3
                 AND verse BETWEEN ?4 AND ?5
               ORDER BY verse"#,
        )
        .bind(translation_id)
        .bind(book)
        .bind(chapter)
        .bind(lo)
        .bind(hi)
        .fetch_all(self.pool)
        .await?)
    }

    /// Full-text search across verse text, optionally within one translation.
    pub async fn search(
        &self,
        query: &str,
        translation_id: Option<&str>,
        limit: i64,
    ) -> AppResult<Vec<BibleVerse>> {
        let match_query = fts_query(query);
        if match_query.is_empty() {
            return Ok(vec![]);
        }
        let verses = if let Some(tid) = translation_id {
            sqlx::query_as::<_, BibleVerse>(
                r#"SELECT v.* FROM bible_verse_search s
                   JOIN bible_verse v ON v.id = s.verse_id
                   WHERE bible_verse_search MATCH ?1 AND s.translation_id = ?2
                   ORDER BY v.book_order, v.chapter, v.verse
                   LIMIT ?3"#,
            )
            .bind(&match_query)
            .bind(tid)
            .bind(limit)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query_as::<_, BibleVerse>(
                r#"SELECT v.* FROM bible_verse_search s
                   JOIN bible_verse v ON v.id = s.verse_id
                   WHERE bible_verse_search MATCH ?1
                   ORDER BY v.book_order, v.chapter, v.verse
                   LIMIT ?2"#,
            )
            .bind(&match_query)
            .bind(limit)
            .fetch_all(self.pool)
            .await?
        };
        Ok(verses)
    }

    /// Cache a chosen passage's text for a scripture service item (read by the
    /// cue compiler at service time).
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

/// Turn raw user input into a safe FTS5 MATCH expression: each whitespace token
/// becomes a quoted term (quotes escaped), AND-ed together. Avoids FTS syntax
/// errors from punctuation in the query.
fn fts_query(raw: &str) -> String {
    raw.split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    async fn db() -> Database {
        let db = Database::open_in_memory().await.unwrap();
        BibleRepo::new(&db.pool).seed().await.unwrap();
        db
    }

    #[tokio::test]
    async fn seed_is_idempotent() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = BibleRepo::new(&db.pool);
        repo.seed().await.unwrap();
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bible_verse")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        repo.seed().await.unwrap(); // again
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bible_verse")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(before, after);
        assert!(before > 0);
    }

    #[tokio::test]
    async fn lists_translations_and_books() {
        let db = db().await;
        let repo = BibleRepo::new(&db.pool);
        let ts = repo.list_translations().await.unwrap();
        assert!(ts.iter().any(|t| t.code == "KJV"));
        let kjv = ts.iter().find(|t| t.code == "KJV").unwrap();
        let books = repo.books(&kjv.id).await.unwrap();
        // Books come back in canonical order (Psalms 19 before John 43).
        assert!(books.first().map(|b| b.1).unwrap_or(0) <= books.last().map(|b| b.1).unwrap_or(0));
        assert!(books.iter().any(|b| b.0 == "John"));
    }

    #[tokio::test]
    async fn passage_and_whole_chapter() {
        let db = db().await;
        let repo = BibleRepo::new(&db.pool);
        let kjv = repo
            .list_translations()
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.code == "KJV")
            .unwrap();
        let single = repo
            .passage(&kjv.id, "John", 3, Some(16), None)
            .await
            .unwrap();
        assert_eq!(single.len(), 1);
        assert!(single[0].text.contains("God so loved"));
        let chapter = repo
            .passage(&kjv.id, "Psalms", 23, None, None)
            .await
            .unwrap();
        assert_eq!(chapter.len(), 6);
    }

    #[tokio::test]
    async fn search_finds_phrase() {
        let db = db().await;
        let repo = BibleRepo::new(&db.pool);
        let hits = repo.search("shepherd", None, 20).await.unwrap();
        assert!(hits.iter().any(|v| v.book == "Psalms" && v.chapter == 23));
        // Punctuation must not blow up the MATCH query.
        assert!(repo.search("shepherd; want!", None, 20).await.is_ok());
    }
}
