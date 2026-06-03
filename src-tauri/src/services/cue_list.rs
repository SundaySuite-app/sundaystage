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

use crate::db::models::{BibleReference, Service, ServiceItem, Slide, SongSection};
use crate::db::repositories::{ArrangementRepo, ServiceRepo, SongRepo};
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
    BlackOut { cue_id: String },
    /// Show the church logo (configured per library).
    ShowLogo { cue_id: String },
    /// Wait for the operator to advance manually. Most cues are
    /// implicitly this, but explicit `Pause` is useful for transitions
    /// (e.g. "do not auto-advance through the offering").
    Pause { cue_id: String, label: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
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
    /// Phase 12.2 — when true the companion broadcast collapses this slide to a
    /// neutral placeholder so private content (e.g. a pastoral prayer or
    /// communion liturgy) never reaches congregation phones. The main output is
    /// unaffected; this gates only the companion transport. Defaults to false so
    /// pre-12.2 persisted sessions deserialize unchanged.
    #[serde(default)]
    pub sensitive_slide: bool,
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
    pub fn len(&self) -> usize {
        self.cues.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<&Cue> {
        self.cues.get(index)
    }
}

/// A planning-time breakdown of what each service item contributes to the
/// queue. Produced by [`CueCompiler::summarize`] from the *same* per-item
/// compilation the live engine runs, so the numbers shown while planning match
/// the cues that actually play. Drives the "what goes into the queue for which
/// song" view in the service/queue editor.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/CueSummary.ts")]
pub struct CueSummary {
    pub service_id: String,
    pub total_cues: u32,
    pub items: Vec<ServiceItemCues>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/ServiceItemCues.ts")]
pub struct ServiceItemCues {
    pub service_item_id: String,
    /// Schema kind: "song" | "scripture" | "custom_deck" | "gap" | …
    pub kind: String,
    /// Human title: song title, scripture reference, deck name, or the
    /// humanized kind for placeholders.
    pub title: String,
    pub cue_count: u32,
    /// Slide count per section in play order (e.g. Verse 1 → 2, Chorus → 1).
    pub sections: Vec<SectionCueCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/SectionCueCount.ts")]
pub struct SectionCueCount {
    pub label: String,
    pub slide_count: u32,
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
            self.compile_item(&service, item, &mut cues).await?;
        }

        Ok(CueList {
            service_id: service.id.clone(),
            compiled_at: crate::db::now_ms(),
            cues,
        })
    }

    /// Compile a single service item, appending its cues. The single source of
    /// truth for "what one item becomes" — both [`compile`](Self::compile) and
    /// [`summarize`](Self::summarize) go through here so the planning preview
    /// can never drift from what actually plays.
    async fn compile_item(
        &self,
        service: &Service,
        item: &ServiceItem,
        cues: &mut Vec<Cue>,
    ) -> AppResult<()> {
        match item.kind.as_str() {
            "song" => self.compile_song_item(service, item, cues).await,
            "scripture" => self.compile_scripture_item(service, item, cues).await,
            "custom_deck" => self.compile_custom_deck_item(service, item, cues).await,
            "announcement" | "video" | "gap" => {
                // Phase placeholders — surfaced as a Pause cue so the operator
                // advances manually. Label from the item's note when present.
                cues.push(Cue::Pause {
                    cue_id: format!("svc:{}:item:{}:pause", service.id, item.id),
                    label: item
                        .notes
                        .clone()
                        .filter(|n| !n.trim().is_empty())
                        .unwrap_or_else(|| humanize_section_label(&item.kind)),
                });
                Ok(())
            }
            other => Err(AppError::Internal(format!(
                "unknown service_item.kind '{}' for item {}",
                other, item.id
            ))),
        }
    }

    /// Per-item / per-section cue breakdown for the queue editor. Compiles each
    /// item exactly as [`compile`](Self::compile) would, then groups the
    /// resulting cues so the operator sees how many slides each song/section
    /// produces before going live.
    pub async fn summarize(&self, service_id: &str) -> AppResult<CueSummary> {
        let svc_repo = ServiceRepo::new(self.pool);
        let service = svc_repo.get(service_id).await?;
        let items = svc_repo.items(&service.id).await?;

        let mut all: Vec<Cue> = Vec::new();
        let mut out_items = Vec::with_capacity(items.len());
        for item in &items {
            let before = all.len();
            self.compile_item(&service, item, &mut all).await?;
            let slice = &all[before..];
            out_items.push(ServiceItemCues {
                service_item_id: item.id.clone(),
                kind: item.kind.clone(),
                title: self.item_title(item, slice).await?,
                cue_count: slice.len() as u32,
                sections: group_sections(slice),
            });
        }

        Ok(CueSummary {
            service_id: service.id.clone(),
            total_cues: all.len() as u32,
            items: out_items,
        })
    }

    /// Resolve a human title for a service item, reusing its compiled cues
    /// where that's the cheapest source (e.g. the scripture reference).
    async fn item_title(&self, item: &ServiceItem, cues: &[Cue]) -> AppResult<String> {
        Ok(match item.kind.as_str() {
            "song" => match &item.song_id {
                Some(id) => SongRepo::new(self.pool)
                    .get(id)
                    .await
                    .map(|s| s.title)
                    .unwrap_or_else(|_| "Sang".into()),
                None => "Sang".into(),
            },
            "scripture" => first_reference(cues).unwrap_or_else(|| "Skrift".into()),
            "custom_deck" => match &item.custom_deck_id {
                Some(id) => {
                    sqlx::query_scalar::<_, String>("SELECT name FROM custom_deck WHERE id = ?1")
                        .bind(id)
                        .fetch_optional(self.pool)
                        .await?
                        .unwrap_or_else(|| "Lysbilder".into())
                }
                None => "Lysbilder".into(),
            },
            other => humanize_section_label(other),
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
        let song = song_repo.get(song_id).await?;

        // Phase 3.3: if the service item names an arrangement, play its
        // ordered (possibly repeating) sections; otherwise fall back to the
        // song's sections in display_order.
        let sections = match &item.arrangement_id {
            Some(arrangement_id) => {
                ArrangementRepo::new(self.pool)
                    .resolved_sections(arrangement_id)
                    .await?
            }
            None => song_repo.sections(song_id).await?,
        };

        let mut cue_idx: u32 = 0;
        for section in &sections {
            let slides = section_to_slides(section, DEFAULT_LINES_PER_SLIDE);
            for slide_lines in slides {
                cues.push(Cue::ShowSlide {
                    cue_id: format!(
                        "svc:{}:song:{}:s:{}:c:{}",
                        item.service_id, song_id, section.id, cue_idx
                    ),
                    slide_content: SlideContent {
                        section_label: Some(humanize_section_label(&section.label)),
                        text_lines: slide_lines,
                        translation_lines: None,
                        reference: None,
                        sensitive_slide: false,
                    },
                    theme_id: None,
                    template_id: None,
                    source: CueSource {
                        service_item_id: item.id.clone(),
                        item_cue_index: cue_idx,
                        // The song's own title identifies the cue (language-
                        // neutral); the section follows it.
                        display_label: format!(
                            "{} — {}",
                            song.title,
                            humanize_section_label(&section.label)
                        ),
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
        let reference: BibleReference =
            sqlx::query_as::<_, BibleReference>("SELECT * FROM bible_reference WHERE id = ?1")
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
            reference
                .verse_end
                .map(|e| format!("-{}", e))
                .unwrap_or_default(),
        );

        // Norwegian + English break verses differently. For v1 we just
        // chunk by line count. Phase 7.1 wires the per-translation
        // breaking strategy.
        let lines: Vec<String> = reference.text.lines().map(|s| s.to_string()).collect();
        for (cue_idx, chunk) in lines.chunks(DEFAULT_LINES_PER_SLIDE).enumerate() {
            let cue_idx = cue_idx as u32;
            cues.push(Cue::ShowSlide {
                cue_id: format!("svc:{}:scripture:{}:c:{}", item.service_id, ref_id, cue_idx),
                slide_content: SlideContent {
                    section_label: None,
                    text_lines: chunk.to_vec(),
                    translation_lines: None,
                    reference: Some(display.clone()),
                    sensitive_slide: false,
                },
                theme_id: None,
                template_id: None,
                source: CueSource {
                    service_item_id: item.id.clone(),
                    item_cue_index: cue_idx,
                    // The reference ("John 3:16-17") is already self-identifying.
                    display_label: display.clone(),
                },
            });
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

        let deck_name: String = sqlx::query_scalar("SELECT name FROM custom_deck WHERE id = ?1")
            .bind(deck_id)
            .fetch_optional(self.pool)
            .await?
            .unwrap_or_default();

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
                    sensitive_slide: false,
                },
                theme_id: slide.theme_id.clone(),
                template_id: slide.template_id.clone(),
                source: CueSource {
                    service_item_id: item.id.clone(),
                    item_cue_index: cue_idx as u32,
                    // Deck name + slide number; the name carries the identity.
                    display_label: if deck_name.is_empty() {
                        (cue_idx + 1).to_string()
                    } else {
                        format!("{} — {}", deck_name, cue_idx + 1)
                    },
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
/// schema is documented in `docs/ARCHITECTURE.md` and produced by
/// `services::slide_doc::SlideDoc`; we accept the minimal
/// `{ "blocks": [{ "type": "text", "text": "..." }] }` here and fall back
/// to empty. `pub(crate)` so `slide_doc`'s cross-consistency test can lock
/// the editor↔engine contract.
pub(crate) fn extract_text_lines_from_content(content: &str) -> Option<Vec<String>> {
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
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Collapse a run of cues into per-section slide counts, in play order. A
/// section label comes from the slide's `section_label`, falling back to its
/// `reference` (scripture) or a label for non-slide cues.
fn group_sections(cues: &[Cue]) -> Vec<SectionCueCount> {
    let mut out: Vec<SectionCueCount> = Vec::new();
    for cue in cues {
        let label = match cue {
            Cue::ShowSlide { slide_content, .. } => slide_content
                .section_label
                .clone()
                .or_else(|| slide_content.reference.clone())
                .unwrap_or_else(|| "—".into()),
            Cue::Pause { label, .. } => label.clone(),
            Cue::BlackOut { .. } => "Blackout".into(),
            Cue::ShowLogo { .. } => "Logo".into(),
        };
        match out.last_mut() {
            Some(last) if last.label == label => last.slide_count += 1,
            _ => out.push(SectionCueCount {
                label,
                slide_count: 1,
            }),
        }
    }
    out
}

/// The first slide reference in a run of cues (used to title scripture items).
fn first_reference(cues: &[Cue]) -> Option<String> {
    cues.iter().find_map(|c| match c {
        Cue::ShowSlide { slide_content, .. } => slide_content.reference.clone(),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{LibraryInput, SongInput};
    use crate::db::repositories::{ArrangementRepo, LibraryRepo, ServiceRepo, SongRepo};
    use crate::db::Database;
    use crate::db::{new_id, now_ms};

    async fn fixture_library_song(db: &Database) -> (String, String) {
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "Test".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        let song = SongRepo::new(&db.pool)
            .create(SongInput {
                library_id: lib.id.clone(),
                title: "Amazing Grace".into(),
                language: Some("en".into()),
                default_key: None,
                tempo_bpm: None,
                ccli_song_id: None,
                tono_work_id: None,
                copyright_notice: Some("Public Domain".into()),
            })
            .await
            .unwrap();
        SongRepo::new(&db.pool)
            .add_section(&song.id, "verse_1",
                "Amazing grace how sweet the sound\nThat saved a wretch like me\nI once was lost but now am found\nWas blind but now I see").await.unwrap();
        SongRepo::new(&db.pool)
            .add_section(&song.id, "chorus", "Praise the Lord\nPraise His name")
            .await
            .unwrap();
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
            id: "x".into(),
            song_id: "y".into(),
            label: "verse_1".into(),
            lyrics: "a\nb\nc\nd\ne".into(),
            chord_chart: None,
            display_order: 0,
            created_at: 0,
            updated_at: 0,
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
            .await
            .unwrap();
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
        .await
        .unwrap();

        let compiler = CueCompiler::new(&db.pool);
        let cl = compiler.compile(&svc.id).await.unwrap();
        assert!(!cl.is_empty(), "compiled CueList must have cues");

        // verse_1 has 4 lines → 1 slide @ 4 lines per slide.
        // chorus has 2 lines → 1 slide.
        // Total: 2 ShowSlide cues.
        assert_eq!(cl.len(), 2);

        match &cl.cues[0] {
            Cue::ShowSlide {
                slide_content,
                source,
                ..
            } => {
                assert_eq!(slide_content.section_label.as_deref(), Some("Verse 1"));
                assert_eq!(slide_content.text_lines.len(), 4);
                assert_eq!(source.display_label, "Amazing Grace — Verse 1");
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
    async fn compile_song_uses_arrangement_when_set() {
        let db = Database::open_in_memory().await.unwrap();
        let (lib_id, song_id) = fixture_library_song(&db).await;

        let sections = SongRepo::new(&db.pool).sections(&song_id).await.unwrap();
        let verse = sections
            .iter()
            .find(|s| s.label == "verse_1")
            .unwrap()
            .id
            .clone();
        let chorus = sections
            .iter()
            .find(|s| s.label == "chorus")
            .unwrap()
            .id
            .clone();

        let arr_repo = ArrangementRepo::new(&db.pool);
        let arr = arr_repo.create(&song_id, "Full").await.unwrap();
        // verse → chorus → chorus (chorus repeats)
        arr_repo
            .set_items(&arr.id, &[verse, chorus.clone(), chorus])
            .await
            .unwrap();

        let svc = ServiceRepo::new(&db.pool)
            .create(&lib_id, "Svc", now_ms())
            .await
            .unwrap();
        let item_id = new_id();
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO service_item (id, service_id, position, kind, song_id,
                arrangement_id, key_override, bible_reference_id, custom_deck_id,
                media_asset_id, notes, created_at, updated_at)
            VALUES (?1, ?2, 0, 'song', ?3, ?4, NULL, NULL, NULL, NULL, NULL, ?5, ?5)
            "#,
        )
        .bind(&item_id)
        .bind(&svc.id)
        .bind(&song_id)
        .bind(&arr.id)
        .bind(now)
        .execute(&db.pool)
        .await
        .unwrap();

        let cl = CueCompiler::new(&db.pool).compile(&svc.id).await.unwrap();
        // verse_1 (4 lines → 1 slide) + chorus (2 → 1) + chorus (2 → 1) = 3 cues.
        assert_eq!(cl.len(), 3);
        match &cl.cues[0] {
            Cue::ShowSlide { slide_content, .. } => {
                assert_eq!(slide_content.section_label.as_deref(), Some("Verse 1"));
            }
            _ => panic!("expected ShowSlide"),
        }
        match &cl.cues[2] {
            Cue::ShowSlide { slide_content, .. } => {
                assert_eq!(slide_content.section_label.as_deref(), Some("Chorus"));
            }
            _ => panic!("expected ShowSlide"),
        }
    }

    #[tokio::test]
    async fn compile_scripture_produces_cues_with_reference_text() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "Test".into(),
                default_locale: None,
            })
            .await
            .unwrap();

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
            .create(&lib.id, "Scripture service", now)
            .await
            .unwrap();

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
        .execute(&db.pool)
        .await
        .unwrap();

        let cl = CueCompiler::new(&db.pool).compile(&svc.id).await.unwrap();
        assert_eq!(cl.len(), 1, "4 lines → 1 slide @ 4 lines per slide");
        match &cl.cues[0] {
            Cue::ShowSlide {
                slide_content,
                source,
                ..
            } => {
                assert_eq!(slide_content.reference.as_deref(), Some("John 3:16-17"));
                assert_eq!(slide_content.text_lines.len(), 4);
                assert_eq!(source.display_label, "John 3:16-17");
            }
            _ => panic!("expected ShowSlide"),
        }
    }

    #[tokio::test]
    async fn summarize_groups_cues_per_item_and_section() {
        let db = Database::open_in_memory().await.unwrap();
        let (lib_id, song_id) = fixture_library_song(&db).await;

        let svc = ServiceRepo::new(&db.pool)
            .create(&lib_id, "Test service", now_ms())
            .await
            .unwrap();
        ServiceRepo::new(&db.pool)
            .add_item(&svc.id, 0, "song", Some(&song_id), None, None, None, None)
            .await
            .unwrap();

        let summary = CueCompiler::new(&db.pool).summarize(&svc.id).await.unwrap();
        // verse_1 (4 lines → 1 slide) + chorus (2 → 1) = 2 cues for the one song.
        assert_eq!(summary.total_cues, 2);
        assert_eq!(summary.items.len(), 1);
        let item = &summary.items[0];
        assert_eq!(item.kind, "song");
        assert_eq!(item.title, "Amazing Grace");
        assert_eq!(item.cue_count, 2);
        assert_eq!(
            item.sections
                .iter()
                .map(|s| (s.label.as_str(), s.slide_count))
                .collect::<Vec<_>>(),
            vec![("Verse 1", 1), ("Chorus", 1)],
        );
    }

    #[tokio::test]
    async fn compile_empty_service_produces_empty_cue_list() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "Empty".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        let svc = ServiceRepo::new(&db.pool)
            .create(&lib.id, "Empty service", now_ms())
            .await
            .unwrap();
        let cl = CueCompiler::new(&db.pool).compile(&svc.id).await.unwrap();
        assert!(cl.is_empty());
    }
}
