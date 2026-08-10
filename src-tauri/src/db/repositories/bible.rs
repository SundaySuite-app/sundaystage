//! Bible repository — the browsable text library (Phase 7.1) + the per-service
//! reference cache (`bible_reference`, read by the live engine at service time
//! so there's no lookup during a Sunday service).

use sqlx::SqlitePool;

use crate::db::models::{BibleReference, BibleTranslation, BibleVerse};
use crate::db::{new_id, now_ms};
use crate::error::AppResult;
use crate::services::bible::bundled_translations;
use crate::services::bible_download::ParsedVerse;

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

    /// How many verses of a translation (by `code`) are currently installed.
    /// 0 = absent, a handful = the bundled starter set, ~31k = a full corpus.
    pub async fn verse_count_by_code(&self, code: &str) -> AppResult<i64> {
        Ok(sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM bible_verse v
               JOIN bible_translation t ON t.id = v.translation_id
               WHERE t.code = ?1"#,
        )
        .bind(code)
        .fetch_one(self.pool)
        .await?)
    }

    /// Install a full downloaded corpus, replacing any existing text for the same
    /// `code`. Used by the C1/C2 downloader; the whole thing is one transaction,
    /// so a failed re-download can never leave a half-populated translation.
    ///
    /// The translation row's **id is preserved** across a re-download (only its
    /// name/language are refreshed), so a `bible_reference` cached against it
    /// keeps pointing at live text.
    ///
    /// FTS consistency does NOT lean on `ON DELETE CASCADE`: whether a cascade
    /// delete fires the `AFTER DELETE` trigger that prunes `bible_verse_search`
    /// depends on the `recursive_triggers` pragma (off by default). So the old
    /// verses are deleted with an **explicit** `DELETE`, which always fires the
    /// trigger — no stale rows survive in the search index after a re-download.
    pub async fn replace_translation(
        &self,
        code: &str,
        name: &str,
        language: &str,
        verses: &[ParsedVerse],
    ) -> AppResult<BibleTranslation> {
        let now = now_ms();
        let mut tx = self.pool.begin().await?;

        let existing: Option<(String, i64)> =
            sqlx::query_as("SELECT id, created_at FROM bible_translation WHERE code = ?1")
                .bind(code)
                .fetch_optional(&mut *tx)
                .await?;

        let (tid, created_at) = match existing {
            Some((id, created)) => {
                sqlx::query(
                    r#"UPDATE bible_translation
                       SET name = ?2, language = ?3, public_domain = 1
                       WHERE id = ?1"#,
                )
                .bind(&id)
                .bind(name)
                .bind(language)
                .execute(&mut *tx)
                .await?;
                // Explicit delete → fires trg_bible_verse_after_delete → FTS pruned.
                sqlx::query("DELETE FROM bible_verse WHERE translation_id = ?1")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                (id, created)
            }
            None => {
                let id = new_id();
                sqlx::query(
                    r#"INSERT INTO bible_translation
                       (id, code, name, language, public_domain, created_at)
                       VALUES (?1, ?2, ?3, ?4, 1, ?5)"#,
                )
                .bind(&id)
                .bind(code)
                .bind(name)
                .bind(language)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                (id, now)
            }
        };

        for v in verses {
            sqlx::query(
                r#"INSERT INTO bible_verse
                   (id, translation_id, book, book_order, chapter, verse, text, created_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
            )
            .bind(new_id())
            .bind(&tid)
            .bind(v.book)
            .bind(v.book_order)
            .bind(v.chapter)
            .bind(v.verse)
            .bind(&v.text)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(BibleTranslation {
            id: tid,
            code: code.into(),
            name: name.into(),
            language: language.into(),
            public_domain: 1,
            created_at,
        })
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

    // ── Full-corpus install (C1) ─────────────────────────────────────────────

    fn verse(book: &'static str, order: i64, ch: i64, vs: i64, text: &str) -> ParsedVerse {
        ParsedVerse {
            book,
            book_order: order,
            chapter: ch,
            verse: vs,
            text: text.into(),
        }
    }

    #[tokio::test]
    async fn replace_translation_installs_and_is_searchable() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = BibleRepo::new(&db.pool);
        let t = repo
            .replace_translation(
                "ASV",
                "American Standard Version",
                "en",
                &[
                    verse("John", 43, 3, 16, "For God so loved the world"),
                    verse(
                        "Psalms",
                        19,
                        23,
                        1,
                        "Jehovah is my shepherd; I shall not want.",
                    ),
                ],
            )
            .await
            .unwrap();
        assert_eq!(repo.verse_count_by_code("ASV").await.unwrap(), 2);
        // The FTS trigger fired on insert, so the new text is searchable.
        let hits = repo.search("shepherd", Some(&t.id), 20).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].book, "Psalms");
    }

    #[tokio::test]
    async fn re_download_replaces_cleanly_and_keeps_fts_consistent() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = BibleRepo::new(&db.pool);

        let first = repo
            .replace_translation(
                "ASV",
                "American Standard Version",
                "en",
                &[verse("John", 43, 3, 16, "a distinctive_oldtoken verse")],
            )
            .await
            .unwrap();

        // Re-install the SAME code with different text (a re-download).
        let second = repo
            .replace_translation(
                "ASV",
                "American Standard Version",
                "en",
                &[
                    verse("John", 43, 3, 16, "a distinctive_newtoken verse"),
                    verse("John", 43, 3, 17, "another verse"),
                ],
            )
            .await
            .unwrap();

        // Idempotent identity: the translation id is preserved across re-download.
        assert_eq!(first.id, second.id);
        // No duplicate rows — the old verses were replaced, not appended.
        assert_eq!(repo.verse_count_by_code("ASV").await.unwrap(), 2);
        // FTS is consistent: the stale token is gone, the fresh one is found.
        assert!(repo
            .search("distinctive_oldtoken", None, 20)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            repo.search("distinctive_newtoken", None, 20)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn download_upgrades_the_bundled_starter_set_in_place() {
        // Seed the curated starter set, then "download" the full KJV over it.
        let db = db().await;
        let repo = BibleRepo::new(&db.pool);
        let starter = repo.verse_count_by_code("KJV").await.unwrap();
        assert!(starter > 0 && starter < 100, "starter set is small");
        let bundled_kjv_id = repo
            .list_translations()
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.code == "KJV")
            .unwrap()
            .id;

        let full = repo
            .replace_translation(
                "KJV",
                "King James Version",
                "en",
                &[
                    verse("Genesis", 1, 1, 1, "In the beginning"),
                    verse("Genesis", 1, 1, 2, "And the earth was without form"),
                    verse("Revelation", 66, 22, 21, "Amen."),
                ],
            )
            .await
            .unwrap();

        // Same translation row (id preserved), now carrying the full text.
        assert_eq!(full.id, bundled_kjv_id);
        assert_eq!(repo.verse_count_by_code("KJV").await.unwrap(), 3);
        // No second KJV row was created.
        let kjv_rows = repo
            .list_translations()
            .await
            .unwrap()
            .into_iter()
            .filter(|t| t.code == "KJV")
            .count();
        assert_eq!(kjv_rows, 1);
    }
}
