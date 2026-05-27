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
