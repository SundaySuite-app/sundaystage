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

    pub async fn create(&self, library_id: &str, name: &str, starts_at: i64) -> AppResult<Service> {
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
        sqlx::query_as::<_, Service>("SELECT * FROM service WHERE id = ?1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound {
                entity: "service",
                id: id.to_string(),
            })
    }

    pub async fn upcoming(
        &self,
        library_id: &str,
        from: i64,
        limit: i64,
    ) -> AppResult<Vec<Service>> {
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

    /// Rename a service.
    pub async fn rename(&self, id: &str, name: &str) -> AppResult<Service> {
        sqlx::query("UPDATE service SET name = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(name)
            .bind(now_ms())
            .bind(id)
            .execute(self.pool)
            .await?;
        self.get(id).await
    }

    /// Set the service's planner notes.
    pub async fn set_notes(&self, id: &str, notes: &str) -> AppResult<Service> {
        sqlx::query("UPDATE service SET notes = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(notes)
            .bind(now_ms())
            .bind(id)
            .execute(self.pool)
            .await?;
        self.get(id).await
    }

    /// The position to append the next item at (current item count). Items use
    /// dense 0-based positions; appending at the count keeps them contiguous.
    pub async fn next_position(&self, service_id: &str) -> AppResult<i64> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM service_item WHERE service_id = ?1")
                .bind(service_id)
                .fetch_one(self.pool)
                .await?;
        Ok(count)
    }

    /// Remove an item and close the gap so positions stay contiguous (the
    /// CueList compiler walks them in `position` order).
    pub async fn remove_item(&self, item_id: &str) -> AppResult<()> {
        let row: Option<(String, i64)> =
            sqlx::query_as("SELECT service_id, position FROM service_item WHERE id = ?1")
                .bind(item_id)
                .fetch_optional(self.pool)
                .await?;
        let Some((service_id, position)) = row else {
            return Err(AppError::NotFound {
                entity: "service_item",
                id: item_id.to_string(),
            });
        };
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM service_item WHERE id = ?1")
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE service_item SET position = position - 1, updated_at = ?1
             WHERE service_id = ?2 AND position > ?3",
        )
        .bind(now_ms())
        .bind(&service_id)
        .bind(position)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Reorder a service's items to match `ordered_ids` exactly (0-based
    /// positions). Pass the full ordered set of the service's item ids.
    ///
    /// `(service_id, position)` is uniquely indexed and SQLite checks it per
    /// statement, so we can't shuffle in place — assigning a final position
    /// would collide with whichever row currently holds it. Two-pass: first
    /// park every listed row at a distinct negative slot (out of the way of the
    /// 0..n range), then write the final positions.
    pub async fn reorder_items(
        &self,
        service_id: &str,
        ordered_ids: &[String],
    ) -> AppResult<Vec<ServiceItem>> {
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        for (pos, id) in ordered_ids.iter().enumerate() {
            sqlx::query(
                "UPDATE service_item SET position = ?1, updated_at = ?2
                 WHERE id = ?3 AND service_id = ?4",
            )
            .bind(-(pos as i64) - 1)
            .bind(now)
            .bind(id)
            .bind(service_id)
            .execute(&mut *tx)
            .await?;
        }
        for (pos, id) in ordered_ids.iter().enumerate() {
            sqlx::query(
                "UPDATE service_item SET position = ?1, updated_at = ?2
                 WHERE id = ?3 AND service_id = ?4",
            )
            .bind(pos as i64)
            .bind(now)
            .bind(id)
            .bind(service_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.items(service_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::LibraryInput;
    use crate::db::repositories::LibraryRepo;
    use crate::db::Database;

    #[tokio::test]
    async fn create_and_list_upcoming() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "Test".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        let repo = ServiceRepo::new(&db.pool);
        let svc = repo
            .create(&lib.id, "Sunday 14 Sept", 1_758_540_000_000)
            .await
            .unwrap();
        assert_eq!(svc.state, "planned");
        let upcoming = repo.upcoming(&lib.id, 0, 10).await.unwrap();
        assert_eq!(upcoming.len(), 1);
        assert_eq!(upcoming[0].id, svc.id);
    }

    async fn song_in(db: &Database, library_id: &str, title: &str) -> String {
        use crate::db::models::SongInput;
        use crate::db::repositories::SongRepo;
        SongRepo::new(&db.pool)
            .create(SongInput {
                library_id: library_id.into(),
                title: title.into(),
                language: Some("no".into()),
                default_key: None,
                tempo_bpm: None,
                ccli_song_id: None,
                tono_work_id: None,
                copyright_notice: None,
            })
            .await
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn add_remove_reorder_keeps_positions_contiguous() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "Test".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        let repo = ServiceRepo::new(&db.pool);
        let svc = repo.create(&lib.id, "Svc", now_ms()).await.unwrap();

        // Append three songs at the running next_position.
        let mut ids = Vec::new();
        for title in ["A", "B", "C"] {
            let song_id = song_in(&db, &lib.id, title).await;
            let pos = repo.next_position(&svc.id).await.unwrap();
            let item = repo
                .add_item(&svc.id, pos, "song", Some(&song_id), None, None, None, None)
                .await
                .unwrap();
            ids.push(item.id);
        }
        let items = repo.items(&svc.id).await.unwrap();
        assert_eq!(
            items.iter().map(|i| i.position).collect::<Vec<_>>(),
            [0, 1, 2]
        );

        // Remove the middle one — positions must close the gap to 0,1.
        repo.remove_item(&ids[1]).await.unwrap();
        let items = repo.items(&svc.id).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items.iter().map(|i| i.position).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(items[0].id, ids[0]);
        assert_eq!(items[1].id, ids[2]);

        // Reorder: C before A.
        let reordered = repo
            .reorder_items(&svc.id, &[ids[2].clone(), ids[0].clone()])
            .await
            .unwrap();
        assert_eq!(reordered[0].id, ids[2]);
        assert_eq!(reordered[1].id, ids[0]);
        assert_eq!(
            reordered.iter().map(|i| i.position).collect::<Vec<_>>(),
            [0, 1]
        );
    }

    #[tokio::test]
    async fn remove_missing_item_errors() {
        let db = Database::open_in_memory().await.unwrap();
        let repo = ServiceRepo::new(&db.pool);
        assert!(repo.remove_item("does-not-exist").await.is_err());
    }
}
