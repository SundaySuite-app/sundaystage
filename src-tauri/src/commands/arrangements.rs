//! Tauri commands for song arrangements (Phase 3.3).

use tauri::State;

use crate::db::models::{ArrangementItem, SongArrangement, SongSection};
use crate::db::repositories::ArrangementRepo;
use crate::error::AppResult;
use crate::AppState;

#[tauri::command]
pub async fn arrangement_create(
    state: State<'_, AppState>,
    song_id: String,
    name: String,
) -> AppResult<SongArrangement> {
    ArrangementRepo::new(&state.db.pool).create(&song_id, &name).await
}

#[tauri::command]
pub async fn arrangement_list(
    state: State<'_, AppState>,
    song_id: String,
) -> AppResult<Vec<SongArrangement>> {
    ArrangementRepo::new(&state.db.pool).list(&song_id).await
}

#[tauri::command]
pub async fn arrangement_rename(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> AppResult<SongArrangement> {
    ArrangementRepo::new(&state.db.pool).rename(&id, &name).await
}

#[tauri::command]
pub async fn arrangement_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    ArrangementRepo::new(&state.db.pool).delete(&id).await
}

#[tauri::command]
pub async fn arrangement_set_default(
    state: State<'_, AppState>,
    song_id: String,
    arrangement_id: String,
) -> AppResult<()> {
    ArrangementRepo::new(&state.db.pool)
        .set_default(&song_id, &arrangement_id)
        .await
}

#[tauri::command]
pub async fn arrangement_duplicate(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<SongArrangement> {
    ArrangementRepo::new(&state.db.pool).duplicate(&id).await
}

#[tauri::command]
pub async fn arrangement_items(
    state: State<'_, AppState>,
    arrangement_id: String,
) -> AppResult<Vec<ArrangementItem>> {
    ArrangementRepo::new(&state.db.pool).items(&arrangement_id).await
}

#[tauri::command]
pub async fn arrangement_set_items(
    state: State<'_, AppState>,
    arrangement_id: String,
    section_ids: Vec<String>,
) -> AppResult<Vec<ArrangementItem>> {
    ArrangementRepo::new(&state.db.pool)
        .set_items(&arrangement_id, &section_ids)
        .await
}

#[tauri::command]
pub async fn arrangement_sections(
    state: State<'_, AppState>,
    arrangement_id: String,
) -> AppResult<Vec<SongSection>> {
    ArrangementRepo::new(&state.db.pool)
        .resolved_sections(&arrangement_id)
        .await
}
