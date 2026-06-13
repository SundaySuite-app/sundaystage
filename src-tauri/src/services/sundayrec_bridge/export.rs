//! Phase 10.2 — the marquee Sunday-suite transforms.
//!
//! Both derive entirely from the live session's log (cue index + timestamp per
//! advance) plus the compiled cue content — no extra operator effort, which is
//! the whole point ("the user did literally nothing extra").
//!
//!   * [`chapter_markers`] — cue advances → recording chapter markers
//!     ("Worship: Amazing Grace", "Sermon", …) for SundayRec's timeline.
//!   * [`session_to_srt`] — lyric/scripture slides → an `.srt` caption file
//!     matching the recording timeline.
//!
//! Both are pure functions over a [`LiveSession`]; the bridge streams/hands the
//! results to SundayRec.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::services::cue_list::Cue;
use crate::services::live_session::{LiveSession, OutputState};

/// A recording chapter marker: milliseconds from the start of the session, plus
/// a human title.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/ChapterMarker.ts")]
pub struct ChapterMarker {
    pub offset_ms: i64,
    pub title: String,
}

struct Point {
    at: i64,
    index: usize,
    output: OutputState,
}

/// The session's display timeline: the first cue is shown from `started_at`,
/// then each log entry records the resulting state at its timestamp.
fn timeline(session: &LiveSession) -> Vec<Point> {
    let mut pts = vec![Point {
        at: session.started_at,
        index: 0,
        output: OutputState::Normal,
    }];
    for e in &session.log {
        pts.push(Point {
            at: e.at,
            index: e.index,
            output: e.output,
        });
    }
    pts
}

/// Group key + chapter title for a cue. `None` means "don't start a chapter
/// here" (e.g. a blackout shouldn't split a song into two chapters).
fn chapter_for(cue: Option<&Cue>, output: OutputState) -> Option<(String, String)> {
    if output == OutputState::Blackout {
        return None;
    }
    if output == OutputState::Logo {
        return Some(("__logo".to_string(), "Logo".to_string()));
    }
    match cue {
        Some(Cue::ShowSlide { source, .. }) => {
            Some((source.service_item_id.clone(), source.display_label.clone()))
        }
        Some(Cue::Pause { cue_id, label }) => Some((cue_id.clone(), label.clone())),
        Some(Cue::ShowLogo { .. }) => Some(("__logo".to_string(), "Logo".to_string())),
        Some(Cue::BlackOut { .. }) | None => None,
    }
}

/// Emit a chapter marker each time the cue's group changes (song → next song →
/// sermon …). Blackout stretches don't create chapters.
pub fn chapter_markers(session: &LiveSession) -> Vec<ChapterMarker> {
    let mut markers = Vec::new();
    let mut last_key: Option<String> = None;
    for pt in timeline(session) {
        let Some((key, title)) = chapter_for(session.cue_list.get(pt.index), pt.output) else {
            continue;
        };
        if last_key.as_deref() != Some(key.as_str()) {
            markers.push(ChapterMarker {
                offset_ms: (pt.at - session.started_at).max(0),
                title,
            });
            last_key = Some(key);
        }
    }
    markers
}

fn srt_timestamp(ms: i64) -> String {
    let ms = ms.max(0);
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let milli = ms % 1000;
    format!("{h:02}:{m:02}:{s:02},{milli:03}")
}

