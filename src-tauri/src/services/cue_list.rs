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

use std::collections::HashMap;

use crate::db::models::{BibleReference, Service, ServiceItem, Slide, SongSection};
use crate::db::repositories::{ArrangementRepo, LibraryRepo, ServiceRepo, SongRepo, ThemeRepo};
use crate::error::{AppError, AppResult};
use crate::services::scripture_break;
use crate::services::theme::{
    layout_for, resolve_template_id, resolve_theme_id, slide_appearance_from, tokens_for,
    SlideAppearance, TemplateLayout, ThemeTokens,
};
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
        // Boxed: SlideContent carries the cascade-resolved appearance, which
        // made this variant ~320 B vs ~48 B for the rest — wasteful in the
        // `Vec<Cue>` the live session moves through on every advance. `Box<T>`
        // serializes transparently, so the JSONL session store is unchanged.
        slide_content: Box<SlideContent>,
        /// The CONCRETE theme/template resolved through the cascade
        /// (slide → song → library default → built-in) at compile time —
        /// `Some` on every compiled cue; `None` only in pre-cascade persisted
        /// sessions. The *resolved look* itself rides on
        /// `slide_content.appearance` so the output needs no lookup.
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
    /// Audit 2c — the cascade-resolved per-cue look (theme colours/font +
    /// template alignment/scale), embedded at compile time so the live output
    /// styles each cue without a DB. `None` = nothing in the cascade was
    /// explicitly chosen → the output falls back to the operator's global
    /// `OutputAppearance` (and old persisted sessions deserialize unchanged).
    #[serde(default)]
    #[ts(optional)]
    pub appearance: Option<SlideAppearance>,
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

/// Everything the theme/template cascade needs at compile time, loaded once
/// per compilation (audit 2c): the library's defaults plus the id→tokens /
/// id→layout catalogues `tokens_for`/`layout_for` resolve against. Compiling
/// once at "Go Live" means the live engine never queries themes mid-service.
struct CascadeCtx {
    library_theme_id: Option<String>,
    library_template_id: Option<String>,
    themes: HashMap<String, ThemeTokens>,
    templates: HashMap<String, TemplateLayout>,
}

impl CascadeCtx {
    /// Resolve one slide's cascade → (concrete theme id, concrete template id,
    /// per-cue appearance). The appearance is `Some` only when *some* level of
    /// the cascade was explicitly chosen — with nothing chosen the cue inherits
    /// the operator's global `OutputAppearance` (today's behaviour), instead of
    /// the built-in default theme silently overriding the operator's settings.
    fn resolve(
        &self,
        slide_theme: &Option<String>,
        slide_template: &Option<String>,
        song_theme: &Option<String>,
        song_template: &Option<String>,
    ) -> (String, String, Option<SlideAppearance>) {
        let theme_id = resolve_theme_id(slide_theme, song_theme, &self.library_theme_id);
        let template_id =
            resolve_template_id(slide_template, song_template, &self.library_template_id);
        let explicit = [slide_theme, song_theme, &self.library_theme_id]
            .iter()
            .any(|o| o.is_some())
            || [slide_template, song_template, &self.library_template_id]
                .iter()
                .any(|o| o.is_some());
        let appearance = explicit.then(|| {
            slide_appearance_from(
                &tokens_for(&theme_id, &self.themes),
                &layout_for(&template_id, &self.templates),
            )
        });
        (theme_id, template_id, appearance)
    }
}

