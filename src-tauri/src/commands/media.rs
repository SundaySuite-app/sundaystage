//! Tauri commands for media assets (Phase 7.2).
//!
//! Import fingerprints the file (no ffmpeg needed); listing reports whether
//! each asset's path still exists; relink searches caller-supplied folders for
//! a file with the same fingerprint and repoints the asset. Thumbnail/probe
//! enrichment is a follow-up (needs ffmpeg).

use std::path::{Path, PathBuf};

use tauri::State;

use crate::db::models::MediaAsset;
use crate::db::repositories::MediaRepo;
use crate::error::{AppError, AppResult};
use crate::services::media::{content_fingerprint, detect_kind, find_by_fingerprint, MediaStatus};
use crate::AppState;

#[tauri::command]
pub async fn media_import(
    state: State<'_, AppState>,
    library_id: String,
    path: String,
) -> AppResult<MediaAsset> {
    let p = Path::new(&path);
    let kind =
        detect_kind(p).ok_or_else(|| AppError::Validation(format!("ustøttet filtype: {path}")))?;
    let fingerprint = content_fingerprint(p)?; // io::Error → AppError::Io
    MediaRepo::new(&state.db.pool)
        .import(&library_id, kind, &path, &fingerprint)
        .await
}

#[tauri::command]
pub async fn media_list(
    state: State<'_, AppState>,
    library_id: String,
) -> AppResult<Vec<MediaStatus>> {
    let assets = MediaRepo::new(&state.db.pool).list(&library_id).await?;
    Ok(assets
        .into_iter()
        .map(|asset| {
            let present = Path::new(&asset.original_path).exists();
            MediaStatus { asset, present }
        })
        .collect())
}

#[tauri::command]
pub async fn media_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    MediaRepo::new(&state.db.pool).delete(&id).await
}

/// Try to relink a moved asset: search `search_dirs` for a file whose
/// fingerprint matches the asset's stored hash. Returns the relinked asset, or
/// `None` if no match was found.
#[tauri::command]
pub async fn media_relink(
    state: State<'_, AppState>,
    id: String,
    search_dirs: Vec<String>,
) -> AppResult<Option<MediaAsset>> {
    let repo = MediaRepo::new(&state.db.pool);
    let asset = repo.get(&id).await?;
    let dirs: Vec<PathBuf> = search_dirs.into_iter().map(PathBuf::from).collect();
    match find_by_fingerprint(&asset.content_hash, &dirs) {
        Some(found) => {
            let relinked = repo.relink(&id, &found.to_string_lossy()).await?;
            Ok(Some(relinked))
        }
        None => Ok(None),
    }
}
