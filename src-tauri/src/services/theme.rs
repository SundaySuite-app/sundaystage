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

/// What a slot is for — drives which token colors/weights it inherits and how
/// the content payload is mapped into it (see [`map_content_to_slots`]).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/SlotRole.ts")]
pub enum SlotRole {
    Title,
    /// Free-form prose (sermon body, announcement text, a quote).
    Body,
    Lyrics,
    Reference,
    Footer,
    /// A media slot. Filled from the payload's `image` field; rendered as an
    /// image background-ish block by the bridge (text bridge skips it for now).
    Image,
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
            name: "Two Column Lyrics".into(),
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
            id: "builtin-template-sermon-title".into(),
            name: "Sermon Title".into(),
            layout: TemplateLayout {
                slots: vec![
                    slot(
                        "title",
                        SlotRole::Title,
                        0.08,
                        0.3,
                        0.84,
                        0.22,
                        HAlign::Center,
                        VAlign::Middle,
                        1.4,
                    ),
                    slot(
                        "body",
                        SlotRole::Body,
                        0.12,
                        0.56,
                        0.76,
                        0.22,
                        HAlign::Center,
                        VAlign::Top,
                        0.6,
                    ),
                ],
            },
        },
        StaticTemplate {
            id: "builtin-template-quote".into(),
            name: "Quote + Attribution".into(),
            layout: TemplateLayout {
                slots: vec![
                    slot(
                        "body",
                        SlotRole::Body,
                        0.1,
                        0.22,
                        0.8,
                        0.5,
                        HAlign::Center,
                        VAlign::Middle,
                        0.95,
                    ),
                    slot(
                        "footer",
                        SlotRole::Footer,
                        0.1,
                        0.76,
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
            id: "builtin-template-image-caption".into(),
            name: "Image + Caption".into(),
            layout: TemplateLayout {
                slots: vec![
                    slot(
                        "image",
                        SlotRole::Image,
                        0.2,
                        0.12,
                        0.6,
                        0.62,
                        HAlign::Center,
                        VAlign::Middle,
                        1.0,
                    ),
                    slot(
                        "footer",
                        SlotRole::Footer,
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
            id: "builtin-template-announcement".into(),
            name: "Announcement".into(),
            layout: TemplateLayout {
                slots: vec![
                    slot(
                        "title",
                        SlotRole::Title,
                        0.08,
                        0.16,
                        0.84,
                        0.2,
                        HAlign::Center,
                        VAlign::Middle,
                        1.2,
                    ),
                    slot(
                        "body",
                        SlotRole::Body,
                        0.12,
                        0.4,
                        0.76,
                        0.4,
                        HAlign::Center,
                        VAlign::Top,
                        0.65,
                    ),
                    slot(
                        "footer",
                        SlotRole::Footer,
                        0.1,
                        0.82,
                        0.8,
                        0.1,
                        HAlign::Center,
                        VAlign::Middle,
                        0.45,
                    ),
                ],
            },
        },
        StaticTemplate {
            id: "builtin-template-title-only".into(),
            name: "Title Only".into(),
            layout: TemplateLayout {
                slots: vec![slot(
                    "title",
                    SlotRole::Title,
                    0.08,
                    0.4,
                    0.84,
                    0.2,
                    HAlign::Center,
                    VAlign::Middle,
                    1.4,
                )],
            },
        },
        StaticTemplate {
            id: "builtin-template-blank".into(),
            name: "Blank".into(),
            layout: TemplateLayout { slots: vec![] },
        },
    ]
}

// ── Slot mapping (apply-template content fill) ─────────────────────────────────

/// A content payload to be poured into a template's slots. Every field is
/// optional; a template only consumes the fields whose roles its slots declare.
///
/// This is the *source* side of the apply-template flow: the gallery hands a
/// payload to [`map_content_to_slots`], which resolves it into the
/// `slot_name -> text` map that [`render_slide`] already understands.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../src/lib/bindings/SlideContentPayload.ts")]
pub struct SlideContentPayload {
    pub title: Option<String>,
    pub body: Option<String>,
    pub lyrics: Option<String>,
    pub reference: Option<String>,
    pub footer: Option<String>,
    pub image: Option<String>,
}

impl SlideContentPayload {
    /// The payload field that feeds a given slot role.
    fn field_for(&self, role: SlotRole) -> Option<&str> {
        let v = match role {
            SlotRole::Title => &self.title,
            SlotRole::Body => &self.body,
            SlotRole::Lyrics => &self.lyrics,
            SlotRole::Reference => &self.reference,
            SlotRole::Footer => &self.footer,
            SlotRole::Image => &self.image,
        };
        v.as_deref()
    }
}

/// Map a content payload onto a template's named slots, producing the
/// `slot_name -> text` map that [`render_slide`] consumes.
///
/// Rules (kept deterministic and total so it can never panic on a Sunday):
///   * Each slot pulls from the payload field matching its [`SlotRole`].
///   * A missing/blank field leaves that slot out of the map → the renderer
///     skips it (empty slot, not a broken one).
///   * When a single role appears in *multiple* slots (e.g. a two-column lyrics
///     template), the source text is split across them paragraph-by-paragraph
///     (blank-line separated), balanced left-to-right; if it doesn't split,
///     the first slot gets it all and the rest stay empty.
pub fn map_content_to_slots(
    layout: &TemplateLayout,
    payload: &SlideContentPayload,
) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();

    // Group slot names by role, preserving template order.
    let mut by_role: Vec<(SlotRole, Vec<&str>)> = Vec::new();
    for s in &layout.slots {
        if let Some((_, names)) = by_role.iter_mut().find(|(r, _)| *r == s.role) {
            names.push(&s.name);
        } else {
            by_role.push((s.role, vec![&s.name]));
        }
    }

    for (role, names) in by_role {
        let Some(text) = payload
            .field_for(role)
            .map(str::trim)
            .filter(|t| !t.is_empty())
        else {
            continue;
        };
        if names.len() == 1 {
            out.insert(names[0].to_string(), text.to_string());
            continue;
        }
        // Split across the N slots of this role by paragraph (blank line).
        let parts = split_into(text, names.len());
        for (name, part) in names.iter().zip(parts) {
            if !part.trim().is_empty() {
                out.insert(name.to_string(), part);
            }
        }
    }

    out
}

/// Split `text` into at most `n` chunks on blank-line paragraph boundaries,
/// balancing paragraphs across chunks. If there are fewer paragraphs than
/// chunks, the leading chunks get one paragraph each and the rest are empty;
/// if there are no blank-line breaks at all, the whole text goes to chunk 0.
fn split_into(text: &str, n: usize) -> Vec<String> {
    if n <= 1 {
        return vec![text.to_string()];
    }
    let paras: Vec<&str> = text
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if paras.len() <= 1 {
        // Can't split meaningfully — first chunk gets everything.
        let mut v = vec![String::new(); n];
        v[0] = text.to_string();
        return v;
    }
    let mut chunks = vec![Vec::<&str>::new(); n];
    // Ceil-divide so the left chunks fill first (balanced, left-heavy).
    let per = paras.len().div_ceil(n);
    for (i, para) in paras.iter().enumerate() {
        let idx = (i / per).min(n - 1);
        chunks[idx].push(para);
    }
    chunks.into_iter().map(|c| c.join("\n\n")).collect()
}

// ── Per-cue resolved appearance (audit 2c) ───────────────────────────────────

/// The *resolved* look of one live cue, embedded on its [`SlideContent`] at
/// compile time (see `CueCompiler`). Derived from the cascade-resolved theme
/// tokens + template layout, so the output process needs no DB and no theme
/// lookup at show time. Every field is optional: `None` means "inherit the
/// operator's global `OutputAppearance`" — the cue only overrides what the
/// theme actually specifies.
///
/// [`SlideContent`]: crate::services::cue_list::SlideContent
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/SlideAppearance.ts")]
pub struct SlideAppearance {
    /// CSS background — a colour or a gradient (themes use both).
    pub bg: Option<String>,
    /// Lyric/text colour.
    pub text_color: Option<String>,
    /// Theme font stack.
    pub font_family: Option<String>,
    /// Horizontal alignment from the template's primary text slot.
    pub h_align: Option<HAlign>,
    /// Multiplier applied ON TOP of the operator's `text_scale` (so a theme
    /// with a bigger base size scales with — never fights — the operator's
    /// "larger text" preference). Theme `body_size`/64 × slot `size_scale`.
    pub text_scale: Option<f32>,
}

/// Derive the per-cue appearance from resolved theme tokens + template layout.
/// The live output renders flat lyric lines (not the full slot layout), so the
/// template contributes its *primary* text slot's alignment and size scale —
/// the lyrics slot if present, else body/title, else template defaults.
pub fn slide_appearance_from(tokens: &ThemeTokens, layout: &TemplateLayout) -> SlideAppearance {
    let primary = layout
        .slots
        .iter()
        .find(|s| s.role == SlotRole::Lyrics)
        .or_else(|| {
            layout
                .slots
                .iter()
                .find(|s| matches!(s.role, SlotRole::Body | SlotRole::Title))
        });
    SlideAppearance {
        bg: Some(tokens.background.value.clone()),
        text_color: Some(tokens.text_color.clone()),
        font_family: Some(tokens.font_family.clone()),
        h_align: primary.map(|s| s.align),
        text_scale: Some((tokens.body_size / 64.0) * primary.map(|s| s.size_scale).unwrap_or(1.0)),
    }
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
        SlotRole::Title | SlotRole::Body | SlotRole::Lyrics | SlotRole::Image => {
            tokens.text_color.clone()
        }
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
        // An image slot's value is an asset id / path → render a media block.
        if s.role == SlotRole::Image {
            blocks.push(SlideBlock::Image {
                id: format!("slot:{}", s.name),
                rect: s.rect,
                src: text,
            });
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

    // ── Per-cue appearance derivation ────────────────────────────────────────

    #[test]
    fn slide_appearance_carries_theme_tokens_and_primary_slot() {
        let tokens = tokens_for("builtin-theme-high-contrast", &HashMap::new());
        let layout = layout_for("builtin-template-lyrics-lower-third", &HashMap::new());
        let a = slide_appearance_from(&tokens, &layout);
        assert_eq!(a.bg.as_deref(), Some("#000000"));
        assert_eq!(a.text_color.as_deref(), Some("#ffffff"));
        assert_eq!(a.h_align, Some(HAlign::Left)); // lower-third lyrics slot
                                                   // body_size 64 / 64 × slot scale 0.85.
        assert!((a.text_scale.unwrap() - 0.85).abs() < 1e-4);
        assert!(a.font_family.is_some());
    }

    #[test]
    fn slide_appearance_prefers_lyrics_slot_then_body_then_defaults() {
        let tokens = ThemeTokens::default();
        // Bible-verse template: lyrics slot wins over the reference slot.
        let bible = layout_for("builtin-template-bible-verse", &HashMap::new());
        assert_eq!(
            slide_appearance_from(&tokens, &bible).h_align,
            Some(HAlign::Center)
        );
        // Quote template has no lyrics slot → body slot drives.
        let quote = layout_for("builtin-template-quote", &HashMap::new());
        let a = slide_appearance_from(&tokens, &quote);
        assert_eq!(a.h_align, Some(HAlign::Center));
        assert!((a.text_scale.unwrap() - 0.95).abs() < 1e-4);
        // Blank template (no slots at all): alignment inherits, scale = theme base.
        let blank = layout_for("builtin-template-blank", &HashMap::new());
        let a = slide_appearance_from(&tokens, &blank);
        assert_eq!(a.h_align, None);
        assert!((a.text_scale.unwrap() - 1.0).abs() < 1e-4);
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

    // ── Slot mapping ──────────────────────────────────────────────────────────

    fn tmpl(id: &str) -> TemplateLayout {
        builtin_templates()
            .into_iter()
            .find(|t| t.id == id)
            .unwrap_or_else(|| panic!("missing builtin {id}"))
            .layout
    }

    #[test]
    fn map_fills_each_slot_from_its_role_field() {
        let layout = tmpl("builtin-template-announcement"); // title + body + footer
        let payload = SlideContentPayload {
            title: Some("Youth Camp".into()),
            body: Some("Sign up after the service".into()),
            footer: Some("July 12-15".into()),
            ..Default::default()
        };
        let map = map_content_to_slots(&layout, &payload);
        assert_eq!(map.get("title").map(String::as_str), Some("Youth Camp"));
        assert_eq!(
            map.get("body").map(String::as_str),
            Some("Sign up after the service")
        );
        assert_eq!(map.get("footer").map(String::as_str), Some("July 12-15"));
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn map_leaves_missing_content_slot_empty_not_broken() {
        let layout = tmpl("builtin-template-announcement");
        let payload = SlideContentPayload {
            title: Some("Welcome".into()),
            // body + footer intentionally absent
            ..Default::default()
        };
        let map = map_content_to_slots(&layout, &payload);
        assert_eq!(map.get("title").map(String::as_str), Some("Welcome"));
        assert!(!map.contains_key("body"), "missing field => slot omitted");
        assert!(!map.contains_key("footer"));
        // And the rendered doc simply skips the empty slots — no panic, no blank block.
        let doc = render_slide(&layout, &ThemeTokens::default(), &map);
        assert_eq!(doc.blocks.len(), 1);
    }

    #[test]
    fn map_treats_blank_field_as_empty() {
        let layout = tmpl("builtin-template-title-only");
        let payload = SlideContentPayload {
            title: Some("   \n  ".into()),
            ..Default::default()
        };
        let map = map_content_to_slots(&layout, &payload);
        assert!(map.is_empty(), "whitespace-only field fills no slot");
    }

    #[test]
    fn map_ignores_unknown_slot_roles_in_payload() {
        // title-only template ignores body/lyrics/reference content entirely.
        let layout = tmpl("builtin-template-title-only");
        let payload = SlideContentPayload {
            title: Some("Sermon".into()),
            body: Some("ignored".into()),
            lyrics: Some("ignored".into()),
            reference: Some("ignored".into()),
            footer: Some("ignored".into()),
            image: Some("ignored.png".into()),
        };
        let map = map_content_to_slots(&layout, &payload);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("title").map(String::as_str), Some("Sermon"));
    }

    #[test]
    fn map_splits_one_role_across_multiple_slots_by_paragraph() {
        let layout = tmpl("builtin-template-two-column"); // two lyrics slots
        let payload = SlideContentPayload {
            lyrics: Some("verse one\nline two\n\nverse three\nline four".into()),
            ..Default::default()
        };
        let map = map_content_to_slots(&layout, &payload);
        assert_eq!(
            map.get("left").map(String::as_str),
            Some("verse one\nline two")
        );
        assert_eq!(
            map.get("right").map(String::as_str),
            Some("verse three\nline four")
        );
    }

    #[test]
    fn map_unsplittable_multislot_role_puts_all_in_first() {
        let layout = tmpl("builtin-template-two-column");
        let payload = SlideContentPayload {
            lyrics: Some("just one block\nno blank line".into()),
            ..Default::default()
        };
        let map = map_content_to_slots(&layout, &payload);
        assert_eq!(
            map.get("left").map(String::as_str),
            Some("just one block\nno blank line")
        );
        assert!(!map.contains_key("right"));
    }

    #[test]
    fn map_is_deterministic() {
        let layout = tmpl("builtin-template-quote");
        let payload = SlideContentPayload {
            body: Some("Be still and know".into()),
            footer: Some("Psalm 46".into()),
            ..Default::default()
        };
        let a = map_content_to_slots(&layout, &payload);
        let b = map_content_to_slots(&layout, &payload);
        assert_eq!(a, b);
    }

    #[test]
    fn map_then_render_produces_image_block_for_image_slot() {
        let layout = tmpl("builtin-template-image-caption");
        let payload = SlideContentPayload {
            image: Some("asset-123".into()),
            footer: Some("Baptism Sunday".into()),
            ..Default::default()
        };
        let map = map_content_to_slots(&layout, &payload);
        let doc = render_slide(&layout, &ThemeTokens::default(), &map);
        assert_eq!(doc.blocks.len(), 2);
        let has_image = doc
            .blocks
            .iter()
            .any(|b| matches!(b, SlideBlock::Image { src, .. } if src == "asset-123"));
        assert!(has_image, "image slot renders a media block, not text");
    }

    #[test]
    fn every_builtin_template_has_unique_named_slots() {
        for t in builtin_templates() {
            let mut names: Vec<&str> = t.layout.slots.iter().map(|s| s.name.as_str()).collect();
            let n = names.len();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), n, "template {} has duplicate slot names", t.id);
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
