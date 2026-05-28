//! Phase 3.1 — Slide document model (the editable *design* of a stored slide).
//!
//! Distinct from [`crate::services::cue_list::SlideContent`], which is the
//! *runtime* projection (a section label + text lines already broken for one
//! screen) that the live engine shows. `SlideDoc` is the *design*: positioned
//! text blocks over a background, serialized into `slide.content` (JSON).
//!
//! ## On-disk contract
//!
//! The cue compiler reads the displayable text out of this shape via
//! `cue_list::extract_text_lines_from_content`, which only requires that each
//! text block serialize to an object with `"type": "text"` and a `"text"`
//! string. The cross-consistency test at the bottom of this file locks that
//! contract: if the JSON shape ever drifts, that test fails before the live
//! engine silently renders blank slides.
//!
//! ## Coordinates
//!
//! All geometry is normalized (0.0–1.0) as a fraction of the output frame, so
//! a slide renders identically at any resolution. Font `size` is px on a
//! virtual 1920×1080 stage; the renderer scales it by `actual_height / 1080`
//! so what the editor shows equals what the projector shows.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Normalized rectangle — each component is a fraction (0.0–1.0) of the frame.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../src/lib/bindings/SlideRect.ts")]
pub struct SlideRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Horizontal text alignment within a block.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/HAlign.ts")]
pub enum HAlign {
    Left,
    Center,
    Right,
}

/// Vertical text alignment within a block.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/VAlign.ts")]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

/// Typographic styling for a text block. `size` is px @1080 (see module docs).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../src/lib/bindings/TextStyle.ts")]
pub struct TextStyle {
    /// Font family; `None` falls back to the theme/default stage font.
    pub family: Option<String>,
    pub size: f32,
    pub weight: u32,
    pub color: String,
    pub italic: bool,
    /// CSS shadow string, e.g. `"0 2px 8px rgba(0,0,0,0.6)"`. `None` = none.
    pub shadow: Option<String>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: None,
            size: 64.0,
            weight: 700,
            color: "#ffffff".to_string(),
            italic: false,
            shadow: Some("0 2px 8px rgba(0,0,0,0.6)".to_string()),
        }
    }
}

/// A single element on a slide. Internally tagged by `type` so the JSON looks
/// like `{ "type": "text", ... }` — the shape the cue compiler reads.
///
/// `Image` is modelled now to leave room (per the Phase 3.1 plan) but has no
/// editor UI yet.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/SlideBlock.ts")]
pub enum SlideBlock {
    Text {
        id: String,
        text: String,
        rect: SlideRect,
        align: HAlign,
        valign: VAlign,
        style: TextStyle,
    },
    Image {
        id: String,
        rect: SlideRect,
        /// Media asset id or absolute path resolved at render time.
        src: String,
    },
}

impl SlideBlock {
    pub fn id(&self) -> &str {
        match self {
            SlideBlock::Text { id, .. } => id,
            SlideBlock::Image { id, .. } => id,
        }
    }
}

/// How the slide's full-frame background is painted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/BackgroundKind.ts")]
pub enum BackgroundKind {
    Color,
    Image,
    Video,
    Gradient,
}

/// Slide background. `value` is interpreted per `kind`: a CSS color for
/// `Color`, a CSS gradient for `Gradient`, an asset id/path for media.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../src/lib/bindings/SlideBackground.ts")]
pub struct SlideBackground {
    #[serde(rename = "type")]
    pub kind: BackgroundKind,
    pub value: String,
}

impl Default for SlideBackground {
    fn default() -> Self {
        Self {
            kind: BackgroundKind::Color,
            value: "#0b1020".to_string(),
        }
    }
}

/// The full editable design of one slide. Serialized into `slide.content`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[ts(export, export_to = "../../src/lib/bindings/SlideDoc.ts")]
pub struct SlideDoc {
    pub background: SlideBackground,
    pub blocks: Vec<SlideBlock>,
}

impl SlideDoc {
    /// An empty slide: dark background, no blocks.
    pub fn blank() -> Self {
        Self {
            background: SlideBackground::default(),
            blocks: Vec::new(),
        }
    }

