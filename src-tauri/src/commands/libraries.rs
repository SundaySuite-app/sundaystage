//! Tauri commands for the Library aggregate.

use tauri::State;

use crate::db::models::{Library, LibraryInput};
use crate::db::repositories::LibraryRepo;
use crate::error::AppResult;
use crate::AppState;

#[tauri::command]
pub async fn library_create(
    state: State<'_, AppState>,
    input: LibraryInput,
) -> AppResult<Library> {
    LibraryRepo::new(&state.db.pool).create(input).await
}

#[tauri::command]
pub async fn library_get(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Library> {
    LibraryRepo::new(&state.db.pool).get(&id).await
}

#[tauri::command]
pub async fn library_list(state: State<'_, AppState>) -> AppResult<Vec<Library>> {
    LibraryRepo::new(&state.db.pool).list().await
}

#[tauri::command]
pub async fn library_rename(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> AppResult<Library> {
    LibraryRepo::new(&state.db.pool).rename(&id, &name).await
}
