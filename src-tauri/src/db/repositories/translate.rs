//! Translation cache repository (Phase 11.2).
//!
//! The offline store behind the live translation overlay. Each row is one
//! source line → one translated line, keyed by `(source_text, target_language)`.
//! Populated at cue-COMPILE time when an Anthropic key is present, and read back
//! forever after — so a Sunday with no network still renders every line that was
//! ever translated. Bundled Bible passages don't need rows here; they're served
//! straight from `services::bible::bundled_translations()`.

use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::db::now_ms;
use crate::error::AppResult;

pub struct TranslateRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> TranslateRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Look up every cached translation for `target` among the given source
    /// lines, in ONE query. Returns a map source_line → translated_line; a
    /// missing key means "not cached" (the compiler then tries bundled text,
    /// then the network). De-duplicates the input so repeated lines cost one
    /// lookup.
    pub async fn get_cached(
        &self,
        sources: &[String],
        target: &str,
    ) -> AppResult<HashMap<String, String>> {
        let mut out = HashMap::new();
        if sources.is_empty() {
            return Ok(out);
        }
        // Distinct, non-empty placeholders. We bind each source separately
        // (SQLite has no array binding); the list is bounded by one service's
        // slide lines, so this is small.
        let mut seen: Vec<&String> = Vec::new();
        for s in sources {
            if !seen.contains(&s) {
                seen.push(s);
            }
        }
        let placeholders = std::iter::repeat_n("?", seen.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT source_text, translated_text FROM translation_cache \
             WHERE target_language = ? AND source_text IN ({placeholders})"
        );
        let mut q = sqlx::query_as::<_, (String, String)>(&sql).bind(target);
        for s in &seen {
            q = q.bind(*s);
        }
        for (src, tr) in q.fetch_all(self.pool).await? {
            out.insert(src, tr);
        }
        Ok(out)
    }

    /// Upsert one translated line. Idempotent on `(source_text, target_language)`.
    pub async fn put_cached(
        &self,
        source_text: &str,
        target: &str,
        translated_text: &str,
        source: &str,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO translation_cache \
               (source_text, target_language, translated_text, source, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(source_text, target_language) \
             DO UPDATE SET translated_text = excluded.translated_text, source = excluded.source",
        )
        .bind(source_text)
        .bind(target)
        .bind(translated_text)
        .bind(source)
        .bind(now_ms())
        .execute(self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[tokio::test]
    async fn put_then_get_roundtrips() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = TranslateRepo::new(&db.pool);
        repo.put_cached("Amazing grace", "no", "Underfull nåde", "ai")
            .await
            .unwrap();
        let got = repo
            .get_cached(&["Amazing grace".to_string(), "missing".to_string()], "no")
            .await
            .unwrap();
        assert_eq!(got.get("Amazing grace").map(String::as_str), Some("Underfull nåde"));
        assert!(!got.contains_key("missing"));
        // Wrong target language is a miss.
        let other = repo
            .get_cached(&["Amazing grace".to_string()], "de")
            .await
            .unwrap();
        assert!(other.is_empty());
    }

    #[tokio::test]
    async fn put_is_idempotent_and_updates() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = TranslateRepo::new(&db.pool);
        repo.put_cached("Holy", "no", "Hellig (old)", "ai")
            .await
            .unwrap();
        repo.put_cached("Holy", "no", "Hellig", "ai").await.unwrap();
        let got = repo.get_cached(&["Holy".to_string()], "no").await.unwrap();
        assert_eq!(got.get("Holy").map(String::as_str), Some("Hellig"));
        // Only one row.
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM translation_cache")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn empty_input_is_empty_map_no_query() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = TranslateRepo::new(&db.pool);
        assert!(repo.get_cached(&[], "no").await.unwrap().is_empty());
    }
}
