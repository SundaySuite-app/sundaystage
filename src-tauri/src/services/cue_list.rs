//! Phase 5.1 — CueList compiler.
//!
//! Takes a Service entity + its items + their songs/scripture/decks
//! and produces a flat ordered list of cues that the live engine
//! advances through.
//!
//! Mental model: a Service is the "score" (high-level intent). The
//! CueList is the executable program. The live engine is a stepper —
//! it never reasons about Songs or arrangements, only Cues.
//!
//! This separation is what lets us:
//!   1. Compile once at "Go Live" time and never re-query during a
//!      service (the runtime is decoupled from the database).
//!   2. Persist the compiled CueList to disk for crash recovery — on
//!      restart the engine resumes the same cue index.
//!   3. Generate SRT captions post-service from the cue timing log
//!      (Phase 10.2).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::models::{Service, ServiceItem, SongSection, BibleReference, Slide};
use crate::db::repositories::{ServiceRepo, SongRepo, BibleRepo};
use crate::error::{AppError, AppResult};
use sqlx::SqlitePool;

/// A single executable step in the live timeline. Discriminated by `kind`.
///
/// JSON-friendly because the engine persists the compiled list and the
/// renderer reads it for the operator UI.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/Cue.ts")]
pub enum Cue {
    /// Show a slide on the main output.
    ShowSlide {
        cue_id: String,
        slide_content: SlideContent,
        /// Optional theme/template override resolved at compile time.
        theme_id: Option<String>,
        template_id: Option<String>,
        /// Back-reference for the operator UI.
        source: CueSource,
    },
    /// Pure black output — operator hotkey 'Esc' fires this.
    BlackOut {
        cue_id: String,
    },
    /// Show the church logo (configured per library).
    ShowLogo {
        cue_id: String,
    },
    /// Wait for the operator to advance manually. Most cues are
    /// implicitly this, but explicit `Pause` is useful for transitions
    /// (e.g. "do not auto-advance through the offering").
    Pause {
        cue_id: String,
        label: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/SlideContent.ts")]
pub struct SlideContent {
    /// Optional section label shown on the stage display
    /// (e.g. "Verse 1", "Chorus"). Musicians need this.
    pub section_label: Option<String>,
    /// The actual text shown on the main output. Lines are pre-broken
    /// for the current slide.
    pub text_lines: Vec<String>,
    /// Optional secondary line (translation) shown below the primary
    /// line — Phase 11.2 fills this in.
    pub translation_lines: Option<Vec<String>>,
    /// Reference text shown small (e.g. "John 3:16").
    pub reference: Option<String>,
}

/// Where in the source data this cue came from. Used by the operator UI
/// to highlight "you are here" in the service plan.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/CueSource.ts")]
pub struct CueSource {
    pub service_item_id: String,
    /// 0-based index of this cue within the service-item it came from.
    pub item_cue_index: u32,
    /// Display label for the operator UI: "Amazing Grace — Verse 2".
    pub display_label: String,
}

/// A compiled live session. Persisted to disk on "Go Live"; survives
/// renderer crashes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/CueList.ts")]
pub struct CueList {
    pub service_id: String,
    pub compiled_at: i64,
    pub cues: Vec<Cue>,
}

impl CueList {
    pub fn len(&self) -> usize { self.cues.len() }
    pub fn is_empty(&self) -> bool { self.cues.is_empty() }
    pub fn get(&self, index: usize) -> Option<&Cue> { self.cues.get(index) }
}

/// Lines-per-slide budget — defaults are conservative; user can override
/// in song-section settings. AI auto-break in Phase 4 produces a better
/// breakdown than this naïve splitter.
const DEFAULT_LINES_PER_SLIDE: usize = 4;

pub struct CueCompiler<'a> {
    pool: &'a SqlitePool,
}

impl<'a> CueCompiler<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Compile a Service into a CueList. This is the only entry point
    /// the live engine calls.
    pub async fn compile(&self, service_id: &str) -> AppResult<CueList> {
        let svc_repo = ServiceRepo::new(self.pool);
        let service = svc_repo.get(service_id).await?;
        let items = svc_repo.items(&service.id).await?;

        let mut cues: Vec<Cue> = Vec::with_capacity(items.len() * 4);

        for item in &items {
            match item.kind.as_str() {
                "song" => self.compile_song_item(&service, item, &mut cues).await?,
                "scripture" => self.compile_scripture_item(&service, item, &mut cues).await?,
                "custom_deck" => self.compile_custom_deck_item(&service, item, &mut cues).await?,
                "announcement" | "video" | "gap" => {
                    // Phase placeholders — we'll surface these as Pause
                    // cues so the operator can advance manually.
                    cues.push(Cue::Pause {
                        cue_id: format!("svc:{}:item:{}:pause", service.id, item.id),
                        label: format!("{} — {}", item.kind, item.id),
                    });
                }
                other => {
                    return Err(AppError::Internal(format!(
                        "unknown service_item.kind '{}' for item {}",
                        other, item.id
                    )));
                }
            }
        }

        Ok(CueList {
            service_id: service.id.clone(),
            compiled_at: crate::db::now_ms(),
            cues,
        })
    }

