//! Tauri command: publish the local song library to the cloud (one-way).
//! **NETWORK-UNVERIFIED** — see services::library_publish. Requires a Sunday
//! login (performed in SundayRec; this app reads/refreshes the shared session).

use tauri::State;

use crate::error::AppResult;
use crate::services::library_publish::{publish_library, PublishResult};
use crate::telemetry::counters::CounterName;
use crate::telemetry::quality::LiveSafe;
use crate::AppState;

#[tauri::command]
pub async fn library_publish(
    state: State<'_, AppState>,
    library_id: String,
) -> AppResult<PublishResult> {
    // E3 — counted on ATTEMPT, not on success: "how often is this used" is the
    // question a counter can honestly answer, and a publish that failed was
    // still a publish someone tried to do.
    state.telemetry.note_counter(CounterName::LibraryPublishRun);
    publish_library(&state.db.pool, &library_id).await
}
