//! Theme + template repository (Phase 3.2).
//!
//! Built-in themes/templates live in code ([`crate::services::theme`]) with
//! `library_id = NULL` and `is_builtin = 1`; library-owned customizations are
//! stored rows. List queries return both (built-ins first), so the UI sees one
//! flat catalogue. Editing a built-in isn't allowed — the UI duplicates it
//! into the library first (the "copy built-in into the library" rule from the
//! architecture doc).

use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::db::models::{Library, Template, Theme};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};
use crate::services::slide_doc::SlideDoc;
use crate::services::theme::{
    builtin_templates, builtin_themes, layout_for, render_slide, tokens_for, TemplateLayout,
    ThemeTokens,
};

pub struct ThemeRepo<'a> {
    pool: &'a SqlitePool,
}

fn static_theme_row(id: String, name: String, tokens: &ThemeTokens) -> Theme {
    Theme {
        id,
        library_id: None,
        name,
        tokens: serde_json::to_string(tokens).unwrap_or_default(),
        is_builtin: 1,
        created_at: 0,
        updated_at: 0,
    }
}

fn static_template_row(id: String, name: String, layout: &TemplateLayout) -> Template {
    Template {
        id,
        library_id: None,
        name,
        slots: serde_json::to_string(layout).unwrap_or_default(),
        is_builtin: 1,
        created_at: 0,
        updated_at: 0,
    }
}