/// Generate an SRT whose timeline matches the recording. Only `Normal` slide
/// cues become captions (blackout/logo/pause produce gaps). Adjacent segments
/// with identical text are coalesced into one caption. `ended_at` closes the
/// final caption.
pub fn session_to_srt(session: &LiveSession, ended_at: i64) -> String {
    let pts = timeline(session);

    // Raw [start, end, lines] segments for caption-bearing cues.
    let mut segs: Vec<(i64, i64, Vec<String>)> = Vec::new();
    for i in 0..pts.len() {
        let start = pts[i].at;
        let end = if i + 1 < pts.len() {
            pts[i + 1].at
        } else {
            ended_at
        };
        if end <= start || pts[i].output != OutputState::Normal {
            continue;
        }
        let lines = match session.cue_list.get(pts[i].index) {
            Some(Cue::ShowSlide { slide_content, .. }) if !slide_content.text_lines.is_empty() => {
                slide_content.text_lines.clone()
            }
            _ => continue,
        };
        // Coalesce with the previous segment if it's the same text and touching.
        if let Some(last) = segs.last_mut() {
            if last.2 == lines && last.1 == start {
                last.1 = end;
                continue;
            }
        }
        segs.push((start, end, lines));
    }

    let mut out = String::new();
    for (n, (start, end, lines)) in segs.into_iter().enumerate() {
        out.push_str(&format!("{}\n", n + 1));
        out.push_str(&format!(
            "{} --> {}\n",
            srt_timestamp(start - session.started_at),
            srt_timestamp(end - session.started_at),
        ));
        out.push_str(&lines.join("\n"));
        out.push_str("\n\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::cue_list::{Cue, CueList, CueSource, SlideContent};
    use crate::services::live_session::LiveAction;

    fn slide(item: &str, label: &str, line: &str) -> Cue {
        Cue::ShowSlide {
            cue_id: format!("{item}-{label}"),
            slide_content: Box::new(SlideContent {
                section_label: Some(label.to_string()),
                text_lines: vec![line.to_string()],
                translation_lines: None,
                reference: None,
                sensitive_slide: false,
                appearance: None,
            }),
            theme_id: None,
            template_id: None,
            source: CueSource {
                service_item_id: item.to_string(),
                item_cue_index: 0,
                display_label: format!("{item} — {label}"),
            },
        }
    }

    /// Two songs (item A, item B), each one slide.
    fn two_song_session() -> LiveSession {
        let cues = vec![
            slide("amazing-grace", "Verse 1", "Amazing grace"),
            slide("amazing-grace", "Chorus", "My chains are gone"),
            slide("how-great", "Verse 1", "O Lord my God"),
        ];
        LiveSession::new(
            "svc",
            CueList {
                service_id: "svc".into(),
                compiled_at: 0,
                cues,
            },
            1_000,
        )
    }

    #[test]
    fn chapters_start_on_each_new_item_not_each_slide() {
        let mut s = two_song_session();
        // started at 1000 on item A; advance within A at 5000; into B at 9000.
        s.dispatch(LiveAction::Next, 5_000); // still amazing-grace (chorus)
        s.dispatch(LiveAction::Next, 9_000); // how-great
        let markers = chapter_markers(&s);
        assert_eq!(markers.len(), 2, "one per song, not per slide");
        assert_eq!(markers[0].offset_ms, 0);
        assert_eq!(markers[0].title, "amazing-grace — Verse 1");
        assert_eq!(markers[1].offset_ms, 8_000); // 9000 - 1000
        assert_eq!(markers[1].title, "how-great — Verse 1");
    }

    #[test]
    fn blackout_does_not_split_a_chapter() {
        let mut s = two_song_session();
        s.dispatch(LiveAction::Blackout, 4_000); // black
        s.dispatch(LiveAction::Blackout, 6_000); // back to same slide
        let markers = chapter_markers(&s);
        // Still just the one chapter for item A (B never reached).
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].title, "amazing-grace — Verse 1");
    }

    #[test]
    fn srt_matches_timeline_and_closes_final_caption() {
        let mut s = two_song_session();
        s.dispatch(LiveAction::Next, 5_000);
        s.dispatch(LiveAction::Next, 9_000);
        let srt = session_to_srt(&s, 12_000);
        // Three captions: 0–4s, 4–8s, 8–11s (offsets from started_at=1000).
        assert!(srt.contains("00:00:00,000 --> 00:00:04,000"));
        assert!(srt.contains("Amazing grace"));
        assert!(srt.contains("00:00:04,000 --> 00:00:08,000"));
        assert!(srt.contains("My chains are gone"));
        assert!(srt.contains("00:00:08,000 --> 00:00:11,000"));
        assert!(srt.contains("O Lord my God"));
        assert!(srt.trim_start().starts_with("1\n"));
    }

    #[test]
    fn srt_omits_blackout_gaps() {
        let mut s = two_song_session();
        s.dispatch(LiveAction::Blackout, 3_000);
        let srt = session_to_srt(&s, 6_000);
        // First caption runs 0–2s (1000→3000), then a blackout gap → no caption.
        assert!(srt.contains("00:00:00,000 --> 00:00:02,000"));
        // The blackout segment (3000→6000) must not appear as a caption.
        assert_eq!(srt.matches("-->").count(), 1);
    }

    #[test]
    fn srt_timestamp_formats_hours() {
        assert_eq!(srt_timestamp(0), "00:00:00,000");
        assert_eq!(srt_timestamp(3_661_500), "01:01:01,500");
    }
}
