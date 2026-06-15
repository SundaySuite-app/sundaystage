//! Tauri commands for Service + ServiceItem.

use std::collections::HashMap;

use tauri::State;

use crate::db::models::{Service, ServiceItem, ServiceItemSong};
use crate::db::repositories::ServiceRepo;
use crate::error::{AppError, AppResult};
use crate::services::cue_list::{CueCompiler, CueSummary};
use crate::services::sundayplan::{self, PlanImportResult};
use crate::AppState;

#[tauri::command]
pub async fn service_create(
    state: State<'_, AppState>,
    library_id: String,
    name: String,
    starts_at: i64,
) -> AppResult<Service> {
    ServiceRepo::new(&state.db.pool)
        .create(&library_id, &name, starts_at)
        .await
}

#[tauri::command]
pub async fn service_get(state: State<'_, AppState>, id: String) -> AppResult<Service> {
    ServiceRepo::new(&state.db.pool).get(&id).await
}

#[tauri::command]
pub async fn service_upcoming(
    state: State<'_, AppState>,
    library_id: String,
    from: Option<i64>,
    limit: Option<i64>,
) -> AppResult<Vec<Service>> {
    ServiceRepo::new(&state.db.pool)
        .upcoming(&library_id, from.unwrap_or(0), limit.unwrap_or(20))
        .await
}

#[tauri::command]
pub async fn service_items(
    state: State<'_, AppState>,
    service_id: String,
) -> AppResult<Vec<ServiceItem>> {
    ServiceRepo::new(&state.db.pool).items(&service_id).await
}

/// The song behind each *song* service item, keyed by `service_item.id`. Feeds
/// the live → SundaySong usage bridge at "Go Live" so it can report which song
/// each cue actually shows. Non-song items are absent from the map.
#[tauri::command]
pub async fn songs_by_item(
    state: State<'_, AppState>,
    service_id: String,
) -> AppResult<HashMap<String, ServiceItemSong>> {
    ServiceRepo::new(&state.db.pool)
        .get_songs_by_item(&service_id)
        .await
}

#[tauri::command]
pub async fn service_rename(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> AppResult<Service> {
    ServiceRepo::new(&state.db.pool).rename(&id, &name).await
}

#[tauri::command]
pub async fn service_set_notes(
    state: State<'_, AppState>,
    id: String,
    notes: String,
) -> AppResult<Service> {
    ServiceRepo::new(&state.db.pool)
        .set_notes(&id, &notes)
        .await
}

/// Set (or clear) the live translation overlay's target language (Phase 11.2).
/// An empty / blank string clears it. An unsupported language is rejected so
/// the operator gets immediate feedback rather than a silently-ignored setting.
#[tauri::command]
pub async fn service_set_secondary_language(
    state: State<'_, AppState>,
    id: String,
    language: Option<String>,
) -> AppResult<Service> {
    let lang = language
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty());
    if let Some(ref l) = lang {
        if !crate::services::ai::translate::is_supported_target(l) {
            return Err(AppError::Validation(format!(
                "Språk '{l}' støttes ikke for oversettelse."
            )));
        }
    }
    ServiceRepo::new(&state.db.pool)
        .set_secondary_language(&id, lang.as_deref())
        .await
}

#[tauri::command]
pub async fn service_set_starts_at(
    state: State<'_, AppState>,
    id: String,
    starts_at: i64,
) -> AppResult<Service> {
    ServiceRepo::new(&state.db.pool)
        .set_starts_at(&id, starts_at)
        .await
}

#[tauri::command]
pub async fn service_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    ServiceRepo::new(&state.db.pool).soft_delete(&id).await
}

/// Append a song to the service queue. `arrangement_id` is optional (falls back
/// to the song's section order at compile time); `key_override` transposes.
#[tauri::command]
pub async fn service_add_song(
    state: State<'_, AppState>,
    service_id: String,
    song_id: String,
    arrangement_id: Option<String>,
    key_override: Option<String>,
) -> AppResult<ServiceItem> {
    let repo = ServiceRepo::new(&state.db.pool);
    let position = repo.next_position(&service_id).await?;
    repo.add_item(
        &service_id,
        position,
        "song",
        Some(&song_id),
        arrangement_id.as_deref(),
        key_override.as_deref(),
        None,
        None,
    )
    .await
}

#[tauri::command]
pub async fn service_remove_item(state: State<'_, AppState>, item_id: String) -> AppResult<()> {
    ServiceRepo::new(&state.db.pool).remove_item(&item_id).await
}

/// Update a song item's arrangement / key / notes in place. Each field is set
/// to exactly the value given (null clears it).
#[tauri::command]
pub async fn service_update_item(
    state: State<'_, AppState>,
    item_id: String,
    arrangement_id: Option<String>,
    key_override: Option<String>,
    notes: Option<String>,
) -> AppResult<ServiceItem> {
    ServiceRepo::new(&state.db.pool)
        .update_item(
            &item_id,
            arrangement_id.as_deref(),
            key_override.as_deref(),
            notes.as_deref(),
        )
        .await
}

/// Append a non-song item to the queue (a pause/announcement/video). Songs go
/// through `service_add_song`; scripture/decks need their own ids.
#[tauri::command]
pub async fn service_add_item(
    state: State<'_, AppState>,
    service_id: String,
    kind: String,
    label: Option<String>,
) -> AppResult<ServiceItem> {
    if !matches!(kind.as_str(), "gap" | "announcement" | "video") {
        return Err(crate::error::AppError::Validation(format!(
            "service_add_item støtter kun gap/announcement/video, ikke '{kind}'"
        )));
    }
    let repo = ServiceRepo::new(&state.db.pool);
    let position = repo.next_position(&service_id).await?;
    repo.add_item(
        &service_id,
        position,
        &kind,
        None,
        None,
        None,
        None,
        label.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn service_reorder_items(
    state: State<'_, AppState>,
    service_id: String,
    ordered_ids: Vec<String>,
) -> AppResult<Vec<ServiceItem>> {
    ServiceRepo::new(&state.db.pool)
        .reorder_items(&service_id, &ordered_ids)
        .await
}

/// Per-item / per-section breakdown of the cues this service will produce —
/// powers the queue editor's "what goes into the queue" view.
#[tauri::command]
pub async fn service_cue_summary(
    state: State<'_, AppState>,
    service_id: String,
) -> AppResult<CueSummary> {
    CueCompiler::new(&state.db.pool)
        .summarize(&service_id)
        .await
}

/// Import a SundayPlan plan (JSON) into a new service: songs are matched to the
/// library by title, unmatched titles become stubs, scripture lands as a
/// placeholder. Returns the new service plus what needs follow-up.
#[tauri::command]
pub async fn service_import_sundayplan(
    state: State<'_, AppState>,
    library_id: String,
    json: String,
) -> AppResult<PlanImportResult> {
    sundayplan::import_plan(&state.db.pool, &library_id, &json).await
}
