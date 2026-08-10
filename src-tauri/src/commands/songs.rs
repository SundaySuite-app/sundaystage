//! Tauri commands for the Song aggregate.

use tauri::State;

use crate::db::models::{SearchResult, Song, SongInput, SongSection};
use crate::db::repositories::{ArrangementRepo, SongRepo};
use crate::error::{AppError, AppResult};
use crate::services::song_export::{to_chordpro, to_openlyrics};
use crate::telemetry::counters::CounterName;
use crate::telemetry::quality::LiveSafe;
use crate::AppState;

#[tauri::command]
pub async fn song_create(state: State<'_, AppState>, input: SongInput) -> AppResult<Song> {
    let song = SongRepo::new(&state.db.pool).create(input).await?;
    // E5 — on success only. A rejected title is not a song.
    state.telemetry.note_counter(CounterName::EditorSongCreated);
    Ok(song)
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

#[tauri::command]
pub async fn song_update_section(
    state: State<'_, AppState>,
    id: String,
    label: String,
    lyrics: String,
) -> AppResult<SongSection> {
    SongRepo::new(&state.db.pool)
        .update_section(&id, &label, &lyrics)
        .await
}

#[tauri::command]
pub async fn song_delete_section(state: State<'_, AppState>, id: String) -> AppResult<()> {
    SongRepo::new(&state.db.pool).delete_section(&id).await
}

#[tauri::command]
pub async fn song_reorder_sections(
    state: State<'_, AppState>,
    song_id: String,
    ordered_ids: Vec<String>,
) -> AppResult<Vec<SongSection>> {
    SongRepo::new(&state.db.pool)
        .reorder_sections(&song_id, &ordered_ids)
        .await
}

/// Export a song to an open interchange format (Spor B5 — the lock-in fix).
///
/// `format` is `"openlyrics"` or `"chordpro"`. The song's sections and its
/// default arrangement (the play order) are serialised by the pure
/// [`crate::services::song_export`] writers; the string is returned for the
/// frontend to show/copy/save, mirroring how `bridge_export_srt` surfaces.
#[tauri::command]
pub async fn export_song(
    state: State<'_, AppState>,
    song_id: String,
    format: String,
) -> AppResult<String> {
    let pool = &state.db.pool;
    let song_repo = SongRepo::new(pool);
    let song = song_repo.get(&song_id).await?;
    let sections = song_repo.sections(&song_id).await?;

    // `ArrangementRepo::list` sorts default-first, so the first arrangement is
    // the play order to export; a song with none exports its sections in order.
    let arr_repo = ArrangementRepo::new(pool);
    let arrangement: Vec<String> = match arr_repo.list(&song_id).await?.first() {
        Some(a) => arr_repo
            .resolved_sections(&a.id)
            .await?
            .into_iter()
            .map(|s| s.label)
            .collect(),
        None => Vec::new(),
    };

    match format.as_str() {
        "openlyrics" => Ok(to_openlyrics(&song, &sections, &arrangement)),
        "chordpro" => Ok(to_chordpro(&song, &sections, &arrangement)),
        other => Err(AppError::Validation(format!(
            "ukjent eksportformat «{other}»"
        ))),
    }
}
