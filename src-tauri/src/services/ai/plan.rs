//! Phase 11.2 — service-planning assistant.
//!
//! The operator describes a service in words ("30-min worship set on
//! forgiveness for a young-adult service"); Claude proposes songs from *their*
//! library, transitions, keys, and a reading. The proposal is a draft the user
//! reviews and then turns into a real `Service` with `ServiceItem`s.
//!
//! Pure here (tested): the prompt, the tool schema, and
//! [`parse_plan_response`], which validates every proposed song against the
//! library so the AI can't invent songs the church doesn't own.
//! [`apply_plan`] writes the accepted plan to the database.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::models::Service;
use crate::db::now_ms;
use crate::db::repositories::{ArrangementRepo, ServiceRepo};
use crate::error::AppResult;
use sqlx::SqlitePool;

/// One proposed item in a service plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/PlanItem.ts")]
pub struct PlanItem {
    /// `song` | `scripture` | `note`.
    pub kind: String,
    /// Human label for the item (song title, passage, or transition note).
    pub title: String,
    /// Library song id — only set (and only kept) for `song` items that match
    /// a real song in the library.
    pub song_id: Option<String>,
    /// Suggested key for a song.
    pub key: Option<String>,
    /// Scripture reference text (for `scripture` items).
    pub reference: Option<String>,
    /// Free-text note / transition guidance.
    pub note: Option<String>,
}

/// A proposed service the user can review and create.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../src/lib/bindings/ServicePlan.ts")]
pub struct ServicePlan {
    pub title: String,
    pub theme: Option<String>,
    pub items: Vec<PlanItem>,
    pub warnings: Vec<String>,
}

/// A song the AI is allowed to pick from.
pub struct LibrarySong {
    pub id: String,
    pub title: String,
    pub key: Option<String>,
}

pub const PLAN_TOOL_NAME: &str = "emit_service_plan";

pub fn system_prompt(songs: &[LibrarySong]) -> String {
    let mut catalogue = String::new();
    for s in songs {
        let key = s.key.as_deref().unwrap_or("?");
        catalogue.push_str(&format!("- id={} | {} (key {})\n", s.id, s.title, key));
    }
    format!(
        "You are a worship service planning assistant. Propose a service plan \
from the user's description.\n\n\
Rules:\n\
- You may ONLY pick songs from the library below, and you MUST reference each \
chosen song by its exact id. Never invent songs.\n\
- Order the items sensibly (gathering → worship → word → response/sending).\n\
- For each song you may suggest a key.\n\
- You may add `scripture` items (with a reference) and `note` items \
(transitions, responsive readings) even though those aren't in the library.\n\
- Keep it to a realistic length for the request.\n\
- Call the {PLAN_TOOL_NAME} tool with the plan.\n\n\
Library songs:\n{catalogue}"
    )
}

pub fn tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "theme": { "type": ["string", "null"] },
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string", "enum": ["song", "scripture", "note"] },
                        "title": { "type": "string" },
                        "song_id": { "type": ["string", "null"] },
                        "key": { "type": ["string", "null"] },
                        "reference": { "type": ["string", "null"] },
                        "note": { "type": ["string", "null"] }
                    },
                    "required": ["kind", "title"]
                }
            }
        },
        "required": ["title", "items"]
    })
}

/// Parse the tool input into a validated [`ServicePlan`]. Song items whose
/// `song_id` isn't in `valid_song_ids` are downgraded to notes with a warning,
/// so an accepted plan can never reference a song the church doesn't own.
pub fn parse_plan_response(
    input: &serde_json::Value,
    valid_song_ids: &HashSet<String>,
) -> ServicePlan {
    let title = input
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("AI-tjenesteplan")
        .to_string();
    let theme = input
        .get("theme")
        .and_then(|t| t.as_str())
        .map(String::from);

    let mut warnings = Vec::new();
    let mut items = Vec::new();
    if let Some(arr) = input.get("items").and_then(|i| i.as_array()) {
        for raw in arr {
            let kind = raw.get("kind").and_then(|k| k.as_str()).unwrap_or("note");
            let title = raw
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            if title.trim().is_empty() {
                continue;
            }
            let song_id = raw
                .get("song_id")
                .and_then(|s| s.as_str())
                .map(String::from);
            let key = raw.get("key").and_then(|k| k.as_str()).map(String::from);
            let reference = raw
                .get("reference")
                .and_then(|r| r.as_str())
                .map(String::from);
            let note = raw.get("note").and_then(|n| n.as_str()).map(String::from);

            if kind == "song" {
                match &song_id {
                    Some(id) if valid_song_ids.contains(id) => {
                        items.push(PlanItem {
                            kind: "song".into(),
                            title,
                            song_id,
                            key,
                            reference: None,
                            note,
                        });
                    }
                    _ => {
                        warnings.push(format!(
                            "«{title}» matchet ingen sang i biblioteket — la til som notat"
                        ));
                        items.push(PlanItem {
                            kind: "note".into(),
                            title,
                            song_id: None,
                            key: None,
                            reference: None,
                            note,
                        });
                    }
                }
            } else {
                items.push(PlanItem {
                    kind: if kind == "scripture" {
                        "scripture"
                    } else {
                        "note"
                    }
                    .into(),
                    title,
                    song_id: None,
                    key: None,
                    reference,
                    note,
                });
            }
        }
    }

    ServicePlan {
        title,
        theme,
        items,
        warnings,
    }
}

