//! Tauri commands for Service + ServiceItem.

use tauri::State;

use crate::db::models::{Service, ServiceItem};
use crate::db::repositories::ServiceRepo;
use crate::error::AppResult;
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

#[tauri::command]
pub async fn service_rename(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> AppResult<Service> {
    ServiceRepo::new(&state.db.pool).rename(&id, &name).await
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
