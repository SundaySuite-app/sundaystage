//! Tauri commands for themes, templates, and the cascade defaults (Phase 3.2).

use std::collections::HashMap;

use tauri::State;

use crate::db::models::{Library, Slide, Template, Theme};
use crate::db::repositories::{DeckRepo, ThemeRepo};
use crate::error::AppResult;
use crate::services::slide_doc::SlideDoc;
use crate::services::theme::ThemeTokens;
use crate::AppState;

#[tauri::command]
pub async fn theme_list(state: State<'_, AppState>, library_id: String) -> AppResult<Vec<Theme>> {
    ThemeRepo::new(&state.db.pool)
        .list_themes(&library_id)
        .await
}

#[tauri::command]
pub async fn template_list(
    state: State<'_, AppState>,
    library_id: String,
) -> AppResult<Vec<Template>> {
    ThemeRepo::new(&state.db.pool)
        .list_templates(&library_id)
        .await
}

#[tauri::command]
pub async fn theme_create(
    state: State<'_, AppState>,
    library_id: String,
    name: String,
    tokens: ThemeTokens,
) -> AppResult<Theme> {
    ThemeRepo::new(&state.db.pool)
        .create_theme(&library_id, &name, &tokens)
        .await
}

#[tauri::command]
pub async fn theme_duplicate(
    state: State<'_, AppState>,
    source_id: String,
    library_id: String,
) -> AppResult<Theme> {
    ThemeRepo::new(&state.db.pool)
        .duplicate_theme(&source_id, &library_id)
        .await
}

#[tauri::command]
pub async fn theme_update_tokens(
    state: State<'_, AppState>,
    id: String,
    tokens: ThemeTokens,
) -> AppResult<Theme> {
    ThemeRepo::new(&state.db.pool)
        .update_theme_tokens(&id, &tokens)
        .await
}

#[tauri::command]
pub async fn theme_rename(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> AppResult<Theme> {
    ThemeRepo::new(&state.db.pool)
        .rename_theme(&id, &name)
        .await
}

#[tauri::command]
pub async fn theme_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    ThemeRepo::new(&state.db.pool).delete_theme(&id).await
}

#[tauri::command]
pub async fn library_set_default_theme(
    state: State<'_, AppState>,
    library_id: String,
    theme_id: Option<String>,
) -> AppResult<Library> {
    ThemeRepo::new(&state.db.pool)
        .set_library_default_theme(&library_id, theme_id.as_deref())
        .await
}

#[tauri::command]
pub async fn library_set_default_template(
    state: State<'_, AppState>,
    library_id: String,
    template_id: Option<String>,
) -> AppResult<Library> {
    ThemeRepo::new(&state.db.pool)
        .set_library_default_template(&library_id, template_id.as_deref())
        .await
}

#[tauri::command]
pub async fn slide_set_theme(
    state: State<'_, AppState>,
    id: String,
    theme_id: Option<String>,
) -> AppResult<Slide> {
    DeckRepo::new(&state.db.pool)
        .set_slide_theme(&id, theme_id.as_deref())
        .await
}

#[tauri::command]
pub async fn slide_set_template(
    state: State<'_, AppState>,
    id: String,
    template_id: Option<String>,
) -> AppResult<Slide> {
    DeckRepo::new(&state.db.pool)
        .set_slide_template(&id, template_id.as_deref())
        .await
}

#[tauri::command]
pub async fn template_render(
    state: State<'_, AppState>,
    library_id: String,
    template_id: String,
    theme_id: String,
    slot_text: HashMap<String, String>,
) -> AppResult<SlideDoc> {
    ThemeRepo::new(&state.db.pool)
        .render(&library_id, &template_id, &theme_id, &slot_text)
        .await
}