/// Create a real `Service` from an accepted plan: a `song` item becomes a song
/// service-item wired to the song's default arrangement (with the suggested
/// key as `key_override`); `scripture`/`note` items become `announcement`
/// items carrying their reference/note text.
pub async fn apply_plan(
    pool: &SqlitePool,
    library_id: &str,
    plan: &ServicePlan,
) -> AppResult<Service> {
    let svc_repo = ServiceRepo::new(pool);
    let arr_repo = ArrangementRepo::new(pool);
    let service = svc_repo.create(library_id, &plan.title, now_ms()).await?;

    for (position, item) in plan.items.iter().enumerate() {
        let pos = position as i64;
        if item.kind == "song" {
            if let Some(song_id) = &item.song_id {
                let arrangement = arr_repo
                    .list(song_id)
                    .await?
                    .into_iter()
                    .find(|a| a.is_default == 1)
                    .map(|a| a.id);
                svc_repo
                    .add_item(
                        &service.id,
                        pos,
                        "song",
                        Some(song_id),
                        arrangement.as_deref(),
                        item.key.as_deref(),
                        None,
                        None,
                    )
                    .await?;
                continue;
            }
        }
        // scripture / note → announcement carrying the text.
        let notes = item
            .reference
            .clone()
            .or_else(|| item.note.clone())
            .unwrap_or_else(|| item.title.clone());
        svc_repo
            .add_item(
                &service.id,
                pos,
                "announcement",
                None,
                None,
                None,
                None,
                Some(&notes),
            )
            .await?;
    }

    Ok(service)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_keeps_valid_songs_and_downgrades_unknown() {
        let input = serde_json::json!({
            "title": "Forgiveness set",
            "theme": "forgiveness",
            "items": [
                { "kind": "song", "title": "Amazing Grace", "song_id": "song-1", "key": "G" },
                { "kind": "song", "title": "Invented Song", "song_id": "ghost" },
                { "kind": "scripture", "title": "Psalm 51", "reference": "Salme 51:1-10" },
                { "kind": "note", "title": "Stille bønn" }
            ]
        });
        let plan = parse_plan_response(&input, &ids(&["song-1"]));
        assert_eq!(plan.title, "Forgiveness set");
        assert_eq!(plan.theme.as_deref(), Some("forgiveness"));
        assert_eq!(plan.items.len(), 4);
        assert_eq!(plan.items[0].kind, "song");
        assert_eq!(plan.items[0].song_id.as_deref(), Some("song-1"));
        assert_eq!(plan.items[0].key.as_deref(), Some("G"));
        // unknown song downgraded
        assert_eq!(plan.items[1].kind, "note");
        assert!(plan.items[1].song_id.is_none());
        assert!(plan.warnings.iter().any(|w| w.contains("Invented Song")));
        assert_eq!(plan.items[2].kind, "scripture");
        assert_eq!(plan.items[2].reference.as_deref(), Some("Salme 51:1-10"));
    }

    #[test]
    fn parse_skips_empty_titles() {
        let input = serde_json::json!({
            "title": "x",
            "items": [{ "kind": "note", "title": "  " }, { "kind": "note", "title": "Velkommen" }]
        });
        let plan = parse_plan_response(&input, &ids(&[]));
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].title, "Velkommen");
    }

    #[test]
    fn system_prompt_lists_song_ids() {
        let songs = vec![
            LibrarySong {
                id: "s1".into(),
                title: "Amazing Grace".into(),
                key: Some("G".into()),
            },
            LibrarySong {
                id: "s2".into(),
                title: "10,000 Reasons".into(),
                key: None,
            },
        ];
        let p = system_prompt(&songs);
        assert!(p.contains("id=s1 | Amazing Grace (key G)"));
        assert!(p.contains("id=s2 | 10,000 Reasons (key ?)"));
        assert!(p.contains(PLAN_TOOL_NAME));
    }

    #[tokio::test]
    async fn apply_creates_service_with_song_and_note_items() {
        use crate::db::models::{LibraryInput, SongInput};
        use crate::db::repositories::{ArrangementRepo, LibraryRepo, ServiceRepo, SongRepo};
        use crate::db::Database;

        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "T".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        let song = SongRepo::new(&db.pool)
            .create(SongInput {
                library_id: lib.id.clone(),
                title: "Amazing Grace".into(),
                language: None,
                default_key: None,
                tempo_bpm: None,
                ccli_song_id: None,
                tono_work_id: None,
                copyright_notice: None,
            })
            .await
            .unwrap();
        let arr = ArrangementRepo::new(&db.pool)
            .create(&song.id, "Full")
            .await
            .unwrap();

        let plan = ServicePlan {
            title: "Sunday".into(),
            theme: Some("grace".into()),
            items: vec![
                PlanItem {
                    kind: "song".into(),
                    title: "Amazing Grace".into(),
                    song_id: Some(song.id.clone()),
                    key: Some("G".into()),
                    reference: None,
                    note: None,
                },
                PlanItem {
                    kind: "scripture".into(),
                    title: "John 3:16".into(),
                    song_id: None,
                    key: None,
                    reference: Some("Joh 3:16".into()),
                    note: None,
                },
            ],
            warnings: vec![],
        };
        let service = apply_plan(&db.pool, &lib.id, &plan).await.unwrap();
        let items = ServiceRepo::new(&db.pool).items(&service.id).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "song");
        assert_eq!(items[0].song_id.as_deref(), Some(song.id.as_str()));
        assert_eq!(items[0].arrangement_id.as_deref(), Some(arr.id.as_str()));
        assert_eq!(items[0].key_override.as_deref(), Some("G"));
        assert_eq!(items[1].kind, "announcement");
        assert_eq!(items[1].notes.as_deref(), Some("Joh 3:16"));
    }
}
