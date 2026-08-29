//! A7 — sangbruksloggen sett fra operatørens side: les perioden, lag rapporten,
//! slett loggen.
//!
//! Ingenting her har en nettverksvei. Eksporten skriver en fil på maskinen; om
//! den filen noen gang sendes til TONO eller CCLI er eiers avgjørelse, tatt i
//! eiers e-postklient. Det er hele modellen.

use std::path::PathBuf;

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::db::models::SongUsageRow;
use crate::db::repositories::SongUsageRepo;
use crate::error::{AppError, AppResult};
use crate::services::song_usage::{export_file_name, to_csv};
use crate::AppState;

/// Where exports land: an app-owned folder, not Documents/Downloads.
///
/// macOS revokes access to Documents/Desktop/Downloads often enough that this
/// codebase has hit it three times; a report the operator cannot produce on a
/// Sunday because the OS forgot a permission is not a report. The folder is
/// opened for the operator instead (`song_usage_open_folder`).
fn reports_dir(state: &AppState) -> PathBuf {
    state.data_dir.join("rapporter")
}

/// The usage log for a period, oldest first.
#[tauri::command]
pub async fn song_usage_list(
    state: State<'_, AppState>,
    from_ms: i64,
    to_ms: i64,
) -> AppResult<Vec<SongUsageRow>> {
    SongUsageRepo::new(&state.db.pool)
        .list_between(from_ms, to_ms)
        .await
}

/// How many rows the log holds in total — what the operator sees next to
/// "delete everything".
#[tauri::command]
pub async fn song_usage_count(state: State<'_, AppState>) -> AppResult<i64> {
    SongUsageRepo::new(&state.db.pool).count().await
}

/// Write the period's report as a CSV and return the file's path.
#[tauri::command]
pub async fn song_usage_export_csv(
    state: State<'_, AppState>,
    from_ms: i64,
    to_ms: i64,
) -> AppResult<String> {
    export_csv(&state, from_ms, to_ms).await
}

/// The body of [`song_usage_export_csv`], over a plain `&AppState` so the file
/// it produces can be tested for real rather than re-described.
async fn export_csv(state: &AppState, from_ms: i64, to_ms: i64) -> AppResult<String> {
    let rows = SongUsageRepo::new(&state.db.pool)
        .list_between(from_ms, to_ms)
        .await?;
    let csv = to_csv(&rows);

    let dir = reports_dir(state);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Internal(e.to_string()))?;
    let path = dir.join(export_file_name(from_ms, to_ms));
    std::fs::write(&path, csv).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(path.to_string_lossy().to_string())
}

/// Reveal the reports folder in Finder/Explorer.
///
/// Takes no path from the UI on purpose: the folder is derived in Rust, so this
/// command can never be talked into opening somewhere else.
#[tauri::command]
pub fn song_usage_open_folder(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let dir = reports_dir(&state);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Internal(e.to_string()))?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| AppError::Internal(e.to_string()))
}

/// Delete the whole log. Returns how many rows went.
#[tauri::command]
pub async fn song_usage_clear(state: State<'_, AppState>) -> AppResult<u64> {
    SongUsageRepo::new(&state.db.pool).clear().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::SongUsageEntry;

    fn entry(song_id: &str, at: i64) -> SongUsageEntry {
        SongUsageEntry {
            service_id: "svc".into(),
            service_name: "Gudstjeneste".into(),
            service_date: "2026-01-04".into(),
            song_id: song_id.into(),
            title: format!("Sang {song_id}"),
            author: None,
            ccli_song_id: None,
            tono_work_id: None,
            copyright_notice: None,
            first_shown_at: at,
            last_shown_at: at + 60_000,
            visible_ms: 60_000,
            show_count: 1,
        }
    }

    /// The file the operator actually gets, produced by the real command body.
    #[tokio::test]
    async fn eksporten_skriver_periodens_rader_til_fil() {
        let (state, _dir) = crate::commands::live::tests::app_state().await;
        let repo = SongUsageRepo::new(&state.db.pool);
        repo.record(&entry("s1", 1_000)).await.expect("record");
        repo.record(&entry("s2", 900_000)).await.expect("record");

        let path = export_csv(&state, 0, 500_000).await.expect("export");
        let written = std::fs::read_to_string(&path).expect("read");
        assert!(written.starts_with('\u{feff}'), "BOM for norsk Excel");
        assert!(written.contains("Sang s1"));
        assert!(
            !written.contains("Sang s2"),
            "utenfor perioden skal ikke med"
        );
        assert!(
            path.contains("rapporter"),
            "havner i app-mappa, ikke i Dokumenter: {path}"
        );
    }
}
