//! Phase 6.1 — opt-in crash-reporting commands.

use tauri::State;

use crate::error::AppResult;
use crate::services::crash;
use crate::AppState;

/// Whether local crash capture is enabled.
#[tauri::command]
pub fn crash_reporting_status(state: State<'_, AppState>) -> bool {
    crash::is_enabled(&state.data_dir)
}

/// Opt in/out of local crash capture.
#[tauri::command]
pub fn crash_reporting_set(state: State<'_, AppState>, enabled: bool) -> AppResult<()> {
    crash::set_enabled(&state.data_dir, enabled)?;
    Ok(())
}

/// How many crash reports have been captured.
#[tauri::command]
pub fn crash_reports_count(state: State<'_, AppState>) -> usize {
    crash::report_count(&state.data_dir)
}

/// Delete all captured crash reports.
#[tauri::command]
pub fn crash_reports_clear(state: State<'_, AppState>) -> AppResult<()> {
    crash::clear_reports(&state.data_dir)?;
    Ok(())
}
