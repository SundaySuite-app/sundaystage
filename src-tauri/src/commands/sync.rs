//! Tauri commands for cloud sync status (Phase 9).
//!
//! Cloud sync isn't wired to a backend yet (free tier is fully local), so this
//! reports `local_only` — except it still honors the live-suspends-sync rule by
//! reading the running session, so the indicator is correct once a Supabase
//! backend is connected.

use tauri::State;

use crate::error::AppResult;
use crate::services::sync::{compute_status, SyncStatus};
use crate::AppState;

/// Whether a cloud backend is configured. Wired to real config when Phase 9's
/// Supabase transport lands.
const CLOUD_ENABLED: bool = false;

#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> AppResult<SyncStatus> {
    let is_live = state.live.lock().expect("live mutex").is_some();
    Ok(compute_status(CLOUD_ENABLED, true, is_live, 0, 0))
}
