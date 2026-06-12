//! SundayPlan → SundayStage plan import.
//!
//! SundayPlan (the suite's volunteer-scheduling + service-planning app) owns
//! the *plan*: which songs, in what order, for a given Sunday. SundayStage owns
//! the *presentation* of that plan. The natural handoff is "send the setlist to
//! the stage".
//!
//! The interchange is tolerant by design and accepts BOTH wire shapes:
//!
//!  - the canonical `ServicePlan` envelope from sunday-platform
//!    `sunday-contracts` v0.4.0 (`{ schema_version, service: { name, starts_at,
//!    notes, … }, items: [{ kind, title, song_ref, key_override, … }] }`) that
//!    SundayPlan's SDK exporter (`packages/sdk/src/serviceplan.ts`) emits —
//!    including the canonical `song_ref` (`local_id`/`default_key`/licensing
//!    ids), so a song's toneart and CCLI/TONO ids survive the handoff;
//!  - the older flat shape (`{ name, starts_at, items: [...] }`) so existing
//!    integrations keep working.
//!
//! Sensible defaults fill anything missing.
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

/// The interchange shape we accept for a SundayPlan plan. Either flat
/// (`name`/`starts_at` at the top level) or the canonical `ServicePlan`
/// envelope (the same fields under `service`).
#[derive(Debug, Clone, Deserialize)]
pub struct PlanImport {
    #[serde(default)]
    pub name: Option<String>,
    /// Service start, unix millis. `starts_at_utc` is SundayPlan's field name.
    #[serde(default, alias = "starts_at_utc")]
    pub starts_at: Option<i64>,
    #[serde(default)]
    pub notes: Option<String>,
    /// The canonical `ServicePlan.service` envelope, when present.
    #[serde(default)]
    pub service: Option<PlanServiceImport>,
    #[serde(default)]
    pub items: Vec<PlanItemImport>,
}

/// The canonical `ServiceRef` subset the importer uses (sunday-contracts
/// v0.4.0, service.ts). Unknown fields (`id`, `church_id`, `state`,
/// `was_streamed`, `schema_version`, …) are ignored — they identify the plan
/// in PLAN's world, not in this library.
#[derive(Debug, Clone, Deserialize)]
pub struct PlanServiceImport {
    #[serde(default)]
    pub name: Option<String>,
    /// Canonical `starts_at` is an ISO-8601 UTC string.
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// The canonical `SongRef` (sunday-contracts v0.4.0, song.ts) on a setlist
/// item. Carries the song's home key (toneart) + licensing ids, which the
/// flat shape never had.
#[derive(Debug, Clone, Deserialize)]
pub struct PlanSongRefImport {
    #[serde(default)]
    pub sundaysong_id: Option<String>,
    #[serde(default)]
    pub local_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub ccli_song_id: Option<String>,
    #[serde(default)]
    pub tono_work_id: Option<String>,
    #[serde(default)]
    pub default_key: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
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
    /// Canonical song reference, when the emitter sends one.
    #[serde(default)]
    pub song_ref: Option<PlanSongRefImport>,
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

/// Parse an ISO-8601 / RFC-3339 UTC timestamp (the canonical `starts_at`) to
/// unix millis. `None` for anything unparseable — the caller falls back.
fn parse_iso_ms(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis())
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

    // Flat fields win when present; otherwise the canonical `service` envelope.
    let envelope = plan.service.as_ref();
    let name = plan
        .name
        .as_deref()
        .or(envelope.and_then(|s| s.name.as_deref()))
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("Importert plan")
        .to_string();
    // Flat `starts_at` is unix ms; the canonical envelope's is ISO-8601 UTC.
    let starts_at = plan
        .starts_at
        .or_else(|| {
            envelope
                .and_then(|s| s.starts_at.as_deref())
                .and_then(parse_iso_ms)
        })
        .unwrap_or_else(now_ms);
    let mut service = svc_repo.create(library_id, &name, starts_at).await?;

