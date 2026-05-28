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
pub mod output;
pub mod services;

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

use crate::db::Database;
use crate::services::live_session::LiveSession;

/// Tauri-managed shared state.
pub struct AppState {
    pub db: Database,
    /// App-local data directory (db file, persisted live session).
    pub data_dir: PathBuf,
    /// The running live session, if any. Held behind a `Mutex` because cue
    /// advance mutates it from command handlers; guards are never held across
    /// an `.await`.
    pub live: Mutex<Option<LiveSession>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
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
            let db = tauri::async_runtime::block_on(async move { Database::open(&db_path).await })?;

            app.manage(AppState {
                db,
                data_dir,
                live: std::sync::Mutex::new(None),
            });
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
            commands::songs::song_update_section,
            commands::songs::song_delete_section,
            commands::songs::song_reorder_sections,
            // Arrangements (Phase 3.3)
            commands::arrangements::arrangement_create,
            commands::arrangements::arrangement_list,
            commands::arrangements::arrangement_rename,
            commands::arrangements::arrangement_delete,
            commands::arrangements::arrangement_set_default,
            commands::arrangements::arrangement_duplicate,
            commands::arrangements::arrangement_items,
            commands::arrangements::arrangement_set_items,
            commands::arrangements::arrangement_sections,
            // AI (Phase 4)
            commands::ai::ai_models,
            commands::ai::ai_format_lyrics,
            commands::ai::ai_apply_format,
            commands::ai::ai_plan_service,
            commands::ai::ai_apply_plan,
            // Media (Phase 7.2)
            commands::media::media_import,
            commands::media::media_list,
            commands::media::media_delete,
            commands::media::media_relink,
            // Onboarding + i18n (Phase 13.1)
            commands::onboarding::app_locales,
            commands::onboarding::onboarding_seed_demo,
            // Cloud sync (Phase 9)
            commands::sync::sync_status,
            // Output displays (Phase 5.2)
            commands::output::output_monitors,
            commands::output::output_config,
            commands::output::output_set_config,
            commands::output::output_open,
            commands::output::output_close,
            commands::output::output_is_open,
            // Service
            commands::services::service_create,
            commands::services::service_get,
            commands::services::service_upcoming,
            commands::services::service_items,
            // Live engine
            commands::live::live_compile_cue_list,
            commands::live::live_start,
            commands::live::live_dispatch,
            commands::live::live_state,
            commands::live::live_end,
            commands::live::live_recover,
            commands::live::stage_presets,
            // SundayRec bridge (Phase 10)
            commands::live::bridge_protocol_version,
            commands::live::bridge_chapter_markers,
            commands::live::bridge_export_srt,
            // Custom decks + slides (Phase 3.1 slide editor)
            commands::decks::deck_create,
            commands::decks::deck_get,
            commands::decks::deck_list,
            commands::decks::deck_rename,
            commands::decks::deck_delete,
            commands::decks::slide_create,
            commands::decks::slide_list,
            commands::decks::slide_update_content,
            commands::decks::slide_duplicate,
            commands::decks::slide_delete,
            commands::decks::slide_reorder,
            // Themes + templates (Phase 3.2)
            commands::themes::theme_list,
            commands::themes::template_list,
            commands::themes::theme_create,
            commands::themes::theme_duplicate,
            commands::themes::theme_update_tokens,
            commands::themes::theme_rename,
            commands::themes::theme_delete,
            commands::themes::library_set_default_theme,
            commands::themes::library_set_default_template,
            commands::themes::slide_set_theme,
            commands::themes::slide_set_template,
            commands::themes::template_render,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
