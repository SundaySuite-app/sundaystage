//! Phase 5.1/5.3 — the live presentation runtime.
//!
//! A `LiveSession` is the running show: it owns the compiled [`CueList`], the
//! current cue index, the output state, and a **session log** of every
//! dispatched action. The plan's hard rule is "all state changes go through a
//! single dispatcher — never two paths to mutate"; that dispatcher is
//! [`LiveSession::dispatch`]. The session serializes to disk so that if the UI
//! crashes mid-service it can be reloaded and resume at the same cue (the
//! output process, Phase 5.2, independently keeps the last frame on screen).
//!
//! Everything here is pure and synchronous — no DB, no IO, no async — so cue
//! advance is instant and fully unit-testable. The < 50 ms keypress→output
//! promise lives downstream in the (Phase 5.2) output process; here the state
//! transition is O(1).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::services::cue_list::{Cue, CueList, SlideContent};

/// What the output is currently doing. `Normal` shows the cue at `index`;
/// `Blackout`/`Logo`/`Message` override it without losing the cue position
/// (the message text itself lives in [`LiveSession::message_text`] so this
/// stays `Copy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/OutputState.ts")]
pub enum OutputState {
    Normal,
    Blackout,
    Logo,
    Message,
}

/// An operator action. The only way to mutate a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/LiveAction.ts")]
pub enum LiveAction {
    Next,
    Previous,
    GoTo {
        index: usize,
    },
    /// Toggle blackout (Esc).
    Blackout,
    /// Toggle the church logo (L).
    ShowLogo,
    /// Show an operator message over the output ("Barnevakt til rom 2",
    /// "Gudstjenesten starter om 5 min") without losing the cue position.
    /// An empty text behaves like `Clear`.
    ShowMessage {
        text: String,
    },
    /// Return to showing the current cue normally.
    Clear,
}

impl LiveAction {
    fn name(&self) -> &'static str {
        match self {
            LiveAction::Next => "next",
            LiveAction::Previous => "previous",
            LiveAction::GoTo { .. } => "go_to",
            LiveAction::Blackout => "blackout",
            LiveAction::ShowLogo => "show_logo",
            LiveAction::ShowMessage { .. } => "show_message",
            LiveAction::Clear => "clear",
        }
    }
}

/// What should be on the main output right now. Derived from the cue + output
/// state; this is what the output process renders and the operator preview
/// mirrors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/LiveFrame.ts")]
pub enum LiveFrame {
    Slide {
        slide_content: SlideContent,
    },
    Black,
    Logo,
    /// A non-slide cue (e.g. a Pause) — show its label to the operator.
    Message {
        text: String,
    },
}

/// One entry in the replay-able session log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLogEntry {
    pub at: i64,
    pub action: String,
    pub index: usize,
    pub output: OutputState,
}

/// The running live session. Persisted to disk for crash recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSession {
    pub service_id: String,
    pub cue_list: CueList,
    pub index: usize,
    pub output: OutputState,
    /// The operator message shown while `output == Message`. `default` so
    /// crash-recovery WALs from older builds still deserialize.
    #[serde(default)]
    pub message_text: Option<String>,
    pub started_at: i64,
    pub log: Vec<SessionLogEntry>,
}

/// A lightweight snapshot sent to the operator UI after each action.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/LiveSessionView.ts")]
pub struct LiveSessionView {
    pub service_id: String,
    pub index: usize,
    pub total: usize,
    pub output: OutputState,
    pub frame: LiveFrame,
    pub log_len: usize,
    /// When the session went live (unix ms) — drives the stage-display timer.
    pub started_at: i64,
}

impl LiveSession {
    pub fn new(service_id: impl Into<String>, cue_list: CueList, now: i64) -> Self {
        Self {
            service_id: service_id.into(),
            cue_list,
            index: 0,
            output: OutputState::Normal,
            message_text: None,
            started_at: now,
            log: Vec::new(),
        }
    }

