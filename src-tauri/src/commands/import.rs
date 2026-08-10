//! Phase 2.2 — song import command.
//!
//! Takes a song file's *text content* (the JS side reads it with a standard
//! `<input type="file">` + `FileReader`, so no native file-dialog plugin is
//! needed), detects the format, parses it to a structured song via the pure
//! [`crate::services::song_import`] parsers, creates the song in `library_id`
//! and applies its sections + arrangement through the shared
//! [`apply_formatted_song`] path. Returns a summary for the UI.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::db::models::SongInput;
use crate::db::repositories::SongRepo;
use crate::error::AppResult;
use crate::services::ai::lyric_format::apply_formatted_song;
use crate::services::import_easyworship::{convert_song, locate_databases, read_songs};
use crate::services::song_import::{import_song, ImportFormat};
use crate::telemetry::quality::LiveSafe;
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

    // E5 — one import RUN, not one song: the counter answers "is the importer
    // used", and a hundred-file drag would otherwise drown every other counter.
    state
        .telemetry
        .note_counter(crate::telemetry::counters::CounterName::LibraryImportRun);

    Ok(ImportResult {
        song_id: song.id,
        title,
        format,
        section_count: formatted.sections.len() as u32,
        warnings: formatted.warnings,
    })
}

/// Outcome of importing a whole EasyWorship library folder.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/lib/bindings/EasyWorshipImportResult.ts"
)]
pub struct EasyWorshipImportResult {
    /// Songs created in the target library.
    pub imported: u32,
    /// Songs left untouched — a same-title song already existed, or the entry
    /// had neither a title nor any lyrics to import.
    pub skipped: u32,
    /// Total songs read from the EasyWorship database.
    pub total: u32,
    /// Per-song failures ("«title»: reason"); the import continues past them.
    pub errors: Vec<String>,
}

/// Import an entire EasyWorship 6/8 song library.
///
/// Unlike [`import_song_file`] (a single file whose text the JS side reads with
/// `FileReader`), this reads EasyWorship's SQLite files **natively** — the
/// renderer cannot open a SQLite database — from a folder the user supplies
/// (the `…/Databases/Data` directory, or its parent). Each song is decoded and
/// landed through the same [`apply_formatted_song`] seam as every other
/// importer. It is a one-time migration action, so the work is sequential and
/// simple; it runs on Tauri's async worker, never on the UI thread.
#[tauri::command]
pub async fn import_easyworship_library(
    state: State<'_, AppState>,
    library_id: String,
    folder_path: String,
) -> AppResult<EasyWorshipImportResult> {
    let (songs_db, words_db) = locate_databases(Path::new(&folder_path))?;
    let songs = read_songs(&songs_db, &words_db).await?;

    let repo = SongRepo::new(&state.db.pool);
    let mut result = EasyWorshipImportResult {
        total: songs.len() as u32,
        ..Default::default()
    };

    for ew in &songs {
        let converted = convert_song(ew);

        // Nothing worth importing: no title and no decodable lyrics.
        if converted.formatted.title_suggestion.is_none() && converted.formatted.sections.is_empty()
        {
            result.skipped += 1;
            continue;
        }
        // Title is authoritative from the DB; the fallback only fires for the
        // rare titled-blank + has-lyrics case (matches filename_stem's default).
        let title = converted
            .formatted
            .title_suggestion
            .clone()
            .unwrap_or_else(|| "Importert sang".to_string());

        // Idempotent re-runs: don't duplicate a song already in this library.
        match repo.by_title(&library_id, &title).await {
            Ok(Some(_)) => {
                result.skipped += 1;
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                result.errors.push(format!("«{title}»: {e}"));
                continue;
            }
        }

        let created = repo
            .create(SongInput {
                library_id: library_id.clone(),
                title: title.clone(),
                language: Some(converted.formatted.language.clone()),
                default_key: None,
                tempo_bpm: None,
                ccli_song_id: converted.ccli_song_id.clone(),
                tono_work_id: None,
                copyright_notice: converted.copyright_notice.clone(),
            })
            .await;
        let song = match created {
            Ok(s) => s,
            Err(e) => {
                result.errors.push(format!("«{title}»: {e}"));
                continue;
            }
        };

        if let Err(e) = apply_formatted_song(&state.db.pool, &song.id, &converted.formatted).await {
            result.errors.push(format!("«{title}»: {e}"));
            continue;
        }
        result.imported += 1;
    }

    // One import RUN, matching import_song_file — the counter answers "was the
    // importer used", not "how many songs".
    state
        .telemetry
        .note_counter(crate::telemetry::counters::CounterName::LibraryImportRun);

    Ok(result)
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