    /// A slide with a single centered text block holding `lines` (joined by
    /// `\n`). Used to seed new slides and in tests. `id` makes the block
    /// addressable by the editor.
    pub fn with_lines(id: impl Into<String>, lines: &[String]) -> Self {
        Self {
            background: SlideBackground::default(),
            blocks: vec![SlideBlock::Text {
                id: id.into(),
                text: lines.join("\n"),
                rect: SlideRect {
                    x: 0.08,
                    y: 0.30,
                    w: 0.84,
                    h: 0.40,
                },
                align: HAlign::Center,
                valign: VAlign::Middle,
                style: TextStyle::default(),
            }],
        }
    }

    /// Serialize to the JSON string stored in `slide.content`.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parse from a `slide.content` JSON string. A blank/garbage value yields
    /// a blank doc rather than an error — a corrupt slide should still open in
    /// the editor so the user can fix it, never hard-fail the whole deck.
    pub fn from_json(content: &str) -> Self {
        serde_json::from_str(content).unwrap_or_else(|_| SlideDoc::blank())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::cue_list::extract_text_lines_from_content;

    #[test]
    fn blank_has_default_background_and_no_blocks() {
        let doc = SlideDoc::blank();
        assert_eq!(doc.background.kind, BackgroundKind::Color);
        assert!(doc.blocks.is_empty());
    }

    #[test]
    fn with_lines_builds_single_centered_text_block() {
        let lines = vec!["Amazing grace".to_string(), "how sweet the sound".to_string()];
        let doc = SlideDoc::with_lines("b1", &lines);
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            SlideBlock::Text { id, text, align, .. } => {
                assert_eq!(id, "b1");
                assert_eq!(text, "Amazing grace\nhow sweet the sound");
                assert_eq!(*align, HAlign::Center);
            }
            _ => panic!("expected a text block"),
        }
    }

    #[test]
    fn serde_round_trip_is_lossless() {
        let lines = vec!["line one".to_string(), "line two".to_string()];
        let doc = SlideDoc::with_lines("b1", &lines);
        let json = doc.to_json().unwrap();
        let back = SlideDoc::from_json(&json);
        assert_eq!(doc, back);
    }

    #[test]
    fn text_block_serializes_with_type_tag() {
        let doc = SlideDoc::with_lines("b1", &["hi".to_string()]);
        let v: serde_json::Value = serde_json::from_str(&doc.to_json().unwrap()).unwrap();
        let block = &v["blocks"][0];
        assert_eq!(block["type"], "text");
        assert_eq!(block["text"], "hi");
        // Background uses the documented `type`/`value` shape.
        assert_eq!(v["background"]["type"], "color");
    }

    #[test]
    fn from_json_recovers_blank_from_garbage() {
        assert_eq!(SlideDoc::from_json("not json at all"), SlideDoc::blank());
        assert_eq!(SlideDoc::from_json(""), SlideDoc::blank());
    }

    /// The load-bearing contract: text authored in the editor must be readable
    /// by the live engine's cue compiler. If the on-disk shape drifts, this
    /// fails before a Sunday morning does.
    #[test]
    fn editor_output_is_readable_by_cue_compiler() {
        let lines = vec![
            "Amazing grace how sweet the sound".to_string(),
            "That saved a wretch like me".to_string(),
        ];
        let doc = SlideDoc::with_lines("b1", &lines);
        let json = doc.to_json().unwrap();
        let extracted = extract_text_lines_from_content(&json).expect("cue compiler reads text");
        assert_eq!(extracted, lines);
    }

    #[test]
    fn multiple_text_blocks_extract_in_order() {
        let doc = SlideDoc {
            background: SlideBackground::default(),
            blocks: vec![
                SlideBlock::Text {
                    id: "a".into(),
                    text: "first".into(),
                    rect: SlideRect { x: 0.0, y: 0.0, w: 1.0, h: 0.5 },
                    align: HAlign::Left,
                    valign: VAlign::Top,
                    style: TextStyle::default(),
                },
                SlideBlock::Text {
                    id: "b".into(),
                    text: "second\nthird".into(),
                    rect: SlideRect { x: 0.0, y: 0.5, w: 1.0, h: 0.5 },
                    align: HAlign::Right,
                    valign: VAlign::Bottom,
                    style: TextStyle::default(),
                },
            ],
        };
        let json = doc.to_json().unwrap();
        let extracted = extract_text_lines_from_content(&json).unwrap();
        assert_eq!(extracted, vec!["first", "second", "third"]);
    }
}