impl<'a> CueCompiler<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Load the cascade context for a library: its default theme/template ids
    /// and the full id→tokens/layout catalogues (built-ins ∪ library rows).
    /// A malformed stored row simply drops out of the catalogue — `tokens_for`
    /// /`layout_for` then degrade to the built-in default rather than failing
    /// a Go-Live.
    async fn cascade_ctx(&self, library_id: &str) -> AppResult<CascadeCtx> {
        let lib = LibraryRepo::new(self.pool).get(library_id).await?;
        let repo = ThemeRepo::new(self.pool);
        let mut themes = HashMap::new();
        for t in repo.list_themes(library_id).await? {
            if let Ok(tokens) = serde_json::from_str::<ThemeTokens>(&t.tokens) {
                themes.insert(t.id, tokens);
            }
        }
        let mut templates = HashMap::new();
        for t in repo.list_templates(library_id).await? {
            if let Ok(layout) = serde_json::from_str::<TemplateLayout>(&t.slots) {
                templates.insert(t.id, layout);
            }
        }
        Ok(CascadeCtx {
            library_theme_id: lib.default_theme_id,
            library_template_id: lib.default_template_id,
            themes,
            templates,
        })
    }

    /// Compile a Service into a CueList. This is the only entry point
    /// the live engine calls.
    pub async fn compile(&self, service_id: &str) -> AppResult<CueList> {
        let svc_repo = ServiceRepo::new(self.pool);
        let service = svc_repo.get(service_id).await?;
        let items = svc_repo.items(&service.id).await?;
        let ctx = self.cascade_ctx(&service.library_id).await?;

        let mut cues: Vec<Cue> = Vec::with_capacity(items.len() * 4);

        for item in &items {
            self.compile_item(&service, item, &ctx, &mut cues).await?;
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
        ctx: &CascadeCtx,
        cues: &mut Vec<Cue>,
    ) -> AppResult<()> {
        match item.kind.as_str() {
            "song" => self.compile_song_item(service, item, ctx, cues).await,
            "scripture" => self.compile_scripture_item(service, item, ctx, cues).await,
            "custom_deck" => {
                self.compile_custom_deck_item(service, item, ctx, cues)
                    .await
            }
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
        let ctx = self.cascade_ctx(&service.library_id).await?;

        let mut all: Vec<Cue> = Vec::new();
        let mut out_items = Vec::with_capacity(items.len());
        for item in &items {
            let before = all.len();
            self.compile_item(&service, item, &ctx, &mut all).await?;
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
        ctx: &CascadeCtx,
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

        // Cascade (audit 2c): song sections have no slide-level override, so
        // the chain is song → library default → built-in.
        let (theme_id, template_id, appearance) =
            ctx.resolve(&None, &None, &song.theme_id, &song.template_id);

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
                    slide_content: Box::new(SlideContent {
                        section_label: Some(humanize_section_label(&section.label)),
                        text_lines: slide_lines,
                        translation_lines: None,
                        reference: None,
                        sensitive_slide: false,
                        appearance: appearance.clone(),
                    }),
                    theme_id: Some(theme_id.clone()),
                    template_id: Some(template_id.clone()),
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
        ctx: &CascadeCtx,
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

        // Cascade (audit 2c): scripture has no slide/song level — the chain is
        // library default → built-in.
        let (theme_id, template_id, appearance) = ctx.resolve(&None, &None, &None, &None);

        // Verse-aware auto-break: keep whole verses together within the line
        // budget (only an over-long single verse spills across slides),
        // preserving verse order across chapters. See `scripture_break`. The
        // reference label rides on every produced slide.
        let verses = scripture_break::verses_from_reference(&reference);
        let slides = scripture_break::break_passage(&verses, &display, DEFAULT_LINES_PER_SLIDE);
        for (cue_idx, slide) in slides.into_iter().enumerate() {
            let cue_idx = cue_idx as u32;
            cues.push(Cue::ShowSlide {
                cue_id: format!("svc:{}:scripture:{}:c:{}", item.service_id, ref_id, cue_idx),
                slide_content: Box::new(SlideContent {
                    section_label: None,
                    text_lines: slide.lines,
                    translation_lines: None,
                    reference: Some(slide.reference_label),
                    sensitive_slide: false,
                    appearance: appearance.clone(),
                }),
                theme_id: Some(theme_id.clone()),
                template_id: Some(template_id.clone()),
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
        ctx: &CascadeCtx,
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
            // Cascade (audit 2c): a deck slide carries its own slide-level
            // override (highest precedence); no song level — the chain is
            // slide → library default → built-in.
            let (theme_id, template_id, appearance) =
                ctx.resolve(&slide.theme_id, &slide.template_id, &None, &None);
            cues.push(Cue::ShowSlide {
                cue_id: format!("svc:{}:deck:{}:c:{}", item.service_id, deck_id, cue_idx),
                slide_content: Box::new(SlideContent {
                    section_label: None,
                    text_lines: lines,
                    translation_lines: None,
                    reference: None,
                    sensitive_slide: false,
                    appearance,
                }),
                theme_id: Some(theme_id),
                template_id: Some(template_id),
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
    async fn compile_scripture_breaks_verse_aware_into_slide_sequence() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "Test".into(),
                default_locale: None,
            })
            .await
            .unwrap();

        // Six one-line verses (Psalms 23:1-6), cached one verse per line — the
        // canonical shape `bible_add_to_service` writes. At the default budget
        // of 4 lines this must produce [v1..v4], [v5,v6] → exactly 2 cues, and
        // the reference must ride on every slide.
        let ref_id = new_id();
        let now = now_ms();
        sqlx::query(
            r#"
            INSERT INTO bible_reference (id, book, chapter, verse_start, verse_end, translation, text, created_at)
            VALUES (?1, 'Psalms', 23, 1, 6, 'KJV', ?2, ?3)
            "#,
        )
        .bind(&ref_id)
        .bind("verse one\nverse two\nverse three\nverse four\nverse five\nverse six")
        .bind(now)
        .execute(&db.pool)
        .await
        .unwrap();

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
        assert_eq!(cl.len(), 2, "6 verses @ 4 lines → [v1..v4], [v5,v6]");

        match &cl.cues[0] {
            Cue::ShowSlide { slide_content, .. } => {
                assert_eq!(
                    slide_content.text_lines,
                    vec!["verse one", "verse two", "verse three", "verse four"]
                );
                assert_eq!(slide_content.reference.as_deref(), Some("Psalms 23:1-6"));
            }
            _ => panic!("expected ShowSlide"),
        }
        match &cl.cues[1] {
            Cue::ShowSlide { slide_content, .. } => {
                assert_eq!(slide_content.text_lines, vec!["verse five", "verse six"]);
                assert_eq!(slide_content.reference.as_deref(), Some("Psalms 23:1-6"));
            }
            _ => panic!("expected ShowSlide"),
        }

        // Reference label present on every produced slide.
        assert!(cl.cues.iter().all(|c| matches!(
            c,
            Cue::ShowSlide { slide_content, .. }
                if slide_content.reference.as_deref() == Some("Psalms 23:1-6")
        )));
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

    // ── theme/template cascade on compiled cues (audit 2c) ───────────────────

    /// Every ShowSlide cue's (theme_id, appearance.bg) pair, for assertions.
    fn cue_themes(cl: &CueList) -> Vec<(Option<String>, Option<String>)> {
        cl.cues
            .iter()
            .filter_map(|c| match c {
                Cue::ShowSlide {
                    theme_id,
                    slide_content,
                    ..
                } => Some((
                    theme_id.clone(),
                    slide_content.appearance.as_ref().and_then(|a| a.bg.clone()),
                )),
                _ => None,
            })
            .collect()
    }

    async fn one_song_service(db: &Database, lib_id: &str, song_id: &str) -> String {
        let svc = ServiceRepo::new(&db.pool)
            .create(lib_id, "Svc", now_ms())
            .await
            .unwrap();
        ServiceRepo::new(&db.pool)
            .add_item(&svc.id, 0, "song", Some(song_id), None, None, None, None)
            .await
            .unwrap();
        svc.id
    }

    #[tokio::test]
    async fn unthemed_service_resolves_builtin_ids_but_keeps_global_appearance() {
        // Nothing chosen anywhere in the cascade: the cue still carries the
        // CONCRETE built-in ids, but NO per-cue appearance — so the operator's
        // global OutputAppearance keeps styling the output (today's look).
        let db = Database::open_in_memory().await.unwrap();
        let (lib_id, song_id) = fixture_library_song(&db).await;
        let svc_id = one_song_service(&db, &lib_id, &song_id).await;

        let cl = CueCompiler::new(&db.pool).compile(&svc_id).await.unwrap();
        for (theme_id, bg) in cue_themes(&cl) {
            assert_eq!(
                theme_id.as_deref(),
                Some(crate::services::theme::DEFAULT_THEME_ID)
            );
            assert_eq!(bg, None, "no explicit choice → global appearance rules");
        }
    }

    #[tokio::test]
    async fn library_default_theme_lands_on_every_cue() {
        let db = Database::open_in_memory().await.unwrap();
        let (lib_id, song_id) = fixture_library_song(&db).await;
        sqlx::query("UPDATE library SET default_theme_id = 'builtin-theme-high-contrast'")
            .execute(&db.pool)
            .await
            .unwrap();
        let svc_id = one_song_service(&db, &lib_id, &song_id).await;

        let cl = CueCompiler::new(&db.pool).compile(&svc_id).await.unwrap();
        assert!(!cl.is_empty());
        for (theme_id, bg) in cue_themes(&cl) {
            assert_eq!(theme_id.as_deref(), Some("builtin-theme-high-contrast"));
            assert_eq!(bg.as_deref(), Some("#000000"), "resolved look embedded");
        }
    }

    /// A custom library theme with a distinctive background, set as a
    /// song/slide-level override (those columns have a FK to `theme(id)`, so
    /// overrides are stored rows — built-ins can only be *library defaults*).
    async fn custom_theme(db: &Database, lib_id: &str, bg: &str) -> String {
        use crate::services::slide_doc::{BackgroundKind, SlideBackground};
        use crate::services::theme::ThemeTokens;
        crate::db::repositories::ThemeRepo::new(&db.pool)
            .create_theme(
                lib_id,
                "Custom",
                &ThemeTokens {
                    background: SlideBackground {
                        kind: BackgroundKind::Color,
                        value: bg.into(),
                    },
                    ..ThemeTokens::default()
                },
            )
            .await
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn song_theme_overrides_library_default_per_cue() {
        let db = Database::open_in_memory().await.unwrap();
        let (lib_id, song_id) = fixture_library_song(&db).await;
        sqlx::query("UPDATE library SET default_theme_id = 'builtin-theme-high-contrast'")
            .execute(&db.pool)
            .await
            .unwrap();
        let song_theme = custom_theme(&db, &lib_id, "#abc123").await;
        sqlx::query("UPDATE song SET theme_id = ?1 WHERE id = ?2")
            .bind(&song_theme)
            .bind(&song_id)
            .execute(&db.pool)
            .await
            .unwrap();
        let svc_id = one_song_service(&db, &lib_id, &song_id).await;

        let cl = CueCompiler::new(&db.pool).compile(&svc_id).await.unwrap();
        assert!(!cl.is_empty());
        for (theme_id, bg) in cue_themes(&cl) {
            assert_eq!(theme_id.as_deref(), Some(song_theme.as_str()));
            assert_eq!(bg.as_deref(), Some("#abc123"), "song beats library");
        }
    }

    #[tokio::test]
    async fn deck_slide_theme_overrides_library_default_per_cue() {
        // Slide-level override is the TOP of the cascade: a themed deck slide
        // must beat the library default, while its un-themed sibling slide in
        // the same deck falls through to the library default.
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "T".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        sqlx::query("UPDATE library SET default_theme_id = 'builtin-theme-high-contrast'")
            .execute(&db.pool)
            .await
            .unwrap();

        let slide_theme = custom_theme(&db, &lib.id, "#112233").await;

        let now = now_ms();
        let deck_id = new_id();
        sqlx::query("INSERT INTO custom_deck (id, library_id, name, created_at, updated_at) VALUES (?1, ?2, 'Deck', ?3, ?3)")
            .bind(&deck_id)
            .bind(&lib.id)
            .bind(now)
            .execute(&db.pool)
            .await
            .unwrap();
        let content = r#"{"blocks":[{"type":"text","text":"hello"}]}"#;
        for (pos, theme) in [(0, Some(slide_theme.as_str())), (1, None)] {
            sqlx::query(
                "INSERT INTO slide (id, custom_deck_id, position, content, theme_id, template_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?6)",
            )
            .bind(new_id())
            .bind(&deck_id)
            .bind(pos)
            .bind(content)
            .bind(theme)
            .bind(now)
            .execute(&db.pool)
            .await
            .unwrap();
        }

        let svc = ServiceRepo::new(&db.pool)
            .create(&lib.id, "Svc", now)
            .await
            .unwrap();
        // add_item has no custom_deck parameter — insert the item directly.
        sqlx::query(
            "INSERT INTO service_item (id, service_id, position, kind, song_id,
                arrangement_id, key_override, bible_reference_id, custom_deck_id,
                media_asset_id, notes, created_at, updated_at)
             VALUES (?1, ?2, 0, 'custom_deck', NULL, NULL, NULL, NULL, ?3, NULL, NULL, ?4, ?4)",
        )
        .bind(new_id())
        .bind(&svc.id)
        .bind(&deck_id)
        .bind(now)
        .execute(&db.pool)
        .await
        .unwrap();

        let cl = CueCompiler::new(&db.pool).compile(&svc.id).await.unwrap();
        let themes = cue_themes(&cl);
        assert_eq!(themes.len(), 2);
        // Slide override wins on slide 0…
        assert_eq!(themes[0].0.as_deref(), Some(slide_theme.as_str()));
        assert_eq!(themes[0].1.as_deref(), Some("#112233"));
        // …and the un-themed sibling falls back to the library default.
        assert_eq!(themes[1].0.as_deref(), Some("builtin-theme-high-contrast"));
        assert_eq!(themes[1].1.as_deref(), Some("#000000"));
    }

    #[tokio::test]
    async fn scripture_cues_inherit_the_library_default_theme() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "T".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        sqlx::query("UPDATE library SET default_theme_id = 'builtin-theme-christmas'")
            .execute(&db.pool)
            .await
            .unwrap();

        let now = now_ms();
        let ref_id = new_id();
        sqlx::query(
            "INSERT INTO bible_reference (id, book, chapter, verse_start, verse_end, translation, text, created_at)
             VALUES (?1, 'John', 3, 16, NULL, 'NIV', 'For God so loved the world', ?2)",
        )
        .bind(&ref_id)
        .bind(now)
        .execute(&db.pool)
        .await
        .unwrap();
        let svc = ServiceRepo::new(&db.pool)
            .create(&lib.id, "Svc", now)
            .await
            .unwrap();
        ServiceRepo::new(&db.pool)
            .add_item(
                &svc.id,
                0,
                "scripture",
                None,
                None,
                None,
                Some(&ref_id),
                None,
            )
            .await
            .unwrap();

        let cl = CueCompiler::new(&db.pool).compile(&svc.id).await.unwrap();
        let themes = cue_themes(&cl);
        assert!(!themes.is_empty());
        for (theme_id, bg) in themes {
            assert_eq!(theme_id.as_deref(), Some("builtin-theme-christmas"));
            assert!(bg.unwrap().contains("gradient"), "christmas bg embedded");
        }
    }

    // ── property: humanize ↔ normalize is a stable round-trip ───────────────────

    // INVARIANT: for every canonical snake_case label the importer/heuristic
    // formatter emits, normalize_label(humanize_section_label(label)) == label.
    // This pins the editor↔stage-display↔re-import contract: a section labelled
    // by the importer, humanized for the operator/musician display, then read
    // back (e.g. re-imported from a humanized export) must not drift.
    #[test]
    fn humanize_normalize_roundtrip_on_canonical_labels() {
        use crate::services::ai::lyric_format::normalize_label;
        let bases = [
            "verse",
            "chorus",
            "pre_chorus",
            "bridge",
            "intro",
            "ending",
            "tag",
            "instrumental",
        ];
        // Numbered forms only apply to the labels normalize_label numbers.
        let numbered = ["verse", "pre_chorus"];
        let mut labels: Vec<String> = bases.iter().map(|s| s.to_string()).collect();
        for b in numbered {
            for n in [1usize, 2, 9, 10, 23] {
                labels.push(format!("{b}_{n}"));
            }
        }
        for label in labels {
            let humanized = humanize_section_label(&label);
            let back = normalize_label(&humanized);
            assert_eq!(
                back, label,
                "round-trip drift: {label:?} -> {humanized:?} -> {back:?}"
            );
        }
    }
}
