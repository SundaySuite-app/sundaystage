//! SundayStage main library — Tauri runtime entry point.
//!
//! Wires up:
//!   - Database (SQLite via sqlx) opened in user's app-local data dir
//!   - AppState held via `tauri::Manager::manage(...)`
//!   - All IPC command handlers
//!
//! The actual command implementations live in `commands::*` — this file
//! only registers them.

pub mod account;
pub mod commands;
pub mod db;
pub mod error;
pub mod output;
pub mod services;

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

use crate::db::Database;
use crate::services::companion::transport::{CompanionBroadcaster, RealtimeTransport};
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
    /// Phase 12.2 — the companion broadcaster for the running session, if any.
    /// Created on `live_start`, fed on `live_dispatch`, terminated on
    /// `live_end`. The network transport is a no-op until the cloud layer is
    /// configured, so it is always safe to drive.
    pub companion: Mutex<Option<CompanionBroadcaster<RealtimeTransport>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .init();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    // Auto-update + relaunch (Phase 13.2) are desktop-only.
    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init());
    }

    builder
        .setup(|app| {
            // Resolve the app-local data directory and open the database.
            let data_dir = app
                .path()
                .app_local_data_dir()
                .expect("app_local_data_dir resolves on supported platforms");
            std::fs::create_dir_all(&data_dir).ok();

            // Opt-in crash capture (Phase 6.1): the hook checks the user's
            // choice at panic time, so installing it always is safe.
            crate::services::crash::install_panic_hook(data_dir.clone());

            let db_path: PathBuf = data_dir.join("sundaystage.db");

            // Open the database synchronously — Tauri's setup is not async.
            // Seed the bundled Bible translations and built-in service
            // templates (both operations are idempotent).
            let db = tauri::async_runtime::block_on(async move {
                let db = Database::open(&db_path).await?;
                crate::db::repositories::BibleRepo::new(&db.pool)
                    .seed()
                    .await?;
                crate::db::repositories::ServiceTemplateRepo::new(&db.pool)
                    .seed_builtins()
                    .await?;
                crate::error::AppResult::Ok(db)
            })?;

            app.manage(AppState {
                db,
                data_dir,
                live: std::sync::Mutex::new(None),
                companion: std::sync::Mutex::new(None),
            });
            tracing::info!("SundayStage backend ready");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Sunday Account (SSO) — shared cross-app session
            commands::account::sunday_account_status,
            commands::account::sunday_sign_out,
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
            commands::import::import_song_file,
            commands::ai::ai_plan_service,
            commands::ai::ai_apply_plan,
            commands::ai::ai_translate,
            commands::ai::ai_key_set,
            commands::ai::ai_key_clear,
            commands::ai::ai_key_status,
            commands::ai::ai_test_connection,
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
            // Crash reporting (Phase 6.1)
            commands::crash::crash_reporting_status,
            commands::crash::crash_reporting_set,
            commands::crash::crash_reports_count,
            commands::crash::crash_reports_clear,
            // Universal search (Phase 2.3)
            commands::search::search_all,
            // Bible (Phase 7.1)
            commands::bible::bible_translations,
            commands::bible::bible_books,
            commands::bible::bible_chapters,
            commands::bible::bible_passage,
            commands::bible::bible_lookup,
            commands::bible::bible_search,
            commands::bible::bible_add_to_service,
            // Output displays (Phase 5.2)
            commands::output::output_monitors,
            commands::output::output_config,
            commands::output::output_set_config,
            commands::output::output_open,
            commands::output::output_close,
            commands::output::output_is_open,
            commands::output::output_appearance,
            commands::output::output_set_appearance,
            commands::output::output_display_config,
            commands::output::output_set_display_config,
            // Service templates
            commands::service_templates::svc_template_create,
            commands::service_templates::svc_template_list,
            commands::service_templates::svc_template_delete,
            commands::service_templates::svc_template_apply,
            // Service
            commands::services::service_create,
            commands::services::service_get,
            commands::services::service_upcoming,
            commands::services::service_items,
            commands::services::songs_by_item,
            commands::services::service_rename,
            commands::services::service_set_notes,
            commands::services::service_set_starts_at,
            commands::services::service_delete,
            commands::services::service_add_song,
            commands::services::service_add_item,
            commands::services::service_update_item,
            commands::services::service_remove_item,
            commands::services::service_reorder_items,
            commands::services::service_cue_summary,
            commands::services::service_import_sundayplan,
            // Live engine
            commands::live::live_compile_cue_list,
            commands::live::live_start,
            commands::live::live_dispatch,
            commands::live::live_state,
            commands::live::live_end,
            commands::live::live_recover,
            commands::live::companion_channel,
            commands::live::companion_broadcast,
            commands::live::stage_presets,
            // SundayRec bridge (Phase 10)
            commands::live::bridge_protocol_version,
            commands::live::bridge_chapter_markers,
            commands::live::bridge_export_srt,
            commands::live::bridge_export_manifest,
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
