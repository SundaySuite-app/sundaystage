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
/// `Blackout`/`Logo` override it without losing the cue position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/OutputState.ts")]
pub enum OutputState {
    Normal,
    Blackout,
    Logo,
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
            }
            LiveAction::Previous => {
                self.index = self.index.saturating_sub(1);
                self.output = OutputState::Normal;
            }
            LiveAction::GoTo { index } => {
                if *index < len {
                    self.index = *index;
                }
                self.output = OutputState::Normal;
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
            LiveAction::Clear => {
                self.output = OutputState::Normal;
            }
        }
        self.log.push(SessionLogEntry {
            at: now,
            action: action.name().to_string(),
            index: self.index,
            output: self.output,
        });
    }

    /// What belongs on the output right now.
    pub fn current_frame(&self) -> LiveFrame {
        match self.output {
            OutputState::Blackout => return LiveFrame::Black,
            OutputState::Logo => return LiveFrame::Logo,
            OutputState::Normal => {}
        }
        match self.cue_list.get(self.index) {
            Some(Cue::ShowSlide { slide_content, .. }) => LiveFrame::Slide {
                slide_content: slide_content.clone(),
            },
            Some(Cue::BlackOut { .. }) => LiveFrame::Black,
            Some(Cue::ShowLogo { .. }) => LiveFrame::Logo,
            Some(Cue::Pause { label, .. }) => LiveFrame::Message {
                text: label.clone(),
            },
            None => LiveFrame::Black,
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
            slide_content: SlideContent {
                section_label: None,
                text_lines: vec![text.to_string()],
                translation_lines: None,
                reference: None,
            },
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
}