    /// Walk a song's arrangement → sections → slides.
    async fn compile_song_item(
        &self,
        _service: &Service,
        item: &ServiceItem,
        cues: &mut Vec<Cue>,
    ) -> AppResult<()> {
        let Some(ref song_id) = item.song_id else {
            return Err(AppError::Validation(format!(
                "service_item {} has kind=song but no song_id",
                item.id
            )));
        };
        let song_repo = SongRepo::new(self.pool);
        let _song = song_repo.get(song_id).await?;
        let sections = song_repo.sections(song_id).await?;

        // Phase 1.2 limit: no explicit `SongArrangement.id` resolution yet
        // — we play sections in their display_order. Phase 3.3 wires the
        // `arrangement_id` lookup so the user's chosen arrangement is used.
        let mut cue_idx: u32 = 0;
        for section in &sections {
            let slides = section_to_slides(section, DEFAULT_LINES_PER_SLIDE);
            for slide_lines in slides {
                cues.push(Cue::ShowSlide {
                    cue_id: format!("svc:{}:song:{}:s:{}:c:{}",
                        item.service_id, song_id, section.id, cue_idx),
                    slide_content: SlideContent {
                        section_label: Some(humanize_section_label(&section.label)),
                        text_lines: slide_lines,
                        translation_lines: None,
                        reference: None,
                    },
                    theme_id: None,
                    template_id: None,
                    source: CueSource {
                        service_item_id: item.id.clone(),
                        item_cue_index: cue_idx,
                        display_label: format!("Sang — {}", humanize_section_label(&section.label)),
                    },
                });
                cue_idx += 1;
            }
        }
        Ok(())
    }

    /// Walk a scripture reference's verses into one or more slides.
    async fn compile_scripture_item(
        &self,
        _service: &Service,
        item: &ServiceItem,
        cues: &mut Vec<Cue>,
    ) -> AppResult<()> {
        let Some(ref ref_id) = item.bible_reference_id else {
            return Err(AppError::Validation(format!(
                "service_item {} has kind=scripture but no bible_reference_id",
                item.id
            )));
        };
        // We cached the reference text on insert (Phase 7); just fetch it.
        let reference: BibleReference = sqlx::query_as::<_, BibleReference>(
            "SELECT * FROM bible_reference WHERE id = ?1",
        )
        .bind(ref_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound {
            entity: "bible_reference",
            id: ref_id.clone(),
        })?;

        let display = format!(
            "{} {}:{}{}",
            reference.book,
            reference.chapter,
            reference.verse_start,
            reference.verse_end.map(|e| format!("-{}", e)).unwrap_or_default(),
        );

        // Norwegian + English break verses differently. For v1 we just
        // chunk by line count. Phase 7.1 wires the per-translation
        // breaking strategy.
        let lines: Vec<String> = reference.text.lines().map(|s| s.to_string()).collect();
        let mut cue_idx: u32 = 0;
        for chunk in lines.chunks(DEFAULT_LINES_PER_SLIDE) {
            cues.push(Cue::ShowSlide {
                cue_id: format!("svc:{}:scripture:{}:c:{}", item.service_id, ref_id, cue_idx),
                slide_content: SlideContent {
                    section_label: None,
                    text_lines: chunk.to_vec(),
                    translation_lines: None,
                    reference: Some(display.clone()),
                },
                theme_id: None,
                template_id: None,
                source: CueSource {
                    service_item_id: item.id.clone(),
                    item_cue_index: cue_idx,
                    display_label: format!("Bibel — {}", display),
                },
            });
            cue_idx += 1;
        }
        Ok(())
    }

