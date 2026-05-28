//! Tauri command handlers — one module per repository.
//!
//! Commands NEVER touch sqlx directly. They go through repositories.
//! Every command returns `Result<T, AppError>` which serialises to JSON
//! with stable `{ code, message }` shape (see error.rs).

pub mod ai;
pub mod arrangements;
pub mod decks;
pub mod libraries;
pub mod live;
pub mod media;
pub mod onboarding;
pub mod services;
pub mod songs;
pub mod sync;
pub mod themes;
