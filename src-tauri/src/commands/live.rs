//! Live-engine commands.
//!
//! Phase 5.1: compile a Service into a CueList. The actual cue-advance
//! state machine + output-process IPC lands in Phase 5.2.

use tauri::State;

use crate::services::cue_list::{CueCompiler, CueList};
use crate::error::AppResult;
use crate::AppState;

#[tauri::command]
pub async fn live_compile_cue_list(
    state: State<'_, AppState>,
    service_id: String,
) -> AppResult<CueList> {
    CueCompiler::new(&state.db.pool).compile(&service_id).await
}