    /// Walk a custom deck's pre-authored slides.
    async fn compile_custom_deck_item(
        &self,
        _service: &Service,
        item: &ServiceItem,
        cues: &mut Vec<Cue>,
    ) -> AppResult<()> {
        let Some(ref deck_id) = item.custom_deck_id else {
            return Err(AppError::Validation(format!(
                "service_item {} has kind=custom_deck but no custom_deck_id",
                item.id
            )));
        };

        let slides: Vec<Slide> = sqlx::query_as::<_, Slide>(
            "SELECT * FROM slide WHERE custom_deck_id = ?1 ORDER BY position",
        )
        .bind(deck_id)
        .fetch_all(self.pool)
        .await?;

        for (cue_idx, slide) in slides.iter().enumerate() {
            // Parse slide.content JSON for text — Phase 3.1 fleshes out
            // the full block model; for now we extract the first text
            // block's lines.
            let lines = extract_text_lines_from_content(&slide.content).unwrap_or_default();
            cues.push(Cue::ShowSlide {
                cue_id: format!("svc:{}:deck:{}:c:{}", item.service_id, deck_id, cue_idx),
                slide_content: SlideContent {
                    section_label: None,
                    text_lines: lines,
                    translation_lines: None,
                    reference: None,
                },
                theme_id: slide.theme_id.clone(),
                template_id: slide.template_id.clone(),
                source: CueSource {
                    service_item_id: item.id.clone(),
                    item_cue_index: cue_idx as u32,
                    display_label: format!("Deck — Slide {}", cue_idx + 1),
                },
            });
        }
        Ok(())
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Split a section's lyrics into slides of at most `lines_per_slide`
/// lines. Naïve — Phase 4's AI breaker is smarter.
pub fn section_to_slides(section: &SongSection, lines_per_slide: usize) -> Vec<Vec<String>> {
    let all_lines: Vec<String> = section
        .lyrics
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .collect();
    if all_lines.is_empty() {
        return vec![];
    }
    all_lines
        .chunks(lines_per_slide.max(1))
        .map(|c| c.to_vec())
        .collect()
}

/// Turn raw section labels into something the stage display shows.
/// "verse_1" → "Verse 1", "chorus" → "Chorus".
pub fn humanize_section_label(label: &str) -> String {
    label
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract `text_lines` from the `slide.content` JSON blob. The full
/// schema is documented in `docs/ARCHITECTURE.md`; we accept the
/// minimal `{ "blocks": [{ "type": "text", "text": "..." }] }` here
/// and fall back to empty.
fn extract_text_lines_from_content(content: &str) -> Option<Vec<String>> {
    let v: serde_json::Value = serde_json::from_str(content).ok()?;
    let blocks = v.get("blocks")?.as_array()?;
    let mut out: Vec<String> = Vec::new();
    for b in blocks {
        if b.get("type")?.as_str() == Some("text") {
            if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                out.extend(t.lines().map(|s| s.to_string()));
            }
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::models::{LibraryInput, SongInput};
    use crate::db::repositories::{LibraryRepo, SongRepo, ServiceRepo};
    use crate::db::{new_id, now_ms};

    async fn fixture_library_song(db: &Database) -> (String, String) {
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput { name: "Test".into(), default_locale: None })
            .await.unwrap();
        let song = SongRepo::new(&db.pool)
            .create(SongInput {
                library_id: lib.id.clone(),
                title: "Amazing Grace".into(),
                language: Some("en".into()),
                default_key: None, tempo_bpm: None,
                ccli_song_id: None, tono_work_id: None,
                copyright_notice: Some("Public Domain".into()),
            })
            .await.unwrap();
        SongRepo::new(&db.pool)
            .add_section(&song.id, "verse_1",
                "Amazing grace how sweet the sound\nThat saved a wretch like me\nI once was lost but now am found\nWas blind but now I see").await.unwrap();
        SongRepo::new(&db.pool)
            .add_section(&song.id, "chorus", "Praise the Lord\nPraise His name").await.unwrap();
        (lib.id, song.id)
    }

    #[tokio::test]
    async fn humanize_label() {
        assert_eq!(humanize_section_label("verse_1"), "Verse 1");
        assert_eq!(humanize_section_label("chorus"), "Chorus");
        assert_eq!(humanize_section_label("pre_chorus"), "Pre Chorus");
    }

    #[tokio::test]
    async fn section_splits_lines() {
        let s = SongSection {
            id: "x".into(), song_id: "y".into(),
            label: "verse_1".into(),
            lyrics: "a\nb\nc\nd\ne".into(),
            chord_chart: None, display_order: 0,
            created_at: 0, updated_at: 0,
        };
        let slides = section_to_slides(&s, 2);
        assert_eq!(slides.len(), 3);
        assert_eq!(slides[0], vec!["a", "b"]);
        assert_eq!(slides[2], vec!["e"]);
    }

    #[tokio::test]
    async fn compile_song_service_produces_cues() {
        let db = Database::open_in_memory().await.unwrap();
        let (lib_id, song_id) = fixture_library_song(&db).await;

        // Build a service with one song item
        let svc = ServiceRepo::new(&db.pool)
            .create(&lib_id, "Test service", now_ms())
            .await.unwrap();
        let item_id = new_id();
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO service_item (id, service_id, position, kind, song_id,
                arrangement_id, key_override, bible_reference_id, custom_deck_id,
                media_asset_id, notes, created_at, updated_at)
            VALUES (?1, ?2, 0, 'song', ?3, NULL, NULL, NULL, NULL, NULL, NULL, ?4, ?4)
            "#,
        )
        .bind(&item_id)
        .bind(&svc.id)
        .bind(&song_id)
        .bind(now)
        .execute(&db.pool)
        .await.unwrap();

        let compiler = CueCompiler::new(&db.pool);
        let cl = compiler.compile(&svc.id).await.unwrap();
        assert!(!cl.is_empty(), "compiled CueList must have cues");

        // verse_1 has 4 lines → 1 slide @ 4 lines per slide.
        // chorus has 2 lines → 1 slide.
        // Total: 2 ShowSlide cues.
        assert_eq!(cl.len(), 2);

        match &cl.cues[0] {
            Cue::ShowSlide { slide_content, source, .. } => {
                assert_eq!(slide_content.section_label.as_deref(), Some("Verse 1"));
                assert_eq!(slide_content.text_lines.len(), 4);
                assert_eq!(source.display_label, "Sang — Verse 1");
            }
            _ => panic!("expected ShowSlide cue"),
        }
        match &cl.cues[1] {
            Cue::ShowSlide { slide_content, .. } => {
                assert_eq!(slide_content.section_label.as_deref(), Some("Chorus"));
                assert_eq!(slide_content.text_lines.len(), 2);
            }
            _ => panic!("expected ShowSlide cue"),
        }
    }

    #[tokio::test]
    async fn compile_scripture_produces_cues_with_reference_text() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput { name: "Test".into(), default_locale: None })
            .await.unwrap();

        // Insert a cached bible reference
        let ref_id = new_id();
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO bible_reference (id, book, chapter, verse_start, verse_end, translation, text, created_at)
            VALUES (?1, 'John', 3, 16, 17, 'NIV', ?2, ?3)
            "#,
        )
        .bind(&ref_id)
        .bind("For God so loved the world\nthat he gave his one and only Son\nthat whoever believes in him\nshall not perish but have eternal life")
        .bind(now)
        .execute(&db.pool).await.unwrap();

