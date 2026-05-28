//! Tauri commands for first-run onboarding + i18n (Phase 13.1).

use tauri::State;

use crate::error::AppResult;
use crate::services::demo::{seed_demo_content, supported_locales, DemoSummary, LocaleInfo};
use crate::AppState;

/// The UI languages the app offers.
#[tauri::command]
pub fn app_locales() -> Vec<LocaleInfo> {
    supported_locales()
}

/// Prefill a library with the demo "Velkomstgudstjeneste".
#[tauri::command]
pub async fn onboarding_seed_demo(
    state: State<'_, AppState>,
    library_id: String,
) -> AppResult<DemoSummary> {
    seed_demo_content(&state.db.pool, &library_id).await
}
