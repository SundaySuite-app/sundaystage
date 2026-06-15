//! Tauri command: publish the local song library to the cloud (one-way).
//! **NETWORK-UNVERIFIED** — see services::library_publish. Requires a Sunday
//! login (performed in SundayRec; this app reads/refreshes the shared session).

use tauri::State;

use crate::error::AppResult;
use crate::services::library_publish::{publish_library, PublishResult};
use crate::AppState;

#[tauri::command]
pub async fn library_publish(
    state: State<'_, AppState>,
    library_id: String,
) -> AppResult<PublishResult> {
    publish_library(&state.db.pool, &library_id).await
}
