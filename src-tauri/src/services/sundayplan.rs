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

    /// Spin up an in-memory DB + library so each edge-case test is isolated.
    async fn fresh_lib() -> (Database, String) {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "T".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        let id = lib.id.clone();
        (db, id)
    }

    // ── defaults for a near-empty plan ──────────────────────────────────────

    #[tokio::test]
    async fn missing_name_and_start_get_defaults() {
        let (db, lib) = fresh_lib().await;
        let before = now_ms();
        // No name, no starts_at, no items at all.
        let res = import_plan(&db.pool, &lib, "{}").await.unwrap();
        let after = now_ms();

        assert_eq!(res.service.name, "Importert plan", "name default applied");
        assert!(
            res.service.starts_at >= before && res.service.starts_at <= after,
            "starts_at fell back to now_ms()"
        );
        assert!(res.service.notes.is_none(), "no notes set");
        assert_eq!(res.matched_songs, 0);
        assert!(res.created_songs.is_empty());
        assert!(res.warnings.is_empty());
    }

    #[tokio::test]
    async fn blank_name_falls_back_to_default() {
        let (db, lib) = fresh_lib().await;
        // Whitespace-only name must be treated as missing, not stored verbatim.
        let res = import_plan(&db.pool, &lib, r#"{ "name": "   " }"#)
            .await
            .unwrap();
        assert_eq!(res.service.name, "Importert plan");
    }

    // ── SundayPlan's real export field names (serde aliases) ─────────────────

    #[tokio::test]
    async fn accepts_sundayplan_export_field_aliases() {
        let (db, lib) = fresh_lib().await;
        // SundayPlan exports starts_at_utc / scripture_ref / key_override; if any
        // alias regresses, the cross-app handoff silently drops that field.
        let json = r#"{
            "name": "Plan",
            "starts_at_utc": 1718352000000,
            "items": [
                { "kind": "song", "title": "New Song", "key_override": "A" },
                { "kind": "scripture", "scripture_ref": "Romans 8:28" }
            ]
        }"#;
        let res = import_plan(&db.pool, &lib, json).await.unwrap();

        assert_eq!(res.service.starts_at, 1718352000000, "starts_at_utc alias");

        let items = ServiceRepo::new(&db.pool)
            .items(&res.service.id)
            .await
            .unwrap();
        assert_eq!(
            items[0].key_override.as_deref(),
            Some("A"),
            "key_override alias"
        );
        assert_eq!(
            items[1].notes.as_deref(),
            Some("Romans 8:28"),
            "scripture_ref alias landed as the placeholder reference"
        );
    }

    // ── song item with no usable title ──────────────────────────────────────

    #[tokio::test]
    async fn song_without_title_is_skipped_with_warning() {
        let (db, lib) = fresh_lib().await;
        let json = r#"{
            "items": [
                { "kind": "song" },
                { "kind": "song", "title": "   " },
                { "kind": "song", "title": "Real" }
            ]
        }"#;
        let res = import_plan(&db.pool, &lib, json).await.unwrap();

        // Two untitled songs warned + skipped; only the real one created.
        assert_eq!(res.warnings.len(), 2);
        assert!(res.warnings.iter().all(|w| w.contains("mangler tittel")));
        assert_eq!(res.created_songs, vec!["Real"]);

        let items = ServiceRepo::new(&db.pool)
            .items(&res.service.id)
            .await
            .unwrap();
        assert_eq!(items.len(), 1, "only the titled song landed");
        assert_eq!(items[0].kind, "song");
    }

    // ── scripture reference fallback chain ──────────────────────────────────

    #[tokio::test]
    async fn scripture_falls_back_to_title_then_generic_label() {
        let (db, lib) = fresh_lib().await;
        let json = r#"{
            "items": [
                { "kind": "scripture", "title": "Salme 23" },
                { "kind": "scripture" }
            ]
        }"#;
        let res = import_plan(&db.pool, &lib, json).await.unwrap();

        let items = ServiceRepo::new(&db.pool)
            .items(&res.service.id)
            .await
            .unwrap();
        assert_eq!(items.len(), 2);
        // No `reference`, so it falls back to `title`…
        assert_eq!(items[0].notes.as_deref(), Some("Salme 23"));
        // …and with neither, to the generic "Skrift".
        assert_eq!(items[1].notes.as_deref(), Some("Skrift"));
        // Both scripture items raise the wire-it-up-in-Bibel warning.
        assert_eq!(res.warnings.len(), 2);
    }

    // ── unknown kind: label → notes → title → kind ──────────────────────────

    #[tokio::test]
    async fn unknown_kind_resolves_label_chain() {
        let (db, lib) = fresh_lib().await;
        let json = r#"{
            "items": [
                { "kind": "announcement", "label": "Kunngjøring" },
                { "kind": "announcement", "notes": "Fra notatfeltet" },
                { "kind": "announcement", "title": "Fra tittel" },
                { "kind": "offering" }
            ]
        }"#;
        let res = import_plan(&db.pool, &lib, json).await.unwrap();

        let items = ServiceRepo::new(&db.pool)
            .items(&res.service.id)
            .await
            .unwrap();
        assert_eq!(items.len(), 4);
        assert!(
            items.iter().all(|it| it.kind == "gap"),
            "all placed as gaps"
        );
        assert_eq!(items[0].notes.as_deref(), Some("Kunngjøring"));
        assert_eq!(items[1].notes.as_deref(), Some("Fra notatfeltet"));
        assert_eq!(items[2].notes.as_deref(), Some("Fra tittel"));
        // Nothing usable → the kind itself becomes the label.
        assert_eq!(items[3].notes.as_deref(), Some("offering"));
        // Unknown kinds aren't warned about — they land cleanly as placeholders.
        assert!(res.warnings.is_empty());
    }

    // ── kind omitted defaults to "song" ─────────────────────────────────────

    #[tokio::test]
    async fn item_without_kind_defaults_to_song() {
        let (db, lib) = fresh_lib().await;
        let res = import_plan(
            &db.pool,
            &lib,
            r#"{ "items": [ { "title": "Untitled Default" } ] }"#,
        )
        .await
        .unwrap();
        assert_eq!(res.created_songs, vec!["Untitled Default"]);
        let items = ServiceRepo::new(&db.pool)
            .items(&res.service.id)
            .await
            .unwrap();
        assert_eq!(items[0].kind, "song");
    }
}
