//! SundayStage main library — Tauri runtime entry point.
//!
//! Wires up:
//!   - Database (SQLite via sqlx) opened in user's app-local data dir
//!   - AppState held via `tauri::Manager::manage(...)`
//!   - All IPC command handlers
//!
//! The actual command implementations live in `commands::*` — this file
//! only registers them.

pub mod commands;
pub mod db;
pub mod error;

use std::path::PathBuf;
use tauri::Manager;

use crate::db::Database;

/// Tauri-managed shared state. Cloneable handle to the sqlx pool.
pub struct AppState {
    pub db: Database,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Resolve the app-local data directory and open the database.
            let data_dir = app
                .path()
                .app_local_data_dir()
                .expect("app_local_data_dir resolves on supported platforms");
            std::fs::create_dir_all(&data_dir).ok();
            let db_path: PathBuf = data_dir.join("sundaystage.db");

            // Open the database synchronously — Tauri's setup is not async.
            let db = tauri::async_runtime::block_on(async move {
                Database::open(&db_path).await
            })?;

            app.manage(AppState { db });
            tracing::info!("SundayStage backend ready");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Library
            commands::libraries::library_create,
            commands::libraries::library_get,
            commands::libraries::library_list,
            commands::libraries::library_rename,
            // Song
            commands::songs::song_create,
            commands::songs::song_get,
            commands::songs::song_list,
            commands::songs::song_delete,
            commands::songs::song_search,
            commands::songs::song_sections,
            commands::songs::song_add_section,
            // Service
            commands::services::service_create,
            commands::services::service_get,
            commands::services::service_upcoming,
            commands::services::service_items,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