    /// The single mutator. Applies `action`, then appends a log entry capturing
    /// the resulting state.
    pub fn dispatch(&mut self, action: LiveAction, now: i64) {
        let len = self.cue_list.len();
        match &action {
            LiveAction::Next => {
                if len > 0 && self.index + 1 < len {
                    self.index += 1;
                }
                self.output = OutputState::Normal;
                self.message_text = None;
            }
            LiveAction::Previous => {
                self.index = self.index.saturating_sub(1);
                self.output = OutputState::Normal;
                self.message_text = None;
            }
            LiveAction::GoTo { index } => {
                if *index < len {
                    self.index = *index;
                }
                self.output = OutputState::Normal;
                self.message_text = None;
            }
            LiveAction::Blackout => {
                self.output = if self.output == OutputState::Blackout {
                    OutputState::Normal
                } else {
                    OutputState::Blackout
                };
            }
            LiveAction::ShowLogo => {
                self.output = if self.output == OutputState::Logo {
                    OutputState::Normal
                } else {
                    OutputState::Logo
                };
            }
            LiveAction::ShowMessage { text } => {
                // Dispatching again replaces the text; an empty text clears.
                if text.trim().is_empty() {
                    self.output = OutputState::Normal;
                    self.message_text = None;
                } else {
                    self.output = OutputState::Message;
                    self.message_text = Some(text.clone());
                }
            }
            LiveAction::Clear => {
                self.output = OutputState::Normal;
                self.message_text = None;
            }
        }
        self.log.push(SessionLogEntry {
            at: now,
            action: action.name().to_string(),
            index: self.index,
            output: self.output,
        });
    }

    /// Swap in a freshly compiled cue list mid-session — the operator added a
    /// verse, reordered items, edited a song. Without this the session plays
    /// the list snapshotted at `live_start` while the grid shows the recompile,
    /// and `go_to(index)` silently lands on the wrong cue.
    ///
    /// The cue on air stays on air: the index is remapped by `cue_id` (cue ids
    /// are deterministic across recompiles), falling back to the same
    /// (service_item_id, item_cue_index) source, falling back to clamping into
    /// range. The output override (blackout/logo) is preserved and the swap is
    /// logged.
    pub fn replace_cue_list(&mut self, cue_list: CueList, now: i64) {
        let remapped = self.cue_list.get(self.index).and_then(|current| {
            let by_id = cue_list
                .cues
                .iter()
                .position(|c| c.cue_id() == current.cue_id());
            by_id.or_else(|| {
                let source = current.source()?;
                cue_list.cues.iter().position(|c| {
                    c.source().is_some_and(|s| {
                        s.service_item_id == source.service_item_id
                            && s.item_cue_index == source.item_cue_index
                    })
                })
            })
        });
        self.index = remapped.unwrap_or_else(|| {
            self.index.min(cue_list.len().saturating_sub(1))
        });
        self.cue_list = cue_list;
        self.log.push(SessionLogEntry {
            at: now,
            action: "reload".to_string(),
            index: self.index,
            output: self.output,
        });
    }

    /// What belongs on the output right now. Infallible by design — the live
    /// output must ALWAYS have something to show, so anything we can't render
    /// (a missing or malformed cue) degrades to a safe `Black` rather than
    /// surfacing an error. The dispatcher and output path call this; for
    /// callers that want to *detect* a malformed cue (operator warnings, the
    /// companion broadcast) see [`try_current_frame`](Self::try_current_frame).
    pub fn current_frame(&self) -> LiveFrame {
        self.try_current_frame().unwrap_or(LiveFrame::Black)
    }