        let svc = ServiceRepo::new(&db.pool)
            .create(&lib.id, "Scripture service", now).await.unwrap();

        let item_id = new_id();
        sqlx::query(
            r#"
            INSERT INTO service_item (id, service_id, position, kind,
              song_id, arrangement_id, key_override, bible_reference_id,
              custom_deck_id, media_asset_id, notes, created_at, updated_at)
            VALUES (?1, ?2, 0, 'scripture', NULL, NULL, NULL, ?3, NULL, NULL, NULL, ?4, ?4)
            "#,
        )
        .bind(&item_id)
        .bind(&svc.id)
        .bind(&ref_id)
        .bind(now)
        .execute(&db.pool).await.unwrap();

        let cl = CueCompiler::new(&db.pool).compile(&svc.id).await.unwrap();
        assert_eq!(cl.len(), 1, "4 lines → 1 slide @ 4 lines per slide");
        match &cl.cues[0] {
            Cue::ShowSlide { slide_content, source, .. } => {
                assert_eq!(slide_content.reference.as_deref(), Some("John 3:16-17"));
                assert_eq!(slide_content.text_lines.len(), 4);
                assert_eq!(source.display_label, "Bibel — John 3:16-17");
            }
            _ => panic!("expected ShowSlide"),
        }
    }

    #[tokio::test]
    async fn compile_empty_service_produces_empty_cue_list() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput { name: "Empty".into(), default_locale: None })
            .await.unwrap();
        let svc = ServiceRepo::new(&db.pool)
            .create(&lib.id, "Empty service", now_ms()).await.unwrap();
        let cl = CueCompiler::new(&db.pool).compile(&svc.id).await.unwrap();
        assert!(cl.is_empty());
    }
}
