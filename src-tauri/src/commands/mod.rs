//! Tauri command handlers — one module per repository.
//!
//! Commands NEVER touch sqlx directly. They go through repositories.
//! Every command returns `Result<T, AppError>` which serialises to JSON
//! with stable `{ code, message }` shape (see error.rs).

pub mod libraries;
pub mod songs;
pub mod services;
pub mod live;
pub mod decks;
pub mod themes;
pub mod arrangements;
pub mod ai;
pub mod media;
