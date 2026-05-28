//! Centralised error type for the SundayStage backend.
//!
//! Tauri commands return `Result<T, AppError>` — `AppError` implements
//! `serde::Serialize` so it crosses the IPC boundary as a stable JSON shape
//! that the renderer can pattern-match on.

use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    /// Underlying SQLite/sqlx failure. Anything we couldn't classify into
    /// a more specific variant ends up here.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Schema migration failed. Generally unrecoverable at runtime.
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// Entity not found by ID — distinct from a general database error so
    /// the renderer can render a "404" UI.
    #[error("not found: {entity} id={id}")]
    NotFound { entity: &'static str, id: String },

    /// Constraint violation — e.g. trying to insert a tag that already
    /// exists in the library. Renderer can show a user-facing validation
    /// message.
    #[error("conflict: {0}")]
    Conflict(String),

    /// Caller passed input that fails our domain rules.
    #[error("validation: {0}")]
    Validation(String),

    /// File-system / IO failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialisation/deserialisation issue for fields like
    /// `slide.content`, `theme.tokens`.
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),

    /// Anything else.
    #[error("internal: {0}")]
    Internal(String),
}

impl AppError {
    /// Short, machine-readable category for the renderer to switch on.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Database(_) => "database",
            AppError::Migration(_) => "migration",
            AppError::NotFound { .. } => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::Validation(_) => "validation",
            AppError::Io(_) => "io",
            AppError::Json(_) => "json",
            AppError::Internal(_) => "internal",
        }
    }
}

/// Custom serializer so the JSON sent to the renderer has both a stable
/// `code` field (for switch statements) and the human-readable `message`.
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

/// Convenience alias for the project.
pub type AppResult<T> = Result<T, AppError>;
