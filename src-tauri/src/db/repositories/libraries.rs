//! Library repository — create / list / update the top-level tenant container.

use sqlx::SqlitePool;

use crate::db::models::{Library, LibraryInput};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

pub struct LibraryRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> LibraryRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, input: LibraryInput) -> AppResult<Library> {
        let id = new_id();
        let now = now_ms();
        let locale = input.default_locale.unwrap_or_else(|| "no".to_string());

        sqlx::query(
            r#"
            INSERT INTO library (id, name, default_locale, default_theme_id, default_template_id, created_at, updated_at)
            VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4)
            "#,
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&locale)
        .bind(now)
        .execute(self.pool)
        .await?;

        self.get(&id).await
    }

    pub async fn get(&self, id: &str) -> AppResult<Library> {
        sqlx::query_as::<_, Library>("SELECT * FROM library WHERE id = ?1")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound {
                entity: "library",
                id: id.to_string(),
            })
    }

    pub async fn list(&self) -> AppResult<Vec<Library>> {
        let rows = sqlx::query_as::<_, Library>("SELECT * FROM library ORDER BY name")
            .fetch_all(self.pool)
            .await?;
        Ok(rows)
    }

    pub async fn rename(&self, id: &str, name: &str) -> AppResult<Library> {
        let now = now_ms();
        sqlx::query("UPDATE library SET name = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(name)
            .bind(now)
            .bind(id)
            .execute(self.pool)
            .await?;
        self.get(id).await
    }

    pub async fn delete(&self, id: &str) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM library WHERE id = ?1")
            .bind(id)
            .execute(self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                entity: "library",
                id: id.to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    async fn db() -> Database {
        Database::open_in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn create_and_get_library() {
        let db = db().await;
        let repo = LibraryRepo::new(&db.pool);
        let lib = repo
            .create(LibraryInput {
                name: "Alta Frikirke".into(),
                default_locale: Some("no".into()),
            })
            .await
            .unwrap();
        assert_eq!(lib.name, "Alta Frikirke");
        assert_eq!(lib.default_locale, "no");

        let fetched = repo.get(&lib.id).await.unwrap();
        assert_eq!(fetched.id, lib.id);
    }

    #[tokio::test]
    async fn rename_library() {
        let db = db().await;
        let repo = LibraryRepo::new(&db.pool);
        let lib = repo
            .create(LibraryInput {
                name: "Test".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        let renamed = repo.rename(&lib.id, "Renamed").await.unwrap();
        assert_eq!(renamed.name, "Renamed");
        assert!(renamed.updated_at >= lib.updated_at);
    }

    #[tokio::test]
    async fn list_libraries_sorted_by_name() {
        let db = db().await;
        let repo = LibraryRepo::new(&db.pool);
        repo.create(LibraryInput { name: "Beta".into(), default_locale: None }).await.unwrap();
        repo.create(LibraryInput { name: "Alpha".into(), default_locale: None }).await.unwrap();
        let list = repo.list().await.unwrap();
        assert_eq!(list[0].name, "Alpha");
        assert_eq!(list[1].name, "Beta");
    }

    #[tokio::test]
    async fn get_missing_library_returns_not_found() {
        let db = db().await;
        let repo = LibraryRepo::new(&db.pool);
        let err = repo.get("nope").await.unwrap_err();
        assert_eq!(err.code(), "not_found");
    }
}
