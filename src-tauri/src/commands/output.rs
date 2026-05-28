//! Phase 5.2 — output display commands.
//!
//! Enumerate monitors, persist the operator's role assignments, and open/close
//! the borderless full-screen output windows. The window glue lives in
//! [`crate::output::window`]; the assignment logic in
//! [`crate::services::display`].

use std::path::PathBuf;
use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::output::window;
use crate::services::display::{self, MonitorInfo, OutputConfig};
use crate::AppState;

fn config_path(state: &AppState) -> PathBuf {
    state.data_dir.join("output_config.json")
}

fn load_config(state: &AppState) -> OutputConfig {
    std::fs::read_to_string(config_path(state))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(state: &AppState, cfg: &OutputConfig) {
    if let Ok(s) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(config_path(state), s);
    }
}

/// Resolve the assignments to use: the saved ones reconciled against the
/// monitors currently present, or a sensible default for a fresh machine.
fn resolve(app: &AppHandle, state: &AppState) -> (Vec<MonitorInfo>, OutputConfig) {
    let monitors = window::list_monitors(app);
    let saved = load_config(state);
    let assignments = if saved.assignments.is_empty() {
        display::default_assignments(&monitors)
    } else {
        display::reconcile(&saved.assignments, &monitors)
    };
    (monitors, OutputConfig { assignments })
}

/// The monitors currently connected.
#[tauri::command]
pub fn output_monitors(app: AppHandle) -> Vec<MonitorInfo> {
    window::list_monitors(&app)
}

/// Current role assignments, reconciled against live monitors and persisted.
#[tauri::command]
pub fn output_config(app: AppHandle, state: State<'_, AppState>) -> OutputConfig {
    let (_, cfg) = resolve(&app, &state);
    save_config(&state, &cfg);
    cfg
}

/// Persist explicit role assignments chosen by the operator.
#[tauri::command]
pub fn output_set_config(state: State<'_, AppState>, config: OutputConfig) -> AppResult<()> {
    save_config(&state, &config);
    Ok(())
}

/// Open output windows for the resolved assignments.
#[tauri::command]
pub fn output_open(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let (monitors, cfg) = resolve(&app, &state);
    save_config(&state, &cfg);
    window::open_outputs(&app, &monitors, &cfg.assignments).map_err(AppError::Validation)
}

/// Close all output windows.
#[tauri::command]
pub fn output_close(app: AppHandle) {
    window::close_outputs(&app);
}

/// Whether any output windows are currently open.
#[tauri::command]
pub fn output_is_open(app: AppHandle) -> bool {
    window::outputs_open(&app)
}
