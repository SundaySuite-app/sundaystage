//! Service repository — services + their ordered items.
//!
//! The CueList compiler (Phase 5) reads through this repo to materialize
//! a flat list of cues for the live engine. Keep queries here aligned with
//! that downstream consumer.

use sqlx::SqlitePool;

use crate::db::models::{Service, ServiceItem};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

pub struct ServiceRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ServiceRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        library_id: &str,
        name: &str,
        starts_at: i64,
    ) -> AppResult<Service> {
        let id = new_id();
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO service (id, library_id, name, starts_at, notes, state, created_at, updated_at, deleted_at)
            VALUES (?1, ?2, ?3, ?4, NULL, 'planned', ?5, ?5, NULL)
            "#,
        )
        .bind(&id)
        .bind(library_id)
        .bind(name)
        .bind(starts_at)
        .bind(now)
        .execute(self.pool)
        .await?;
        self.get(&id).await
    }

    pub async fn get(&self, id: &str) -> AppResult<Service> {
        sqlx::query_as::<_, Service>(
            "SELECT * FROM service WHERE id = ?1 AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound {
            entity: "service",
            id: id.to_string(),
        })
    }

    pub async fn upcoming(&self, library_id: &str, from: i64, limit: i64) -> AppResult<Vec<Service>> {
        let rows = sqlx::query_as::<_, Service>(
            r#"
            SELECT * FROM service
            WHERE library_id = ?1 AND deleted_at IS NULL AND starts_at >= ?2
            ORDER BY starts_at ASC
            LIMIT ?3
            "#,
        )
        .bind(library_id)
        .bind(from)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn items(&self, service_id: &str) -> AppResult<Vec<ServiceItem>> {
        let rows = sqlx::query_as::<_, ServiceItem>(
            "SELECT * FROM service_item WHERE service_id = ?1 ORDER BY position",
        )
        .bind(service_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Append an item to a service. `kind` must be one of the schema's allowed
    /// values; the matching id column should be set for `song`/`scripture`/etc.
    #[allow(clippy::too_many_arguments)]
    pub async fn add_item(
        &self,
        service_id: &str,
        position: i64,
        kind: &str,
        song_id: Option<&str>,
        arrangement_id: Option<&str>,
        key_override: Option<&str>,
        bible_reference_id: Option<&str>,
        notes: Option<&str>,
    ) -> AppResult<ServiceItem> {
        let id = new_id();
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO service_item (id, service_id, position, kind, song_id,
                arrangement_id, key_override, bible_reference_id, custom_deck_id,
                media_asset_id, notes, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL, ?9, ?10, ?10)
            "#,
        )
        .bind(&id)
        .bind(service_id)
        .bind(position)
        .bind(kind)
        .bind(song_id)
        .bind(arrangement_id)
        .bind(key_override)
        .bind(bible_reference_id)
        .bind(notes)
        .bind(now)
        .execute(self.pool)
        .await?;
        sqlx::query_as::<_, ServiceItem>("SELECT * FROM service_item WHERE id = ?1")
            .bind(&id)
            .fetch_one(self.pool)
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::LibraryRepo;
    use crate::db::models::LibraryInput;
    use crate::db::Database;

    #[tokio::test]
    async fn create_and_list_upcoming() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput { name: "Test".into(), default_locale: None })
            .await.unwrap();
        let repo = ServiceRepo::new(&db.pool);
        let svc = repo.create(&lib.id, "Sunday 14 Sept", 1_758_540_000_000).await.unwrap();
        assert_eq!(svc.state, "planned");
        let upcoming = repo.upcoming(&lib.id, 0, 10).await.unwrap();
        assert_eq!(upcoming.len(), 1);
        assert_eq!(upcoming[0].id, svc.id);
    }
}
