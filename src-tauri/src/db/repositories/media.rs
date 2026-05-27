//! Media repository — image/video/audio asset metadata.

use sqlx::SqlitePool;

use crate::db::models::MediaAsset;
use crate::error::AppResult;

pub struct MediaRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> MediaRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
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
