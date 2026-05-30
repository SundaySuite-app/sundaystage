//! Phase 2.2 — song import command.
//!
//! Takes a song file's *text content* (the JS side reads it with a standard
//! `<input type="file">` + `FileReader`, so no native file-dialog plugin is
//! needed), detects the format, parses it to a structured song via the pure
//! [`crate::services::song_import`] parsers, creates the song in `library_id`
//! and applies its sections + arrangement through the shared
//! [`apply_formatted_song`] path. Returns a summary for the UI.

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::db::models::SongInput;
use crate::db::repositories::SongRepo;
use crate::error::AppResult;
use crate::services::ai::lyric_format::apply_formatted_song;
use crate::services::song_import::{import_song, ImportFormat};
use crate::AppState;

/// Outcome of importing one song file.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/ImportResult.ts")]
pub struct ImportResult {
    pub song_id: String,
    pub title: String,
    pub format: ImportFormat,
    pub section_count: u32,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn import_song_file(
    state: State<'_, AppState>,
    library_id: String,
    filename: String,
    content: String,
) -> AppResult<ImportResult> {
    let (format, formatted) = import_song(&filename, &content);

    let title = formatted
        .title_suggestion
        .clone()
        .unwrap_or_else(|| filename_stem(&filename));

    let song = SongRepo::new(&state.db.pool)
        .create(SongInput {
            library_id,
            title: title.clone(),
            language: Some(formatted.language.clone()),
            default_key: None,
            tempo_bpm: None,
            ccli_song_id: None,
            tono_work_id: None,
            copyright_notice: None,
        })
        .await?;

    apply_formatted_song(&state.db.pool, &song.id, &formatted).await?;

    Ok(ImportResult {
        song_id: song.id,
        title,
        format,
        section_count: formatted.sections.len() as u32,
        warnings: formatted.warnings,
    })
}

/// Derive a human title from a filename: drop the path and extension, turn
/// `_`/`-` into spaces. Used when the file carries no title of its own.
fn filename_stem(filename: &str) -> String {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base);
    let cleaned = stem.replace(['_', '-'], " ");
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "Importert sang".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_stem_strips_path_ext_and_separators() {
        assert_eq!(filename_stem("/songs/amazing_grace.cho"), "amazing grace");
        assert_eq!(filename_stem("Be-Thou-My-Vision.xml"), "Be Thou My Vision");
        assert_eq!(filename_stem("song"), "song");
        assert_eq!(filename_stem(".txt"), "Importert sang");
    }
}