    if let Some(notes) = plan
        .notes
        .as_deref()
        .or(envelope.and_then(|s| s.notes.as_deref()))
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
                // Title: the item's own, else the canonical song_ref's.
                let song_ref = item.song_ref.as_ref();
                let Some(title) = item
                    .title
                    .as_deref()
                    .or(song_ref.and_then(|r| r.title.as_deref()))
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
                        // A stub inherits everything the canonical song_ref
                        // carries: home key (toneart), language, CCLI/TONO ids.
                        // Falls back to the per-item key for old flat payloads.
                        let s = song_repo
                            .create(SongInput {
                                library_id: library_id.into(),
                                title: title.into(),
                                language: song_ref.and_then(|r| r.language.clone()),
                                default_key: song_ref
                                    .and_then(|r| r.default_key.clone())
                                    .or_else(|| item.key.clone()),
                                tempo_bpm: None,
                                ccli_song_id: song_ref.and_then(|r| r.ccli_song_id.clone()),
                                tono_work_id: song_ref.and_then(|r| r.tono_work_id.clone()),
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

    // ── canonical ServicePlan envelope (sunday-contracts v0.4.0) ─────────────

    #[tokio::test]
    async fn accepts_canonical_serviceplan_envelope_with_song_ref() {
        let (db, lib) = fresh_lib().await;
        // Exactly what SundayPlan's SDK exporter emits post-convergence:
        // schema_version + service envelope + canonical song_ref per item.
        let json = r#"{
            "schema_version": 1,
            "service": {
                "schema_version": 1,
                "id": "33333333-3333-3333-3333-333333333333",
                "church_id": "11111111-1111-1111-1111-111111111111",
                "name": "Gudstjeneste 14. juni",
                "starts_at": "2026-06-14T09:00:00Z",
                "state": "published",
                "was_streamed": false,
                "notes": "Pinse"
            },
            "items": [
                {
                    "position": 1,
                    "kind": "song",
                    "title": "Oceans",
                    "song_ref": {
                        "sundaysong_id": null,
                        "local_id": "22222222-2222-2222-2222-222222222222",
                        "title": "Oceans",
                        "ccli_song_id": "6428767",
                        "tono_work_id": "T-915",
                        "default_key": "D",
                        "language": "en"
                    },
                    "scripture_ref": null,
                    "key_override": "C",
                    "duration_min": 5,
                    "notes": null
                },
                { "position": 2, "kind": "scripture", "scripture_ref": "Apg 2:1-4" }
            ]
        }"#;
        let res = import_plan(&db.pool, &lib, json).await.unwrap();

        // The canonical envelope drives name/start/notes.
        assert_eq!(res.service.name, "Gudstjeneste 14. juni");
        assert_eq!(res.service.starts_at, 1_781_427_600_000); // 2026-06-14T09:00:00Z
        assert_eq!(res.service.notes.as_deref(), Some("Pinse"));

        // The stub song inherits the song_ref's toneart + licensing identity.
        assert_eq!(res.created_songs, vec!["Oceans"]);
        let song = SongRepo::new(&db.pool)
            .by_title(&lib, "Oceans")
            .await
            .unwrap()
            .expect("stub created");
        assert_eq!(song.default_key.as_deref(), Some("D"), "toneart preserved");
        assert_eq!(song.ccli_song_id.as_deref(), Some("6428767"));
        assert_eq!(song.tono_work_id.as_deref(), Some("T-915"));
        assert_eq!(song.language, "en");

        let items = ServiceRepo::new(&db.pool)
            .items(&res.service.id)
            .await
            .unwrap();
        assert_eq!(items.len(), 2);
        // The per-service override still comes from key_override, not the home key.
        assert_eq!(items[0].key_override.as_deref(), Some("C"));
        assert_eq!(items[1].notes.as_deref(), Some("Apg 2:1-4"));
    }

    #[tokio::test]
    async fn song_ref_title_is_the_fallback_match_key() {
        let (db, lib) = fresh_lib().await;
        // No item title at all — the canonical song_ref's title must carry it.
        let json = r#"{
            "items": [
                { "kind": "song", "song_ref": { "title": "How Great Thou Art", "language": "en" } }
            ]
        }"#;
        let res = import_plan(&db.pool, &lib, json).await.unwrap();
        assert_eq!(res.created_songs, vec!["How Great Thou Art"]);
        assert!(res.warnings.is_empty());
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
