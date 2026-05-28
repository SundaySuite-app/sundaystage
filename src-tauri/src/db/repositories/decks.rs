//! Deck repository — custom decks and their slides (Phase 3.1).
//!
//! A `CustomDeck` is an ad-hoc slide deck (announcements, sermon points, a
//! welcome slide) not tied to a song or scripture. Its slides are stored
//! explicitly (unlike song/scripture slides, which are generated at compile
//! time), so the slide editor reads and writes through here.
//!
//! Slide `content` is the JSON serialization of [`SlideDoc`]; this repo owns
//! that (de)serialization so callers work with the typed model, never raw
//! JSON. Positions are 0-based and kept contiguous on insert/delete/reorder.

use sqlx::SqlitePool;

use crate::db::models::{CustomDeck, Slide};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};
use crate::services::slide_doc::SlideDoc;

pub struct DeckRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> DeckRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    // ── Deck ────────────────────────────────────────────────────────────────

    pub async fn create_deck(&self, library_id: &str, name: &str) -> AppResult<CustomDeck> {
        let id = new_id();
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO custom_deck (id, library_id, name, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?4)
            "#,
        )
        .bind(&id)
        .bind(library_id)
        .bind(name)
        .bind(now)
        .execute(self.pool)
        .await?;
        self.get_deck(&id).await
    }

    pub async fn get_deck(&self, id: &str) -> AppResult<CustomDeck> {
        sqlx::query_as::<_, CustomDeck>("SELECT * FROM custom_deck WHERE id = ?1")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound {
                entity: "custom_deck",
                id: id.to_string(),
            })
    }

    /// List a library's decks, most-recently-edited first.
    pub async fn list_decks(&self, library_id: &str) -> AppResult<Vec<CustomDeck>> {
        let rows = sqlx::query_as::<_, CustomDeck>(
            r#"
            SELECT * FROM custom_deck
            WHERE library_id = ?1
            ORDER BY updated_at DESC, name ASC
            "#,
        )
        .bind(library_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn rename_deck(&self, id: &str, name: &str) -> AppResult<CustomDeck> {
        let now = now_ms();
        let result = sqlx::query("UPDATE custom_deck SET name = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(name)
            .bind(now)
            .bind(id)
            .execute(self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                entity: "custom_deck",
                id: id.to_string(),
            });
        }
        self.get_deck(id).await
    }

    /// Hard delete — `slide` rows cascade via the FK (`ON DELETE CASCADE`).
    pub async fn delete_deck(&self, id: &str) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM custom_deck WHERE id = ?1")
            .bind(id)
            .execute(self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                entity: "custom_deck",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    // ── Slide ─────────────────────────────────────────────────────────────────

    /// Append a slide to the end of a deck.
    pub async fn create_slide(&self, deck_id: &str, doc: &SlideDoc) -> AppResult<Slide> {
        let id = new_id();
        let now = now_ms();
        let content = doc.to_json()?;
        let next_pos: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM slide WHERE custom_deck_id = ?1",
        )
        .bind(deck_id)
        .fetch_one(self.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO slide (id, custom_deck_id, position, content, theme_id, template_id, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5, ?5)
            "#,
        )
        .bind(&id)
        .bind(deck_id)
        .bind(next_pos)
        .bind(&content)
        .bind(now)
        .execute(self.pool)
        .await?;
        // Keep the deck's updated_at fresh so list ordering reflects edits.
        self.touch_deck(deck_id, now).await?;
        self.get_slide(&id).await
    }

    pub async fn get_slide(&self, id: &str) -> AppResult<Slide> {
        sqlx::query_as::<_, Slide>("SELECT * FROM slide WHERE id = ?1")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound {
                entity: "slide",
                id: id.to_string(),
            })
    }

    pub async fn list_slides(&self, deck_id: &str) -> AppResult<Vec<Slide>> {
        let rows = sqlx::query_as::<_, Slide>(
            "SELECT * FROM slide WHERE custom_deck_id = ?1 ORDER BY position",
        )
        .bind(deck_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Overwrite a slide's design. The editor calls this (debounced) on edit.
    pub async fn update_slide_content(&self, id: &str, doc: &SlideDoc) -> AppResult<Slide> {
        let now = now_ms();
        let content = doc.to_json()?;
        let result = sqlx::query("UPDATE slide SET content = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(&content)
            .bind(now)
            .bind(id)
            .execute(self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                entity: "slide",
                id: id.to_string(),
            });
        }
        let slide = self.get_slide(id).await?;
        if let Some(deck_id) = &slide.custom_deck_id {
            self.touch_deck(deck_id, now).await?;
        }
        Ok(slide)
    }

    /// Duplicate a slide, inserting the copy immediately after the original
    /// and shifting later slides down by one.
    pub async fn duplicate_slide(&self, id: &str) -> AppResult<Slide> {
        let src = self.get_slide(id).await?;
        let Some(deck_id) = src.custom_deck_id.clone() else {
            return Err(AppError::Validation(format!(
                "slide {} is not attached to a custom deck",
                id
            )));
        };
        let new_id_str = new_id();
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE slide SET position = position + 1, updated_at = ?1 WHERE custom_deck_id = ?2 AND position > ?3",
        )
        .bind(now)
        .bind(&deck_id)
        .bind(src.position)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO slide (id, custom_deck_id, position, content, theme_id, template_id, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            "#,
        )
        .bind(&new_id_str)
        .bind(&deck_id)
        .bind(src.position + 1)
        .bind(&src.content)
        .bind(&src.theme_id)
        .bind(&src.template_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.touch_deck(&deck_id, now).await?;
        self.get_slide(&new_id_str).await
    }

    /// Delete a slide and compact the remaining positions so they stay
    /// contiguous (0,1,2,…).
    pub async fn delete_slide(&self, id: &str) -> AppResult<()> {
        let slide = self.get_slide(id).await?;
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM slide WHERE id = ?1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if let Some(deck_id) = &slide.custom_deck_id {
            sqlx::query(
                "UPDATE slide SET position = position - 1, updated_at = ?1 WHERE custom_deck_id = ?2 AND position > ?3",
            )
            .bind(now)
            .bind(deck_id)
            .bind(slide.position)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        if let Some(deck_id) = &slide.custom_deck_id {
            self.touch_deck(deck_id, now).await?;
        }
        Ok(())
    }

    /// Set (or clear, with `None`) a slide's per-slide theme override.
    pub async fn set_slide_theme(&self, id: &str, theme_id: Option<&str>) -> AppResult<Slide> {
        self.set_slide_field("theme_id", id, theme_id).await
    }

    /// Set (or clear, with `None`) a slide's per-slide template override.
    pub async fn set_slide_template(
        &self,
        id: &str,
        template_id: Option<&str>,
    ) -> AppResult<Slide> {
        self.set_slide_field("template_id", id, template_id).await
    }

    async fn set_slide_field(
        &self,
        column: &'static str,
        id: &str,
        value: Option<&str>,
    ) -> AppResult<Slide> {
        // `column` is a fixed in-code identifier (never user input), so the
        // format! is injection-safe.
        let now = now_ms();
        let sql = format!("UPDATE slide SET {column} = ?1, updated_at = ?2 WHERE id = ?3");
        let res = sqlx::query(&sql)
            .bind(value)
            .bind(now)
            .bind(id)
            .execute(self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound {
                entity: "slide",
                id: id.to_string(),
            });
        }
        self.get_slide(id).await
    }

    /// Reorder a deck's slides to match `ordered_ids` exactly. Ids are assigned
    /// positions by their index in the list. Rejects a list that doesn't match
    /// the deck's current slide set so the UI and DB can't silently diverge.
    pub async fn reorder_slides(
        &self,
        deck_id: &str,
        ordered_ids: &[String],
    ) -> AppResult<Vec<Slide>> {
        let current = self.list_slides(deck_id).await?;
        if current.len() != ordered_ids.len() {
            return Err(AppError::Validation(format!(
                "reorder list has {} ids but deck {} has {} slides",
                ordered_ids.len(),
                deck_id,
                current.len()
            )));
        }
        let mut current_ids: Vec<&str> = current.iter().map(|s| s.id.as_str()).collect();
        current_ids.sort_unstable();
        let mut wanted: Vec<&str> = ordered_ids.iter().map(|s| s.as_str()).collect();
        wanted.sort_unstable();
        if current_ids != wanted {
            return Err(AppError::Validation(format!(
                "reorder list does not match the slides in deck {}",
                deck_id
            )));
        }

        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        for (idx, slide_id) in ordered_ids.iter().enumerate() {
            sqlx::query(
                "UPDATE slide SET position = ?1, updated_at = ?2 WHERE id = ?3 AND custom_deck_id = ?4",
            )
            .bind(idx as i64)
            .bind(now)
            .bind(slide_id)
            .bind(deck_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.touch_deck(deck_id, now).await?;
        self.list_slides(deck_id).await
    }

    async fn touch_deck(&self, deck_id: &str, when: i64) -> AppResult<()> {
        sqlx::query("UPDATE custom_deck SET updated_at = ?1 WHERE id = ?2")
            .bind(when)
            .bind(deck_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::LibraryInput;
    use crate::db::repositories::LibraryRepo;
    use crate::db::Database;
    use crate::services::slide_doc::SlideDoc;

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

    fn doc(text: &str) -> SlideDoc {
        SlideDoc::with_lines("b1", &[text.to_string()])
    }

    #[tokio::test]
    async fn create_and_list_decks() {
        let (db, lib) = fixture().await;
        let repo = DeckRepo::new(&db.pool);
        let deck = repo.create_deck(&lib, "Announcements").await.unwrap();
        assert_eq!(deck.name, "Announcements");
        let decks = repo.list_decks(&lib).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].id, deck.id);
    }

    #[tokio::test]
    async fn rename_and_delete_deck() {
        let (db, lib) = fixture().await;
        let repo = DeckRepo::new(&db.pool);
        let deck = repo.create_deck(&lib, "Old").await.unwrap();
        let renamed = repo.rename_deck(&deck.id, "New").await.unwrap();
        assert_eq!(renamed.name, "New");
        repo.delete_deck(&deck.id).await.unwrap();
        assert_eq!(
            repo.get_deck(&deck.id).await.unwrap_err().code(),
            "not_found"
        );
    }

    #[tokio::test]
    async fn deleting_deck_cascades_to_slides() {
        let (db, lib) = fixture().await;
        let repo = DeckRepo::new(&db.pool);
        let deck = repo.create_deck(&lib, "Deck").await.unwrap();
        repo.create_slide(&deck.id, &doc("a")).await.unwrap();
        repo.delete_deck(&deck.id).await.unwrap();
        // No slides should remain pointing at the gone deck.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM slide WHERE custom_deck_id = ?1")
            .bind(&deck.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn slides_append_with_contiguous_positions() {
        let (db, lib) = fixture().await;
        let repo = DeckRepo::new(&db.pool);
        let deck = repo.create_deck(&lib, "Deck").await.unwrap();
        let a = repo.create_slide(&deck.id, &doc("a")).await.unwrap();
        let b = repo.create_slide(&deck.id, &doc("b")).await.unwrap();
        let c = repo.create_slide(&deck.id, &doc("c")).await.unwrap();
        assert_eq!(a.position, 0);
        assert_eq!(b.position, 1);
        assert_eq!(c.position, 2);
        let slides = repo.list_slides(&deck.id).await.unwrap();
        assert_eq!(
            slides.iter().map(|s| s.position).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[tokio::test]
    async fn update_slide_content_round_trips_through_slide_doc() {
        let (db, lib) = fixture().await;
        let repo = DeckRepo::new(&db.pool);
        let deck = repo.create_deck(&lib, "Deck").await.unwrap();
        let slide = repo.create_slide(&deck.id, &doc("before")).await.unwrap();
        let updated = repo
            .update_slide_content(&slide.id, &doc("after lyrics"))
            .await
            .unwrap();
        let parsed = SlideDoc::from_json(&updated.content);
        match &parsed.blocks[0] {
            crate::services::slide_doc::SlideBlock::Text { text, .. } => {
                assert_eq!(text, "after lyrics");
            }
            _ => panic!("expected text block"),
        }
    }

    #[tokio::test]
    async fn duplicate_inserts_copy_after_source_and_shifts() {
        let (db, lib) = fixture().await;
        let repo = DeckRepo::new(&db.pool);
        let deck = repo.create_deck(&lib, "Deck").await.unwrap();
        let a = repo.create_slide(&deck.id, &doc("a")).await.unwrap();
        let _b = repo.create_slide(&deck.id, &doc("b")).await.unwrap();
        let dup = repo.duplicate_slide(&a.id).await.unwrap();
        assert_eq!(dup.position, 1);
        assert_eq!(dup.content, a.content);
        let slides = repo.list_slides(&deck.id).await.unwrap();
        // a(0), dup(1), b(2)
        assert_eq!(slides.len(), 3);
        assert_eq!(slides[0].id, a.id);
        assert_eq!(slides[1].id, dup.id);
        assert_eq!(slides[2].content, doc("b").to_json().unwrap());
        assert_eq!(
            slides.iter().map(|s| s.position).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[tokio::test]
    async fn delete_slide_compacts_positions() {
        let (db, lib) = fixture().await;
        let repo = DeckRepo::new(&db.pool);
        let deck = repo.create_deck(&lib, "Deck").await.unwrap();
        let a = repo.create_slide(&deck.id, &doc("a")).await.unwrap();
        let b = repo.create_slide(&deck.id, &doc("b")).await.unwrap();
        let c = repo.create_slide(&deck.id, &doc("c")).await.unwrap();
        repo.delete_slide(&b.id).await.unwrap();
        let slides = repo.list_slides(&deck.id).await.unwrap();
        assert_eq!(slides.len(), 2);
        assert_eq!(slides[0].id, a.id);
        assert_eq!(slides[1].id, c.id);
        assert_eq!(
            slides.iter().map(|s| s.position).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[tokio::test]
    async fn reorder_assigns_positions_by_index() {
        let (db, lib) = fixture().await;
        let repo = DeckRepo::new(&db.pool);
        let deck = repo.create_deck(&lib, "Deck").await.unwrap();
        let a = repo.create_slide(&deck.id, &doc("a")).await.unwrap();
        let b = repo.create_slide(&deck.id, &doc("b")).await.unwrap();
        let c = repo.create_slide(&deck.id, &doc("c")).await.unwrap();
        // Move c to the front: c, a, b
        let reordered = repo
            .reorder_slides(&deck.id, &[c.id.clone(), a.id.clone(), b.id.clone()])
            .await
            .unwrap();
        assert_eq!(reordered[0].id, c.id);
        assert_eq!(reordered[1].id, a.id);
        assert_eq!(reordered[2].id, b.id);
        assert_eq!(
            reordered.iter().map(|s| s.position).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[tokio::test]
    async fn reorder_rejects_mismatched_id_set() {
        let (db, lib) = fixture().await;
        let repo = DeckRepo::new(&db.pool);
        let deck = repo.create_deck(&lib, "Deck").await.unwrap();
        let a = repo.create_slide(&deck.id, &doc("a")).await.unwrap();
        let _b = repo.create_slide(&deck.id, &doc("b")).await.unwrap();
        // Wrong length
        assert_eq!(
            repo.reorder_slides(&deck.id, std::slice::from_ref(&a.id))
                .await
                .unwrap_err()
                .code(),
            "validation"
        );
        // Right length, unknown id
        let err = repo
            .reorder_slides(&deck.id, &[a.id.clone(), "ghost".to_string()])
            .await
            .unwrap_err();
        assert_eq!(err.code(), "validation");
    }
}
