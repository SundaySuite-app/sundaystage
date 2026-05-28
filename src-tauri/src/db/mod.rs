//! Database module: connection pool, migrations, repositories.
//!
//! The application opens a single SQLite database file per library.
//! Migrations live in `sql/` at the repo root and are embedded via
//! `sqlx::migrate!` at compile time.
//!
//! Usage:
//! ```ignore
//! let pool = Database::open("path/to/library.db").await?;
//! let song = SongRepo::new(&pool).get(&id).await?;
//! ```

pub mod models;
pub mod repositories;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};
use std::path::Path;
use std::str::FromStr;

use crate::error::AppResult;

/// Database handle wrapping the sqlx pool.
#[derive(Clone, Debug)]
pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    /// Open a database at `path`. Creates the file if it doesn't exist and
    /// runs all migrations from the `sql/` directory.
    pub async fn open<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let url = format!("sqlite:{}", path.to_string_lossy());
        let opts = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;

        run_migrations(&pool).await?;

        Ok(Database { pool })
    }

    /// In-memory database for tests.
    pub async fn open_in_memory() -> AppResult<Self> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1) // shared in-memory needs a single connection
            .connect_with(opts)
            .await?;
        run_migrations(&pool).await?;
        Ok(Database { pool })
    }
}

/// Apply migrations bundled at compile time. Migrations live in
/// `sql/` at the workspace root (one level above `src-tauri`).
async fn run_migrations(pool: &SqlitePool) -> AppResult<()> {
    sqlx::migrate!("../sql").run(pool).await?;
    Ok(())
}

/// Current unix-ms timestamp — every domain entity uses this.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_millis() as i64
}

/// Generate a new UUIDv7 as TEXT — sortable by creation time.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn opens_in_memory_and_runs_migrations() {
        let db = Database::open_in_memory().await.expect("open");
        // schema_migrations should have one row from 0001_initial
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM schema_migrations")
            .fetch_one(&db.pool)
            .await
            .expect("query");
        assert!(row.0 >= 1, "expected at least one applied migration");
    }

    #[tokio::test]
    async fn new_id_is_uuid_v7() {
        let id = new_id();
        let parsed = uuid::Uuid::parse_str(&id).expect("valid uuid");
        assert_eq!(parsed.get_version_num(), 7);
    }
}
