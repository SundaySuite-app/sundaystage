//! Tauri commands for custom decks and their slides (Phase 3.1).
//!
//! The slide editor is the primary caller. Commands accept/return the typed
//! [`SlideDoc`] so the frontend never hand-builds slide JSON.

use tauri::State;

use crate::db::models::{CustomDeck, Slide};
use crate::db::repositories::DeckRepo;
use crate::error::AppResult;
use crate::services::slide_doc::SlideDoc;
use crate::AppState;

#[tauri::command]
pub async fn deck_create(
    state: State<'_, AppState>,
    library_id: String,
    name: String,
) -> AppResult<CustomDeck> {
    DeckRepo::new(&state.db.pool)
        .create_deck(&library_id, &name)
        .await
}

#[tauri::command]
pub async fn deck_get(state: State<'_, AppState>, id: String) -> AppResult<CustomDeck> {
    DeckRepo::new(&state.db.pool).get_deck(&id).await
}

#[tauri::command]
pub async fn deck_list(
    state: State<'_, AppState>,
    library_id: String,
) -> AppResult<Vec<CustomDeck>> {
    DeckRepo::new(&state.db.pool).list_decks(&library_id).await
}

#[tauri::command]
pub async fn deck_rename(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> AppResult<CustomDeck> {
    DeckRepo::new(&state.db.pool).rename_deck(&id, &name).await
}

#[tauri::command]
pub async fn deck_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    DeckRepo::new(&state.db.pool).delete_deck(&id).await
}

#[tauri::command]
pub async fn slide_create(
    state: State<'_, AppState>,
    deck_id: String,
    doc: SlideDoc,
) -> AppResult<Slide> {
    DeckRepo::new(&state.db.pool)
        .create_slide(&deck_id, &doc)
        .await
}

#[tauri::command]
pub async fn slide_list(state: State<'_, AppState>, deck_id: String) -> AppResult<Vec<Slide>> {
    DeckRepo::new(&state.db.pool).list_slides(&deck_id).await
}

#[tauri::command]
pub async fn slide_update_content(
    state: State<'_, AppState>,
    id: String,
    doc: SlideDoc,
) -> AppResult<Slide> {
    DeckRepo::new(&state.db.pool)
        .update_slide_content(&id, &doc)
        .await
}

#[tauri::command]
pub async fn slide_duplicate(state: State<'_, AppState>, id: String) -> AppResult<Slide> {
    DeckRepo::new(&state.db.pool).duplicate_slide(&id).await
}

#[tauri::command]
pub async fn slide_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    DeckRepo::new(&state.db.pool).delete_slide(&id).await
}

#[tauri::command]
pub async fn slide_reorder(
    state: State<'_, AppState>,
    deck_id: String,
    ordered_ids: Vec<String>,
) -> AppResult<Vec<Slide>> {
    DeckRepo::new(&state.db.pool)
        .reorder_slides(&deck_id, &ordered_ids)
        .await
}
