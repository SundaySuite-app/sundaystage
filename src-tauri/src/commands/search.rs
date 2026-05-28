//! Phase 2.3 — universal search.
//!
//! One query, results across songs, Bible, and services. Plain full-text for
//! now (FTS5 for songs + verses, LIKE for service names); semantic search is
//! Phase 11.1. Feeds the ⌘K command palette.

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::db::repositories::{BibleRepo, SongRepo};
use crate::error::AppResult;
use crate::AppState;

/// A single cross-type search hit. `kind` is "song" | "bible" | "service".
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/UniversalHit.ts")]
pub struct UniversalHit {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub subtitle: String,
}

const PER_KIND: usize = 8;

#[tauri::command]
pub async fn search_all(
    state: State<'_, AppState>,
    library_id: String,
    query: String,
) -> AppResult<Vec<UniversalHit>> {
    let q = query.trim();
    if q.len() < 2 {
        return Ok(vec![]);
    }
    let mut hits = Vec::new();

    // Songs (FTS over title + lyrics).
    let songs = SongRepo::new(&state.db.pool)
        .search(&library_id, q, PER_KIND as i64)
        .await?;
    for s in songs {
        hits.push(UniversalHit {
            kind: "song".into(),
            id: s.song_id,
            title: s.title,
            subtitle: s.snippet,
        });
    }

    // Bible (FTS over verse text, across translations).
    let verses = BibleRepo::new(&state.db.pool)
        .search(q, None, PER_KIND as i64)
        .await?;
    for v in verses {
        hits.push(UniversalHit {
            kind: "bible".into(),
            title: format!("{} {}:{}", v.book, v.chapter, v.verse),
            subtitle: v.text,
            id: v.id,
        });
    }

    // Services (name match).
    let like = format!("%{}%", q);
    let services: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT id, name FROM service
           WHERE library_id = ?1 AND name LIKE ?2 AND deleted_at IS NULL
           ORDER BY starts_at DESC LIMIT ?3"#,
    )
    .bind(&library_id)
    .bind(&like)
    .bind(PER_KIND as i64)
    .fetch_all(&state.db.pool)
    .await?;
    for (id, name) in services {
        hits.push(UniversalHit {
            kind: "service".into(),
            id,
            title: name,
            subtitle: String::new(),
        });
    }

    Ok(hits)
}
