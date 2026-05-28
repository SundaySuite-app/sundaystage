//! Media repository — image/video/audio asset metadata.

use sqlx::SqlitePool;

use crate::db::models::MediaAsset;
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

pub struct MediaRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> MediaRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert a freshly-imported asset. Resolution/duration/thumbnail are
    /// nullable — filled later by the (ffmpeg-backed) probe/thumbnail step.
    pub async fn import(
        &self,
        library_id: &str,
        kind: &str,
        original_path: &str,
        content_hash: &str,
    ) -> AppResult<MediaAsset> {
        let id = new_id();
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO media_asset (id, library_id, kind, original_path, content_hash,
                thumbnail_path, width, height, duration_ms, tags, imported_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, NULL, NULL, NULL, ?6, ?6)
            "#,
        )
        .bind(&id)
        .bind(library_id)
        .bind(kind)
        .bind(original_path)
        .bind(content_hash)
        .bind(now)
        .execute(self.pool)
        .await?;
        self.get(&id).await
    }

    pub async fn get(&self, id: &str) -> AppResult<MediaAsset> {
        sqlx::query_as::<_, MediaAsset>("SELECT * FROM media_asset WHERE id = ?1")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound { entity: "media_asset", id: id.to_string() })
    }

    pub async fn delete(&self, id: &str) -> AppResult<()> {
        let res = sqlx::query("DELETE FROM media_asset WHERE id = ?1")
            .bind(id)
            .execute(self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound { entity: "media_asset", id: id.to_string() });
        }
        Ok(())
    }

    /// Point an asset at a new path (after a hash-based relink).
    pub async fn relink(&self, id: &str, new_path: &str) -> AppResult<MediaAsset> {
        let now = now_ms();
        let res = sqlx::query(
            "UPDATE media_asset SET original_path = ?1, updated_at = ?2 WHERE id = ?3",
        )
        .bind(new_path)
        .bind(now)
        .bind(id)
        .execute(self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound { entity: "media_asset", id: id.to_string() });
        }
        self.get(id).await
    }

    pub async fn list(&self, library_id: &str) -> AppResult<Vec<MediaAsset>> {
        let rows = sqlx::query_as::<_, MediaAsset>(
            "SELECT * FROM media_asset WHERE library_id = ?1 ORDER BY imported_at DESC",
        )
        .bind(library_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Path-stability lookup — used when a stored `original_path` no
    /// longer exists. The Rust caller walks common locations (user media
    /// folder, sundayrec recordings, cloud-sync folders) and asks us to
    /// match by hash.
    pub async fn find_by_hash(&self, content_hash: &str) -> AppResult<Vec<MediaAsset>> {
        let rows = sqlx::query_as::<_, MediaAsset>(
            "SELECT * FROM media_asset WHERE content_hash = ?1",
        )
        .bind(content_hash)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }
}
