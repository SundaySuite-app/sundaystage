//! Phase 3.2 — Themes, templates, and the cascade.
//!
//! Two orthogonal concepts (kept separate on purpose, see ADR in
//! `docs/ARCHITECTURE.md`):
//!
//!   * **Theme** — a bundle of color + typographic tokens ("Sunday Morning",
//!     "Christmas"). *How* text looks.
//!   * **Template** — a layout of named slots ("Lyrics Centered", "Bible
//!     Verse"). *Where* text sits. A template styled by any theme should look
//!     right.
//!
//! The effective theme/template for a slide is resolved by a cascade:
//!
//! ```text
//! slide override  >  song override  >  library default  >  built-in default
//! ```
//!
//! That cascade is the part the build plan flags as bug-prone, so it lives in
//! small pure functions ([`resolve_theme_id`], [`resolve_template_id`]) with
//! their own test suite. [`render_slide`] then bridges a resolved
//! template+theme into the Phase 3.1 [`SlideDoc`] model, so the editor preview
//! and the live engine paint identical pixels from the same code.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::services::slide_doc::{
    BackgroundKind, HAlign, SlideBackground, SlideBlock, SlideDoc, SlideRect, TextStyle, VAlign,
};

/// Stable id of the built-in theme used when nothing else resolves.
pub const DEFAULT_THEME_ID: &str = "builtin-theme-sunday-morning";
/// Stable id of the built-in template used when nothing else resolves.
pub const DEFAULT_TEMPLATE_ID: &str = "builtin-template-lyrics-centered";

/// Color + typography tokens stored in `theme.tokens` (JSON).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../src/lib/bindings/ThemeTokens.ts")]
pub struct ThemeTokens {
    pub background: SlideBackground,
    pub text_color: String,
    pub accent_color: String,
    pub font_family: String,
    pub heading_weight: u32,
    /// Base body text size in px on the virtual 1080 stage; slots scale it.
    pub body_size: f32,
    pub shadow: Option<String>,
}

impl Default for ThemeTokens {
    fn default() -> Self {
        Self {
            background: SlideBackground {
                kind: BackgroundKind::Gradient,
                value: "linear-gradient(160deg, #1a2a52, #0b1020)".to_string(),
            },
            text_color: "#ffffff".to_string(),
            accent_color: "#e8c069".to_string(),
            font_family: "Inter, system-ui, sans-serif".to_string(),
            heading_weight: 700,
            body_size: 64.0,
            shadow: Some("0 2px 8px rgba(0,0,0,0.6)".to_string()),
        }
    }
}

/// What a slot is for — drives which token colors/weights it inherits.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/SlotRole.ts")]
pub enum SlotRole {
    Title,
    Lyrics,
    Reference,
    Footer,
}

/// One named region in a template. Themes style it; content fills it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../src/lib/bindings/TemplateSlot.ts")]
pub struct TemplateSlot {
    /// Stable key used to address the slot when filling content.
    pub name: String,
    pub role: SlotRole,
    pub rect: SlideRect,
    pub align: HAlign,
    pub valign: VAlign,
    /// Multiplier on the theme's `body_size` (e.g. title 1.4, reference 0.45).
    pub size_scale: f32,
}

/// The layout stored in `template.slots` (JSON).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[ts(export, export_to = "../../src/lib/bindings/TemplateLayout.ts")]
pub struct TemplateLayout {
    pub slots: Vec<TemplateSlot>,
}

// ── Cascade ──────────────────────────────────────────────────────────────────

/// First `Some` in the precedence chain (highest priority first), or `None`.
fn first_some(chain: &[&Option<String>]) -> Option<String> {
    chain.iter().copied().flatten().next().cloned()
}

/// Resolve the effective theme id for a slide. Always yields a concrete id —
/// falls back to the built-in default so rendering never has "no theme".
pub fn resolve_theme_id(
    slide_theme_id: &Option<String>,
    song_theme_id: &Option<String>,
    library_default_theme_id: &Option<String>,
) -> String {
    first_some(&[slide_theme_id, song_theme_id, library_default_theme_id])
        .unwrap_or_else(|| DEFAULT_THEME_ID.to_string())
}

/// Resolve the effective template id for a slide (same precedence rules).
pub fn resolve_template_id(
    slide_template_id: &Option<String>,
    song_template_id: &Option<String>,
    library_default_template_id: &Option<String>,
) -> String {
    first_some(&[
        slide_template_id,
        song_template_id,
        library_default_template_id,
    ])
    .unwrap_or_else(|| DEFAULT_TEMPLATE_ID.to_string())
}

// ── Built-ins ────────────────────────────────────────────────────────────────

