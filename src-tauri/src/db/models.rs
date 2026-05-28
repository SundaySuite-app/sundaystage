//! Rust structs mapping the SQLite schema. Mirrors `sql/0001_initial.sql`.
//!
//! Every domain entity derives `serde::{Serialize, Deserialize}` (for IPC)
//! and `sqlx::FromRow` (for query result mapping). The `ts-rs` derives
//! generate TypeScript bindings via `cargo test export_bindings`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use ts_rs::TS;

// ── Library ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/Library.ts")]
pub struct Library {
    pub id: String,
    pub name: String,
    pub default_locale: String,
    pub default_theme_id: Option<String>,
    pub default_template_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/LibraryInput.ts")]
pub struct LibraryInput {
    pub name: String,
    pub default_locale: Option<String>,
}

// ── Person ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/Person.ts")]
pub struct Person {
    pub id: String,
    pub library_id: String,
    pub display_name: String,
    pub sort_name: Option<String>,
    pub external_ids: Option<String>, // JSON
    pub created_at: i64,
    pub updated_at: i64,
}

// ── Tag ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/Tag.ts")]
pub struct Tag {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub color: Option<String>,
}

// ── Song ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/Song.ts")]
pub struct Song {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub ccli_song_id: Option<String>,
    pub tono_work_id: Option<String>,
    pub copyright_notice: Option<String>,
    pub default_key: Option<String>,
    pub tempo_bpm: Option<i64>,
    pub language: String,
    pub last_used_at: Option<i64>,
    /// Per-song theme override (cascade level 2). See `services::theme`.
    pub theme_id: Option<String>,
    /// Per-song template override (cascade level 2).
    pub template_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/SongInput.ts")]
pub struct SongInput {
    pub library_id: String,
    pub title: String,
    pub language: Option<String>,
    pub default_key: Option<String>,
    pub tempo_bpm: Option<i64>,
    pub ccli_song_id: Option<String>,
    pub tono_work_id: Option<String>,
    pub copyright_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/SongSection.ts")]
pub struct SongSection {
    pub id: String,
    pub song_id: String,
    pub label: String,
    pub lyrics: String,
    pub chord_chart: Option<String>,
    pub display_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/SongArrangement.ts")]
pub struct SongArrangement {
    pub id: String,
    pub song_id: String,
    pub name: String,
    pub is_default: i64, // 0/1 — SQLite has no bool
    pub created_at: i64,
    pub updated_at: i64,
}

/// One position in an arrangement's ordered sequence. The same `section_id`
/// may appear at multiple positions (verse → chorus → verse → chorus).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/ArrangementItem.ts")]
pub struct ArrangementItem {
    pub arrangement_id: String,
    pub position: i64,
    pub section_id: String,
}

// ── BibleReference ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/BibleReference.ts")]
pub struct BibleReference {
    pub id: String,
    pub book: String,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: Option<i64>,
    pub translation: String,
    pub text: String,
    pub created_at: i64,
}

// ── Service / ServiceItem ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/Service.ts")]
pub struct Service {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub starts_at: i64,
    pub notes: Option<String>,
    pub state: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/ServiceItem.ts")]
pub struct ServiceItem {
    pub id: String,
    pub service_id: String,
    pub position: i64,
    pub kind: String,
    pub song_id: Option<String>,
    pub arrangement_id: Option<String>,
    pub key_override: Option<String>,
    pub bible_reference_id: Option<String>,
    pub custom_deck_id: Option<String>,
    pub media_asset_id: Option<String>,
    pub notes: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

// ── Theme / Template ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/Theme.ts")]
pub struct Theme {
    pub id: String,
    pub library_id: Option<String>,
    pub name: String,
    pub tokens: String, // JSON
    pub is_builtin: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/Template.ts")]
pub struct Template {
    pub id: String,
    pub library_id: Option<String>,
    pub name: String,
    pub slots: String, // JSON
    pub is_builtin: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

// ── CustomDeck / Slide ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/CustomDeck.ts")]
pub struct CustomDeck {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/Slide.ts")]
pub struct Slide {
    pub id: String,
    pub custom_deck_id: Option<String>,
    pub position: i64,
    pub content: String, // JSON — see ARCHITECTURE.md for shape
    pub theme_id: Option<String>,
    pub template_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

// ── MediaAsset ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/MediaAsset.ts")]
pub struct MediaAsset {
    pub id: String,
    pub library_id: String,
    pub kind: String,
    pub original_path: String,
    pub content_hash: String,
    pub thumbnail_path: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub tags: Option<String>, // JSON array
    pub imported_at: i64,
    pub updated_at: i64,
}

// ── Search result (FTS5) ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../src/lib/bindings/SearchResult.ts")]
pub struct SearchResult {
    pub song_id: String,
    pub title: String,
    pub snippet: String,
    pub rank: f64,
}

/// Helper for converting any `DateTime<Utc>` to unix-ms.
#[allow(dead_code)]
pub fn to_unix_ms(dt: DateTime<Utc>) -> i64 {
    dt.timestamp_millis()
}