    /// Like [`current_frame`](Self::current_frame) but returns `Err` when the
    /// cue at the current index is malformed (e.g. a `ShowSlide` carrying no
    /// renderable text or reference — a corrupt/partially-compiled cue). The
    /// live path never propagates this (it falls back to `Black`); it lets the
    /// operator UI surface "this slide is empty" without a panic.
    pub fn try_current_frame(&self) -> Result<LiveFrame, String> {
        match self.output {
            OutputState::Blackout => return Ok(LiveFrame::Black),
            OutputState::Logo => return Ok(LiveFrame::Logo),
            OutputState::Message => {
                return Ok(LiveFrame::Message {
                    text: self.message_text.clone().unwrap_or_default(),
                })
            }
            OutputState::Normal => {}
        }
        match self.cue_list.get(self.index) {
            Some(Cue::ShowSlide { slide_content, .. }) => {
                // A slide with no text AND no reference is unrenderable — a
                // corrupt or partially-compiled cue. Report it instead of
                // pushing a blank slide to the projector.
                let has_text = slide_content
                    .text_lines
                    .iter()
                    .any(|l| !l.trim().is_empty());
                let has_ref = slide_content
                    .reference
                    .as_deref()
                    .is_some_and(|r| !r.trim().is_empty());
                if !has_text && !has_ref {
                    return Err(format!(
                        "malformed cue at index {}: slide has no renderable content",
                        self.index
                    ));
                }
                // `slide_content` is `&Box<SlideContent>`; LiveFrame holds it
                // unboxed, so deref-clone the inner value.
                Ok(LiveFrame::Slide {
                    slide_content: (**slide_content).clone(),
                })
            }
            Some(Cue::BlackOut { .. }) => Ok(LiveFrame::Black),
            Some(Cue::ShowLogo { .. }) => Ok(LiveFrame::Logo),
            Some(Cue::Pause { label, .. }) => Ok(LiveFrame::Message {
                text: label.clone(),
            }),
            None => Ok(LiveFrame::Black),
        }
    }

