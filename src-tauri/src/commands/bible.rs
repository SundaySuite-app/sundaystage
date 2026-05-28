//! Phase 7.1 — Bible commands: browse, look up, search, compare, add-to-service.

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::db::models::{BibleTranslation, BibleVerse, ServiceItem};
use crate::db::repositories::{BibleRepo, ServiceRepo};
use crate::error::{AppError, AppResult};
use crate::services::bible::{book_display, parse_reference, render_reference};
use crate::AppState;

/// A book in a translation, with its localized display name.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/BibleBook.ts")]
pub struct BibleBook {
    pub book: String,
    pub book_order: i64,
    pub display: String,
}

/// A resolved passage: the verses plus a tidy display reference.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/BiblePassage.ts")]
pub struct BiblePassage {
    pub reference: String,
    pub verses: Vec<BibleVerse>,
}

#[tauri::command]
pub async fn bible_translations(state: State<'_, AppState>) -> AppResult<Vec<BibleTranslation>> {
    BibleRepo::new(&state.db.pool).list_translations().await
}

#[tauri::command]
pub async fn bible_books(
    state: State<'_, AppState>,
    translation_id: String,
) -> AppResult<Vec<BibleBook>> {
    let repo = BibleRepo::new(&state.db.pool);
    let lang = repo
        .list_translations()
        .await?
        .into_iter()
        .find(|t| t.id == translation_id)
        .map(|t| t.language)
        .unwrap_or_else(|| "en".into());
    let books = repo.books(&translation_id).await?;
    Ok(books
        .into_iter()
        .map(|(book, book_order)| BibleBook {
            display: book_display(&book, &lang),
            book,
            book_order,
        })
        .collect())
}

#[tauri::command]
pub async fn bible_chapters(
    state: State<'_, AppState>,
    translation_id: String,
    book: String,
) -> AppResult<Vec<i64>> {
    BibleRepo::new(&state.db.pool)
        .chapters(&translation_id, &book)
        .await
}

#[tauri::command]
pub async fn bible_passage(
    state: State<'_, AppState>,
    translation_id: String,
    book: String,
    chapter: i64,
    verse_start: Option<i64>,
    verse_end: Option<i64>,
) -> AppResult<Vec<BibleVerse>> {
    BibleRepo::new(&state.db.pool)
        .passage(&translation_id, &book, chapter, verse_start, verse_end)
        .await
}

/// Parse "John 3:16" / "Sal 23" and return the matching verses.
#[tauri::command]
pub async fn bible_lookup(
    state: State<'_, AppState>,
    translation_id: String,
    query: String,
) -> AppResult<BiblePassage> {
    let parsed = parse_reference(&query)
        .map_err(|e| AppError::Validation(format!("Ugyldig referanse: {e}")))?;
    let verses = BibleRepo::new(&state.db.pool)
        .passage(
            &translation_id,
            &parsed.book,
            parsed.chapter as i64,
            parsed.verse_start.map(|v| v as i64),
            parsed.verse_end.map(|v| v as i64),
        )
        .await?;
    Ok(BiblePassage {
        reference: render_reference(&parsed),
        verses,
    })
}

#[tauri::command]
pub async fn bible_search(
    state: State<'_, AppState>,
    query: String,
    translation_id: Option<String>,
) -> AppResult<Vec<BibleVerse>> {
    BibleRepo::new(&state.db.pool)
        .search(&query, translation_id.as_deref(), 100)
        .await
}

/// Cache a passage and append it to a service as a scripture item with
/// auto-generated slides (the cue compiler breaks the cached text into slides).
#[tauri::command]
pub async fn bible_add_to_service(
    state: State<'_, AppState>,
    service_id: String,
    translation_id: String,
    book: String,
    chapter: i64,
    verse_start: Option<i64>,
    verse_end: Option<i64>,
) -> AppResult<ServiceItem> {
    let repo = BibleRepo::new(&state.db.pool);
    let translation = repo
        .list_translations()
        .await?
        .into_iter()
        .find(|t| t.id == translation_id)
        .ok_or_else(|| AppError::Validation("Ukjent oversettelse".into()))?;
    let verses = repo
        .passage(&translation_id, &book, chapter, verse_start, verse_end)
        .await?;
    if verses.is_empty() {
        return Err(AppError::Validation("Ingen vers for referansen".into()));
    }
    // One verse per line so the cue compiler can break sensibly into slides.
    let text = verses
        .iter()
        .map(|v| v.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    let cached = repo
        .cache(
            &book,
            chapter,
            verse_start.unwrap_or(1),
            verse_end,
            &translation.code,
            &text,
        )
        .await?;

    let position: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM service_item WHERE service_id = ?1")
            .bind(&service_id)
            .fetch_one(&state.db.pool)
            .await?;

    ServiceRepo::new(&state.db.pool)
        .add_item(
            &service_id,
            position,
            "scripture",
            None,
            None,
            None,
            Some(&cached.id),
            None,
        )
        .await
}
