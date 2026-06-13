//! Phase 5.2 — output display commands.
//!
//! Enumerate monitors, persist the operator's role assignments, and open/close
//! the borderless full-screen output windows. The window glue lives in
//! [`crate::output::window`]; the assignment logic in
//! [`crate::services::display`].

use std::path::PathBuf;
use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::output::{process, window};
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
    (
        monitors,
        OutputConfig {
            assignments,
            ..saved
        },
    )
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

/// Open the live outputs for the resolved assignments.
///
/// Default path (Phase 5.2, `process_isolation: true`): spawn one supervised
/// `sundaystage-output` **process** per assigned display — if the operator UI
/// crashes, the projector keeps the slide. Falls back to the in-process
/// webview windows when isolation is disabled in the saved config or the
/// output binary isn't available (e.g. plain `tauri dev` without building it),
/// so "Go Live" never fails on a Sunday because of a missing sidecar.
#[tauri::command]
pub async fn output_open(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let (monitors, cfg) = resolve(&app, &state);
    save_config(&state, &cfg);

    if cfg.process_isolation {
        if let Some(binary) = process::output_binary_path() {
            let appearance_file = appearance_path(&state);
            let specs: Vec<process::OutputSpec> = cfg
                .assignments
                .iter()
                .filter_map(|a| {
                    let label = window::output_label(a.role, a.monitor_index)?;
                    let m = monitors.iter().find(|m| m.index == a.monitor_index)?;
                    Some(process::OutputSpec {
                        label,
                        x: m.x,
                        y: m.y,
                        width: m.width,
                        height: m.height,
                        headless: false,
                        appearance_file: Some(appearance_file.clone()),
                    })
                })
                .collect();
            if specs.is_empty() {
                return Ok(());
            }
            // Replace any previous supervisor (re-apply after a role change).
            // Take it out of the lock first — the guard must not live across
            // the await.
            let old = state.outputs.lock().expect("outputs mutex").take();
            if let Some(old) = old {
                old.shutdown().await;
            }
            // Make sure no in-process windows linger from a fallback run.
            window::close_outputs(&app);
            let supervisor = process::OutputSupervisor::start(binary, specs);
            // A service may already be live (outputs opened mid-service) —
            // seed the current frame so the first paint is correct.
            if let Some(frame) = state
                .live
                .lock()
                .expect("live mutex")
                .as_ref()
                .map(|s| s.current_frame())
            {
                supervisor.render(frame);
            }
            *state.outputs.lock().expect("outputs mutex") = Some(supervisor);
            return Ok(());
        }
        tracing::warn!(
            "sundaystage-output binary not found — falling back to in-process output windows"
        );
    }
    window::open_outputs(&app, &monitors, &cfg.assignments).map_err(AppError::Validation)
}

/// Close all live outputs (isolated processes and/or in-process windows).
#[tauri::command]
pub async fn output_close(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let supervisor = state.outputs.lock().expect("outputs mutex").take();
    if let Some(supervisor) = supervisor {
        supervisor.shutdown().await;
    }
    window::close_outputs(&app);
    Ok(())
}

/// Whether any live output (process or window) is currently driving a screen.
#[tauri::command]
pub fn output_is_open(app: AppHandle, state: State<'_, AppState>) -> bool {
    let supervised = state
        .outputs
        .lock()
        .expect("outputs mutex")
        .as_ref()
        .is_some_and(|s| s.is_running());
    supervised || window::outputs_open(&app)
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
