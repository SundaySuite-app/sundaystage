//! SundayPlan → SundayStage plan import.
//!
//! SundayPlan (the suite's volunteer-scheduling + service-planning app) owns
//! the *plan*: which songs, in what order, for a given Sunday. SundayStage owns
//! the *presentation* of that plan. The natural handoff is "send the setlist to
//! the stage".
//!
//! SundayPlan has no export yet (its data lives in Supabase, export is a later
//! phase), so this module defines the interchange we'll accept and is tolerant
//! by design: it mirrors SundayPlan's documented Service + Setlist model and
//! fills sensible defaults for anything missing. When SundayPlan ships export,
//! its JSON drops straight in.
//!
//! SundayPlan's song ids don't exist in this library, so songs are matched by
//! **title** against the local library; unmatched titles become empty stub
//! songs (nothing is silently dropped) and are reported back. Scripture is
//! imported as a labelled placeholder for the operator to wire up in the Bible
//! module — we don't guess a translation.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;

use crate::db::models::{Service, SongInput};
use crate::db::now_ms;
use crate::db::repositories::{ServiceRepo, SongRepo};
use crate::error::{AppError, AppResult};

/// The interchange shape we accept for a SundayPlan plan.
#[derive(Debug, Clone, Deserialize)]
pub struct PlanImport {
    #[serde(default)]
    pub name: Option<String>,
    /// Service start, unix millis. `starts_at_utc` is SundayPlan's field name.
    #[serde(default, alias = "starts_at_utc")]
    pub starts_at: Option<i64>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub items: Vec<PlanItemImport>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanItemImport {
    /// "song" | "scripture" | "gap" | "announcement" | … (defaults to "song").
    #[serde(default)]
    pub kind: Option<String>,
    /// Song title (the match key).
    #[serde(default)]
    pub title: Option<String>,
    /// Performance key for a song.
    #[serde(default, alias = "key_override")]
    pub key: Option<String>,
    /// Scripture reference, e.g. "John 3:16".
    #[serde(default, alias = "scripture_ref")]
    pub reference: Option<String>,
    /// Label for a non-song item (gap/announcement).
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Outcome of an import — what landed, and what needs a human's attention.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/PlanImportResult.ts")]
pub struct PlanImportResult {
    pub service: Service,
    /// Songs matched to an existing library entry.
    pub matched_songs: u32,
    /// Titles of songs not found locally — created as empty stubs to fill in.
    pub created_songs: Vec<String>,
    /// Items that couldn't be imported faithfully (skipped, or placed as a
    /// placeholder to finish manually).
    pub warnings: Vec<String>,
}

/// Parse a SundayPlan plan JSON and build a SundayStage service from it.
pub async fn import_plan(
    pool: &SqlitePool,
    library_id: &str,
    json: &str,
) -> AppResult<PlanImportResult> {
    let plan: PlanImport = serde_json::from_str(json)
        .map_err(|e| AppError::Validation(format!("ugyldig SundayPlan-JSON: {e}")))?;

    let svc_repo = ServiceRepo::new(pool);
    let song_repo = SongRepo::new(pool);

    let name = plan
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("Importert plan")
        .to_string();
    let starts_at = plan.starts_at.unwrap_or_else(now_ms);
    let mut service = svc_repo.create(library_id, &name, starts_at).await?;

    if let Some(notes) = plan
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        service = svc_repo.set_notes(&service.id, notes).await?;
    }

    let mut matched_songs = 0u32;
    let mut created_songs = Vec::new();
    let mut warnings = Vec::new();

    for (i, item) in plan.items.iter().enumerate() {
        let kind = item.kind.as_deref().unwrap_or("song");
        match kind {
            "song" => {
                let Some(title) = item
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                else {
                    warnings.push(format!(
                        "Item {} (sang) mangler tittel — hoppet over",
                        i + 1
                    ));
                    continue;
                };
                let song = match song_repo.by_title(library_id, title).await? {
                    Some(s) => {
                        matched_songs += 1;
                        s
                    }
                    None => {
                        let s = song_repo
                            .create(SongInput {
                                library_id: library_id.into(),
                                title: title.into(),
                                language: None,
                                default_key: item.key.clone(),
                                tempo_bpm: None,
                                ccli_song_id: None,
                                tono_work_id: None,
                                copyright_notice: None,
                            })
                            .await?;
                        created_songs.push(title.to_string());
                        s
                    }
                };
                let pos = svc_repo.next_position(&service.id).await?;
                svc_repo
                    .add_item(
                        &service.id,
                        pos,
                        "song",
                        Some(&song.id),
                        None,
                        item.key.as_deref(),
                        None,
                        None,
                    )
                    .await?;
            }
            "scripture" => {
                let reference = item
                    .reference
                    .as_deref()
                    .or(item.title.as_deref())
                    .unwrap_or("Skrift");
                let pos = svc_repo.next_position(&service.id).await?;
                svc_repo
                    .add_item(
                        &service.id,
                        pos,
                        "gap",
                        None,
                        None,
                        None,
                        None,
                        Some(reference),
                    )
                    .await?;
                warnings.push(format!(
                    "Skrift «{reference}» lagt til som plassholder — koble til en oversettelse i Bibel-modulen"
                ));
            }
            other => {
                let label = item
                    .label
                    .as_deref()
                    .or(item.notes.as_deref())
                    .or(item.title.as_deref())
                    .unwrap_or(other);
                let pos = svc_repo.next_position(&service.id).await?;
                svc_repo
                    .add_item(&service.id, pos, "gap", None, None, None, None, Some(label))
                    .await?;
            }
        }
    }

    Ok(PlanImportResult {
        service,
        matched_songs,
        created_songs,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::LibraryInput;
    use crate::db::repositories::LibraryRepo;
    use crate::db::Database;

    #[tokio::test]
    async fn imports_matches_stubs_and_placeholders() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "Test".into(),
                default_locale: None,
            })
            .await
            .unwrap();

        // One song already in the library, matched case-insensitively.
        SongRepo::new(&db.pool)
            .create(SongInput {
                library_id: lib.id.clone(),
                title: "Amazing Grace".into(),
                language: Some("en".into()),
                default_key: None,
                tempo_bpm: None,
                ccli_song_id: None,
                tono_work_id: None,
                copyright_notice: None,
            })
            .await
            .unwrap();

        let json = r#"{
            "name": "Sunday 14 June",
            "starts_at": 1718352000000,
            "notes": "Pinse",
            "items": [
                { "kind": "song", "title": "amazing grace", "key": "G" },
                { "kind": "song", "title": "Oceans" },
                { "kind": "scripture", "reference": "John 3:16" },
                { "kind": "gap", "label": "Kollekt" }
            ]
        }"#;

        let res = import_plan(&db.pool, &lib.id, json).await.unwrap();

        assert_eq!(res.service.name, "Sunday 14 June");
        assert_eq!(res.service.starts_at, 1718352000000);
        assert_eq!(res.service.notes.as_deref(), Some("Pinse"));
        assert_eq!(res.matched_songs, 1, "Amazing Grace matched the library");
        assert_eq!(res.created_songs, vec!["Oceans"], "missing song stubbed");
        assert_eq!(res.warnings.len(), 1, "scripture left as a placeholder");

        // Four items landed, in order.
        let items = ServiceRepo::new(&db.pool)
            .items(&res.service.id)
            .await
            .unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].kind, "song");
        assert_eq!(items[0].key_override.as_deref(), Some("G"));
        assert_eq!(items[2].kind, "gap"); // scripture placeholder
        assert_eq!(items[2].notes.as_deref(), Some("John 3:16"));
        assert_eq!(items[3].notes.as_deref(), Some("Kollekt"));
    }

    #[tokio::test]
    async fn rejects_invalid_json() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "T".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        assert!(import_plan(&db.pool, &lib.id, "not json").await.is_err());
    }
}
