//! Live-engine commands.
//!
//! Phase 5.1: compile a Service into a CueList.
//! Phase 5.3: drive the running [`LiveSession`] — start, dispatch operator
//! actions, snapshot, end. The session is held in `AppState` behind a mutex and
//! persisted to disk after every action for crash recovery (the Phase 5.2
//! output process independently holds the last frame if the UI dies).

use tauri::State;

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::services::cue_list::{CueCompiler, CueList};
use crate::services::live_session::{LiveAction, LiveSession, LiveSessionView};
use crate::services::session_store::SessionStore;
use crate::services::stage_display::{builtin_stage_presets, StageDisplayConfig};
use crate::services::sundayrec_bridge::export::{chapter_markers, session_to_srt, ChapterMarker};
use crate::services::sundayrec_bridge::protocol::PROTOCOL_VERSION;
use crate::AppState;

/// Built-in stage-display presets (Phase 8).
#[tauri::command]
pub fn stage_presets() -> Vec<StageDisplayConfig> {
    builtin_stage_presets()
}

/// The bridge protocol version SundayStage speaks (Phase 10.1).
#[tauri::command]
pub fn bridge_protocol_version() -> String {
    PROTOCOL_VERSION.to_string()
}

fn require_session<T>(state: &AppState, f: impl FnOnce(&LiveSession) -> T) -> AppResult<T> {
    let guard = state.live.lock().expect("live mutex");
    let session = guard
        .as_ref()
        .ok_or_else(|| AppError::Validation("ingen aktiv live-sesjon".into()))?;
    Ok(f(session))
}

/// Chapter markers for the recording timeline, from the current session log
/// (Phase 10.2).
#[tauri::command]
pub fn bridge_chapter_markers(state: State<'_, AppState>) -> AppResult<Vec<ChapterMarker>> {
    require_session(&state, chapter_markers)
}

/// SRT captions matching the recording timeline (Phase 10.2). `ended_at`
/// defaults to now if the recording is still running.
#[tauri::command]
pub fn bridge_export_srt(state: State<'_, AppState>, ended_at: Option<i64>) -> AppResult<String> {
    let end = ended_at.unwrap_or_else(now_ms);
    require_session(&state, |s| session_to_srt(s, end))
}

#[tauri::command]
pub async fn live_compile_cue_list(
    state: State<'_, AppState>,
    service_id: String,
) -> AppResult<CueList> {
    CueCompiler::new(&state.db.pool).compile(&service_id).await
}

fn store(state: &AppState) -> SessionStore {
    SessionStore::in_dir(&state.data_dir)
}

/// Compile the service and start a live session (replacing any previous one).
#[tauri::command]
pub async fn live_start(
    state: State<'_, AppState>,
    service_id: String,
) -> AppResult<LiveSessionView> {
    // Compile first (async, no lock held), then install the session.
    let cue_list = CueCompiler::new(&state.db.pool)
        .compile(&service_id)
        .await?;
    let session = LiveSession::new(service_id, cue_list, now_ms());
    let view = session.view();
    // Best-effort WAL; a failed write must never block going live.
    let _ = store(&state).begin(&session);
    *state.live.lock().expect("live mutex") = Some(session);
    Ok(view)
}

/// Apply one operator action to the running session.
#[tauri::command]
pub fn live_dispatch(state: State<'_, AppState>, action: LiveAction) -> AppResult<LiveSessionView> {
    let mut guard = state.live.lock().expect("live mutex");
    let session = guard
        .as_mut()
        .ok_or_else(|| AppError::Validation("ingen aktiv live-sesjon".into()))?;
    // Log the action before applying it; a failed append must not break the
    // show (worst case: recovery loses the last action).
    let _ = store(&state).record(&action);
    session.dispatch(action, now_ms());
    Ok(session.view())
}

/// Snapshot of the current session, or `None` if not live.
#[tauri::command]
pub fn live_state(state: State<'_, AppState>) -> AppResult<Option<LiveSessionView>> {
    Ok(state
        .live
        .lock()
        .expect("live mutex")
        .as_ref()
        .map(|s| s.view()))
}

/// End the session and clear the recovery log (marks a clean shutdown).
#[tauri::command]
pub fn live_end(state: State<'_, AppState>) -> AppResult<()> {
    *state.live.lock().expect("live mutex") = None;
    store(&state).clear();
    Ok(())
}

/// On launch, recover an abnormally-terminated session if one exists. Installs
/// it as the active session and returns its view so the UI can offer "resume".
#[tauri::command]
pub fn live_recover(state: State<'_, AppState>) -> AppResult<Option<LiveSessionView>> {
    let Some(session) = store(&state).recover() else {
        return Ok(None);
    };
    let view = session.view();
    *state.live.lock().expect("live mutex") = Some(session);
    Ok(Some(view))
}
