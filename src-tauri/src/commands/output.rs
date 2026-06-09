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
use crate::services::display::{
    self, MonitorInfo, OutputAppearance, OutputConfig, OutputDisplayConfig,
};
use crate::AppState;

fn config_path(state: &AppState) -> PathBuf {
    state.data_dir.join("output_config.json")
}

fn appearance_path(state: &AppState) -> PathBuf {
    state.data_dir.join("output_appearance.json")
}

fn display_config_path(state: &AppState) -> PathBuf {
    state.data_dir.join("output_display_config.json")
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

/// The saved congregation-output appearance (or defaults on a fresh machine).
#[tauri::command]
pub fn output_appearance(state: State<'_, AppState>) -> OutputAppearance {
    std::fs::read_to_string(appearance_path(&state))
        .ok()
        .and_then(|s| serde_json::from_str::<OutputAppearance>(&s).ok())
        .unwrap_or_default()
        .sanitized()
}

/// Persist the output appearance. The output windows pick this up live by
/// listening for the `ss://appearance` event the operator UI emits on save.
#[tauri::command]
pub fn output_set_appearance(
    state: State<'_, AppState>,
    appearance: OutputAppearance,
) -> AppResult<OutputAppearance> {
    let clean = appearance.sanitized();
    let s =
        serde_json::to_string_pretty(&clean).map_err(|e| AppError::Validation(e.to_string()))?;
    std::fs::write(appearance_path(&state), s).map_err(|e| AppError::Validation(e.to_string()))?;
    Ok(clean)
}

/// The saved output display configuration (resolution, safe-zone, transitions)
/// or sensible defaults on a fresh machine.
#[tauri::command]
pub fn output_display_config(state: State<'_, AppState>) -> OutputDisplayConfig {
    std::fs::read_to_string(display_config_path(&state))
        .ok()
        .and_then(|s| serde_json::from_str::<OutputDisplayConfig>(&s).ok())
        .unwrap_or_default()
        .sanitized()
}

/// Persist the output display configuration.
#[tauri::command]
pub fn output_set_display_config(
    state: State<'_, AppState>,
    config: OutputDisplayConfig,
) -> AppResult<OutputDisplayConfig> {
    let clean = config.sanitized();
    let s =
        serde_json::to_string_pretty(&clean).map_err(|e| AppError::Validation(e.to_string()))?;
    std::fs::write(display_config_path(&state), s)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    Ok(clean)
}