fn theme(id: &str, name: &str, tokens: ThemeTokens) -> StaticTheme {
    StaticTheme {
        id: id.to_string(),
        name: name.to_string(),
        tokens,
    }
}

/// A built-in theme as plain data (no DB row). The repo maps it to a `Theme`.
#[derive(Debug, Clone)]
pub struct StaticTheme {
    pub id: String,
    pub name: String,
    pub tokens: ThemeTokens,
}

/// A built-in template as plain data. The repo maps it to a `Template`.
#[derive(Debug, Clone)]
pub struct StaticTemplate {
    pub id: String,
    pub name: String,
    pub layout: TemplateLayout,
}

pub fn builtin_themes() -> Vec<StaticTheme> {
    vec![
        theme(
            "builtin-theme-sunday-morning",
            "Sunday Morning",
            ThemeTokens::default(),
        ),
        theme(
            "builtin-theme-evening",
            "Evening Service",
            ThemeTokens {
                background: SlideBackground {
                    kind: BackgroundKind::Gradient,
                    value: "linear-gradient(160deg, #20143a, #05030c)".into(),
                },
                accent_color: "#b794f6".into(),
                ..ThemeTokens::default()
            },
        ),
        theme(
            "builtin-theme-christmas",
            "Christmas",
            ThemeTokens {
                background: SlideBackground {
                    kind: BackgroundKind::Gradient,
                    value: "linear-gradient(160deg, #0c2a1a, #07150d)".into(),
                },
                accent_color: "#e0b341".into(),
                ..ThemeTokens::default()
            },
        ),
        theme(
            "builtin-theme-minimal-light",
            "Minimal Light",
            ThemeTokens {
                background: SlideBackground {
                    kind: BackgroundKind::Color,
                    value: "#f7f7f5".into(),
                },
                text_color: "#101014".into(),
                accent_color: "#9a6a18".into(),
                heading_weight: 600,
                shadow: None,
                ..ThemeTokens::default()
            },
        ),
        theme(
            "builtin-theme-high-contrast",
            "High Contrast",
            ThemeTokens {
                background: SlideBackground {
                    kind: BackgroundKind::Color,
                    value: "#000000".into(),
                },
                text_color: "#ffffff".into(),
                accent_color: "#ffd400".into(),
                heading_weight: 800,
                shadow: None,
                ..ThemeTokens::default()
            },
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn slot(
    name: &str,
    role: SlotRole,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    align: HAlign,
    valign: VAlign,
    scale: f32,
) -> TemplateSlot {
    TemplateSlot {
        name: name.into(),
        role,
        rect: SlideRect { x, y, w, h },
        align,
        valign,
        size_scale: scale,
    }
}

pub fn builtin_templates() -> Vec<StaticTemplate> {
    vec![
        StaticTemplate {
            id: "builtin-template-lyrics-centered".into(),
            name: "Lyrics Centered".into(),
            layout: TemplateLayout {
                slots: vec![slot(
                    "lyrics",
                    SlotRole::Lyrics,
                    0.08,
                    0.25,
                    0.84,
                    0.5,
                    HAlign::Center,
                    VAlign::Middle,
                    1.0,
                )],
            },
        },
        StaticTemplate {
            id: "builtin-template-lyrics-lower-third".into(),
            name: "Lyrics Lower Third".into(),
            layout: TemplateLayout {
                slots: vec![slot(
                    "lyrics",
                    SlotRole::Lyrics,
                    0.06,
                    0.68,
                    0.88,
                    0.26,
                    HAlign::Left,
                    VAlign::Bottom,
                    0.85,
                )],
            },
        },
        StaticTemplate {
            id: "builtin-template-bible-verse".into(),
            name: "Bible Verse".into(),
            layout: TemplateLayout {
                slots: vec![
                    slot(
                        "lyrics",
                        SlotRole::Lyrics,
                        0.1,
                        0.2,
                        0.8,
                        0.55,
                        HAlign::Center,
                        VAlign::Middle,
                        0.9,
                    ),
                    slot(
                        "reference",
                        SlotRole::Reference,
                        0.1,
                        0.78,
                        0.8,
                        0.12,
                        HAlign::Center,
                        VAlign::Middle,
                        0.5,
                    ),
                ],
            },
        },
        StaticTemplate {
            id: "builtin-template-title".into(),
            name: "Title".into(),
            layout: TemplateLayout {
                slots: vec![
                    slot(
                        "title",
                        SlotRole::Title,
                        0.1,
                        0.34,
                        0.8,
                        0.2,
                        HAlign::Center,
                        VAlign::Middle,
                        1.4,
                    ),
                    slot(
                        "footer",
                        SlotRole::Footer,
                        0.1,
                        0.56,
                        0.8,
                        0.1,
                        HAlign::Center,
                        VAlign::Top,
                        0.45,
                    ),
                ],
            },
        },
        StaticTemplate {
            id: "builtin-template-two-column".into(),
            name: "Two Column".into(),
            layout: TemplateLayout {
                slots: vec![
                    slot(
                        "left",
                        SlotRole::Lyrics,
                        0.06,
                        0.2,
                        0.42,
                        0.6,
                        HAlign::Center,
                        VAlign::Middle,
                        0.85,
                    ),
                    slot(
                        "right",
                        SlotRole::Lyrics,
                        0.52,
                        0.2,
                        0.42,
                        0.6,
                        HAlign::Center,
                        VAlign::Middle,
                        0.85,
                    ),
                ],
            },
        },
        StaticTemplate {
            id: "builtin-template-blank".into(),
            name: "Blank".into(),
            layout: TemplateLayout { slots: vec![] },
        },
    ]
}

/// Tokens for a theme id, checking built-ins then the supplied DB themes.
/// A missing/dangling id yields the default theme's tokens rather than an
/// error — a deleted theme should degrade gracefully, never blank the screen.
pub fn tokens_for(theme_id: &str, db_lookup: &HashMap<String, ThemeTokens>) -> ThemeTokens {
    if let Some(t) = builtin_themes().into_iter().find(|t| t.id == theme_id) {
        return t.tokens;
    }
    if let Some(t) = db_lookup.get(theme_id) {
        return t.clone();
    }
    builtin_themes()
        .into_iter()
        .find(|t| t.id == DEFAULT_THEME_ID)
        .map(|t| t.tokens)
        .unwrap_or_default()
}

/// Layout for a template id, checking built-ins then the supplied DB layouts.
pub fn layout_for(
    template_id: &str,
    db_lookup: &HashMap<String, TemplateLayout>,
) -> TemplateLayout {
    if let Some(t) = builtin_templates()
        .into_iter()
        .find(|t| t.id == template_id)
    {
        return t.layout;
    }
    if let Some(l) = db_lookup.get(template_id) {
        return l.clone();
    }
    builtin_templates()
        .into_iter()
        .find(|t| t.id == DEFAULT_TEMPLATE_ID)
        .map(|t| t.layout)
        .unwrap_or_default()
}

// ── Rendering ────────────────────────────────────────────────────────────────

fn slot_color(role: SlotRole, tokens: &ThemeTokens) -> String {
    match role {
        SlotRole::Reference | SlotRole::Footer => tokens.accent_color.clone(),
        SlotRole::Title | SlotRole::Lyrics => tokens.text_color.clone(),
    }
}

/// Render a template + theme + per-slot text into a [`SlideDoc`]. Empty slots
/// are skipped. Block ids are derived from slot names so re-rendering the same
/// template updates blocks in place.
pub fn render_slide(
    layout: &TemplateLayout,
    tokens: &ThemeTokens,
    slot_text: &HashMap<String, String>,
) -> SlideDoc {
    let mut blocks: Vec<SlideBlock> = Vec::new();
    for s in &layout.slots {
        let text = slot_text.get(&s.name).cloned().unwrap_or_default();
        if text.trim().is_empty() {
            continue;
        }
        blocks.push(SlideBlock::Text {
            id: format!("slot:{}", s.name),
            text,
            rect: s.rect,
            align: s.align,
            valign: s.valign,
            style: TextStyle {
                family: Some(tokens.font_family.clone()),
                size: tokens.body_size * s.size_scale,
                weight: tokens.heading_weight,
                color: slot_color(s.role, tokens),
                italic: false,
                shadow: tokens.shadow.clone(),
            },
        });
    }
    SlideDoc {
        background: tokens.background.clone(),
        blocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    // ── Cascade ────────────────────────────────────────────────────────────

    #[test]
    fn cascade_prefers_slide_then_song_then_library() {
        assert_eq!(
            resolve_theme_id(&some("slide"), &some("song"), &some("lib")),
            "slide"
        );
        assert_eq!(resolve_theme_id(&None, &some("song"), &some("lib")), "song");
        assert_eq!(resolve_theme_id(&None, &None, &some("lib")), "lib");
    }

    #[test]
    fn cascade_falls_back_to_builtin_default_when_chain_empty() {
        assert_eq!(resolve_theme_id(&None, &None, &None), DEFAULT_THEME_ID);
        assert_eq!(
            resolve_template_id(&None, &None, &None),
            DEFAULT_TEMPLATE_ID
        );
    }

    #[test]
    fn template_cascade_mirrors_theme_cascade() {
        assert_eq!(
            resolve_template_id(&some("s"), &some("song"), &some("lib")),
            "s"
        );
        assert_eq!(resolve_template_id(&None, &None, &some("lib")), "lib");
    }

    // ── Token / layout lookup ───────────────────────────────────────────────

    #[test]
    fn tokens_for_resolves_builtin() {
        let empty = HashMap::new();
        let t = tokens_for("builtin-theme-high-contrast", &empty);
        assert_eq!(t.background.value, "#000000");
        assert_eq!(t.heading_weight, 800);
    }

    #[test]
    fn tokens_for_prefers_db_for_non_builtin_id() {
        let mut db = HashMap::new();
        let custom = ThemeTokens {
            text_color: "#ff0000".into(),
            ..ThemeTokens::default()
        };
        db.insert("lib-theme-1".to_string(), custom.clone());
        assert_eq!(tokens_for("lib-theme-1", &db), custom);
    }

    #[test]
    fn tokens_for_dangling_id_falls_back_to_default() {
        let empty = HashMap::new();
        let fallback = tokens_for("does-not-exist", &empty);
        let default = tokens_for(DEFAULT_THEME_ID, &empty);
        assert_eq!(fallback, default);
    }

    #[test]
    fn layout_for_dangling_id_falls_back_to_default() {
        let empty = HashMap::new();
        let fallback = layout_for("nope", &empty);
        let default = layout_for(DEFAULT_TEMPLATE_ID, &empty);
        assert_eq!(fallback, default);
    }

    #[test]
    fn builtins_have_unique_stable_ids() {
        let mut ids: Vec<String> = builtin_themes().into_iter().map(|t| t.id).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n, "theme ids must be unique");

        let mut tids: Vec<String> = builtin_templates().into_iter().map(|t| t.id).collect();
        let tn = tids.len();
        tids.sort();
        tids.dedup();
        assert_eq!(tids.len(), tn, "template ids must be unique");
    }

    #[test]
    fn default_ids_exist_among_builtins() {
        assert!(builtin_themes().iter().any(|t| t.id == DEFAULT_THEME_ID));
        assert!(builtin_templates()
            .iter()
            .any(|t| t.id == DEFAULT_TEMPLATE_ID));
    }

    // ── Render ──────────────────────────────────────────────────────────────

    #[test]
    fn render_fills_slots_and_skips_empty_ones() {
        let layout = builtin_templates()
            .into_iter()
            .find(|t| t.id == "builtin-template-bible-verse")
            .unwrap()
            .layout;
        let tokens = ThemeTokens::default();
        let mut text = HashMap::new();
        text.insert(
            "lyrics".to_string(),
            "For God so loved the world".to_string(),
        );
        // reference slot intentionally left empty
        let doc = render_slide(&layout, &tokens, &text);
        assert_eq!(doc.blocks.len(), 1, "only the filled slot renders");
        match &doc.blocks[0] {
            SlideBlock::Text { id, text, .. } => {
                assert_eq!(id, "slot:lyrics");
                assert_eq!(text, "For God so loved the world");
            }
            _ => panic!("expected text block"),
        }
    }

    #[test]
    fn render_applies_theme_tokens_to_slots() {
        let layout = builtin_templates()
            .into_iter()
            .find(|t| t.id == "builtin-template-bible-verse")
            .unwrap()
            .layout;
        let tokens = ThemeTokens::default();
        let mut text = HashMap::new();
        text.insert("lyrics".to_string(), "Line".to_string());
        text.insert("reference".to_string(), "John 3:16".to_string());
        let doc = render_slide(&layout, &tokens, &text);
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.background, tokens.background);
        // Reference slot uses the accent color; lyrics use the text color.
        for b in &doc.blocks {
            if let SlideBlock::Text { id, style, .. } = b {
                if id == "slot:reference" {
                    assert_eq!(style.color, tokens.accent_color);
                } else {
                    assert_eq!(style.color, tokens.text_color);
                }
            }
        }
    }

    #[test]
    fn rendered_doc_is_readable_by_cue_compiler() {
        let layout = layout_for("builtin-template-lyrics-centered", &HashMap::new());
        let tokens = ThemeTokens::default();
        let mut text = HashMap::new();
        text.insert(
            "lyrics".to_string(),
            "Amazing grace\nhow sweet the sound".to_string(),
        );
        let doc = render_slide(&layout, &tokens, &text);
        let json = doc.to_json().unwrap();
        let lines = crate::services::cue_list::extract_text_lines_from_content(&json).unwrap();
        assert_eq!(lines, vec!["Amazing grace", "how sweet the sound"]);
    }
}