impl<'a> ThemeRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    // ── Listing (built-ins ∪ library) ─────────────────────────────────────────

    pub async fn list_themes(&self, library_id: &str) -> AppResult<Vec<Theme>> {
        let mut out: Vec<Theme> = builtin_themes()
            .into_iter()
            .map(|t| static_theme_row(t.id, t.name, &t.tokens))
            .collect();
        let db =
            sqlx::query_as::<_, Theme>("SELECT * FROM theme WHERE library_id = ?1 ORDER BY name")
                .bind(library_id)
                .fetch_all(self.pool)
                .await?;
        out.extend(db);
        Ok(out)
    }

    pub async fn list_templates(&self, library_id: &str) -> AppResult<Vec<Template>> {
        let mut out: Vec<Template> = builtin_templates()
            .into_iter()
            .map(|t| static_template_row(t.id, t.name, &t.layout))
            .collect();
        let db = sqlx::query_as::<_, Template>(
            "SELECT * FROM template WHERE library_id = ?1 ORDER BY name",
        )
        .bind(library_id)
        .fetch_all(self.pool)
        .await?;
        out.extend(db);
        Ok(out)
    }

    pub async fn get_theme(&self, id: &str) -> AppResult<Theme> {
        if let Some(t) = builtin_themes().into_iter().find(|t| t.id == id) {
            return Ok(static_theme_row(t.id, t.name, &t.tokens));
        }
        sqlx::query_as::<_, Theme>("SELECT * FROM theme WHERE id = ?1")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound {
                entity: "theme",
                id: id.to_string(),
            })
    }

    fn is_builtin(id: &str) -> bool {
        builtin_themes().iter().any(|t| t.id == id)
    }

    // ── Mutations ──────────────────────────────────────────────────────────────

    pub async fn create_theme(
        &self,
        library_id: &str,
        name: &str,
        tokens: &ThemeTokens,
    ) -> AppResult<Theme> {
        let id = new_id();
        let now = now_ms();
        let json = serde_json::to_string(tokens)?;
        sqlx::query(
            r#"
            INSERT INTO theme (id, library_id, name, tokens, is_builtin, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)
            "#,
        )
        .bind(&id)
        .bind(library_id)
        .bind(name)
        .bind(&json)
        .bind(now)
        .execute(self.pool)
        .await?;
        self.get_theme(&id).await
    }

    /// Copy any theme (built-in or library) into an editable library theme.
    pub async fn duplicate_theme(&self, source_id: &str, library_id: &str) -> AppResult<Theme> {
        let source = self.get_theme(source_id).await?;
        let tokens: ThemeTokens = serde_json::from_str(&source.tokens).unwrap_or_default();
        self.create_theme(library_id, &format!("{} (kopi)", source.name), &tokens)
            .await
    }

    pub async fn update_theme_tokens(&self, id: &str, tokens: &ThemeTokens) -> AppResult<Theme> {
        if Self::is_builtin(id) {
            return Err(AppError::Validation(
                "innebygde temaer kan ikke endres — dupliser først".to_string(),
            ));
        }
        let json = serde_json::to_string(tokens)?;
        let now = now_ms();
        let res = sqlx::query("UPDATE theme SET tokens = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(&json)
            .bind(now)
            .bind(id)
            .execute(self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound {
                entity: "theme",
                id: id.to_string(),
            });
        }
        self.get_theme(id).await
    }

    pub async fn rename_theme(&self, id: &str, name: &str) -> AppResult<Theme> {
        if Self::is_builtin(id) {
            return Err(AppError::Validation(
                "innebygde temaer kan ikke endres — dupliser først".to_string(),
            ));
        }
        let now = now_ms();
        let res = sqlx::query("UPDATE theme SET name = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(name)
            .bind(now)
            .bind(id)
            .execute(self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound {
                entity: "theme",
                id: id.to_string(),
            });
        }
        self.get_theme(id).await
    }

    pub async fn delete_theme(&self, id: &str) -> AppResult<()> {
        if Self::is_builtin(id) {
            return Err(AppError::Validation(
                "innebygde temaer kan ikke slettes".to_string(),
            ));
        }
        let res = sqlx::query("DELETE FROM theme WHERE id = ?1")
            .bind(id)
            .execute(self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound {
                entity: "theme",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    pub async fn set_library_default_theme(
        &self,
        library_id: &str,
        theme_id: Option<&str>,
    ) -> AppResult<Library> {
        let now = now_ms();
        sqlx::query("UPDATE library SET default_theme_id = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(theme_id)
            .bind(now)
            .bind(library_id)
            .execute(self.pool)
            .await?;
        self.library(library_id).await
    }

    pub async fn set_library_default_template(
        &self,
        library_id: &str,
        template_id: Option<&str>,
    ) -> AppResult<Library> {
        let now = now_ms();
        sqlx::query("UPDATE library SET default_template_id = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(template_id)
            .bind(now)
            .bind(library_id)
            .execute(self.pool)
            .await?;
        self.library(library_id).await
    }

    async fn library(&self, id: &str) -> AppResult<Library> {
        sqlx::query_as::<_, Library>("SELECT * FROM library WHERE id = ?1")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound {
                entity: "library",
                id: id.to_string(),
            })
    }

    // ── Render bridge ──────────────────────────────────────────────────────────

    async fn db_theme_tokens(&self, library_id: &str) -> AppResult<HashMap<String, ThemeTokens>> {
        let rows = sqlx::query_as::<_, Theme>("SELECT * FROM theme WHERE library_id = ?1")
            .bind(library_id)
            .fetch_all(self.pool)
            .await?;
        let mut map = HashMap::new();
        for r in rows {
            if let Ok(tok) = serde_json::from_str::<ThemeTokens>(&r.tokens) {
                map.insert(r.id, tok);
            }
        }
        Ok(map)
    }

    async fn db_template_layouts(
        &self,
        library_id: &str,
    ) -> AppResult<HashMap<String, TemplateLayout>> {
        let rows = sqlx::query_as::<_, Template>("SELECT * FROM template WHERE library_id = ?1")
            .bind(library_id)
            .fetch_all(self.pool)
            .await?;
        let mut map = HashMap::new();
        for r in rows {
            if let Ok(layout) = serde_json::from_str::<TemplateLayout>(&r.slots) {
                map.insert(r.id, layout);
            }
        }
        Ok(map)
    }

    /// Render a slide from a template + theme + slot text, resolving ids
    /// against built-ins and this library's stored themes/templates.
    pub async fn render(
        &self,
        library_id: &str,
        template_id: &str,
        theme_id: &str,
        slot_text: &HashMap<String, String>,
    ) -> AppResult<SlideDoc> {
        let tokens = tokens_for(theme_id, &self.db_theme_tokens(library_id).await?);
        let layout = layout_for(template_id, &self.db_template_layouts(library_id).await?);
        Ok(render_slide(&layout, &tokens, slot_text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::LibraryInput;
    use crate::db::repositories::LibraryRepo;
    use crate::db::Database;
    use crate::services::theme::DEFAULT_THEME_ID;

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
    async fn list_includes_builtins_and_library_themes() {
        let (db, lib) = fixture().await;
        let repo = ThemeRepo::new(&db.pool);
        let before = repo.list_themes(&lib).await.unwrap();
        let builtin_count = before.len();
        assert!(builtin_count >= 5);
        assert!(before.iter().all(|t| t.is_builtin == 1));

        repo.create_theme(&lib, "Min stil", &ThemeTokens::default())
            .await
            .unwrap();
        let after = repo.list_themes(&lib).await.unwrap();
        assert_eq!(after.len(), builtin_count + 1);
        assert!(after
            .iter()
            .any(|t| t.name == "Min stil" && t.is_builtin == 0));
    }

    #[tokio::test]
    async fn builtin_themes_are_immutable() {
        let (db, lib) = fixture().await;
        let repo = ThemeRepo::new(&db.pool);
        let err = repo
            .update_theme_tokens(DEFAULT_THEME_ID, &ThemeTokens::default())
            .await
            .unwrap_err();
        assert_eq!(err.code(), "validation");
        assert_eq!(
            repo.delete_theme(DEFAULT_THEME_ID)
                .await
                .unwrap_err()
                .code(),
            "validation"
        );
        let _ = lib;
    }

    #[tokio::test]
    async fn duplicate_builtin_creates_editable_copy() {
        let (db, lib) = fixture().await;
        let repo = ThemeRepo::new(&db.pool);
        let copy = repo.duplicate_theme(DEFAULT_THEME_ID, &lib).await.unwrap();
        assert_eq!(copy.is_builtin, 0);
        assert!(copy.name.contains("kopi"));
        // The copy is editable.
        let edited = ThemeTokens {
            text_color: "#ff0000".into(),
            ..ThemeTokens::default()
        };
        let updated = repo.update_theme_tokens(&copy.id, &edited).await.unwrap();
        let tok: ThemeTokens = serde_json::from_str(&updated.tokens).unwrap();
        assert_eq!(tok.text_color, "#ff0000");
    }

    #[tokio::test]
    async fn set_library_default_theme_persists() {
        let (db, lib) = fixture().await;
        let repo = ThemeRepo::new(&db.pool);
        let updated = repo
            .set_library_default_theme(&lib, Some("builtin-theme-evening"))
            .await
            .unwrap();
        assert_eq!(
            updated.default_theme_id.as_deref(),
            Some("builtin-theme-evening")
        );
        // Clearing it back to None works too.
        let cleared = repo.set_library_default_theme(&lib, None).await.unwrap();
        assert_eq!(cleared.default_theme_id, None);
    }

    #[tokio::test]
    async fn render_uses_library_theme_tokens() {
        let (db, lib) = fixture().await;
        let repo = ThemeRepo::new(&db.pool);
        let custom = ThemeTokens {
            text_color: "#abcdef".into(),
            ..ThemeTokens::default()
        };
        let theme = repo.create_theme(&lib, "Custom", &custom).await.unwrap();
        let mut text = HashMap::new();
        text.insert("lyrics".to_string(), "Hello".to_string());
        let doc = repo
            .render(&lib, "builtin-template-lyrics-centered", &theme.id, &text)
            .await
            .unwrap();
        match &doc.blocks[0] {
            crate::services::slide_doc::SlideBlock::Text { style, .. } => {
                assert_eq!(style.color, "#abcdef");
            }
            _ => panic!("expected text block"),
        }
    }
}
