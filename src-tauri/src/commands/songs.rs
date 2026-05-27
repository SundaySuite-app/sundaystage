//! Tauri commands for the Song aggregate.

use tauri::State;

use crate::db::models::{SearchResult, Song, SongInput, SongSection};
use crate::db::repositories::SongRepo;
use crate::error::AppResult;
use crate::AppState;

#[tauri::command]
pub async fn song_create(
    state: State<'_, AppState>,
    input: SongInput,
) -> AppResult<Song> {
    SongRepo::new(&state.db.pool).create(input).await
}

#[tauri::command]
pub async fn song_get(state: State<'_, AppState>, id: String) -> AppResult<Song> {
    SongRepo::new(&state.db.pool).get(&id).await
}

#[tauri::command]
pub async fn song_list(
    state: State<'_, AppState>,
    library_id: String,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<Song>> {
    SongRepo::new(&state.db.pool)
        .list(&library_id, limit.unwrap_or(100), offset.unwrap_or(0))
        .await
}

#[tauri::command]
pub async fn song_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    SongRepo::new(&state.db.pool).soft_delete(&id).await
}

#[tauri::command]
pub async fn song_search(
    state: State<'_, AppState>,
    library_id: String,
    query: String,
    limit: Option<i64>,
) -> AppResult<Vec<SearchResult>> {
    SongRepo::new(&state.db.pool)
        .search(&library_id, &query, limit.unwrap_or(50))
        .await
}

#[tauri::command]
pub async fn song_sections(
    state: State<'_, AppState>,
    song_id: String,
) -> AppResult<Vec<SongSection>> {
    SongRepo::new(&state.db.pool).sections(&song_id).await
}

#[tauri::command]
pub async fn song_add_section(
    state: State<'_, AppState>,
    song_id: String,
    label: String,
    lyrics: String,
) -> AppResult<SongSection> {
    SongRepo::new(&state.db.pool)
        .add_section(&song_id, &label, &lyrics)
        .await
}
