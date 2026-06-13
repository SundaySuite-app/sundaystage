//! Phase 12.1 — companion broadcast publisher (transform).
//!
//! On every cue advance the live frame is reduced to a small, **text-only**
//! payload (never background images/video) and published to congregation
//! phones. Privacy: a slide flagged sensitive broadcasts only a neutral
//! placeholder. This module is the pure transform + the versioned schema; the
//! Supabase Realtime transport is the (Phase 9-dependent) follow-up.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::services::live_session::LiveFrame;

/// Schema version — bump on any breaking change to [`BroadcastFrame`].
pub const BROADCAST_SCHEMA_VERSION: u32 = 1;

/// What kind of content the phone should render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/BroadcastKind.ts")]
pub enum BroadcastKind {
    Lyric,
    Scripture,
    Announcement,
    Blackout,
}

/// The text-only payload a phone receives. Versioned; ordered by `seq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/BroadcastFrame.ts")]
pub struct BroadcastFrame {
    pub v: u32,
    pub kind: BroadcastKind,
    pub text: String,
    pub section_label: Option<String>,
    pub reference: Option<String>,
    pub seq: u32,
}

/// Reduce a live output frame to a broadcast payload. `sensitive` (e.g. a
/// communion/pastoral-prayer slide marked "don't broadcast") collapses to a
/// neutral placeholder so private content never reaches phones. Media is never
/// included — text only.
pub fn to_broadcast(frame: &LiveFrame, seq: u32, sensitive: bool) -> BroadcastFrame {
    // A slide can carry its own `sensitive_slide` flag (set in the editor); the
    // caller may also force-gate. Either gates the broadcast.
    let slide_sensitive = match frame {
        LiveFrame::Slide { slide_content } => slide_content.sensitive_slide,
        _ => false,
    };
    if sensitive || slide_sensitive {
        return BroadcastFrame {
            v: BROADCAST_SCHEMA_VERSION,
            kind: BroadcastKind::Announcement,
            text: "Tjeneste pågår".to_string(),
            section_label: None,
            reference: None,
            seq,
        };
    }
    match frame {
        LiveFrame::Slide { slide_content } => {
            let kind = if slide_content.reference.is_some() {
                BroadcastKind::Scripture
            } else {
                BroadcastKind::Lyric
            };
            BroadcastFrame {
                v: BROADCAST_SCHEMA_VERSION,
                kind,
                text: slide_content.text_lines.join("\n"),
                section_label: slide_content.section_label.clone(),
                reference: slide_content.reference.clone(),
                seq,
            }
        }
        LiveFrame::Message { text } => BroadcastFrame {
            v: BROADCAST_SCHEMA_VERSION,
            kind: BroadcastKind::Announcement,
            text: text.clone(),
            section_label: None,
            reference: None,
            seq,
        },
        // Black + Logo carry nothing to read.
        LiveFrame::Black | LiveFrame::Logo => BroadcastFrame {
            v: BROADCAST_SCHEMA_VERSION,
            kind: BroadcastKind::Blackout,
            text: String::new(),
            section_label: None,
            reference: None,
            seq,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::cue_list::SlideContent;

    fn slide(lines: &[&str], section: Option<&str>, reference: Option<&str>) -> LiveFrame {
        slide_flagged(lines, section, reference, false)
    }

    fn slide_flagged(
        lines: &[&str],
        section: Option<&str>,
        reference: Option<&str>,
        sensitive_slide: bool,
    ) -> LiveFrame {
        LiveFrame::Slide {
            slide_content: SlideContent {
                section_label: section.map(Into::into),
                text_lines: lines.iter().map(|s| s.to_string()).collect(),
                translation_lines: None,
                reference: reference.map(Into::into),
                sensitive_slide,
                appearance: None,
            },
        }
    }

    #[test]
    fn lyric_slide_becomes_lyric_payload() {
        let b = to_broadcast(&slide(&["Holy", "Holy"], Some("chorus"), None), 3, false);
        assert_eq!(b.kind, BroadcastKind::Lyric);
        assert_eq!(b.text, "Holy\nHoly");
        assert_eq!(b.section_label.as_deref(), Some("chorus"));
        assert_eq!(b.seq, 3);
        assert_eq!(b.v, BROADCAST_SCHEMA_VERSION);
    }

    #[test]
    fn slide_with_reference_is_scripture() {
        let b = to_broadcast(
            &slide(&["For God so loved"], None, Some("John 3:16")),
            1,
            false,
        );
        assert_eq!(b.kind, BroadcastKind::Scripture);
        assert_eq!(b.reference.as_deref(), Some("John 3:16"));
    }

    #[test]
    fn blackout_and_logo_carry_no_text() {
        assert_eq!(
            to_broadcast(&LiveFrame::Black, 1, false).kind,
            BroadcastKind::Blackout
        );
        let logo = to_broadcast(&LiveFrame::Logo, 1, false);
        assert_eq!(logo.kind, BroadcastKind::Blackout);
        assert!(logo.text.is_empty());
    }

    #[test]
    fn sensitive_slide_is_placeholder_only() {
        let b = to_broadcast(&slide(&["Secret pastoral prayer"], None, None), 7, true);
        assert_eq!(b.kind, BroadcastKind::Announcement);
        assert_eq!(b.text, "Tjeneste pågår");
        assert!(!b.text.contains("Secret"));
        assert_eq!(b.seq, 7);
    }

    #[test]
    fn slide_flagged_sensitive_collapses_even_without_caller_gate() {
        // The slide carries its own `sensitive_slide` flag; the caller passes
        // `sensitive = false`, yet the broadcast must still collapse.
        let b = to_broadcast(
            &slide_flagged(&["Pastoral prayer text"], Some("verse"), None, true),
            4,
            false,
        );
        assert_eq!(b.kind, BroadcastKind::Announcement);
        assert_eq!(b.text, "Tjeneste pågår");
        assert!(!b.text.contains("Pastoral"));
        assert_eq!(b.section_label, None);
        assert_eq!(b.seq, 4);
    }

    #[test]
    fn message_frame_is_announcement() {
        let b = to_broadcast(
            &LiveFrame::Message {
                text: "Offering".into(),
            },
            2,
            false,
        );
        assert_eq!(b.kind, BroadcastKind::Announcement);
        assert_eq!(b.text, "Offering");
    }
}