    pub fn view(&self) -> LiveSessionView {
        LiveSessionView {
            service_id: self.service_id.clone(),
            index: self.index,
            total: self.cue_list.len(),
            output: self.output,
            frame: self.current_frame(),
            log_len: self.log.len(),
            started_at: self.started_at,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::cue_list::{CueSource, SlideContent};

    fn slide_cue(id: &str, text: &str) -> Cue {
        Cue::ShowSlide {
            cue_id: id.to_string(),
            slide_content: Box::new(SlideContent {
                section_label: None,
                text_lines: vec![text.to_string()],
                translation_lines: None,
                reference: None,
                sensitive_slide: false,
                appearance: None,
            }),
            theme_id: None,
            template_id: None,
            source: CueSource {
                service_item_id: "item".into(),
                item_cue_index: 0,
                display_label: text.to_string(),
            },
        }
    }

    fn session(n: usize) -> LiveSession {
        let cues: Vec<Cue> = (0..n)
            .map(|i| slide_cue(&format!("c{i}"), &format!("line {i}")))
            .collect();
        let cl = CueList {
            service_id: "svc".into(),
            compiled_at: 0,
            cues,
        };
        LiveSession::new("svc", cl, 0)
    }

    #[test]
    fn starts_at_first_cue() {
        let s = session(3);
        assert_eq!(s.index, 0);
        assert_eq!(s.output, OutputState::Normal);
        match s.current_frame() {
            LiveFrame::Slide { slide_content } => {
                assert_eq!(slide_content.text_lines, vec!["line 0"])
            }
            _ => panic!("expected slide"),
        }
    }

    #[test]
    fn next_advances_and_clamps_at_end() {
        let mut s = session(2);
        s.dispatch(LiveAction::Next, 1);
        assert_eq!(s.index, 1);
        s.dispatch(LiveAction::Next, 2); // already last
        assert_eq!(s.index, 1);
    }

    #[test]
    fn previous_clamps_at_zero() {
        let mut s = session(2);
        s.dispatch(LiveAction::Previous, 1);
        assert_eq!(s.index, 0);
    }

    #[test]
    fn goto_clamps_to_range() {
        let mut s = session(3);
        s.dispatch(LiveAction::GoTo { index: 2 }, 1);
        assert_eq!(s.index, 2);
        s.dispatch(LiveAction::GoTo { index: 99 }, 2); // out of range → no move
        assert_eq!(s.index, 2);
    }

    #[test]
    fn blackout_toggles_and_advance_clears_it() {
        let mut s = session(3);
        s.dispatch(LiveAction::Blackout, 1);
        assert_eq!(s.output, OutputState::Blackout);
        assert_eq!(s.current_frame(), LiveFrame::Black);
        // Pressing next un-blacks and advances.
        s.dispatch(LiveAction::Next, 2);
        assert_eq!(s.output, OutputState::Normal);
        assert_eq!(s.index, 1);
    }

    #[test]
    fn blackout_toggles_back_to_normal() {
        let mut s = session(1);
        s.dispatch(LiveAction::Blackout, 1);
        s.dispatch(LiveAction::Blackout, 2);
        assert_eq!(s.output, OutputState::Normal);
    }

    #[test]
    fn logo_toggles() {
        let mut s = session(1);
        s.dispatch(LiveAction::ShowLogo, 1);
        assert_eq!(s.current_frame(), LiveFrame::Logo);
        s.dispatch(LiveAction::Clear, 2);
        assert_eq!(s.output, OutputState::Normal);
    }

    #[test]
    fn log_grows_with_each_action() {
        let mut s = session(3);
        s.dispatch(LiveAction::Next, 10);
        s.dispatch(LiveAction::Blackout, 20);
        assert_eq!(s.log.len(), 2);
        assert_eq!(s.log[0].action, "next");
        assert_eq!(s.log[0].index, 1);
        assert_eq!(s.log[1].action, "blackout");
        assert_eq!(s.log[1].output, OutputState::Blackout);
    }

    #[test]
    fn frame_derives_from_control_cues() {
        let cl = CueList {
            service_id: "s".into(),
            compiled_at: 0,
            cues: vec![
                Cue::BlackOut { cue_id: "b".into() },
                Cue::ShowLogo { cue_id: "l".into() },
                Cue::Pause {
                    cue_id: "p".into(),
                    label: "Offering".into(),
                },
            ],
        };
        let mut s = LiveSession::new("s", cl, 0);
        assert_eq!(s.current_frame(), LiveFrame::Black);
        s.dispatch(LiveAction::Next, 1);
        assert_eq!(s.current_frame(), LiveFrame::Logo);
        s.dispatch(LiveAction::Next, 2);
        assert_eq!(
            s.current_frame(),
            LiveFrame::Message {
                text: "Offering".into()
            }
        );
    }

    #[test]
    fn serde_round_trip_preserves_position_and_log() {
        let mut s = session(3);
        s.dispatch(LiveAction::Next, 1);
        s.dispatch(LiveAction::Blackout, 2);
        let json = s.to_json().unwrap();
        let back = LiveSession::from_json(&json).unwrap();
        assert_eq!(back.index, 1);
        assert_eq!(back.output, OutputState::Blackout);
        assert_eq!(back.log.len(), 2);
        assert_eq!(back.view().total, 3);
    }

    #[test]
    fn empty_cue_list_is_safe() {
        let mut s = session(0);
        s.dispatch(LiveAction::Next, 1);
        s.dispatch(LiveAction::Previous, 2);
        assert_eq!(s.index, 0);
        assert_eq!(s.current_frame(), LiveFrame::Black);
    }

    /// A `ShowSlide` cue with no text and no reference is corrupt — the
    /// fallible accessor must REPORT it (Err) rather than panic, and the
    /// infallible one must degrade to a safe `Black` so the projector never
    /// shows a blank slide or crashes the live output.
    #[test]
    fn malformed_slide_cue_errs_but_never_panics() {
        let cl = CueList {
            service_id: "s".into(),
            compiled_at: 0,
            cues: vec![Cue::ShowSlide {
                cue_id: "bad".into(),
                slide_content: Box::new(SlideContent {
                    section_label: None,
                    // Only blank lines + no reference → nothing to render.
                    text_lines: vec!["".into(), "   ".into()],
                    translation_lines: None,
                    reference: None,
                    sensitive_slide: false,
                    appearance: None,
                }),
                theme_id: None,
                template_id: None,
                source: CueSource {
                    service_item_id: "item".into(),
                    item_cue_index: 0,
                    display_label: "bad".into(),
                },
            }],
        };
        let s = LiveSession::new("s", cl, 0);
        // Fallible: detects the malformed cue.
        assert!(s.try_current_frame().is_err());
        // Infallible (the live path): degrades to Black, never panics.
        assert_eq!(s.current_frame(), LiveFrame::Black);
        // `view()` (used by every command) is also safe on a corrupt cue.
        assert_eq!(s.view().frame, LiveFrame::Black);
    }

    #[test]
    fn show_message_overrides_and_advance_clears_it() {
        let mut s = session(3);
        s.dispatch(
            LiveAction::ShowMessage {
                text: "Barnevakt til rom 2".into(),
            },
            1,
        );
        assert_eq!(s.output, OutputState::Message);
        assert_eq!(
            s.current_frame(),
            LiveFrame::Message {
                text: "Barnevakt til rom 2".into()
            }
        );
        assert_eq!(s.index, 0); // cue position untouched
        // Advancing returns to the cue and drops the message.
        s.dispatch(LiveAction::Next, 2);
        assert_eq!(s.output, OutputState::Normal);
        assert_eq!(s.message_text, None);
        assert!(matches!(s.current_frame(), LiveFrame::Slide { .. }));
    }

    #[test]
    fn show_message_replaces_text_and_empty_clears() {
        let mut s = session(1);
        s.dispatch(
            LiveAction::ShowMessage {
                text: "første".into(),
            },
            1,
        );
        s.dispatch(
            LiveAction::ShowMessage {
                text: "andre".into(),
            },
            2,
        );
        assert_eq!(
            s.current_frame(),
            LiveFrame::Message {
                text: "andre".into()
            }
        );
        // Empty text = clear (never a blank takeover of the projector).
        s.dispatch(LiveAction::ShowMessage { text: "  ".into() }, 3);
        assert_eq!(s.output, OutputState::Normal);
        assert_eq!(s.message_text, None);
    }

    #[test]
    fn blackout_over_message_then_clear_returns_to_cue() {
        let mut s = session(2);
        s.dispatch(
            LiveAction::ShowMessage {
                text: "info".into(),
            },
            1,
        );
        s.dispatch(LiveAction::Blackout, 2);
        assert_eq!(s.current_frame(), LiveFrame::Black);
        s.dispatch(LiveAction::Clear, 3);
        assert_eq!(s.output, OutputState::Normal);
        assert_eq!(s.message_text, None);
        assert!(matches!(s.current_frame(), LiveFrame::Slide { .. }));
        // The log names the action for the session export.
        assert_eq!(s.log[0].action, "show_message");
    }

    /// Pre-message WALs (no `message_text` field) must still deserialize —
    /// crash recovery cannot break on an app update.
    #[test]
    fn deserializes_sessions_persisted_before_message_support() {
        let mut s = session(2);
        s.dispatch(LiveAction::Next, 1);
        let mut json: serde_json::Value = serde_json::from_str(&s.to_json().unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("message_text");
        let back = LiveSession::from_json(&json.to_string()).unwrap();
        assert_eq!(back.index, 1);
        assert_eq!(back.message_text, None);
    }

    fn cue_list_of(cues: Vec<Cue>) -> CueList {
        CueList {
            service_id: "svc".into(),
            compiled_at: 1,
            cues,
        }
    }

    /// Inserting a cue BEFORE the live one must keep the live cue on air
    /// (index shifts by the insertion).
    #[test]
    fn reload_remaps_index_across_insertion_before_current() {
        let mut s = session(3);
        s.dispatch(LiveAction::GoTo { index: 1 }, 1); // live on c1
        let new = cue_list_of(vec![
            slide_cue("c0", "line 0"),
            slide_cue("new", "inserted"),
            slide_cue("c1", "line 1"),
            slide_cue("c2", "line 2"),
        ]);
        s.replace_cue_list(new, 2);
        assert_eq!(s.index, 2); // still c1
        match s.current_frame() {
            LiveFrame::Slide { slide_content } => {
                assert_eq!(slide_content.text_lines, vec!["line 1"])
            }
            other => panic!("expected slide, got {other:?}"),
        }
    }

    /// Appending after the live cue must not move it at all.
    #[test]
    fn reload_keeps_index_across_append() {
        let mut s = session(2);
        s.dispatch(LiveAction::Next, 1); // live on c1
        let new = cue_list_of(vec![
            slide_cue("c0", "line 0"),
            slide_cue("c1", "line 1"),
            slide_cue("c2", "added"),
        ]);
        s.replace_cue_list(new, 2);
        assert_eq!(s.index, 1);
    }

    /// When the live cue was REMOVED, fall back to clamping into range — never
    /// panic, never leave the index dangling past the end.
    #[test]
    fn reload_clamps_when_current_cue_removed() {
        let mut s = session(3);
        s.dispatch(LiveAction::GoTo { index: 2 }, 1); // live on c2
        // Neither the id nor the source of c2 survives → pure clamp path.
        let mut survivor = slide_cue("c0", "line 0");
        if let Cue::ShowSlide { source, .. } = &mut survivor {
            source.service_item_id = "other-item".into();
        }
        let new = cue_list_of(vec![survivor]);
        s.replace_cue_list(new, 2);
        assert_eq!(s.index, 0);
        // An empty recompile is also safe.
        s.replace_cue_list(cue_list_of(vec![]), 3);
        assert_eq!(s.index, 0);
        assert_eq!(s.current_frame(), LiveFrame::Black);
    }

    /// Blackout must survive a reload — the projector state is the operator's,
    /// not the recompiler's.
    #[test]
    fn reload_preserves_output_override_and_logs() {
        let mut s = session(2);
        s.dispatch(LiveAction::Blackout, 1);
        let log_before = s.log.len();
        s.replace_cue_list(cue_list_of(vec![slide_cue("c0", "line 0")]), 2);
        assert_eq!(s.output, OutputState::Blackout);
        assert_eq!(s.current_frame(), LiveFrame::Black);
        assert_eq!(s.log.len(), log_before + 1);
        assert_eq!(s.log.last().unwrap().action, "reload");
    }

    /// A cue whose id changed but whose source survived (e.g. the scripture
    /// ref-id changed after an edit) remaps via (service_item_id, cue index).
    #[test]
    fn reload_falls_back_to_source_match() {
        let mut s = session(2);
        s.dispatch(LiveAction::Next, 1); // live on c1 (source item/idx 0)
        let mut replacement = slide_cue("renamed", "line 1 edited");
        if let Cue::ShowSlide { source, .. } = &mut replacement {
            source.service_item_id = "item".into();
            source.item_cue_index = 0;
        }
        // c1 is gone; "renamed" carries the same source as every test cue.
        let new = cue_list_of(vec![replacement, slide_cue("other", "x")]);
        s.replace_cue_list(new, 2);
        assert_eq!(s.index, 0);
    }

    /// A slide with only a reference (e.g. a scripture cue whose body is empty)
    /// is still renderable — it must NOT be flagged as malformed.
    #[test]
    fn reference_only_slide_is_valid() {
        let cl = CueList {
            service_id: "s".into(),
            compiled_at: 0,
            cues: vec![Cue::ShowSlide {
                cue_id: "ref".into(),
                slide_content: Box::new(SlideContent {
                    section_label: None,
                    text_lines: vec![],
                    translation_lines: None,
                    reference: Some("John 3:16".into()),
                    sensitive_slide: false,
                    appearance: None,
                }),
                theme_id: None,
                template_id: None,
                source: CueSource {
                    service_item_id: "item".into(),
                    item_cue_index: 0,
                    display_label: "ref".into(),
                },
            }],
        };
        let s = LiveSession::new("s", cl, 0);
        assert!(s.try_current_frame().is_ok());
        assert!(matches!(s.current_frame(), LiveFrame::Slide { .. }));
    }
}
